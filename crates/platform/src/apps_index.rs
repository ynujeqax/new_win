//! The Spotlight index (spec: M3 §3): the apps of `shell:AppsFolder`, and notifications when
//! apps are installed or removed (Start-menu folders, packages). Nothing here polls.

use std::path::Path;

use crate::dir_watch::Registration;
use newwin_core::spotlight::{Candidate, Kind, is_listed};
use windows::ApplicationModel::{PackageCatalog, PackageInstallingEventArgs, PackageUninstallingEventArgs, PackageUpdatingEventArgs};
use windows::Foundation::TypedEventHandler;
use windows::Win32::Foundation::{HWND, LPARAM, PROPERTYKEY, WPARAM};
use windows::Win32::Storage::EnhancedStorage::PKEY_Link_TargetParsingPath;
use windows::Win32::Storage::FileSystem::{FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellItem2, KF_FLAG_DEFAULT, SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{GUID, Interface, Result};

/// `System.AppUserModel.PackageFullName`: set only on packaged apps (not in `windows` 0.62).
const PKEY_PACKAGE_FULL_NAME: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 21 };

fn display_name(item: &IShellItem, sigdn: SIGDN) -> Option<String> {
    unsafe {
        let p = item.GetDisplayName(sigdn).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| !s.is_empty())
    }
}

fn string_prop(item: &IShellItem2, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let p = item.GetString(key).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| !s.is_empty())
    }
}

/// Every app the Start menu lists, minus uninstallers and documents. Needs COM on the caller.
pub fn enumerate_apps() -> Result<Vec<Candidate>> {
    let folder: IShellItem = unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)? };
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems)? };
    let mut out: Vec<Candidate> = Vec::new();
    loop {
        let mut got = [None];
        let mut n = 0;
        if unsafe { items.Next(&mut got, Some(&mut n)) }.is_err() || n == 0 {
            break;
        }
        let Some(item) = got[0].take() else { break };
        let (Some(name), Some(id)) = (display_name(&item, SIGDN_NORMALDISPLAY), display_name(&item, SIGDN_PARENTRELATIVEPARSING)) else { continue };
        let item2: Option<IShellItem2> = item.cast().ok();
        let file_path = item2.as_ref().and_then(|i| string_prop(i, &PKEY_Link_TargetParsingPath)).filter(|p| Path::new(p).is_file());
        let packaged = item2.as_ref().is_some_and(|i| string_prop(i, &PKEY_PACKAGE_FULL_NAME).is_some());
        if !is_listed(&name, file_path.as_deref()) || out.iter().any(|c| c.id.eq_ignore_ascii_case(&id)) {
            continue;
        }
        out.push(Candidate { name, id, kind: Kind::App, file_path, packaged });
    }
    Ok(out)
}

/// Posts `msg` when an app may have been installed or removed. Several posts can arrive for one
/// install: the owner debounces.
pub struct AppsWatch {
    _waits: Vec<Registration>,
    catalog: Option<(PackageCatalog, [i64; 3])>,
}

fn start_menu_dirs() -> Vec<std::path::PathBuf> {
    ["APPDATA", "ProgramData"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| Path::new(&base).join(r"Microsoft\Windows\Start Menu\Programs"))
        .filter(|p| p.is_dir())
        .collect()
}

impl AppsWatch {
    pub fn start(notify: HWND, msg: u32) -> Result<AppsWatch> {
        let n = notify.0 as isize;
        let mut w = AppsWatch { _waits: Vec::new(), catalog: None };
        for dir in start_menu_dirs() {
            w._waits.push(Registration::new(&dir, true, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME, notify, msg)?);
        }
        match watch_packages(n, msg) {
            Ok(c) => w.catalog = Some(c),
            Err(e) => log::warn!("package notifications unavailable: {e}"),
        }
        Ok(w)
    }
}

fn watch_packages(notify: isize, msg: u32) -> Result<(PackageCatalog, [i64; 3])> {
    let post = move || unsafe {
        let _ = PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(0), LPARAM(0));
    };
    let catalog = PackageCatalog::OpenForCurrentUser()?;
    let installing = catalog.PackageInstalling(&TypedEventHandler::<PackageCatalog, PackageInstallingEventArgs>::new(move |_, a| {
        if a.as_ref().is_some_and(|a| a.IsComplete().unwrap_or(false)) {
            post();
        }
        Ok(())
    }))?;
    let uninstalling = catalog.PackageUninstalling(&TypedEventHandler::<PackageCatalog, PackageUninstallingEventArgs>::new(move |_, a| {
        if a.as_ref().is_some_and(|a| a.IsComplete().unwrap_or(false)) {
            post();
        }
        Ok(())
    }))?;
    let updating = catalog.PackageUpdating(&TypedEventHandler::<PackageCatalog, PackageUpdatingEventArgs>::new(move |_, a| {
        if a.as_ref().is_some_and(|a| a.IsComplete().unwrap_or(false)) {
            post();
        }
        Ok(())
    }))?;
    Ok((catalog, [installing, uninstalling, updating]))
}

impl Drop for AppsWatch {
    fn drop(&mut self) {
        if let Some((c, [i, u, up])) = self.catalog.take() {
            let _ = c.RemovePackageInstalling(i);
            let _ = c.RemovePackageUninstalling(u);
            let _ = c.RemovePackageUpdating(up);
        }
    }
}
