//! Tray (spec: M4 §3.5, §4.4): the apps from `tray_seen.toml` in `tray.order` order, then the
//! rest by name. Each row: grip, the app's name (its last icon tip), its exe in small, and a
//! *Show* toggle. Exe names compare case-insensitively.

use newwin_config::Config;
use newwin_config::tray_seen::{SeenApp, TraySeen};
use newwin_core::items::reorder;
use newwin_core::strings::Strings;
use newwin_ui::reorder_list::ReorderRow;
use newwin_ui::toggle::Toggle;

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The rows with their exe names: `tray.order` first (entries never seen are skipped), then the
/// remaining seen apps by name. On = not in `tray.hidden`.
pub fn rows(c: &Config, seen: &TraySeen) -> Vec<(String, ReorderRow)> {
    let mut ordered: Vec<&SeenApp> = Vec::with_capacity(seen.apps.len());
    for exe in &c.tray.order {
        if let Some(a) = seen.apps.iter().find(|a| same(&a.exe, exe))
            && !ordered.iter().any(|o| same(&o.exe, &a.exe))
        {
            ordered.push(a);
        }
    }
    let mut rest: Vec<&SeenApp> = Vec::new();
    for a in &seen.apps {
        if !ordered.iter().chain(&rest).any(|o| same(&o.exe, &a.exe)) {
            rest.push(a);
        }
    }
    rest.sort_by_cached_key(|a| (label(a).to_lowercase(), a.exe.to_lowercase()));
    ordered.extend(rest);
    ordered
        .into_iter()
        .map(|a| {
            let on = !c.tray.hidden.iter().any(|h| same(h, &a.exe));
            (a.exe.clone(), ReorderRow { label: label(a).to_owned(), detail: Some(a.exe.clone()), on: Toggle::new(on) })
        })
        .collect()
}

/// The app's name; its exe while the icon has had no tip.
fn label(a: &SeenApp) -> &str {
    if a.name.trim().is_empty() { &a.exe } else { &a.name }
}

/// A row dragged from `from` to `to`: `tray.order` becomes the whole list in its new order, then
/// the old entries for apps not in the list (no longer in `tray_seen.toml`), in their old order.
pub fn moved(c: &mut Config, rows_exe: &[String], from: usize, to: usize) {
    if from >= rows_exe.len() || to >= rows_exe.len() {
        return;
    }
    let mut order = rows_exe.to_vec();
    reorder(&mut order, from, to);
    let unseen: Vec<String> = c.tray.order.iter().filter(|o| !rows_exe.iter().any(|r| same(r, o))).cloned().collect();
    order.extend(unseen);
    c.tray.order = order;
}

/// The app's *Show* toggle: off adds its exe to `tray.hidden` once, on removes it.
pub fn toggled(c: &mut Config, exe: &str, on: bool) {
    if on {
        c.tray.hidden.retain(|h| !same(h, exe));
    } else if !c.tray.hidden.iter().any(|h| same(h, exe)) {
        c.tray.hidden.push(exe.to_owned());
    }
}

