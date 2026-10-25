//! App icons (1.0.1 spec): asked for only when something is shown, fetched on the worker (shell
//! icons) or here (tray icons), cached, and faded in over 150 ms when they arrive.

use std::rc::Rc;

use newwin_core::app_icon::{ICON_FADE_MS, ICON_LOGICAL_PX, IconImage, IconKey, Lookup, fade};
use newwin_core::spotlight::{Kind, SystemCommand};
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::worker::{IconReady, Job};

use super::App;

/// An icon ready to draw: the image and how visible it is now.
pub(super) type Shown = (Rc<IconImage>, f32);

impl App {
    /// The icon side in physical px at the bar's DPI.
    pub(super) fn icon_px(&self) -> u32 {
        scale(ICON_LOGICAL_PX, self.dpi).round() as u32
    }

    /// The icon Explorer shows for `source` (or, when it has none, for `fallback`), if it is here;
    /// otherwise asks the worker once.
    pub(super) fn shell_icon(&mut self, source: &str, fallback: Option<&str>) -> Option<Shown> {
        let key = IconKey::Shell { source: source.to_string(), px: self.icon_px() };
        match self.icons.get(&key) {
            Lookup::Ready(img) => Some((img, self.icon_opacity(&key))),
            Lookup::Ask => {
                let px = self.icon_px();
                self.submit_job(Job::Icon { key, source: source.to_string(), fallback: fallback.map(str::to_string), px });
                None
            }
            Lookup::Missing | Lookup::Waiting => None,
        }
    }

    /// The tray icon `hicon`, converted right here the first time.
    pub(super) fn tray_icon(&mut self, hicon: u32) -> Option<Shown> {
        if hicon == 0 {
            return None;
        }
        let px = self.icon_px();
        let key = IconKey::Tray { hicon, px };
        match self.icons.get(&key) {
            Lookup::Ready(img) => Some((img, self.icon_opacity(&key))),
            Lookup::Ask => {
                let img = newwin_platform::app_icon::hicon_image(hicon, px).map(Rc::new);
                self.store_icon(key.clone(), img.clone());
                img.map(|i| (i, self.icon_opacity(&key)))
            }
            Lookup::Missing | Lookup::Waiting => None,
        }
    }

    pub(super) fn on_icon_ready(&mut self, r: IconReady) {
        self.store_icon(r.key, r.image.map(Rc::new));
        if self.spotlight.is_some() {
            self.fill_spotlight_icons();
        }
        self.refresh_tray_rows();
        self.paint();
        self.keep_animating();
    }

    fn store_icon(&mut self, key: IconKey, img: Option<Rc<IconImage>>) {
        let now = now_ms();
        if img.is_some() {
            self.icon_arrived.retain(|_, at| now.saturating_sub(*at) < ICON_FADE_MS);
            self.icon_arrived.insert(key.clone(), now);
        }
        self.icons.fill(key, img);
    }

    fn icon_opacity(&self, key: &IconKey) -> f32 {
        self.icon_arrived.get(key).map_or(1.0, |&at| fade(at, now_ms()))
    }

    /// An icon is still fading in.
    pub(super) fn icons_animating(&self, now: u64) -> bool {
        self.icon_arrived.values().any(|&at| now.saturating_sub(at) < ICON_FADE_MS)
    }
}

impl App {
    /// Gives the open Spotlight the icons of the rows it shows; asks for the missing ones.
    pub(super) fn fill_spotlight_icons(&mut self) {
        let Some(shown) = self.spotlight.as_ref().and_then(|p| p.read(|v| v.shown())) else { return };
        let px = self.icon_px();
        let catalog = self.shown_catalog();
        // *NewWin Settings* is a command, not an app entry: it shows NewWinSettings.exe's icon.
        let settings = newwin_platform::launch::settings_exe().and_then(|p| p.to_str().map(str::to_string));
        let mut ready = Vec::new();
        for (index, source) in shown {
            let is_settings = catalog.get(index).is_some_and(|c| c.kind == Kind::System(SystemCommand::Settings));
            let Some(source) = source.or_else(|| is_settings.then(|| settings.clone()).flatten()) else { continue };
            if self.shell_icon(&source, None).is_some() {
                let key = IconKey::Shell { source, px };
                if let Lookup::Ready(img) = self.icons.get(&key) {
                    ready.push((index, img, self.icon_arrived.get(&key).copied().unwrap_or(0)));
                }
            }
        }
        if let Some(p) = &self.spotlight {
            p.update(|v| {
                for (index, img, at) in ready {
                    v.set_icon(index, img, at);
                }
            });
        }
    }
}
