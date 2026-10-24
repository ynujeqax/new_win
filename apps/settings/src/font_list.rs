//! The font list (spec: M4 §3.2): opened from the *Font* row, drawn over the window in the
//! popups' glass style, each family in its own face, filtered by typing like the tray. Up/Down
//! move, Enter picks (the first match when nothing is selected), Esc (the window) closes, a click
//! outside closes, the wheel and the thumb scroll.

use std::collections::HashMap;
use std::rc::Rc;

use newwin_core::tray::match_range;
use newwin_platform::dpi::scale;
use newwin_ui::panel::PanelInput;
use newwin_ui::scroll::Scroll;
use newwin_ui::{Painter, PointF, RectF, TextCache, TextLayout, TextStyle, paint_glass, theme};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DOWN, VK_RETURN, VK_UP};

const MAX_ROWS: usize = 10;
const MIN_W: f32 = 260.0;
/// Between the font row and the list, and the list and the window edge, logical px.
const GAP: f32 = 4.0;
const EDGE: f32 = 8.0;
const ROW_TEXT: f32 = 14.0;
/// When a family cannot be laid out in itself.
const FALLBACK: &str = "Segoe UI Variable Text";

/// What the window does after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListReply {
    Ignored,
    Repaint,
    Pick(String),
    Close,
}

pub struct FontList {
    families: Rc<Vec<String>>,
    current: String,
    query: String,
    /// Index into [`FontList::visible`].
    hover: Option<usize>,
    scroll: Scroll,
    /// Dragging the thumb, grabbed this far below its top.
    drag: Option<f32>,
    dpi: u32,
    /// Laid-out rows by family index; cleared when the query changes.
    layouts: HashMap<usize, Option<TextLayout>>,
}

impl FontList {
    pub fn new(families: Rc<Vec<String>>, current: &str, dpi: u32) -> FontList {
        let mut l = FontList {
            families,
            current: current.to_owned(),
            query: String::new(),
            hover: None,
            scroll: Scroll::default(),
            drag: None,
            dpi,
            layouts: HashMap::new(),
        };
        l.hover = l.visible().iter().position(|&(i, _)| l.families.get(i).is_some_and(|f| f.eq_ignore_ascii_case(current)));
        l.sync_scroll();
        if let Some(k) = l.hover {
            // Open with the current family in the middle of the view.
            l.scroll.offset = (k as f32 - (MAX_ROWS / 2) as f32) * l.item_h();
            l.scroll.clamp();
        }
        l
    }

    fn s(&self, v: f32) -> f32 {
        scale(v, self.dpi).round()
    }

    fn item_h(&self) -> f32 {
        self.s(theme::POPUP_ITEM_HEIGHT)
    }

    /// Shown families: (index into the families, emphasised UTF-16 range of the match).
    pub fn visible(&self) -> Vec<(usize, Option<(usize, usize)>)> {
        self.families
            .iter()
            .enumerate()
            .filter_map(|(i, f)| if self.query.is_empty() { Some((i, None)) } else { match_range(f, &self.query).map(|m| (i, Some(m))) })
            .collect()
    }

    #[cfg(test)]
    pub fn hovered(&self) -> Option<&str> {
        let (i, _) = *self.visible().get(self.hover?)?;
        self.families.get(i).map(String::as_str)
    }

    fn rows_shown(&self) -> usize {
        self.visible().len().clamp(1, MAX_ROWS)
    }

    fn sync_scroll(&mut self) {
        self.scroll.content_h = self.visible().len() as f32 * self.item_h();
        self.scroll.view_h = self.rows_shown() as f32 * self.item_h();
        self.scroll.clamp();
    }

    /// The list's rectangle under (or, without room, over) the font row `anchor`, right-aligned
    /// with it, inside `window`; physical px.
    pub fn rect_at(&self, anchor: RectF, window: RectF) -> RectF {
        let (gap, edge, pad) = (self.s(GAP), self.s(EDGE), self.s(theme::POPUP_PAD));
        let w = anchor.w.max(self.s(MIN_W)).min(window.w - 2.0 * edge);
        let h = self.rows_shown() as f32 * self.item_h() + 2.0 * pad;
        let x = (anchor.right() - w).max(window.x + edge);
        let (below, above) = (anchor.bottom() + gap, anchor.y - gap - h);
        let y = if below + h <= window.bottom() - edge {
            below
        } else if above >= window.y + edge {
            above
        } else {
            (window.bottom() - edge - h).max(window.y)
        };
        RectF::new(x, y, w, h)
    }

    /// The scrolled area inside `rect`.
    fn view(&self, rect: RectF) -> RectF {
        rect.inset(self.s(theme::POPUP_PAD))
    }

