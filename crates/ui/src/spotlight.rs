//! Spotlight's content (spec: M3 §2, §5): a query field, up to `max` result rows and a line of
//! key hints, on the glass `Panel`. Ranking is `newwin_core::spotlight`; this decides what a key
//! or click means and draws it.

use std::collections::HashMap;
use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::app_icon::{ICON_FADE_MS, ICON_GAP_LOGICAL_PX, ICON_LOGICAL_PX, IconImage, fade as icon_fade, source_for_candidate};
use newwin_core::spotlight::{Candidate, Confirm, Hit, Kind, SystemCommand, rank};
use newwin_core::text_edit::{Edit, TextEdit};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_RETURN, VK_UP};

use crate::icon_bitmaps::{IconBitmaps, icon_rect};
use crate::panel::{PanelContent, PanelInput, PanelReply, radius_for};
use crate::popup::PopupStyle;
use crate::text_field::{caret_visible, edit_for, next_flip};
use crate::{Gfx, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotlightOp {
    Open = 0,
    Admin = 1,
    Menu = 2,
    /// The results changed: the owner looks up their icons ([`SpotlightView::shown`]).
    Icons = 3,
}

/// Panel action id: `index` is into the catalog the view shows.
pub fn action_id(index: usize, op: SpotlightOp) -> u32 {
    ((index as u32) << 2) | op as u32
}

pub fn decode_action(id: u32) -> (usize, SpotlightOp) {
    let op = match id & 3 {
        1 => SpotlightOp::Admin,
        2 => SpotlightOp::Menu,
        3 => SpotlightOp::Icons,
        _ => SpotlightOp::Open,
    };
    ((id >> 2) as usize, op)
}

pub struct SpotlightLabels {
    pub hint: String,
    pub no_results: String,
    pub kind_app: String,
    pub kind_system: String,
    pub press_again_shut_down: String,
    pub press_again_restart: String,
    pub keys: String,
}

struct Text {
    cache: TextCache,
    query: TextStyle,
    regular: TextStyle,
    small: TextStyle,
}

struct RowText {
    name: Option<TextLayout>,
    kind: Option<TextLayout>,
}

pub struct SpotlightView {
    catalog: Rc<Vec<Candidate>>,
    launches: HashMap<String, u32>,
    labels: SpotlightLabels,
    max: usize,
    dpi: u32,
    blink_ms: Option<u32>,
    text: Option<Text>,
    edit: TextEdit,
    hits: Vec<Hit>,
    selected: Option<usize>,
    hover: Option<usize>,
    confirm: Confirm,
    /// When the caret last restarted its blink (on every edit).
    blink_from: u64,
    pill_y: Tween,
    hover_y: Tween,
    hover_o: Tween,
    rows_o: Tween,
    query_text: Option<TextLayout>,
    hint_text: Option<TextLayout>,
    keys_text: Option<TextLayout>,
    empty_text: Option<TextLayout>,
    rows: Vec<RowText>,
    /// The armed row's label is drawn instead of its name (catalog index, layout).
    armed_text: Option<(usize, TextLayout)>,
    /// App icons by catalog index, with when each arrived (it fades in).
    icons: HashMap<usize, (Rc<IconImage>, u64)>,
    icon_bitmaps: IconBitmaps,
}

impl SpotlightView {
    pub fn new(
        catalog: Rc<Vec<Candidate>>,
        launches: HashMap<String, u32>,
        labels: SpotlightLabels,
        style: &PopupStyle,
        max_results: usize,
        blink_ms: Option<u32>,
        gfx: &Rc<Gfx>,
    ) -> SpotlightView {
        let s = |v: f32| (v * style.dpi as f32 / 96.0).round();
        let regular = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let text = Text {
            cache: TextCache::new(gfx),
            query: TextStyle { size: s(theme::SPOTLIGHT_QUERY_TEXT), ..regular.clone() },
            small: TextStyle { size: s(theme::SPOTLIGHT_HINT_TEXT), ..regular.clone() },
            regular,
        };
        let mut v = Self::build(catalog, launches, labels, style.dpi, max_results, blink_ms, Some(text));
        v.rebuild();
        v
    }

    /// Without text layout (geometry and input only), at 96 DPI.
    pub fn for_test(catalog: Rc<Vec<Candidate>>, max_results: usize) -> SpotlightView {
        let labels = SpotlightLabels {
            hint: String::new(),
            no_results: String::new(),
            kind_app: String::new(),
            kind_system: String::new(),
            press_again_shut_down: String::new(),
            press_again_restart: String::new(),
            keys: String::new(),
        };
        Self::build(catalog, HashMap::new(), labels, 96, max_results, None, None)
    }

    fn build(catalog: Rc<Vec<Candidate>>, launches: HashMap<String, u32>, labels: SpotlightLabels, dpi: u32, max: usize, blink_ms: Option<u32>, text: Option<Text>) -> Self {
        SpotlightView {
            catalog,
            launches,
            labels,
            max,
            dpi,
            blink_ms,
            text,
            edit: TextEdit::default(),
            hits: Vec::new(),
            selected: None,
            hover: None,
            confirm: Confirm::default(),
            blink_from: 0,
            pill_y: Tween::at_rest(0.0),
            hover_y: Tween::at_rest(0.0),
            hover_o: Tween::at_rest(0.0),
            rows_o: Tween::at_rest(1.0),
            query_text: None,
            hint_text: None,
            keys_text: None,
            empty_text: None,
            rows: Vec::new(),
            armed_text: None,
            icons: HashMap::new(),
            icon_bitmaps: IconBitmaps::default(),
        }
    }

    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    pub fn query(&self) -> &str {
        self.edit.text()
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// Each shown row's catalog index and where its icon comes from (none for system commands).
    pub fn shown(&self) -> Vec<(usize, Option<String>)> {
        self.hits.iter().filter_map(|h| self.catalog.get(h.index).map(|c| (h.index, source_for_candidate(c)))).collect()
    }

    /// The icon of catalog entry `index`, arrived at `arrived_ms` (0: shown at once).
    pub fn set_icon(&mut self, index: usize, img: Rc<IconImage>, arrived_ms: u64) {
        self.icons.insert(index, (img, arrived_ms));
    }

    /// Where a row's name starts, from the row's left: after the icon column, for every row.
    pub fn name_x(&self) -> f32 {
        self.s(theme::POPUP_ITEM_PAD_X) + self.s(ICON_LOGICAL_PX).round() + self.s(ICON_GAP_LOGICAL_PX).round()
    }

    /// The selected row.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn catalog(&self) -> &Rc<Vec<Candidate>> {
        &self.catalog
    }

    /// A new index (apps installed or removed): re-ranks, keeping the same entry selected when it
    /// is still shown.
    pub fn set_catalog(&mut self, c: Rc<Vec<Candidate>>) {
        // Icons are kept by index, which a new catalog changes.
        self.icons.clear();
        let keep = self.selected.and_then(|r| self.hits.get(r)).and_then(|h| self.catalog.get(h.index)).map(|c| c.id.clone());
        self.catalog = c;
        self.rank();
        if let Some(id) = keep
            && let Some(r) = self.hits.iter().position(|h| self.catalog.get(h.index).is_some_and(|c| c.id == id))
        {
            self.selected = Some(r);
        }
        self.pill_y = Tween::at_rest(self.selected.map_or(0.0, |r| self.row_rect(r).y));
        self.rebuild();
    }

    /// Rows shown below the field: none for an empty query, one for "No results".
    fn rows_shown(&self) -> usize {
        if self.edit.text().trim().is_empty() { 0 } else { self.hits.len().max(1) }
    }

    fn rows_h(&self) -> f32 {
        match self.rows_shown() {
            0 => 0.0,
            n => 2.0 * self.s(theme::SPOTLIGHT_PAD) + n as f32 * self.s(theme::SPOTLIGHT_ROW_H),
        }
    }

    fn width(&self) -> f32 {
        self.s(theme::SPOTLIGHT_WIDTH)
    }

    /// Panel-local rectangle of result row `row`.
    pub fn row_rect(&self, row: usize) -> RectF {
        let pad = self.s(theme::SPOTLIGHT_PAD);
        let y = self.s(theme::SPOTLIGHT_FIELD_H) + pad + row as f32 * self.s(theme::SPOTLIGHT_ROW_H);
        RectF::new(pad, y, self.width() - 2.0 * pad, self.s(theme::SPOTLIGHT_ROW_H))
    }

    fn hit_row(&self, p: PointF) -> Option<usize> {
        (0..self.hits.len()).find(|&r| self.row_rect(r).contains(p))
    }

    fn rank(&mut self) {
        let launches = &self.launches;
        self.hits = rank(self.edit.text(), &self.catalog, &|k| launches.get(k).copied().unwrap_or(0), self.max);
        self.selected = (!self.hits.is_empty()).then_some(0);
        self.hover = None;
        self.confirm.reset();
    }

    /// Lays out the text after the query, the rows or the confirmation changed.
    fn rebuild(&mut self) {
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let row_w = self.row_rect(0).w - 2.0 * pad_x;
        let name_w = self.row_rect(0).w - self.name_x() - pad_x;
        let field_room = self.width() - self.s(44.0) - self.s(16.0);
        // (name, bold ranges, kind label) of each row.
        type Row<'a> = (String, Vec<(usize, usize)>, &'a str);
        let rows: Vec<Row> = self
            .hits
            .iter()
            .filter_map(|h| {
                let c = self.catalog.get(h.index)?;
                let kind = if c.kind == Kind::App { self.labels.kind_app.as_str() } else { self.labels.kind_system.as_str() };
                Some((c.name.clone(), h.ranges.clone(), kind))
            })
            .collect();
        let armed = self.armed_label();
        let width = self.width();
        let Some(t) = self.text.as_mut() else { return };
        self.query_text = (!self.edit.text().is_empty()).then(|| t.cache.layout(self.edit.text(), &t.query, field_room).ok()).flatten();
        self.hint_text = t.cache.layout(&self.labels.hint, &t.query, field_room).ok();
        self.keys_text = t.cache.layout(&self.labels.keys, &t.small, width).ok();
        self.empty_text = t.cache.layout(&self.labels.no_results, &t.regular, row_w).ok();
        self.rows = rows
            .iter()
            .map(|(name, ranges, kind)| {
                let kind = t.cache.layout(kind, &t.small, row_w / 3.0).ok();
                let kind_w = kind.as_ref().map_or(0.0, |k| k.width() + pad_x);
                RowText { name: t.cache.layout_ranges(name, &t.regular, (name_w - kind_w).max(1.0), ranges).ok(), kind }
            })
            .collect();
        self.armed_text = armed.and_then(|(index, label)| t.cache.layout(&label, &t.regular, row_w).ok().map(|l| (index, l)));
    }

    /// The armed row's catalog index and its "press again" label.
    fn armed_label(&self) -> Option<(usize, String)> {
        let index = self.confirm_index()?;
        let label = match self.catalog.get(index)?.kind {
            Kind::System(SystemCommand::ShutDown) => &self.labels.press_again_shut_down,
            Kind::System(SystemCommand::Restart) => &self.labels.press_again_restart,
            _ => return None,
        };
        Some((index, label.clone()))
    }

    /// The catalog index the confirmation is armed on, ignoring its expiry (drawing follows the
    /// wake timer, which fires at the expiry and repaints).
    fn confirm_index(&self) -> Option<usize> {
        let at = self.confirm.expires_at()?;
        self.confirm.armed(at.saturating_sub(1))
    }

    fn queried(&mut self, now: u64) -> PanelReply {
        self.rank();
        self.blink_from = now;
        self.pill_y = Tween::at_rest(self.row_rect(0).y);
        self.hover_o = Tween::at_rest(0.0);
        self.rows_o = Tween::at_rest(0.0);
        self.rows_o.to(1.0, now, theme::ROW_FADE_MS);
        self.rebuild();
        let icons = (!self.hits.is_empty()).then(|| action_id(0, SpotlightOp::Icons));
        PanelReply { repaint: true, resize: true, action: icons, ..Default::default() }
    }

    fn edit(&mut self, e: Edit, now: u64) -> PanelReply {
        if self.edit.apply(e) {
            self.queried(now)
        } else {
            self.blink_from = now;
            self.rebuild();
            PanelReply { repaint: true, ..Default::default() }
        }
    }

    fn select(&mut self, row: usize, now: u64) {
        if self.selected != Some(row) {
            self.confirm.reset();
            self.rebuild();
        }
        self.selected = Some(row);
        self.pill_y.to(self.row_rect(row).y, now, theme::HOVER_GLIDE_MS);
    }

    fn step(&mut self, down: bool, now: u64) -> PanelReply {
        let n = self.hits.len();
        if n == 0 {
            return PanelReply::default();
        }
        let row = match (self.selected, down) {
            (None, _) => 0,
            (Some(r), true) => (r + 1) % n,
            (Some(r), false) => (r + n - 1) % n,
        };
        self.select(row, now);
        PanelReply { repaint: true, ..Default::default() }
    }

    fn selected_candidate(&self) -> Option<(usize, &Candidate)> {
        let index = self.hits.get(self.selected?)?.index;
        Some((index, self.catalog.get(index)?))
    }

    /// Enter or a click on the selected row.
    fn activate(&mut self, now: u64) -> PanelReply {
        let Some((index, c)) = self.selected_candidate() else { return PanelReply::default() };
        let needs_confirm = matches!(c.kind, Kind::System(cmd) if cmd.needs_confirm());
        if self.confirm.press(index, needs_confirm, now) {
            return PanelReply { action: Some(action_id(index, SpotlightOp::Open)), ..Default::default() };
        }
        self.rebuild();
        PanelReply { repaint: true, ..Default::default() }
    }

    fn admin(&self) -> PanelReply {
        match self.selected_candidate() {
            Some((index, c)) if c.can_run_as_admin() => PanelReply { action: Some(action_id(index, SpotlightOp::Admin)), ..Default::default() },
            _ => PanelReply::default(),
        }
    }

    fn menu(&self) -> PanelReply {
        match self.selected_candidate() {
            Some((index, c)) if c.kind == Kind::App => PanelReply { action: Some(action_id(index, SpotlightOp::Menu)), ..Default::default() },
            _ => PanelReply::default(),
        }
    }

    fn set_hover(&mut self, row: Option<usize>, now: u64) -> PanelReply {
        if row == self.hover {
            return PanelReply::default();
        }
        if let Some(r) = row {
            let y = self.row_rect(r).y;
            if self.hover_o.value(now) < 0.05 {
                self.hover_y = Tween::at_rest(y);
            } else {
                self.hover_y.to(y, now, theme::HOVER_GLIDE_MS);
            }
        }
        self.hover_o.to(if row.is_some() { 1.0 } else { 0.0 }, now, theme::HOVER_GLIDE_MS);
        self.hover = row;
        PanelReply { repaint: true, ..Default::default() }
    }

    fn caret_on(&self, now: u64) -> bool {
        caret_visible(self.blink_from, now, self.blink_ms)
    }
}

