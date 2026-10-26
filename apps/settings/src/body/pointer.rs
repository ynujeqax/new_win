//! Pointer input on the body: the wheel, presses, drags and releases.

use super::*;

impl Body {
    /// The wheel scrolls; during a row drag the dragged row stays under the pointer, so its slot
    /// moves with the list.
    pub fn wheel(&mut self, delta: i16, now: u64) -> bool {
        let before = self.scroll.offset;
        self.scroll.wheel_dpi(delta, self.dpi);
        self.layout_list();
        self.follow_drag(now);
        self.scroll.offset != before
    }

    /// One autoscroll step while a row is dragged near the list's top or bottom edge; true when
    /// the list scrolled. Called every frame (paint) while [`Body::animating`] says so.
    pub fn autoscroll(&mut self, now: u64) -> bool {
        let speed = self.autoscroll_speed();
        if speed == 0.0 {
            self.scroll_tick = None;
            return false;
        }
        let last = self.scroll_tick.replace(now).unwrap_or(now);
        let dt = (now.saturating_sub(last)).min(AUTOSCROLL_MAX_STEP_MS) as f32 / 1000.0;
        let before = self.scroll.offset;
        self.scroll.offset += speed * dt;
        self.layout_list();
        self.follow_drag(now);
        self.scroll.offset != before
    }

    /// px/s the list scrolls by itself now: 0 unless a row is dragged near an edge.
    pub(super) fn autoscroll_speed(&self) -> f32 {
        match (self.drag_at, self.list.as_ref()) {
            (Some(p), Some(l)) if l.dragging() => {
                autoscroll_speed(p.y, self.area.y, self.area.bottom(), self.s(AUTOSCROLL_ZONE), self.s(AUTOSCROLL_MAX))
            }
            _ => 0.0,
        }
    }

    /// After a scroll the dragged row is moved to the pointer again: its slot follows.
    fn follow_drag(&mut self, now: u64) {
        if let (Some(p), Some(l)) = (self.drag_at, self.list.as_mut()) {
            l.move_to(p, now);
        }
    }

    #[cfg(test)]
    pub fn scroll_offset(&self) -> f32 {
        self.scroll.offset
    }

    fn row_at(&self, p: PointF) -> Option<usize> {
        if !self.area.contains(p) {
            return None;
        }
        self.geo.iter().position(|g| g.row.contains(p))
    }

    /// A press in the body.
    pub fn press(&mut self, p: PointF, now: u64) -> Option<BodyEvent> {
        if !self.area.contains(p) {
            return None;
        }
        if let Some(t) = self.scroll.thumb_rect_dpi(self.area, self.dpi).filter(|t| t.inset(-self.s(4.0)).contains(p)) {
            self.drag = Some(Drag::Thumb(p.y - t.y));
            return None;
        }
        if let Some(l) = self.list.as_mut() {
            return l.press(p, now);
        }
        let i = self.row_at(p)?;
        let (r, g) = (self.rows.get(i)?, self.geo.get(i)?);
        let id = r.id;
        let grab = g.control.inset(-self.s(4.0));
        match &r.control {
            Control::Toggle(_) if grab.contains(p) => {
                let t = self.toggles.get_mut(&id)?;
                t.flip(now);
                Some(BodyEvent::Set(id, RowValue::Bool(t.on)))
            }
            Control::Choice(_) => {
                let segs = g.segs.clone();
                let seg = self.segs.get_mut(&id)?;
                let k = seg.hit(&segs, p)?;
                if k == seg.selected {
                    return None;
                }
                seg.select(k, now);
                Some(BodyEvent::Set(id, RowValue::Choice(k)))
            }
            Control::Slider(v) if grab.contains(p) => {
                self.drag = Some(Drag::Slider(i));
                let next = value_at(p.x, g.track, sections::range_of(id));
                (next != *v).then_some(BodyEvent::Set(id, RowValue::Int(next)))
            }
            Control::Font(_) if grab.contains(p) => Some(BodyEvent::OpenFonts),
            Control::Text { .. } if grab.contains(p) => Some(BodyEvent::FocusField),
            Control::Button | Control::Link { .. } if grab.contains(p) => {
                self.pressed = Some(i);
                None
            }
            _ => None,
        }
    }

    /// The pointer moved; `(event, repaint)`.
    pub fn move_to(&mut self, p: PointF, now: u64) -> (Option<BodyEvent>, bool) {
        if let Some(l) = self.list.as_mut()
            && l.dragging()
        {
            self.drag_at = Some(p);
            return (None, l.move_to(p, now));
        }
        match self.drag {
            Some(Drag::Slider(i)) => {
                let (Some((id, v)), Some(g)) = (self.slider_value(i), self.geo.get(i)) else { return (None, false) };
                let next = value_at(p.x, g.track, sections::range_of(id));
                ((next != v).then_some(BodyEvent::Set(id, RowValue::Int(next))), false)
            }
            Some(Drag::Thumb(grab)) => {
                self.scroll.drag_thumb_dpi(self.area, p.y, grab, self.dpi);
                self.layout_list();
                (None, true)
            }
            None => {
                let hover = self.row_at(p).filter(|&i| {
                    let pointable = self.rows.get(i).is_some_and(|r| matches!(r.control, Control::Button | Control::Font(_) | Control::Link { .. }));
                    pointable && self.geo.get(i).is_some_and(|g| g.control.contains(p))
                });
                let changed = hover != self.hover;
                self.hover = hover;
                (None, changed)
            }
        }
    }

    pub fn leave(&mut self) -> bool {
        self.hover.take().is_some()
    }

    /// The button released over itself is pressed; a list row let go lands.
    pub fn release(&mut self, p: PointF, now: u64) -> (Option<BodyEvent>, bool) {
        if let Some(l) = self.list.as_mut()
            && l.dragging()
        {
            self.drag_at = None;
            self.scroll_tick = None;
            return l.release(now);
        }
        let dragged = self.drag.take().is_some();
        let Some(i) = self.pressed.take() else { return (None, dragged) };
        let hit = self.geo.get(i).is_some_and(|g| g.control.inset(-self.s(4.0)).contains(p));
        (hit.then(|| self.rows.get(i).map(|r| BodyEvent::Press(r.id))).flatten(), true)
    }
}
