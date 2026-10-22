//! The tray panel's content (spec: M2 §3.1): NewWin first, then background apps as text rows,
//! filtered by typing.

use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::tray::match_range;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DOWN, VK_RETURN, VK_UP};

use crate::panel::{PanelContent, PanelInput, PanelReply};
use crate::popup::PopupStyle;
use crate::{Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, theme};

const MAX_ROWS: usize = 14;
const SCROLL_MS: u32 = 120;
const FADE_MS: u32 = 120;

#[derive(Clone, Debug, PartialEq)]
pub struct TrayRow {
    pub label: String,
    /// The NewWin row: always shown, bold, followed by a separator.
    pub header: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayOp {
    Left = 0,
    Right = 1,
    Key = 2,
}

/// Panel action id for `row` (an index into the rows given to the list).
pub fn action_id(row: usize, op: TrayOp) -> u32 {
    ((row as u32) << 2) | op as u32
}

pub fn decode_action(id: u32) -> (usize, TrayOp) {
    let op = match id & 3 {
        1 => TrayOp::Right,
        2 => TrayOp::Key,
        _ => TrayOp::Left,
    };
    ((id >> 2) as usize, op)
}

struct Text {
    cache: TextCache,
    regular: TextStyle,
    bold: TextStyle,
}

pub struct TrayList {
    rows: Vec<TrayRow>,
    query: String,
    empty_text: String,
    dpi: u32,
    width: f32,
    text: Option<Text>,
    /// Text of each visible entry.
    layouts: Vec<Option<TextLayout>>,
    empty: Option<TextLayout>,
    /// Index into `visible()`.
    hover: Option<usize>,
    pill_y: Tween,
    pill_o: Tween,
    scroll: Tween,
}

impl TrayList {
    pub fn new(rows: Vec<TrayRow>, empty_text: String, style: &PopupStyle, gfx: &Rc<Gfx>) -> TrayList {
        let regular = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let bold = TextStyle { weight: theme::WEIGHT_SEMIBOLD, ..regular.clone() };
        let text = Text { cache: TextCache::new(gfx), regular, bold };
        let mut l = Self::build(rows, empty_text, style.dpi, Some(text));
        l.rebuild();
        l
    }

    /// Without text layout (geometry and input only).
    pub fn for_test(rows: Vec<TrayRow>) -> TrayList {
        Self::build(rows, String::new(), 96, None)
    }

