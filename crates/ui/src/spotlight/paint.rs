//! Laying out and drawing Spotlight: the field, the rows with their icon column, the key hints.

use newwin_core::app_icon::{ICON_LOGICAL_PX, fade as icon_fade};
use newwin_core::calc::AnswerKind;
use newwin_core::links::Target;
use newwin_core::spotlight::Kind;

use super::{HintKind, Row, RowText, SpotlightView, is_emoji_query};
use crate::icon_bitmaps::icon_rect;
use crate::panel::radius_for;
use crate::{Painter, PointF, RectF, TextStyle, icons, theme};

/// What a row draws in its icon column.
enum Glyph {
    Equals,
    Emoji,
    AppIcon(usize),
    Gear,
    Link(Target),
    Globe,
    None,
}

impl SpotlightView {
    fn kind_label(&self, row: &Row) -> String {
        let l = &self.labels;
        match row {
            Row::Answer(a) if a.kind == AnswerKind::Math => l.kind_calculator.clone(),
            Row::Answer(_) => l.kind_conversion.clone(),
            Row::Currency { date, .. } => {
                let months: [&str; 12] = std::array::from_fn(|i| l.months_short[i].as_str());
                match newwin_core::currency::short_date(date, &months) {
                    d if d.is_empty() => l.kind_currency.clone(),
                    d => format!("{} · {d}", l.kind_currency),
                }
            }
            Row::Emoji(_) => l.kind_emoji.clone(),
            Row::Web => l.kind_web.clone(),
            Row::Hit(h) => match self.catalog.get(h.index).map(|c| &c.kind) {
                Some(Kind::App) => l.kind_app.clone(),
                Some(Kind::SettingsPage(_)) => l.kind_settings_page.clone(),
                Some(Kind::Link(_)) => self.catalog.get(h.index).and_then(|c| self.extras.links.get(&c.name)).map(|(label, _)| label.clone()).unwrap_or_default(),
                Some(Kind::System(_)) => l.kind_system.clone(),
                None => String::new(),
            },
        }
    }

    fn glyph(&self, row: &Row) -> Glyph {
        match row {
            Row::Answer(_) | Row::Currency { .. } => Glyph::Equals,
            Row::Emoji(_) => Glyph::Emoji,
            Row::Web => Glyph::Globe,
            Row::Hit(h) => match self.catalog.get(h.index).map(|c| &c.kind) {
                Some(Kind::App) => Glyph::AppIcon(h.index),
                Some(Kind::SettingsPage(_)) => Glyph::Gear,
                Some(Kind::Link(_)) => Glyph::Link(self.catalog.get(h.index).and_then(|c| self.extras.links.get(&c.name)).map_or(Target::Url, |(_, t)| *t)),
                _ => Glyph::None,
            },
        }
    }

