//! The calendar panel (1.0.5): a month under the date, today marked, ‹ › and the wheel to
//! leaf through months, a click on the title to come back to today.

use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::calendar::{Month, grid};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT};

use crate::panel::{PanelContent, PanelInput, PanelReply};
use crate::popup::PopupStyle;
use crate::{Color, Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, icons, theme};

const LEAF_MS: u32 = 260;
const FADE_MS: u32 = 140;
/// Today's number, cut out of its white disc.
const ON_WHITE: Color = Color::rgba(10, 10, 12, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Prev,
    Next,
    Title,
}

struct Text {
    cache: TextCache,
    title: TextStyle,
    regular: TextStyle,
    strong: TextStyle,
    small: TextStyle,
}

pub struct CalendarPanel {
    today: (i32, u32, u32),
    first_weekday: u32,
    month_name: Box<dyn Fn(Month) -> String>,
    shown: Month,
    /// The month leafed away from, sliding out while `shown` slides in; `dir` is +1 for later.
    prev: Option<Month>,
    dir: f32,
    leaf: Tween,
    dpi: u32,
    text: Option<Text>,
    title: Option<TextLayout>,
    prev_title: Option<TextLayout>,
    weekdays: Vec<TextLayout>,
    days: Vec<TextLayout>,
    days_strong: Vec<TextLayout>,
    hover: Option<Hit>,
    hover_o: Tween,
}

impl CalendarPanel {
    /// `weekdays` are the short names Monday first; `month_name` titles a month ("October 2026").
    pub fn new(today: (i32, u32, u32), first_weekday: u32, weekdays: [String; 7], month_name: Box<dyn Fn(Month) -> String>, style: &PopupStyle, gfx: &Rc<Gfx>) -> CalendarPanel {
        let regular = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let mut text = Text {
            cache: TextCache::new(gfx),
            title: TextStyle { weight: theme::WEIGHT_SEMIBOLD, size: (style.font_size_px * 1.1).round(), ..regular.clone() },
            strong: TextStyle { weight: theme::WEIGHT_SEMIBOLD, ..regular.clone() },
            small: TextStyle { size: (style.font_size_px * 0.85).round(), ..regular.clone() },
            regular,
        };
        let order: Vec<usize> = (0..7).map(|i| (first_weekday as usize + i) % 7).collect();
        let weekdays = order.iter().filter_map(|&i| text.cache.layout(&weekdays[i], &text.small, 100.0).ok()).collect();
        let days = (1..=31).filter_map(|d| text.cache.layout(&d.to_string(), &text.regular, 100.0).ok()).collect();
        let days_strong = (1..=31).filter_map(|d| text.cache.layout(&d.to_string(), &text.strong, 100.0).ok()).collect();
        let mut p = Self::build(today, first_weekday, month_name, style.dpi, Some(text));
        p.weekdays = weekdays;
        p.days = days;
        p.days_strong = days_strong;
        p.rebuild();
        p
    }

    /// Without text layout (geometry and input only), at 96 DPI.
    pub fn for_test(today: (i32, u32, u32), first_weekday: u32) -> CalendarPanel {
        Self::build(today, first_weekday, Box::new(|m| format!("{}-{}", m.year, m.month)), 96, None)
    }

    fn build(today: (i32, u32, u32), first_weekday: u32, month_name: Box<dyn Fn(Month) -> String>, dpi: u32, text: Option<Text>) -> CalendarPanel {
        CalendarPanel {
            today,
            first_weekday,
            month_name,
            shown: Month { year: today.0, month: today.1 },
            prev: None,
            dir: 1.0,
            leaf: Tween::at_rest(1.0),
            dpi,
            text,
            title: None,
            prev_title: None,
            weekdays: Vec::new(),
            days: Vec::new(),
            days_strong: Vec::new(),
            hover: None,
            hover_o: Tween::at_rest(0.0),
        }
    }

