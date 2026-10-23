//! The tray button on the left and the right side of the bar (spec: M2 §2): placement, the
//! gliding hover pill, tooltips, clicks.

use newwin_core::anim::Tween;
use newwin_core::items::{RightItem, parse_right, place_right};
use newwin_platform::clock::{format_long_date, now_ms};
use newwin_platform::dpi::scale;
use newwin_platform::launch;
use newwin_ui::popup::PopupStyle;
use newwin_ui::tooltip::Tooltip;
use newwin_ui::{Painter, PointF, RectF, TextLayout, icons, theme};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::App;
use super::apps_view::{Button, Target};
use crate::bar::text_style;

/// A capsule already showing `shown`'s tooltip only changes its text (the volume while the wheel
/// turns); a fresh one would fade in again on every notch.
fn reuse_tooltip(shown: Option<RightItem>, want: RightItem) -> bool {
    shown == Some(want)
}

pub(super) const ANIM_TIMER: usize = 5;
pub(super) const TOOLTIP_TIMER: usize = 6;
const FRAME_MS: u32 = 16;
const PILL_FADE_MS: u32 = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RightSlot {
    pub item: RightItem,
    pub rect: RectF,
}

/// Everything the right side (and the tray button) draws, kept apart from the bar's layer so the
/// paint closure can borrow it.
pub(super) struct RightView {
    pub items: Vec<RightItem>,
    pub slots: Vec<RightSlot>,
    pub sep_x: Option<f32>,
    pub chevron: RectF,
    pub left_sep_x: f32,
    pub chevron_turn: Tween,
    pill_x: Tween,
    pill_w: Tween,
    pill_o: Tween,
    pub date: Option<TextLayout>,
    pub time: Option<TextLayout>,
    /// `None` until the audio service runs.
    pub speaker: Option<SpeakerView>,
    /// `None` until the network has been read once.
    pub net: Option<super::network_view::NetView>,
    /// `None` until the layout has been read once.
    pub lang: Option<super::layout_view::LangView>,
    /// Whose tooltip is showing.
    pub tooltip_item: Option<RightItem>,
}

/// The speaker icon's animated state.
pub(super) struct SpeakerView {
    pub available: bool,
    pub waves: Tween,
    pub mute: Tween,
}

impl RightView {
    pub(super) fn new(names: &[String]) -> RightView {
        RightView {
            items: parse_right(names),
            slots: Vec::new(),
            sep_x: None,
            chevron: RectF::default(),
            left_sep_x: 0.0,
            chevron_turn: Tween::at_rest(0.0),
            pill_x: Tween::at_rest(0.0),
            pill_w: Tween::at_rest(0.0),
            pill_o: Tween::at_rest(0.0),
            date: None,
            time: None,
            speaker: None,
            net: None,
            lang: None,
            tooltip_item: None,
        }
    }

    /// A new set or order of items (config change). Everything else stays: the icons keep their
    /// state and the next layout moves the items to their new slots.
    pub(super) fn set_items(&mut self, names: &[String]) {
        self.items = parse_right(names);
    }

    pub(super) fn slot(&self, item: RightItem) -> Option<RectF> {
        self.slots.iter().find(|s| s.item == item).map(|s| s.rect)
    }

    /// The tray button or a right item under `p`.
    pub(super) fn hit(&self, p: PointF) -> Option<(Target, RectF)> {
        if self.chevron.contains(p) {
            return Some((Target::Tray, self.chevron));
        }
        self.slots.iter().find(|s| s.rect.contains(p)).map(|s| (Target::Right(s.item), s.rect))
    }

