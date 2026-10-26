//! Pictures of the bar's right side for checking by eye: `cargo test -p newwin-ui --test preview
//! -- --ignored`, then open `target/preview/*.bmp`.

use newwin_ui::preview::render_bmp;
use newwin_ui::{Color, Gfx, PointF, RectF, TextCache, TextStyle, icons, theme};

const BG: Color = Color::rgba(14, 14, 16, 1.0);

fn out(name: &str) -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/preview");
    std::fs::create_dir_all(dir).unwrap();
    format!("{dir}/{name}.bmp")
}

#[test]
#[ignore]
fn bar_right_side() {
    let gfx = Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let style = |weight| TextStyle { family: "Segoe UI Variable Text".into(), size: 13.0, weight };
    let lang = cache.layout("UKR", &style(theme::WEIGHT_REGULAR), 200.0).unwrap();
    let date = cache.layout("Mon Oct 26", &style(theme::WEIGHT_REGULAR), 200.0).unwrap();
    let time = cache.layout("11:29 AM", &style(theme::WEIGHT_SEMIBOLD), 200.0).unwrap();
    let (h, pill) = (36.0, theme::PILL_HEIGHT);
    let w = theme::ICON_STROKE;
    render_bmp(&out("right"), 420, 36, BG, |p| {
        let y = (h - pill) / 2.0;
        let mut x = 6.0;
        let text = |p: &mut newwin_ui::Painter, t: &newwin_ui::TextLayout, x: f32, slot: f32, c| {
            p.text(t, PointF { x: (x + (slot - t.width()) / 2.0).round(), y: ((h - t.height()) / 2.0).round() }, c);
        };
        // Hover pill under the speaker, as when pointed at.
        let lang_w = (lang.width() + 10.0).max(pill).round();
        text(p, &lang, x, lang_w, theme::TEXT_INACTIVE);
        x += lang_w + theme::ITEM_GAP;
        icons::wifi(p, PointF { x: x + pill / 2.0, y: h / 2.0 }, 18.0, 3.0, theme::TEXT_INACTIVE, w);
        x += pill + theme::ITEM_GAP;
        p.fill_round_rect(RectF::new(x, y, pill, pill), theme::PILL_RADIUS, theme::HOVER_PILL);
        icons::speaker(p, PointF { x: x + pill / 2.0, y: h / 2.0 }, 18.0, 3.0, 0.0, theme::TEXT_STRONG, w);
        x += pill + theme::ITEM_GAP;
        icons::magnifier(p, PointF { x: x + pill / 2.0, y: h / 2.0 }, 16.0, theme::TEXT_INACTIVE, w);
        x += pill + theme::ITEM_GAP;
        let sx = (x + 6.0).round() + 0.5;
        p.line(PointF { x: sx, y: h / 2.0 - 7.0 }, PointF { x: sx, y: h / 2.0 + 7.0 }, theme::SEPARATOR, 1.0);
        x += 13.0 + theme::ITEM_GAP;
        let dw = date.width() + 2.0 * theme::PILL_PAD_X;
        text(p, &date, x, dw, theme::TEXT);
        x += dw + theme::ITEM_GAP;
        text(p, &time, x, time.width() + 2.0 * theme::PILL_PAD_X, theme::TEXT_STRONG);
    })
    .unwrap();
}

#[test]
#[ignore]
fn icon_sheet() {
    // Every icon at bar size and states, 48 px apart.
    render_bmp(&out("icons"), 48 * 8, 48, BG, |p| {
        let c = |i: f32| PointF { x: 24.0 + 48.0 * i, y: 24.0 };
        let (col, w) = (theme::TEXT_STRONG, theme::ICON_STROKE);
        icons::wifi(p, c(0.0), 18.0, 3.0, col, w);
        icons::wifi(p, c(1.0), 18.0, 1.0, col, w);
        icons::ethernet(p, c(2.0), 18.0, col, w);
        icons::speaker(p, c(3.0), 18.0, 3.0, 0.0, col, w);
        icons::speaker(p, c(4.0), 18.0, 1.0, 0.0, col, w);
        icons::speaker(p, c(5.0), 18.0, 0.0, 1.0, col, w);
        icons::magnifier(p, c(6.0), 16.0, col, w);
        icons::chevron(p, c(7.0), 16.0, 0.0, col, w);
    })
    .unwrap();
}

