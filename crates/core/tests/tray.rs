use newwin_core::tray::*;

/// A TRAYNOTIFYDATA buffer as shell32 sends it.
#[allow(clippy::too_many_arguments)]
fn nid(msg: u32, cb: u32, hwnd: u32, id: u32, flags: u32, cbmsg: u32, tip: &str, state: (u32, u32), version: u32, guid: Option<u128>) -> Vec<u8> {
    let mut b = vec![0u8; 8 + NID_FULL_SIZE];
    let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut b, 0, TRAY_SIGNATURE);
    put(&mut b, 4, msg);
    put(&mut b, 8, cb);
    put(&mut b, 12, hwnd);
    put(&mut b, 16, id);
    put(&mut b, 20, flags);
    put(&mut b, 24, cbmsg);
    for (i, u) in tip.encode_utf16().enumerate() {
        b[32 + 2 * i..34 + 2 * i].copy_from_slice(&u.to_le_bytes());
    }
    put(&mut b, 288, state.0);
    put(&mut b, 292, state.1);
    put(&mut b, 808, version);
    if let Some(g) = guid {
        b[944..960].copy_from_slice(&g.to_le_bytes());
    }
    b
}

const FULL: u32 = NID_FULL_SIZE as u32;

#[test]
fn parses_add_with_tip_and_message() {
    let d = parse_notify(&nid(0, FULL, 0x1234, 7, NIF_MESSAGE | NIF_TIP, 0x8001, "Telegram", (0, 0), 0, None)).unwrap();
    assert_eq!(d.command, TrayCommand::Add);
    assert_eq!(d.key, IconKey::Id { hwnd: 0x1234, id: 7 });
    assert_eq!(d.callback_message, 0x8001);
    assert_eq!(d.tip.as_deref(), Some("Telegram"));
}

#[test]
fn guid_icons_are_keyed_by_guid() {
    let g = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeffu128;
    let d = parse_notify(&nid(1, FULL, 5, 1, NIF_GUID, 0, "", (0, 0), 0, Some(g))).unwrap();
    assert_eq!(d.key, IconKey::Guid(g));
}

#[test]
fn v1_size_reads_a_64_unit_tip_and_no_guid() {
    let long: String = "x".repeat(100);
    let mut b = nid(0, NID_V1_SIZE as u32, 5, 1, NIF_TIP | NIF_GUID, 0, &long, (0, 0), 0, Some(9));
    b.truncate(8 + NID_V1_SIZE);
    let d = parse_notify(&b).unwrap();
    assert_eq!(d.tip.as_deref().map(str::len), Some(64));
    assert_eq!(d.key, IconKey::Id { hwnd: 5, id: 1 });
}

#[test]
fn tip_without_nul_stops_at_field_end() {
    let d = parse_notify(&nid(0, FULL, 5, 1, NIF_TIP, 0, &"y".repeat(128), (0, 0), 0, None)).unwrap();
    assert_eq!(d.tip.unwrap().len(), 128);
}

#[test]
fn set_version_reads_uversion() {
    let d = parse_notify(&nid(4, FULL, 5, 1, 0, 0, "", (0, 0), 4, None)).unwrap();
    assert_eq!((d.command, d.version), (TrayCommand::SetVersion, 4));
}

#[test]
fn rejects_short_bad_signature_bad_command_and_lying_size() {
    assert_eq!(parse_notify(&[]), Err(ParseError::TooShort));
    let mut b = nid(0, FULL, 5, 1, 0, 0, "", (0, 0), 0, None);
    b[0] = 0;
    assert_eq!(parse_notify(&b), Err(ParseError::BadSignature));
    assert_eq!(parse_notify(&nid(9, FULL, 5, 1, 0, 0, "", (0, 0), 0, None)), Err(ParseError::BadCommand(9)));
    assert_eq!(parse_notify(&nid(0, 40, 5, 1, 0, 0, "", (0, 0), 0, None)), Err(ParseError::BadSize(40)));
    // cbSize claims more than was sent: fields past the end are simply absent.
    let mut cut = nid(0, FULL, 5, 1, NIF_GUID, 0, "", (0, 0), 0, Some(3));
    cut.truncate(8 + 600);
    assert_eq!(parse_notify(&cut).unwrap().key, IconKey::Id { hwnd: 5, id: 1 });
}

fn add(m: &mut TrayModel, hwnd: u32, id: u32, tip: &str) -> Change {
    m.apply(&parse_notify(&nid(0, FULL, hwnd, id, NIF_TIP | NIF_MESSAGE, 0x400, tip, (0, 0), 0, None)).unwrap(), 42)
}