    /// Visible row `k` inside the list at `rect`, at the current scroll.
    pub fn row_rect(&self, rect: RectF, k: usize) -> RectF {
        let v = self.view(rect);
        let ih = self.item_h();
        RectF::new(v.x, v.y + k as f32 * ih - self.scroll.offset, v.w, ih)
    }

    fn hit(&self, rect: RectF, p: PointF) -> Option<usize> {
        let v = self.view(rect);
        if !v.contains(p) {
            return None;
        }
        let k = ((p.y - v.y + self.scroll.offset) / self.item_h()).floor();
        (k >= 0.0 && (k as usize) < self.visible().len()).then_some(k as usize)
    }

    /// Scrolls so that row `k` is in view.
    fn reveal(&mut self, k: usize) {
        let ih = self.item_h();
        let (top, view) = (k as f32 * ih, self.scroll.view_h);
        if top < self.scroll.offset {
            self.scroll.offset = top;
        } else if top + ih > self.scroll.offset + view {
            self.scroll.offset = top + ih - view;
        }
        self.scroll.clamp();
    }

    fn query_changed(&mut self) -> ListReply {
        self.hover = None;
        self.layouts.clear();
        self.scroll.offset = 0.0;
        self.sync_scroll();
        ListReply::Repaint
    }

    fn pick(&self, k: usize) -> ListReply {
        match self.visible().get(k).and_then(|&(i, _)| self.families.get(i)) {
            Some(f) => ListReply::Pick(f.clone()),
            None => ListReply::Ignored,
        }
    }

    fn set_hover(&mut self, k: Option<usize>) -> ListReply {
        if k == self.hover {
            return ListReply::Ignored;
        }
        self.hover = k;
        ListReply::Repaint
    }

