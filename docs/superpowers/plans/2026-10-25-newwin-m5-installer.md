# NewWin M5: інсталятор і полірування — план реалізації

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** NewWin 1.0.0 ставиться й видаляється одним `NewWin-Setup-1.0.0.exe`, а відкладені дрібниці M4 закрито.

**Architecture:** Нова роль `NewWin.exe --quit` (чиста логіка в `crates/core::quit`, Win32 у `crates/platform::processes`) гарантовано закриває всі процеси NewWin і повертає таскбар; її викликають інсталятор, деінсталятор і кнопка *Restore* у Settings. Inno Setup 6.7 (`installer/newwin.iss`) складає темний per-user інсталятор. Полірування — точкові зміни в `crates/ui`, `apps/settings`, `crates/config`, `crates/platform`.

**Tech Stack:** Rust 2024, `windows` 0.62, Inno Setup 6.7.x (ISCC), `embed-resource` (іконка), PowerShell.

**Spec:** [docs/superpowers/specs/2026-10-25-newwin-m5-installer-design.md](../specs/2026-10-25-newwin-m5-installer-design.md)

## Global Constraints

- Версія workspace `1.0.0`; Setup називається `NewWin-Setup-<версія>.exe` і лежить у `target\installer\`.
- `PrivilegesRequired=lowest`; папка за замовчуванням `{localappdata}\Programs\NewWin`; `DisableDirPage=auto`; `CloseApplications=no`.
- Мови Setup: англійська й українська, без діалогу вибору (за мовою інтерфейсу).
- Сторінки Setup: лише папка → встановлення → «Готово»; темний `WizardStyle`, без картинок.
- `--quit`: подія `Local\NewWin.Quitting`; очікування **3000 мс**, опитування **100 мс**, до **3** раундів примусового завершення; код 0 = процесів не лишилось, 1 = лишились.
- Імена образів для `--quit`: рівно `NewWin.exe` і `NewWinSettings.exe` (без урахування регістру), лише поточна сесія, крім себе.
- Видалення стирає `{userappdata}\NewWin`, `{localappdata}\NewWin`, `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\NewWin` — без запитань.
- `*.tmp` у `%APPDATA%\NewWin` прибираються, лише якщо старші за **60 с**.
- Іконка: білий контур капсули на прозорому тлі; розміри 16, 20, 24, 32, 40, 48, 64, 256. Вікна NewWin її не малюють.
- Монохром, рядки інтерфейсу в таблицях `en`/`uk` (`crates/core/src/strings.rs`).
- Інсталятор не пише `Run\NewWin`: це робить shell при старті (M4).
- Shell у простої не прокидається: жодних нових таймерів поза `--quit` і перетягуванням.
- Перед release-збіркою: `Get-Process NewWin, NewWinSettings -ErrorAction SilentlyContinue | Stop-Process -Force; .\target\release\NewWin.exe --restore-taskbar`; після — `Start-Process .\target\release\NewWin.exe` (до появи `--quit`).

## Review Focus

1. **NewWin не запущений під час встановлення/видалення** → `--quit` одразу відновлює таскбар (без шкоди, якщо він і так видимий) і повертає 0. Тест: `quit_with_nothing_running_restores_at_once` (Task 2).
2. **Shell завис і не відповідає на `WM_APP + 20`** → через 3 с його завершено, watchdog не підняв його знову. Тести: `a_hung_shell_is_killed_after_3_s`, `watchdog_stops_instead_of_restarting_while_quitting` (Task 2).
3. **Settings відкрито з незбереженою зміною, коли приходить `--quit`** → `WM_CLOSE` спершу записує зміну. Тест: `closing_the_window_saves_a_pending_edit` (Task 4).
4. **Папка встановлення з пробілами чи кирилицею** (`C:\Мої програми\New Win`) → значення `Run` у лапках і запускає exe. Тест: `run_value_quotes_paths_with_spaces_and_cyrillic` (Task 5).
5. **Схожий чужий процес** (`NewWinHelper.exe`, `newwin.exe.bak`) → `--quit` його не чіпає. Тест: `only_exact_newwin_images_match` (Task 3).

---

### Task 1: Версія 1.0.0 та іконка

**Files:**
- Modify: `Cargo.toml` (workspace `version = "1.0.0"`)
- Create: `scripts/make-icon.ps1`, `assets/newwin.ico`, `apps/shell/newwin.rc`, `apps/settings/newwin.rc`
- Modify: `apps/shell/build.rs`, `apps/settings/build.rs`, `apps/shell/Cargo.toml`, `apps/settings/Cargo.toml`
- Test: `apps/settings/src/sections/about.rs` (тести)

**Interfaces:**
- Produces: `assets/newwin.ico` (інсталятор бере його як `SetupIconFile`); ресурс іконки з id `1` в обох exe (`UninstallDisplayIcon={app}\NewWin.exe`).

- [ ] **Step 1: Тест версії.** У тестах `about.rs` додати `about_shows_version_1_0_0`: рядок версії в *About* містить `"1.0.0"`.
- [ ] **Step 2: Запустити**: `cargo test -p newwin-settings about_shows_version_1_0_0`. Очікується FAIL (`0.1.0`).
- [ ] **Step 3: Підняти версію** в `Cargo.toml` до `1.0.0`; `cargo test -p newwin-settings about_shows_version_1_0_0` → PASS.
- [ ] **Step 4: `scripts/make-icon.ps1`**: System.Drawing малює для кожного розміру з Global Constraints капсулу-контур (ширина 80 % кадру, висота 36 % кадру, повністю заокруглені кінці, по центру), білим `#FFFFFF`, товщиною `max(1, round(size/16))` px, з антиаліасингом, на прозорому тлі. Пише PNG-кадри в один `assets/newwin.ico` (ICO-заголовок + PNG-записи). Запустити, відкрити `.ico` у переглядачі й переконатися, що всі 8 розмірів є.
- [ ] **Step 5: Вбудувати іконку.** `newwin.rc`: `1 ICON "../../assets/newwin.ico"`. У `build.rs` після `embed_manifest(...)` — `embed_resource::compile("newwin.rc", embed_resource::NONE)`; `embed-resource` у `[build-dependencies]` (остання 3.x). Якщо лінкер скаржиться на два ресурсні об'єкти, перенести маніфест у той самий `.rc` (`1 24 "…manifest"`, згенерований раніше) і прибрати `embed-manifest`.
- [ ] **Step 6: Перевірка.** `cargo build --release`; у Провіднику обидва exe мають іконку-капсулу; `cargo test --workspace` → PASS.
- [ ] **Step 7: Commit** `feat: version 1.0.0 and the NewWin icon`.

