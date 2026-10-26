//! Spotlight 1.0.6 actions (spec: 1.0.6 §2): settings pages, quick links, web search, emoji, and
//! the currency rates fetched in the background.

use std::rc::Rc;

use newwin_core::currency::Rates;
use newwin_core::emoji::usage_key;
use newwin_core::links::{Link, label, target_kind};
use newwin_core::web::{Engine, search_url};
use newwin_platform::launch;
use newwin_platform::worker::Job;
use newwin_ui::spotlight::SpotlightExtras;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::App;
use super::spotlight_view::WM_APP_RATES;

/// After a failed download, the next try waits this long: short, since the first try is often at
/// sign-in, before the network is up.
pub(super) const RATES_RETRY_SECS: u64 = 60;
/// Rates older than this are fetched again at start.
pub(super) const RATES_STARTUP_SECS: u64 = 24 * 3600;

const NBU_URL: &str = "https://bank.gov.ua/NBUStatService/v1/statdirectory/exchange?json";

/// Whether to start a rates download now.
pub(super) fn should_refresh(currency_on: bool, fetching: bool, failed_at: Option<u64>, now: u64) -> bool {
    currency_on && !fetching && failed_at.is_none_or(|t| now.saturating_sub(t) >= RATES_RETRY_SECS)
}

/// At start: fetch when there are no rates or they are a day old.
pub(super) fn refresh_at_start(fetched_at: Option<u64>, now: u64) -> bool {
    fetched_at.is_none_or(|t| now.saturating_sub(t) > RATES_STARTUP_SECS)
}

/// The target of the link named `keyword` now: an open Spotlight may show links from before a
/// config change, so the keyword, not the position, says which one.
pub(super) fn link_target<'a>(links: &'a [newwin_config::LinkConfig], keyword: &str) -> Option<&'a str> {
    links.iter().find(|l| l.keyword == keyword).map(|l| l.target.as_str())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Today, local time, "2026-10-27".
fn today() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
}

/// Where rates come from; a debug build may point it elsewhere (`NEWWIN_RATES_URL`) to try failures.
fn rates_url() -> String {
    if cfg!(debug_assertions)
        && let Ok(url) = std::env::var("NEWWIN_RATES_URL")
    {
        return url;
    }
    NBU_URL.to_string()
}

impl App {
    /// The quick links as the core sees them; `Kind::Link` indexes this list.
    pub(super) fn links(&self) -> Vec<Link> {
        self.config.spotlight.links.iter().map(|l| Link { keyword: l.keyword.clone(), target: l.target.clone() }).collect()
    }

    pub(super) fn spotlight_extras(&self) -> SpotlightExtras {
        let s = &self.config.spotlight;
        SpotlightExtras {
            engine: Engine::from_key(&s.web_search),
            rates: self.rates.clone(),
            currency_on: s.currency,
            uk: self.strings.ukrainian,
            now_secs: now_secs(),
            links: s.links.iter().map(|l| (l.keyword.clone(), (label(&l.target), target_kind(&l.target)))).collect(),
        }
    }

    /// Loads the cached rates; fetches new ones when there are none or they are a day old.
    pub(super) fn start_rates(&mut self) {
        self.rates = newwin_config::rates::load(&newwin_config::rates::rates_path()).map(Rc::new);
        if refresh_at_start(self.rates.as_ref().map(|r| r.fetched_at), now_secs()) {
            self.refresh_rates();
        }
    }

