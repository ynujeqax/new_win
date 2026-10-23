use newwin_core::items::*;
use RightItem::*;

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_keeps_search_skips_unknown_and_duplicates() {
    assert_eq!(parse_right(&names(&["layout", "network", "volume", "search", "date", "time"])), vec![Layout, Network, Volume, Search, Date, Time]);
    assert_eq!(parse_right(&names(&["Time", "bogus", "time", "DATE"])), vec![Time, Date]);
}

#[test]
fn placement_right_to_left_with_separator() {
    let (xs, sep) = place_right(&[(Network, 26.0), (Volume, 26.0), (Date, 80.0), (Time, 40.0)], 1000.0, 2.0, 13.0);
    // Time 960..1000, gap, Date 878..958, gap + separator 863..876 + gap, Volume 835..861, gap, Network 807..833.
    assert_eq!(xs, vec![(Network, 807.0), (Volume, 835.0), (Date, 878.0), (Time, 960.0)]);
    assert_eq!(sep, Some(869.5));
}

#[test]
fn no_separator_without_status_items() {
    let (xs, sep) = place_right(&[(Date, 80.0), (Time, 40.0)], 1000.0, 2.0, 13.0);
    assert_eq!(xs, vec![(Date, 878.0), (Time, 960.0)]);
    assert_eq!(sep, None);
}

#[test]
fn no_separator_when_the_clock_comes_first() {
    let (xs, sep) = place_right(&[(Time, 40.0), (Volume, 26.0)], 1000.0, 2.0, 13.0);
    assert_eq!(xs, vec![(Time, 932.0), (Volume, 974.0)]);
    assert_eq!(sep, None);
}

#[test]
fn search_is_a_status_item_before_the_separator() {
    let (xs, sep) = place_right(&[(Volume, 26.0), (Search, 26.0), (Date, 80.0), (Time, 40.0)], 1000.0, 2.0, 13.0);
    assert!(sep.is_some_and(|s| s > xs[1].1 + 26.0 && s < xs[2].1), "the separator sits between Search and the date: {xs:?} {sep:?}");
}

#[test]
fn reorder_moves_one_item() {
    let mut v = vec![1, 2, 3, 4];
    reorder(&mut v, 0, 2);
    assert_eq!(v, [2, 3, 1, 4]);
    reorder(&mut v, 9, 0);
    assert_eq!(v, [2, 3, 1, 4]);
    reorder(&mut v, 2, 0);
    assert_eq!(v, [1, 2, 3, 4]);
    reorder(&mut v, 1, 9);
    assert_eq!(v, [1, 2, 3, 4]);
    let mut e: Vec<i32> = Vec::new();
    reorder(&mut e, 0, 0);
    assert!(e.is_empty());
}

#[test]
fn drop_index_uses_row_midpoints() {
    assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, 44.0), 1);
    assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, 46.0), 2);
    assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, 200.0), 2);
    assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, -50.0), 0);
    assert_eq!(drop_index(&[], 30.0, 10.0), 0);
    assert_eq!(drop_index(&[0.0, 30.0], 30.0, f32::NAN), 0);
}
