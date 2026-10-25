//! Pointer input on the body: the wheel, presses, drags and releases.

use super::*;

impl Body {
    pub fn wheel(&mut self, delta: i16) -> bool {
        let before = self.scroll.offset;
        self.scroll.wheel_dpi(delta, self.dpi);
        self.layout_list();
        self.scroll.offset != before
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
            Control::Button if grab.contains(p) => {
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
                    let pointable = self.rows.get(i).is_some_and(|r| matches!(r.control, Control::Button | Control::Font(_)));
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
            return l.release(now);
        }
        let dragged = self.drag.take().is_some();
        let Some(i) = self.pressed.take() else { return (None, dragged) };
        let hit = self.geo.get(i).is_some_and(|g| g.control.inset(-self.s(4.0)).contains(p));
        (hit.then(|| self.rows.get(i).map(|r| BodyEvent::Press(r.id))).flatten(), true)
    }
}
