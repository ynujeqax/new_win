//! The media panel (1.0.5): what is playing, which app plays it, and ⏮ ⏯ ⏭.

use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::media::{MediaCmd, MediaInfo};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LEFT, VK_RIGHT, VK_SPACE};

use crate::panel::{PanelContent, PanelInput, PanelReply};
use crate::popup::PopupStyle;
use crate::{Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, icons, theme};

const FADE_MS: u32 = 160;

pub fn encode(c: MediaCmd) -> u32 {
    match c {
        MediaCmd::PlayPause => 0,
        MediaCmd::Next => 1,
        MediaCmd::Prev => 2,
    }
}

pub fn decode(id: u32) -> Option<MediaCmd> {
    match id {
        0 => Some(MediaCmd::PlayPause),
        1 => Some(MediaCmd::Next),
        2 => Some(MediaCmd::Prev),
        _ => None,
    }
}

struct Text {
    cache: TextCache,
    title: TextStyle,
    regular: TextStyle,
    small: TextStyle,
}

pub struct MediaPanel {
    info: MediaInfo,
    app_name: String,
    dpi: u32,
    text: Option<Text>,
    title: Option<TextLayout>,
    artist: Option<TextLayout>,
    app: Option<TextLayout>,
    hover: Option<MediaCmd>,
    hover_o: Tween,
    hover_x: Tween,
    /// Play ⇄ pause cross-fade.
    playing: Tween,
}

impl MediaPanel {
    pub fn new(info: MediaInfo, app_name: String, style: &PopupStyle, gfx: &Rc<Gfx>) -> MediaPanel {
        let regular = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let text = Text {
            cache: TextCache::new(gfx),
            title: TextStyle { weight: theme::WEIGHT_SEMIBOLD, size: (style.font_size_px * 1.15).round(), ..regular.clone() },
            small: TextStyle { size: (style.font_size_px * 0.85).round(), ..regular.clone() },
            regular,
        };
        let mut p = Self::build(info, app_name, style.dpi, Some(text));
        p.rebuild();
        p
    }

    /// Without text layout (geometry and input only), at 96 DPI.
    pub fn for_test(info: MediaInfo) -> MediaPanel {
        Self::build(info, String::new(), 96, None)
    }

    fn build(info: MediaInfo, app_name: String, dpi: u32, text: Option<Text>) -> MediaPanel {
        let playing = Tween::at_rest(if info.playing { 1.0 } else { 0.0 });
        MediaPanel {
            info,
            app_name,
            dpi,
            text,
            title: None,
            artist: None,
            app: None,
            hover: None,
            hover_o: Tween::at_rest(0.0),
            hover_x: Tween::at_rest(0.0),
            playing,
        }
    }

    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    fn width(&self) -> f32 {
        self.s(300.0)
    }

    fn pad(&self) -> f32 {
        self.s(16.0)
    }

    /// A new track, or play/pause from elsewhere (keys, the app itself).
    pub fn set_info(&mut self, info: MediaInfo, app_name: String, now: u64) {
        self.playing.to(if info.playing { 1.0 } else { 0.0 }, now, FADE_MS);
        self.info = info;
        self.app_name = app_name;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let room = self.width() - 2.0 * self.pad();
        let Some(t) = self.text.as_mut() else { return };
        self.title = t.cache.layout(self.info.title.trim(), &t.title, room).ok();
        self.artist = (!self.info.artist.trim().is_empty()).then(|| t.cache.layout(self.info.artist.trim(), &t.regular, room).ok()).flatten();
        self.app = (!self.app_name.is_empty()).then(|| t.cache.layout(&self.app_name, &t.small, room).ok()).flatten();
    }

    fn text_h(&self) -> f32 {
        self.s(20.0) + self.s(18.0) + self.s(16.0)
    }

    /// Centre of a control button.
    pub fn button_center(&self, c: MediaCmd) -> PointF {
        let x = self.width() / 2.0
            + match c {
                MediaCmd::Prev => -self.s(64.0),
                MediaCmd::PlayPause => 0.0,
                MediaCmd::Next => self.s(64.0),
            };
        PointF { x, y: self.pad() + self.text_h() + self.s(14.0) + self.s(22.0) }
    }

    fn button_r(&self, c: MediaCmd) -> f32 {
        if c == MediaCmd::PlayPause { self.s(22.0) } else { self.s(18.0) }
    }