    fn build(rows: Vec<TrayRow>, empty_text: String, dpi: u32, text: Option<Text>) -> TrayList {
        TrayList {
            rows,
            query: String::new(),
            empty_text,
            dpi,
            width: (220.0 * dpi as f32 / 96.0).round(),
            text,
            layouts: Vec::new(),
            empty: None,
            hover: None,
            pill_y: Tween::at_rest(0.0),
            pill_o: Tween::at_rest(0.0),
            scroll: Tween::at_rest(0.0),
        }
    }

    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    /// New rows from the tray model; false (and nothing redone) when they did not change. The
    /// selection stays on the same row (by its label) if that row is still there.
    pub fn set_rows(&mut self, rows: Vec<TrayRow>) -> bool {
        if rows == self.rows {
            return false;
        }
        let selected = self.hover.and_then(|k| self.visible().get(k).map(|&(i, _)| self.rows[i].clone()));
        self.rows = rows;
        self.scroll = Tween::at_rest(self.scroll.target().min(self.max_scroll()));
        self.hover = selected.and_then(|row| self.visible().iter().position(|&(i, _)| self.rows[i] == row));
        match self.hover {
            Some(k) => self.pill_y = Tween::at_rest(self.rect(k, self.scroll.target()).y),
            None => self.pill_o = Tween::at_rest(0.0),
        }
        self.rebuild();
        true
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Shown entries: (index into the rows, emphasised UTF-16 range). The header always shows.
    pub fn visible(&self) -> Vec<(usize, Option<(usize, usize)>)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if r.header || self.query.is_empty() {
                    Some((i, None))
                } else {
                    match_range(&r.label, &self.query).map(|m| (i, Some(m)))
                }
            })
            .collect()
    }

    fn apps_shown(&self) -> usize {
        self.visible().len().saturating_sub(1)
    }

    fn top(&self) -> f32 {
        self.s(theme::POPUP_PAD) + self.s(theme::POPUP_ITEM_HEIGHT) + self.s(9.0)
    }

    fn max_scroll(&self) -> f32 {
        (self.apps_shown().saturating_sub(MAX_ROWS)) as f32 * self.s(theme::POPUP_ITEM_HEIGHT)
    }

    /// Panel-local rectangle of visible entry `k` at scroll offset `scroll`.
    fn rect(&self, k: usize, scroll: f32) -> RectF {
        let (pad, ih) = (self.s(theme::POPUP_PAD), self.s(theme::POPUP_ITEM_HEIGHT));
        let y = if k == 0 { pad } else { self.top() + (k - 1) as f32 * ih - scroll };
        RectF::new(pad, y, self.width - 2.0 * pad, ih)
    }

    /// Centre of visible entry `k` (panel-local px).
    pub fn row_center(&self, k: usize) -> PointF {
        let r = self.rect(k, self.scroll.target());
        PointF { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0 }
    }

    fn hit(&self, p: PointF) -> Option<usize> {
        let head = self.rect(0, 0.0);
        if head.contains(p) {
            return Some(0);
        }
        let ih = self.s(theme::POPUP_ITEM_HEIGHT);
        let view = self.apps_shown().min(MAX_ROWS) as f32 * ih;
        if p.y < self.top() || p.y >= self.top() + view || p.x < head.x || p.x >= head.right() {
            return None;
        }
        let k = ((p.y - self.top() + self.scroll.target()) / ih).floor() as usize + 1;
        (k < self.visible().len()).then_some(k)
    }

    /// Re-lays the text after the rows or the query changed.
    fn rebuild(&mut self) {
        let visible = self.visible();
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let frame = 2.0 * self.s(theme::POPUP_PAD) + 2.0 * pad_x;
        let (min_w, max_w) = (self.s(220.0), self.s(360.0));
        let Some(t) = self.text.as_mut() else { return };
        let widest = self
            .rows
            .iter()
            .map(|r| t.cache.natural_width(&r.label, if r.header { &t.bold } else { &t.regular }).unwrap_or(0.0))
            .fold(0.0, f32::max);
        self.width = (widest + frame).clamp(min_w, max_w);
        let room = self.width - frame;
        self.layouts = visible
            .iter()
            .map(|&(i, m)| {
                let r = &self.rows[i];
                t.cache.layout_emphasis(&r.label, if r.header { &t.bold } else { &t.regular }, room, m).ok()
            })
            .collect();
        self.empty = t.cache.layout(&self.empty_text, &t.regular, room).ok();
    }

    fn set_hover(&mut self, k: Option<usize>, now: u64) {
        if k == self.hover {
            return;
        }
        if let Some(k) = k {
            let y = self.rect(k, self.scroll.target()).y;
            if self.pill_o.value(now) < 0.05 {
                self.pill_y = Tween::at_rest(y);
            } else {
                self.pill_y.to(y, now, theme::HOVER_GLIDE_MS);
            }
        }
        self.pill_o.to(if k.is_some() { 1.0 } else { 0.0 }, now, FADE_MS);
        self.hover = k;
    }

    fn scroll_to(&mut self, target: f32, now: u64) {
        let target = target.clamp(0.0, self.max_scroll());
        self.scroll.to(target, now, SCROLL_MS);
        if let Some(k) = self.hover {
            self.pill_y = Tween::at_rest(self.rect(k, target).y);
        }
    }

    /// Keeps keyboard-selected entry `k` inside the scrolled view.
    fn reveal(&mut self, k: usize, now: u64) {
        if k == 0 {
            return;
        }
        let ih = self.s(theme::POPUP_ITEM_HEIGHT);
        let at = (k - 1) as f32 * ih;
        let s = self.scroll.target();
        if at < s {
            self.scroll_to(at, now);
        } else if at + ih > s + MAX_ROWS as f32 * ih {
            self.scroll_to(at + ih - MAX_ROWS as f32 * ih, now);
        }
    }

    fn query_changed(&mut self, now: u64) -> PanelReply {
        self.scroll = Tween::at_rest(0.0);
        self.set_hover(None, now);
        self.rebuild();
        PanelReply { repaint: true, resize: true, ..Default::default() }
    }

    fn act(&self, k: usize, op: TrayOp, close: bool) -> PanelReply {
        let row = self.visible().get(k).map(|v| v.0);
        PanelReply { action: row.map(|r| action_id(r, op)), close: close && row.is_some(), ..Default::default() }
    }
}

