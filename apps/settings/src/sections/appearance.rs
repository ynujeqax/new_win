//! Appearance (spec: M4 §3.2): *Glass*, *Size*, *Text* (font, size) and *Clock* (date format,
//! seconds).

use newwin_config::Config;

use super::{Control, Group, Row, RowId, RowValue, clamp_to};

/// The date template's placeholder (a `GetDateFormatEx` picture).
pub const DATE_HINT: &str = "ddd d MMM";

pub fn rows(c: &Config) -> Vec<Row> {
    rows_with(c, false)
}

/// `date_custom`: *Custom* is picked though no template has been typed yet.
pub fn rows_with(c: &Config, date_custom: bool) -> Vec<Row> {
    let a = &c.appearance;
    let template = (a.date_format != SYSTEM).then(|| a.date_format.clone());
    let custom = template.is_some() || date_custom;
    let mut rows = vec![
        Row::new(RowId::Glass, Control::Toggle(a.glass)).opens(Group::Glass),
        Row::new(RowId::Height, Control::Slider(i32::try_from(a.height).unwrap_or(i32::MAX))).opens(Group::Size),
        Row::new(RowId::Margin, Control::Slider(i32::try_from(a.margin).unwrap_or(i32::MAX))),
        Row::new(RowId::CornerRadius, Control::Slider(a.corner_radius.round() as i32)),
        Row::new(RowId::Font, Control::Font(a.font_family.clone())).opens(Group::Text),
        Row::new(RowId::FontSize, Control::Slider(a.font_size.round() as i32)),
        Row::new(RowId::DateFormat, Control::Choice(usize::from(custom))).opens(Group::Clock),
    ];
    if custom {
        let text = template.unwrap_or_default();
        rows.push(Row::new(RowId::DateTemplate, Control::Text { text: text.clone(), hint: DATE_HINT }));
        rows.push(Row::new(RowId::DateSample, Control::DateSample(if text.is_empty() { SYSTEM.into() } else { text })));
    }
    rows.push(Row::new(RowId::ShowSeconds, Control::Toggle(a.show_seconds)));
    rows
}

/// `date_format` for the system's own format.
const SYSTEM: &str = "system";