    /// The hover pill heads for `rect`, or fades out. A pill that was invisible appears in place
    /// instead of gliding from wherever it was last.
    pub(super) fn hover_to(&mut self, rect: Option<RectF>, now: u64) {
        match rect {
            Some(r) => {
                if self.pill_o.value(now) < 0.05 {
                    self.pill_x = Tween::at_rest(r.x);
                    self.pill_w = Tween::at_rest(r.w);
                } else {
                    self.pill_x.to(r.x, now, theme::HOVER_GLIDE_MS);
                    self.pill_w.to(r.w, now, theme::HOVER_GLIDE_MS);
                }
                self.pill_o.to(1.0, now, PILL_FADE_MS);
            }
            None => self.pill_o.to(0.0, now, PILL_FADE_MS),
        }
    }

    pub(super) fn animating(&self, now: u64) -> bool {
        let speaker = self.speaker.as_ref().is_some_and(|s| s.waves.running(now) || s.mute.running(now));
        let net = self.net.as_ref().is_some_and(|n| n.animating(now));
        let lang = self.lang.as_ref().is_some_and(|l| l.fade.running(now));
        speaker || net || lang || [self.pill_x, self.pill_w, self.pill_o, self.chevron_turn].iter().any(|t| t.running(now))
    }

    /// Draws the tray button, the separators, the hover pill and the right items.
    pub(super) fn paint(&self, p: &mut Painter, h: f32, dpi: u32, now: u64, hover: Option<Target>) {
        let s = |v: f32| scale(v, dpi).round();
        let stroke = scale(theme::ICON_STROKE, dpi);
        let pill_h = s(theme::PILL_HEIGHT);
        let y = ((h - pill_h) / 2.0).round();
        let o = self.pill_o.value(now);
        if o > 0.0 {
            let pill = RectF::new(self.pill_x.value(now), y, self.pill_w.value(now), pill_h);
            p.fill_round_rect(pill, s(theme::PILL_RADIUS), theme::HOVER_PILL.with_alpha(theme::HOVER_PILL.a * o));
        }
        let sep = |p: &mut Painter, x: f32| {
            let x = x.round() + 0.5;
            p.line(PointF { x, y: (h / 2.0 - s(7.0)).round() }, PointF { x, y: (h / 2.0 + s(7.0)).round() }, theme::SEPARATOR, 1.0);
        };
        let lit = |t: Target| if hover == Some(t) { theme::TEXT_STRONG } else { theme::TEXT_INACTIVE };
        let c = self.chevron;
        icons::chevron(p, PointF { x: c.x + c.w / 2.0, y: h / 2.0 }, s(16.0), self.chevron_turn.value(now), lit(Target::Tray), stroke);
        sep(p, self.left_sep_x);
        if let Some(x) = self.sep_x {
            sep(p, x);
        }
        for slot in &self.slots {
            let r = slot.rect;
            let text = |p: &mut Painter, t: &TextLayout, color| {
                p.text(t, PointF { x: (r.x + (r.w - t.width()) / 2.0).round(), y: ((h - t.height()) / 2.0).round() }, color);
            };
            match slot.item {
                // The clock keeps M1's look; hover only brightens the date.
                RightItem::Date => {
                    if let Some(t) = &self.date {
                        text(p, t, if hover == Some(Target::Right(RightItem::Date)) { theme::TEXT_STRONG } else { theme::TEXT });
                    }
                }
                RightItem::Time => {
                    if let Some(t) = &self.time {
                        text(p, t, theme::TEXT_STRONG);
                    }
                }
                RightItem::Volume => {
                    if let Some(sp) = &self.speaker {
                        let c = PointF { x: r.x + r.w / 2.0, y: h / 2.0 };
                        let color = lit(Target::Right(RightItem::Volume));
                        if sp.available {
                            icons::speaker(p, c, s(18.0), sp.waves.value(now), sp.mute.value(now), color, stroke);
                        } else {
                            icons::speaker(p, c, s(18.0), 0.0, 1.0, color.with_alpha(color.a * 0.6), stroke);
                        }
                    }
                }
                RightItem::Search => {
                    icons::magnifier(p, PointF { x: r.x + r.w / 2.0, y: h / 2.0 }, s(16.0), lit(Target::Right(RightItem::Search)), stroke);
                }
                RightItem::Network => {
                    if let Some(n) = &self.net {
                        n.paint(p, PointF { x: r.x + r.w / 2.0, y: h / 2.0 }, s(18.0), lit(Target::Right(RightItem::Network)), stroke, now);
                    }
                }
                RightItem::Layout => {
                    if let Some(l) = &self.lang {
                        let color = lit(Target::Right(RightItem::Layout));
                        let f = l.fade.value(now);
                        if let Some(t) = l.prev_text.as_ref().filter(|_| f < 1.0) {
                            text(p, t, color.with_alpha(color.a * (1.0 - f)));
                        }
                        if let Some(t) = &l.text {
                            text(p, t, color.with_alpha(color.a * f));
                        }
                    }
                }
            }
        }
    }
}

