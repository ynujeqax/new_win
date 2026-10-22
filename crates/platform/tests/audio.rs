use newwin_platform::appinfo::ComGuard;
use newwin_platform::audio::AudioService;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

/// Holds on any machine: without audio devices the snapshot is simply `None`.
#[test]
fn snapshot_is_consistent() {
    let _com = ComGuard::init();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "audio").unwrap();
    let service = AudioService::start(notify, WM_APP + 8);
    if let Some(s) = service.snapshot() {
        assert!(s.level <= 100);
        assert!(s.devices.iter().all(|d| !d.name.is_empty() && !d.id.is_empty()));
        if let Some(id) = &s.default_id {
            assert!(s.devices.iter().any(|d| &d.id == id), "default {id} not among {:?}", s.devices);
        }
    }
}
