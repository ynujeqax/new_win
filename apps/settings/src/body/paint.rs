//! Laying out and painting the body's rows and controls.

use super::*;

impl Body {
    fn build_texts(&mut self, cache: &mut TextCache, s: &Strings) -> Vec<RowTexts> {
        let style = |size: f32, weight: u16| TextStyle { family: FONT.into(), size: scale(size, self.dpi), weight };
        let (label, small, small_bold, group) = (
            style(LABEL_TEXT, theme::WEIGHT_REGULAR),
            style(SMALL_TEXT, theme::WEIGHT_REGULAR),
            style(SMALL_TEXT, theme::WEIGHT_SEMIBOLD),
            style(GROUP_TEXT, theme::WEIGHT_SEMIBOLD),
        );
        let label_w = (self.area.w * 0.55).max(1.0);
        let wide = self.area.w.max(1.0);
        let field_room = self.s(FIELD_W - 2.0 * theme::TEXT_FIELD_PAD_X);
        let font_room = self.s(FONT_W - 2.0 * BUTTON_PAD_X - CHEVRON - 8.0);
        let mut lay = |t: &str, st: &TextStyle, w: f32| cache.layout(t, st, w).ok();
        self.rows
            .iter()
            .map(|r| {
                let name = sections::label(r.id, s);
                let mut t = RowTexts {
                    label: (!name.is_empty()).then(|| lay(name, &label, label_w)).flatten(),
                    group: r.group.and_then(|g| lay(sections::group_title(g, s), &group, wide)),
                    ..RowTexts::default()
                };
                let parts: Vec<Option<TextLayout>> = match &r.control {
                    Control::Choice(_) => sections::choices(r.id, s).iter().map(|c| lay(c, &small, wide)).collect(),
                    Control::Slider(v) => vec![lay(&sections::value_text(r.id, *v, s), &small, self.s(VALUE_W))],
                    Control::Font(f) => {
                        let own = TextStyle { family: f.clone(), ..label.clone() };
                        vec![lay(f, &own, font_room).or_else(|| lay(f, &label, font_room))]
                    }
                    Control::Text { hint, .. } => {
                        t.hint = lay(hint, &label, field_room);
                        let typed = self.field.edit.text();
                        if typed.is_empty() { Vec::new() } else { vec![lay(typed, &label, field_room)] }
                    }
                    Control::DateSample(f) => vec![lay(&format_bar_date(f), &small, wide)],
                    Control::Button => vec![lay(sections::button_label(r.id, self.armed.contains(&r.id), s), &small_bold, wide)],
                    Control::Info(text) => vec![lay(text, &label, wide)],
                    Control::Toggle(_) => Vec::new(),
                };
                // A part that failed to lay out is left out; the control then draws without it.
                t.parts = parts.into_iter().flatten().collect();
                t
            })
            .collect()
    }

