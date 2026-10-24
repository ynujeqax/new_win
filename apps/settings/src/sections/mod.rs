//! The section bodies (spec: M4 §3): each section is a list of [`Row`]s built from the config
//! (`rows`) and a mapping from a control's new value back to the config (`apply`). Both are pure;
//! the window lays the rows out and paints them (`body.rs`).
//!
//! A new section (Items, Tray) adds its module with the same two functions, its row ids to
//! [`RowId`], and an arm to [`rows_for`] and [`apply`].

pub mod about;
pub mod appearance;
pub mod bar;
pub mod spotlight;

use newwin_config::Config;
use newwin_core::strings::Strings;
use newwin_ui::slider::Range;

use crate::view::Section;

/// Every row of every section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowId {
    // Bar
    Mode,
    ShowDelay,
    HideDelay,
    TriggerStrip,
    IgnoreFullscreen,
    StartWithWindows,
    // Appearance
    Glass,
    Height,
    Margin,
    CornerRadius,
    Font,
    FontSize,
    DateFormat,
    DateTemplate,
    DateSample,
    ShowSeconds,
    // Spotlight
    WinKey,
    MaxResults,
    ClearHistory,
    // About
    Version,
    OpenLogs,
    OpenConfig,
    RestoreTaskbar,
}

/// A control's new value, from the window to [`apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowValue {
    Bool(bool),
    Int(i32),
    Choice(usize),
    Text(String),
}

/// The small caption above a group of rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Behavior,
    System,
    Glass,
    Size,
    Text,
    Clock,
}

/// What a row shows at its right, with the value from the config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Toggle(bool),
    /// Labels from [`choices`].
    Choice(usize),
    /// Range from [`range_of`].
    Slider(i32),
    /// The font row: the family's name, painted in itself; a click opens the list.
    Font(String),
    /// A one-line field (the date template), with the placeholder shown while empty.
    Text { text: String, hint: &'static str },
    /// A live sample of a date template (`"system"` = the system format).
    DateSample(String),
    /// A push button; label from [`button_label`].
    Button,
    /// Plain text (the version).
    Info(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: RowId,
    /// The group this row opens (its caption is drawn above it).
    pub group: Option<Group>,
    pub control: Control,
}

impl Row {
    pub fn new(id: RowId, control: Control) -> Row {
        Row { id, group: None, control }
    }

    pub fn opens(mut self, g: Group) -> Row {
        self.group = Some(g);
        self
    }
}

/// The section's rows. `date_custom`: the user picked *Custom* and has not typed a template yet
/// (the config still says `"system"`).
pub fn rows_for(section: Section, c: &Config, date_custom: bool) -> Vec<Row> {
    match section {
        Section::Bar => bar::rows(c),
        Section::Appearance if date_custom => appearance::rows_with(c, true),
        Section::Appearance => appearance::rows(c),
        Section::Spotlight => spotlight::rows(c),
        Section::About => about::rows(c),
        // Task 11.
        Section::Items | Section::Tray => Vec::new(),
    }
}

/// Writes a control's value into the config; values of the wrong kind are ignored.
pub fn apply(c: &mut Config, id: RowId, v: RowValue) {
    use RowId::*;
    match id {
        Mode | ShowDelay | HideDelay | TriggerStrip | IgnoreFullscreen | StartWithWindows => bar::apply(c, id, v),
        Glass | Height | Margin | CornerRadius | Font | FontSize | DateFormat | DateTemplate | DateSample | ShowSeconds => {
            appearance::apply(c, id, v)
        }
        WinKey | MaxResults | ClearHistory => spotlight::apply(c, id, v),
        Version | OpenLogs | OpenConfig | RestoreTaskbar => about::apply(c, id, v),
    }
}

/// A slider's bounds and step (spec §3; the same bounds `Config::sanitized` enforces).
pub fn range_of(id: RowId) -> Range {
    let r = |min, max, step| Range { min, max, step };
    match id {
        RowId::ShowDelay => r(0, 1000, 10),
        RowId::HideDelay => r(0, 2000, 10),
        RowId::Height => r(28, 48, 1),
        RowId::Margin => r(0, 16, 1),
        RowId::CornerRadius => r(0, 24, 1),
        RowId::FontSize => r(10, 18, 1),
        RowId::MaxResults => r(3, 15, 1),
        _ => r(0, 0, 1),
    }
}

/// `v` clamped into the slider's range.
pub fn clamp_to(id: RowId, v: i32) -> i32 {
    let r = range_of(id);
    v.clamp(r.min, r.max.max(r.min))
}

/// The value one step left (`dir` < 0) or right of `v`.
pub fn step(id: RowId, v: i32, dir: i32) -> i32 {
    let r = range_of(id);
    clamp_to(id, v.saturating_add(r.step.max(1).saturating_mul(dir.signum())))
}

/// The row's name at the left (empty for the template field and its sample).
pub fn label(id: RowId, s: &Strings) -> &'static str {
    use RowId::*;
    match id {
        Mode => s.mode,
        ShowDelay => s.show_delay,
        HideDelay => s.hide_delay,
        TriggerStrip => s.trigger_strip,
        IgnoreFullscreen => s.hide_over_fullscreen,
        StartWithWindows => s.start_with_windows,
        Glass => s.liquid_glass,
        Height => s.height,
        Margin => s.margin,
        CornerRadius => s.corner_radius,
        Font => s.font,
        FontSize => s.font_size,
        DateFormat => s.date_format,
        DateTemplate | DateSample => "",
        ShowSeconds => s.show_seconds,
        WinKey => s.win_key_opens_spotlight,
        MaxResults => s.results,
        ClearHistory => s.launch_history,
        Version => s.version,
        OpenLogs | OpenConfig | RestoreTaskbar => "",
    }
}

