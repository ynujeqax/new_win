//! Who a window belongs to: process image, AppUserModelID, display name.

use std::path::Path;

use newwin_core::apps::GroupKey;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand, PKEY_AppUserModel_RelaunchDisplayNameResource,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{
    FOLDERID_AppsFolder, FOLDERID_ProgramFilesX64, FOLDERID_ProgramFilesX86, FOLDERID_System, FOLDERID_Windows, IShellItem,
    KF_FLAG_DEFAULT, SHCreateItemInKnownFolder, SHGetKnownFolderPath, SHLoadIndirectString, SIGDN_NORMALDISPLAY,
};
use windows::core::{GUID, HSTRING, PCWSTR, PWSTR};

/// Initializes COM (STA) for the current thread for its lifetime.
pub struct ComGuard(bool);

impl ComGuard {
    pub fn init() -> Self {
        ComGuard(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// Full path of a process image; works for elevated processes too.
pub fn process_image_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// When the process was created (FILETIME ticks); with the pid it identifies one process.
pub fn process_creation_time(pid: u32) -> Option<u64> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut created, mut exit, mut kernel, mut user) = Default::default();
        let r = GetProcessTimes(h, &mut created, &mut exit, &mut kernel, &mut user);
        let _ = CloseHandle(h);
        r.ok()?;
        let FILETIME { dwLowDateTime: lo, dwHighDateTime: hi } = created;
        Some((u64::from(hi) << 32) | u64::from(lo))
    }
}

/// AppUserModelID of a packaged (UWP/MSIX) process.
pub fn process_app_id(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let r = GetApplicationUserModelId(h, &mut len, Some(PWSTR(buf.as_mut_ptr())));
        let _ = CloseHandle(h);
        if r.is_err() || len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize - 1]))
    }
}

fn window_property(hwnd: HWND, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let v = store.GetValue(key).ok()?;
        let s = v.to_string();
        (!s.trim().is_empty()).then_some(s)
    }
}

/// Explicit AppUserModelID set on the window (Chrome, Explorer, …).
pub fn window_app_id(hwnd: HWND) -> Option<String> {
    window_property(hwnd, &PKEY_AppUserModel_ID)
}

/// Name the window asks the taskbar to show when its app id has no Start menu entry (Steam
/// windows come from steamwebhelper.exe and say "Steam"). May be an `@dll,-id` resource.
pub fn window_relaunch_name(hwnd: HWND) -> Option<String> {
    window_property(hwnd, &PKEY_AppUserModel_RelaunchDisplayNameResource)
}

/// Command line the window asks the taskbar to run to start the app again.
pub fn window_relaunch_command(hwnd: HWND) -> Option<String> {
    window_property(hwnd, &PKEY_AppUserModel_RelaunchCommand)
}

/// A relaunch name as text: `@file,-id` resources are loaded, blanks are `None`.
fn relaunch_display_name(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let s = if raw.starts_with('@') {
        let mut buf = [0u16; 512];
        unsafe {
            SHLoadIndirectString(&HSTRING::from(raw), &mut buf, None).ok()?;
            PCWSTR(buf.as_ptr()).to_string().ok()?.trim().to_string()
        }
    } else {
        raw.to_string()
    };
    (!s.is_empty()).then_some(s)
}

/// `FileDescription` from the exe's version resource (first translation).
pub fn file_description(path: &str) -> Option<String> {
    unsafe {
        let wide = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&wide, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&wide, None, size, data.as_mut_ptr() as _).ok()?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(data.as_ptr() as _, &HSTRING::from(r"\VarFileInfo\Translation"), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let lang = *(ptr as *const u16);
        let cp = *(ptr as *const u16).add(1);
        let key = HSTRING::from(format!(r"\StringFileInfo\{lang:04x}{cp:04x}\FileDescription"));
        if !VerQueryValueW(data.as_ptr() as _, &key, &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let s = PCWSTR(ptr as *const u16).to_string().ok()?;
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

/// Name the Start menu shows for an AUMID. Needs COM on the calling thread.
pub fn appsfolder_display_name(aumid: &str) -> Option<String> {
    unsafe {
        let item: IShellItem = SHCreateItemInKnownFolder(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, &HSTRING::from(aumid)).ok()?;
        let p = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| !s.is_empty())
    }
}

/// Explorer windows carry no AUMID, but Windows groups them as this app.
pub fn implicit_app_id(exe: &str) -> Option<&'static str> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let expected = format!(r"{windir}\explorer.exe");
    exe.eq_ignore_ascii_case(&expected).then_some("Microsoft.Windows.Explorer")
}

fn guid_string(g: &GUID) -> String {
    let d4 = g.data4;
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3, d4[0], d4[1], d4[2], d4[3], d4[4], d4[5], d4[6], d4[7]
    )
}