    fn layout(&mut self, texts: &[RowTexts]) {
        let a = self.area;
        let right = a.right() - self.s(SCROLL_GAP);
        let width = |t: Option<&TextLayout>| t.map_or(0.0, |t| t.width());
        let mut y = 0.0;
        let mut geo = Vec::with_capacity(self.rows.len());
        for (i, r) in self.rows.iter().enumerate() {
            let t = texts.get(i);
            let mut g = RowGeo::default();
            if r.group.is_some() {
                if i > 0 {
                    y += self.s(GROUP_GAP);
                }
                g.group = Some(PointF { x: a.x, y });
                y += self.s(GROUP_H);
            }
            let h = if matches!(r.control, Control::DateSample(_)) { self.s(SAMPLE_H) } else { self.s(ROW_H) };
            let cy = y + h / 2.0;
            let first = t.and_then(|t| t.parts.first());
            let centered = |w: f32, ch: f32| RectF::new(right - w, (cy - ch / 2.0).round(), w, ch);
            g.control = match &r.control {
                Control::Toggle(_) => {
                    let tr = newwin_ui::toggle::Toggle::rect(PointF::default(), self.dpi);
                    centered(tr.w, tr.h)
                }
                Control::Choice(_) => {
                    let widths: Vec<f32> = t.map_or(Vec::new(), |t| t.parts.iter().map(|p| p.width()).collect());
                    let seg = self.segs.get(&r.id);
                    let probe = seg.map(|s| s.layout(PointF::default(), &widths, self.dpi)).unwrap_or_default();
                    let total = probe.last().map_or(0.0, |l| l.right());
                    let sh = self.s(theme::SEGMENT_H);
                    let origin = PointF { x: right - total, y: (cy - sh / 2.0).round() };
                    g.segs = seg.map(|s| s.layout(origin, &widths, self.dpi)).unwrap_or_default();
                    RectF::new(origin.x, origin.y, total, sh)
                }
                Control::Slider(_) => {
                    let (tw, th) = (self.s(SLIDER_W), self.s(TRACK_H));
                    let x = right - self.s(VALUE_W) - self.s(VALUE_GAP) - tw;
                    g.track = RectF::new(x, (cy - th / 2.0).round(), tw, th);
                    let k = self.s(THUMB) + self.s(6.0);
                    RectF::new(x - k / 2.0, (cy - k / 2.0).round(), tw + k, k)
                }
                Control::Font(_) => {
                    let w = (width(first) + self.s(2.0 * BUTTON_PAD_X + CHEVRON + 8.0)).min(self.s(FONT_W));
                    centered(w, self.s(BUTTON_H))
                }
                Control::Text { .. } => centered(self.s(FIELD_W), self.s(theme::TEXT_FIELD_H)),
                Control::Button => centered(width(first) + self.s(2.0 * BUTTON_PAD_X), self.s(BUTTON_H)),
                Control::DateSample(_) | Control::Info(_) => centered(width(first), first.map_or(0.0, |t| t.height())),
            };
            g.row = RectF::new(a.x, y, right - a.x, h);
            y += h;
            geo.push(g);
        }
        self.scroll.content_h = y;
        self.scroll.view_h = a.h;
        self.scroll.clamp();
        let dy = a.y - self.scroll.offset;
        for g in &mut geo {
            let shift = |r: &mut RectF| r.y += dy;
            shift(&mut g.row);
            shift(&mut g.control);
            shift(&mut g.track);
            g.segs.iter_mut().for_each(shift);
            if let Some(p) = g.group.as_mut() {
                p.y += dy;
            }
        }
        self.geo = geo;
    }

    /// Paints the body into `area` (physical px). `blink` is the caret blink period.
    pub fn paint(&mut self, p: &mut Painter, cache: &mut TextCache, s: &Strings, area: RectF, now: u64, blink: Option<u32>) {
        self.set_area(area);
        self.autoscroll(now);
        if let Some(l) = self.list.as_mut() {
            let style = |size: f32| TextStyle { family: FONT.into(), size: scale(size, self.dpi), weight: theme::WEIGHT_REGULAR };
            let (label, small) = (style(LABEL_TEXT), style(SMALL_TEXT));
            p.push_clip(area);
            l.paint(p, cache, (&label, &small), area, now);
            p.pop_clip();
            self.scroll.paint(p, area, self.dpi);
            return;
        }
        let texts = match self.texts.take() {
            Some(t) => t,
            None => self.build_texts(cache, s),
        };
        self.layout(&texts);
        p.push_clip(area);
        for (i, (r, g)) in self.rows.iter().zip(&self.geo).enumerate() {
            if g.row.bottom() < area.y || g.row.y > area.bottom() {
                continue;
            }
            let Some(t) = texts.get(i) else { continue };
            if let (Some(at), Some(gt)) = (g.group, t.group.as_ref()) {
                p.text(gt, PointF { x: at.x.round(), y: (at.y + self.s(GROUP_H) - gt.height() - self.s(4.0)).round() }, theme::TEXT_INACTIVE);
            }
            if let Some(l) = t.label.as_ref() {
                p.text(l, PointF { x: g.row.x.round(), y: (g.row.y + (g.row.h - l.height()) / 2.0).round() }, theme::TEXT);
            }
            self.paint_control(p, r, g, t, i, (now, blink));
        }
        p.pop_clip();
        self.scroll.paint(p, area, self.dpi);
        self.texts = Some(texts);
    }

