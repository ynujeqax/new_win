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

pub fn tooltip(info: &NetInfo, not_connected: &str, no_internet: &str) -> String {
    match (&info.kind, info.internet, &info.ipv4) {
        (NetKind::None, _, _) => not_connected.to_string(),
        (_, false, _) => format!("{} · {no_internet}", info.name),
        (_, true, Some(ip)) => format!("{} · {ip}", info.name),
        (_, true, None) => info.name.clone(),
    }
}
