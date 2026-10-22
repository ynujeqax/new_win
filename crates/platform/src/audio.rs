//! The default output device: volume, mute, the list of devices and switching between them
//! (spec: M2 §4.1). Windows calls the callbacks on its own threads; they only post a message.
#![allow(non_snake_case)]

use std::ffi::c_void;

use newwin_core::audio::percent;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{HWND, LPARAM, PROPERTYKEY, WPARAM};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, Result, implement, interface};

/// Posted `wParam`s; for `AUDIO_VOLUME`, `lParam` = level | muted << 8.
pub const AUDIO_VOLUME: usize = 1;
pub const AUDIO_DEVICES: usize = 2;

/// Marks volume changes NewWin made itself, so their echo is not reported back.
const EVENT_CONTEXT: GUID = GUID::from_u128(0x5f3c_8a1e_2b7d_4c90_9e61_0a4d_b2c3_e7f8);

/// `CPolicyConfigClient`: undocumented, but the only way to change the default device
/// (checked on 26100 by `examples/audio_switch_probe.rs`).
const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    // Slots before SetDefaultEndpoint are never called; they only keep the vtable order.
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSnapshot {
    pub level: u8,
    pub muted: bool,
    pub devices: Vec<AudioDevice>,
    pub default_id: Option<String>,
}

fn post(notify: isize, msg: u32, wp: usize, lp: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(wp), LPARAM(lp));
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    notify: isize,
    msg: u32,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        let Some(d) = (unsafe { data.as_ref() }) else { return Ok(()) };
        if d.guidEventContext != EVENT_CONTEXT {
            let lp = isize::from(percent(d.fMasterVolume)) | (isize::from(d.bMuted.as_bool()) << 8);
            post(self.notify, self.msg, AUDIO_VOLUME, lp);
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceCallback {
    notify: isize,
    msg: u32,
}

impl DeviceCallback_Impl {
    fn changed(&self) -> Result<()> {
        post(self.notify, self.msg, AUDIO_DEVICES, 0);
        Ok(())
    }
}

impl IMMNotificationClient_Impl for DeviceCallback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        self.changed()
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        self.changed()
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        self.changed()
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eConsole { self.changed() } else { Ok(()) }
    }

    /// Frequent and never about what the bar shows.
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

fn id_of(d: &IMMDevice) -> Option<String> {
    unsafe {
        let p = d.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn name_of(d: &IMMDevice) -> Option<String> {
    let name = unsafe { d.OpenPropertyStore(STGM_READ).ok()?.GetValue(&PKEY_Device_FriendlyName).ok()? }.to_string();
    (!name.is_empty()).then_some(name)
}

pub struct AudioService {
    enumerator: Option<IMMDeviceEnumerator>,
    endpoint: Option<IAudioEndpointVolume>,
    volume_cb: IAudioEndpointVolumeCallback,
    device_cb: IMMNotificationClient,
}

impl AudioService {
    /// Never fails: without devices (or with the audio service down) it reports nothing and
    /// waits for the next device notification.
    pub fn start(notify: HWND, msg: u32) -> AudioService {
        let n = notify.0 as isize;
        let mut s = AudioService {
            enumerator: unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| log::warn!("audio: no device enumerator: {e}")).ok(),
            endpoint: None,
            volume_cb: VolumeCallback { notify: n, msg }.into(),
            device_cb: DeviceCallback { notify: n, msg }.into(),
        };
        if let Some(e) = &s.enumerator
            && let Err(err) = unsafe { e.RegisterEndpointNotificationCallback(&s.device_cb) }
        {
            log::warn!("audio: device notifications unavailable: {err}");
        }
        s.rebind();
        s
    }

    /// Follows the current default output device (call on `AUDIO_DEVICES`).
    pub fn rebind(&mut self) {
        if let Some(old) = self.endpoint.take() {
            let _ = unsafe { old.UnregisterControlChangeNotify(&self.volume_cb) };
        }
        let Some(e) = &self.enumerator else { return };
        let endpoint = unsafe { e.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)) };
        match endpoint {
            Ok(v) => {
                if let Err(err) = unsafe { v.RegisterControlChangeNotify(&self.volume_cb) } {
                    log::warn!("audio: volume notifications unavailable: {err}");
                }
                self.endpoint = Some(v);
            }
            Err(err) => log::info!("audio: no default output device ({err})"),
        }
    }

    pub fn snapshot(&self) -> Option<AudioSnapshot> {
        let v = self.endpoint.as_ref()?;
        let e = self.enumerator.as_ref()?;
        unsafe {
            let level = percent(v.GetMasterVolumeLevelScalar().ok()?);
            let muted = v.GetMute().ok()?.as_bool();
            let list = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).ok()?;
            let mut devices = Vec::new();
            for i in 0..list.GetCount().unwrap_or(0) {
                let Ok(d) = list.Item(i) else { continue };
                if let (Some(id), Some(name)) = (id_of(&d), name_of(&d)) {
                    devices.push(AudioDevice { id, name });
                }
            }
            let default_id = e.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| id_of(&d)).filter(|id| devices.iter().any(|d| &d.id == id));
            Some(AudioSnapshot { level, muted, devices, default_id })
        }
    }

    pub fn set_level(&self, level: u8) {
        if let Some(v) = &self.endpoint {
            let _ = unsafe { v.SetMasterVolumeLevelScalar(f32::from(level.min(100)) / 100.0, &EVENT_CONTEXT) };
        }
    }

    pub fn set_muted(&self, muted: bool) {
        if let Some(v) = &self.endpoint {
            let _ = unsafe { v.SetMute(muted, &EVENT_CONTEXT) };
        }
    }

    /// Makes `id` the default output for every role (console, multimedia, communications).
    pub fn set_default(&self, id: &str) -> Result<()> {
        let policy: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)? };
        let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
        for role in [eConsole, eMultimedia, eCommunications] {
            unsafe { policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()? };
        }
        Ok(())
    }
}

impl Drop for AudioService {
    fn drop(&mut self) {
        unsafe {
            if let Some(v) = &self.endpoint {
                let _ = v.UnregisterControlChangeNotify(&self.volume_cb);
            }
            if let Some(e) = &self.enumerator {
                let _ = e.UnregisterEndpointNotificationCallback(&self.device_cb);
            }
        }
    }
}
