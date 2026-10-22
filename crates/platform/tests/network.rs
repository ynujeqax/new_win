use newwin_core::network::NetKind;
use newwin_platform::network::{NetworkWatch, current};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

/// Holds offline too: then the kind is `None`.
#[test]
fn current_network_is_consistent() {
    let info = current();
    eprintln!("{info:?}");
    match info.kind {
        NetKind::None => assert!(!info.internet),
        NetKind::Ethernet | NetKind::WiFi { .. } => {
            assert!(!info.name.is_empty(), "{info:?}");
            if let Some(ip) = &info.ipv4 {
                assert_eq!(ip.split('.').count(), 4, "{ip}");
            }
        }
    }
    if let NetKind::WiFi { signal } = info.kind {
        assert!(signal <= 100);
    }
}

#[test]
fn watch_starts_and_stops() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "net").unwrap();
    let w = NetworkWatch::start(notify, WM_APP + 9).unwrap();
    drop(w);
}
