# M1 acceptance — shell core (top bar)

Build: `m1-shell-core`, release, run with `NEWWIN_DEBUG=1`. PC: Windows 11 IoT Enterprise LTSC 2024 (26100), 16 logical CPUs, HDR display, NVIDIA GPU (unused: WARP renderer).

## Performance (`scripts/measure.ps1 -Minutes 10`)

2026-10-22, 601 s. The PC was not strictly idle: the user browsed and a release build ran during the window, so the shell also handled real window events.

| Check | Value | Limit | Result |
|-------|-------|-------|--------|
| Shell CPU average | 0.0036 % | ≤ 0.05 % | PASS |
| Shell private working set (peak) | 7.9 MB | < 20 MB | PASS |
| Watchdog CPU average | 0.0002 % | — | — |
| Watchdog private working set (peak) | 1.1 MB | < 2 MB | PASS |
| First frame | 44 ms | < 300 ms | PASS |
| Restarts during the run | none | none | PASS |

CPU is the process's CPU time over the whole run divided by elapsed time × CPU count (the Task Manager scale), not an average of 5 s counter samples. Private working set is the peak of the 5 s samples. Both are read through CIM, so the script works on a non-English Windows.

## Review Focus

| # | Case | Result | Note |
|---|------|--------|------|
| 1 | explorer.exe restarts while NewWin runs | USER_EXPLORER | Simulated in Task 9: a posted `TaskbarCreated` logs "explorer restarted" and the taskbar is re-hidden; a forced `ShowWindow` on the taskbar is re-hidden in < 300 ms. |
| 2 | Shell hard-killed or hung | pass | Hard kill: the watchdog restarts the shell 10 ms later, the state file is healed and the bar is back (Task 9). Crash policy (3 in 60 s → restore the taskbar and exit) is unit-tested (Task 4). Restart NewWin from the menu: the shell exits 3 and the watchdog restarts it (Task 12). |
| 3 | Pointer jitter / leave during reveal / the 6 px gap | pass | Scripted cursor test 10/10 (Task 11), repeated after the S4 and stage-5 fixes. A reveal during a running hide resumes from the current position (S4). |
| 4 | Existing, UWP, cloaked, untitled, elevated windows | partial | Existing windows are listed at start; Explorer is named "File Explorer"; system UI (Screen Snipping, PopupHost) is excluded and confirmed live by the user. USER_WINDOWS |
| 5 | Sleep/resume, time-zone/clock change, DPI/resolution change | USER_SYSTEM | The clock re-arms on the minute boundary (verified rolling to 12:26 in Task 12). |

## Task checklists

| Task | Check | Result |
|------|-------|--------|
| 9 | Native taskbar hidden, glass bar 6 px from the top, first frame < 300 ms | pass (first frame 44 ms) |
| 9 | End `--shell` in Task Manager → the watchdog restarts it | pass |
| 9 | Real explorer restart, DPI 100% → 150% | USER_EXPLORER_DPI |
| 11 | Auto-hide: nothing visible at rest, maximized window fills the screen, 150 ms hover reveal, gap does not hide, 400 ms hide, jitter safe | pass (scripted 10/10) |
| 11 | Fullscreen app: the edge does nothing | pass (borderless fullscreen window; logs fullscreen true/false) |
| 11 | Pinned: maximized window starts at y = 42 | pass (work area top 42, Notepad top 34 = 42 − 8 px border) |
| 12 | Labels in launch order, active one bold in a pill, click minimizes/restores, list for several windows, middle-click closes | pass |
| 12 | Right-click → all Win+X items, Quit restores the taskbar and both processes exit | pass |
| 12 | Date `Wed Oct 21`, time `2:32 PM` style | pass |
| 12 | 30 apps: labels shrink, then `»` | OVERFLOW |
| 12 | Region format / display language change → bar updates without restart | USER_REGION |
| — | Sign out and back in: native taskbar visible when NewWin is not started, hidden once it is | USER_LOGON |
| — | Uninstall path | n/a (M5 installer) |

## Live bugs found and fixed during acceptance

- Lost window messages during nested message loops (Settings late, every other click ignored, bar not hiding) — fixed in dfd5006.
- Screen Snipping and PopupHost windows shown as apps — fixed in dfd5006.
- "New window" did nothing for Chromium profiles — fixed in 36c4eb9.

## Open

- The bar sometimes stays shown by itself. The debug trace is in place (`NEWWIN_DEBUG=1`); waiting for the next occurrence in the log.