impl PanelContent for TrayList {
    fn size(&self) -> (f32, f32) {
        let rows = self.apps_shown().clamp(1, MAX_ROWS) as f32;
        (self.width, self.top() + rows * self.s(theme::POPUP_ITEM_HEIGHT) + self.s(theme::POPUP_PAD))
    }

    fn paint(&mut self, p: &mut Painter, now: u64) {
        let scroll = self.scroll.value(now);
        let ih = self.s(theme::POPUP_ITEM_HEIGHT);
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let view_bottom = self.top() + self.apps_shown().min(MAX_ROWS) as f32 * ih;
        let base = self.rect(0, 0.0);
        let o = self.pill_o.value(now);
        if o > 0.0 && self.hover.is_some() {
            let pill = RectF::new(base.x, self.pill_y.value(now), base.w, ih);
            p.fill_round_rect(pill, self.s(theme::POPUP_ITEM_RADIUS), theme::POPUP_HOVER.with_alpha(theme::POPUP_HOVER.a * o));
        }
        let sep_y = (self.top() - self.s(9.0) / 2.0).round() + 0.5;
        p.line(PointF { x: base.x + pad_x, y: sep_y }, PointF { x: base.right() - pad_x, y: sep_y }, theme::SEPARATOR, 1.0);
        let more_below = scroll + 0.5 < self.max_scroll();
        for (k, layout) in self.layouts.iter().enumerate() {
            let Some(t) = layout else { continue };
            let r = self.rect(k, scroll);
            if k > 0 && (r.y < self.top() - 0.5 || r.bottom() > view_bottom + 0.5) {
                continue;
            }
            let mut color = if k == 0 || self.hover == Some(k) { theme::TEXT_STRONG } else { theme::TEXT };
            // The last row before hidden ones fades: there is more to scroll to.
            if k > 0 && more_below && r.bottom() + ih > view_bottom + 0.5 {
                color = color.with_alpha(0.45);
            }
            p.text(t, PointF { x: r.x + pad_x, y: (r.y + (r.h - t.height()) / 2.0).round() }, color);
        }
        if self.apps_shown() == 0
            && let Some(t) = &self.empty
        {
            let r = self.rect(1, 0.0);
            p.text(t, PointF { x: r.x + pad_x, y: (r.y + (r.h - t.height()) / 2.0).round() }, theme::TEXT_INACTIVE);
        }
    }

