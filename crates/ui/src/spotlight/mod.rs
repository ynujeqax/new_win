//! Spotlight's content (spec: M3 §2, §5; 1.0.6): a query field, up to `max` result rows and a
//! line of key hints, on the glass `Panel`. The rows are an answer (a calculation, a unit or a
//! currency conversion), the found apps, settings pages, quick links and system commands, and a
//! web search; or emoji for a `:` query. Ranking is `newwin_core`; this decides what a key or click
//! means, and `paint` draws it.

mod paint;

use std::collections::HashMap;
use std::rc::Rc;

use newwin_core::anim::Tween;
use newwin_core::app_icon::{ICON_FADE_MS, ICON_GAP_LOGICAL_PX, ICON_LOGICAL_PX, IconImage, source_for_candidate};
use newwin_core::calc::Answer;
use newwin_core::currency::{Rates, looks_like_currency};
use newwin_core::emoji::Emoji;
use newwin_core::links::Target;
use newwin_core::spotlight::{Candidate, Confirm, Hit, Kind, SystemCommand, rank};
use newwin_core::strings::Strings;
use newwin_core::text_edit::{Edit, TextEdit};
use newwin_core::web::Engine;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_RETURN, VK_UP};

use crate::icon_bitmaps::IconBitmaps;
use crate::panel::{PanelContent, PanelInput, PanelReply};
use crate::popup::PopupStyle;
use crate::text_field::{caret_visible, edit_for, next_flip};
use crate::{Gfx, PointF, RectF, TextCache, TextLayout, TextStyle, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotlightOp {
    Open = 0,
    Admin = 1,
    Menu = 2,
    /// The results changed: the owner looks up their icons ([`SpotlightView::shown`]), and
    /// fetches rates when [`SpotlightView::wants_rates`].
    Icons = 3,
    /// Enter on an answer: the owner copies [`SpotlightView::copy_text`].
    Copy = 4,
    /// Enter on the web row: the owner searches [`SpotlightView::web_query`].
    Web = 5,
    /// Enter on an emoji: the owner copies it and counts [`SpotlightView::selected_emoji`].
    CopyEmoji = 6,
    /// A currency query without fresh rates: the owner fetches them, then calls
    /// [`SpotlightView::set_rates`].
    Refresh = 7,
}

/// Panel action id: `index` is into the catalog the view shows.
pub fn action_id(index: usize, op: SpotlightOp) -> u32 {
    ((index as u32) << 3) | op as u32
}

pub fn decode_action(id: u32) -> (usize, SpotlightOp) {
    let op = match id & 7 {
        1 => SpotlightOp::Admin,
        2 => SpotlightOp::Menu,
        3 => SpotlightOp::Icons,
        4 => SpotlightOp::Copy,
        5 => SpotlightOp::Web,
        6 => SpotlightOp::CopyEmoji,
        7 => SpotlightOp::Refresh,
        _ => SpotlightOp::Open,
    };
    ((id >> 3) as usize, op)
}

/// One result row, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A calculation or a unit conversion.
    Answer(Answer),
    /// A currency conversion, with the rates' date ("2026-10-27").
    Currency { text: String, date: String },
    Emoji(&'static Emoji),
    /// An app, settings page, quick link or system command.
    Hit(Hit),
    /// Search the web for the query.
    Web,
}

/// Which key hints the selected row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintKind {
    /// An app: open, run as administrator.
    App,
    Open,
    Copy,
    Search,
}

/// Rates older than this are refreshed when a currency query is typed.
pub const RATES_STALE_SECS: u64 = 6 * 3600;

/// What Spotlight needs beyond the catalog (spec: 1.0.6).
#[derive(Clone, Debug, Default)]
pub struct SpotlightExtras {
    /// Web search; `None` hides the row.
    pub engine: Option<Engine>,
    pub rates: Option<Rc<Rates>>,
    /// Currency answers and fetching rates.
    pub currency_on: bool,
    /// Emoji named in Ukrainian.
    pub uk: bool,
    /// Now, Unix seconds (to tell stale rates).
    pub now_secs: u64,
    /// Each quick link's label and kind, by `Kind::Link` index.
    pub links: Vec<(String, Target)>,
}