    /// `rect` is where the list is (from [`FontList::rect_at`]).
    pub fn input(&mut self, ev: PanelInput, rect: RectF) -> ListReply {
        self.sync_scroll();
        let view = self.view(rect);
        match ev {
            PanelInput::Move(p) => match self.drag {
                Some(grab) => {
                    self.scroll.drag_thumb_dpi(view, p.y, grab, self.dpi);
                    ListReply::Repaint
                }
                None => self.set_hover(self.hit(rect, p)),
            },
            PanelInput::Leave if self.drag.is_none() => self.set_hover(None),
            PanelInput::Press(p) => {
                if !rect.contains(p) {
                    return ListReply::Close;
                }
                if let Some(t) = self.scroll.thumb_rect_dpi(view, self.dpi).filter(|t| t.inset(-self.s(4.0)).contains(p)) {
                    self.drag = Some(p.y - t.y);
                    return ListReply::Repaint;
                }
                self.set_hover(self.hit(rect, p));
                ListReply::Repaint
            }
            PanelInput::Release(p) => {
                if self.drag.take().is_some() {
                    return ListReply::Repaint;
                }
                match self.hit(rect, p) {
                    Some(k) => self.pick(k),
                    None => ListReply::Ignored,
                }
            }
            PanelInput::Wheel(delta) => {
                self.scroll.wheel_dpi(delta, self.dpi);
                ListReply::Repaint
            }
            PanelInput::Key(vk) if vk == VK_UP.0 || vk == VK_DOWN.0 => {
                let n = self.visible().len();
                if n == 0 {
                    return ListReply::Ignored;
                }
                let k = match (self.hover, vk == VK_DOWN.0) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(k), true) => (k + 1) % n,
                    (Some(k), false) => (k + n - 1) % n,
                };
                self.hover = Some(k);
                self.reveal(k);
                ListReply::Repaint
            }
            PanelInput::Key(vk) if vk == VK_RETURN.0 => self.pick(self.hover.unwrap_or(0)),
            PanelInput::Key(vk) if vk == VK_BACK.0 => {
                if self.query.pop().is_some() {
                    self.query_changed()
                } else {
                    ListReply::Ignored
                }
            }
            PanelInput::Char(c) => {
                self.query.push(c);
                self.query_changed()
            }
            _ => ListReply::Ignored,
        }
    }

    /// The list at `rect`; each family is laid out in itself the first time it is shown.
    pub fn paint(&mut self, p: &mut Painter, text: &mut TextCache, rect: RectF) {
        self.sync_scroll();
        paint_glass(p, rect, self.s(theme::POPUP_RADIUS), false);
        let view = self.view(rect);
        let (pad_x, ih, item_r) = (self.s(theme::POPUP_ITEM_PAD_X), self.item_h(), self.s(theme::POPUP_ITEM_RADIUS));
        let room = (view.w - 2.0 * pad_x - self.s(8.0)).max(0.0);
        let size = scale(ROW_TEXT, self.dpi);
        let visible = self.visible();
        let first = (self.scroll.offset / ih).floor().max(0.0) as usize;
        p.push_clip(view);
        for (k, &(i, m)) in visible.iter().enumerate().skip(first).take(MAX_ROWS + 1) {
            let r = self.row_rect(rect, k);
            let Some(name) = self.families.get(i) else { continue };
            let current = name.eq_ignore_ascii_case(&self.current);
            if self.hover == Some(k) {
                p.fill_round_rect(r, item_r, theme::POPUP_HOVER);
            } else if current {
                p.fill_round_rect(r, item_r, theme::ACTIVE_PILL);
            }
            let layout = self.layouts.entry(i).or_insert_with(|| {
                let own = TextStyle { family: name.clone(), size, weight: theme::WEIGHT_REGULAR };
                text.layout_emphasis(name, &own, room, m)
                    .or_else(|_| text.layout_emphasis(name, &TextStyle { family: FALLBACK.into(), ..own }, room, m))
                    .ok()
            });
            if let Some(t) = layout {
                let color = if current || self.hover == Some(k) { theme::TEXT_STRONG } else { theme::TEXT };
                p.text(t, PointF { x: r.x + pad_x, y: (r.y + (r.h - t.height()) / 2.0).round() }, color);
            }
        }
        p.pop_clip();
        self.scroll.paint(p, view, self.dpi);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DOWN, VK_RETURN, VK_UP};

    fn list(current: &str) -> FontList {
        let f: Vec<String> = ["Arial", "Bahnschrift", "Cascadia Code", "Consolas", "Segoe UI"].map(String::from).to_vec();
        FontList::new(Rc::new(f), current, 96)
    }

    fn rect(l: &FontList) -> RectF {
        l.rect_at(RectF::new(400.0, 100.0, 200.0, 26.0), RectF::new(0.0, 0.0, 760.0, 560.0))
    }

    #[test]
    fn opens_on_the_current_family() {
        let l = list("Consolas");
        assert_eq!(l.hovered(), Some("Consolas"));
        assert_eq!(list("Nope").hovered(), None);
    }

    #[test]
    fn typing_filters_and_enter_picks() {
        let mut l = list("Arial");
        let r = rect(&l);
        assert_eq!(l.input(PanelInput::Char('c'), r), ListReply::Repaint);
        assert_eq!(l.input(PanelInput::Char('O'), r), ListReply::Repaint);
        let names: Vec<_> = l.visible().iter().map(|&(i, _)| l.families[i].clone()).collect();
        assert_eq!(names, ["Cascadia Code", "Consolas"]);
        assert_eq!(l.visible()[1].1, Some((0, 2)), "the match is emphasised");
        assert_eq!(l.input(PanelInput::Key(VK_RETURN.0), r), ListReply::Pick("Cascadia Code".into()), "Enter takes the first match");
        l.input(PanelInput::Key(VK_BACK.0), r);
        assert_eq!(l.visible().len(), 3, "Backspace widens the filter (Bahnschrift, Cascadia Code, Consolas)");
    }

    #[test]
    fn arrows_move_and_wrap() {
        let mut l = list("Segoe UI");
        let r = rect(&l);
        l.input(PanelInput::Key(VK_DOWN.0), r);
        assert_eq!(l.hovered(), Some("Arial"));
        l.input(PanelInput::Key(VK_UP.0), r);
        assert_eq!(l.hovered(), Some("Segoe UI"));
        assert_eq!(l.input(PanelInput::Key(VK_RETURN.0), r), ListReply::Pick("Segoe UI".into()));
    }

    #[test]
    fn clicks_pick_a_row_and_outside_closes() {
        let mut l = list("Arial");
        let r = rect(&l);
        let row = l.row_rect(r, 1);
        let mid = PointF { x: row.x + row.w / 2.0, y: row.y + row.h / 2.0 };
        assert_eq!(l.input(PanelInput::Press(mid), r), ListReply::Repaint);
        assert_eq!(l.input(PanelInput::Release(mid), r), ListReply::Pick("Bahnschrift".into()));
        assert_eq!(l.input(PanelInput::Press(PointF { x: 5.0, y: 5.0 }), r), ListReply::Close);
    }

    #[test]
    fn the_list_stays_inside_the_window() {
        let l = list("Arial");
        let win = RectF::new(0.0, 0.0, 760.0, 560.0);
        let low = l.rect_at(RectF::new(400.0, 520.0, 200.0, 26.0), win);
        assert!(low.bottom() <= win.bottom() && low.y >= win.y, "{low:?}");
        let high = l.rect_at(RectF::new(400.0, 100.0, 200.0, 26.0), win);
        assert!(high.y >= 126.0, "opens below the row when it fits: {high:?}");
        assert!(high.right() <= 600.5);
    }
}
