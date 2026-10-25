//! The settings window's content (spec: M4 §1, §3.7, §5): side navigation of the six sections,
//! one status banner at a time, the section title and its body (filled by the sections), and
//! keyboard focus over the section's controls.
//!
//! Edits go through [`SettingsView::edit`]: the [`Writer`] holds them and the 100 ms
//! [`WRITE_TIMER`] writes the last one. The directory watch posts [`DIR_CHANGED`]; 50 ms later the
//! file is re-read unless it is the window's own write, and `tray_seen.toml` for the Tray list. All
//! timers and messages arrive through [`AppContent::message`] on the window itself.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use newwin_config::tray_seen::TraySeen;
use newwin_config::{Config, ConfigError};
use newwin_core::strings::Strings;
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::launch::shell_execute;
use newwin_ui::app_window::{AppContent, CAPTION_H, CONTENT_MSG_MIN, CONTENT_TIMER_MIN};
use newwin_ui::nav::Nav;
use newwin_ui::panel::{PanelInput, PanelReply};
use newwin_ui::{Color, Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, theme};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_DOWN, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::{GetCaretBlinkTime, KillTimer, SetTimer, WA_INACTIVE, WM_ACTIVATE, WM_TIMER};
use windows::core::w;

use crate::body::Body;
use crate::font_list::FontList;
use crate::sections::Confirms;
use crate::writer::Writer;

mod dispatch;
mod input;

/// The window: fixed size, logical px (global constraints).
pub const SIZE: (f32, f32) = (760.0, 560.0);
pub const CLASS: &str = "NewWin.Settings";
pub const TITLE: &str = "NewWin";
/// Posted by the directory watch on the config folder.
pub const DIR_CHANGED: u32 = CONTENT_MSG_MIN;
/// One-shot, [`WRITE_DELAY_MS`] after the last edit: write the config.
const WRITE_TIMER: usize = CONTENT_TIMER_MIN;
/// One-shot, [`RELOAD_DELAY_MS`] after a directory event (one save gives several).
const RELOAD_TIMER: usize = CONTENT_TIMER_MIN + 1;
/// One-shot, after *Start NewWin*: look for the shell again.
const RECHECK_TIMER: usize = CONTENT_TIMER_MIN + 2;
/// One-shot, when an armed *Clear* / *Restore* runs out: its label goes back.
const CONFIRM_TIMER: usize = CONTENT_TIMER_MIN + 3;
const WRITE_DELAY_MS: u32 = 100;
const RELOAD_DELAY_MS: u32 = 50;
const RECHECK_MS: u32 = 3000;

const FONT: &str = "Segoe UI Variable Text";
// Layout, logical px.
const NAV_X: f32 = 12.0;
const NAV_W: f32 = 168.0;
const TOP: f32 = CAPTION_H + 4.0;
const BODY_X: f32 = 200.0;
const BODY_RIGHT_PAD: f32 = 28.0;
const NAV_TEXT: f32 = 13.0;
const TITLE_TEXT: f32 = 24.0;
const BANNER_TEXT: f32 = 12.0;
const BANNER_H: f32 = 40.0;
const BANNER_PAD_X: f32 = 14.0;
const BANNER_RADIUS: f32 = 10.0;
const BUTTON_H: f32 = 26.0;
const BUTTON_PAD_X: f32 = 12.0;
const GAP: f32 = 16.0;
const FOCUS_GAP: f32 = 3.0;
const BANNER_FILL: Color = Color::rgba(255, 255, 255, 0.08);
const BANNER_RIM: Color = Color::rgba(255, 255, 255, 0.14);
const BUTTON_FILL: Color = Color::rgba(255, 255, 255, 0.12);
const BUTTON_HOVER: Color = Color::rgba(255, 255, 255, 0.22);
const FOCUS_RING: Color = Color::rgba(255, 255, 255, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Bar,
    Appearance,
    Items,
    Spotlight,
    Tray,
    About,
}

impl Section {
    pub const ALL: [Section; 6] = [Section::Bar, Section::Appearance, Section::Items, Section::Spotlight, Section::Tray, Section::About];