    pub fn shown(&self) -> Month {
        self.shown
    }

    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    fn pad(&self) -> f32 {
        self.s(14.0)
    }

    fn cell(&self) -> f32 {
        self.s(36.0)
    }

    fn header_h(&self) -> f32 {
        self.s(36.0)
    }

    fn weekdays_h(&self) -> f32 {
        self.s(24.0)
    }

    fn button(&self, h: Hit) -> RectF {
        let b = self.s(28.0);
        let right = self.pad() + 7.0 * self.cell();
        let y = self.pad() + (self.header_h() - b) / 2.0;
        match h {
            Hit::Next => RectF::new(right - b, y, b, b),
            Hit::Prev => RectF::new(right - 2.0 * b - self.s(2.0), y, b, b),
            Hit::Title => RectF::new(self.pad(), self.pad(), 4.0 * self.cell(), self.header_h()),
        }
    }

    fn hit(&self, p: PointF) -> Option<Hit> {
        [Hit::Prev, Hit::Next, Hit::Title].into_iter().find(|&h| self.button(h).contains(p))
    }

    fn rebuild(&mut self) {
        let name = (self.month_name)(self.shown);
        let prev = self.prev.map(|m| (self.month_name)(m));
        let Some(t) = self.text.as_mut() else { return };
        self.title = t.cache.layout(&name, &t.title, 400.0).ok();
        self.prev_title = prev.and_then(|n| t.cache.layout(&n, &t.title, 400.0).ok());
    }

    fn go(&mut self, to: Month, now: u64) -> PanelReply {
        if to == self.shown {
            return PanelReply::default();
        }
        self.dir = if to > self.shown { 1.0 } else { -1.0 };
        self.prev = Some(self.shown);
        self.shown = to;
        self.leaf = Tween::at_rest(0.0);
        self.leaf.to(1.0, now, LEAF_MS);
        self.rebuild();
        PanelReply { repaint: true, ..Default::default() }
    }

    fn today_month(&self) -> Month {
        Month { year: self.today.0, month: self.today.1 }
    }

    fn set_hover(&mut self, h: Option<Hit>, now: u64) -> PanelReply {
        if h == self.hover {
            return PanelReply::default();
        }
        self.hover_o = Tween::at_rest(0.0);
        if h.is_some() {
            self.hover_o.to(1.0, now, FADE_MS);
        }
        self.hover = h;
        PanelReply { repaint: true, ..Default::default() }
    }

    fn paint_month(&self, p: &mut Painter, m: Month, dx: f32, alpha: f32) {
        let top = self.pad() + self.header_h() + self.weekdays_h();
        let cell = self.cell();
        let mark = self.s(30.0);
        for (i, c) in grid(m, self.first_weekday).iter().enumerate() {
            let (col, row) = ((i % 7) as f32, (i / 7) as f32);
            let center = PointF { x: self.pad() + col * cell + cell / 2.0 + dx, y: top + row * cell + cell / 2.0 };
            let today = c.this_month && (m.year, m.month, c.day) == self.today;
            let list = if today { &self.days_strong } else { &self.days };
            let Some(t) = list.get(c.day as usize - 1) else { continue };
            let color = if today {
                // Today: a white disc with the number cut out of it in black.
                p.fill_round_rect(RectF::new(center.x - mark / 2.0, center.y - mark / 2.0, mark, mark), mark / 2.0, theme::TEXT_STRONG.with_alpha(alpha));
                ON_WHITE.with_alpha(alpha)
            } else if c.this_month {
                theme::TEXT.with_alpha(alpha)
            } else {
                theme::TEXT_DISABLED.with_alpha(alpha * 0.8)
            };
            p.text(t, PointF { x: (center.x - t.width() / 2.0).round(), y: (center.y - t.height() / 2.0).round() }, color);
        }
    }
}

impl PanelContent for CalendarPanel {
    fn size(&self) -> (f32, f32) {
        let w = 2.0 * self.pad() + 7.0 * self.cell();
        (w, self.pad() + self.header_h() + self.weekdays_h() + 6.0 * self.cell() + self.pad() - self.s(4.0))
    }

