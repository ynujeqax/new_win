//! The volume panel (spec: M2 §3.2): mute, slider, value; then the output devices.

use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::audio::{Wheel, step, waves_for};
use newwin_platform::audio::AudioSnapshot;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_END, VK_HOME, VK_LEFT, VK_RIGHT};

use crate::panel::{PanelContent, PanelInput, PanelReply};
use crate::popup::PopupStyle;
use crate::slider::{level_at, thumb_x};
use crate::{Color, Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, icons, theme};

const LEVEL_MS: u32 = 120;
const MUTE_MS: u32 = 120;
const FADE_MS: u32 = 120;
const TRACK: Color = Color::rgba(255, 255, 255, 0.14);
const FILL: Color = Color::rgba(255, 255, 255, 0.85);
const THUMB: Color = Color::rgba(255, 255, 255, 0.95);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeAction {
    SetLevel(u8),
    ToggleMute,
    PickDevice(usize),
    SoundSettings,
}

pub fn encode(a: VolumeAction) -> u32 {
    match a {
        VolumeAction::ToggleMute => 1,
        VolumeAction::SoundSettings => 3,
        VolumeAction::SetLevel(l) => 1000 + u32::from(l.min(100)),
        VolumeAction::PickDevice(i) => 2000 + i as u32,
    }
}

pub fn decode(id: u32) -> Option<VolumeAction> {
    match id {
        1 => Some(VolumeAction::ToggleMute),
        3 => Some(VolumeAction::SoundSettings),
        1000..=1100 => Some(VolumeAction::SetLevel((id - 1000) as u8)),
        2000..=2999 => Some(VolumeAction::PickDevice((id - 2000) as usize)),
        _ => None,
    }
}

struct Text {
    cache: TextCache,
    regular: TextStyle,
    bold: TextStyle,
}

/// What a click at a point hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Mute,
    Track,
    Device(usize),
    Settings,
}

pub struct VolumePanel {
    snapshot: Option<AudioSnapshot>,
    strings: (String, String),
    settings_row: bool,
    dpi: u32,
    text: Option<Text>,
    devices: Vec<Option<TextLayout>>,
    value: Option<TextLayout>,
    empty: Option<TextLayout>,
    settings: Option<TextLayout>,
    level: Tween,
    mute: Tween,
    dragging: bool,
    wheel: Wheel,
    hover: Option<Hit>,
    pill_y: Tween,
    pill_o: Tween,
}

impl VolumePanel {
    /// `strings` = ("No output devices", "Sound settings…").
    pub fn new(snapshot: Option<AudioSnapshot>, strings: (&str, &str), style: &PopupStyle, gfx: &Rc<Gfx>) -> VolumePanel {
        let regular = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let bold = TextStyle { weight: theme::WEIGHT_SEMIBOLD, ..regular.clone() };
        let mut v = Self::build(snapshot, strings, style.dpi, Some(Text { cache: TextCache::new(gfx), regular, bold }));
        v.rebuild();
        v
    }

    /// Without text layout (geometry and input only).
    pub fn for_test(snapshot: Option<AudioSnapshot>) -> VolumePanel {
        Self::build(snapshot, ("", ""), 96, None)
    }

    fn build(snapshot: Option<AudioSnapshot>, strings: (&str, &str), dpi: u32, text: Option<Text>) -> VolumePanel {
        let (level, muted) = snapshot.as_ref().map_or((0, false), |s| (s.level, s.muted));
        VolumePanel {
            snapshot,
            strings: (strings.0.to_string(), strings.1.to_string()),
            settings_row: false,
            dpi,
            text,
            devices: Vec::new(),
            value: None,
            empty: None,
            settings: None,
            level: Tween::at_rest(f32::from(level)),
            mute: Tween::at_rest(if muted { 1.0 } else { 0.0 }),
            dragging: false,
            wheel: Wheel::default(),
            hover: None,
            pill_y: Tween::at_rest(0.0),
            pill_o: Tween::at_rest(0.0),
        }
    }

    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    /// New state from Windows (volume keys, another app, a device change): the slider glides.
    pub fn set_snapshot(&mut self, s: Option<AudioSnapshot>, now: u64) {
        if let Some(s) = &s {
            if !self.dragging {
                self.level.to(f32::from(s.level), now, LEVEL_MS);
            }
            self.mute.to(if s.muted { 1.0 } else { 0.0 }, now, MUTE_MS);
        }
        self.snapshot = s;
        self.rebuild();
    }

    /// Switching devices failed: offer Windows' own sound settings.
    pub fn show_settings_row(&mut self) {
        self.settings_row = true;
        self.rebuild();
    }