    pub fn title(self, s: &Strings) -> &'static str {
        match self {
            Section::Bar => s.section_bar,
            Section::Appearance => s.section_appearance,
            Section::Items => s.section_items,
            Section::Spotlight => s.section_spotlight,
            Section::Tray => s.section_tray,
            Section::About => s.section_about,
        }
    }

    fn index(self) -> usize {
        Section::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
}

/// The status strip above the section title (spec §3.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Banner {
    ParseError { line: Option<usize> },
    /// The reason, without a final full stop.
    SaveFailed(String),
    NotRunning,
}

impl Banner {
    pub fn text(&self, s: &Strings) -> String {
        match self {
            Banner::ParseError { line: Some(n) } => s.banner_parse_error_line.replace("{line}", &n.to_string()),
            Banner::ParseError { line: None } => s.banner_parse_error.to_owned(),
            Banner::SaveFailed(reason) => s.banner_save_failed.replace("{reason}", reason),
            Banner::NotRunning => s.banner_not_running.to_owned(),
        }
    }

    pub fn button(&self, s: &Strings) -> Option<&'static str> {
        match self {
            Banner::ParseError { .. } => Some(s.open_file),
            Banner::SaveFailed(_) => None,
            Banner::NotRunning => Some(s.start_newwin),
        }
    }
}

/// The one banner to show: a failed save first (it is about what the user just did), then a
/// broken file, then a stopped shell.
pub fn pick_banner(save_error: Option<&str>, parse_error: Option<Option<usize>>, shell_running: bool) -> Option<Banner> {
    if let Some(reason) = save_error {
        Some(Banner::SaveFailed(reason.to_owned()))
    } else if let Some(line) = parse_error {
        Some(Banner::ParseError { line })
    } else if !shell_running {
        Some(Banner::NotRunning)
    } else {
        None
    }
}

/// An I/O error as the banner's reason: the system message without "(os error N)" and without
/// its final full stop (the banner adds one).
pub fn save_reason(e: &std::io::Error) -> String {
    let text = e.to_string();
    let text = match text.rfind(" (os error ") {
        Some(i) => text.get(..i).unwrap_or(&text).to_owned(),
        None => text,
    };
    let reason = text.trim().trim_end_matches('.').trim_end();
    if reason.is_empty() { "unknown error".to_owned() } else { reason.to_owned() }
}

/// The parse banner a load result needs (`Some(line)`), or `None` when the file loaded. A file
/// that is there but cannot be read (locked, denied) gets the same banner, without a line: the
/// window never shows silent defaults for an existing file.
pub fn parse_error_of(r: &Result<Config, ConfigError>) -> Option<Option<usize>> {
    match r {
        Ok(_) => None,
        Err(ConfigError::Parse { line, .. }) => Some(*line),
        Err(ConfigError::Io(_)) => Some(None),
    }
}

/// The config to show at start and the banner it needs: a broken or unreadable file shows the
/// defaults and the parse banner, and is left alone until an edit.
pub fn load_for_view(path: &Path) -> (Config, Option<Banner>) {
    let r = newwin_config::load(path);
    match parse_error_of(&r) {
        None => (r.unwrap_or_default(), None),
        Some(line) => {
            if let Err(e) = &r {
                log::warn!("config.toml cannot be used, showing defaults: {e}");
            }
            (Config::default(), Some(Banner::ParseError { line }))
        }
    }
}

/// Tab / Shift+Tab (`back`) over `count` controls, wrapping; `None` when there are none.
pub fn focus_next(current: Option<usize>, count: usize, back: bool) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match current.filter(|&i| i <= last) {
        None if back => last,
        None => 0,
        Some(i) if back => i.checked_sub(1).unwrap_or(last),
        Some(i) if i == last => 0,
        Some(i) => i + 1,
    })
}

/// The focus index after the banner's button (focusable 0) came (`has`) or went (`had`): the
/// same section control stays focused; the button itself loses the focus when it goes.
pub fn shift_focus(focus: Option<usize>, had: bool, has: bool) -> Option<usize> {
    match (focus, had, has) {
        (Some(0), true, false) => None,
        (Some(i), true, false) => Some(i - 1),
        (Some(i), false, true) => Some(i + 1),
        (f, _, _) => f,
    }
}

