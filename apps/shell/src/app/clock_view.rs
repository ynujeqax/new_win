//! Date and time on the right of the bar, updated once per minute.

use newwin_config::Items;
use newwin_core::anim::Tween;
use newwin_core::clock::ms_until_next_minute;
use newwin_core::calendar::Month;
use newwin_platform::clock::{first_weekday, format_bar_date, format_bar_time, local_second_and_millis, month_title, now_ms, today, weekday_names};
use newwin_platform::dpi::scale;
use newwin_ui::PointF;
use newwin_ui::calendar_panel::CalendarPanel;
use newwin_ui::panel::Panel;
use windows::Win32::UI::WindowsAndMessaging::SetTimer;

use super::App;
use super::panels::{ShellPanel, WM_APP_PANEL};

pub(super) const CLOCK_TIMER: usize = 3;
const TIME_ROLL_MS: u32 = 360;

impl App {
    /// Re-reads date/time, repaints if the text changed and re-arms the minute timer.
    pub(super) fn tick_clock(&mut self) {
        let a = &self.config.appearance;
        let date = format_bar_date(&a.date_format);
        let time = format_bar_time(a.show_seconds);
        if date != self.date_text || time != self.time_text {
            if time != self.time_text && !self.time_text.is_empty() {
                self.right.prev_time_text = Some(std::mem::take(&mut self.time_text));
                self.right.time_roll = Tween::at_rest(0.0);
                self.right.time_roll.to(1.0, now_ms(), TIME_ROLL_MS);
            }
            self.date_text = date;
            self.time_text = time;
            self.refresh();
        }
        let (s, ms) = local_second_and_millis();
        let delay = if ticks_every_second(self.config.appearance.show_seconds, &self.config.items) { 1000 - u32::from(ms) + 20 } else { ms_until_next_minute(s, ms) };
        unsafe { SetTimer(Some(self.bar), CLOCK_TIMER, delay, None) };
    }

    /// A click on the date: this month, today marked.
    pub(super) fn toggle_calendar(&mut self) {
        let Some(slot) = self.right.slot(newwin_core::items::RightItem::Date) else { return };
        let style = newwin_ui::popup::PopupStyle {
            font_family: self.config.appearance.font_family.clone(),
            font_size_px: scale(self.config.appearance.font_size, self.dpi),
            dpi: self.dpi,
        };
        let content = CalendarPanel::new(today(), first_weekday(), weekday_names(), Box::new(|m: Month| month_title(m.year, m.month)), &style, &self.gfx);
        let anchor = PointF { x: self.bar_rect.left as f32 + slot.x + slot.w / 2.0, y: self.bar_rect.bottom as f32 + scale(4.0, self.dpi).round() };
        match Panel::show(&self.gfx, content, anchor, self.dpi, self.bar, WM_APP_PANEL) {
            Ok(p) => self.open_panel(ShellPanel::Calendar(p)),
            Err(e) => log::error!("cannot open the calendar: {e}"),
        }
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
