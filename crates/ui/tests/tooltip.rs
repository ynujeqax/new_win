use newwin_ui::popup::PopupStyle;
use newwin_ui::tooltip::Tooltip;
use newwin_ui::{GlassWindow, Gfx, PointF};

#[test]
fn longer_text_widens_the_tooltip_around_the_same_centre() {
    let gfx = Gfx::new().unwrap();
    let style = PopupStyle { font_family: "Segoe UI Variable Text".into(), font_size_px: 13.0, dpi: 96 };
    let t = Tooltip::show(&gfx, "Ethernet", PointF { x: 400.0, y: 60.0 }, &style).unwrap();
    let a = t.windows()[0].1;
    t.set_text("Ethernet · 192.168.1.10");
    let b = t.windows()[0].1;
    assert!(b.right - b.left > a.right - a.left);
    assert!(((a.left + a.right) - (b.left + b.right)).abs() <= 1, "{a:?} vs {b:?}");
    assert_eq!(a.top, 60);
    assert_eq!(t.radius(), (b.bottom - b.top) as f32 / 2.0, "a capsule");
}
