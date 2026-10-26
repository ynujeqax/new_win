use newwin_core::media::MediaInfo;

fn info(title: &str, artist: &str) -> MediaInfo {
    MediaInfo { title: title.into(), artist: artist.into(), playing: true, ..Default::default() }
}

#[test]
fn labels() {
    assert_eq!(info("Bohemian Rhapsody", "Queen").label(), "Bohemian Rhapsody · Queen");
    assert_eq!(info("Lo-fi beats", "").label(), "Lo-fi beats");
    assert_eq!(info("Queen - Bohemian Rhapsody", "Queen").label(), "Queen - Bohemian Rhapsody", "artist already in the title");
    assert_eq!(info("  Some   video - YouTube ", "Channel").label(), "Some video · Channel");
}

#[test]
fn only_sessions_with_a_title_show() {
    assert!(info("Song", "").shown());
    assert!(!info("  ", "Artist").shown());
    assert!(!MediaInfo::default().shown());
}