/// What a focusable control is, for the keys it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    Button,
    Toggle,
    Segmented,
    Slider,
    /// The font row: opens its list.
    FontList,
    /// Takes the typing itself (the date template).
    TextField,
    /// A row of the Items / Tray list: Space switches it, Ctrl+↑/↓ move it.
    ListRow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Press a button, flip a toggle, open the font list.
    Activate,
    /// A slider by one step, a segment by one (-1 left, +1 right), a list row by one (-1 up).
    Step(i32),
}

/// The key `vk` on a focused control of `kind` (spec §5).
pub fn key_action(kind: FocusKind, vk: u16) -> Option<KeyAction> {
    match (kind, vk) {
        (FocusKind::Slider | FocusKind::Segmented, v) if v == VK_LEFT.0 => Some(KeyAction::Step(-1)),
        (FocusKind::Slider | FocusKind::Segmented, v) if v == VK_RIGHT.0 => Some(KeyAction::Step(1)),
        (FocusKind::Toggle | FocusKind::ListRow, v) if v == VK_SPACE.0 => Some(KeyAction::Activate),
        (FocusKind::Button | FocusKind::FontList, v) if v == VK_SPACE.0 || v == VK_RETURN.0 => Some(KeyAction::Activate),
        _ => None,
    }
}

/// The key `vk` pressed with Ctrl on a focused control of `kind`: Ctrl+↑/↓ move a list row.
pub fn ctrl_key_action(kind: FocusKind, vk: u16) -> Option<KeyAction> {
    match (kind, vk) {
        (FocusKind::ListRow, v) if v == VK_UP.0 => Some(KeyAction::Step(-1)),
        (FocusKind::ListRow, v) if v == VK_DOWN.0 => Some(KeyAction::Step(1)),
        _ => None,
    }
}

/// True while the shell holds its single-instance mutex.
fn shell_running() -> bool {
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, w!("Local\\NewWin.Shell")) } {
        Ok(h) => {
            unsafe {
                let _ = CloseHandle(h);
            }
            true
        }
        Err(_) => false,
    }
}

/// `NewWin.exe` next to this exe.
fn newwin_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|p| p.with_file_name("NewWin.exe")).filter(|p| p.is_file())
}

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(i32::from(vk)) < 0 }
}

/// Laid-out texts; rebuilt when the DPI, the section or the banner changes.
struct Texts {
    nav: Vec<TextLayout>,
    title: Option<TextLayout>,
    banner: Option<TextLayout>,
    button: Option<TextLayout>,
}

/// Where things were painted last, physical px; mouse hits use it.
#[derive(Default)]
struct Geometry {
    nav: Vec<RectF>,
    banner: Option<RectF>,
    button: Option<RectF>,
    title: PointF,
    /// The section body's area.
    body: RectF,
}

pub struct SettingsView {
    /// The window, for timers; set right after it is created.
    pub hwnd: HWND,
    strings: &'static Strings,
    text: TextCache,
    texts: Option<Texts>,
    geo: Geometry,
    dpi: u32,
    path: PathBuf,
    pub config: Config,
    writer: Rc<RefCell<Writer>>,
    pub section: Section,
    nav: Nav,
    pub banner: Option<Banner>,
    /// `Some(line)` while the file on disk does not parse.
    parse_error: Option<Option<usize>>,
    save_error: Option<String>,
    shell_running: bool,
    /// Index into [`SettingsView::focusables`].
    focus: Option<usize>,
    button_hover: bool,
    button_pressed: bool,
    /// The section's rows and controls.
    body: Body,
    /// Open over the window while picking a font.
    font_list: Option<FontList>,
    /// The installed families, read the first time the list opens.
    families: Option<Rc<Vec<String>>>,
    /// *Custom* picked, no template typed yet.
    date_custom: bool,
    confirms: Confirms,
    /// The apps that have had a tray icon (the shell writes the file), for the Tray list.
    seen_path: PathBuf,
    seen: TraySeen,
    /// The caret blink period.
    blink: Option<u32>,
}

