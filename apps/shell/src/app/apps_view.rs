//! Running apps on the bar: model updates, layout, painting, hit testing, clicks.

use newwin_core::apps::{ClickAction, WindowInfo, is_screen_snip, left_click, middle_click};
use newwin_core::items::RightItem;
use newwin_core::layout::fit_labels;
use newwin_platform::clock::now_ms;
use newwin_platform::activate;
use newwin_platform::dpi::scale;
use newwin_platform::windows_watch::{WindowEvent, describe};
use newwin_platform::worker::ResolvedName;
use newwin_ui::{Painter, PointF, RectF, TextLayout, theme};

use super::App;
use crate::bar::{paint_background, text_style};

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
    pub text: TextLayout,
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
        let names: Vec<(String, bool)> =
            self.model.groups().iter().enumerate().map(|(i, g)| (g.name.clone(), Some(i) == active)).collect();
        let natural: Vec<f32> = names
            .iter()
            .map(|(n, act)| self.text.natural_width(n, if *act { &strong } else { &regular }).unwrap_or(0.0) + 2.0 * pill_pad)
            .collect();
        let available = (end - start).max(0.0);
        let fit = fit_labels(&natural, available, s(40.0), s(theme::ITEM_GAP), s(24.0));
        let mut items = Vec::new();
        let mut x = start;
        let y = ((h - pill_h) / 2.0).round();
        for (i, width) in fit.widths.iter().enumerate() {
            let (name, act) = &names[i];
            let style = if *act { &strong } else { &regular };
            if let Ok(text) = self.text.layout(name, style, (width - 2.0 * pill_pad).max(1.0)) {
                items.push(BarItem { target: Target::Group(i), rect: RectF::new(x, y, *width, pill_h), text });
            }
            x += width + s(theme::ITEM_GAP);
        }
        if fit.overflow_from.is_some()
            && let Ok(text) = self.text.layout("»", &regular, s(24.0))
        {
            items.push(BarItem { target: Target::Overflow, rect: RectF::new(x, y, s(24.0), pill_h), text });
        }
        let _ = h;
        self.items = items;
        self.overflow_from = fit.overflow_from;
    }

    pub(super) fn paint(&mut self) {
        let Some(layer) = self.bar_layer.as_mut() else { return };
        let (w, h) = layer.size();
        let (w, h) = (w as f32, h as f32);
        let dpi = self.dpi;
        let a = &self.config.appearance;
        let radius = scale(theme::PILL_RADIUS, dpi).round();
        let active = self.model.active_group();
        let (items, hover, glass, right) = (&self.items, self.hover, self.bar_glass, &self.right);
        let now = now_ms();
        let r = layer.draw(None, |p: &mut Painter| {
            paint_background(p, w, h, dpi, a, glass);
            for it in items {
                let is_active = matches!(it.target, Target::Group(g) if Some(g) == active);
                let is_hover = hover == Some(it.target);
                if is_active {
                    p.fill_round_rect(it.rect, radius, theme::ACTIVE_PILL);
                } else if is_hover {
                    p.fill_round_rect(it.rect, radius, theme::HOVER_PILL);
                }
                let color = if is_active {
                    theme::TEXT_STRONG
                } else if is_hover {
                    theme::TEXT
                } else {
                    theme::TEXT_INACTIVE
                };
                let x = (it.rect.x + (it.rect.w - it.text.width()) / 2.0).round();
                p.text(&it.text, PointF { x, y: (it.rect.y + (it.rect.h - it.text.height()) / 2.0).round() }, color);
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