impl PanelContent for SpotlightView {
    fn size(&self) -> (f32, f32) {
        (self.width(), self.s(theme::SPOTLIGHT_FIELD_H) + self.rows_h() + self.s(theme::SPOTLIGHT_HINTS_H))
    }

    fn paint(&mut self, p: &mut Painter, now: u64) {
        let (w, h) = self.size();
        p.fill_round_rect(RectF::new(0.0, 0.0, w, h), radius_for(h, self.dpi), theme::SPOTLIGHT_SHADE);
        let field_h = self.s(theme::SPOTLIGHT_FIELD_H);
        let mid = field_h / 2.0;
        icons::magnifier(p, PointF { x: self.s(24.0), y: mid }, self.s(18.0), theme::TEXT_INACTIVE, self.s(theme::ICON_STROKE).max(1.0));
        let text_x = self.s(44.0);
        match &self.query_text {
            Some(q) => {
                let y = (mid - q.height() / 2.0).round();
                if self.edit.all_selected() {
                    p.fill_round_rect(RectF::new(text_x - 2.0, y, q.width() + 4.0, q.height()), self.s(4.0), theme::ACTIVE_PILL);
                }
                p.text(q, PointF { x: text_x, y }, theme::TEXT_STRONG);
                if self.caret_on(now) && !self.edit.all_selected() {
                    let x = (text_x + q.caret_x(self.edit.caret_utf16())).round() + 0.5;
                    let half = (q.height() * 0.42).round();
                    p.line(PointF { x, y: mid - half }, PointF { x, y: mid + half }, theme::TEXT_STRONG, self.s(theme::CARET_W).max(1.0));
                }
            }
            None => {
                if let Some(hint) = &self.hint_text {
                    p.text(hint, PointF { x: text_x, y: (mid - hint.height() / 2.0).round() }, theme::TEXT_INACTIVE);
                }
                if self.caret_on(now) {
                    let half = self.hint_text.as_ref().map_or(self.s(10.0), |t| (t.height() * 0.42).round());
                    let x = text_x.round() + 0.5;
                    p.line(PointF { x, y: mid - half }, PointF { x, y: mid + half }, theme::TEXT_STRONG, self.s(theme::CARET_W).max(1.0));
                }
            }
        }
        let rows = self.rows_shown();
        if rows > 0 {
            let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
            let y = field_h.round() + 0.5;
            p.line(PointF { x: pad_x, y }, PointF { x: w - pad_x, y }, theme::SEPARATOR, 1.0);
            let fade = self.rows_o.value(now);
            let base = self.row_rect(0);
            let radius = self.s(theme::POPUP_ITEM_RADIUS);
            let ho = self.hover_o.value(now);
            if ho > 0.0 && self.hover.is_some() && self.hover != self.selected {
                p.fill_round_rect(RectF::new(base.x, self.hover_y.value(now), base.w, base.h), radius, theme::HOVER_PILL.with_alpha(theme::HOVER_PILL.a * ho));
            }
            if self.selected.is_some() {
                p.fill_round_rect(RectF::new(base.x, self.pill_y.value(now), base.w, base.h), radius, theme::ACTIVE_PILL.with_alpha(theme::ACTIVE_PILL.a * fade));
            }
            if self.hits.is_empty()
                && let Some(t) = &self.empty_text
            {
                p.text(t, PointF { x: base.x + pad_x, y: (base.y + (base.h - t.height()) / 2.0).round() }, theme::TEXT_INACTIVE.with_alpha(fade));
            }
            let armed = self.armed_text.as_ref().filter(|_| self.confirm.armed(now).is_some());
            let name_x = self.name_x();
            for (r, row) in self.rows.iter().enumerate() {
                let rect = self.row_rect(r);
                let strong = self.selected == Some(r);
                let index = self.hits.get(r).map(|h| h.index);
                if let Some((img, at)) = index.and_then(|i| self.icons.get(&i)) {
                    let dst = icon_rect(rect.x + pad_x, rect.y + rect.h / 2.0, self.dpi);
                    p.icon(&mut self.icon_bitmaps, img, dst, icon_fade(*at, now) * fade);
                }
                let name = match armed {
                    Some((i, label)) if Some(*i) == index => Some(label),
                    _ => row.name.as_ref(),
                };
                if let Some(t) = name {
                    let color = if strong { theme::TEXT_STRONG } else { theme::TEXT };
                    p.text(t, PointF { x: rect.x + name_x, y: (rect.y + (rect.h - t.height()) / 2.0).round() }, color.with_alpha(fade));
                }
                if let Some(k) = &row.kind {
                    let at = PointF { x: rect.right() - pad_x - k.width(), y: (rect.y + (rect.h - k.height()) / 2.0).round() };
                    p.text(k, at, theme::TEXT_INACTIVE.with_alpha(fade));
                }
            }
        }
        if let Some(k) = &self.keys_text {
            let top = field_h + self.rows_h();
            let hints_h = self.s(theme::SPOTLIGHT_HINTS_H);
            p.text(k, PointF { x: ((w - k.width()) / 2.0).round(), y: (top + (hints_h - k.height()) / 2.0 - self.s(2.0)).round() }, theme::TEXT_INACTIVE);
        }
    }

