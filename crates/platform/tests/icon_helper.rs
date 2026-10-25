use std::io::{BufReader, Cursor};

use newwin_core::app_icon::IconImage;
use newwin_platform::appinfo::ComGuard;
use newwin_platform::icon_helper::{read_reply, read_request, serve, write_reply, write_request};

#[test]
fn a_request_round_trips_with_and_without_a_fallback() {
    let mut buf = Vec::new();
    write_request(&mut buf, r"shell:AppsFolder\Helium.Profile1", Some(r"C:\Apps\helium.exe"), 24).unwrap();
    write_request(&mut buf, r"C:\Windows\explorer.exe", None, 16).unwrap();
    let mut r = BufReader::new(Cursor::new(buf));
    assert_eq!(read_request(&mut r).unwrap(), Some((24, r"shell:AppsFolder\Helium.Profile1".into(), Some(r"C:\Apps\helium.exe".into()))));
    assert_eq!(read_request(&mut r).unwrap(), Some((16, r"C:\Windows\explorer.exe".into(), None)));
    assert_eq!(read_request(&mut r).unwrap(), None, "end of input");
}

#[test]
fn a_source_that_would_break_the_line_is_refused() {
    let mut buf = Vec::new();
    assert!(write_request(&mut buf, "a\nb", None, 16).is_err());
    assert!(write_request(&mut buf, "a", Some("b\tc"), 16).is_err());
    assert!(buf.is_empty(), "nothing half-written");
}

#[test]
fn a_reply_round_trips_with_and_without_an_image() {
    let img = IconImage { width: 2, height: 1, bgra: vec![1, 2, 3, 4, 5, 6, 7, 8] };
    let mut buf = Vec::new();
    write_reply(&mut buf, Some(&img)).unwrap();
    write_reply(&mut buf, None).unwrap();
    let mut r = BufReader::new(Cursor::new(buf));
    assert_eq!(read_reply(&mut r).unwrap(), Some(img));
    assert_eq!(read_reply(&mut r).unwrap(), None);
}

#[test]
fn a_reply_claiming_a_huge_image_is_an_error() {
    let mut r = BufReader::new(Cursor::new(b"9999 9999\n".to_vec()));
    assert!(read_reply(&mut r).is_err());
}

#[test]
fn serve_answers_each_request_in_order() {
    let _com = ComGuard::init();
    let mut input = Vec::new();
    write_request(&mut input, r"C:\Windows\explorer.exe", None, 16).unwrap();
    write_request(&mut input, r"C:\nope\nothing.exe", None, 16).unwrap();
    write_request(&mut input, r"C:\nope\nothing.exe", Some(r"C:\Windows\explorer.exe"), 16).unwrap();
    let mut out = Vec::new();
    serve(BufReader::new(Cursor::new(input)), &mut out);
    let mut r = BufReader::new(Cursor::new(out));
    let first = read_reply(&mut r).unwrap().expect("explorer");
    assert_eq!((first.width, first.height), (16, 16));
    assert_eq!(read_reply(&mut r).unwrap(), None, "no such file");
    assert!(read_reply(&mut r).unwrap().is_some(), "the fallback");
}

/// A helper that never answers (a shell extension stuck in it) is ended, and the worker goes on.
#[test]
fn a_helper_that_never_replies_is_ended_after_the_timeout() {
    use std::time::{Duration, Instant};
    use newwin_platform::icon_helper::IconClient;
    let powershell = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let args = ["-NoProfile", "-Command", "Start-Sleep 60"].map(String::from).to_vec();
    let mut client = IconClient::with_command(powershell, args, Duration::from_millis(1500));
    let started = Instant::now();
    assert_eq!(client.fetch(r"C:\Windows\explorer.exe", None, 16), None);
    assert!(!client.running(), "the hung helper is gone");
    assert!(started.elapsed() < Duration::from_secs(5), "one timeout, no retry: {:?}", started.elapsed());
}
