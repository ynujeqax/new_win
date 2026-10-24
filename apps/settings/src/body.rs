//! The section body (spec: M4 §3): the rows of [`crate::sections`] laid out as "name — control"
//! under small group captions, scrolled when taller than the area.
//!
//! The body keeps the controls' own state between frames (a toggle's glide, a segment's pill, the
//! template field's text and caret); [`Body::set_rows`] brings it to a new config. Controls do not
//! change the config: they answer with a [`BodyEvent`] the window applies and then calls
//! `set_rows` with the result. Geometry is the last painted one (physical px); hits use it.

use std::collections::HashMap;

use newwin_core::strings::Strings;
use newwin_core::text_edit::Edit;
use newwin_platform::clock::format_bar_date;
use newwin_platform::dpi::scale;
use newwin_ui::scroll::Scroll;
use newwin_ui::segmented::Segmented;
use newwin_ui::slider::{thumb_x_for, value_at};
use newwin_ui::text_field::TextField;
use newwin_ui::toggle::Toggle;
use newwin_ui::{Color, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, theme};

use crate::sections::{self, Control, Row, RowId, RowValue};
use crate::view::{FocusKind, KeyAction};

/// What a control asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyEvent {
    Set(RowId, RowValue),
    Press(RowId),
    OpenFonts,
    FocusField,
}

const FONT: &str = "Segoe UI Variable Text";
// Layout, logical px.
const ROW_H: f32 = 40.0;
const SAMPLE_H: f32 = 22.0;
const GROUP_GAP: f32 = 14.0;
const GROUP_H: f32 = 22.0;
/// Room at the right for the scroll thumb.
const SCROLL_GAP: f32 = 14.0;
const LABEL_TEXT: f32 = 13.0;
const SMALL_TEXT: f32 = 12.0;
const GROUP_TEXT: f32 = 11.0;
const SLIDER_W: f32 = 180.0;
const VALUE_W: f32 = 64.0;
const VALUE_GAP: f32 = 12.0;
const TRACK_H: f32 = 4.0;
const THUMB: f32 = 14.0;
const FIELD_W: f32 = 220.0;
const FONT_W: f32 = 240.0;
const BUTTON_H: f32 = 26.0;
const BUTTON_PAD_X: f32 = 12.0;
const CHEVRON: f32 = 8.0;
const TRACK: Color = Color::rgba(255, 255, 255, 0.14);
const FILL: Color = Color::rgba(255, 255, 255, 0.85);
const THUMB_COLOR: Color = Color::rgba(255, 255, 255, 0.95);
const BUTTON_FILL: Color = Color::rgba(255, 255, 255, 0.12);
const BUTTON_HOVER: Color = Color::rgba(255, 255, 255, 0.22);
const ARMED_RIM: Color = Color::rgba(255, 255, 255, 0.6);

/// Laid-out texts of one row: its name, then what the control shows (segment labels, the value,
/// the button label, the font name, the field's text…), and the field's placeholder.
#[derive(Default)]
struct RowTexts {
    label: Option<TextLayout>,
    parts: Vec<TextLayout>,
    hint: Option<TextLayout>,
    group: Option<TextLayout>,
}

#[derive(Clone, Debug, Default)]
struct RowGeo {
    row: RectF,
    /// What a click hits and the focus ring goes around.
    control: RectF,
    /// A slider's track.
    track: RectF,
    segs: Vec<RectF>,
    group: Option<PointF>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Slider(usize),
    /// The scroll thumb, grabbed this far below its top.
    Thumb(f32),
}

pub struct Body {
    rows: Vec<Row>,
    toggles: HashMap<RowId, Toggle>,
    segs: HashMap<RowId, Segmented>,
    /// The date template field (the only field).
    pub field: TextField,
    /// Buttons waiting for their confirming click.
    armed: Vec<RowId>,
    texts: Option<Vec<RowTexts>>,
    geo: Vec<RowGeo>,
    area: RectF,
    scroll: Scroll,
    drag: Option<Drag>,
    /// A button or the font row under the pointer, and the button being pressed.
    hover: Option<usize>,
    pressed: Option<usize>,
    dpi: u32,
}

fn focus_kind(c: &Control) -> Option<FocusKind> {
    match c {
        Control::Toggle(_) => Some(FocusKind::Toggle),
        Control::Choice(_) => Some(FocusKind::Segmented),
        Control::Slider(_) => Some(FocusKind::Slider),
        Control::Font(_) => Some(FocusKind::FontList),
        Control::Text { .. } => Some(FocusKind::TextField),
        Control::Button => Some(FocusKind::Button),
        Control::DateSample(_) | Control::Info(_) => None,
    }
}

fn set_text(field: &mut TextField, text: &str) {
    field.edit.clear();
    if !text.is_empty() {
        field.edit.apply(Edit::Insert(text.to_owned()));
    }
}