impl SettingsView {
    /// `path`: `config.toml`; `seen_path`: `tray_seen.toml`, in the same watched folder.
    pub fn new(gfx: &Rc<Gfx>, strings: &'static Strings, path: PathBuf, seen_path: PathBuf, writer: Rc<RefCell<Writer>>) -> SettingsView {
        let (config, banner) = load_for_view(&path);
        let parse_error = match banner {
            Some(Banner::ParseError { line }) => Some(line),
            _ => None,
        };
        let nav = Nav::new(Section::ALL.iter().map(|s| s.title(strings).to_owned()).collect(), 0);
        let mut v = SettingsView {
            hwnd: HWND::default(),
            strings,
            text: TextCache::new(gfx),
            texts: None,
            geo: Geometry::default(),
            dpi: 96,
            path,
            config,
            writer,
            section: Section::Bar,
            nav,
            banner: None,
            parse_error,
            save_error: None,
            shell_running: shell_running(),
            focus: None,
            button_hover: false,
            button_pressed: false,
            body: Body::new(),
            font_list: None,
            families: None,
            date_custom: false,
            confirms: Confirms::default(),
            seen: dispatch::load_seen(&seen_path).unwrap_or_default(),
            seen_path,
            blink: {
                let b = unsafe { GetCaretBlinkTime() };
                (b != 0 && b != u32::MAX).then_some(b)
            },
        };
        v.refresh_banner();
        v.sync_body(0);
        v
    }

    /// Changes the config from a control: shown at once, written 100 ms after the last change.
    pub fn edit(&mut self, f: impl FnOnce(&mut Config)) {
        let mut c = self.config.clone();
        f(&mut c);
        let c = c.sanitized();
        if c == self.config {
            return;
        }
        self.config = c.clone();
        if self.writer.borrow_mut().edit(c) {
            self.set_timer(WRITE_TIMER, WRITE_DELAY_MS);
        }
    }

    fn set_timer(&self, id: usize, ms: u32) {
        if !self.hwnd.is_invalid() {
            unsafe { SetTimer(Some(self.hwnd), id, ms, None) };
        }
    }

