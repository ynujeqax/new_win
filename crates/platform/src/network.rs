//! Which connection carries traffic, and whether it reaches the internet (spec: M2 §4.2).
//! IP Helper and WLAN call back on their own threads; the callbacks only post a message.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU8, Ordering};

use newwin_core::network::{NetIcon, NetInfo, NetKind, WlanEvent, icon_for, wlan_wakes};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::NetworkManagement::WiFi::*;
use windows::Win32::Networking::WinSock::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{GUID, Result};

const IF_TYPE_IEEE80211: u32 = 71;
/// Route lookup target for "the default connection"; nothing is sent to it.
const ROUTE_PROBE: [u8; 4] = [1, 1, 1, 1];

struct Target {
    notify: isize,
    msg: u32,
    /// Wi-Fi arcs on the bar now; WLAN signal changes that keep them are ignored.
    bars: AtomicU8,
}

fn post(ctx: *const c_void) {
    let Some(t) = (unsafe { (ctx as *const Target).as_ref() }) else { return };
    unsafe {
        let _ = PostMessageW(Some(HWND(t.notify as *mut _)), t.msg, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn on_interface(ctx: *const c_void, _: *const MIB_IPINTERFACE_ROW, _: MIB_NOTIFICATION_TYPE) {
    post(ctx);
}

unsafe extern "system" fn on_hint(ctx: *const c_void, _: NL_NETWORK_CONNECTIVITY_HINT) {
    post(ctx);
}

unsafe extern "system" fn on_wlan(data: *mut L2_NOTIFICATION_DATA, ctx: *mut c_void) {
    let (Some(d), Some(t)) = (unsafe { data.as_ref() }, unsafe { (ctx as *const Target).as_ref() }) else { return };
    let ev = if d.NotificationSource != WLAN_NOTIFICATION_SOURCE_MSM {
        WlanEvent::Other
    } else if d.NotificationCode == wlan_notification_msm_signal_quality_change.0 as u32 && d.dwDataSize >= 4 && !d.pData.is_null() {
        WlanEvent::SignalQuality(unsafe { *(d.pData as *const u32) })
    } else if d.NotificationCode == wlan_notification_msm_connected.0 as u32 {
        WlanEvent::Connected
    } else if d.NotificationCode == wlan_notification_msm_disconnected.0 as u32 {
        WlanEvent::Disconnected
    } else {
        WlanEvent::Other
    };
    let mut bars = t.bars.load(Ordering::Relaxed);
    if wlan_wakes(ev, &mut bars) {
        t.bars.store(bars, Ordering::Relaxed);
        post(ctx);
    }
}

/// Event subscriptions; dropping it cancels them.
pub struct NetworkWatch {
    target: Box<Target>,
    interface: HANDLE,
    hint: HANDLE,
    wlan: Option<HANDLE>,
}

impl NetworkWatch {
    /// Posts `msg` to `notify` on any interface or connectivity change (the caller debounces).
    pub fn start(notify: HWND, msg: u32) -> Result<NetworkWatch> {
        let target = Box::new(Target { notify: notify.0 as isize, msg, bars: AtomicU8::new(0) });
        let ctx = &*target as *const Target as *const c_void;
        let mut w = NetworkWatch { target, interface: HANDLE::default(), hint: HANDLE::default(), wlan: None };
        unsafe {
            NotifyIpInterfaceChange(AF_UNSPEC, Some(on_interface), Some(ctx), false, &mut w.interface).ok()?;
            if let Err(e) = NotifyNetworkConnectivityHintChange(Some(on_hint), Some(ctx), false, &mut w.hint).ok() {
                log::warn!("network: connectivity hints unavailable: {e}");
            }
        }
        Ok(w)
    }

    /// Signal strength changes are only watched while Wi-Fi is the connection (`info`).
    pub fn follow_wifi(&mut self, info: &NetInfo) {
        if let NetIcon::WiFi { bars, .. } = icon_for(info) {
            self.target.bars.store(bars, Ordering::Relaxed);
        }
        let on = matches!(info.kind, NetKind::WiFi { .. });
        match (on, self.wlan) {
            (true, None) => unsafe {
                let mut version = 0;
                let mut h = HANDLE::default();
                if WlanOpenHandle(2, None, &mut version, &mut h) != 0 {
                    return;
                }
                let ctx = &*self.target as *const Target as *const c_void;
                if WlanRegisterNotification(h, WLAN_NOTIFICATION_SOURCE_MSM, true, Some(on_wlan), Some(ctx), None, None) == 0 {
                    self.wlan = Some(h);
                } else {
                    WlanCloseHandle(h, None);
                }
            },
            (false, Some(h)) => {
                unsafe { WlanCloseHandle(h, None) };
                self.wlan = None;
            }
            _ => {}
        }
    }
}

impl Drop for NetworkWatch {
    fn drop(&mut self) {
        // After these return no callback runs, so the target box may go.
        unsafe {
            if !self.interface.is_invalid() {
                let _ = CancelMibChangeNotify2(self.interface);
            }
            if !self.hint.is_invalid() {
                let _ = CancelMibChangeNotify2(self.hint);
            }
        }
        if let Some(h) = self.wlan.take() {
            unsafe { WlanCloseHandle(h, None) };
        }
    }
}

fn best_interface() -> Option<u32> {
    let addr = SOCKADDR_IN { sin_family: AF_INET, sin_port: 0, sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(ROUTE_PROBE) } }, sin_zero: [0; 8] };
    let mut index = 0;
    (unsafe { GetBestInterfaceEx(&addr as *const _ as *const SOCKADDR, &mut index) } == 0).then_some(index)
}

fn has_internet() -> bool {
    let mut hint = NL_NETWORK_CONNECTIVITY_HINT::default();
    unsafe { GetNetworkConnectivityHint(&mut hint) }.is_ok() && hint.ConnectivityLevel == NetworkConnectivityLevelHintInternetAccess
}

/// (SSID, signal quality) of the Wi-Fi interface named `adapter` ("{GUID}").
fn wifi_details(adapter: &str) -> Option<(String, u8)> {
    let guid = GUID::try_from(adapter.trim_matches(['{', '}'])).ok()?;
    unsafe {
        let mut version = 0;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut version, &mut h) != 0 {
            return None;
        }
        let mut size = 0;
        let mut data: *mut c_void = std::ptr::null_mut();
        let r = WlanQueryInterface(h, &guid, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None);
        let out = (r == 0 && !data.is_null() && size as usize >= size_of::<WLAN_CONNECTION_ATTRIBUTES>()).then(|| {
            let a = &(*(data as *const WLAN_CONNECTION_ATTRIBUTES)).wlanAssociationAttributes;
            let len = (a.dot11Ssid.uSSIDLength as usize).min(a.dot11Ssid.ucSSID.len());
            (String::from_utf8_lossy(&a.dot11Ssid.ucSSID[..len]).into_owned(), a.wlanSignalQuality.min(100) as u8)
        });
        if !data.is_null() {
            WlanFreeMemory(data);
        }
        WlanCloseHandle(h, None);
        out
    }
}

