//! System targets of the bar's right-click menu (the Win+X items).

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{EWX_LOGOFF, ExitWindowsEx, LockWorkStation, SHUTDOWN_REASON};
use newwin_core::spotlight::Candidate;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{IShellDispatch4, SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, SHOpenFolderAndSelectItems, SHParseDisplayName, Shell, ShellExecuteExW, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL};
use windows::core::{HSTRING, PCWSTR, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTarget {
    InstalledApps,
    PowerOptions,
    EventViewer,
    System,
    DeviceManager,
    NetworkConnections,
    DiskManagement,
    ComputerManagement,
    PowerShell,
    PowerShellAdmin,
    TaskManager,
    Settings,
    FileExplorer,
    Run,
    SignOut,
    Sleep,
    Lock,
    ShutDown,
    Restart,
    Desktop,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Shell { verb: &'static str, file: &'static str, args: &'static str },
    RunDialog,
    ToggleDesktop,
    SignOut,
    Sleep,
    Lock,
}

/// What each menu entry launches — the same targets Windows uses.
pub fn action(t: SystemTarget) -> Action {
    use SystemTarget::*;
    let open = |file| Action::Shell { verb: "open", file, args: "" };
    match t {
        InstalledApps => open("ms-settings:appsfeatures"),
        PowerOptions => open("ms-settings:powersleep"),
        EventViewer => open("eventvwr.msc"),
        System => open("ms-settings:about"),
        DeviceManager => open("devmgmt.msc"),
        NetworkConnections => open("ncpa.cpl"),
        DiskManagement => open("diskmgmt.msc"),
        ComputerManagement => open("compmgmt.msc"),
        PowerShell => open("powershell.exe"),
        PowerShellAdmin => Action::Shell { verb: "runas", file: "powershell.exe", args: "" },
        TaskManager => open("taskmgr.exe"),
        Settings => open("ms-settings:"),
        FileExplorer => open("explorer.exe"),
        Run => Action::RunDialog,
        SignOut => Action::SignOut,
        Sleep => Action::Sleep,
        Lock => Action::Lock,
        ShutDown => Action::Shell { verb: "open", file: "shutdown.exe", args: "/s /t 0" },
        Restart => Action::Shell { verb: "open", file: "shutdown.exe", args: "/r /t 0" },
        Desktop => Action::ToggleDesktop,
    }
}

pub fn shell_execute(verb: &str, file: &str, args: &str) -> Result<()> {
    let r = unsafe {
        ShellExecuteW(None, &HSTRING::from(verb), &HSTRING::from(file), &HSTRING::from(args), PCWSTR::null(), SW_SHOWNORMAL)
    };
    // ShellExecute returns a value > 32 on success.
    if r.0 as isize > 32 { Ok(()) } else { Err(windows::core::Error::from_thread()) }
}

fn shell_dispatch() -> Result<IShellDispatch4> {
    unsafe { CoCreateInstance(&Shell, None, CLSCTX_INPROC_SERVER) }
}

/// Runs a target. Needs COM initialized on the calling thread.
pub fn run(t: SystemTarget) -> Result<()> {
    match action(t) {
        Action::Shell { verb, file, args } => shell_execute(verb, file, args),
        Action::RunDialog => unsafe { shell_dispatch()?.FileRun() },
        Action::ToggleDesktop => unsafe { shell_dispatch()?.ToggleDesktop() },
        Action::SignOut => unsafe { ExitWindowsEx(EWX_LOGOFF, SHUTDOWN_REASON(0)) },
        Action::Lock => unsafe { LockWorkStation() },
        Action::Sleep => {
            unsafe { SetSuspendState(false, false, false) };
            Ok(())
        }
    }
}

/// Win+N: Windows' notification center with the calendar (works with the taskbar hidden,
/// checked live). Tagged, so our own hook ignores it.
pub fn open_notification_center() {
    const VK_LWIN: u16 = 0x5B;
    const VK_N: u16 = 0x4E;
    let _ = crate::input_hook::send_tagged(&[(VK_LWIN, false), (VK_N, false), (VK_N, true), (VK_LWIN, true)]);
}

/// `shell:AppsFolder\<id>`: how the Start menu itself launches an app, classic or packaged.
pub fn apps_folder_path(id: &str) -> String {
    format!(r"shell:AppsFolder\{id}")
}

fn shell_execute_ex(verb: &str, file: &str) -> Result<()> {
    let (verb, file) = (HSTRING::from(verb), HSTRING::from(file));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }
}

/// Launches a Spotlight app. Needs COM initialized on the calling thread.
pub fn open_app(id: &str) -> Result<()> {
    shell_execute_ex("open", &apps_folder_path(id))
}

/// Elevated, through the same AppsFolder path (keeps the shortcut's arguments; checked by the
/// M3 probe). A declined UAC prompt is an error (`ERROR_CANCELLED`).
pub fn run_as_admin(c: &Candidate) -> Result<()> {
    shell_execute_ex("runas", &apps_folder_path(&c.id))
}

/// Opens Explorer on the file's folder with the file selected.
pub fn open_file_location(path: &str) -> Result<()> {
    if !std::path::Path::new(path).exists() {
        return Err(windows::Win32::Foundation::ERROR_FILE_NOT_FOUND.into());
    }
    unsafe {
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(&HSTRING::from(path), None, &mut pidl, 0, None)?;
        let r = SHOpenFolderAndSelectItems(pidl, None, 0);
        CoTaskMemFree(Some(pidl as _));
        r
    }
}

/// `NewWinSettings.exe` next to the running exe, if it is there.
pub fn settings_exe() -> Option<std::path::PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join("NewWinSettings.exe");
    path.is_file().then_some(path)
}

/// Starts Settings (single-instance: a running one just comes to the front). Lets it take the
/// foreground first, since the click that asked for it belongs to this process.
pub fn run_settings(exe: &std::path::Path) -> std::io::Result<()> {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    std::process::Command::new(exe).spawn().map(|_| ())
}

/// `%VARIABLES%` in a path replaced with their values; the text as is when Windows cannot.
pub fn expand_env(path: &str) -> String {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    let src = HSTRING::from(path);
    let mut buf = vec![0u16; 1024];
    let n = unsafe { ExpandEnvironmentStringsW(&src, Some(&mut buf)) } as usize;
    if n == 0 || n > buf.len() {
        return path.to_string();
    }
    String::from_utf16_lossy(&buf[..n - 1])
}

/// Opens a URL or a `ms-settings:` page with whatever handles it (the default browser).
pub fn open_uri(uri: &str) -> Result<()> {
    shell_execute("open", uri, "")
}

/// Opens a folder or file, `%VARIABLES%` expanded.
pub fn open_path(path: &str) -> Result<()> {
    shell_execute("open", &expand_env(path), "")
}