    fn kill_timer(&self, id: usize) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id);
        }
    }

    fn refresh_banner(&mut self) {
        let banner = pick_banner(self.save_error.as_deref(), self.parse_error, self.shell_running);
        if banner != self.banner {
            let had = self.has_button();
            self.banner = banner;
            self.texts = None;
            self.button_hover = false;
            self.button_pressed = false;
            // The button is focusable 0: keep the focus on the same section control.
            self.focus = shift_focus(self.focus, had, self.has_button());
        }
    }

    fn has_button(&self) -> bool {
        self.banner.as_ref().and_then(|b| b.button(self.strings)).is_some()
    }

    fn flush(&mut self) {
        let had = self.writer.borrow().pending.is_some();
        let r = self.writer.borrow_mut().flush(&self.path);
        match r {
            Ok(()) if had => {
                self.save_error = None;
                // What is on disk now is our valid text.
                self.parse_error = None;
            }
            Ok(()) => {}
            Err(e) => {
                log::error!("cannot save {}: {e}", self.path.display());
                self.save_error = Some(save_reason(&e));
            }
        }
        self.refresh_banner();
    }

    /// The file changed on disk: show it, unless it is our own write or an edit is about to be
    /// written over it.
    fn reload(&mut self) {
        self.reload_seen();
        if self.writer.borrow().pending.is_some() {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(&self.path)
            && self.writer.borrow().is_own(&text)
        {
            return;
        }
        let r = newwin_config::load(&self.path);
        self.parse_error = parse_error_of(&r);
        match r {
            Ok(c) => {
                if c != self.config {
                    log::info!("config.toml changed outside, reloading");
                    self.config = c;
                    self.controls_changed();
                }
            }
            // The shown config stays; the banner says the file cannot be used.
            Err(e) => log::warn!("config.toml cannot be used: {e}"),
        }
        self.refresh_banner();
    }

    /// The config was replaced from outside: the section's controls take the new values.
    fn controls_changed(&mut self) {
        self.sync_body(now_ms());
    }

    fn check_shell(&mut self) {
        self.shell_running = shell_running();
        self.refresh_banner();
    }

    fn start_newwin(&mut self) {
        match newwin_exe() {
            Some(exe) => {
                if let Err(e) = shell_execute("open", &exe.to_string_lossy(), "") {
                    log::error!("cannot start {}: {e}", exe.display());
                }
                self.set_timer(RECHECK_TIMER, RECHECK_MS);
            }
            None => log::warn!("NewWin.exe is not next to NewWinSettings.exe"),
        }
    }

    fn press_banner_button(&mut self) {
        match self.banner {
            Some(Banner::ParseError { .. }) => {
                if let Err(e) = shell_execute("open", &self.path.to_string_lossy(), "") {
                    log::error!("cannot open {}: {e}", self.path.display());
                }
            }
            Some(Banner::NotRunning) => self.start_newwin(),
            _ => {}
        }
    }

    fn select_section(&mut self, i: usize, now: u64) {
        let Some(&s) = Section::ALL.get(i) else { return };
        if s == self.section {
            return;
        }
        self.section = s;
        self.nav.select(i, now);
        self.focus = None;
        self.texts = None;
        self.font_list = None;
        self.date_custom = false;
        self.body.reset();
        self.sync_body(now);
    }

    fn style(&self, size: f32, weight: u16) -> TextStyle {
        TextStyle { family: FONT.into(), size: scale(size, self.dpi), weight }
    }

    fn build_texts(&mut self) -> Texts {
        let nav_style = self.style(NAV_TEXT, theme::WEIGHT_REGULAR);
        let nav_max = scale(NAV_W - 2.0 * theme::NAV_PAD_X, self.dpi);
        let nav = Section::ALL.iter().filter_map(|s| self.text.layout(s.title(self.strings), &nav_style, nav_max).ok()).collect();
        let body_w = scale(SIZE.0 - BODY_X - BODY_RIGHT_PAD, self.dpi);
        let title_style = self.style(TITLE_TEXT, theme::WEIGHT_SEMIBOLD);
        let title = self.text.layout(self.section.title(self.strings), &title_style, body_w).ok();
        let (banner, button) = match self.banner.clone() {
            Some(b) => {
                let button_style = self.style(BANNER_TEXT, theme::WEIGHT_SEMIBOLD);
                let button = b.button(self.strings).and_then(|t| self.text.layout(t, &button_style, body_w / 2.0).ok());
                let reserved = button.as_ref().map_or(0.0, |t| t.width() + scale(2.0 * BUTTON_PAD_X + BANNER_PAD_X, self.dpi));
                let max = body_w - scale(2.0 * BANNER_PAD_X, self.dpi) - reserved;
                let text_style = self.style(BANNER_TEXT, theme::WEIGHT_REGULAR);
                (self.text.layout(&b.text(self.strings), &text_style, max).ok(), button)
            }
            None => (None, None),
        };
        Texts { nav, title, banner, button }
    }

    fn layout(&self, texts: &Texts) -> Geometry {
        let d = self.dpi;
        let s = |v: f32| scale(v, d);
        let nav = self.nav.layout(PointF { x: s(NAV_X), y: s(TOP) }, s(NAV_W), d);
        let (x, w) = (s(BODY_X), s(SIZE.0 - BODY_X - BODY_RIGHT_PAD));
        let mut y = s(TOP);
        let (mut banner, mut button) = (None, None);
        if self.banner.is_some() {
            let r = RectF::new(x, y, w, s(BANNER_H));
            if let Some(t) = texts.button.as_ref() {
                let bw = t.width() + s(2.0 * BUTTON_PAD_X);
                let bh = s(BUTTON_H);
                button = Some(RectF::new(r.right() - s((BANNER_H - BUTTON_H) / 2.0) - bw, r.y + (r.h - bh) / 2.0, bw, bh));
            }
            banner = Some(r);
            y = r.bottom() + s(GAP);
        }
        let title_h = texts.title.as_ref().map_or(s(TITLE_TEXT), |t| t.height());
        let title = PointF { x, y };
        let body_y = y + title_h + s(GAP);
        Geometry { nav, banner, button, title, body: RectF::new(x, body_y, w, (s(SIZE.1) - body_y - s(GAP)).max(0.0)) }
    }
}

