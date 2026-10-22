use newwin_ui::{Gfx, TextCache, TextStyle, theme};

#[test]
fn emphasised_range_is_wider_than_plain() {
    let gfx = Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let style = TextStyle { family: "Segoe UI Variable Text".into(), size: 13.0, weight: theme::WEIGHT_REGULAR };
    let plain = cache.layout_emphasis("Telegram", &style, 1000.0, None).unwrap();
    let bold = cache.layout_emphasis("Telegram", &style, 1000.0, Some((0, 3))).unwrap();
    assert!(bold.width() > plain.width(), "bold {} vs plain {}", bold.width(), plain.width());
    // An out-of-range emphasis is clamped, never an error.
    assert!(cache.layout_emphasis("Tel", &style, 1000.0, Some((2, 50))).is_ok());
}

#[test]
fn ranges_bold_several() {
    let gfx = Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let style = TextStyle { family: "Segoe UI Variable Text".into(), size: 13.0, weight: theme::WEIGHT_REGULAR };
    let plain = cache.layout_ranges("Visual Studio Code", &style, 1000.0, &[]).unwrap();
    let one = cache.layout_ranges("Visual Studio Code", &style, 1000.0, &[(0, 1)]).unwrap();
    let three = cache.layout_ranges("Visual Studio Code", &style, 1000.0, &[(0, 1), (7, 1), (14, 1)]).unwrap();
    assert!(plain.width() < one.width() && one.width() < three.width(), "{} {} {}", plain.width(), one.width(), three.width());
    assert!(cache.layout_ranges("Tel", &style, 1000.0, &[(2, 50), (90, 3)]).is_ok(), "out of range is clamped");
}

#[test]
fn caret_x_grows() {
    let gfx = Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let style = TextStyle { family: "Segoe UI Variable Text".into(), size: 20.0, weight: theme::WEIGHT_REGULAR };
    let t = cache.layout("steam", &style, 1000.0).unwrap();
    assert_eq!(t.caret_x(0), 0.0);
    assert!(t.caret_x(0) < t.caret_x(3) && t.caret_x(3) < t.caret_x(5));
    assert!((t.caret_x(5) - t.width()).abs() < 1.0, "{} vs {}", t.caret_x(5), t.width());
    assert_eq!(t.caret_x(999), t.caret_x(5));
}
