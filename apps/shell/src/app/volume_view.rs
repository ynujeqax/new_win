//! Volume on the bar (spec: M2 §2, §3.2): the speaker icon, wheel and middle click, the panel.

use newwin_core::anim::Tween;
use newwin_core::audio::{speaker_icon, step, SpeakerIcon};
use newwin_core::items::RightItem;
use newwin_platform::audio::{AUDIO_DEVICES, AUDIO_VOLUME, AudioService, AudioSnapshot};
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::launch::shell_execute;
use newwin_ui::panel::Panel;
use newwin_ui::volume_panel::{VolumeAction, VolumePanel, decode};
use newwin_ui::PointF;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::App;
use super::apps_view::Target;
use super::panels::{ShellPanel, WM_APP_PANEL};
use super::right_view::SpeakerView;

pub(super) const WM_APP_AUDIO: u32 = WM_APP + 8;
const WAVE_MS: u32 = 160;
const MUTE_MS: u32 = 240;
/// Volume per wheel notch over the speaker.
const WHEEL_STEP: u8 = 2;

impl App {
    pub(super) fn start_audio(&mut self) {
        self.audio = Some(AudioService::start(self.ping, WM_APP_AUDIO));
        self.right.speaker = Some(SpeakerView { available: false, waves: Tween::at_rest(0.0), mute: Tween::at_rest(0.0) });
        self.refresh_volume();
    }

    /// Re-reads everything from Windows (start, device changes).
    fn refresh_volume(&mut self) {
        let snap = self.audio.as_ref().and_then(AudioService::snapshot);
        self.volume_device = snap
            .as_ref()
            .and_then(|s| s.devices.iter().find(|d| Some(&d.id) == s.default_id.as_ref()))
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.set_volume_state(snap.as_ref().map(|s| (s.level, s.muted)));
        if let Some(ShellPanel::Volume(p)) = &self.panel {
            p.update(|v| v.set_snapshot(snap, now_ms()));
        }
        self.sync_glass();
    }

    /// The speaker (and an open panel or tooltip) follow `state` = (level, muted).
    fn set_volume_state(&mut self, state: Option<(u8, bool)>) {
        let first = self.right.slot(RightItem::Volume).is_none();
        self.volume = state;
        let now = now_ms();
        if let Some(sp) = self.right.speaker.as_mut() {
            match speaker_icon(state) {
                SpeakerIcon::Unavailable => sp.available = false,
                SpeakerIcon::Level { waves, muted } => {
                    sp.available = true;
                    let from = sp.waves.target();
                    let dur = (WAVE_MS as f32 * (f32::from(waves) - from).abs()).max(1.0) as u32;
                    sp.waves.to(f32::from(waves), now, dur);
                    sp.mute.to(if muted { 1.0 } else { 0.0 }, now, MUTE_MS);
                }
            }
        }
        if first { self.refresh() } else { self.paint() }
        if self.tooltip.is_some() && self.hover == Some(Target::Right(RightItem::Volume)) {
            self.show_tooltip();
        }
    }

    pub(super) fn volume_tooltip(&self) -> Option<String> {
        let (level, muted) = self.volume?;
        let name = if self.volume_device.is_empty() { "Sound" } else { &self.volume_device };
        Some(if muted { format!("{name} · {level}% · muted") } else { format!("{name} · {level}%") })
    }

    pub(super) fn on_audio(&mut self, wp: WPARAM, lp: LPARAM) {
        match wp.0 {
            AUDIO_VOLUME => {
                let state = ((lp.0 & 0xFF) as u8, (lp.0 >> 8) & 1 != 0);
                self.set_volume_state(Some(state));
                if let Some(ShellPanel::Volume(p)) = &self.panel {
                    let snap = self.audio.as_ref().and_then(AudioService::snapshot);
                    p.update(|v| v.set_snapshot(snap, now_ms()));
                }
            }
            AUDIO_DEVICES => {
                if let Some(a) = self.audio.as_mut() {
                    a.rebind();
                }
                self.refresh_volume();
            }
            _ => {}
        }
    }

    /// Our own change: Windows does not echo it back to us.
    fn apply_level(&mut self, level: u8) {
        let Some((_, muted)) = self.volume else { return };
        if let Some(a) = &self.audio {
            a.set_level(level);
        }
        self.set_volume_state(Some((level, muted)));
    }

    pub(super) fn toggle_mute(&mut self) {
        let Some((level, muted)) = self.volume else { return };
        if let Some(a) = &self.audio {
            a.set_muted(!muted);
        }
        self.set_volume_state(Some((level, !muted)));
        if let Some(ShellPanel::Volume(p)) = &self.panel {
            let snap = self.audio.as_ref().and_then(AudioService::snapshot);
            p.update(|v| v.set_snapshot(snap, now_ms()));
        }
    }

    /// Wheel over the bar: only the speaker listens.
    pub(super) fn on_wheel(&mut self, delta: i16, p: PointF) {
        if self.hit(p).map(|(t, _)| t) != Some(Target::Right(RightItem::Volume)) {
            return;
        }
        let n = self.volume_wheel.notches(delta);
        let Some((level, _)) = self.volume else { return };
        if n != 0 {
            self.apply_level(step(level, n, WHEEL_STEP));
        }
        // The level shows right away while scrolling.
        self.show_tooltip();
    }

    pub(super) fn toggle_volume_panel(&mut self) {
        let Some(slot) = self.right.slot(RightItem::Volume) else { return };
        let snap: Option<AudioSnapshot> = match self.audio.as_mut() {
            Some(a) => {
                if a.snapshot().is_none() {
                    a.rebind();
                }
                a.snapshot()
            }
            None => None,
        };
        let s = self.strings;
        let style = newwin_ui::popup::PopupStyle {
            font_family: self.config.appearance.font_family.clone(),
            font_size_px: scale(self.config.appearance.font_size, self.dpi),
            dpi: self.dpi,
        };
        let content = VolumePanel::new(snap, (s.no_output_devices, s.sound_settings), &style, &self.gfx);
        let anchor = PointF { x: self.bar_rect.left as f32 + slot.x + slot.w / 2.0, y: self.under_bar() };
        match Panel::show(&self.gfx, content, anchor, self.dpi, self.bar, WM_APP_PANEL) {
            Ok(p) => self.open_panel(ShellPanel::Volume(p)),
            Err(e) => log::error!("cannot open the volume panel: {e}"),
        }
    }

    pub(super) fn on_volume_action(&mut self, id: u32) {
        match decode(id) {
            Some(VolumeAction::SetLevel(l)) => self.apply_level(l),
            Some(VolumeAction::ToggleMute) => self.toggle_mute(),
            Some(VolumeAction::PickDevice(i)) => {
                let id = self.audio.as_ref().and_then(AudioService::snapshot).and_then(|s| s.devices.get(i).map(|d| d.id.clone()));
                let (Some(a), Some(id)) = (&self.audio, id) else { return };
                if let Err(e) = a.set_default(&id) {
                    if !self.audio_switch_warned {
                        self.audio_switch_warned = true;
                        log::warn!("switching the output device failed: {e}");
                    }
                    if let Some(ShellPanel::Volume(p)) = &self.panel {
                        p.update(VolumePanel::show_settings_row);
                    }
                    self.sync_glass();
                }
            }
            Some(VolumeAction::SoundSettings) => {
                if let Err(e) = shell_execute("open", "ms-settings:sound", "") {
                    log::error!("sound settings: {e}");
                }
            }
            None => {}
        }    }
}
