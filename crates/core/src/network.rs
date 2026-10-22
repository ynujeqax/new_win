//! Network status on the bar (spec: M2 §2, §4.2): which icon, which tooltip.

#[derive(Clone, Debug, PartialEq)]
pub enum NetKind {
    Ethernet,
    /// `signal` = Wi-Fi quality 0..=100.
    WiFi { signal: u8 },
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetInfo {
    pub kind: NetKind,
    pub internet: bool,
    /// Adapter name, or the SSID for Wi-Fi.
    pub name: String,
    pub ipv4: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetIcon {
    Ethernet { internet: bool },
    WiFi { bars: u8, internet: bool },
    Disconnected,
}

/// Wi-Fi arcs lit for a signal quality.
fn bars_for(signal: u8) -> u8 {
    match signal {
        0 => 0,
        1..=33 => 1,
        34..=66 => 2,
        _ => 3,
    }
}

pub fn icon_for(info: &NetInfo) -> NetIcon {
    match info.kind {
        NetKind::Ethernet => NetIcon::Ethernet { internet: info.internet },
        NetKind::WiFi { signal } => NetIcon::WiFi { bars: bars_for(signal), internet: info.internet },
        NetKind::None => NetIcon::Disconnected,
    }
}

/// What a WLAN notification is about (only these matter to the bar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WlanEvent {
    SignalQuality(u32),
    Connected,
    Disconnected,
    Other,
}

/// Whether a WLAN notification should wake the bar: signal changes arrive every few seconds, but
/// only a change in the arcs shown (`bars`, updated here) is worth a look.
pub fn wlan_wakes(ev: WlanEvent, bars: &mut u8) -> bool {
    match ev {
        WlanEvent::SignalQuality(q) => {
            let b = bars_for(q.min(100) as u8);
            let changed = b != *bars;
            *bars = b;
            changed
        }
        WlanEvent::Connected | WlanEvent::Disconnected => true,
        WlanEvent::Other => false,
    }
}

pub fn tooltip(info: &NetInfo, not_connected: &str, no_internet: &str) -> String {
    match (&info.kind, info.internet, &info.ipv4) {
        (NetKind::None, _, _) => not_connected.to_string(),
        (_, false, _) => format!("{} · {no_internet}", info.name),
        (_, true, Some(ip)) => format!("{} · {ip}", info.name),
        (_, true, None) => info.name.clone(),
    }
}