impl AppContent for SettingsView {
    fn paint(&mut self, p: &mut Painter, now_ms: u64) {
        let texts = match self.texts.take() {
            Some(t) => t,
            None => self.build_texts(),
        };
        self.geo = self.layout(&texts);
        let (d, g) = (self.dpi, &self.geo);
        let s = |v: f32| scale(v, d);
        self.nav.paint(p, &g.nav, &texts.nav, now_ms, d);
        if let Some(r) = g.banner {
            p.fill_round_rect(r, s(BANNER_RADIUS), BANNER_FILL);
            p.stroke_round_rect(r, s(BANNER_RADIUS), BANNER_RIM, s(1.0).round().max(1.0));
            if let Some(t) = texts.banner.as_ref() {
                p.text(t, PointF { x: (r.x + s(BANNER_PAD_X)).round(), y: (r.y + (r.h - t.height()) / 2.0).round() }, theme::TEXT);
            }
            if let (Some(b), Some(t)) = (g.button, texts.button.as_ref()) {
                let fill = if self.button_pressed { theme::ACTIVE_PILL } else if self.button_hover { BUTTON_HOVER } else { BUTTON_FILL };
                p.fill_round_rect(b, b.h / 2.0, fill);
                p.text(t, PointF { x: (b.x + (b.w - t.width()) / 2.0).round(), y: (b.y + (b.h - t.height()) / 2.0).round() }, theme::TEXT_STRONG);
            }
        }
        if let Some(t) = texts.title.as_ref() {
            p.text(t, PointF { x: g.title.x.round(), y: g.title.y.round() }, theme::TEXT_STRONG);
        }
        let body_area = g.body;
        self.body.paint(p, &mut self.text, self.strings, body_area, now_ms, self.blink);
        // The field shows its own focus; a ring outside the body area would sit on the title.
        if let Some(i) = self.focus
            && let Some(&(kind, r)) = self.focusables().get(i)
            && kind != FocusKind::TextField
            && ((i == 0 && self.has_button()) || (r.y >= body_area.y && r.bottom() <= body_area.bottom()))
        {
            let ring = r.inset(-s(FOCUS_GAP));
            p.stroke_round_rect(ring, ring.h.min(ring.w) / 2.0, FOCUS_RING, s(1.0).round().max(1.0));
        }
        if let Some(rect) = self.list_rect()
            && let Some(list) = self.font_list.as_mut()
        {
            list.paint(p, &mut self.text, rect);
        }
        self.texts = Some(texts);
    }

    fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply {
        let repaint = self.route(ev, now_ms);
        PanelReply { repaint, ..PanelReply::default() }
    }

    fn animating(&self, now_ms: u64) -> bool {
        self.nav.animating(now_ms) || self.body.animating(now_ms)
    }

    fn wake_at(&self, now_ms: u64) -> Option<u64> {
        self.body.field.next_blink(now_ms, self.blink)
    }

    fn dpi_changed(&mut self, dpi: u32) {
        self.dpi = dpi;
        self.texts = None;
        self.body.set_dpi(dpi);
        // The list is laid out for the old DPI.
        self.font_list = None;
    }