### Task 2: Логіка `--quit` (чиста) і watchdog

**Files:**
- Create: `crates/core/src/quit.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/src/watchdog.rs`
- Test: у тих самих файлах (`#[cfg(test)]`)

**Interfaces:**
- Produces (`newwin_core::quit`):
  - `pub const QUITTING_EVENT: &str = "Local\\NewWin.Quitting";`
  - `pub const QUIT_WAIT_MS: u64 = 3000; pub const QUIT_POLL_MS: u32 = 100; pub const MAX_KILL_ROUNDS: u32 = 3;`
  - `pub enum QuitStep { Wait, Kill, Restore { code: i32 } }`
  - `pub struct QuitPlan` з `pub fn new(now_ms: u64) -> QuitPlan` і `pub fn step(&mut self, now_ms: u64, alive: bool) -> QuitStep`
- Produces (`newwin_core::watchdog`): `pub fn while_quitting(d: ExitDecision, quitting: bool) -> ExitDecision`

Правила `step`: `!alive` → `Restore { code: 0 }`; `alive` і `now - start < QUIT_WAIT_MS` → `Wait`; `alive` після очікування і раундів < `MAX_KILL_ROUNDS` → `Kill` (раунд +1); `alive` після 3 раундів → `Restore { code: 1 }`. `while_quitting`: при `quitting` будь-яке `Restart`/`GiveUp` стає `Stop`, інакше без змін.

