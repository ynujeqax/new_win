use std::path::Path;

use newwin_platform::taskbar::{NativeTaskbar, native_taskbar_visible, restore_from_state_file};

#[test]
fn restore_from_missing_state_file_is_noop() {
    assert!(!restore_from_state_file(Path::new("Z:/definitely/missing-taskbar-state.toml")).unwrap());
}

/// Touches the real taskbar; run with `--include-ignored`.
#[test]
#[ignore]
fn hide_then_restore_roundtrip() {
    let state = std::env::temp_dir().join(format!("newwin-taskbar-test-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&state);
    assert!(native_taskbar_visible());

    let tb = NativeTaskbar::hide(&state).unwrap();
    assert!(!native_taskbar_visible());
    assert!(state.exists());

    tb.restore().unwrap();
    assert!(native_taskbar_visible());
    assert!(!state.exists());
}

/// A hide that was never restored (crash) is healed by the state file.
#[test]
#[ignore]
fn state_file_heals_an_unrestored_hide() {
    let state = std::env::temp_dir().join(format!("newwin-taskbar-heal-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&state);

    let tb = NativeTaskbar::hide(&state).unwrap();
    std::mem::forget(tb);
    assert!(!native_taskbar_visible());

    assert!(restore_from_state_file(&state).unwrap());
    assert!(native_taskbar_visible());
    assert!(!state.exists());
}
