//! Temporary M3 probe — remove after M3.
//!
//! Probe 3 of the M3 plan: enumerates `shell:AppsFolder`, times it, and prints per item the
//! display name, parsing name and the properties that may give a file path or mark a UWP app.
//! `--runas <parsing name>` then runs that item elevated two ways (AppsFolder path, link target).

use std::time::Instant;

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Storage::EnhancedStorage::{PKEY_Link_TargetParsingPath, PKEY_ParsingPath};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, HSTRING, Interface, PCWSTR, w};

const PKEY_PACKAGE_FULL_NAME: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 21 };

fn prop(item: &IShellItem2, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let p = item.GetString(key).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| !s.is_empty())
    }
}

fn name(item: &IShellItem, sigdn: SIGDN) -> String {
    unsafe {
        let Ok(p) = item.GetDisplayName(sigdn) else { return String::new() };
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

fn runas(file: &str) {
    let file = HSTRING::from(file);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    let r = unsafe { ShellExecuteExW(&mut info) };
    println!("runas {file}: {r:?}");
}

fn main() -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let args: Vec<String> = std::env::args().collect();
    let started = Instant::now();
    let folder: IShellItem = unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)? };
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems)? };
    let mut rows = Vec::new();
    loop {
        let mut got = [None];
        let mut n = 0;
        if unsafe { items.Next(&mut got, Some(&mut n)) }.is_err() || n == 0 {
            break;
        }
        let Some(item) = got[0].take() else { break };
        let item2: Option<IShellItem2> = item.cast().ok();
        let display = name(&item, SIGDN_NORMALDISPLAY);
        let parsing = name(&item, SIGDN_PARENTRELATIVEPARSING);
        let (target, ppath, pkg) = match &item2 {
            Some(i) => (prop(i, &PKEY_Link_TargetParsingPath), prop(i, &PKEY_ParsingPath), prop(i, &PKEY_PACKAGE_FULL_NAME)),
            None => (None, None, None),
        };
        rows.push((display, parsing, target, ppath, pkg));
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    for (d, p, t, pp, pkg) in &rows {
        println!("{d:?}\n    parsing={p:?}\n    link_target={t:?}\n    parsing_path={pp:?}\n    package={pkg:?}");
    }
    let with_target = rows.iter().filter(|r| r.2.is_some()).count();
    let with_pkg = rows.iter().filter(|r| r.4.is_some()).count();
    println!("\n{} items in {ms:.1} ms; link_target on {with_target}, package on {with_pkg}", rows.len());
    if let Some(i) = args.iter().position(|a| a == "--runas") {
        let want = args.get(i + 1).map(String::as_str).unwrap_or("Command Prompt");
        if let Some((_, p, t, _, _)) = rows.iter().find(|r| r.0 == want) {
            runas(&format!(r"shell:AppsFolder\{p}"));
            if let Some(t) = t {
                runas(t);
            }
        } else {
            println!("no item named {want:?}");
        }
    }
    Ok(())
}