- [ ] **Step 1: Тести** в `quit.rs`:
  - `quit_with_nothing_running_restores_at_once`: `QuitPlan::new(0).step(0, false) == Restore { code: 0 }`.
  - `a_quitting_shell_is_waited_for`: `step(100, true) == Wait`, `step(2999, true) == Wait`, `step(3100, false) == Restore { code: 0 }`.
  - `a_hung_shell_is_killed_after_3_s`: `step(3000, true) == Kill`.
  - `three_kill_rounds_then_give_up_with_code_1`: після трьох `Kill` наступне `step(_, true) == Restore { code: 1 }`.
  
  У `watchdog.rs`: `watchdog_stops_instead_of_restarting_while_quitting`: `while_quitting(Restart, true) == Stop`, `while_quitting(GiveUp, true) == Stop`, `while_quitting(Restart, false) == Restart`.
- [ ] **Step 2:** `cargo test -p newwin-core quit` → FAIL (модуля немає).
- [ ] **Step 3:** Реалізувати `quit.rs` і `while_quitting` за правилами вище; `pub mod quit;` у `lib.rs`.
- [ ] **Step 4:** `cargo test -p newwin-core` → PASS.
- [ ] **Step 5: Commit** `feat: quit plan and watchdog stop while quitting`.

### Task 3: Роль `NewWin.exe --quit`

**Files:**
- Create: `crates/platform/src/processes.rs`, `apps/shell/src/quit.rs`, `scripts/suspend-process.ps1`
- Modify: `crates/platform/src/lib.rs`, `crates/platform/src/taskbar.rs`, `apps/shell/src/roles.rs`, `apps/shell/src/main.rs`, `apps/shell/src/watchdog.rs`

**Interfaces:**
- Consumes: `newwin_core::quit::*`, `newwin_core::watchdog::{while_quitting, PING_WINDOW_CLASS}`, `newwin_platform::taskbar::restore_from_state_file`, `paths::taskbar_state_file`.
- Produces (`newwin_platform::processes`):
  - `pub const NEWWIN_IMAGES: [&str; 2] = ["NewWin.exe", "NewWinSettings.exe"];`
  - `pub fn is_newwin_image(name: &str) -> bool` — точний збіг без урахування регістру
  - `pub fn other_newwin_pids() -> Vec<u32>` — Toolhelp32, `is_newwin_image`, та сама сесія (`ProcessIdToSessionId`), без `GetCurrentProcessId()`
  - `pub fn close_windows_of(pid: u32)` — `WM_CLOSE` усім видимим top-level вікнам процесу
  - `pub fn terminate(pid: u32) -> bool`
  - `pub fn event_exists(name: &str) -> bool`; `pub fn create_event(name: &str) -> Option<OwnedHandle>` (тримається до виходу)
- Produces (`apps/shell`): `Role::Quit` для аргументу `--quit`; `pub fn run_quit() -> i32` у `quit.rs`.

`run_quit`: `create_event(QUITTING_EVENT)` → `PostMessageW(FindWindowW(PING_WINDOW_CLASS), WM_APP + 20)` → `close_windows_of` для кожного pid `NewWinSettings.exe` → цикл `QuitPlan` з `Sleep(QUIT_POLL_MS)`, `alive = !other_newwin_pids().is_empty()`, на `Kill` → `terminate` кожного → на `Restore { code }` → `restore_from_state_file`, а якщо файла стану немає (`Ok(false)`) і `native_taskbar_visible()` = false — `taskbar::show_native()` (нова `pub fn show_native() -> Result<()>` = наявна приватна `restore_native(false)`) → повернути `code`. Логи: `quit: asked`, `quit: killed pid=…`, `quit: done code=…`.