fn style() -> newwin_ui::popup::PopupStyle {
    newwin_ui::popup::PopupStyle { font_family: "Segoe UI Variable Text".into(), font_size_px: 13.0, dpi: 96 }
}

fn panel(name: &str, content: &mut dyn newwin_ui::panel::PanelContent, now: u64) {
    let (w, h) = content.size();
    render_bmp(&out(name), w.ceil() as u32, h.ceil() as u32, theme::GLASS_TINT_SOLID.with_alpha(1.0), |p| content.paint(p, now)).unwrap();
}

#[test]
#[ignore]
fn volume_panel() {
    use newwin_platform::audio::{AudioDevice, AudioSnapshot};
    use newwin_ui::panel::{PanelContent, PanelInput};
    let gfx = Gfx::new().unwrap();
    let devices = vec![
        AudioDevice { id: "a".into(), name: "Speakers (Realtek USB Audio)".into() },
        AudioDevice { id: "b".into(), name: "Headphones".into() },
    ];
    let snap = AudioSnapshot { level: 64, muted: false, devices, default_id: Some("a".into()) };
    let mut v = newwin_ui::volume_panel::VolumePanel::new(Some(snap), ("No output devices", "Sound settings…"), &style(), &gfx);
    let c = v.device_center(1);
    v.input(PanelInput::Move(c), 0);
    panel("volume", &mut v, 10_000);
}

#[test]
#[ignore]
fn spotlight() {
    use newwin_core::spotlight::{Candidate, Kind};
    use newwin_ui::panel::{PanelContent, PanelInput};
    let gfx = Gfx::new().unwrap();
    let app = |n: &str| Candidate { name: n.into(), id: format!("id:{n}"), kind: Kind::App, file_path: None, packaged: false };
    let catalog = std::rc::Rc::new(vec![app("Steam"), app("Steam Support Center"), app("Settings"), app("Snipping Tool"), app("Services")]);
    let labels = newwin_ui::spotlight::SpotlightLabels {
        hint: "Search apps".into(),
        no_results: "No results".into(),
        kind_app: "App".into(),
        kind_system: "System".into(),
        kind_calculator: "Calculator".into(),
        kind_conversion: "Conversion".into(),
        press_again_shut_down: "Press Enter again to shut down".into(),
        press_again_restart: "Press Enter again to restart".into(),
        keys: "enter open   esc close".into(),
    };
    let mut v = newwin_ui::spotlight::SpotlightView::new(catalog, Default::default(), labels, &style(), 6, None, &gfx);
    for ch in "10 km in mi".chars() {
        v.input(PanelInput::Char(ch), 0);
    }
    panel("spotlight", &mut v, 10_000);
}

#[test]
#[ignore]
fn tray_list() {
    use newwin_ui::tray_list::{TrayList, TrayRow};
    let gfx = Gfx::new().unwrap();
    let row = |l: &str, header| TrayRow { label: l.into(), header, icon: None };
    let rows = vec![row("NewWin", true), row("Steam", false), row("Telegram Desktop", false), row("MSI Afterburner", false), row("Counter-Strike 2", false)];
    let mut t = TrayList::new(rows, "Nothing here".into(), &style(), &gfx);
    panel("tray", &mut t, 10_000);
}

#[test]
#[ignore]
fn shadow_on_a_light_wallpaper() {
    // A Spotlight-sized glass over a pink wallpaper, as in the user's screenshot.
    let bg = Color::rgba(222, 170, 170, 1.0);
    render_bmp(&out("shadow"), 700, 220, bg, |p| {
        let glass = RectF::new(70.0, 60.0, 560.0, 80.0);
        newwin_ui::shadow::paint_shadow(p, glass, 16.0, 28.0, 6.0);
        p.fill_round_rect(glass, 16.0, Color::rgba(150, 100, 104, 1.0));
    })
    .unwrap();
}
