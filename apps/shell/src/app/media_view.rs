//! Now playing on the bar (1.0.5): the track, a click opens the media panel, a middle click
//! plays or pauses.

use newwin_core::anim::Tween;
use newwin_core::items::RightItem;
use newwin_core::media::{MediaCmd, MediaInfo};
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::media::{MediaWatch, take_media};
use newwin_ui::PointF;
use newwin_ui::TextLayout;
use newwin_ui::media_panel::{MediaPanel, decode};
use newwin_ui::panel::Panel;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::App;
use super::apps_view::Target;
use super::panels::{ShellPanel, WM_APP_PANEL};

pub(super) const WM_APP_MEDIA: u32 = WM_APP + 22;
/// A new track rolls in like the clock.
const ROLL_MS: u32 = 320;
/// The longest the track's text gets on the bar (96 DPI); longer ones end in "…".
pub(super) const LABEL_MAX: f32 = 220.0;

pub(super) struct MediaView {
    pub info: MediaInfo,
    pub text: Option<TextLayout>,
    pub prev_label: Option<String>,
    pub prev_text: Option<TextLayout>,
    pub roll: Tween,
}

/// A readable name for the app that plays: its Start menu name, else its exe or id, tidied.
fn app_name(app: &str) -> String {
    if let Some(n) = newwin_platform::appinfo::appsfolder_display_name(app) {
        return n;
    }
    let tail = app.rsplit(['\\', '!']).next().unwrap_or(app);
    let stem = tail.strip_suffix(".exe").or_else(|| tail.strip_suffix(".EXE")).unwrap_or(tail);
    let mut c = stem.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

impl App {
    pub(super) fn start_media(&mut self) {
        self.media = Some(MediaWatch::start(self.ping, WM_APP_MEDIA));
    }

    pub(super) fn on_media(&mut self, lp: LPARAM) {
        let info = *unsafe { take_media(lp) };
        let now = now_ms();
        match (info, self.right.media.as_mut()) {
            (None, _) => {
                self.right.media = None;
                if let Some(ShellPanel::Media(_)) = &self.panel {
                    self.begin_close_panel();
                }
            }
            (Some(info), Some(v)) => {
                if info.label() != v.info.label() {
                    v.prev_label = Some(v.info.label());
                    v.roll = Tween::at_rest(0.0);
                    v.roll.to(1.0, now, ROLL_MS);
                }
                v.info = info;
            }
            (Some(info), None) => {
                let mut roll = Tween::at_rest(0.0);
                roll.to(1.0, now, ROLL_MS);
                self.right.media = Some(MediaView { info, text: None, prev_label: None, prev_text: None, roll });
            }
        }
        if let (Some(ShellPanel::Media(p)), Some(v)) = (&self.panel, &self.right.media) {
            let (info, name) = (v.info.clone(), app_name(&v.info.app));
            p.update(|m| m.set_info(info, name, now));
        }
        self.refresh();
        if self.tooltip.is_some() && self.hover == Some(Target::Right(RightItem::Media)) {
            self.show_tooltip();
        }
    }

    pub(super) fn media_tooltip(&self) -> Option<String> {
        let v = self.right.media.as_ref()?;
        Some(format!("{} — {}", v.info.label(), app_name(&v.info.app)))
    }

    pub(super) fn media_command(&self, c: MediaCmd) {
        if let Some(m) = &self.media {
            m.send(c);
        }
    }

    pub(super) fn toggle_media_panel(&mut self) {
        let (Some(slot), Some(v)) = (self.right.slot(RightItem::Media), &self.right.media) else { return };
        let style = newwin_ui::popup::PopupStyle {
            font_family: self.config.appearance.font_family.clone(),
            font_size_px: scale(self.config.appearance.font_size, self.dpi),
            dpi: self.dpi,
        };
        let content = MediaPanel::new(v.info.clone(), app_name(&v.info.app), &style, &self.gfx);
        let anchor = PointF { x: self.bar_rect.left as f32 + slot.x + slot.w / 2.0, y: self.under_bar() };
        match Panel::show(&self.gfx, content, anchor, self.dpi, self.bar, WM_APP_PANEL) {
            Ok(p) => self.open_panel(ShellPanel::Media(p)),
            Err(e) => log::error!("cannot open the media panel: {e}"),
        }
    }

    pub(super) fn on_media_action(&mut self, id: u32) {
        if let Some(c) = decode(id) {
            self.media_command(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_are_tidied() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name(r"C:\Apps\vlc.exe"), "Vlc");
    }
}
