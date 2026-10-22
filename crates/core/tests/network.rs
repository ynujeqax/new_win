use newwin_core::network::*;

fn info(kind: NetKind, internet: bool, ip: Option<&str>) -> NetInfo {
    NetInfo { kind, internet, name: "Ethernet".into(), ipv4: ip.map(String::from) }
}

#[test]
fn icons() {
    assert_eq!(icon_for(&info(NetKind::Ethernet, true, None)), NetIcon::Ethernet { internet: true });
    assert_eq!(icon_for(&info(NetKind::WiFi { signal: 50 }, false, None)), NetIcon::WiFi { bars: 2, internet: false });
    assert_eq!(icon_for(&info(NetKind::WiFi { signal: 0 }, true, None)), NetIcon::WiFi { bars: 0, internet: true });
    assert_eq!(icon_for(&info(NetKind::WiFi { signal: 100 }, true, None)), NetIcon::WiFi { bars: 3, internet: true });
    assert_eq!(icon_for(&info(NetKind::None, false, None)), NetIcon::Disconnected);
}

#[test]
fn tooltips() {
    assert_eq!(tooltip(&info(NetKind::Ethernet, true, Some("192.168.1.10")), "Not connected", "No internet"), "Ethernet · 192.168.1.10");
    assert_eq!(tooltip(&info(NetKind::Ethernet, true, None), "Not connected", "No internet"), "Ethernet");
    assert_eq!(tooltip(&info(NetKind::Ethernet, false, Some("10.0.0.2")), "Not connected", "No internet"), "Ethernet · No internet");
    assert_eq!(tooltip(&info(NetKind::None, false, None), "Not connected", "No internet"), "Not connected");
}
