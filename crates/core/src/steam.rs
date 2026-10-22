//! Steam games: the name the store shows, read from the library's `appmanifest_*.acf`. Game exes
//! carry only a file name ("cs2") and Steam adds no Start menu entry.

const COMMON: &str = r"\steamapps\common\";

/// `…\steamapps\common\<installdir>\…\game.exe` → (`…\steamapps`, `<installdir>`).
pub fn game_dir(exe: &str) -> Option<(&str, &str)> {
    // ASCII lowercasing keeps byte offsets, so they index `exe` too.
    let at = exe.to_ascii_lowercase().find(COMMON)?;
    let rest = &exe[at + COMMON.len()..];
    let (dir, _) = rest.split_once('\\').filter(|(d, _)| !d.is_empty())?;
    Some((&exe[..at + r"\steamapps".len()], dir))
}

/// The manifest's top-level `"key" "value"` pairs (nested blocks are skipped).
fn top_level_pairs(acf: &str) -> Vec<(String, String)> {
    let mut depth = 0;
    let mut out = Vec::new();
    for line in acf.lines() {
        let line = line.trim();
        match line {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ if depth == 1 => {
                let quoted = quoted_strings(line);
                if let [k, v] = quoted.as_slice() {
                    out.push((k.clone(), v.clone()));
                }
            }
            _ => {}
        }
    }
    out
}

fn quoted_strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => s.extend(chars.next()),
                c => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

/// The store name in `acf` when it installs into `installdir` (case-insensitive).
pub fn manifest_name(acf: &str, installdir: &str) -> Option<String> {
    let pairs = top_level_pairs(acf);
    let get = |key: &str| pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim());
    get("installdir").filter(|d| d.eq_ignore_ascii_case(installdir))?;
    get("name").filter(|n| !n.is_empty()).map(str::to_string)
}