impl App {
    /// Width of a right item, or `None` while it has nothing to show (it is left out).
    fn right_width(&self, item: RightItem, pad: f32) -> Option<f32> {
        match item {
            RightItem::Date => self.right.date.as_ref().map(|t| t.width() + 2.0 * pad),
            RightItem::Time => self.right.time.as_ref().map(|t| t.width() + 2.0 * pad),
            RightItem::Volume => self.right.speaker.as_ref().map(|_| scale(theme::PILL_HEIGHT, self.dpi).round()),
            RightItem::Network => self.right.net.as_ref().map(|_| scale(theme::PILL_HEIGHT, self.dpi).round()),
            RightItem::Search => Some(scale(theme::PILL_HEIGHT, self.dpi).round()),
            RightItem::Layout => self.right.lang.as_ref().and_then(|l| l.text.as_ref()).map(|t| t.width() + 2.0 * pad),
        }
    }

    /// Lays out the tray button and the right side; returns where the app pills may start and end.
    pub(super) fn relayout_right(&mut self, w: f32, h: f32) -> (f32, f32) {
        let dpi = self.dpi;
        let a = &self.config.appearance;
        let s = |v: f32| scale(v, dpi).round();
        let regular = text_style(a, dpi, theme::WEIGHT_REGULAR);
        let strong = text_style(a, dpi, theme::WEIGHT_SEMIBOLD);
        self.right.time = self.text.layout(&self.time_text, &strong, w / 3.0).ok();
        self.right.date = self.text.layout(&self.date_text, &regular, w / 3.0).ok();
        if let Some(l) = self.right.lang.as_mut() {
            l.text = self.text.layout(&l.code, &regular, w / 3.0).ok();
            l.prev_text = l.prev.as_ref().and_then(|p| self.text.layout(p, &regular, w / 3.0).ok());
        }
        let (edge, pad, pill_h) = (s(6.0), s(theme::PILL_PAD_X), s(theme::PILL_HEIGHT));
        let y = ((h - pill_h) / 2.0).round();
        self.right.chevron = RectF::new(edge, y, pill_h, pill_h);
        self.right.left_sep_x = self.right.chevron.right() + s(6.0);
        let widths: Vec<(RightItem, f32)> = self.right.items.iter().filter_map(|&i| self.right_width(i, pad).map(|w| (i, w))).collect();
        let (xs, sep) = place_right(&widths, w - edge, s(theme::ITEM_GAP), s(13.0));
        self.right.slots = xs.iter().zip(&widths).map(|(&(item, x), &(_, w))| RightSlot { item, rect: RectF::new(x.round(), y, w.round(), pill_h) }).collect();
        self.right.sep_x = sep;
        log::debug!("right slots {:?}", self.right.slots.iter().map(|s| (s.item, s.rect.x, s.rect.w)).collect::<Vec<_>>());
        let right_start = self.right.slots.first().map_or(w - edge, |s| s.rect.x);
        (self.right.left_sep_x + s(7.0), right_start - s(16.0))
    }

