//! A small HTTPS GET over WinHTTP (spec: 1.0.6 §2.3), for the currency rates: the system proxy,
//! a timeout, a cap on the body. Blocking: call it on the worker, never on the UI thread.

use std::ffi::c_void;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{HSTRING, PCWSTR};

/// A WinHTTP handle, closed when dropped.
struct Handle(*mut c_void);

impl Handle {
    fn new(h: *mut c_void, what: &str) -> Result<Handle, String> {
        if h.is_null() { Err(format!("{what}: {}", windows::core::Error::from_thread())) } else { Ok(Handle(h)) }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// (host, port, path and query) of an `https://` URL.
fn split_url(url: &str) -> Result<(&str, u16, &str), String> {
    let rest = url.strip_prefix("https://").ok_or_else(|| format!("not an https URL: {url}"))?;
    let (authority, path) = rest.find(['/', '?']).map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| format!("bad port in {url}"))?),
        None => (authority, INTERNET_DEFAULT_HTTPS_PORT),
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(format!("no host in {url}"));
    }
    Ok((host, port, path))
}

/// The body of a 200 response as text; an error for anything else, a body over `max_bytes`, or
/// no answer within `timeout_ms` per step.
pub fn get(url: &str, timeout_ms: u32, max_bytes: usize) -> Result<String, String> {
    let (host, port, path) = split_url(url)?;
    let t = i32::try_from(timeout_ms).unwrap_or(i32::MAX);
    let agent = HSTRING::from(concat!("NewWin/", env!("CARGO_PKG_VERSION")));
    unsafe {
        let session = Handle::new(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "open")?;
        WinHttpSetTimeouts(session.0, t, t, t, t).map_err(|e| format!("timeouts: {e}"))?;
        let connect = Handle::new(WinHttpConnect(session.0, &HSTRING::from(host), port, 0), "connect")?;
        let request = Handle::new(
            WinHttpOpenRequest(connect.0, &HSTRING::from("GET"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "request",
        )?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| format!("send: {e}"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| format!("receive: {e}"))?;
        let (mut status, mut len) = (0u32, 4u32);
        WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as *mut c_void), &mut len, std::ptr::null_mut())
            .map_err(|e| format!("status: {e}"))?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut body = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read).map_err(|e| format!("read: {e}"))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > max_bytes {
                return Err(format!("body over {max_bytes} bytes"));
            }
        }
        String::from_utf8(body).map_err(|_| "body is not UTF-8".to_string())
    }
}
