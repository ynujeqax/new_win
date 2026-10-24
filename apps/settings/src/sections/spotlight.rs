//! Spotlight (spec: M4 §3.4): the Win key, the number of results, and clearing the launch
//! history (a button; the window deletes `usage.toml`).

use newwin_config::Config;

use super::{Control, Row, RowId, RowValue, clamp_to};

pub fn rows(c: &Config) -> Vec<Row> {
    let s = &c.spotlight;
    vec![
        Row::new(RowId::WinKey, Control::Toggle(s.win_key)),
        Row::new(RowId::MaxResults, Control::Slider(i32::try_from(s.max_results).unwrap_or(i32::MAX))),
        Row::new(RowId::ClearHistory, Control::Button),
    ]
}

pub fn apply(c: &mut Config, id: RowId, v: RowValue) {
    match (id, v) {
        (RowId::WinKey, RowValue::Bool(on)) => c.spotlight.win_key = on,
        (RowId::MaxResults, RowValue::Int(v)) => c.spotlight.max_results = clamp_to(id, v).unsigned_abs(),
        _ => {}
    }
}

/// *Clear*: deletes `usage.toml`; a missing file is already clear. The shell sees the file go
/// through its folder watch and forgets its counts.
pub fn clear_history(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::range_of;
    use newwin_ui::slider::Range;

    #[test]
    fn win_key_toggle_maps_to_win_key() {
        let mut c = Config::default();
        apply(&mut c, RowId::WinKey, RowValue::Bool(false));
        assert!(!c.spotlight.win_key);
    }

    #[test]
    fn results_slider_is_3_to_15() {
        assert_eq!(range_of(RowId::MaxResults), Range { min: 3, max: 15, step: 1 });
        let mut c = Config::default();
        apply(&mut c, RowId::MaxResults, RowValue::Int(12));
        assert_eq!(c.spotlight.max_results, 12);
        apply(&mut c, RowId::MaxResults, RowValue::Int(1));
        assert_eq!(c.spotlight.max_results, 3);
    }

    #[test]
    fn rows_are_win_key_results_and_clear() {
        let r = rows(&Config::default());
        let got: Vec<_> = r.iter().map(|r| (r.id, r.control.clone())).collect();
        assert_eq!(got, [(RowId::WinKey, Control::Toggle(true)), (RowId::MaxResults, Control::Slider(8)), (RowId::ClearHistory, Control::Button)]);
    }

    #[test]
    fn clear_deletes_usage_toml_and_a_missing_file_is_fine() {
        let dir = std::env::temp_dir().join(format!("newwin-settings-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("usage.toml");
        std::fs::write(&path, "[launches]\n\"a\" = 3\n").unwrap();
        clear_history(&path).unwrap();
        assert!(!path.exists());
        clear_history(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