impl Body {
    pub fn new() -> Body {
        Body {
            rows: Vec::new(),
            toggles: HashMap::new(),
            segs: HashMap::new(),
            field: TextField::default(),
            armed: Vec::new(),
            texts: None,
            geo: Vec::new(),
            area: RectF::default(),
            scroll: Scroll::default(),
            drag: None,
            hover: None,
            pressed: None,
            dpi: 96,
        }
    }

    fn s(&self, v: f32) -> f32 {
        scale(v, self.dpi).round()
    }

    /// Another section: controls start fresh, at the top.
    pub fn reset(&mut self) {
        *self = Body { dpi: self.dpi, ..Body::new() };
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
        self.texts = None;
    }

    /// The rows for the current config: changed toggles and segments glide to their new value;
    /// the field keeps what is being typed while it is focused.
    pub fn set_rows(&mut self, rows: Vec<Row>, s: &Strings, now: u64) {
        for r in &rows {
            match &r.control {
                Control::Toggle(on) => match self.toggles.get_mut(&r.id) {
                    Some(t) if t.on != *on => t.set(*on, now),
                    Some(_) => {}
                    None => {
                        self.toggles.insert(r.id, Toggle::new(*on));
                    }
                },
                Control::Choice(i) => match self.segs.get_mut(&r.id) {
                    Some(g) if g.selected != *i => g.select(*i, now),
                    Some(_) => {}
                    None => {
                        self.segs.insert(r.id, Segmented::new(sections::choices(r.id, s), *i));
                    }
                },
                Control::Text { text, .. } if !self.field.focused && self.field.edit.text() != text => set_text(&mut self.field, text),
                _ => {}
            }
        }
        self.toggles.retain(|id, _| rows.iter().any(|r| r.id == *id));
        self.segs.retain(|id, _| rows.iter().any(|r| r.id == *id));
        if !rows.iter().any(|r| matches!(r.control, Control::Text { .. })) {
            self.field = TextField::default();
        }
        if rows.len() != self.rows.len() {
            self.hover = None;
            self.pressed = None;
            self.drag = None;
        }
        self.rows = rows;
        self.texts = None;
    }

    /// Buttons now waiting for a second click (their label says so).
    pub fn set_armed(&mut self, armed: Vec<RowId>) {
        if armed != self.armed {
            self.armed = armed;
            self.texts = None;
        }
    }