    fn input(&mut self, ev: PanelInput, now: u64) -> PanelReply {
        let repaint = PanelReply { repaint: true, ..Default::default() };
        match ev {
            PanelInput::Move(p) => {
                self.set_hover(self.hit(p), now);
                repaint
            }
            PanelInput::Leave => {
                self.set_hover(None, now);
                repaint
            }
            PanelInput::Release(p) => self.hit(p).map_or_else(PanelReply::default, |k| self.act(k, TrayOp::Left, true)),
            PanelInput::RightRelease(p) => self.hit(p).map_or_else(PanelReply::default, |k| self.act(k, TrayOp::Right, false)),
            PanelInput::Wheel(delta) => {
                let rows = -f32::from(delta) / 120.0 * 3.0;
                self.scroll_to(self.scroll.target() + rows * self.s(theme::POPUP_ITEM_HEIGHT), now);
                repaint
            }
            PanelInput::Key(vk) if vk == VK_UP.0 || vk == VK_DOWN.0 => {
                let n = self.visible().len();
                let k = match (self.hover, vk == VK_DOWN.0) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(k), true) => (k + 1) % n,
                    (Some(k), false) => (k + n - 1) % n,
                };
                self.reveal(k, now);
                self.set_hover(Some(k), now);
                repaint
            }
            PanelInput::Key(vk) if vk == VK_RETURN.0 => self.act(self.hover.unwrap_or(1), TrayOp::Key, true),
            PanelInput::Key(vk) if vk == VK_BACK.0 => {
                if self.query.pop().is_some() {
                    self.query_changed(now)
                } else {
                    PanelReply::default()
                }
            }
            PanelInput::Char(c) => {
                self.query.push(c);
                self.query_changed(now)
            }
            PanelInput::Press(_) | PanelInput::Key(_) => PanelReply::default(),
        }
    }

    fn animating(&self, now: u64) -> bool {
        self.pill_y.running(now) || self.pill_o.running(now) || self.scroll.running(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::{PanelContent, PanelInput};
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_RETURN};

    fn list() -> TrayList {
        TrayList::for_test(vec![
            TrayRow { label: "NewWin".into(), header: true },
            TrayRow { label: "Telegram".into(), header: false },
            TrayRow { label: "Steam".into(), header: false },
        ])
    }

    #[test]
    fn action_ids_roundtrip() {
        assert_eq!(decode_action(action_id(5, TrayOp::Right)), (5, TrayOp::Right));
        assert_eq!(decode_action(action_id(0, TrayOp::Left)), (0, TrayOp::Left));
    }

    #[test]
    fn typing_filters_apps_but_keeps_header() {
        let mut l = list();
        assert_eq!(l.visible(), vec![(0, None), (1, None), (2, None)]);
        for c in ['t', 'e', 'l'] {
            l.input(PanelInput::Char(c), 0);
        }
        assert_eq!(l.query(), "tel");
        assert_eq!(l.visible(), vec![(0, None), (1, Some((0, 3)))]);
        for _ in 0..3 {
            l.input(PanelInput::Key(VK_BACK.0), 0);
        }
        assert_eq!(l.visible().len(), 3);
    }

    #[test]
    fn enter_clicks_first_visible_app() {
        let mut l = list();
        l.input(PanelInput::Char('s'), 0);
        l.input(PanelInput::Char('t'), 0);
        let r = l.input(PanelInput::Key(VK_RETURN.0), 0);
        assert_eq!(r.action, Some(action_id(2, TrayOp::Key)));
        assert!(r.close);
    }

    #[test]
    fn right_release_does_not_close() {
        let mut l = list();
        let p = l.row_center(1);
        let r = l.input(PanelInput::RightRelease(p), 0);
        assert_eq!(r.action, Some(action_id(1, TrayOp::Right)));
        assert!(!r.close);
        let r = l.input(PanelInput::Release(l.row_center(2)), 0);
        assert_eq!((r.action, r.close), (Some(action_id(2, TrayOp::Left)), true));
    }

    #[test]
    fn row_updates_keep_the_selection() {
        let mut l = list();
        l.input(PanelInput::Key(windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0), 0);
        l.input(PanelInput::Key(windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0), 0);
        l.input(PanelInput::Key(windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0), 0);
        assert!(!l.set_rows(l.rows.clone()), "same rows: nothing to do");
        // An app above the selection goes away; the selection stays on Steam.
        assert!(l.set_rows(vec![TrayRow { label: "NewWin".into(), header: true }, TrayRow { label: "Steam".into(), header: false }]));
        let r = l.input(PanelInput::Key(VK_RETURN.0), 0);
        assert_eq!(r.action, Some(action_id(1, TrayOp::Key)), "Enter still picks Steam");
    }

    #[test]
    fn size_grows_with_rows_up_to_fourteen() {
        let mut rows = vec![TrayRow { label: "NewWin".into(), header: true }];
        let (_, h1) = TrayList::for_test(rows.clone()).size();
        rows.extend((0..30).map(|i| TrayRow { label: format!("App {i}"), header: false }));
        let (_, h30) = TrayList::for_test(rows.clone()).size();
        rows.truncate(15);
        let (_, h14) = TrayList::for_test(rows).size();
        assert!(h14 > h1);
        assert_eq!(h30, h14, "at most 14 app rows show; the rest scroll");
    }
}
