//! The body of Items and Tray (spec: M4 §3.3, §3.5): one [`ReorderList`] whose rows are dragged
//! by their grip and switched by their toggle. Rows are known by their key (item name / exe), so
//! a reload that brings the same rows keeps the list (toggles glide, names update) and a focus
//! follows its row.

use std::collections::HashMap;

use newwin_core::items::reorder;
use newwin_ui::reorder_list::ReorderList;
use newwin_ui::{Painter, PointF, RectF, TextCache, TextLayout, TextStyle, theme};

use super::BodyEvent;
use crate::sections::{List, ListEdit};
use crate::view::{FocusKind, KeyAction};

pub struct SectionList {
    pub list: ReorderList,
    /// Row keys, aligned with `list.rows`.
    pub keys: Vec<String>,
    pub hint: Option<&'static str>,
    /// Laid-out name and detail by key; a reorder needs no new layouts.
    texts: HashMap<String, (TextLayout, Option<TextLayout>)>,
    hint_text: Option<TextLayout>,
    /// The width the texts were laid out for.
    texts_w: f32,
}

impl SectionList {
    pub fn new(l: List) -> SectionList {
        let (keys, rows) = l.rows.into_iter().unzip();
        SectionList { list: ReorderList::new(rows), keys, hint: l.hint, texts: HashMap::new(), hint_text: None, texts_w: 0.0 }
    }

    /// Brings the list to `l`. The same keys in the same order update in place; other rows
    /// replace the list, except during a drag, when the window syncs again after the release.
    pub fn update(&mut self, l: List, now: u64) {
        self.hint = l.hint;
        if l.rows.len() == self.keys.len() && l.rows.iter().zip(&self.keys).all(|((k, _), old)| k == old) {
            for ((key, new), row) in l.rows.into_iter().zip(self.list.rows.iter_mut()) {
                if row.on.on != new.on.on {
                    row.on.set(new.on.on, now);
                }
                if row.label != new.label || row.detail != new.detail {
                    self.texts.remove(&key);
                    row.label = new.label;
                    row.detail = new.detail;
                }
            }
        } else if !self.dragging() {
            // The body lays the new list out right after (`Body::set_list`); names may have
            // changed too, so the texts are laid out again.
            *self = SectionList::new(l);
        }
    }

    /// The DPI changed: lay the texts out again.
    pub fn forget_texts(&mut self) {
        self.texts.clear();
        self.hint_text = None;
    }

    pub fn dragging(&self) -> bool {
        self.list.drag.is_some()
    }

    pub fn row_count(&self) -> usize {
        self.list.rows.len()
    }

    /// One focusable per row; Space switches its toggle, Ctrl+↑/↓ move it.
    pub fn focusables(&self) -> Vec<(FocusKind, RectF, usize)> {
        (0..self.row_count()).map(|i| (FocusKind::ListRow, self.list.row_rect(i), i)).collect()
    }

    fn flip(&mut self, i: usize, now: u64) -> Option<BodyEvent> {
        let row = self.list.rows.get_mut(i)?;
        row.on.flip(now);
        Some(BodyEvent::List(ListEdit::Toggled(i, row.on.on)))
    }

    /// A press: on a toggle it switches; elsewhere on a row it picks the row up.
    pub fn press(&mut self, p: PointF, now: u64) -> Option<BodyEvent> {
        if let Some(i) = self.list.toggle_hit(p) {
            self.list.drag = None;
            return self.flip(i, now);
        }
        self.list.press(p);
        None
    }

    /// The pointer moved during a drag: the row follows, the others glide aside.
    pub fn move_to(&mut self, p: PointF, now: u64) -> bool {
        if !self.dragging() {
            return false;
        }
        self.list.drag_to(p);
        self.list.glide(now);
        true
    }

    /// The row is let go: `Moved` when it landed in another slot. The window writes it and then
    /// calls [`SectionList::commit`].
    pub fn release(&mut self, now: u64) -> (Option<BodyEvent>, bool) {
        if !self.dragging() {
            return (None, false);
        }
        let moved = self.list.release();
        // The rows shifted aside glide back when nothing moved.
        self.list.glide(now);
        (moved.map(|(from, to)| BodyEvent::List(ListEdit::Moved(from, to))), true)
    }

    pub fn activate(&mut self, row: usize, action: KeyAction, now: u64) -> Option<BodyEvent> {
        match action {
            KeyAction::Activate => self.flip(row, now),
            KeyAction::Step(d) => {
                let to = if d < 0 { row.checked_sub(1)? } else { row.checked_add(1)? };
                (row < self.row_count() && to < self.row_count()).then_some(BodyEvent::List(ListEdit::Moved(row, to)))
            }
        }
    }

    /// Applies a move the window has written.
    pub fn commit(&mut self, from: usize, to: usize, now: u64) {
        if from >= self.row_count() || to >= self.row_count() || self.keys.len() != self.row_count() {
            return;
        }
        self.list.commit(from, to, now);
        reorder(&mut self.keys, from, to);
    }

    pub fn animating(&self, now: u64) -> bool {
        self.list.animating(now) || self.list.rows.iter().any(|r| r.on.animating(now))
    }

    pub fn row_rect(&self, i: usize) -> Option<RectF> {
        (i < self.row_count()).then(|| self.list.row_rect(i))
    }

    pub fn key(&self, i: usize) -> Option<&str> {
        self.keys.get(i).map(String::as_str)
    }

    pub fn position(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    /// Paints the rows (or the hint) inside `area`, already clipped by the body.
    pub fn paint(&mut self, p: &mut Painter, cache: &mut TextCache, styles: (&TextStyle, &TextStyle), area: RectF, now: u64) {
        let (label, small) = styles;
        if (area.w - self.texts_w).abs() > 0.5 {
            self.texts.clear();
            self.hint_text = None;
            self.texts_w = area.w;
        }
        let (label_w, detail_w) = ((area.w * 0.5).max(1.0), (area.w * 0.3).max(1.0));
        for (key, row) in self.keys.iter().zip(&self.list.rows) {
            if !self.texts.contains_key(key)
                && let Ok(l) = cache.layout(&row.label, label, label_w)
            {
                let d = row.detail.as_ref().and_then(|d| cache.layout(d, small, detail_w).ok());
                self.texts.insert(key.clone(), (l, d));
            }
        }
        // Rows after one that could not be laid out paint without text.
        let refs: Vec<(&TextLayout, Option<&TextLayout>)> =
            self.keys.iter().map_while(|k| self.texts.get(k)).map(|(l, d)| (l, d.as_ref())).collect();
        self.list.paint(p, &refs, now);
        if self.row_count() == 0
            && let Some(h) = self.hint
        {
            if self.hint_text.is_none() {
                self.hint_text = cache.layout(h, label, area.w.max(1.0)).ok();
            }
            if let Some(t) = self.hint_text.as_ref() {
                let y = area.y + (self.list.row_h() - t.height()) / 2.0;
                p.text(t, PointF { x: area.x.round(), y: y.round() }, theme::TEXT_INACTIVE);
            }
        }
    }
}
