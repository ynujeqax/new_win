<p align="center">
  <img src="docs/assets/hero.svg" alt="NewWin — a liquid-glass top bar for Windows 11" width="100%">
</p>

<p align="center">
  <b>A minimal, monochrome, liquid-glass top bar that replaces the Windows 11 taskbar and Start menu.</b><br>
  Native Rust · Win32 · Direct2D · DirectComposition. No Electron, no web views, no runtime.
</p>

<p align="center">
  <a href="https://github.com/ynujeqax/new_win/releases/latest"><img alt="Download" src="https://img.shields.io/badge/download-NewWin%201.0.2-fff?style=for-the-badge&labelColor=111"></a>
</p>

<p align="center">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2011-111?style=flat-square">
  <img alt="Rust" src="https://img.shields.io/badge/rust-2024%20edition-111?style=flat-square&logo=rust">
  <img alt="Version" src="https://img.shields.io/badge/version-1.0.2-111?style=flat-square">
  <img alt="RAM" src="https://img.shields.io/badge/RAM-~8%20MB-111?style=flat-square">
  <img alt="Idle CPU" src="https://img.shields.io/badge/idle%20CPU-0.002%25-111?style=flat-square">
  <img alt="Tests" src="https://img.shields.io/badge/tests-492%20passing-111?style=flat-square">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-111?style=flat-square">
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#install">Install</a> ·
  <a href="#performance">Performance</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="README.uk.md">Українською</a>
</p>

---

## Why

Built for an OLED monitor and a black wallpaper: **no static pixels, no clutter, colour only where it carries meaning.** The bar hides above the screen edge and slides in only when the mouse touches a 1–2 px strip at the top, so windows get 100 % of the screen. It is text, thin vector lines and the apps' own icons, set in liquid glass with its own blur and a lens-like rim.

It has to be invisible in another way too. It should never lag, freeze or crash, and if it ever does, you still get a taskbar back.

## Features

<table>
  <tr>
    <td width="58%"><img src="docs/assets/spotlight.svg" alt="Spotlight"></td>
    <td width="42%"><img src="docs/assets/tray.svg" alt="Tray panel"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Spotlight</b>: press <kbd>Win</kbd>, type, <kbd>↵</kbd></sub></td>
    <td align="center"><sub><b>Tray</b>: background apps with their own menus</sub></td>
  </tr>
</table>

### The bar
- **Running apps** on the left, each with its Windows icon. The active one is bold, in a soft glass pill that glides between apps.
- Click to switch, minimise or restore; middle click to close; an app with several windows shows a list. Right click gives *New window* and *Close*.
- **Right side:** keyboard layout (`ENG` / `UKR`), network (Ethernet, Wi-Fi with real signal bars, offline), volume (scroll to change, middle click to mute, click for a slider and output device), search, then date and time (click for the notification centre).
- **Auto-hide** (appears on hover) or **Pinned** (reserves its space like the native taskbar, so maximised windows never slide under it). It also stays out of the way of full-screen apps.
- Right click on empty space opens the full **Win+X menu**: Device Manager, PowerShell (admin), Task Manager, Run, shut down, and the rest.

### Spotlight: replaces Start
- <kbd>Win</kbd> or <kbd>Ctrl</kbd> + <kbd>Esc</kbd> opens a centred glass panel. Type to search apps and system commands (*Shut down*, *Restart*, *Sleep*, *Lock*, *Sign out*, *NewWin Settings*).
- Results carry their app icons. Two apps with the same name are told apart, and the matched part is bold.
- Ranked by how often you launch things. New apps show up live as they are installed.
- <kbd>↵</kbd> open · <kbd>Ctrl</kbd> + <kbd>↵</kbd> run as administrator · right click: *Open file location* · <kbd>Esc</kbd> close.

### Tray
- `⌃` opens a list of background apps (Discord, Steam, NVIDIA, …) with their tray icons, named after their tray tooltips.
- A left click does what the icon does. A right click opens the **app's own native menu**, never a look-alike.
- Reorder and hide apps in Settings.

<p align="center">
  <img src="docs/assets/settings.svg" alt="NewWin Settings" width="72%">
</p>

### Settings: `NewWinSettings.exe`
- The same glass, in six sections: *Bar · Appearance · Items · Spotlight · Tray · About*.
- Toggles, sliders, segmented choices, a font picker that draws each family in its own face, and drag-to-reorder lists.
- **No Save button.** Every change reaches the bar live, in well under a second.
- Settings are plain TOML. Edit `config.toml` by hand and the bar picks it up instantly; a typo shows a banner instead of breaking anything.
- English and Ukrainian, following the Windows display language.

### Safety net
- A tiny watchdog (~1 MB) restarts the shell if it crashes or hangs. After 3 crashes in a minute it gives the native taskbar back.
- `NewWin.exe --quit`, *Quit NewWin* in the bar's menu, or *Restore Windows taskbar* in Settings puts Windows back the way it was. So does uninstalling.
- Shell icons are fetched by a short-lived helper process that exits after 30 s idle. A stuck shell extension costs at most 5 s and never freezes the bar.
- All files are written atomically. Release builds use `panic = "abort"` and never trust data read from disk or from other processes.

## Install

