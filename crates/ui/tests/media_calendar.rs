use newwin_core::calendar::Month;
use newwin_core::media::{MediaCmd, MediaInfo};
use newwin_ui::calendar_panel::CalendarPanel;
use newwin_ui::media_panel::{MediaPanel, decode, encode};
use newwin_ui::panel::{PanelContent, PanelInput};

const LEFT: u16 = 0x25;
const RIGHT: u16 = 0x27;
const SPACE: u16 = 0x20;
const HOME: u16 = 0x24;

fn playing() -> MediaInfo {
    MediaInfo { title: "Song".into(), artist: "Artist".into(), app: "Spotify.exe".into(), playing: true, can_toggle: true, can_prev: true, can_next: true }
}

#[test]
fn media_buttons_send_their_commands() {
    let mut p = MediaPanel::for_test(playing());
    for c in [MediaCmd::Prev, MediaCmd::PlayPause, MediaCmd::Next] {
        let r = p.input(PanelInput::Release(p.button_center(c)), 0);
        assert_eq!(r.action.and_then(decode), Some(c));
    }
    assert_eq!(p.input(PanelInput::Key(SPACE), 0).action, Some(encode(MediaCmd::PlayPause)));
    assert_eq!(p.input(PanelInput::Key(RIGHT), 0).action, Some(encode(MediaCmd::Next)));
    assert_eq!(p.input(PanelInput::Key(LEFT), 0).action, Some(encode(MediaCmd::Prev)));
}

#[test]
fn disabled_media_buttons_do_nothing() {
    let mut p = MediaPanel::for_test(MediaInfo { can_next: false, ..playing() });
    assert_eq!(p.input(PanelInput::Release(p.button_center(MediaCmd::Next)), 0).action, None);
    assert_eq!(p.input(PanelInput::Key(RIGHT), 0).action, None);
}

#[test]
fn the_calendar_opens_on_today_and_leafs() {
    let mut c = CalendarPanel::for_test((2026, 10, 26), 0);
    assert_eq!(c.shown(), Month { year: 2026, month: 10 });
    c.input(PanelInput::Key(RIGHT), 0);
    c.input(PanelInput::Key(RIGHT), 0);
    c.input(PanelInput::Key(RIGHT), 0);
    assert_eq!(c.shown(), Month { year: 2027, month: 1 }, "across the new year");
    c.input(PanelInput::Wheel(120), 0);
    assert_eq!(c.shown(), Month { year: 2026, month: 12 }, "wheel up: earlier");
    assert!(c.input(PanelInput::Key(HOME), 0).repaint);
    assert_eq!(c.shown(), Month { year: 2026, month: 10 }, "Home: back to today");
    assert!(!c.input(PanelInput::Key(HOME), 0).repaint, "already there");
}

#[test]
fn the_calendar_keeps_its_size_for_every_month() {
    let mut c = CalendarPanel::for_test((2026, 2, 1), 0);
    let size = c.size();
    for _ in 0..14 {
        c.input(PanelInput::Key(RIGHT), 0);
        assert_eq!(c.size(), size, "six weeks always: the panel never jumps");
    }
}
