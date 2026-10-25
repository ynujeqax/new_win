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

use crate::sections::{self, Control, List, ListEdit, Row, RowId, RowValue};
use crate::view::{FocusKind, KeyAction};

mod list;
mod paint;
mod pointer;

use list::SectionList;

/// What a control asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyEvent {
    Set(RowId, RowValue),
    Press(RowId),
    OpenFonts,
    FocusField,
    /// A toggle or a drag in the Items / Tray list.
    List(ListEdit),
}

/// What the focus is on, kept across a change of rows: a row, or a list row by its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusKey {
    Row(RowId),
    Item(String),
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
    /// Items and Tray: the reorder list instead of rows.
    list: Option<SectionList>,
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
            list: None,
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
        if let Some(l) = self.list.as_mut() {
            l.forget_texts();
        }
        self.layout_list();
    }

    /// The rows for the current config: changed toggles and segments glide to their new value;
    /// the field keeps what is being typed while it is focused.
    pub fn set_rows(&mut self, rows: Vec<Row>, s: &Strings, now: u64) {
        self.list = None;
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

    /// Items / Tray: the list for the current config (see [`SectionList::update`]).
    pub fn set_list(&mut self, l: List, now: u64) {
        if !self.rows.is_empty() {
            *self = Body { area: self.area, dpi: self.dpi, ..Body::new() };
        }
        match self.list.as_mut() {
            Some(current) => current.update(l, now),
            None => self.list = Some(SectionList::new(l)),
        }
        self.layout_list();
    }

    /// The list's keys in the order shown (empty for a section of rows).
    pub fn list_keys(&self) -> &[String] {
        self.list.as_ref().map_or(&[], |l| &l.keys)
    }

    /// Applies a move the window has written into the config.
    pub fn commit_move(&mut self, from: usize, to: usize, now: u64) {
        if let Some(l) = self.list.as_mut() {
            l.commit(from, to, now);
        }
    }

    /// The text shown by an empty list.
    #[cfg(test)]
    pub fn hint(&self) -> Option<&'static str> {
        self.list.as_ref().filter(|l| l.row_count() == 0).and_then(|l| l.hint)
    }

    /// What row `row` is, to find it again after the rows change.
    pub fn focus_key(&self, row: usize) -> Option<FocusKey> {
        match self.list.as_ref() {
            Some(l) => l.key(row).map(|k| FocusKey::Item(k.to_owned())),
            None => self.rows.get(row).map(|r| FocusKey::Row(r.id)),
        }
    }

    /// The index into [`Body::focusables`] of the control `key` names, if it is still there.
    pub fn focus_of(&self, key: &FocusKey) -> Option<usize> {
        let row = match (key, self.list.as_ref()) {
            (FocusKey::Item(k), Some(l)) => l.position(k)?,
            (FocusKey::Row(id), None) => self.rows.iter().position(|r| r.id == *id)?,
            _ => return None,
        };
        self.focusables().iter().position(|f| f.2 == row)
    }

    /// Lays the list out in the area at the current scroll.
    fn layout_list(&mut self) {
        let (a, dpi) = (self.area, self.dpi);
        let (right, row_h) = (a.right() - self.s(SCROLL_GAP), self.s(ROW_H));
        let Some(l) = self.list.as_mut() else { return };
        self.scroll.content_h = row_h * l.row_count().max(1) as f32;
        self.scroll.view_h = a.h;
        self.scroll.clamp();
        l.list.set_layout(PointF { x: a.x, y: a.y - self.scroll.offset }, right - a.x, row_h, dpi);
    }

    /// The body's area (physical px); painting sets it too.
    pub fn set_area(&mut self, area: RectF) {
        if area != self.area {
            self.area = area;
            self.texts = None;
        }
        self.layout_list();
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
        if let Some(l) = self.list.as_ref() {
            return l.focusables();
        }
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
        if let Some(l) = self.list.as_mut() {
            return l.activate(row, action, now);
        }
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
        let rect = match self.list.as_ref() {
            Some(l) => l.row_rect(row),
            None => self.geo.get(row).map(|g| g.row),
        };
        let Some(r) = rect else { return };
        let (top, bottom) = (r.y, r.bottom());
        if top < self.area.y {
            self.scroll.offset -= self.area.y - top;
        } else if bottom > self.area.bottom() {
            self.scroll.offset += bottom - self.area.bottom();
        }
        self.scroll.clamp();
        self.layout_list();
    }


    pub fn animating(&self, now: u64) -> bool {
        self.toggles.values().any(|t| t.animating(now))
            || self.segs.values().any(|g| g.animating(now))
            || self.list.as_ref().is_some_and(|l| l.animating(now))
    }
}

#[cfg(test)]
mod tests;