    fn input(&mut self, ev: PanelInput, now: u64) -> PanelReply {
        if let Some(e) = edit_for(&ev) {
            return self.edit(e, now);
        }
        match ev {
            PanelInput::Char(_) => PanelReply::default(),
            PanelInput::Key(vk) => match vk {
                k if k == VK_UP.0 => self.step(false, now),
                k if k == VK_DOWN.0 => self.step(true, now),
                k if k == VK_RETURN.0 => self.activate(now),
                _ => PanelReply::default(),
            },
            PanelInput::CtrlKey(vk) => match vk {
                k if k == VK_RETURN.0 => self.admin(),
                _ => PanelReply::default(),
            },
            PanelInput::ContextMenu(at) => {
                if let Some(r) = at.and_then(|p| self.hit_row(p)) {
                    self.select(r, now);
                }
                self.menu()
            }
            PanelInput::RightRelease(p) => match self.hit_row(p) {
                Some(r) => {
                    self.select(r, now);
                    PanelReply { repaint: true, ..self.menu() }
                }
                None => PanelReply::default(),
            },
            PanelInput::Release(p) => match self.hit_row(p) {
                Some(r) => {
                    self.select(r, now);
                    PanelReply { repaint: true, ..self.activate(now) }
                }
                None => PanelReply::default(),
            },
            PanelInput::Move(p) => self.set_hover(self.hit_row(p), now),
            PanelInput::Leave => self.set_hover(None, now),
            PanelInput::Press(_) | PanelInput::Wheel(_) => PanelReply::default(),
        }
    }

    fn animating(&self, now: u64) -> bool {
        [self.pill_y, self.hover_y, self.hover_o, self.rows_o].iter().any(|t| t.running(now))
            || self.icons.values().any(|(_, at)| now.saturating_sub(*at) < ICON_FADE_MS)
    }

    fn wake_at(&self, now: u64) -> Option<u64> {
        let blink = next_flip(self.blink_from, now, self.blink_ms);
        let confirm = self.confirm.expires_at().filter(|&at| at > now);
        match (blink, confirm) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}