    /// Repaints while a tween runs; stops waking once all are at rest.
    pub(super) fn keep_animating(&mut self) {
        if self.right.animating(now_ms()) {
            unsafe { SetTimer(Some(self.bar), ANIM_TIMER, FRAME_MS, None) };
        }
    }

    pub(super) fn tooltip_text(&self, item: RightItem) -> Option<String> {
        match item {
            RightItem::Date => Some(format_long_date()),
            RightItem::Volume => self.volume_tooltip(),
            RightItem::Network => self.network_tooltip(),
            RightItem::Layout => self.layout_tooltip(),
            RightItem::Time => None,
            RightItem::Search => Some(self.strings.search.to_string()),
        }
    }

    /// The hovered target changed: move the pill and the tooltip along.
    pub(super) fn on_hover_changed(&mut self, rect: Option<RectF>) {
        let now = now_ms();
        let right_group = matches!(self.hover, Some(Target::Tray | Target::Right(_)));
        self.right.hover_to(rect.filter(|_| right_group), now);
        match self.hover {
            Some(Target::Right(item)) if self.tooltip_text(item).is_some() => {
                if self.tooltip.is_some() {
                    // Moving between items while a tooltip shows: no new delay.
                    self.show_tooltip();
                } else {
                    unsafe { SetTimer(Some(self.bar), TOOLTIP_TIMER, theme::TOOLTIP_DELAY_MS, None) };
                }
            }
            _ => self.hide_tooltip(),
        }
    }

    pub(super) fn show_tooltip(&mut self) {
        let Some(Target::Right(item)) = self.hover else { return };
        let (Some(text), Some(r)) = (self.tooltip_text(item), self.right.slot(item)) else { return };
        if reuse_tooltip(self.right.tooltip_item.filter(|_| self.tooltip.is_some()), item) {
            if let Some(t) = &self.tooltip {
                t.set_text(&text);
            }
            self.sync_glass();
            return;
        }
        self.right.tooltip_item = Some(item);
        let a = &self.config.appearance;
        let style = PopupStyle { font_family: a.font_family.clone(), font_size_px: scale(a.font_size, self.dpi), dpi: self.dpi };
        let anchor = PointF { x: self.bar_rect.left as f32 + r.x + r.w / 2.0, y: self.bar_rect.bottom as f32 + scale(6.0, self.dpi).round() };
        self.tooltip = None;
        match Tooltip::show(&self.gfx, &text, anchor, &style) {
            Ok(t) => self.tooltip = Some(t),
            Err(e) => log::error!("tooltip failed: {e}"),
        }
        self.sync_glass();
    }

    pub(super) fn hide_tooltip(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.bar), TOOLTIP_TIMER);
        }
        self.right.tooltip_item = None;
        if self.tooltip.take().is_some() {
            self.sync_glass();
        }
    }

    pub(super) fn on_right_click(&mut self, item: RightItem, button: Button) {
        self.hide_tooltip();
        match (item, button) {
            (RightItem::Date, Button::Left) => launch::open_notification_center(),
            (RightItem::Volume, Button::Left) => self.toggle_volume_panel(),
            (RightItem::Volume, Button::Middle) => self.toggle_mute(),
            (RightItem::Layout, Button::Left) => self.switch_layout(),
            (RightItem::Search, Button::Left) => self.toggle_spotlight(),
            (RightItem::Network, Button::Left) => {
                if let Err(e) = newwin_platform::launch::shell_execute("open", "ms-settings:network", "") {
                    log::error!("network settings: {e}");
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shown_tooltip_is_updated_not_recreated() {
        assert!(reuse_tooltip(Some(RightItem::Volume), RightItem::Volume), "scrolling the wheel: same capsule, new text");
        assert!(!reuse_tooltip(Some(RightItem::Date), RightItem::Volume), "another item: a capsule in its place");
        assert!(!reuse_tooltip(None, RightItem::Volume));
    }
}