Watchdog: у `run_watchdog` після `crashes.on_exit(...)` застосувати `while_quitting(decision, event_exists(QUITTING_EVENT))`.

- [ ] **Step 1: Тести.** `roles.rs`: `role_from_args(["--quit"]) == Role::Quit`. `processes.rs`: `only_exact_newwin_images_match` — `true` для `NewWin.exe`, `newwin.EXE`, `NewWinSettings.exe`; `false` для `NewWinHelper.exe`, `newwin.exe.bak`, `NewWin`, `explorer.exe`. `other_newwin_pids_excludes_this_process` (у тесті поточний exe не `NewWin.exe` → список не містить свого pid).
- [ ] **Step 2:** `cargo test -p newwin-platform processes; cargo test -p newwin-shell roles` → FAIL.
- [ ] **Step 3:** Реалізувати `processes.rs`, `Role::Quit`, `quit.rs`, гілку в `main.rs`, перевірку події у watchdog.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: Ручна перевірка (release).** (а) Запущений NewWin: `.\target\release\NewWin.exe --quit; $LASTEXITCODE` → `0`, процесів `NewWin*` немає, таскбар Windows видно. (б) Повторно без NewWin → `0` менш ніж за 0.5 с. (в) Зависання: запустити NewWin, призупинити shell-процес (той `NewWin.exe`, що має вікно `NewWin.Ping`) скриптом `scripts/suspend-process.ps1 -Pid <pid>` (новий, `NtSuspendProcess` через `Add-Type`), `--quit` → завершено за ~3 с, watchdog не перезапустив, код `0`. Після перевірки знову `Start-Process .\target\release\NewWin.exe`.
- [ ] **Step 6: Commit** `feat: NewWin.exe --quit closes every NewWin process and restores the taskbar`.

### Task 4: Settings — Restore через `--quit`, взаємні підтвердження, збереження при закритті

**Files:**
- Modify: `apps/settings/src/sections/about.rs`, `apps/settings/src/sections/mod.rs`, `apps/settings/src/view/dispatch.rs`, `apps/settings/src/view.rs`

**Interfaces:**
- Consumes: `NewWin.exe --quit` (Task 3).
- Produces (`sections/mod.rs`): `pub struct Confirms { pub clear: Confirm, pub restore: Confirm }` з `pub fn click(&mut self, id: RowId, now: u64) -> Click` (натискання одного скидає інший у `Confirm::default()`) і `pub fn armed(&self, now: u64) -> Vec<RowId>`.
- Removes: `RestoreWait`, `RestoreStep`, `RESTORE_WAIT_MS`, `RESTORE_POLL_MS`, `request_quit`, `RESTORE_TIMER`, `poll_restore`.
- Produces (`about.rs`): `pub fn run_quit(newwin_exe: Option<&Path>)` — `ShellExecute` `NewWin.exe --quit` поруч із собою.

- [ ] **Step 1: Тести.** `arming_one_confirm_disarms_the_other`: `click(ClearHistory, 0) == Armed`, `click(RestoreTaskbar, 500) == Armed`, `armed(600) == [RestoreTaskbar]`, `click(ClearHistory, 700) == Armed` (а не `Fire`). `closing_the_window_saves_a_pending_edit`: після `view.edit(...)` і `WM_CLOSE` (через `message`) writer не має відкладеної зміни, файл містить нове значення (на тимчасовому шляху). Видалити тести `restore_waits_up_to_3_s_…`.
- [ ] **Step 2:** `cargo test -p newwin-settings` → FAIL.
- [ ] **Step 3:** `Confirms` замість двох окремих `Confirm`; *Restore* на `Fire` → `about::run_quit(newwin_exe())`, потім `flush()` і `DestroyWindow` власного вікна. `WM_CLOSE` → `flush()` перед закриттям (якщо ще не так).
- [ ] **Step 4:** `cargo test -p newwin-settings` → PASS.
- [ ] **Step 5: Ручна перевірка.** Settings → About → *Restore Windows taskbar* двічі → NewWin і Settings вийшли, таскбар видно, через 10 с NewWin не повернувся.
- [ ] **Step 6: Commit** `feat: Restore uses --quit; Clear and Restore disarm each other`.