    fn enabled(&self, c: MediaCmd) -> bool {
        match c {
            MediaCmd::PlayPause => self.info.can_toggle,
            MediaCmd::Next => self.info.can_next,
            MediaCmd::Prev => self.info.can_prev,
        }
    }

    fn hit(&self, p: PointF) -> Option<MediaCmd> {
        [MediaCmd::Prev, MediaCmd::PlayPause, MediaCmd::Next].into_iter().find(|&c| {
            let o = self.button_center(c);
            let r = self.button_r(c);
            (p.x - o.x).powi(2) + (p.y - o.y).powi(2) <= r * r
        })
    }

    fn set_hover(&mut self, h: Option<MediaCmd>, now: u64) -> PanelReply {
        let h = h.filter(|&c| self.enabled(c));
        if h == self.hover {
            return PanelReply::default();
        }
        if let Some(c) = h {
            let x = self.button_center(c).x;
            if self.hover_o.value(now) < 0.05 {
                self.hover_x = Tween::at_rest(x);
            } else {
                self.hover_x.to(x, now, theme::HOVER_GLIDE_MS);
            }
        }
        self.hover_o.to(if h.is_some() { 1.0 } else { 0.0 }, now, FADE_MS);
        self.hover = h;
        PanelReply { repaint: true, ..Default::default() }
    }

    fn press(&self, c: MediaCmd) -> PanelReply {
        if self.enabled(c) { PanelReply { action: Some(encode(c)), ..Default::default() } } else { PanelReply::default() }
    }
}

impl PanelContent for MediaPanel {
    fn size(&self) -> (f32, f32) {
        let controls_y = self.button_center(MediaCmd::PlayPause).y;
        (self.width(), controls_y + self.s(22.0) + self.pad())
    }

    fn paint(&mut self, p: &mut Painter, now: u64) {
        let pad = self.pad();
        let mut y = pad;
        if let Some(t) = &self.title {
            p.text(t, PointF { x: pad, y }, theme::TEXT_STRONG);
            y += t.height() + self.s(2.0);
        }
        if let Some(t) = &self.artist {
            p.text(t, PointF { x: pad, y }, theme::TEXT);
            y += t.height() + self.s(2.0);
        }
        if let Some(t) = &self.app {
            p.text(t, PointF { x: pad, y }, theme::TEXT_INACTIVE);
        }
        // The hover circle glides between the buttons.
        let o = self.hover_o.value(now);
        if o > 0.0
            && let Some(c) = self.hover
        {
            let r = self.button_r(c);
            let cy = self.button_center(c).y;
            let x = self.hover_x.value(now);
            p.fill_round_rect(RectF::new(x - r, cy - r, 2.0 * r, 2.0 * r), r, theme::POPUP_HOVER.with_alpha(theme::POPUP_HOVER.a * o));
        }
        let color = |c: MediaCmd| if !self.enabled(c) { theme::TEXT_DISABLED } else if self.hover == Some(c) { theme::TEXT_STRONG } else { theme::TEXT };
        icons::skip(p, self.button_center(MediaCmd::Prev), self.s(18.0), true, color(MediaCmd::Prev));
        icons::skip(p, self.button_center(MediaCmd::Next), self.s(18.0), false, color(MediaCmd::Next));
        let c = self.button_center(MediaCmd::PlayPause);
        let k = self.playing.value(now);
        let col = color(MediaCmd::PlayPause);
        if k < 1.0 {
            icons::play(p, c, self.s(22.0), col.with_alpha(col.a * (1.0 - k)));
        }
        if k > 0.0 {
            icons::pause(p, c, self.s(22.0), col.with_alpha(col.a * k));
        }
    }

    fn input(&mut self, ev: PanelInput, now: u64) -> PanelReply {
        match ev {
            PanelInput::Move(p) => self.set_hover(self.hit(p), now),
            PanelInput::Leave => self.set_hover(None, now),
            PanelInput::Release(p) => match self.hit(p) {
                Some(c) => self.press(c),
                None => PanelReply::default(),
            },
            PanelInput::Key(vk) if vk == VK_SPACE.0 => self.press(MediaCmd::PlayPause),
            PanelInput::Key(vk) if vk == VK_RIGHT.0 => self.press(MediaCmd::Next),
            PanelInput::Key(vk) if vk == VK_LEFT.0 => self.press(MediaCmd::Prev),
            _ => PanelReply::default(),
        }
    }

    fn animating(&self, now: u64) -> bool {
        [self.hover_o, self.hover_x, self.playing].iter().any(|t| t.running(now))
    }
}
