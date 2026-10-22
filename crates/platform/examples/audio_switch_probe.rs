//! Throwaway probe for M2's `IPolicyConfig` risk; not used by NewWin.
//!
//! Lists active output devices, makes the next one the default through the undocumented
//! `IPolicyConfig`, checks that Windows reports the change, and switches back at once.
#![allow(non_snake_case)]

use std::ffi::c_void;

use newwin_platform::appinfo::ComGuard;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, Result, interface};

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
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

fn id_of(d: &IMMDevice) -> Result<String> {
    unsafe {
        let p = d.GetId()?;
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        Ok(s)
    }
}

fn default_id(e: &IMMDeviceEnumerator) -> Result<String> {
    id_of(&unsafe { e.GetDefaultAudioEndpoint(eRender, eConsole)? })
}

fn set_default(p: &IPolicyConfig, id: &str) -> Result<()> {
    let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
    for role in [eConsole, eMultimedia, eCommunications] {
        unsafe { p.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()? };
    }
    Ok(())
}

fn main() -> Result<()> {
    let _com = ComGuard::init();
    unsafe {
        let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let list = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut devices = Vec::new();
        for i in 0..list.GetCount()? {
            let d = list.Item(i)?;
            let props = d.OpenPropertyStore(STGM_READ)?;
            let name = props.GetValue(&PKEY_Device_FriendlyName)?.to_string();
            devices.push((id_of(&d)?, name));
        }
        let original = default_id(&e)?;
        for (id, name) in &devices {
            println!("{} {name}", if *id == original { "*" } else { " " });
        }
        if std::env::args().any(|a| a == "--list") {
            return Ok(());
        }
        let Some(pos) = devices.iter().position(|(id, _)| *id == original) else { return Ok(()) };
        if devices.len() < 2 {
            println!("only one device: switching cannot be tried");
            return Ok(());
        }
        let next = &devices[(pos + 1) % devices.len()];
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
        let r = set_default(&policy, &next.0);
        println!("switch to {:?}: {r:?}; default now is it: {}", next.1, default_id(&e)? == next.0);
        let back = set_default(&policy, &original);
        println!("switch back: {back:?}; default restored: {}", default_id(&e)? == original);
    }
    Ok(())
}