    /// The focusable controls in order: (kind, rectangle, row index).
    pub fn focusables(&self) -> Vec<(FocusKind, RectF, usize)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| focus_kind(&r.control).map(|k| (k, self.geo.get(i).map(|g| g.control).unwrap_or_default(), i)))
            .collect()
    }

    /// The font row's control, where the font list opens.
    pub fn font_anchor(&self) -> Option<RectF> {
        let i = self.rows.iter().position(|r| matches!(r.control, Control::Font(_)))?;
        self.geo.get(i).map(|g| g.control)
    }

    pub fn font_focus_index(&self) -> Option<usize> {
        self.focusables().iter().position(|f| f.0 == FocusKind::FontList)
    }

    pub fn field_focus_index(&self) -> Option<usize> {
        self.focusables().iter().position(|f| f.0 == FocusKind::TextField)
    }

    fn slider_value(&self, i: usize) -> Option<(RowId, i32)> {
        let r = self.rows.get(i)?;
        match r.control {
            Control::Slider(v) => Some((r.id, v)),
            _ => None,
        }
    }

    /// A key on the control in row `row`.
    pub fn activate(&mut self, row: usize, action: KeyAction, now: u64) -> Option<BodyEvent> {
        let r = self.rows.get(row)?;
        let id = r.id;
        match (&r.control, action) {
            (Control::Toggle(_), KeyAction::Activate) => {
                let t = self.toggles.get_mut(&id)?;
                t.flip(now);
                Some(BodyEvent::Set(id, RowValue::Bool(t.on)))
            }
            (Control::Choice(_), KeyAction::Step(d)) => {
                let g = self.segs.get_mut(&id)?;
                let last = g.labels.len().checked_sub(1)?;
                let next = if d < 0 { g.selected.checked_sub(1)? } else { g.selected + 1 };
                if next > last {
                    return None;
                }
                g.select(next, now);
                Some(BodyEvent::Set(id, RowValue::Choice(next)))
            }
            (Control::Slider(v), KeyAction::Step(d)) => {
                let next = sections::step(id, *v, d);
                (next != *v).then_some(BodyEvent::Set(id, RowValue::Int(next)))
            }
            (Control::Font(_), KeyAction::Activate) => Some(BodyEvent::OpenFonts),
            (Control::Button, KeyAction::Activate) => Some(BodyEvent::Press(id)),
            _ => None,
        }
    }

    /// Typing in the focused template field.
    pub fn field_input(&mut self, ev: newwin_ui::panel::PanelInput, now: u64) -> Option<BodyEvent> {
        let id = self.rows.iter().find(|r| matches!(r.control, Control::Text { .. }))?.id;
        if !self.field.input_at(ev, now) {
            return None;
        }
        self.texts = None;
        Some(BodyEvent::Set(id, RowValue::Text(self.field.edit.text().to_owned())))
    }

    /// Scrolls so that row `row` is inside the area.
    pub fn reveal(&mut self, row: usize) {
        let Some(g) = self.geo.get(row) else { return };
        let (top, bottom) = (g.row.y, g.row.bottom());
        if top < self.area.y {
            self.scroll.offset -= self.area.y - top;
        } else if bottom > self.area.bottom() {
            self.scroll.offset += bottom - self.area.bottom();
        }
        self.scroll.clamp();
    }

    pub fn wheel(&mut self, delta: i16) -> bool {
        let before = self.scroll.offset;
        self.scroll.wheel_dpi(delta, self.dpi);
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
    pub fn move_to(&mut self, p: PointF) -> (Option<BodyEvent>, bool) {
        match self.drag {
            Some(Drag::Slider(i)) => {
                let (Some((id, v)), Some(g)) = (self.slider_value(i), self.geo.get(i)) else { return (None, false) };
                let next = value_at(p.x, g.track, sections::range_of(id));
                ((next != v).then_some(BodyEvent::Set(id, RowValue::Int(next))), false)
            }
            Some(Drag::Thumb(grab)) => {
                self.scroll.drag_thumb_dpi(self.area, p.y, grab, self.dpi);
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

    /// The button released over itself is pressed.
    pub fn release(&mut self, p: PointF) -> (Option<BodyEvent>, bool) {
        let dragged = self.drag.take().is_some();
        let Some(i) = self.pressed.take() else { return (None, dragged) };
        let hit = self.geo.get(i).is_some_and(|g| g.control.inset(-self.s(4.0)).contains(p));
        (hit.then(|| self.rows.get(i).map(|r| BodyEvent::Press(r.id))).flatten(), true)
    }

    pub fn animating(&self, now: u64) -> bool {
        self.toggles.values().any(|t| t.animating(now)) || self.segs.values().any(|g| g.animating(now))
    }

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
        if area != self.area {
            self.area = area;
            self.texts = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::{Control, rows_for};
    use crate::view::Section;
    use newwin_config::Config;
    use newwin_core::strings::strings_for;

    fn body(section: Section) -> Body {
        let mut b = Body::new();
        b.set_rows(rows_for(section, &Config::default(), false), strings_for("en-US"), 0);
        b
    }

    #[test]
    fn every_bar_control_is_focusable_in_order() {
        let kinds: Vec<_> = body(Section::Bar).focusables().iter().map(|f| f.0).collect();
        use FocusKind::*;
        assert_eq!(kinds, [Segmented, Slider, Slider, Segmented, Toggle, Toggle]);
    }

    #[test]
    fn about_focuses_its_buttons_not_the_version() {
        let f = body(Section::About).focusables();
        assert_eq!(f.iter().map(|f| (f.0, f.2)).collect::<Vec<_>>(), [(FocusKind::Button, 1), (FocusKind::Button, 2), (FocusKind::Button, 3)]);
    }

    #[test]
    fn keys_change_the_focused_control() {
        let mut b = body(Section::Bar);
        assert_eq!(b.activate(0, KeyAction::Step(1), 0), Some(BodyEvent::Set(RowId::Mode, RowValue::Choice(1))));
        assert_eq!(b.activate(0, KeyAction::Step(1), 0), None, "already on the last segment");
        assert_eq!(b.activate(1, KeyAction::Step(-1), 0), Some(BodyEvent::Set(RowId::ShowDelay, RowValue::Int(140))));
        assert_eq!(b.activate(4, KeyAction::Activate, 0), Some(BodyEvent::Set(RowId::IgnoreFullscreen, RowValue::Bool(false))));
        let mut a = body(Section::About);
        assert_eq!(a.activate(3, KeyAction::Activate, 0), Some(BodyEvent::Press(RowId::RestoreTaskbar)));
        let mut ap = body(Section::Appearance);
        let font = rows_for(Section::Appearance, &Config::default(), false).iter().position(|r| matches!(r.control, Control::Font(_))).unwrap();
        assert_eq!(ap.activate(font, KeyAction::Activate, 0), Some(BodyEvent::OpenFonts));
    }

    #[test]
    fn an_outside_change_keeps_the_typed_template_while_the_field_is_focused() {
        let en = strings_for("en-US");
        let mut c = Config::default();
        c.appearance.date_format = "dd.MM".into();
        let mut b = Body::new();
        b.set_rows(rows_for(Section::Appearance, &c, false), en, 0);
        assert_eq!(b.field.edit.text(), "dd.MM");
        b.field.focused = true;
        c.appearance.date_format = "system".into();
        b.set_rows(rows_for(Section::Appearance, &c, true), en, 0);
        assert_eq!(b.field.edit.text(), "dd.MM");
    }
}