### Task 5: Автозапуск і сиротливі `.tmp`

**Files:**
- Modify: `crates/platform/src/autostart.rs`, `crates/config/src/lib.rs`, `apps/shell/src/app.rs` (виклик при старті)
- Test: `crates/platform/src/autostart.rs` (`#[cfg(test)]`), `crates/config/tests/config.rs`

**Interfaces:**
- Produces: `pub fn run_value_for(exe: &Path) -> Option<String>` (`autostart_command` викликає її з `current_exe()`); `newwin_config::clean_stale_tmp(dir: &Path, older_than: Duration) -> usize` (видаляє лише `*.tmp`, повертає кількість).

- [ ] **Step 1: Тести.**
  - `run_value_quotes_paths_with_spaces_and_cyrillic`: `run_value_for(r"C:\Мої програми\New Win\NewWin.exe") == Some("\"C:\\Мої програми\\New Win\\NewWin.exe\"")`.
  - `turning_autostart_off_removes_an_expand_sz_value`: значення `NewWinTest-<pid>` типу `REG_EXPAND_SZ` записати через `RegSetKeyValueW`, `autostart_set_named(name, false, "")` → значення немає (`RegGetValueW` з `RRF_RT_ANY` повертає `ERROR_FILE_NOT_FOUND`); прибрати за собою.
  - `stale_tmp_files_go_fresh_ones_stay`: у тимчасовій теці `config.toml.1.0.tmp` з mtime −120 с, `config.toml.1.1.tmp` свіжий, `config.toml` старий → `clean_stale_tmp(dir, 60 s) == 1`, лишилися свіжий `.tmp` і `config.toml`; теку видалити.
- [ ] **Step 2:** `cargo test -p newwin-platform autostart; cargo test -p newwin-config stale` → FAIL.
- [ ] **Step 3:** `autostart_set_named(.., false, ..)` викликає `RegDeleteKeyValueW` без попередньої перевірки типу (`ERROR_FILE_NOT_FOUND` = Ok). `clean_stale_tmp` за mtime. Shell при старті: `clean_stale_tmp(config_dir, Duration::from_secs(60))`, лог `removed N stale temp files`, коли N > 0.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `fix: autostart off removes any value type; stale temp files are cleaned up`.

### Task 6: Розбити `body.rs` і `view.rs`

**Files:**
- Modify/Create: `apps/settings/src/body.rs` → `body.rs` + `body/pointer.rs` (press/move/release/wheel/drag), `body/paint.rs` (paint, paint_control, build_texts, layout), `body/tests.rs`; `apps/settings/src/view.rs` → `view.rs` + `view/banner.rs` (`Banner`, `pick_banner`, `save_reason`, `parse_error_of`, `load_for_view`), `view/focus.rs` (`focus_next`, `shift_focus`, `key_action`, `ctrl_key_action`), `view/tests.rs`.

**Interfaces:** публічні імена й сигнатури не змінюються; лише переміщення (`pub(super)`, де треба).

- [ ] **Step 1:** Перемістити код; кожен файл ≤ 500 рядків.
- [ ] **Step 2:** `cargo test -p newwin-settings` → PASS з тією самою кількістю тестів, що й до зміни; `cargo clippy -p newwin-settings` без нових попереджень.
- [ ] **Step 3: Commit** `refactor: split Settings body and view into smaller files`.

### Task 7: Перетягування рядків