pub fn group_title(g: Group, s: &Strings) -> &'static str {
    match g {
        Group::Behavior => s.group_behavior,
        Group::System => s.group_system,
        Group::Glass => s.group_glass,
        Group::Size => s.group_size,
        Group::Text => s.group_text,
        Group::Clock => s.group_clock,
    }
}

/// A segmented row's labels.
pub fn choices(id: RowId, s: &Strings) -> Vec<String> {
    match id {
        RowId::Mode => vec![s.auto_hide.into(), s.pinned.into()],
        RowId::TriggerStrip => vec![format!("1 {}", s.unit_px), format!("2 {}", s.unit_px)],
        RowId::DateFormat => vec![s.date_system.into(), s.date_custom.into()],
        _ => Vec::new(),
    }
}

/// A slider's value as shown next to it.
pub fn value_text(id: RowId, v: i32, s: &Strings) -> String {
    match id {
        RowId::ShowDelay | RowId::HideDelay => format!("{v} {}", s.unit_ms),
        RowId::Height | RowId::Margin | RowId::CornerRadius => format!("{v} {}", s.unit_px),
        _ => v.to_string(),
    }
}

/// A button's label; `armed` = waiting for the confirming second click.
pub fn button_label(id: RowId, armed: bool, s: &Strings) -> &'static str {
    match (id, armed) {
        (RowId::ClearHistory, false) => s.clear,
        (RowId::ClearHistory, true) => s.click_again_to_clear,
        (RowId::OpenLogs, _) => s.open_logs,
        (RowId::OpenConfig, _) => s.open_config_file,
        (RowId::RestoreTaskbar, false) => s.restore_taskbar,
        (RowId::RestoreTaskbar, true) => s.click_again_to_restore,
        _ => "",
    }
}

/// How long a first click on *Clear* / *Restore Windows taskbar* waits for the second one.
pub const CONFIRM_MS: u64 = 3000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// The first click: the button now asks for a second one.
    Armed,
    /// The confirming click: do it.
    Fire,
}

/// Two-click confirmation (spec §3.4, §3.6), shared by *Clear* and *Restore Windows taskbar*: the
/// second click within [`CONFIRM_MS`] fires; later, it arms again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Confirm {
    pub armed_at: Option<u64>,
}

impl Confirm {
    pub fn click(&mut self, now: u64) -> Click {
        if self.armed(now) {
            self.armed_at = None;
            Click::Fire
        } else {
            self.armed_at = Some(now);
            Click::Armed
        }
    }

    /// Waiting for the second click at `now`.
    pub fn armed(&self, now: u64) -> bool {
        self.armed_at.is_some_and(|t| now >= t && now - t < CONFIRM_MS)
    }

    /// When the armed state runs out, if armed.
    pub fn expires_at(&self) -> Option<u64> {
        self.armed_at.map(|t| t.saturating_add(CONFIRM_MS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newwin_core::strings::strings_for;

    #[test]
    fn clear_history_needs_a_second_click_within_3_s() {
        let mut c = Confirm::default();
        assert_eq!(c.click(10_000), Click::Armed);
        assert!(c.armed(11_000));
        assert_eq!(c.click(12_000), Click::Fire);
        assert!(!c.armed(12_000), "firing disarms");
        assert_eq!(c.click(20_000), Click::Armed);
        assert!(!c.armed(23_000), "3 s later it is no longer armed");
        assert_eq!(c.click(24_000), Click::Armed, "a click 4 s after the first arms again");
        assert_eq!(c.click(24_500), Click::Fire);
    }

    #[test]
    fn confirm_survives_a_clock_going_backwards() {
        let mut c = Confirm { armed_at: Some(5_000) };
        assert_eq!(c.click(1_000), Click::Armed);
    }

    #[test]
    fn sliders_step_within_their_range() {
        assert_eq!(step(RowId::ShowDelay, 150, 1), 160);
        assert_eq!(step(RowId::ShowDelay, 0, -1), 0);
        assert_eq!(step(RowId::Height, 48, 1), 48);
        assert_eq!(step(RowId::MaxResults, 8, -1), 7);
    }

    #[test]
    fn values_show_their_unit() {
        let en = strings_for("en-US");
        assert_eq!(value_text(RowId::HideDelay, 400, en), "400 ms");
        assert_eq!(value_text(RowId::Margin, 6, en), "6 px");
        assert_eq!(value_text(RowId::MaxResults, 8, en), "8");
        assert_eq!(choices(RowId::TriggerStrip, en), ["1 px", "2 px"]);
    }

    #[test]
    fn confirm_buttons_say_click_again_while_armed() {
        let en = strings_for("en-US");
        assert_eq!(button_label(RowId::ClearHistory, false, en), "Clear");
        assert_eq!(button_label(RowId::ClearHistory, true, en), "Click again to clear");
        assert_eq!(button_label(RowId::RestoreTaskbar, false, en), "Restore Windows taskbar");
        assert_eq!(button_label(RowId::OpenConfig, true, en), "Open config file");
    }

    #[test]
    fn every_row_has_an_apply_path_and_unknown_kinds_are_ignored() {
        let mut c = Config::default();
        apply(&mut c, RowId::Mode, RowValue::Text("pinned".into()));
        apply(&mut c, RowId::Version, RowValue::Bool(true));
        assert_eq!(c, Config::default());
    }
}