    fn paint_control(&self, p: &mut Painter, r: &Row, g: &RowGeo, t: &RowTexts, i: usize, clock: (u64, Option<u32>)) {
        let now = clock.0;
        let c = g.control;
        let first = t.parts.first();
        let mid = |t: &TextLayout, x: f32| PointF { x: x.round(), y: (c.y + (c.h - t.height()) / 2.0).round() };
        match &r.control {
            Control::Toggle(_) => {
                if let Some(tg) = self.toggles.get(&r.id) {
                    tg.paint(p, c, now);
                }
            }
            Control::Choice(_) => {
                if let Some(seg) = self.segs.get(&r.id)
                    && t.parts.len() == g.segs.len()
                {
                    seg.paint(p, &g.segs, &t.parts, now);
                }
            }
            Control::Slider(v) => {
                let (tr, range) = (g.track, sections::range_of(r.id));
                let x = thumb_x_for(*v, tr, range);
                p.fill_round_rect(tr, tr.h / 2.0, TRACK);
                p.fill_round_rect(RectF::new(tr.x, tr.y, (x - tr.x).max(0.0), tr.h), tr.h / 2.0, FILL);
                let k = self.s(THUMB);
                p.fill_round_rect(RectF::new(x - k / 2.0, tr.y + tr.h / 2.0 - k / 2.0, k, k), k / 2.0, THUMB_COLOR);
                if let Some(vt) = first {
                    p.text(vt, mid(vt, g.row.right() - vt.width()), theme::TEXT_INACTIVE);
                }
            }
            Control::Font(_) => {
                p.fill_round_rect(c, c.h / 2.0, if self.hover == Some(i) { BUTTON_HOVER } else { BUTTON_FILL });
                if let Some(ft) = first {
                    p.text(ft, mid(ft, c.x + self.s(BUTTON_PAD_X)), theme::TEXT_STRONG);
                }
                let (cx, cy, w) = (c.right() - self.s(BUTTON_PAD_X) - self.s(CHEVRON) / 2.0, c.y + c.h / 2.0, self.s(CHEVRON) / 2.0);
                let pts = [PointF { x: cx - w, y: cy - w / 2.0 }, PointF { x: cx, y: cy + w / 2.0 }, PointF { x: cx + w, y: cy - w / 2.0 }];
                p.stroke_polyline(&pts, theme::TEXT, scale(theme::ICON_STROKE, self.dpi));
            }
            Control::Text { .. } => self.field.paint(p, c, first, t.hint.as_ref(), clock, self.dpi),
            Control::Button => {
                let fill = if self.pressed == Some(i) {
                    theme::ACTIVE_PILL
                } else if self.hover == Some(i) {
                    BUTTON_HOVER
                } else {
                    BUTTON_FILL
                };
                p.fill_round_rect(c, c.h / 2.0, fill);
                if self.armed.contains(&r.id) {
                    p.stroke_round_rect(c, c.h / 2.0, ARMED_RIM, self.s(1.0).max(1.0));
                }
                if let Some(bt) = first {
                    p.text(bt, mid(bt, c.x + (c.w - bt.width()) / 2.0), theme::TEXT_STRONG);
                }
            }
            Control::DateSample(_) => {
                if let Some(st) = first {
                    p.text(st, mid(st, c.x), theme::TEXT_INACTIVE);
                }
            }
            Control::Info(_) => {
                if let Some(it) = first {
                    p.text(it, mid(it, c.x), theme::TEXT);
                }
            }
        }
    }
}