**Files:**
- Modify: `crates/ui/src/reorder_list.rs`, `apps/settings/src/body/list.rs`, `apps/settings/src/body/pointer.rs`, `apps/settings/src/view.rs`
- Test: `crates/ui/tests/controls.rs`, `apps/settings/src/body/tests.rs`

**Interfaces:**
- Produces (`reorder_list.rs`): `pub fn commit_step(&mut self, from: usize, to: usize, now_ms: u64)` — як `commit`, але рядок плавно їде з `from` у `to` (`dropped_offset = from − to` рядків); `pub fn autoscroll_speed(y: f32, top: f32, bottom: f32, zone: f32, max: f32) -> f32` — 0 поза зоною, лінійно до `±max` на краю (від'ємне = вгору).
- Produces (`body`): `pub fn autoscroll(&mut self, now: u64) -> bool` (крок прокрутки за час від попереднього кадру, перераховує слот; true = прокрутилось); `animating()` повертає true, поки триває автопрокрутка.
- Константи: зона **24** логічних px, максимум **600** логічних px/с.

- [ ] **Step 1: Тести.**
  - `autoscroll_speed_is_zero_outside_and_max_at_the_edge`: (`y=top+30`, zone 24) → 0; `y=top` → `−600`; `y=bottom` → `600`; `y=top+12` → `−300`.
  - `a_keyboard_move_glides_the_row`: після `commit_step(0, 2, 0)` `animating(1) == true`, а `animating(theme::REORDER_MS + 1) == false`.
  - `the_wheel_during_a_drag_moves_the_slot` (body): довгий список *Tray* (20 рядків), тягнути рядок 0, колесо вниз на 3 рядки → `release` переносить у слот ≥ 3.
  - `dragging_near_the_bottom_edge_scrolls_the_list`: курсор на нижньому краю, `autoscroll(0)` потім `autoscroll(100)` → `scroll.offset` збільшився.
  - `a_mouse_drag_does_not_scroll_to_the_focused_row`: фокус на рядку 0, тягнути рядок 5 у 6 → `scroll.offset` не змінився після `release`.
- [ ] **Step 2:** `cargo test -p newwin-ui --test controls; cargo test -p newwin-settings` → FAIL.
- [ ] **Step 3:** Реалізувати; колесо під час перетягування — `drag_to` з останньою точкою курсора; клавіатурні переміщення (`Ctrl+↑/↓`) викликають `commit_step`; після мишачого `release` фокус і прокрутка не змінюються (не викликати `reveal`).
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `feat: drag autoscroll, wheel during drag, animated keyboard moves`.

### Task 8: Шаблон дати й каретка

**Files:**
- Modify: `crates/core/src/strings.rs`, `apps/settings/src/sections/appearance.rs` (або місце, де будується приклад шаблону), `crates/ui/src/text_field.rs`, `apps/settings/src/view.rs`
- Test: `crates/core/src/strings.rs`, `crates/ui/tests/controls.rs`, `apps/settings/src/sections/appearance.rs`

**Interfaces:**
- Produces: `Strings::invalid_date_format` (`en`: `"Invalid format"`, `uk`: `"Неправильний формат"`); `pub fn date_example(formatted: &str, s: &Strings) -> String` (порожній результат форматування → `invalid_date_format`); `TextField::set_window_active(&mut self, active: bool)`.

- [ ] **Step 1: Тести.** `an_empty_date_example_says_invalid_format`: `date_example("", en) == "Invalid format"`, `date_example("", uk) == "Неправильний формат"`, `date_example("25.10", en) == "25.10"`. `an_inactive_window_hides_the_caret_and_needs_no_timer`: сфокусоване поле, `set_window_active(false)` → `next_blink(now, Some(530)) == None` і `caret_on(now, Some(530)) == false`; `set_window_active(true)` → `next_blink` знову `Some`.
- [ ] **Step 2:** запустити ці тести → FAIL.
- [ ] **Step 3:** Реалізувати; `SettingsView::message` на `WM_ACTIVATE` викликає `body.field.set_window_active(...)` і перемальовує.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `fix: invalid date pattern says so; caret rests in an inactive window`.

### Task 9: Нестабільні тести й тимчасові теки

**Files:**
- Modify: `crates/ui/tests/backdrop.rs`, `crates/config/tests/tray_seen.rs`, тести, що лишають теки (знайти: `grep -rn "temp_dir()" crates apps --include=*.rs`)

- [ ] **Step 1: Відтворити.** Кожен із двох тестів 50 разів поспіль (`for ($i=0;$i -lt 50;$i++){ cargo test -p newwin-ui --test backdrop an_unchanged_screen_posts_nothing_more -q }`), записати частоту збоїв і повідомлення. Причину шукати за superpowers:systematic-debugging (гонка таймінгу, спільний шлях файла між паралельними тестами тощо).
- [ ] **Step 2:** Виправити причину (не збільшувати таймаути наосліп і не `#[ignore]`).
- [ ] **Step 3:** Тести з тимчасовими теками створюють унікальну теку й видаляють її в `Drop`-guard (навіть при паніці).
- [ ] **Step 4: Перевірка.** Обидва тести 50 разів поспіль → 0 збоїв; `cargo test --workspace` двічі → PASS; у `%TEMP%` після прогону не з'явилося нових тек `newwin*`.
- [ ] **Step 5: Commit** `test: fix flaky backdrop and tray_seen tests; tests clean up temp dirs`.

### Task 10: Інсталятор

**Files:**
- Create: `installer/newwin.iss`, `scripts/build-installer.ps1`
- Modify: `.gitignore` (якщо `target/` уже ігнорується — нічого)

**Interfaces:**
- Consumes: `assets/newwin.ico` (Task 1), `NewWin.exe --quit` (Task 3).

**Передумова:** Inno Setup 6.7.x встановлено (`ISCC.exe`). Завантаження — **лише з дозволу користувача** (джерело `https://jrsoftware.org/isdl.php` або `winget install JRSoftware.InnoSetup`).

`newwin.iss` — ключові значення (решта на розсуд виконавця в межах Global Constraints):
- `AppId={{8F3C2A71-5B4E-4D2A-9C1E-6A7B3D0E4F12}`, `AppName=NewWin`, `AppVersion={#AppVersion}`, `AppPublisher=NewWin`, `DefaultDirName={localappdata}\Programs\NewWin`, `PrivilegesRequired=lowest`, `DisableDirPage=auto`, `DisableWelcomePage=yes`, `DisableProgramGroupPage=yes`, `DisableReadyPage=yes`, `CloseApplications=no`, `ShowLanguageDialog=no`, `LanguageDetectionMethod=uilanguage`, `SetupIconFile=..\assets\newwin.ico`, `UninstallDisplayIcon={app}\NewWin.exe`, `OutputDir=..\target\installer`, `OutputBaseFilename=NewWin-Setup-{#AppVersion}`, `WizardStyle` — модерний темний варіант (у 6.7: `modern dark`; якщо ISCC відкидає, звірити з довідкою `Inno Setup 6\ISetup.chm`), бічна й верхня картинки вимкнені (`WizardImageFile=`/`WizardSmallImageFile=` порожні або 1×1 прозорі, як дозволяє 6.7).
- `[Languages]`: `english` (`Default.isl`), `ukrainian` (`Languages\Ukrainian.isl`).
- `[CustomMessages]`: `CannotCloseNewWin` — en `Could not close NewWin. Close it and run Setup again.`, uk `Не вдалося закрити NewWin. Закрийте його й запустіть інсталятор ще раз.`
- `[Files]`: `NewWin.exe`, `NewWinSettings.exe`, `LICENSE` → `{app}`; `NewWin.exe` ще раз з `Flags: dontcopy` для `ExtractTemporaryFile`.
- `[Icons]`: `{userprograms}\NewWin Settings` → `{app}\NewWinSettings.exe`.
- `[Run]`: `{app}\NewWin.exe`, `Flags: nowait` (без `postinstall`, без `skipifsilent`).
- `[UninstallRun]`: `{app}\NewWin.exe`, `Parameters: --quit`, `Flags: runhidden waituntilterminated`, `RunOnceId: QuitNewWin`.
- `[UninstallDelete]`: `filesandordirs` для `{userappdata}\NewWin` і `{localappdata}\NewWin`.
- `[Code]`: `PrepareToInstall` → `ExtractTemporaryFile('NewWin.exe')`, `Exec('{tmp}\NewWin.exe', '--quit', …, ewWaitUntilTerminated, Code)`; невдача або `Code <> 0` → `Result := CustomMessage('CannotCloseNewWin')`. `CurUninstallStepChanged(usPostUninstall)` → `RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'NewWin')`.

`build-installer.ps1`: знайти `ISCC.exe` (`PATH`, `${env:ProgramFiles(x86)}\Inno Setup 6`, `$env:LOCALAPPDATA\Programs\Inno Setup 6`; інакше помилка з посиланням на jrsoftware.org); якщо є `target\release\NewWin.exe` — запустити `--quit`; `cargo build --release`; версія з `[workspace.package] version` у `Cargo.toml`; `ISCC.exe /DAppVersion=<v> installer\newwin.iss`; вивести шлях до Setup.

- [ ] **Step 1:** Отримати дозвіл і встановити Inno Setup; `ISCC.exe /?` друкує версію 6.7.x.
- [ ] **Step 2:** Написати `newwin.iss` і `build-installer.ps1`.
- [ ] **Step 3:** `.\scripts\build-installer.ps1` → `target\installer\NewWin-Setup-1.0.0.exe` без попереджень ISCC.
- [ ] **Step 4: Тихий прогін (лише з дозволу користувача, бо замінить робочий NewWin):** `NewWin-Setup-1.0.0.exe /VERYSILENT` → є `{localappdata}\Programs\NewWin\NewWin.exe`, процес NewWin з цього шляху працює, `Run\NewWin` указує на нього. `…\unins000.exe /VERYSILENT` → немає процесів, папки програми, `%APPDATA%\NewWin`, `%LOCALAPPDATA%\NewWin`, `Run\NewWin`; таскбар видно. Перед цим скопіювати `%APPDATA%\NewWin` у `%TEMP%\newwin-m5-backup` і відновити після.
- [ ] **Step 5: Commit** `feat: Inno Setup installer and build script`.

### Task 11: README, прибирання, приймання

**Files:**
- Modify: `README.md`, `README.uk.md`
- Create: `docs/superpowers/notes/2026-10-25-m5-acceptance.md`, наступний handoff-файл
- Delete: `.superpowers/sdd/2026-10-23-newwin-m4-settings/`, `%APPDATA%\NewWin\config.backup-m4.toml`

- [ ] **Step 1:** README (обидві мови): розділ *Install* / *Встановлення* — завантажити `NewWin-Setup-<версія>.exe`, запустити, попередження SmartScreen («Докладніше → Все одно запустити»), видалення через «Встановлені програми» (стирає й налаштування). Розділ про збірку з коду лишити для розробників, додати `scripts\build-installer.ps1`.
- [ ] **Step 2:** Видалити теку SDD M4 і `config.backup-m4.toml`.
- [ ] **Step 3:** Фінальне рев'ю всієї гілки (superpowers:requesting-code-review), виправити знахідки.
- [ ] **Step 4:** Живий чекліст специфікації §7 разом із користувачем; виміри простою (`scripts/measure.ps1`, 10 хв) після встановлення.
- [ ] **Step 5:** Нотатки приймання (виміри, чекліст, рішення) і handoff; оновити пам'ять статусу.
- [ ] **Step 6: Commit** `docs: README install section, M5 acceptance notes`. Злиття в `main` — лише з дозволу користувача.