/// What the list shows when no app has had a tray icon yet.
pub fn empty_hint(rows: &[(String, ReorderRow)], s: &Strings) -> Option<&'static str> {
    rows.is_empty().then_some(s.tray_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use newwin_core::strings::strings_for;

    fn seen(apps: &[(&str, &str)]) -> TraySeen {
        TraySeen { apps: apps.iter().map(|(e, n)| SeenApp { exe: (*e).into(), name: (*n).into() }).collect() }
    }

    fn exes(r: &[(String, ReorderRow)]) -> Vec<&str> {
        r.iter().map(|(e, _)| e.as_str()).collect()
    }

    #[test]
    fn tray_rows_follow_order_then_name() {
        let s = seen(&[("Zed.exe", "Zed"), ("Alpha.exe", "Alpha"), ("Discord.exe", "Discord")]);
        let mut c = Config::default();
        c.tray.order = vec!["Discord.exe".into()];
        let r = rows(&c, &s);
        assert_eq!(exes(&r), ["Discord.exe", "Alpha.exe", "Zed.exe"]);
        let labels: Vec<_> = r.iter().map(|(_, r)| r.label.as_str()).collect();
        assert_eq!(labels, ["Discord", "Alpha", "Zed"]);
        assert_eq!(r[0].1.detail.as_deref(), Some("Discord.exe"), "the detail line is the exe name");
        // Case-insensitive, and an order entry never seen is skipped.
        c.tray.order = vec!["gone.exe".into(), "zed.EXE".into()];
        assert_eq!(exes(&rows(&c, &s)), ["Zed.exe", "Alpha.exe", "Discord.exe"]);
    }

    #[test]
    fn rest_sorts_by_name_ignoring_case_and_a_nameless_app_shows_its_exe() {
        let s = seen(&[("b.exe", "beta"), ("x.exe", ""), ("a.exe", "Alpha")]);
        let r = rows(&Config::default(), &s);
        assert_eq!(exes(&r), ["a.exe", "b.exe", "x.exe"]);
        assert_eq!(r[2].1.label, "x.exe");
    }

    #[test]
    fn hidden_apps_show_off() {
        let s = seen(&[("Discord.exe", "Discord"), ("Steam.exe", "Steam")]);
        let mut c = Config::default();
        c.tray.hidden = vec!["STEAM.exe".into()];
        let on: Vec<_> = rows(&c, &s).iter().map(|(_, r)| r.on.on).collect();
        assert_eq!(on, [true, false]);
    }

    #[test]
    fn hiding_a_tray_app_adds_its_exe_once() {
        let mut c = Config::default();
        toggled(&mut c, "Discord.exe", false);
        toggled(&mut c, "discord.exe", false);
        assert_eq!(c.tray.hidden, ["Discord.exe"]);
        toggled(&mut c, "DISCORD.EXE", true);
        assert!(c.tray.hidden.is_empty());
    }

    #[test]
    fn moving_a_tray_app_writes_the_whole_order() {
        let mut c = Config::default();
        let keys: Vec<String> = ["a.exe", "b.exe", "c.exe"].map(String::from).to_vec();
        moved(&mut c, &keys, 2, 0);
        assert_eq!(c.tray.order, ["c.exe", "a.exe", "b.exe"]);
        moved(&mut c, &keys, 7, 0);
        assert_eq!(c.tray.order, ["c.exe", "a.exe", "b.exe"], "out of range does nothing");
    }

    #[test]
    fn moving_keeps_the_saved_order_of_apps_not_in_the_list() {
        let mut c = Config::default();
        // gone.exe and Old.exe are in the order but not in tray_seen.toml (not listed).
        c.tray.order = ["gone.exe", "b.exe", "Old.exe", "a.exe"].map(String::from).to_vec();
        let keys: Vec<String> = ["b.exe", "a.exe", "c.exe"].map(String::from).to_vec();
        moved(&mut c, &keys, 2, 0);
        assert_eq!(c.tray.order, ["c.exe", "b.exe", "a.exe", "gone.exe", "Old.exe"]);
        // Case-insensitive: a listed app is not kept twice.
        c.tray.order = vec!["A.EXE".into(), "x.exe".into()];
        moved(&mut c, &keys, 0, 1);
        assert_eq!(c.tray.order, ["a.exe", "b.exe", "c.exe", "x.exe"]);
    }

    #[test]
    fn empty_tray_list_shows_the_hint() {
        let en = strings_for("en-US");
        let r = rows(&Config::default(), &TraySeen::default());
        assert!(r.is_empty());
        assert_eq!(empty_hint(&r, en), Some("Apps appear here after they put an icon in the tray."));
        let one = rows(&Config::default(), &seen(&[("a.exe", "A")]));
        assert_eq!(empty_hint(&one, en), None);
    }
}