#[test]
fn model_add_modify_delete_in_order() {
    let mut m = TrayModel::default();
    assert_eq!(add(&mut m, 1, 1, "A"), Change::Added);
    assert_eq!(add(&mut m, 2, 1, "B"), Change::Added);
    assert_eq!(add(&mut m, 1, 1, "A2"), Change::Updated);
    assert_eq!(m.icons().iter().map(|i| i.tip.as_str()).collect::<Vec<_>>(), ["A2", "B"]);
    assert_eq!(m.icons()[0].pid, 42);
    let set_version_unknown = parse_notify(&nid(4, FULL, 9, 9, 0, 0, "", (0, 0), 4, None)).unwrap();
    assert_eq!(m.apply(&set_version_unknown, 1), Change::Ignored);
    let del = parse_notify(&nid(2, FULL, 1, 1, 0, 0, "", (0, 0), 0, None)).unwrap();
    assert_eq!(m.apply(&del, 1), Change::Removed);
    assert_eq!(m.apply(&del, 1), Change::Ignored);
    assert_eq!(m.icons().len(), 1);
}

/// Apps that registered with an earlier tray (Telegram after NewWin restarts) only send updates.
#[test]
fn modify_of_an_unknown_icon_adds_it() {
    let mut m = TrayModel::default();
    let modify = parse_notify(&nid(1, FULL, 9, 3, NIF_TIP | NIF_MESSAGE, 0x8001, "Telegram Desktop (148758)", (0, 0), 0, None)).unwrap();
    assert_eq!(m.apply(&modify, 7), Change::Added);
    assert_eq!((m.icons()[0].callback_message, m.icons()[0].pid), (0x8001, 7));
    assert_eq!(m.apply(&modify, 7), Change::Updated);
}

#[test]
fn hidden_state_follows_mask() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "A");
    let hide = parse_notify(&nid(1, FULL, 1, 1, NIF_STATE, 0, "", (NIS_HIDDEN, NIS_HIDDEN), 0, None)).unwrap();
    m.apply(&hide, 42);
    assert!(m.icons()[0].hidden);
    let other_bit = parse_notify(&nid(1, FULL, 1, 1, NIF_STATE, 0, "", (0, 0x2), 0, None)).unwrap();
    m.apply(&other_bit, 42);
    assert!(m.icons()[0].hidden, "bits outside the mask are untouched");
}

#[test]
fn modify_without_tip_flag_keeps_tip() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "Keep");
    m.apply(&parse_notify(&nid(1, FULL, 1, 1, 0, 0, "Drop", (0, 0), 0, None)).unwrap(), 42);
    assert_eq!(m.icons()[0].tip, "Keep");
}

#[test]
fn prune_drops_dead_windows() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "A");
    add(&mut m, 2, 1, "B");
    assert!(m.prune(|h| h == 2));
    assert_eq!(m.icons().len(), 1);
    assert!(!m.prune(|_| true));
}

#[test]
fn labels() {
    assert_eq!(row_label("Steam - Online", "steam"), "Steam");
    assert_eq!(row_label("Telegram\n3 unread", ""), "Telegram");
    assert_eq!(row_label("NVIDIA Settings – Ready", ""), "NVIDIA Settings");
    assert_eq!(row_label("  ", " Discord "), "Discord");
    assert_eq!(row_label("", ""), "");
    assert_eq!(row_label("Telegram Desktop (148758)", ""), "Telegram Desktop", "unread counter");
    assert_eq!(row_label("Notes (beta)", ""), "Notes (beta)", "words in brackets stay");
}

#[test]
fn match_ranges_are_utf16_and_case_insensitive() {
    assert_eq!(match_range("Telegram", "GRAM"), Some((4, 4)));
    assert_eq!(match_range("Телеграм", "теле"), Some((0, 4)));
    assert_eq!(match_range("Steam", ""), Some((0, 0)));
    assert_eq!(match_range("Steam", "x"), None);
    assert_eq!(match_range("😀 Discord", "dis"), Some((3, 3)));
}

/// Explorer registers its own volume and network icons; the bar already shows both.
#[test]
fn explorer_volume_and_network_icons_are_bar_duplicates() {
    let volume: u128 = "207834616086241708723547713941425663603".parse().unwrap();
    let safely_remove: u128 = "207834616086241708723547713941425663608".parse().unwrap();
    assert!(is_bar_duplicate(&IconKey::Guid(volume)));
    assert!(is_bar_duplicate(&IconKey::Guid(volume + 1)), "network is 7820AE74");
    assert!(!is_bar_duplicate(&IconKey::Guid(safely_remove)));
    assert!(!is_bar_duplicate(&IconKey::Id { hwnd: 1, id: 1 }));
}

#[test]
fn icons_taken_from_an_update_are_marked_adopted() {
    let mut m = TrayModel::default();
    m.apply(&parse_notify(&nid(1, FULL, 9, 3, NIF_MESSAGE, 0x8001, "", (0, 0), 0, None)).unwrap(), 1);
    add(&mut m, 2, 1, "B");
    assert_eq!(m.icons().iter().map(|i| i.adopted).collect::<Vec<_>>(), [true, false]);
}
