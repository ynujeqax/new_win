//! Date and time on the right of the bar, updated once per minute.

use newwin_core::clock::ms_until_next_minute;
use newwin_platform::clock::{format_bar_date, format_bar_time, local_second_and_millis};
use windows::Win32::UI::WindowsAndMessaging::SetTimer;

use super::App;

pub(super) const CLOCK_TIMER: usize = 3;

impl App {
    /// Re-reads date/time, repaints if the text changed and re-arms the minute timer.
    pub(super) fn tick_clock(&mut self) {
        let a = &self.config.appearance;
        let date = format_bar_date(&a.date_format);
        let time = format_bar_time(a.show_seconds);
        if date != self.date_text || time != self.time_text {
            self.date_text = date;
            self.time_text = time;
            self.refresh();
        }
        let (s, ms) = local_second_and_millis();
        let delay = if self.config.appearance.show_seconds { 1000 - u32::from(ms) + 20 } else { ms_until_next_minute(s, ms) };
        unsafe { SetTimer(Some(self.bar), CLOCK_TIMER, delay, None) };
    }
}