pub fn apply(c: &mut Config, id: RowId, v: RowValue) {
    let a = &mut c.appearance;
    match (id, v) {
        (RowId::Glass, RowValue::Bool(on)) => a.glass = on,
        (RowId::Height, RowValue::Int(v)) => a.height = clamp_to(id, v).unsigned_abs(),
        (RowId::Margin, RowValue::Int(v)) => a.margin = clamp_to(id, v).unsigned_abs(),
        (RowId::CornerRadius, RowValue::Int(v)) => a.corner_radius = clamp_to(id, v) as f32,
        (RowId::Font, RowValue::Text(name)) => {
            let name = name.trim();
            if !name.is_empty() {
                a.font_family = name.to_owned();
            }
        }
        (RowId::FontSize, RowValue::Int(v)) => a.font_size = clamp_to(id, v) as f32,
        // *Custom* without a template keeps the system format (the window shows the field).
        (RowId::DateFormat, RowValue::Choice(0)) => a.date_format = SYSTEM.into(),
        (RowId::DateTemplate, RowValue::Text(t)) => {
            let t = t.trim();
            a.date_format = if t.is_empty() { SYSTEM.into() } else { t.to_owned() };
        }
        (RowId::ShowSeconds, RowValue::Bool(on)) => a.show_seconds = on,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::range_of;
    use newwin_ui::slider::Range;

    fn applied(id: RowId, v: RowValue) -> Config {
        let mut c = Config::default();
        apply(&mut c, id, v);
        c
    }

    #[test]
    fn glass_toggle_maps_to_appearance_glass() {
        assert!(!applied(RowId::Glass, RowValue::Bool(false)).appearance.glass);
    }

    #[test]
    fn size_sliders_have_the_spec_bounds() {
        assert_eq!(range_of(RowId::Height), Range { min: 28, max: 48, step: 1 });
        assert_eq!(range_of(RowId::Margin), Range { min: 0, max: 16, step: 1 });
        assert_eq!(range_of(RowId::CornerRadius), Range { min: 0, max: 24, step: 1 });
        assert_eq!(range_of(RowId::FontSize), Range { min: 10, max: 18, step: 1 });
    }

    #[test]
    fn height_slider_maps_to_height() {
        assert_eq!(applied(RowId::Height, RowValue::Int(40)).appearance.height, 40);
        assert_eq!(applied(RowId::Height, RowValue::Int(99)).appearance.height, 48);
        assert_eq!(applied(RowId::Height, RowValue::Int(0)).appearance.height, 28);
    }

    #[test]
    fn margin_slider_maps_to_margin() {
        assert_eq!(applied(RowId::Margin, RowValue::Int(12)).appearance.margin, 12);
        assert_eq!(applied(RowId::Margin, RowValue::Int(-1)).appearance.margin, 0);
    }

    #[test]
    fn radius_slider_maps_to_corner_radius() {
        assert_eq!(applied(RowId::CornerRadius, RowValue::Int(10)).appearance.corner_radius, 10.0);
        assert_eq!(applied(RowId::CornerRadius, RowValue::Int(30)).appearance.corner_radius, 24.0);
    }

    #[test]
    fn font_pick_maps_to_font_family() {
        assert_eq!(applied(RowId::Font, RowValue::Text("Bahnschrift".into())).appearance.font_family, "Bahnschrift");
        assert_eq!(applied(RowId::Font, RowValue::Text("  ".into())).appearance.font_family, "Segoe UI Variable Text", "no empty family");
    }

    #[test]
    fn font_size_slider_maps_to_font_size() {
        assert_eq!(applied(RowId::FontSize, RowValue::Int(15)).appearance.font_size, 15.0);
        assert_eq!(applied(RowId::FontSize, RowValue::Int(40)).appearance.font_size, 18.0);
    }

    #[test]
    fn date_segment_system_resets_the_format() {
        let mut c = Config::default();
        c.appearance.date_format = "dd.MM.yyyy".into();
        apply(&mut c, RowId::DateFormat, RowValue::Choice(0));
        assert_eq!(c.appearance.date_format, "system");
        let mut c = Config::default();
        apply(&mut c, RowId::DateFormat, RowValue::Choice(1));
        assert_eq!(c.appearance.date_format, "system", "Custom alone keeps the system format until a template is typed");
    }

    #[test]
    fn custom_date_template_is_the_format() {
        assert_eq!(applied(RowId::DateTemplate, RowValue::Text("dd.MM.yyyy".into())).appearance.date_format, "dd.MM.yyyy");
        assert_eq!(applied(RowId::DateTemplate, RowValue::Text(" ddd d MMM ".into())).appearance.date_format, "ddd d MMM");
    }

    #[test]
    fn empty_custom_date_is_system() {
        assert_eq!(applied(RowId::DateTemplate, RowValue::Text("  ".into())).appearance.date_format, "system");
    }

    #[test]
    fn seconds_toggle_maps_to_show_seconds() {
        assert!(applied(RowId::ShowSeconds, RowValue::Bool(true)).appearance.show_seconds);
    }

    #[test]
    fn rows_show_the_template_field_only_for_custom() {
        let c = Config::default();
        let ids: Vec<_> = rows(&c).iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [RowId::Glass, RowId::Height, RowId::Margin, RowId::CornerRadius, RowId::Font, RowId::FontSize, RowId::DateFormat, RowId::ShowSeconds]
        );
        let r = rows_with(&c, true);
        let date = r.iter().find(|r| r.id == RowId::DateFormat).unwrap();
        assert_eq!(date.control, Control::Choice(1));
        let field = r.iter().find(|r| r.id == RowId::DateTemplate).unwrap();
        assert_eq!(field.control, Control::Text { text: String::new(), hint: DATE_HINT });
        let mut c = Config::default();
        c.appearance.date_format = "dd.MM".into();
        let r = rows(&c);
        let ids: Vec<_> = r.iter().map(|r| r.id).collect();
        assert!(ids.windows(3).any(|w| w == [RowId::DateFormat, RowId::DateTemplate, RowId::DateSample]));
        assert!(r.iter().any(|r| r.control == Control::DateSample("dd.MM".into())));
        assert_eq!(r.iter().find(|r| r.id == RowId::Font).unwrap().control, Control::Font("Segoe UI Variable Text".into()));
        assert_eq!(r.iter().find(|r| r.id == RowId::Font).unwrap().group, Some(Group::Text));
    }
}
