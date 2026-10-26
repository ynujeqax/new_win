//! Network status on the bar (spec: M2 §2, §4.2): icon, tooltip, click.

use newwin_core::anim::Tween;
use newwin_core::items::RightItem;
use newwin_core::network::{NetIcon, icon_for, tooltip};
use newwin_platform::clock::now_ms;
use newwin_platform::network::{NetworkWatch, current};
use newwin_ui::{Color, Painter, PointF, icons};
use windows::Win32::UI::WindowsAndMessaging::{SetTimer, WM_APP};

use super::App;
use super::apps_view::Target;

pub(super) const WM_APP_NET: u32 = WM_APP + 9;
pub(super) const NET_DEBOUNCE_TIMER: usize = 7;
/// Windows sends a burst of events while a connection comes up.
const DEBOUNCE_MS: u32 = 300;
const CROSSFADE_MS: u32 = 280;

/// The network icon, cross-fading from the previous state.
pub(super) struct NetView {
    pub icon: NetIcon,
    pub prev: Option<NetIcon>,
    pub fade: Tween,
}

impl NetView {
    pub(super) fn animating(&self, now: u64) -> bool {
        self.fade.running(now)
    }

    pub(super) fn paint(&self, p: &mut Painter, c: PointF, s: f32, color: Color, w: f32, now: u64) {
        let f = self.fade.value(now);
        if let Some(prev) = self.prev.filter(|_| f < 1.0) {
            draw(p, prev, c, s, color.with_alpha(color.a * (1.0 - f)), w);
        }
        draw(p, self.icon, c, s, color.with_alpha(color.a * f), w);
    }
}

fn draw(p: &mut Painter, icon: NetIcon, c: PointF, s: f32, color: Color, w: f32) {
    match icon {
        NetIcon::Ethernet { internet } => {
            icons::ethernet(p, c, s, color, w);
            if !internet {
                icons::slash(p, c, s, 1.0, color, w);
            }
        }
        NetIcon::WiFi { bars, internet } => {
            icons::wifi(p, c, s, f32::from(bars), color, w);
            if !internet {
                icons::slash(p, c, s, 1.0, color, w);
            }
        }
        NetIcon::Disconnected => {
            let dim = color.with_alpha(color.a * 0.6);
            icons::ethernet(p, c, s, dim, w);
            icons::slash(p, c, s, 1.0, dim, w);
        }
    }
}

impl App {
    pub(super) fn start_network(&mut self) {
        self.network = NetworkWatch::start(self.ping, WM_APP_NET).map_err(|e| log::error!("network watch: {e}")).ok();
        self.read_network();
    }

    /// An IP Helper or WLAN event: read once things settle.
    pub(super) fn on_network_event(&mut self) {
        unsafe { SetTimer(Some(self.bar), NET_DEBOUNCE_TIMER, DEBOUNCE_MS, None) };
    }

    pub(super) fn read_network(&mut self) {
        let info = current();
        log::debug!("network {info:?}");
        if let Some(w) = self.network.as_mut() {
            w.follow_wifi(&info);
        }
        let icon = icon_for(&info);
        self.net_info = Some(info);
        let first = self.right.net.is_none();
        match self.right.net.as_mut() {
            None => self.right.net = Some(NetView { icon, prev: None, fade: Tween::at_rest(1.0) }),
            Some(v) if v.icon != icon => {
                v.prev = Some(v.icon);
                v.icon = icon;
                v.fade = Tween::at_rest(0.0);
                v.fade.to(1.0, now_ms(), CROSSFADE_MS);
            }
            Some(_) => {}
        }
        if first { self.refresh() } else { self.paint() }
        if self.tooltip.is_some() && self.hover == Some(Target::Right(RightItem::Network)) {
            self.show_tooltip();
        }
    }

    pub(super) fn network_tooltip(&self) -> Option<String> {
        let s = self.strings;
        self.net_info.as_ref().map(|i| tooltip(i, s.not_connected, s.no_internet))
    }
}
