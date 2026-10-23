//! A list whose rows are dragged into a new order (the bar's right-hand items): each row has a
//! grip, a label, an optional dim detail and an on/off toggle. Geometry and drag state are pure;
//! the other rows glide aside over 150 ms while one is dragged.

use newwin_core::anim::Tween;
use newwin_core::items::{drop_index, reorder};
use newwin_platform::dpi::scale;

use crate::toggle::Toggle;
use crate::{Color, Painter, PointF, RectF, TextLayout, theme};

const ROW_BG: Color = Color::rgba(255, 255, 255, 0.07);
const GRIP: Color = Color::rgba(255, 255, 255, 0.35);
const GRIP_W: f32 = 28.0;
const TOGGLE_PAD: f32 = 12.0;

#[derive(Clone, Debug)]
pub struct ReorderRow {
    pub label: String,
    pub detail: Option<String>,
    pub on: Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub from: usize,
    pub to: usize,
    /// The pointer's distance below the top of the grabbed row.
    pub grab_dy: f32,
    /// The pointer's latest y.
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct ReorderList {
    pub rows: Vec<ReorderRow>,
    pub drag: Option<Drag>,
    origin: PointF,
    width: f32,
    row_h: f32,
    dpi: u32,
    /// Per row: how far it is shifted from its slot, in rows.
    shift: Vec<Tween>,
    /// Where the last released row was dropped, relative to its new slot, in rows.
    dropped_offset: Option<f32>,
}

impl ReorderList {
    pub fn new(rows: Vec<ReorderRow>) -> ReorderList {
        let shift = vec![Tween::at_rest(0.0); rows.len()];
        ReorderList { rows, drag: None, origin: PointF::default(), width: 0.0, row_h: 0.0, dpi: 96, shift, dropped_offset: None }
    }

    /// Where the list sits: top-left `origin`, `width` and `row_h` in physical px.
    pub fn set_layout(&mut self, origin: PointF, width: f32, row_h: f32, dpi: u32) {
        self.origin = origin;
        self.width = width.max(0.0);
        self.row_h = row_h.max(0.0);
        self.dpi = dpi;
    }

    pub fn row_h(&self) -> f32 {
        self.row_h
    }

    pub fn row_top(&self, i: usize) -> f32 {
        self.origin.y + self.row_h * i as f32
    }

    pub fn row_rect(&self, i: usize) -> RectF {
        RectF::new(self.origin.x, self.row_top(i), self.width, self.row_h)
    }

    pub fn height(&self) -> f32 {
        self.row_h * self.rows.len() as f32
    }

    /// The toggle's rectangle in row `i`, at the row's right end.
    pub fn toggle_rect(&self, i: usize) -> RectF {
        let r = self.row_rect(i);
        let t = Toggle::rect(PointF { x: 0.0, y: 0.0 }, self.dpi);
        RectF::new(r.right() - scale(TOGGLE_PAD, self.dpi) - t.w, r.y + (r.h - t.h) / 2.0, t.w, t.h)
    }

    pub fn row_at(&self, p: PointF) -> Option<usize> {
        if !p.x.is_finite() || !p.y.is_finite() || self.row_h <= 0.0 {
            return None;
        }
        let area = RectF::new(self.origin.x, self.origin.y, self.width, self.height());
        if !area.contains(p) {
            return None;
        }
        let i = ((p.y - self.origin.y) / self.row_h) as usize;
        (i < self.rows.len()).then_some(i)
    }

    /// The row whose toggle is under `p`.
    pub fn toggle_hit(&self, p: PointF) -> Option<usize> {
        self.row_at(p).filter(|&i| self.toggle_rect(i).contains(p))
    }

    /// Starts dragging the row under `p`; false when there is none or `p` is on a toggle.
    pub fn press(&mut self, p: PointF) -> bool {
        self.drag = None;
        let Some(i) = self.row_at(p) else { return false };
        if self.toggle_rect(i).contains(p) {
            return false;
        }
        self.drag = Some(Drag { from: i, to: i, grab_dy: p.y - self.row_top(i), y: p.y });
        true
    }

    /// Moves the grabbed row with the pointer; the slot it would drop into follows its centre.
    pub fn drag_to(&mut self, p: PointF) {
        let Some(mut d) = self.drag else { return };
        if !p.y.is_finite() {
            return;
        }
        let h = self.row_h;
        let tops: Vec<f32> = (0..self.rows.len()).map(|i| self.row_top(i)).collect();
        let centre = p.y - d.grab_dy + h / 2.0;
        // Moving down, the row's own midpoint is above its centre and must not count.
        let probe = if centre > self.row_top(d.from) + h / 2.0 { centre - h } else { centre };
        d.to = drop_index(&tops, h, probe).min(self.rows.len().saturating_sub(1));
        d.y = p.y;
        self.drag = Some(d);
    }

