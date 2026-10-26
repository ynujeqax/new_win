//! Running apps on the bar: model updates, layout, painting, hit testing, clicks.

use newwin_core::apps::{ClickAction, WindowInfo, is_screen_snip, left_click, middle_click};
use newwin_core::items::RightItem;
use newwin_core::app_icon::{ICON_GAP_LOGICAL_PX, ICON_LOGICAL_PX, fallback_for_group, source_for_group};
use newwin_core::layout::{fit_labels, pill_widths};
use newwin_platform::clock::now_ms;
use newwin_platform::activate;
use newwin_platform::dpi::scale;
use newwin_platform::windows_watch::{WindowEvent, describe};
use newwin_platform::worker::ResolvedName;
use newwin_ui::icon_bitmaps::icon_rect;
use newwin_ui::{Painter, PointF, RectF, TextLayout, theme};

use super::App;
use crate::bar::{paint_background, text_style};

/// The active app's pill glides a little slower than the hover one: it travels farther.
const ACTIVE_GLIDE_MS: u32 = 180;
const PILL_FADE_MS: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Group(usize),
    Overflow,
    /// The tray button `⌃`.
    Tray,
    Right(RightItem),
}

pub(super) struct BarItem {
    pub target: Target,
    pub rect: RectF,
    /// None when the pill is too narrow for any of the name (only the icon shows).
    pub text: Option<TextLayout>,
    /// Where an app pill takes its icon from, and where else when that has none.
    pub icon: Option<(String, Option<String>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Button {
    Left,
    Middle,
    Right,
}

impl App {
    fn refresh_window(&mut self, h: u64) -> bool {
        let Some(names) = self.names.as_mut() else { return false };
        match describe(h, names) {
            Some(w) => {
                self.track_snip(h, is_screen_snip(&w));
                self.model.upsert(w)
            }
            None => {
                self.track_snip(h, false);
                self.model.remove(h)
            }
        }
    }

    pub(super) fn load_windows(&mut self, windows: Vec<u64>, foreground: u64) {
        for h in windows {
            self.refresh_window(h);
        }
        self.model.set_active(Some(foreground));
    }

    pub(super) fn on_window_event(&mut self, ev: WindowEvent) {
        log::debug!("window event {ev:?}");
        match ev {
            WindowEvent::Changed(h) => {
                self.refresh_window(h);
            }
            WindowEvent::Destroyed(h) => {
                self.track_snip(h, false);
                self.model.remove(h);
            }
            WindowEvent::Activated(h) => {
                if let Some(h) = h {
                    self.refresh_window(h);
                }
                self.model.set_active(h);
                // Each window has its own layout.
                self.read_layout();
            }
        }
        self.refresh_if_changed();
    }

    pub(super) fn on_name_resolved(&mut self, r: &ResolvedName) {
        if let Some(names) = self.names.as_mut() {
            names.store(r);
        }
        self.model.set_group_name(&r.key, r.name.clone());
        self.refresh_if_changed();
    }

    /// What the bar shows; window titles and other details do not cause a repaint.
    fn signature(&self) -> Vec<(String, bool)> {
        let active = self.model.active_group();
        self.model.groups().iter().enumerate().map(|(i, g)| (g.name.clone(), Some(i) == active)).collect()
    }

    fn refresh_if_changed(&mut self) {
        let sig = self.signature();
        if sig != self.last_signature {
            log::debug!("bar items {sig:?}");
            self.last_signature = sig;
            self.refresh();
        }
    }

    /// Recomputes item positions and redraws the whole bar.
    pub(super) fn refresh(&mut self) {
        self.relayout();
        self.paint();
    }

    fn relayout(&mut self) {
        let Some(layer) = self.bar_layer.as_ref() else { return };
        let (w, h) = layer.size();
        let (w, h) = (w as f32, h as f32);
        let dpi = self.dpi;
        let a = &self.config.appearance;
        let s = |v: f32| scale(v, dpi).round();
        let regular = text_style(a, dpi, theme::WEIGHT_REGULAR);
        let strong = text_style(a, dpi, theme::WEIGHT_SEMIBOLD);
        let pill_pad = s(theme::PILL_PAD_X);
        let pill_h = s(theme::PILL_HEIGHT);
        let (start, end) = self.relayout_right(w, h);

        // Left: one pill per app group, shrinking or overflowing into » when crowded.
        let active = self.model.active_group();
        let names: Vec<(String, bool, (String, Option<String>))> = self
            .model
            .groups()
            .iter()
            .enumerate()
            .map(|(i, g)| (g.name.clone(), Some(i) == active, (source_for_group(&g.key), fallback_for_group(&g.key, g.exe.as_deref()))))
            .collect();
        // Room for the icon is kept from the start, so the name does not jump when it arrives.
        let (icon, icon_gap) = (s(ICON_LOGICAL_PX), s(ICON_GAP_LOGICAL_PX));
        let natural: Vec<f32> = names
            .iter()
            .map(|(n, act, _)| {
                let text_w = self.text.natural_width(n, if *act { &strong } else { &regular }).unwrap_or(0.0);
                pill_widths(text_w, pill_pad, icon, icon_gap).0
            })
            .collect();
        let min_w = pill_widths(0.0, pill_pad, icon, icon_gap).1;
        let available = (end - start).max(0.0);
        let fit = fit_labels(&natural, available, min_w, s(theme::ITEM_GAP), s(24.0));
        let mut items = Vec::new();
        let mut x = start;
        let y = ((h - pill_h) / 2.0).round();
        for (i, width) in fit.widths.iter().enumerate() {
            let (name, act, source) = &names[i];
            let style = if *act { &strong } else { &regular };
            let room = width - 2.0 * pill_pad - icon - icon_gap;
            let text = (room >= s(8.0)).then(|| self.text.layout(name, style, room).ok()).flatten();
            items.push(BarItem { target: Target::Group(i), rect: RectF::new(x, y, *width, pill_h), text, icon: Some(source.clone()) });
            x += width + s(theme::ITEM_GAP);
        }
        if fit.overflow_from.is_some()
            && let Ok(text) = self.text.layout("»", &regular, s(24.0))
        {
            items.push(BarItem { target: Target::Overflow, rect: RectF::new(x, y, s(24.0), pill_h), text: Some(text), icon: None });
        }
        let _ = h;
        self.items = items;
        self.overflow_from = fit.overflow_from;
    }

    pub(super) fn paint(&mut self) {
        // Icons first: asking for one needs the whole App, drawing borrows its layer.
        let sources: Vec<Option<(String, Option<String>)>> = self.items.iter().map(|it| it.icon.clone()).collect();
        let icons: Vec<Option<super::icons_view::Shown>> =
            sources.iter().map(|s| s.as_ref().and_then(|(src, fb)| self.shell_icon(src, fb.as_deref()))).collect();
        // The highlights follow the active app and the pointer, gliding when either moves.
        let now = now_ms();
        let active = self.model.active_group();
        let rect_of = |want: &dyn Fn(Target) -> bool| self.items.iter().find(|it| want(it.target)).map(|it| it.rect);
        let active_rect = rect_of(&|t| matches!(t, Target::Group(g) if Some(g) == active));
        let hover_rect = rect_of(&|t| self.hover == Some(t) && matches!(t, Target::Group(_) | Target::Overflow)).filter(|r| Some(*r) != active_rect);
        match active_rect {
            Some(r) => self.active_pill.show(r.x, r.w, now, ACTIVE_GLIDE_MS, PILL_FADE_MS),
            None => self.active_pill.hide(now, PILL_FADE_MS),
        }
        match hover_rect {
            Some(r) => self.hover_pill.show(r.x, r.w, now, theme::HOVER_GLIDE_MS, PILL_FADE_MS),
            None => self.hover_pill.hide(now, PILL_FADE_MS),
        }
        let Some(layer) = self.bar_layer.as_mut() else { return };
        let (w, h) = layer.size();
        let (w, h) = (w as f32, h as f32);
        let dpi = self.dpi;
        let a = &self.config.appearance;
        let (pad, icon_w, icon_gap) = (scale(theme::PILL_PAD_X, dpi).round(), scale(ICON_LOGICAL_PX, dpi).round(), scale(ICON_GAP_LOGICAL_PX, dpi).round());
        let bitmaps = &mut self.icon_bitmaps;
        let radius = scale(theme::PILL_RADIUS, dpi).round();
        let (items, hover, glass, right) = (&self.items, self.hover, self.bar_glass, &self.right);
        let pills = [(self.hover_pill, theme::HOVER_PILL), (self.active_pill, theme::ACTIVE_PILL)];
        let pill_h = scale(theme::PILL_HEIGHT, dpi).round();
        let r = layer.draw(None, |p: &mut Painter| {
            paint_background(p, w, h, dpi, a, glass);
            for (pill, color) in pills {
                if let Some((x, pw, o)) = pill.at(now) {
                    p.fill_round_rect(RectF::new(x, ((h - pill_h) / 2.0).round(), pw, pill_h), radius, color.with_alpha(color.a * o));
                }
            }
            for (it, icon) in items.iter().zip(&icons) {
                let is_active = matches!(it.target, Target::Group(g) if Some(g) == active);
                let is_hover = hover == Some(it.target);
                let color = if is_active {
                    theme::TEXT_STRONG
                } else if is_hover {
                    theme::TEXT
                } else {
                    theme::TEXT_INACTIVE
                };
                let mid_y = it.rect.y + it.rect.h / 2.0;
                if let Some((img, opacity)) = icon {
                    p.icon(bitmaps, img, icon_rect(it.rect.x + pad, mid_y, dpi), *opacity);
                }
                if let Some(t) = &it.text {
                    // An app pill: the name after the icon; the overflow pill: centred.
                    let x = if it.icon.is_some() { it.rect.x + pad + icon_w + icon_gap } else { it.rect.x + (it.rect.w - t.width()) / 2.0 };
                    p.text(t, PointF { x: x.round(), y: (mid_y - t.height() / 2.0).round() }, color);
                }
            }
            right.paint(p, h, dpi, now, hover);
        });
        if let Err(e) = r.and_then(|_| self.gfx.commit()) {
            log::error!("bar paint failed: {e}");
        }
        self.keep_animating();
    }

    /// What is under `p`: the tray button, a right item, or an app pill.
    pub(super) fn hit(&self, p: PointF) -> Option<(Target, RectF)> {
        self.right.hit(p).or_else(|| self.items.iter().find(|it| it.rect.contains(p)).map(|it| (it.target, it.rect)))
    }

    pub(super) fn update_hover(&mut self, p: Option<PointF>) {
        let hit = p.and_then(|p| self.hit(p));
        let hover = hit.map(|(t, _)| t);
        if hover != self.hover {
            self.hover = hover;
            self.on_hover_changed(hit.map(|(_, r)| r));
            self.paint();
        }
    }

    pub(super) fn on_click(&mut self, button: Button, p: PointF) {
        let target = self.hit(p);
        log::debug!(
            "click {button:?} at {p:?} target={:?} foreground={:?} popup_open={}",
            target.map(|t| t.0),
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() },
            self.popup.is_some()
        );
        // A click on the item whose menu is open just closes it (toggle).
        self.hide_tooltip();
        // The bar never takes focus, so a click on it does not close a panel by itself.
        if let Some(p) = &self.panel {
            let same = target.map(|t| t.0) == Some(p.anchor());
            self.begin_close_panel();
            if same {
                return;
            }
        }
        let toggled = self.popup.as_ref().is_some_and(|(_, ctx)| target.map(|t| t.0) == ctx.anchor_target() && button != Button::Middle);
        self.close_popup();
        if toggled {
            return;
        }
        match (target, button) {
            (Some((Target::Group(g), rect)), Button::Left) => self.run_click(left_click(&self.model, g), rect),
            (Some((Target::Group(g), rect)), Button::Middle) => self.run_click(middle_click(&self.model, g), rect),
            (Some((Target::Group(g), rect)), Button::Right) => self.open_app_menu(g, rect),
            (Some((Target::Overflow, rect)), Button::Left | Button::Right) => self.open_overflow_menu(rect),
            (Some((Target::Right(item), _)), b) => self.on_right_click(item, b),
            (Some((Target::Tray, _)), Button::Left) => self.open_tray(),
            (None, Button::Right) => self.open_bar_menu(p),
            _ => {}
        }
    }

    pub(super) fn run_click(&mut self, action: ClickAction, rect: RectF) {
        match action {
            ClickAction::Activate(h) => activate::activate(h),
            ClickAction::Minimize(h) => activate::minimize(h),
            ClickAction::Restore(h) => activate::restore(h),
            ClickAction::Close(h) => activate::close(h),
            ClickAction::ShowWindowList(g) => self.open_window_list(g, rect),
            ClickAction::Nothing => {}
        }
    }

    pub(super) fn window_info(&self, h: u64) -> Option<&WindowInfo> {
        self.model.window(h)
    }
}