    fn paint(&mut self, p: &mut Painter, now: u64) {
        let f = self.leaf.value(now);
        let shift = self.s(24.0);
        // Title: the new month's name slides in with its grid.
        let ty = |t: &TextLayout| (self.pad() + (self.header_h() - t.height()) / 2.0).round();
        let lit = self.hover == Some(Hit::Title) && self.shown != self.today_month();
        if let Some(t) = self.prev_title.as_ref().filter(|_| f < 1.0) {
            p.text(t, PointF { x: (self.pad() + self.s(4.0) - self.dir * shift * f).round(), y: ty(t) }, theme::TEXT_STRONG.with_alpha(1.0 - f));
        }
        if let Some(t) = &self.title {
            let c = if lit { theme::TEXT_STRONG } else { theme::TEXT };
            p.text(t, PointF { x: (self.pad() + self.s(4.0) + self.dir * shift * (1.0 - f)).round(), y: ty(t) }, c.with_alpha(f));
        }
        // ‹ ›, with a round hover.
        let o = self.hover_o.value(now);
        for h in [Hit::Prev, Hit::Next] {
            let r = self.button(h);
            if self.hover == Some(h) && o > 0.0 {
                p.fill_round_rect(r, r.w / 2.0, theme::POPUP_HOVER.with_alpha(theme::POPUP_HOVER.a * o));
            }
            let c = PointF { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0 };
            let color = if self.hover == Some(h) { theme::TEXT_STRONG } else { theme::TEXT_INACTIVE };
            let turn = if h == Hit::Prev { -0.5 } else { 0.5 };
            icons::chevron(p, c, self.s(14.0), turn, color, self.s(theme::ICON_STROKE).max(1.0));
        }
        // Weekday names.
        let wy = self.pad() + self.header_h();
        for (i, t) in self.weekdays.iter().enumerate() {
            let cx = self.pad() + i as f32 * self.cell() + self.cell() / 2.0;
            p.text(t, PointF { x: (cx - t.width() / 2.0).round(), y: (wy + (self.weekdays_h() - t.height()) / 2.0).round() }, theme::TEXT_INACTIVE);
        }
        // The grids: the old one slides out, the new one in.
        if let Some(m) = self.prev.filter(|_| f < 1.0) {
            self.paint_month(p, m, -self.dir * shift * f, 1.0 - f);
        }
        self.paint_month(p, self.shown, self.dir * shift * (1.0 - f), f);
    }

    fn input(&mut self, ev: PanelInput, now: u64) -> PanelReply {
        match ev {
            PanelInput::Move(p) => self.set_hover(self.hit(p), now),
            PanelInput::Leave => self.set_hover(None, now),
            PanelInput::Release(p) => match self.hit(p) {
                Some(Hit::Prev) => self.go(self.shown.prev(), now),
                Some(Hit::Next) => self.go(self.shown.next(), now),
                Some(Hit::Title) => self.go(self.today_month(), now),
                None => PanelReply::default(),
            },
            // The wheel leafs: up for earlier, down for later.
            PanelInput::Wheel(d) if d > 0 => self.go(self.shown.prev(), now),
            PanelInput::Wheel(d) if d < 0 => self.go(self.shown.next(), now),
            PanelInput::Key(vk) if vk == VK_LEFT.0 || vk == VK_PRIOR.0 => self.go(self.shown.prev(), now),
            PanelInput::Key(vk) if vk == VK_RIGHT.0 || vk == VK_NEXT.0 => self.go(self.shown.next(), now),
            PanelInput::Key(vk) if vk == VK_HOME.0 => self.go(self.today_month(), now),
            _ => PanelReply::default(),
        }
    }

    fn animating(&self, now: u64) -> bool {
        self.leaf.running(now) || self.hover_o.running(now)
    }
}