/// The connection carrying the default route right now.
pub fn current() -> NetInfo {
    let none = NetInfo { kind: NetKind::None, internet: false, name: String::new(), ipv4: None };
    let Some(index) = best_interface() else { return none };
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = 16 * 1024u32;
    let mut buf: Vec<u64> = Vec::new();
    // The list can grow between the size query and the read; retry a couple of times.
    for _ in 0..3 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        let r = unsafe { GetAdaptersAddresses(u32::from(AF_INET.0), flags, None, Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH), &mut size) };
        if r == 0 {
            break;
        }
        if r != 111 {
            // Anything but ERROR_BUFFER_OVERFLOW.
            return none;
        }
    }
    let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while let Some(a) = unsafe { p.as_ref() } {
        if unsafe { a.Anonymous1.Anonymous.IfIndex } == index {
            let name = unsafe { a.FriendlyName.to_string() }.unwrap_or_default();
            let adapter = unsafe { a.AdapterName.to_string() }.unwrap_or_default();
            let ipv4 = unsafe { a.FirstUnicastAddress.as_ref() }.and_then(|u| {
                let sa = unsafe { (u.Address.lpSockaddr as *const SOCKADDR_IN).as_ref() }?;
                (sa.sin_family == AF_INET).then(|| {
                    let b = unsafe { sa.sin_addr.S_un.S_un_b };
                    format!("{}.{}.{}.{}", b.s_b1, b.s_b2, b.s_b3, b.s_b4)
                })
            });
            let internet = has_internet();
            return match a.IfType {
                IF_TYPE_IEEE80211 => {
                    let (ssid, signal) = wifi_details(&adapter).unwrap_or((name.clone(), 0));
                    NetInfo { kind: NetKind::WiFi { signal }, internet, name: if ssid.is_empty() { name } else { ssid }, ipv4 }
                }
                // Ethernet, and anything else carrying the default route (USB tethering, VPN),
                // shows as wired.
                _ => NetInfo { kind: NetKind::Ethernet, internet, name, ipv4 },
            };
        }
        p = a.Next;
    }
    none
}
