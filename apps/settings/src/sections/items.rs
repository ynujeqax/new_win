//! Items (spec: M4 §3.3): the bar's six right-hand items in `items.right` order, each with a grip
//! and an on/off toggle. A hidden item keeps its place (`items.hidden`), so it comes back there.

use newwin_config::Config;
use newwin_core::items::reorder;
use newwin_core::strings::Strings;
use newwin_ui::reorder_list::ReorderRow;
use newwin_ui::toggle::Toggle;

/// An item's name in the list; an unknown name shows as itself.
pub fn display_name(name: &str, s: &Strings) -> String {
    match name {
        "media" => s.item_media,
        "layout" => s.item_layout,
        "network" => s.item_network,
        "volume" => s.item_volume,
        "search" => s.item_search,
        "date" => s.item_date,
        "time" => s.item_time,
        other => other,
    }
    .to_owned()
}

/// One row per entry of `items.right`, in that order; on = not hidden.
pub fn rows(c: &Config, s: &Strings) -> Vec<ReorderRow> {
    c.items
        .right
        .iter()
        .map(|n| ReorderRow { label: display_name(n, s), detail: None, on: Toggle::new(!c.items.hidden.contains(n)) })
        .collect()
}

/// The row keys (the item names), aligned with [`rows`].
pub fn keys(c: &Config) -> Vec<String> {
    c.items.right.clone()
}

/// A row dragged from `from` to `to`; out-of-range indices do nothing.
pub fn moved(c: &mut Config, from: usize, to: usize) {
    reorder(&mut c.items.right, from, to);
}

/// Row `index` switched on or off; the item stays in `items.right` either way.
pub fn toggled(c: &mut Config, index: usize, on: bool) {
    let Some(name) = c.items.right.get(index).cloned() else { return };
    c.items.hidden.retain(|h| *h != name);
    if !on {
        c.items.hidden.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newwin_core::strings::strings_for;

    #[test]
    fn hiding_an_item_keeps_its_place() {
        let mut c = Config::default();
        toggled(&mut c, 3, false);
        assert_eq!(c.items.right[3], "volume");
        assert_eq!(c.items.hidden, ["volume"]);
        toggled(&mut c, 3, false);
        assert_eq!(c.items.hidden, ["volume"], "hidden once");
        toggled(&mut c, 3, true);
        assert!(c.items.hidden.is_empty());
    }

    #[test]
    fn moving_an_item_rewrites_right() {
        let mut c = Config::default();
        moved(&mut c, 6, 0);
        assert_eq!(c.items.right[0], "time");
        assert_eq!(c.items.right, ["time", "media", "layout", "network", "volume", "search", "date"]);
        moved(&mut c, 9, 0);
        toggled(&mut c, 9, false);
        assert_eq!(c.items.right[0], "time", "out of range does nothing");
        assert!(c.items.hidden.is_empty());
    }

    #[test]
    fn rows_follow_right_and_show_hidden_ones_off() {
        let en = strings_for("en-US");
        let mut c = Config::default();
        c.items.hidden = vec!["search".into()];
        let r = rows(&c, en);
        let labels: Vec<_> = r.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Now playing", "Keyboard layout", "Network", "Volume", "Search", "Date", "Time"]);
        let on: Vec<_> = r.iter().map(|r| r.on.on).collect();
        assert_eq!(on, [true, true, true, true, false, true, true]);
        assert!(r.iter().all(|r| r.detail.is_none()));
        assert_eq!(keys(&c), c.items.right);
    }
}
