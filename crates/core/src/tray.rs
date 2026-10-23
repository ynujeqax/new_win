//! The notification-area protocol (`Shell_NotifyIcon` over `WM_COPYDATA`) and the icon model.
//!
//! Buffers come from other processes, so parsing never panics: every field is read through a
//! bounds-checked helper, and a field outside the readable size is simply absent.

pub const TRAY_SIGNATURE: u32 = 0x3475_3423;

pub const NIF_MESSAGE: u32 = 0x1;
pub const NIF_TIP: u32 = 0x4;
pub const NIF_STATE: u32 = 0x8;
pub const NIF_GUID: u32 = 0x20;
pub const NIS_HIDDEN: u32 = 0x1;

/// Sizes of the 32-bit `NOTIFYICONDATAW` versions shell32 sends.
pub const NID_V1_SIZE: usize = 152;
pub const NID_V2_SIZE: usize = 936;
pub const NID_V3_SIZE: usize = 952;
pub const NID_FULL_SIZE: usize = 956;

/// Offsets from the start of the buffer (8-byte header, then the structure).
const HEADER: usize = 8;
const OFF_CB_SIZE: usize = 8;
const OFF_HWND: usize = 12;
const OFF_ID: usize = 16;
const OFF_FLAGS: usize = 20;
const OFF_CALLBACK: usize = 24;
const OFF_TIP: usize = 32;
const OFF_STATE: usize = 288;
const OFF_STATE_MASK: usize = 292;
const OFF_VERSION: usize = 808;
const OFF_GUID: usize = 944;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Add,
    Modify,
    Delete,
    SetFocus,
    SetVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconKey {
    Id { hwnd: u32, id: u32 },
    /// Windows identifies a GUID icon by its GUID alone; its window may change.
    Guid(u128),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotifyData {
    pub command: TrayCommand,
    pub hwnd: u32,
    /// `uID`, sent even with a GUID key (Electron finds its icon by it).
    pub id: u32,
    pub key: IconKey,
    pub flags: u32,
    pub callback_message: u32,
    pub tip: Option<String>,
    pub state: u32,
    pub state_mask: u32,
    pub version: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    BadSignature,
    BadCommand(u32),
    BadSize(u32),
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// UTF-16 up to the first NUL or the end of the field.
fn utf16_at(b: &[u8], at: usize, units: usize) -> Option<String> {
    let field = b.get(at..at + units * 2)?;
    let wide: Vec<u16> = field.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&u| u != 0).collect();
    Some(String::from_utf16_lossy(&wide))
}

pub fn parse_notify(bytes: &[u8]) -> Result<NotifyData, ParseError> {
    if bytes.len() < HEADER + NID_V1_SIZE {
        return Err(ParseError::TooShort);
    }
    if u32_at(bytes, 0) != Some(TRAY_SIGNATURE) {
        return Err(ParseError::BadSignature);
    }
    let message = u32_at(bytes, 4).unwrap_or(u32::MAX);
    let command = match message {
        0 => TrayCommand::Add,
        1 => TrayCommand::Modify,
        2 => TrayCommand::Delete,
        3 => TrayCommand::SetFocus,
        4 => TrayCommand::SetVersion,
        n => return Err(ParseError::BadCommand(n)),
    };
    let cb = u32_at(bytes, OFF_CB_SIZE).unwrap_or(0);
    let readable = (cb as usize).min(bytes.len() - HEADER);
    if readable < NID_V1_SIZE {
        return Err(ParseError::BadSize(cb));
    }
    // Only the readable part of the structure counts.
    let b = &bytes[..HEADER + readable];
    let hwnd = u32_at(b, OFF_HWND).unwrap_or(0);
    let id = u32_at(b, OFF_ID).unwrap_or(0);
    let flags = u32_at(b, OFF_FLAGS).unwrap_or(0);
    let tip_units = if (cb as usize) < NID_V2_SIZE { 64 } else { 128 };
    let tip = if flags & NIF_TIP != 0 { utf16_at(b, OFF_TIP, tip_units) } else { None };
    let guid = (flags & NIF_GUID != 0)
        .then(|| b.get(OFF_GUID..OFF_GUID + 16))
        .flatten()
        .map(|g| u128::from_le_bytes(g.try_into().unwrap_or([0; 16])));
    Ok(NotifyData {
        command,
        hwnd,
        id,
        key: guid.map_or(IconKey::Id { hwnd, id }, IconKey::Guid),
        flags,
        callback_message: u32_at(b, OFF_CALLBACK).unwrap_or(0),
        tip,
        state: u32_at(b, OFF_STATE).unwrap_or(0),
        state_mask: u32_at(b, OFF_STATE_MASK).unwrap_or(0),
        version: u32_at(b, OFF_VERSION).unwrap_or(0),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrayIcon {
    pub key: IconKey,
    pub hwnd: u32,
    /// `uID`: the app may need it back in clicks even for a GUID icon.
    pub uid: u32,
    pub pid: u32,
    pub callback_message: u32,
    pub tip: String,
    pub version: u32,
    pub hidden: bool,
    /// Taken over from an update (`NIM_MODIFY`): its `NIM_SETVERSION` went to an earlier tray, so
    /// the version is unknown.
    pub adopted: bool,
    /// Order of addition.
    pub seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Updated,
    Removed,
    Ignored,
    Focus,
}

#[derive(Default)]
pub struct TrayModel {
    icons: Vec<TrayIcon>,
    next_seq: u64,
}

impl TrayIcon {
    fn update(&mut self, d: &NotifyData) {
        self.hwnd = d.hwnd;
        self.uid = d.id;
        if d.flags & NIF_MESSAGE != 0 {
            self.callback_message = d.callback_message;
        }
        if d.flags & NIF_TIP != 0 {
            self.tip = d.tip.clone().unwrap_or_default();
        }
        if d.flags & NIF_STATE != 0 && d.state_mask & NIS_HIDDEN != 0 {
            self.hidden = d.state & NIS_HIDDEN != 0;
        }
    }
}

impl TrayModel {
    pub fn apply(&mut self, d: &NotifyData, pid: u32) -> Change {
        let pos = self.icons.iter().position(|i| i.key == d.key);
        match (d.command, pos) {
            (TrayCommand::Add | TrayCommand::Modify, Some(p)) => {
                self.icons[p].update(d);
                self.icons[p].pid = pid;
                Change::Updated
            }
            // An update for an icon we never saw: the app registered with an earlier tray (explorer,
            // a NewWin before a restart) and does not register again; take it as an add.
            (TrayCommand::Add | TrayCommand::Modify, None) => {
                let mut icon = TrayIcon {
                    key: d.key,
                    hwnd: d.hwnd,
                    uid: d.id,
                    pid,
                    callback_message: 0,
                    tip: String::new(),
                    version: 0,
                    hidden: false,
                    adopted: d.command == TrayCommand::Modify,
                    seq: self.next_seq,
                };
                icon.update(d);
                self.next_seq += 1;
                self.icons.push(icon);
                Change::Added
            }
            (TrayCommand::Delete, Some(p)) => {
                self.icons.remove(p);
                Change::Removed
            }
            (TrayCommand::SetVersion, Some(p)) => {
                self.icons[p].version = d.version;
                Change::Updated
            }
            (TrayCommand::SetFocus, _) => Change::Focus,
            _ => Change::Ignored,
        }
    }

    /// In order of addition.
    pub fn icons(&self) -> &[TrayIcon] {
        &self.icons
    }

    pub fn get(&self, key: &IconKey) -> Option<&TrayIcon> {
        self.icons.iter().find(|i| i.key == *key)
    }

    /// Drops icons whose window is gone (the app died without `NIM_DELETE`); true if any.
    pub fn prune(&mut self, alive: impl Fn(u32) -> bool) -> bool {
        let before = self.icons.len();
        self.icons.retain(|i| alive(i.hwnd));
        self.icons.len() != before
    }

    pub fn clear(&mut self) {
        self.icons.clear();
    }
}

/// The tray row's text: the tip's first line without a status tail ("Steam - Online" → "Steam").
pub fn row_label(tip: &str, fallback: &str) -> String {
    let line = tip.split(['\r', '\n']).next().unwrap_or("");
    let cut = [" - ", " – ", " — "].iter().filter_map(|s| line.find(s)).min().unwrap_or(line.len());
    let mut label = line[..cut].trim();
    // A trailing counter such as unread messages: "Telegram Desktop (148758)".
    if let Some(open) = label.rfind(" (")
        && let Some(inner) = label[open + 2..].strip_suffix(')')
        && !inner.is_empty()
        && inner.chars().all(|c| c.is_ascii_digit())
    {
        label = label[..open].trim_end();
    }
    if label.is_empty() { fallback.trim().to_string() } else { label.to_string() }
}

/// First case-insensitive match of `query` in `label`, as a UTF-16 (start, len).
pub fn match_range(label: &str, query: &str) -> Option<(usize, usize)> {
    let hay: Vec<char> = label.chars().collect();
    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() {
        return Some((0, 0));
    }
    let same = |a: char, b: char| a.to_lowercase().eq(b.to_lowercase());
    let start = (0..=hay.len().checked_sub(needle.len())?).find(|&i| needle.iter().zip(&hay[i..]).all(|(&n, &h)| same(n, h)))?;
    let units = |cs: &[char]| cs.iter().map(|c| c.len_utf16()).sum::<usize>();
    Some((units(&hay[..start]), units(&hay[start..start + needle.len()])))
}

/// Explorer's own volume and network icons ({7820AE73-…} and {7820AE74-…}, as read from the
/// message): the bar shows both itself, so the tray leaves them out.
pub fn is_bar_duplicate(key: &IconKey) -> bool {
    const TAIL: [u8; 12] = [0xe3, 0x23, 0x29, 0x42, 0x82, 0xc1, 0xe4, 0x1c, 0xb6, 0x7d, 0x5b, 0x9c];
    let IconKey::Guid(g) = key else { return false };
    let b = g.to_le_bytes();
    let data1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    b[4..] == TAIL && matches!(data1, 0x7820_ae73 | 0x7820_ae74)
}

/// A background app whose icon may never reach our tray: it registers once, with explorer,
/// before NewWin starts, and ignores `TaskbarCreated` (the NVIDIA container runs as SYSTEM, so
/// our broadcast is filtered out). Its icon window lives on, so the tray re-creates the icon from
/// what the app sent when it did register with us, and clicks reach the app as usual.
#[derive(Debug, PartialEq, Eq)]
pub struct KnownIcon {
    /// Executable file name.
    pub exe: &'static str,
    /// Class of the (message-only) window that owns the icon.
    pub window_class: &'static str,
    pub uid: u32,
    pub callback_message: u32,
    pub version: u32,
    pub tip: &'static str,
}

/// Recorded from the app's own registration (live, 2026-10-22).
pub const KNOWN_ICONS: [KnownIcon; 1] = [KnownIcon {
    exe: "nvdisplay.container.exe",
    window_class: "NotificationIconWindowClass",
    uid: 1051,
    callback_message: 0xB00,
    version: 3,
    tip: "NVIDIA Settings",
}];

impl KnownIcon {
    /// `NIM_ADD` and `NIM_SETVERSION` as the app sends them, for its icon window `hwnd`.
    pub fn registration(&self, hwnd: u32) -> [NotifyData; 2] {
        let key = IconKey::Id { hwnd, id: self.uid };
        let add = NotifyData {
            command: TrayCommand::Add,
            hwnd,
            id: self.uid,
            key,
            flags: NIF_MESSAGE | NIF_TIP,
            callback_message: self.callback_message,
            tip: Some(self.tip.to_string()),
            state: 0,
            state_mask: 0,
            version: 0,
        };
        let set_version = NotifyData { command: TrayCommand::SetVersion, flags: 0, tip: None, version: self.version, ..add.clone() };
        [add, set_version]
    }
}

/// The known icons to restore: their process runs (`running`: pid, exe file name) and none of
/// its processes has an icon in our tray (`icon_pids`). Each comes with the pids to search for
/// its window.
pub fn missing_known(running: &[(u32, String)], icon_pids: &[u32]) -> Vec<(&'static KnownIcon, Vec<u32>)> {
    KNOWN_ICONS
        .iter()
        .filter_map(|k| {
            let pids: Vec<u32> = running.iter().filter(|(_, exe)| exe.eq_ignore_ascii_case(k.exe)).map(|&(pid, _)| pid).collect();
            (!pids.is_empty() && !pids.iter().any(|p| icon_pids.contains(p))).then_some((k, pids))
        })
        .collect()
}

/// The tray rows as the user arranged them: `hidden` apps removed, `order` first in its order,
/// the rest in their incoming order. Rows are `(exe file name, row)`; an empty exe (process gone)
/// is never ordered or hidden. Exe names compare case-insensitively.
pub fn arrange_rows<T>(rows: Vec<(String, T)>, order: &[String], hidden: &[String]) -> Vec<T> {
    let is = |list: &[String], exe: &str| list.iter().any(|x| x.eq_ignore_ascii_case(exe));
    let (mut ranked, mut rest) = (Vec::new(), Vec::new());
    for (exe, row) in rows {
        if !exe.is_empty() && is(hidden, &exe) {
            continue;
        }
        match (!exe.is_empty()).then(|| order.iter().position(|x| x.eq_ignore_ascii_case(&exe))).flatten() {
            Some(p) => ranked.push((p, row)),
            None => rest.push(row),
        }
    }
    ranked.sort_by_key(|(p, _)| *p);
    ranked.into_iter().map(|(_, r)| r).chain(rest).collect()
}
