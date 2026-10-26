//! Colour emoji and the monochrome glyphs of Spotlight's rows, checked on rendered pixels.

use newwin_ui::preview::render_pixels;
use newwin_ui::{Color, Gfx, Painter, PointF, TextCache, TextStyle, icons, theme};

const BG: Color = Color::rgba(0, 0, 0, 1.0);

fn coloured(px: &[[u8; 3]]) -> usize {
    px.iter().filter(|p| p.iter().max().unwrap() - p.iter().min().unwrap() > 40).count()
}

#[test]
fn emoji_draws_in_colour_and_plain_text_does_not() {
    let gfx = Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let style = TextStyle { family: "Segoe UI Variable Text".into(), size: 24.0, weight: theme::WEIGHT_REGULAR };
    let emoji = cache.layout("😀", &style, 100.0).unwrap();
    let px = render_pixels(40, 40, BG, |p| p.text_color(&emoji, PointF { x: 4.0, y: 4.0 }, 1.0)).unwrap();
    assert!(coloured(&px) > 50, "the emoji is drawn in colour: {} coloured px", coloured(&px));
    let mono = render_pixels(40, 40, BG, |p| p.text(&emoji, PointF { x: 4.0, y: 4.0 }, theme::TEXT_STRONG)).unwrap();
    assert!(coloured(&mono) < 5, "plain text stays monochrome");
    let faded = render_pixels(40, 40, BG, |p| p.text_color(&emoji, PointF { x: 4.0, y: 4.0 }, 0.0)).unwrap();
    assert!(faded.iter().all(|p| *p == [0, 0, 0]), "opacity 0 draws nothing");
}

/// Each glyph at s = 16 around (20, 20) draws something, all of it inside a 20 × 20 box.
#[test]
fn glyphs_stay_inside_their_box() {
    type Glyph = fn(&mut Painter, PointF, f32, Color, f32);
    let glyphs: [(&str, Glyph); 4] = [("gear", icons::gear), ("globe", icons::globe), ("link", icons::link), ("folder", icons::folder)];
    for (name, draw) in glyphs {
        let px = render_pixels(40, 40, BG, |p| draw(p, PointF { x: 20.0, y: 20.0 }, 16.0, theme::TEXT_STRONG, 1.5)).unwrap();
        let lit: Vec<(usize, usize)> = px.iter().enumerate().filter(|(_, p)| p[0] > 30).map(|(i, _)| (i % 40, i / 40)).collect();
        assert!(lit.len() > 20, "{name} draws: {} px", lit.len());
        assert!(lit.iter().all(|&(x, y)| (10..30).contains(&x) && (10..30).contains(&y)), "{name} leaves its box");
        assert_eq!(coloured(&px), 0, "{name} is monochrome");
    }
}