/// `C:\Windows\System32\notepad.exe` → `{System32 GUID}\notepad.exe`, the form AppsFolder uses
/// for desktop apps pinned in the Start menu.
pub fn known_folder_relative(path: &str) -> Option<String> {
    let folders = [FOLDERID_System, FOLDERID_ProgramFilesX64, FOLDERID_ProgramFilesX86, FOLDERID_Windows];
    let lower = path.to_lowercase();
    for id in folders {
        let base = unsafe {
            let Ok(p) = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) else { continue };
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as _));
            s
        };
        let Some(base) = base else { continue };
        let prefix = format!("{}\\", base.to_lowercase());
        if lower.starts_with(&prefix) {
            return Some(format!(r"{}\{}", guid_string(&id), &path[prefix.len()..]));
        }
    }
    None
}

fn file_stem(path: &str) -> Option<String> {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned())
}

/// Shown until the real name is resolved: exe file stem, else the AUMID's last segment.
pub fn placeholder_name(key: &GroupKey, exe: Option<&str>) -> String {
    match (key, exe.and_then(file_stem)) {
        (_, Some(stem)) => stem,
        (GroupKey::ExePath(p), None) => file_stem(p).unwrap_or_else(|| p.clone()),
        (GroupKey::AppId(id), None) => id.rsplit(['!', '.', '\\']).next().unwrap_or(id).to_string(),
    }
}

/// Display name for a group: Start menu name, then (app ids only) the window's relaunch name,
/// then the exe's FileDescription, then the placeholder.
pub fn resolve_name(key: &GroupKey, exe: Option<&str>, relaunch_name: Option<&str>) -> String {
    let from_shell = match key {
        GroupKey::AppId(id) => appsfolder_display_name(id),
        GroupKey::ExePath(p) => known_folder_relative(p).and_then(|rel| appsfolder_display_name(&rel)),
    };
    let (exe_path, relaunch_name) = match key {
        GroupKey::ExePath(p) => (Some(p.as_str()), None),
        GroupKey::AppId(_) => (exe, relaunch_name),
    };
    from_shell
        .or_else(|| relaunch_name.and_then(relaunch_display_name))
        .or_else(|| exe_path.and_then(file_description))
        .unwrap_or_else(|| placeholder_name(key, exe))
}

/// A tray row's fallback name: the exe's `FileDescription`, else its file name without `.exe`.
pub fn process_display_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let path = process_image_path(pid)?;
    file_description(&path).or_else(|| {
        let file = path.rsplit(['\\', '/']).next()?;
        let stem = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
        (!stem.is_empty()).then(|| stem.to_string())
    })
}

/// (pid, exe file name) of every process in our session, including ones we may not open (a
/// SYSTEM process in the user's session, like the NVIDIA container): the terminal-services list
/// carries each session id, which `ProcessIdToSessionId` would refuse for those.
pub fn session_processes() -> Vec<(u32, String)> {
    use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTS_CURRENT_SERVER_HANDLE, WTS_PROCESS_INFOW, WTSEnumerateProcessesW, WTSFreeMemory};
    let mut mine = 0;
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut mine) }.is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    unsafe {
        let (mut list, mut count): (*mut WTS_PROCESS_INFOW, u32) = (std::ptr::null_mut(), 0);
        if WTSEnumerateProcessesW(Some(WTS_CURRENT_SERVER_HANDLE), 0, 1, &mut list, &mut count).is_err() || list.is_null() {
            return out;
        }
        for p in std::slice::from_raw_parts(list, count as usize) {
            if p.SessionId == mine
                && !p.pProcessName.is_null()
                && let Ok(name) = p.pProcessName.to_string()
            {
                out.push((p.ProcessId, name));
            }
        }
        WTSFreeMemory(list as _);
    }
    out
}