    pub fn level_shown(&self, now: u64) -> f32 {
        self.level.value(now)
    }

    fn current(&self) -> u8 {
        self.level.target().round() as u8
    }

    fn device_count(&self) -> usize {
        self.snapshot.as_ref().map_or(0, |s| s.devices.len())
    }

    fn width(&self) -> f32 {
        self.s(260.0)
    }

    /// Rows under the slider: devices, the settings row, or the empty-state row.
    fn rows(&self) -> usize {
        if self.snapshot.is_none() {
            return 1;
        }
        self.device_count() + usize::from(self.settings_row)
    }

    fn row1(&self) -> RectF {
        let pad = self.s(theme::POPUP_PAD);
        RectF::new(pad, pad, self.width() - 2.0 * pad, self.s(44.0))
    }

    fn mute_rect(&self) -> RectF {
        let r = self.row1();
        let b = self.s(26.0);
        RectF::new(r.x + self.s(6.0), r.y + (r.h - b) / 2.0, b, b)
    }

    /// The slider's track (4 px tall, vertically centred in the first row).
    pub fn track(&self) -> RectF {
        let r = self.row1();
        let left = self.mute_rect().right() + self.s(10.0);
        let right = r.right() - self.s(8.0) - self.s(30.0) - self.s(10.0);
        let h = self.s(4.0);
        RectF::new(left, r.y + (r.h - h) / 2.0, (right - left).max(1.0), h)
    }

    fn rows_top(&self) -> f32 {
        if self.snapshot.is_none() { self.s(theme::POPUP_PAD) } else { self.row1().bottom() + self.s(9.0) }
    }

    fn row_rect(&self, i: usize) -> RectF {
        let pad = self.s(theme::POPUP_PAD);
        let ih = self.s(theme::POPUP_ITEM_HEIGHT);
        RectF::new(pad, self.rows_top() + i as f32 * ih, self.width() - 2.0 * pad, ih)
    }

    pub fn device_center(&self, i: usize) -> PointF {
        let r = self.row_rect(i);
        PointF { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0 }
    }

    fn hit(&self, p: PointF) -> Option<Hit> {
        self.snapshot.as_ref()?;
        if self.mute_rect().contains(p) {
            return Some(Hit::Mute);
        }
        let t = self.track();
        let row = self.row1();
        // The whole row height around the track is grabbable, plus the thumb's overhang.
        if p.y >= row.y && p.y < row.bottom() && p.x >= t.x - self.s(7.0) && p.x <= t.right() + self.s(7.0) {
            return Some(Hit::Track);
        }
        (0..self.rows()).find(|&i| self.row_rect(i).contains(p)).map(|i| if i < self.device_count() { Hit::Device(i) } else { Hit::Settings })
    }