    fn message(&mut self, msg: u32, wp: WPARAM, _lp: LPARAM, now_ms: u64) -> PanelReply {
        match (msg, wp.0) {
            (DIR_CHANGED, _) => {
                self.set_timer(RELOAD_TIMER, RELOAD_DELAY_MS);
                return PanelReply::default();
            }
            (WM_TIMER, WRITE_TIMER) => {
                self.kill_timer(WRITE_TIMER);
                self.flush();
            }
            (WM_TIMER, RELOAD_TIMER) => {
                self.kill_timer(RELOAD_TIMER);
                self.reload();
            }
            (WM_TIMER, RECHECK_TIMER) => {
                self.kill_timer(RECHECK_TIMER);
                self.check_shell();
            }
            (WM_TIMER, CONFIRM_TIMER) => {
                self.kill_timer(CONFIRM_TIMER);
                self.refresh_armed(now_ms);
            }
            (WM_ACTIVATE, w) if (w & 0xFFFF) as u32 != WA_INACTIVE => self.check_shell(),
            _ => return PanelReply::default(),
        }
        PanelReply { repaint: true, ..PanelReply::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newwin_core::strings::strings_for;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};

    #[test]
    fn keyboard_focus_moves_and_wraps() {
        assert_eq!(focus_next(None, 3, false), Some(0));
        assert_eq!(focus_next(Some(2), 3, false), Some(0));
        assert_eq!(focus_next(Some(0), 3, true), Some(2));
        assert_eq!(focus_next(None, 0, false), None);
        assert_eq!(focus_next(None, 3, true), Some(2));
        assert_eq!(focus_next(Some(1), 3, false), Some(2));
        assert_eq!(focus_next(Some(7), 3, false), Some(0), "a stale index starts over");
    }

    #[test]
    fn a_banner_button_coming_or_going_keeps_the_focused_control() {
        assert_eq!(shift_focus(Some(2), false, true), Some(3));
        assert_eq!(shift_focus(Some(3), true, false), Some(2));
        assert_eq!(shift_focus(Some(0), true, false), None, "the button itself went");
        assert_eq!(shift_focus(Some(1), true, true), Some(1));
        assert_eq!(shift_focus(None, false, true), None);
    }

    #[test]
    fn keys_act_on_the_focused_control() {
        assert_eq!(key_action(FocusKind::Slider, VK_LEFT.0), Some(KeyAction::Step(-1)));
        assert_eq!(key_action(FocusKind::Segmented, VK_RIGHT.0), Some(KeyAction::Step(1)));
        assert_eq!(key_action(FocusKind::Toggle, VK_SPACE.0), Some(KeyAction::Activate));
        assert_eq!(key_action(FocusKind::Button, VK_RETURN.0), Some(KeyAction::Activate));
        assert_eq!(key_action(FocusKind::Button, VK_SPACE.0), Some(KeyAction::Activate));
        assert_eq!(key_action(FocusKind::FontList, VK_SPACE.0), Some(KeyAction::Activate));
        assert_eq!(key_action(FocusKind::Toggle, VK_LEFT.0), None);
        assert_eq!(key_action(FocusKind::Slider, VK_UP.0), None);
    }

    #[test]
    fn list_rows_switch_with_space_and_move_with_ctrl_arrows() {
        use windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN;
        assert_eq!(key_action(FocusKind::ListRow, VK_SPACE.0), Some(KeyAction::Activate));
        assert_eq!(key_action(FocusKind::ListRow, VK_UP.0), None, "plain arrows do not move rows");
        assert_eq!(ctrl_key_action(FocusKind::ListRow, VK_UP.0), Some(KeyAction::Step(-1)));
        assert_eq!(ctrl_key_action(FocusKind::ListRow, VK_DOWN.0), Some(KeyAction::Step(1)));
        assert_eq!(ctrl_key_action(FocusKind::Slider, VK_UP.0), None);
    }

    #[test]
    fn six_sections_in_spec_order() {
        let en = strings_for("en-US");
        let titles: Vec<_> = Section::ALL.iter().map(|s| s.title(en)).collect();
        assert_eq!(titles, ["Bar", "Appearance", "Items", "Spotlight", "Tray", "About"]);
    }

    #[test]
    fn banner_copy_is_the_spec_text() {
        let en = strings_for("en-US");
        let parse = Banner::ParseError { line: Some(12) };
        assert_eq!(parse.text(en), "config.toml has an error on line 12. The bar uses the last good settings.");
        assert_eq!(parse.button(en), Some("Open file"));
        let saved = Banner::SaveFailed("Access is denied".into());
        assert_eq!(saved.text(en), "Couldn't save settings: Access is denied.");
        assert_eq!(saved.button(en), None);
        assert_eq!(Banner::NotRunning.text(en), "NewWin is not running. Changes apply when it starts.");
        assert_eq!(Banner::NotRunning.button(en), Some("Start NewWin"));
        assert!(!Banner::ParseError { line: None }.text(en).contains("{line}"));
    }

    #[test]
    fn one_banner_at_a_time() {
        assert_eq!(pick_banner(None, None, true), None);
        assert_eq!(pick_banner(None, None, false), Some(Banner::NotRunning));
        assert_eq!(pick_banner(None, Some(Some(3)), false), Some(Banner::ParseError { line: Some(3) }));
        assert_eq!(pick_banner(Some("disk full"), Some(Some(3)), false), Some(Banner::SaveFailed("disk full".into())));
    }

    #[test]
    fn save_reason_drops_the_os_code_and_full_stop() {
        let e = std::io::Error::from_raw_os_error(5);
        let r = save_reason(&e);
        assert!(!r.contains("os error"), "{r}");
        assert!(!r.ends_with('.'), "{r}");
        assert!(!r.is_empty());
    }
}