    /// Lays out the text after the query, the rows or the confirmation changed.
    pub(super) fn rebuild(&mut self) {
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let row_w = self.row_rect(0).w - 2.0 * pad_x;
        let name_w = self.row_rect(0).w - self.name_x() - pad_x;
        let field_room = self.width() - self.s(44.0) - self.s(16.0);
        let uk = self.extras.uk;
        let query = self.edit.text().trim().to_string();
        // (glyph, name, bold ranges, strong, kind label) of each row.
        type Laid = (Option<&'static str>, String, Vec<(usize, usize)>, bool, String);
        let rows: Vec<Laid> = self
            .rows
            .iter()
            .map(|row| {
                let kind = self.kind_label(row);
                match row {
                    Row::Answer(a) => (None, a.text.clone(), Vec::new(), true, kind),
                    Row::Currency { text, .. } => (None, text.clone(), Vec::new(), true, kind),
                    Row::Emoji(e) => (Some(e.glyph), (if uk { e.uk } else { e.en }).to_string(), Vec::new(), false, kind),
                    Row::Hit(h) => (None, self.catalog.get(h.index).map(|c| c.name.clone()).unwrap_or_default(), h.ranges.clone(), false, kind),
                    Row::Web => {
                        let engine = self.extras.engine.map_or("", |e| e.name());
                        (None, self.labels.web_row.replace("{engine}", engine).replace("{query}", &query), Vec::new(), false, kind)
                    }
                }
            })
            .collect();
        let armed = self.armed_label();
        let width = self.width();
        let empty = if is_emoji_query(self.edit.text()) && self.edit.text().trim() == ":" { &self.labels.emoji_hint } else { &self.labels.no_results };
        let keys = [&self.labels.keys, &self.labels.keys_open, &self.labels.keys_copy, &self.labels.keys_search];
        let Some(t) = self.text.as_mut() else { return };
        self.query_text = (!self.edit.text().is_empty()).then(|| t.cache.layout(self.edit.text(), &t.query, field_room).ok()).flatten();
        self.hint_text = t.cache.layout(&self.labels.hint, &t.query, field_room).ok();
        self.keys_text = keys.map(|k| t.cache.layout(k, &t.small, width).ok());
        self.empty_text = t.cache.layout(empty, &t.regular, row_w).ok();
        // An answer reads first: a size up from the names, between them and the query.
        let bold = TextStyle { weight: theme::WEIGHT_SEMIBOLD, size: (t.regular.size + t.query.size) / 2.0, ..t.regular.clone() };
        self.row_text = rows
            .iter()
            .map(|(glyph, name, ranges, strong, kind)| {
                let kind = t.cache.layout(kind, &t.small, row_w / 3.0).ok();
                let kind_w = kind.as_ref().map_or(0.0, |k| k.width() + pad_x);
                let room = (name_w - kind_w).max(1.0);
                let name = if *strong { t.cache.layout(name, &bold, room) } else { t.cache.layout_ranges(name, &t.regular, room, ranges) };
                RowText { glyph: glyph.and_then(|g| t.cache.layout(g, &t.glyph, row_w).ok()), name: name.ok(), kind }
            })
            .collect();
        self.armed_text = armed.and_then(|(index, label)| t.cache.layout(&label, &t.regular, row_w).ok().map(|l| (index, l)));
    }

    pub(super) fn paint_view(&mut self, p: &mut Painter, now: u64) {
        use crate::panel::PanelContent;
        let (w, h) = self.size();
        p.fill_round_rect(RectF::new(0.0, 0.0, w, h), radius_for(h, self.dpi), theme::SPOTLIGHT_SHADE);
        let field_h = self.s(theme::SPOTLIGHT_FIELD_H);
        let mid = field_h / 2.0;
        let stroke = self.s(theme::ICON_STROKE).max(1.0);
        icons::magnifier(p, PointF { x: self.s(24.0), y: mid }, self.s(18.0), theme::TEXT_INACTIVE, stroke);
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
        if self.rows_shown() > 0 {
            self.paint_rows(p, now, w);
        }
        if let Some(k) = &self.keys_text[self.hint_kind() as usize] {
            let top = field_h + self.rows_h();
            let hints_h = self.s(theme::SPOTLIGHT_HINTS_H);
            p.text(k, PointF { x: ((w - k.width()) / 2.0).round(), y: (top + (hints_h - k.height()) / 2.0 - self.s(2.0)).round() }, theme::TEXT_INACTIVE);
        }
    }

    fn paint_rows(&mut self, p: &mut Painter, now: u64, w: f32) {
        let pad_x = self.s(theme::POPUP_ITEM_PAD_X);
        let field_h = self.s(theme::SPOTLIGHT_FIELD_H);
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
        if self.total_rows() == 0
            && let Some(t) = &self.empty_text
        {
            p.text(t, PointF { x: base.x + pad_x, y: (base.y + (base.h - t.height()) / 2.0).round() }, theme::TEXT_INACTIVE.with_alpha(fade));
        }
        let armed = self.armed_text.as_ref().filter(|_| self.confirm.armed(now).is_some()).map(|(i, l)| (*i, l));
        let name_x = self.name_x();
        let icon = self.s(ICON_LOGICAL_PX);
        let stroke = self.s(theme::ICON_STROKE).max(1.0);
        for r in 0..self.row_text.len().min(self.rows.len()) {
            let rect = self.row_rect(r);
            let strong = self.selected == Some(r);
            let c = PointF { x: rect.x + pad_x + icon / 2.0, y: rect.y + rect.h / 2.0 };
            let glyph_color = if strong { theme::TEXT_STRONG } else { theme::TEXT_INACTIVE }.with_alpha(fade);
            match self.glyph(&self.rows[r]) {
                Glyph::Equals => {
                    let (d, gap) = (self.s(5.0), self.s(2.5));
                    for dy in [-gap, gap] {
                        p.line(PointF { x: c.x - d, y: c.y + dy }, PointF { x: c.x + d, y: c.y + dy }, theme::TEXT_STRONG.with_alpha(fade), stroke + 0.3);
                    }
                }
                Glyph::Emoji => {
                    if let Some(g) = &self.row_text[r].glyph {
                        p.text_color(g, PointF { x: (c.x - g.width() / 2.0).round(), y: (c.y - g.height() / 2.0).round() }, fade);
                    }
                }
                Glyph::AppIcon(index) => {
                    if let Some((img, at)) = self.icons.get(&index) {
                        let dst = icon_rect(rect.x + pad_x, rect.y + rect.h / 2.0, self.dpi);
                        p.icon(&mut self.icon_bitmaps, img, dst, icon_fade(*at, now) * fade);
                    }
                }
                Glyph::Gear => icons::gear(p, c, icon, glyph_color, stroke),
                Glyph::Link(Target::Url) => icons::link(p, c, icon, glyph_color, stroke),
                Glyph::Link(Target::Path) => icons::folder(p, c, icon, glyph_color, stroke),
                Glyph::Globe => icons::globe(p, c, icon, glyph_color, stroke),
                Glyph::None => {}
            }
            let index = match &self.rows[r] {
                Row::Hit(h) => Some(h.index),
                _ => None,
            };
            let row = &self.row_text[r];
            let name = match &armed {
                Some((i, label)) if Some(*i) == index => Some(*label),
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
}

// HintKind indexes `keys_text`.
const _: () = assert!(HintKind::App as usize == 0 && HintKind::Open as usize == 1 && HintKind::Copy as usize == 2 && HintKind::Search as usize == 3);
