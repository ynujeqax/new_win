use std::time::{Duration, Instant};

use newwin_core::app_icon::{IconImage, IconKey};
use newwin_platform::app_icon::{hicon_image, shell_icon};
use newwin_platform::appinfo::ComGuard;
use newwin_platform::worker::{Job, Worker, take_icon};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, ICONINFO, IDI_APPLICATION, LoadIconW, MSG, PM_REMOVE, PeekMessageW, WM_APP,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

fn alpha_at(img: &IconImage, x: u32, y: u32) -> u8 {
    img.bgra[((y * img.width + x) * 4 + 3) as usize]
}

#[test]
fn explorer_has_a_16_px_icon() {
    let _com = ComGuard::init();
    let img = shell_icon(r"C:\Windows\explorer.exe", 16).expect("explorer icon");
    assert_eq!((img.width, img.height, img.bgra.len()), (16, 16, 1024));
    assert!(img.bgra.chunks(4).any(|p| p[3] > 0), "something is drawn");
    assert!(img.bgra.chunks(4).any(|p| p[3] == 0), "a transparent corner");
    assert!(img.bgra.chunks(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]), "premultiplied");
}

#[test]
fn a_missing_file_has_no_icon() {
    let _com = ComGuard::init();
    assert_eq!(shell_icon(r"C:\nope\nothing.exe", 16), None);
}

#[test]
fn a_packaged_app_has_an_icon() {
    let _com = ComGuard::init();
    let src = r"shell:AppsFolder\windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel";
    let img = shell_icon(src, 24).expect("Settings icon");
    assert_eq!((img.width, img.height), (24, 24));
}

#[test]
fn the_stock_app_icon_converts() {
    let h = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap();
    let img = hicon_image(h.0 as usize as u32, 32).expect("IDI_APPLICATION");
    assert_eq!((img.width, img.height), (32, 32));
    assert!(img.bgra.chunks(4).any(|p| p[3] > 0));
}

#[test]
fn a_dead_hicon_gives_none() {
    assert_eq!(hicon_image(0x0BAD_BEEF, 16), None);
}

#[test]
fn an_old_mask_icon_is_opaque_where_drawn() {
    // 16x16, 1-bit: the AND mask is 0 (drawn) in the left half and 1 (transparent) in the right;
    // the XOR (colour) bitmap is all white.
    let and: Vec<u8> = (0..16).flat_map(|_| [0x00u8, 0xFF]).collect();
    let xor = [0xFFu8; 32];
    unsafe {
        let mask = CreateBitmap(16, 16, 1, 1, Some(and.as_ptr().cast()));
        let color = CreateBitmap(16, 16, 1, 1, Some(xor.as_ptr().cast()));
        let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let h = CreateIconIndirect(&info).unwrap();
        let img = hicon_image(h.0 as usize as u32, 16).expect("mask icon");
        assert_eq!(alpha_at(&img, 2, 8), 255, "inside the mask");
        assert_eq!(alpha_at(&img, 13, 8), 0, "outside the mask");
        let _ = DestroyIcon(h);
        let _ = DeleteObject(mask.into());
        let _ = DeleteObject(color.into());
    }
}

#[test]
fn worker_answers_an_icon_job() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "icon-worker").unwrap();
    let w = Worker::spawn(notify, WM_APP + 4, WM_APP + 16, WM_APP + 21);
    let key = IconKey::Shell { source: r"C:\Windows\explorer.exe".into(), px: 16 };
    w.submit_job(Job::Icon { key: key.clone(), source: r"C:\Windows\explorer.exe".into(), fallback: None, px: 16 });
    let end = Instant::now() + Duration::from_secs(10);
    let mut m = MSG::default();
    while Instant::now() < end {
        if unsafe { PeekMessageW(&mut m, Some(notify), WM_APP + 21, WM_APP + 21, PM_REMOVE) }.as_bool() {
            let r = unsafe { take_icon(m.lParam) };
            assert_eq!(r.key, key);
            assert!(r.image.is_some());
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no icon posted");
}

#[test]
fn an_icon_job_falls_back_when_the_source_has_none() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "icon-fallback").unwrap();
    let w = Worker::spawn(notify, WM_APP + 4, WM_APP + 16, WM_APP + 21);
    let source = r"shell:AppsFolder\NoSuch.App.Profile7".to_string();
    let key = IconKey::Shell { source: source.clone(), px: 16 };
    w.submit_job(Job::Icon { key: key.clone(), source, fallback: Some(r"C:\Windows\explorer.exe".into()), px: 16 });
    let end = Instant::now() + Duration::from_secs(10);
    let mut m = MSG::default();
    while Instant::now() < end {
        if unsafe { PeekMessageW(&mut m, Some(notify), WM_APP + 21, WM_APP + 21, PM_REMOVE) }.as_bool() {
            let r = unsafe { take_icon(m.lParam) };
            assert_eq!(r.key, key, "kept under the first source's key");
            assert!(r.image.is_some(), "the exe's icon");
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no icon posted");
}
