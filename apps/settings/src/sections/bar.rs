//! Bar (spec: M4 §3.1): *Behavior* (mode, delays, trigger strip, full-screen) and *System*
//! (start with Windows).

use newwin_config::{BarMode, Config};

use super::{Control, Group, Row, RowId, RowValue, clamp_to};

pub fn rows(c: &Config) -> Vec<Row> {
    let b = &c.bar;
    vec![
        Row::new(RowId::Mode, Control::Choice(usize::from(b.mode == BarMode::Pinned))).opens(Group::Behavior),
        Row::new(RowId::ShowDelay, Control::Slider(i32::try_from(b.show_delay_ms).unwrap_or(i32::MAX))),
        Row::new(RowId::HideDelay, Control::Slider(i32::try_from(b.hide_delay_ms).unwrap_or(i32::MAX))),
        Row::new(RowId::TriggerStrip, Control::Choice(usize::from(b.trigger_height_px >= 2))),
        Row::new(RowId::IgnoreFullscreen, Control::Toggle(b.ignore_fullscreen)),
        Row::new(RowId::StartWithWindows, Control::Toggle(c.system.start_with_windows)).opens(Group::System),
    ]
}

pub fn apply(c: &mut Config, id: RowId, v: RowValue) {
    let b = &mut c.bar;
    match (id, v) {
        (RowId::Mode, RowValue::Choice(i)) => b.mode = if i == 1 { BarMode::Pinned } else { BarMode::AutoHide },
        (RowId::ShowDelay, RowValue::Int(v)) => b.show_delay_ms = clamp_to(id, v).unsigned_abs(),
        (RowId::HideDelay, RowValue::Int(v)) => b.hide_delay_ms = clamp_to(id, v).unsigned_abs(),
        (RowId::TriggerStrip, RowValue::Choice(i)) => b.trigger_height_px = if i == 1 { 2 } else { 1 },
        (RowId::IgnoreFullscreen, RowValue::Bool(on)) => b.ignore_fullscreen = on,
        (RowId::StartWithWindows, RowValue::Bool(on)) => c.system.start_with_windows = on,
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
    fn mode_segment_maps_to_bar_mode() {
        assert_eq!(applied(RowId::Mode, RowValue::Choice(1)).bar.mode, BarMode::Pinned);
        let mut c = Config::default();
        c.bar.mode = BarMode::Pinned;
        apply(&mut c, RowId::Mode, RowValue::Choice(0));
        assert_eq!(c.bar.mode, BarMode::AutoHide);
    }

    #[test]
    fn delay_slider_range_is_0_to_1000_step_10() {
        assert_eq!(range_of(RowId::ShowDelay), Range { min: 0, max: 1000, step: 10 });
        assert_eq!(applied(RowId::ShowDelay, RowValue::Int(250)).bar.show_delay_ms, 250);
        assert_eq!(applied(RowId::ShowDelay, RowValue::Int(5000)).bar.show_delay_ms, 1000);
        assert_eq!(applied(RowId::ShowDelay, RowValue::Int(-3)).bar.show_delay_ms, 0);
    }

    #[test]
    fn hide_delay_slider_is_0_to_2000_step_10() {
        assert_eq!(range_of(RowId::HideDelay), Range { min: 0, max: 2000, step: 10 });
        assert_eq!(applied(RowId::HideDelay, RowValue::Int(1500)).bar.hide_delay_ms, 1500);
        assert_eq!(applied(RowId::HideDelay, RowValue::Int(9999)).bar.hide_delay_ms, 2000);
    }

    #[test]
    fn trigger_strip_segment_is_one_or_two_px() {
        assert_eq!(applied(RowId::TriggerStrip, RowValue::Choice(1)).bar.trigger_height_px, 2);
        assert_eq!(applied(RowId::TriggerStrip, RowValue::Choice(0)).bar.trigger_height_px, 1);
    }

    #[test]
    fn fullscreen_toggle_maps_to_ignore_fullscreen() {
        assert!(!applied(RowId::IgnoreFullscreen, RowValue::Bool(false)).bar.ignore_fullscreen);
    }

    #[test]
    fn start_with_windows_toggle_maps_to_system() {
        assert!(!applied(RowId::StartWithWindows, RowValue::Bool(false)).system.start_with_windows);
    }

    #[test]
    fn rows_show_the_config_in_spec_order() {
        let mut c = Config::default();
        c.bar.mode = BarMode::Pinned;
        c.bar.trigger_height_px = 2;
        let r = rows(&c);
        let ids: Vec<_> = r.iter().map(|r| r.id).collect();
        assert_eq!(ids, [RowId::Mode, RowId::ShowDelay, RowId::HideDelay, RowId::TriggerStrip, RowId::IgnoreFullscreen, RowId::StartWithWindows]);
        assert_eq!(r[0].group, Some(Group::Behavior));
        assert_eq!(r[0].control, Control::Choice(1));
        assert_eq!(r[1].control, Control::Slider(150));
        assert_eq!(r[3].control, Control::Choice(1));
        assert_eq!(r[5].group, Some(Group::System));
        assert_eq!(r[5].control, Control::Toggle(true));
    }
}