    /// Downloads the rates on the worker, unless currencies are off, a download is running, or the
    /// last one failed less than ten minutes ago.
    pub(super) fn refresh_rates(&mut self) {
        if !should_refresh(self.config.spotlight.currency, self.rates_fetching, self.rates_failed_at, now_secs()) {
            return;
        }
        let ping = self.ping.0 as isize;
        // Its own short-lived thread: a slow network (proxy discovery, a captive portal) must not
        // hold up the worker, which also names windows, fetches icons and launches apps.
        let spawned = std::thread::Builder::new().name("newwin-rates".into()).spawn(move || {
            let url = rates_url();
            let rates = match newwin_platform::http::get(&url, 5000, 262_144) {
                Ok(body) => newwin_core::currency::parse_nbu(&body, &today()).map(|r| Rates { fetched_at: now_secs(), ..r }).or_else(|| {
                    log::warn!("currency rates: no usable answer from {url}");
                    None
                }),
                Err(e) => {
                    log::warn!("currency rates: {e}");
                    None
                }
            };
            if let Some(r) = &rates
                && let Err(e) = newwin_config::save_atomic(&newwin_config::rates::rates_path(), &newwin_config::rates::to_toml(r))
            {
                log::warn!("cannot write rates.toml: {e}");
            }
            let boxed = Box::into_raw(Box::new(rates));
            if unsafe { PostMessageW(Some(HWND(ping as *mut _)), WM_APP_RATES, WPARAM(0), LPARAM(boxed as isize)) }.is_err() {
                drop(unsafe { Box::from_raw(boxed) });
            }
        });
        match spawned {
            Ok(_) => self.rates_fetching = true,
            Err(e) => log::warn!("cannot start the rates download: {e}"),
        }
    }

    pub(super) fn on_rates(&mut self, rates: Option<Rates>) {
        self.rates_fetching = false;
        let Some(r) = rates else {
            self.rates_failed_at = Some(now_secs());
            return;
        };
        log::info!("currency rates for {}: {} currencies", r.date, r.per_uah.len());
        self.rates_failed_at = None;
        let r = Rc::new(r);
        self.rates = Some(r.clone());
        if let Some(p) = &self.spotlight {
            p.update(|v| {
                v.set_rates(r);
            });
        }
    }

    /// Enter on the web row: the query in the chosen engine, in the default browser.
    pub(super) fn search_web(&mut self) {
        let query = self.spotlight.as_ref().and_then(|p| p.read(|v| v.web_query())).flatten();
        self.close_spotlight();
        let (Some(q), Some(engine)) = (query, Engine::from_key(&self.config.spotlight.web_search)) else { return };
        let url = search_url(engine, &q);
        self.submit_job(Job::Run(Box::new(move || {
            if let Err(e) = launch::open_uri(&url) {
                log::warn!("web search: {e}");
            }
        })));
    }

    /// Enter on an emoji: it goes to the clipboard and counts, so it comes first next time.
    pub(super) fn copy_spotlight_emoji(&mut self) {
        let emoji = self.spotlight.as_ref().and_then(|p| p.read(|v| v.selected_emoji())).flatten();
        if let Some(e) = emoji {
            if newwin_platform::clipboard::set_text(e.glyph) {
                self.on_launched(usage_key(e));
            } else {
                log::warn!("cannot copy the emoji");
            }
        }
        self.close_spotlight();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_gate() {
        let now = 1_800_000_000;
        assert!(should_refresh(true, false, None, now));
        assert!(!should_refresh(false, false, None, now), "currency off: no network");
        assert!(!should_refresh(true, true, None, now), "one download at a time");
        assert!(!should_refresh(true, false, Some(now - 20), now), "just failed: wait");
        assert!(should_refresh(true, false, Some(now - 120), now), "two minutes after a failed start (no network yet): a typed query tries again");
    }

    #[test]
    fn a_link_opens_by_its_keyword_whatever_its_place() {
        let l = |k: &str, t: &str| newwin_config::LinkConfig { keyword: k.into(), target: t.into() };
        let links = [l("dl", "D:\\"), l("gh", "https://github.com")];
        assert_eq!(link_target(&links, "gh"), Some("https://github.com"));
        assert_eq!(link_target(&links, "yt"), None, "removed meanwhile: nothing opens");
    }

    #[test]
    fn startup_refresh_only_if_older_than_a_day() {
        let now = 1_800_000_000;
        assert!(refresh_at_start(None, now));
        assert!(!refresh_at_start(Some(now - 3600), now));
        assert!(refresh_at_start(Some(now - RATES_STARTUP_SECS - 1), now));
    }
}
