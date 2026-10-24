<p align="center">
  <img src="docs/assets/bar.svg" alt="NewWin — a liquid-glass top bar for Windows 11" width="100%">
</p>

<p align="center">
  <b>A minimal, monochrome, liquid-glass top bar that replaces the Windows 11 taskbar and Start menu.</b><br>
  Native Rust · Win32 · Direct2D · DirectComposition — no Electron, no web views, no runtime.
</p>

<p align="center">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2011-111?style=flat-square">
  <img alt="Rust" src="https://img.shields.io/badge/rust-2024%20edition-111?style=flat-square&logo=rust">
  <img alt="Idle CPU" src="https://img.shields.io/badge/idle%20CPU-0.002%25-111?style=flat-square">
  <img alt="RAM" src="https://img.shields.io/badge/RAM-~11%20MB-111?style=flat-square">
  <img alt="License" src="https://img.shields.io/badge/license-MIT-111?style=flat-square">
</p>

<p align="center">
  <a href="README.uk.md">Українською</a>
</p>

---

## Why

Built for an OLED monitor and a black wallpaper: **no static pixels, no colour, no clutter.** The bar hides above the screen edge and slides in only when the mouse touches a 1–2 px strip at the top. Windows get 100 % of the screen. Everything is text and thin vector lines, set in liquid glass with its own blur and a lens-like rim.

And it has to be invisible in another sense too: it should never lag, never freeze, never crash — and if it does, you are never left without a taskbar.

## Features

**The bar**
- Running apps on the left; the active one is bold, in a soft glass pill that glides between apps.
- Keyboard layout (`ENG` / `UKR`), network (Ethernet, Wi-Fi with real signal bars, offline), volume, search, date and time on the right.
- **Auto-hide** (appears on hover, apps get the whole screen) or **Pinned** (reserves its space like the native taskbar; maximised windows never slide under it).

**Spotlight** — replaces Start
- Press <kbd>Win</kbd>: a centred glass panel, type to search apps and system commands.
- Results are text only, the matched part in bold, ranked by how often you launch things.
- <kbd>↵</kbd> open · <kbd>Ctrl</kbd> + <kbd>↵</kbd> run as administrator · <kbd>Esc</kbd> close.

**Tray**
- `⌃` opens a text list of background apps (NVIDIA, Discord, …), named after their tray tooltips.
- Left click does what the icon does; right click opens the app's **own native menu** — no look-alikes.
- Reorder and hide apps from Settings.

**Settings** — `NewWinSettings.exe`
- Same glass, six sections: *Bar · Appearance · Items · Spotlight · Tray · About*.
- Toggles, sliders, segmented choices, a font picker that draws each family in itself, drag-to-reorder lists.
- No Save button: every change reaches the bar live, in well under a second.
- Settings are plain TOML — edit `config.toml` by hand and the bar picks it up instantly; a typo shows a banner instead of breaking anything.

**Safety net**
- A tiny watchdog (~1 MB) restarts the shell if it crashes or hangs; after 3 crashes in a minute it gives the native taskbar back.
- `NewWin.exe --restore-taskbar` or *Restore Windows taskbar* in Settings puts Windows back the way it was.
- All files are written atomically; release builds use `panic = "abort"` and never trust data read from disk or other processes.

## Performance

Measured on the author's machine (Windows 11, 16 logical CPUs), see [acceptance notes](docs/superpowers/notes/2026-10-23-m4-acceptance.md):

| | Result |
|---|---|
| CPU, 10 minutes idle | **0.0016 %** (shell) · 0.0003 % (watchdog) |
| Memory (private working set) | **~11 MB** shell · 1.3 MB watchdog |
| Bar first frame | 55 ms |
| Spotlight first frame | 1–2 ms |
| Settings first frame | 10–13 ms |
| Ranking 1 000 apps | < 1 ms |

No polling: everything is event-driven (WinEvents, change notifications, shell hooks). The GPU device for glass is warmed on demand and released after 60 s of idle.

## Getting started

> **Status:** personal project, Windows 11 only, single monitor. There is no installer yet — you build it yourself.

**Requirements:** Windows 11, [Rust](https://rustup.rs) stable (MSVC toolchain).

```powershell
git clone https://github.com/ynujeqax/new_win.git
cd new_win
cargo build --release --workspace
.\target\release\NewWin.exe
```

The native taskbar is hidden while NewWin runs. To get it back at any time:

```powershell
.\target\release\NewWin.exe --restore-taskbar
```

Open Settings from the tray (`⌃` → **NewWin**), the bar's menu, Spotlight (`settings`), or by launching `NewWin.exe` again. *Start with Windows* lives in Settings → Bar.

### Files

| Path | What |
|---|---|
| `%APPDATA%\NewWin\config.toml` | All settings; safe to edit by hand |
| `%APPDATA%\NewWin\usage.toml` | Launch counts for Spotlight ranking |
| `%APPDATA%\NewWin\tray_seen.toml` | Tray apps seen so far (names for Settings) |
| `%LOCALAPPDATA%\NewWin\logs\` | Logs; set `NEWWIN_DEBUG=1` for verbose ones |

## Architecture

```
NewWin.exe (watchdog) ──spawns──▶ NewWin.exe --shell ◀──config.toml──▶ NewWinSettings.exe
      │ restarts on crash,              │ bar · Spotlight · tray               glass settings window
      │ restores the taskbar            │ (one UI thread + small workers)      (separate process)
```

| Crate | Role |
|---|---|
| [`crates/core`](crates/core) | Pure logic: ranking, layout, strings (en/uk), decisions — no Win32, fully unit-tested |
| [`crates/config`](crates/config) | Config types, bounds, diffing, atomic TOML files |
| [`crates/platform`](crates/platform) | Win32 / WinRT: windows, tray host, audio, network, watchers, launching |
| [`crates/ui`](crates/ui) | Direct2D / DirectComposition rendering, liquid glass, controls, windows |
| [`apps/shell`](apps/shell) | The bar, Spotlight, tray panel and the watchdog |
| [`apps/settings`](apps/settings) | `NewWinSettings.exe` |

Design documents for every milestone live in [`docs/superpowers/specs`](docs/superpowers/specs), the implementation plans in [`docs/superpowers/plans`](docs/superpowers/plans), and the measurement / acceptance notes in [`docs/superpowers/notes`](docs/superpowers/notes).

## Roadmap

- [x] **M1** — shell core: bar, auto-hide / pinned, running apps, clock, watchdog
- [x] **Liquid glass** — own blur, lens rim, capsule shape
- [x] **M2** — tray, network, volume, keyboard layout
- [x] **M3** — Spotlight
- [x] **M4** — Settings window, live config
- [ ] Installer (`.exe` with folder choice)
- [ ] Multiple monitors

## Development

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets
powershell -File scripts\measure.ps1 -Minutes 10   # idle CPU / RAM budget check
```

Glass is invisible to DXGI screenshots, so visual checks are done by eye.

## License

[MIT](LICENSE)