#[derive(Clone, Debug, Default)]
pub struct SpotlightLabels {
    pub hint: String,
    pub no_results: String,
    pub kind_app: String,
    pub kind_system: String,
    pub kind_calculator: String,
    pub kind_conversion: String,
    pub kind_settings_page: String,
    pub kind_web: String,
    pub kind_emoji: String,
    pub kind_currency: String,
    pub months_short: [String; 12],
    /// "Search {engine} for “{query}”".
    pub web_row: String,
    pub emoji_hint: String,
    pub press_again_shut_down: String,
    pub press_again_restart: String,
    pub keys: String,
    pub keys_open: String,
    pub keys_copy: String,
    pub keys_search: String,
}

impl SpotlightLabels {
    pub fn from_strings(s: &Strings) -> SpotlightLabels {
        SpotlightLabels {
            hint: s.search.into(),
            no_results: s.no_results.into(),
            kind_app: s.kind_app.into(),
            kind_system: s.kind_system.into(),
            kind_calculator: s.kind_calculator.into(),
            kind_conversion: s.kind_conversion.into(),
            kind_settings_page: s.kind_settings_page.into(),
            kind_web: s.kind_web.into(),
            kind_emoji: s.kind_emoji.into(),
            kind_currency: s.kind_currency.into(),
            months_short: s.months_short.map(String::from),
            web_row: s.web_search_row.into(),
            emoji_hint: s.emoji_hint.into(),
            press_again_shut_down: s.press_again_shut_down.into(),
            press_again_restart: s.press_again_restart.into(),
            keys: s.spotlight_keys.into(),
            keys_open: s.spotlight_keys_open.into(),
            keys_copy: s.spotlight_keys_copy.into(),
            keys_search: s.spotlight_keys_search.into(),
        }
    }
}

struct Text {
    cache: TextCache,
    query: TextStyle,
    regular: TextStyle,
    small: TextStyle,
    /// Emoji in the icon column.
    glyph: TextStyle,
}

/// A row's laid-out text.
struct RowText {
    /// An emoji's glyph, for the icon column.
    glyph: Option<TextLayout>,
    name: Option<TextLayout>,
    kind: Option<TextLayout>,
}

