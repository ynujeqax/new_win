//! Date and time on the right of the bar, updated once per minute.

use newwin_config::Items;
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
        let delay = if ticks_every_second(self.config.appearance.show_seconds, &self.config.items) { 1000 - u32::from(ms) + 20 } else { ms_until_next_minute(s, ms) };
        unsafe { SetTimer(Some(self.bar), CLOCK_TIMER, delay, None) };
    }
}

/// Seconds tick only while the bar shows the time; otherwise the clock wakes once a minute.
pub(super) fn ticks_every_second(show_seconds: bool, items: &Items) -> bool {
    show_seconds && super::config_view::visible_items(items).iter().any(|n| n == "time")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(hidden: &[&str]) -> Items {
        Items { hidden: hidden.iter().map(|s| s.to_string()).collect(), ..Items::default() }
    }

    #[test]
    fn seconds_tick_only_while_the_time_shows() {
        assert!(ticks_every_second(true, &items(&[])));
        assert!(ticks_every_second(true, &items(&["date"])));
        assert!(!ticks_every_second(true, &items(&["time"])), "time hidden: minute cadence");
        assert!(!ticks_every_second(true, &items(&["date", "time"])), "no clock item visible");
        assert!(!ticks_every_second(false, &items(&[])));
    }
}