    fn rebuild(&mut self) {
        let level = self.current();
        let width = self.width();
        // Room for the current device's check at the right end.
        let room = width - 2.0 * self.s(theme::POPUP_PAD) - 2.0 * self.s(theme::POPUP_ITEM_PAD_X) - self.s(20.0);
        let Some(t) = self.text.as_mut() else { return };
        let snap = self.snapshot.as_ref();
        self.devices = snap
            .map(|s| {
                s.devices
                    .iter()
                    .map(|d| {
                        let style = if s.default_id.as_deref() == Some(d.id.as_str()) { &t.bold } else { &t.regular };
                        t.cache.layout(&d.name, style, room).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.value = t.cache.layout(&level.to_string(), &t.regular, 100.0).ok();
        self.empty = t.cache.layout(&self.strings.0, &t.regular, room).ok();
        self.settings = t.cache.layout(&self.strings.1, &t.regular, room).ok();
    }

    fn set_hover(&mut self, h: Option<Hit>, now: u64) {
        if h == self.hover {
            return;
        }
        let row = match h {
            Some(Hit::Device(i)) => Some(i),
            Some(Hit::Settings) => Some(self.device_count()),
            _ => None,
        };
        if let Some(i) = row {
            let y = self.row_rect(i).y;
            if self.pill_o.value(now) < 0.05 {
                self.pill_y = Tween::at_rest(y);
            } else {
                self.pill_y.to(y, now, theme::HOVER_GLIDE_MS);
            }
        }
        self.pill_o.to(if row.is_some() { 1.0 } else { 0.0 }, now, FADE_MS);
        self.hover = h;
    }

    /// A new level from the user: animated (`glide`) or following the pointer.
    fn level_to(&mut self, level: u8, now: u64, glide: bool) -> PanelReply {
        if glide {
            self.level.to(f32::from(level), now, LEVEL_MS);
        } else {
            self.level = Tween::at_rest(f32::from(level));
        }
        self.rebuild();
        PanelReply { repaint: true, action: Some(encode(VolumeAction::SetLevel(level))), ..Default::default() }
    }
}

impl PanelContent for VolumePanel {
    fn size(&self) -> (f32, f32) {
        let pad = self.s(theme::POPUP_PAD);
        (self.width(), self.rows_top() + self.rows() as f32 * self.s(theme::POPUP_ITEM_HEIGHT) + pad)
    }

    fn paint(&mut self, p: &mut Painter, now: u64) {
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let row_text = |p: &mut Painter, r: RectF, t: &TextLayout, c: Color| {
            p.text(t, PointF { x: r.x + pad_x, y: (r.y + (r.h - t.height()) / 2.0).round() }, c);
        };
        let Some(snap) = self.snapshot.as_ref() else {
            if let Some(t) = &self.empty {
                row_text(p, self.row_rect(0), t, theme::TEXT_INACTIVE);
            }
            return;
        };
        // Row 1: mute, slider, value.
        let level = self.level.value(now);
        let m = self.mute_rect();
        let mute_lit = self.hover == Some(Hit::Mute);
        if mute_lit {
            p.fill_round_rect(m, m.h / 2.0, theme::HOVER_PILL);
        }
        let waves = f32::from(waves_for(level.round() as u8));
        let stroke = theme::ICON_STROKE * self.dpi as f32 / 96.0;
        icons::speaker(p, PointF { x: m.x + m.w / 2.0, y: m.y + m.h / 2.0 }, self.s(18.0), waves, self.mute.value(now), if mute_lit { theme::TEXT_STRONG } else { theme::TEXT }, stroke);
        let t = self.track();
        let x = thumb_x(level.round() as u8, t);
        p.fill_round_rect(t, t.h / 2.0, TRACK);
        p.fill_round_rect(RectF::new(t.x, t.y, (x - t.x).max(0.0), t.h), t.h / 2.0, FILL);
        let k = self.s(14.0);
        p.fill_round_rect(RectF::new(x - k / 2.0, t.y + t.h / 2.0 - k / 2.0, k, k), k / 2.0, THUMB);
        if let Some(v) = &self.value {
            let r = self.row1();
            p.text(v, PointF { x: r.right() - self.s(8.0) - v.width(), y: (r.y + (r.h - v.height()) / 2.0).round() }, theme::TEXT);
        }
        // Separator, then the devices.
        let sep_y = (self.row1().bottom() + self.s(9.0) / 2.0).round() + 0.5;
        let base = self.row_rect(0);
        p.line(PointF { x: base.x + pad_x, y: sep_y }, PointF { x: base.right() - pad_x, y: sep_y }, theme::SEPARATOR, 1.0);
        let item_r = self.s(theme::POPUP_ITEM_RADIUS);
        // The hover pill goes under the names; the current device is marked by a check, so it
        // never looks like a second hovered row.
        let o = self.pill_o.value(now);
        if o > 0.0 {
            let pill = RectF::new(base.x, self.pill_y.value(now), base.w, base.h);
            p.fill_round_rect(pill, item_r, theme::POPUP_HOVER.with_alpha(theme::POPUP_HOVER.a * o));
        }
        for (i, d) in snap.devices.iter().enumerate() {
            let r = self.row_rect(i);
            let current = snap.default_id.as_deref() == Some(d.id.as_str());
            if current {
                icons::check(p, PointF { x: r.right() - pad_x - self.s(6.0), y: r.y + r.h / 2.0 }, self.s(12.0), theme::TEXT_STRONG, stroke);
            }
            if let Some(Some(t)) = self.devices.get(i) {
                row_text(p, r, t, if current || self.hover == Some(Hit::Device(i)) { theme::TEXT_STRONG } else { theme::TEXT });
            }
        }
        if self.settings_row
            && let Some(t) = &self.settings
        {
            let lit = self.hover == Some(Hit::Settings);
            row_text(p, self.row_rect(self.device_count()), t, if lit { theme::TEXT } else { theme::TEXT_INACTIVE });
        }
    }

    fn input(&mut self, ev: PanelInput, now: u64) -> PanelReply {
        let repaint = PanelReply { repaint: true, ..Default::default() };
        if self.snapshot.is_none() {
            return PanelReply::default();
        }
        match ev {
            PanelInput::Press(p) if self.hit(p) == Some(Hit::Track) => {
                self.dragging = true;
                self.level_to(level_at(p.x, self.track()), now, true)
            }
            PanelInput::Move(p) if self.dragging => {
                let l = level_at(p.x, self.track());
                if l == self.current() && !self.level.running(now) { PanelReply::default() } else { self.level_to(l, now, false) }
            }
            PanelInput::Move(p) => {
                self.set_hover(self.hit(p), now);
                repaint
            }
            PanelInput::Leave => {
                self.set_hover(None, now);
                repaint
            }
            PanelInput::Release(_) if self.dragging => {
                self.dragging = false;
                PanelReply::default()
            }
            PanelInput::Release(p) => match self.hit(p) {
                Some(Hit::Mute) => {
                    let muted = self.mute.target() < 0.5;
                    self.mute.to(if muted { 1.0 } else { 0.0 }, now, MUTE_MS);
                    PanelReply { repaint: true, action: Some(encode(VolumeAction::ToggleMute)), ..Default::default() }
                }
                Some(Hit::Device(i)) => PanelReply { action: Some(encode(VolumeAction::PickDevice(i))), ..Default::default() },
                Some(Hit::Settings) => PanelReply { action: Some(encode(VolumeAction::SoundSettings)), close: true, ..Default::default() },
                _ => PanelReply::default(),
            },
            PanelInput::Wheel(d) => {
                let n = self.wheel.notches(d);
                if n == 0 { PanelReply::default() } else { self.level_to(step(self.current(), n, 2), now, true) }
            }
            PanelInput::Key(vk) => {
                let l = self.current();
                let target = match vk {
                    k if k == VK_LEFT.0 => step(l, -1, 1),
                    k if k == VK_RIGHT.0 => step(l, 1, 1),
                    k if k == VK_HOME.0 => 0,
                    k if k == VK_END.0 => 100,
                    _ => return PanelReply::default(),
                };
                self.level_to(target, now, true)
            }
            PanelInput::Press(_) | PanelInput::RightRelease(_) | PanelInput::Char(_) | PanelInput::CtrlKey(_) | PanelInput::ContextMenu(_) => PanelReply::default(),
        }
    }

    fn animating(&self, now: u64) -> bool {
        [self.level, self.mute, self.pill_y, self.pill_o].iter().any(|t| t.running(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::{PanelContent, PanelInput};
    use newwin_platform::audio::{AudioDevice, AudioSnapshot};

    fn snap() -> AudioSnapshot {
        AudioSnapshot {
            level: 42,
            muted: false,
            devices: vec![AudioDevice { id: "a".into(), name: "Speakers".into() }, AudioDevice { id: "b".into(), name: "Headphones".into() }],
            default_id: Some("a".into()),
        }
    }

    #[test]
    fn volume_actions_roundtrip() {
        for a in [VolumeAction::SetLevel(0), VolumeAction::SetLevel(100), VolumeAction::ToggleMute, VolumeAction::PickDevice(3), VolumeAction::SoundSettings] {
            assert_eq!(decode(encode(a)), Some(a));
        }
        assert_eq!(decode(1042), Some(VolumeAction::SetLevel(42)));
        assert_eq!(decode(999_999), None);
    }

    #[test]
    fn drag_emits_levels_without_tween() {
        let mut v = VolumePanel::for_test(Some(snap()));
        let t = v.track();
        let at = |f: f32| PointF { x: t.x + t.w * f, y: t.y + t.h / 2.0 };
        assert_eq!(v.input(PanelInput::Press(at(0.3)), 0).action, Some(encode(VolumeAction::SetLevel(30))));
        assert_eq!(v.input(PanelInput::Move(at(0.6)), 10).action, Some(encode(VolumeAction::SetLevel(60))));
        assert_eq!(v.level_shown(10), 60.0, "the thumb follows the pointer at once while dragging");
        assert_eq!(v.input(PanelInput::Release(at(0.6)), 20).action, None);
    }

    #[test]
    fn wheel_and_keys_step_the_level() {
        let mut v = VolumePanel::for_test(Some(snap()));
        assert_eq!(v.input(PanelInput::Wheel(120), 0).action, Some(encode(VolumeAction::SetLevel(44))));
        assert_eq!(v.input(PanelInput::Key(0x25), 0).action, Some(encode(VolumeAction::SetLevel(43))), "← is −1");
        assert_eq!(v.input(PanelInput::Key(0x23), 0).action, Some(encode(VolumeAction::SetLevel(100))), "End");
    }

    #[test]
    fn device_rows_and_empty_state() {
        let mut v = VolumePanel::for_test(Some(snap()));
        let r = v.input(PanelInput::Release(v.device_center(1)), 0);
        assert_eq!(r.action, Some(encode(VolumeAction::PickDevice(1))));
        let (_, h_two) = v.size();
        v.show_settings_row();
        assert!(v.size().1 > h_two);
        let empty = VolumePanel::for_test(None);
        assert!(empty.size().1 < h_two, "only the 'no devices' row");
    }
}