/// What a row is, to find it again after a re-rank.
#[derive(Clone, Debug, PartialEq)]
enum RowKey {
    Answer,
    Currency,
    Emoji(&'static str),
    Hit(String),
    Web,
}

pub struct SpotlightView {
    catalog: Rc<Vec<Candidate>>,
    launches: HashMap<String, u32>,
    labels: SpotlightLabels,
    extras: SpotlightExtras,
    max: usize,
    dpi: u32,
    blink_ms: Option<u32>,
    text: Option<Text>,
    edit: TextEdit,
    /// The calculation or unit conversion of the query (a currency answer is only a row).
    answer: Option<Answer>,
    hits: Vec<Hit>,
    rows: Vec<Row>,
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
    /// The key hints for each [`HintKind`], in its order.
    keys_text: [Option<TextLayout>; 4],
    empty_text: Option<TextLayout>,
    row_text: Vec<RowText>,
    /// The armed row's label is drawn instead of its name (catalog index, layout).
    armed_text: Option<(usize, TextLayout)>,
    /// App icons by catalog index, with when each arrived (it fades in).
    icons: HashMap<usize, (Rc<IconImage>, u64)>,
    icon_bitmaps: IconBitmaps,
}

impl SpotlightView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        catalog: Rc<Vec<Candidate>>,
        launches: HashMap<String, u32>,
        labels: SpotlightLabels,
        extras: SpotlightExtras,
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
            glyph: TextStyle { family: "Segoe UI Emoji".into(), size: s(ICON_LOGICAL_PX), weight: theme::WEIGHT_REGULAR },
            regular,
        };
        let mut v = Self::build(catalog, launches, labels, extras, style.dpi, max_results, blink_ms, Some(text));
        v.rebuild();
        v
    }

    /// Without text layout (geometry and input only), at 96 DPI.
    pub fn for_test(catalog: Rc<Vec<Candidate>>, max_results: usize) -> SpotlightView {
        Self::for_test_with(catalog, max_results, SpotlightExtras::default())
    }

    pub fn for_test_with(catalog: Rc<Vec<Candidate>>, max_results: usize, extras: SpotlightExtras) -> SpotlightView {
        Self::build(catalog, HashMap::new(), SpotlightLabels::default(), extras, 96, max_results, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        catalog: Rc<Vec<Candidate>>,
        launches: HashMap<String, u32>,
        labels: SpotlightLabels,
        extras: SpotlightExtras,
        dpi: u32,
        max: usize,
        blink_ms: Option<u32>,
        text: Option<Text>,
    ) -> Self {
        SpotlightView {
            catalog,
            launches,
            labels,
            extras,
            max,
            dpi,
            blink_ms,
            text,
            edit: TextEdit::default(),
            answer: None,
            hits: Vec::new(),
            rows: Vec::new(),
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
            keys_text: [None, None, None, None],
            empty_text: None,
            row_text: Vec::new(),
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

    /// The apps, pages, links and commands found, in their order among the rows.
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// The query's calculation or unit conversion.
    pub fn answer(&self) -> Option<&Answer> {
        self.answer.as_ref()
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    fn total_rows(&self) -> usize {
        self.rows.len()
    }

    /// Each shown hit's catalog index and where its icon comes from (none but for apps).
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

    /// What Enter on the selected row copies: an answer or an emoji.
    pub fn copy_text(&self) -> Option<String> {
        match self.rows.get(self.selected?)? {
            Row::Answer(a) => Some(a.text.clone()),
            Row::Currency { text, .. } => Some(text.clone()),
            Row::Emoji(e) => Some(e.glyph.to_string()),
            _ => None,
        }
    }

    pub fn selected_emoji(&self) -> Option<&'static Emoji> {
        match self.rows.get(self.selected?)? {
            Row::Emoji(e) => Some(e),
            _ => None,
        }
    }

    /// The query to search the web for, when the web row is there.
    pub fn web_query(&self) -> Option<String> {
        self.rows.contains(&Row::Web).then(|| self.edit.text().trim().to_string())
    }

    /// A currency query, currencies on, and no rates or stale ones.
    pub fn wants_rates(&self) -> bool {
        let x = &self.extras;
        let q = self.edit.text();
        x.currency_on
            && self.answer.is_none()
            && !is_emoji_query(q)
            && looks_like_currency(q)
            && x.rates.as_ref().is_none_or(|r| x.now_secs.saturating_sub(r.fetched_at) > RATES_STALE_SECS)
    }

    /// New rates arrived: the answer row comes in, the query stays. A selection on the first row
    /// stays on the first row (now the answer); one moved further stays on the same row.
    pub fn set_rates(&mut self, r: Rc<Rates>) -> PanelReply {
        self.extras.rates = Some(r);
        let at_top = self.selected == Some(0);
        let key = self.selected.and_then(|r| self.row_key(r));
        self.rank();
        self.reselect(if at_top { None } else { key });
        PanelReply { repaint: true, resize: true, ..Default::default() }
    }

    /// A new index (apps installed or removed): re-ranks, keeping the same entry selected when it
    /// is still shown.
    pub fn set_catalog(&mut self, c: Rc<Vec<Candidate>>) {
        // Icons are kept by index, which a new catalog changes.
        self.icons.clear();
        let key = self.selected.and_then(|r| self.row_key(r));
        self.catalog = c;
        self.rank();
        self.reselect(key);
    }

    fn reselect(&mut self, key: Option<RowKey>) {
        if let Some(key) = key
            && let Some(r) = (0..self.rows.len()).find(|&r| self.row_key(r).as_ref() == Some(&key))
        {
            self.selected = Some(r);
        }
        self.pill_y = Tween::at_rest(self.selected.map_or(0.0, |r| self.row_rect(r).y));
        self.rebuild();
    }

    fn row_key(&self, row: usize) -> Option<RowKey> {
        Some(match self.rows.get(row)? {
            Row::Answer(_) => RowKey::Answer,
            Row::Currency { .. } => RowKey::Currency,
            Row::Emoji(e) => RowKey::Emoji(e.glyph),
            Row::Hit(h) => RowKey::Hit(self.catalog.get(h.index)?.id.clone()),
            Row::Web => RowKey::Web,
        })
    }

    /// The key hints for the selected row.
    pub fn hint_kind(&self) -> HintKind {
        match self.selected.and_then(|r| self.rows.get(r)) {
            Some(Row::Answer(_) | Row::Currency { .. } | Row::Emoji(_)) => HintKind::Copy,
            Some(Row::Web) => HintKind::Search,
            Some(Row::Hit(h)) if self.catalog.get(h.index).is_some_and(|c| c.kind != Kind::App) => HintKind::Open,
            _ => HintKind::App,
        }
    }

    /// Rows shown below the field: none for an empty query, one for "No results" or the emoji hint.
    fn rows_shown(&self) -> usize {
        if self.edit.text().trim().is_empty() { 0 } else { self.total_rows().max(1) }
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
        (0..self.total_rows()).find(|&r| self.row_rect(r).contains(p))
    }

    fn rank(&mut self) {
        let q = self.edit.text();
        let launches = &self.launches;
        let count = |k: &str| launches.get(k).copied().unwrap_or(0);
        self.answer = None;
        self.hits.clear();
        let mut rows = Vec::new();
        if let Some(name) = q.trim_start().strip_prefix(':') {
            rows.extend(newwin_core::emoji::search(name, self.max, &count).into_iter().map(Row::Emoji));
        } else if !q.trim().is_empty() {
            self.answer = newwin_core::calc::answer(q);
            let x = &self.extras;
            match &self.answer {
                Some(a) => rows.push(Row::Answer(a.clone())),
                None if x.currency_on => {
                    if let Some(r) = &x.rates
                        && let Some(text) = newwin_core::currency::answer(q, r)
                    {
                        rows.push(Row::Currency { text, date: r.date.clone() });
                    }
                }
                None => {}
            }
            let web = x.engine.is_some();
            // The answer and the web row take places among the results, not on top of them.
            let room = self.max.saturating_sub(rows.len() + usize::from(web));
            self.hits = rank(q, &self.catalog, &count, room);
            rows.extend(self.hits.iter().cloned().map(Row::Hit));
            if web {
                rows.push(Row::Web);
            }
        }
        self.rows = rows;
        self.selected = (!self.rows.is_empty()).then_some(0);
        self.hover = None;
        self.confirm.reset();
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
        let action = if !self.hits.is_empty() {
            Some(action_id(0, SpotlightOp::Icons))
        } else if self.wants_rates() {
            Some(action_id(0, SpotlightOp::Refresh))
        } else {
            None
        };
        PanelReply { repaint: true, resize: true, action, ..Default::default() }
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
        let n = self.total_rows();
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
        let Row::Hit(h) = self.rows.get(self.selected?)? else { return None };
        Some((h.index, self.catalog.get(h.index)?))
    }

    /// Enter or a click on the selected row.
    fn activate(&mut self, now: u64) -> PanelReply {
        let op = match self.selected.and_then(|r| self.rows.get(r)) {
            Some(Row::Answer(_) | Row::Currency { .. }) => Some(SpotlightOp::Copy),
            Some(Row::Emoji(_)) => Some(SpotlightOp::CopyEmoji),
            Some(Row::Web) => Some(SpotlightOp::Web),
            _ => None,
        };
        if let Some(op) = op {
            return PanelReply { action: Some(action_id(0, op)), ..Default::default() };
        }
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

fn is_emoji_query(q: &str) -> bool {
    q.trim_start().starts_with(':')
}

impl PanelContent for SpotlightView {
    fn size(&self) -> (f32, f32) {
        (self.width(), self.s(theme::SPOTLIGHT_FIELD_H) + self.rows_h() + self.s(theme::SPOTLIGHT_HINTS_H))
    }

    fn paint(&mut self, p: &mut crate::Painter, now: u64) {
        self.paint_view(p, now);
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
