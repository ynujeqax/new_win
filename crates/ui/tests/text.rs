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
