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

mod banner;
mod content;
mod dispatch;
mod focus;
mod input;

pub use banner::*;
pub use focus::*;

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

#[cfg(test)]
mod tests;
