//! System targets of the bar's right-click menu (the Win+X items).

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{EWX_LOGOFF, ExitWindowsEx, SHUTDOWN_REASON};
use windows::Win32::UI::Shell::{IShellDispatch4, Shell, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
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
        Action::Sleep => {
            unsafe { SetSuspendState(false, false, false) };
            Ok(())
        }
    }
}
