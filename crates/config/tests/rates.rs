use std::collections::BTreeMap;

use newwin_config::rates::{load, to_toml};
use newwin_core::currency::Rates;

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("newwin-rates-{}-{name}.toml", std::process::id()))
}

#[test]
fn round_trip() {
    let per_uah: BTreeMap<String, f64> = [("UAH", 1.0), ("USD", 41.3012)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let r = Rates { date: "2026-10-27".into(), fetched_at: 1_793_000_000, per_uah };
    let path = temp("rt");
    std::fs::write(&path, to_toml(&r)).unwrap();
    assert_eq!(load(&path), Some(r));
    std::fs::remove_file(&path).ok();
}

#[test]
fn garbage_and_missing_are_none() {
    let path = temp("bad");
    std::fs::write(&path, "this is = not [valid").unwrap();
    assert_eq!(load(&path), None);
    std::fs::write(&path, "date = \"2026-10-27\"\nfetched_at = 5\n[rates]\n").unwrap();
    assert_eq!(load(&path), None, "no rates at all");
    std::fs::write(&path, "date = \"2026-10-27\"\nfetched_at = 5\n[rates]\nUSD = -3.0\nEUR = 44.8\n").unwrap();
    assert_eq!(load(&path).map(|r| r.per_uah.len()), Some(2), "the bad rate is dropped, UAH added");
    std::fs::remove_file(&path).ok();
    assert_eq!(load(std::path::Path::new("Z:/missing/rates.toml")), None);
}

#[test]
fn rates_live_in_local_app_data() {
    let p = newwin_config::rates::rates_path();
    assert!(p.ends_with(r"NewWin\rates.toml"), "{p:?}");
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        assert!(p.starts_with(local));
    }
}