1. Download **`NewWin-Setup-1.0.2.exe`** from [Releases](https://github.com/ynujeqax/new_win/releases/latest), or [build it yourself](#build-from-source).
2. Run it. No admin rights are needed: NewWin installs for your user only, into `%LOCALAPPDATA%\Programs\NewWin` (you can pick another folder). Setup is not code-signed yet, so SmartScreen may warn: **More info → Run anyway**.
3. NewWin starts right away, and with Windows from then on. Running a newer Setup over it updates it and keeps your settings.

Silent install, for scripts: `NewWin-Setup-1.0.2.exe /VERYSILENT /DIR="C:\Programs\NewWin"`.

**Uninstall** from *Settings → Apps → Installed apps → NewWin*. The Windows taskbar comes back, and everything NewWin wrote is removed, settings and logs included.

You can open Settings from the tray (`⌃` → **NewWin**), from the bar's menu, from Spotlight (`settings`), or by launching `NewWin.exe` again.

### Keys and clicks

| | |
|---|---|
| <kbd>Win</kbd> · <kbd>Ctrl</kbd> + <kbd>Esc</kbd> | Spotlight (on by default; see Settings → Spotlight) |
| Click an app | Switch to it · minimise if active · list if several windows |
| Middle click an app | Close it |
| Right click empty bar | Win+X menu, NewWin Settings, Restart / Quit NewWin |
| Scroll / middle click on volume | Change volume / mute |
| Click the date | Notification centre |
| Click `ENG` | Next keyboard layout |

## Performance

Measured on the author's machine (Windows 11, 16 logical CPUs).

| | Result |
|---|---|
| Memory, shell (private working set, 1.0.1) | **~8 MB**, 19 threads |
| Memory, icon helper (only while fetching icons) | 2.6 MB; exits 30 s after its last request |
| Memory, watchdog | 1.3 MB |
| CPU, 10 minutes idle (1.0.0) | **0.0016 %** shell · 0.0003 % watchdog |
| Bar first frame | 55 ms |
| Spotlight / tray panel first frame | ~1 ms |
| Settings first frame | 10–13 ms |
| Ranking 1 000 apps | < 1 ms |

Nothing polls: everything is event-driven (WinEvents, change notifications, shell hooks). The GPU device for the glass is warmed on demand and released after 60 s idle.

## Build from source

**Requirements:** Windows 11, [Rust](https://rustup.rs) stable (MSVC toolchain), and [Inno Setup 7](https://jrsoftware.org/isdl.php) for the installer.

```powershell
git clone https://github.com/ynujeqax/new_win.git
cd new_win
cargo build --release --workspace
.\target\release\NewWin.exe                 # run it from the build folder
.\scripts\build-installer.ps1               # or make target\installer\NewWin-Setup-<version>.exe
```

The native taskbar is hidden while NewWin runs. To quit NewWin and get it back at any time:

```powershell
.\target\release\NewWin.exe --quit
```

### Files

| Path | What |
|---|---|
| `%APPDATA%\NewWin\config.toml` | All settings; safe to edit by hand |
| `%APPDATA%\NewWin\usage.toml` | Launch counts for Spotlight ranking |
| `%APPDATA%\NewWin\tray_seen.toml` | Tray apps seen so far (names for Settings) |
| `%LOCALAPPDATA%\NewWin\logs\` | Logs; set `NEWWIN_DEBUG=1` for verbose ones |

## Architecture

```
NewWin.exe  (watchdog)                     restarts the shell on a crash or hang,
   │                                       gives the taskbar back after 3 crashes
   └─▶ NewWin.exe --shell                  bar · Spotlight · tray host · panels
          │   one UI thread + small workers (glass, input hook, worker)
          ├─▶ NewWin.exe --icon-helper     shell icons over stdin/stdout, exits when idle
          └─◀ config.toml ─▶ NewWinSettings.exe    glass settings window, separate process
```

| Crate | Role |
|---|---|
| [`crates/core`](crates/core) | Pure logic: ranking, layout, the tray protocol, strings (en/uk), decisions. No Win32, fully unit-tested |
| [`crates/config`](crates/config) | Config types, bounds, diffing, atomic TOML files |
| [`crates/platform`](crates/platform) | Win32 / WinRT: windows, tray host, icons, audio, network, watchers, launching |
| [`crates/ui`](crates/ui) | Direct2D / DirectComposition rendering, liquid glass, controls, windows |
| [`apps/shell`](apps/shell) | The bar, Spotlight, tray panel, watchdog and icon helper (`NewWin.exe`) |
| [`apps/settings`](apps/settings) | `NewWinSettings.exe` |

## Roadmap

- [x] **M1**: shell core (bar, auto-hide / pinned, running apps, clock, watchdog)
- [x] **Liquid glass**: own blur, lens rim, capsule shape
- [x] **M2**: tray, network, volume, keyboard layout
- [x] **M3**: Spotlight
- [x] **M4**: Settings window, live config
- [x] **M5**: installer, an uninstall that restores the taskbar, polish
- [x] **1.0.1**: colour app icons on the bar, in Spotlight and in the tray
- [x] **1.0.2**: animations everywhere (gliding highlights, sliding app pills, a rolling clock), redrawn Wi-Fi and speaker icons, Steam games named right away
- [ ] Multiple monitors

## Development

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets
powershell -File scripts\measure.ps1 -Minutes 10   # idle CPU / RAM budget check
```

The glass is invisible to DXGI screenshots, so visual checks are done by eye. The pictures in this README are drawn to match the real UI.

## License

[MIT](LICENSE)