    /// Ends the drag: `Some((from, to))` when the row landed in another slot. The caller then
    /// stores the new order and calls [`ReorderList::commit`].
    pub fn release(&mut self) -> Option<(usize, usize)> {
        let d = self.drag.take()?;
        self.dropped_offset = (self.row_h > 0.0).then(|| (d.y - d.grab_dy - self.row_top(d.from)) / self.row_h - (d.to as f32 - d.from as f32));
        (d.from != d.to).then_some((d.from, d.to))
    }

    /// Applies a reorder to the rows and glides the moved row from where it was dropped.
    /// Call it with the pair `release` returned, before touching the pointer again.
    pub fn commit(&mut self, from: usize, to: usize, now_ms: u64) {
        if from >= self.rows.len() || to >= self.rows.len() {
            return;
        }
        reorder(&mut self.rows, from, to);
        self.shift = vec![Tween::at_rest(0.0); self.rows.len()];
        if let (Some(t), Some(dy)) = (self.shift.get_mut(to), self.dropped_offset.take()) {
            *t = Tween::at_rest(dy);
            t.to(0.0, now_ms, theme::REORDER_MS);
        }
    }

    /// Points the other rows at their shifted slots for the current drag; call after `drag_to`.
    pub fn glide(&mut self, now_ms: u64) {
        let d = self.drag;
        self.shift.resize(self.rows.len(), Tween::at_rest(0.0));
        for (i, t) in self.shift.iter_mut().enumerate() {
            let target = match d {
                Some(d) if d.from < d.to && i > d.from && i <= d.to => -1.0,
                Some(d) if d.to < d.from && i >= d.to && i < d.from => 1.0,
                _ => 0.0,
            };
            t.to(target, now_ms, theme::REORDER_MS);
        }
    }

    pub fn animating(&self, now_ms: u64) -> bool {
        self.shift.iter().any(|t| t.running(now_ms))
    }

    /// `texts[i]` is row `i`'s laid-out label and optional detail.
    pub fn paint(&self, p: &mut Painter, texts: &[(&TextLayout, Option<&TextLayout>)], now_ms: u64) {
        let dragged = self.drag.map(|d| d.from);
        for i in 0..self.rows.len() {
            if Some(i) != dragged {
                let y = self.row_top(i) + self.shift.get(i).map_or(0.0, |t| t.value(now_ms)) * self.row_h;
                self.paint_row(p, i, y, texts, now_ms, false);
            }
        }
        if let Some(d) = self.drag {
            let lo = self.origin.y;
            let hi = (self.origin.y + self.height() - self.row_h).max(lo);
            self.paint_row(p, d.from, (d.y - d.grab_dy).clamp(lo, hi), texts, now_ms, true);
        }
    }

    fn paint_row(&self, p: &mut Painter, i: usize, y: f32, texts: &[(&TextLayout, Option<&TextLayout>)], now_ms: u64, lifted: bool) {
        let Some(row) = self.rows.get(i) else { return };
        let r = RectF::new(self.origin.x, y, self.width, self.row_h);
        let radius = scale(theme::POPUP_ITEM_RADIUS, self.dpi);
        p.fill_round_rect(r.inset(self.row_h * 0.04), radius, if lifted { theme::ACTIVE_PILL } else { ROW_BG });
        let gx = r.x + scale(GRIP_W, self.dpi) / 2.0;
        for k in [-1.0f32, 0.0, 1.0] {
            let ly = (r.y + r.h / 2.0 + k * scale(4.0, self.dpi)).round() + 0.5;
            p.line(PointF { x: gx - scale(5.0, self.dpi), y: ly }, PointF { x: gx + scale(5.0, self.dpi), y: ly }, GRIP, 1.0);
        }
        let mut x = r.x + scale(GRIP_W, self.dpi);
        if let Some((label, detail)) = texts.get(i) {
            p.text(label, PointF { x, y: (r.y + (r.h - label.height()) / 2.0).round() }, theme::TEXT_STRONG);
            x += label.width() + scale(10.0, self.dpi);
            if let (Some(d), Some(_)) = (detail, &row.detail) {
                p.text(d, PointF { x, y: (r.y + (r.h - d.height()) / 2.0).round() }, theme::TEXT_INACTIVE);
            }
        }
        let t = self.toggle_rect(i);
        row.on.paint(p, RectF::new(t.x, y + (r.h - t.h) / 2.0, t.w, t.h), now_ms);
    }
}
