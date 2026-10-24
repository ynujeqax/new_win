//! Installed font families for the font list (spec: M4 §3.2, §5): DirectWrite's system font
//! collection, each family by its name in the user's locale (English, then the first name, when
//! there is none), sorted case-insensitively without duplicates.

use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteLocalizedStrings};
use windows::core::{BOOL, HSTRING, PCWSTR};

/// `LOCALE_NAME_MAX_LENGTH`.
const LOCALE_MAX: usize = 85;

/// The family names, sorted and deduplicated; empty when DirectWrite fails.
pub fn families() -> Vec<String> {
    match read_families() {
        Ok(names) => sorted_unique(names),
        Err(e) => {
            log::warn!("cannot list the installed fonts: {e}");
            Vec::new()
        }
    }
}

fn user_locale() -> String {
    let mut buf = [0u16; LOCALE_MAX];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    let n = usize::try_from(n).unwrap_or(0).saturating_sub(1).min(LOCALE_MAX);
    String::from_utf16_lossy(buf.get(..n).unwrap_or(&[]))
}

fn read_families() -> windows::core::Result<Vec<String>> {
    let locale = user_locale();
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe { factory.GetSystemFontCollection(&mut collection, false)? };
    let Some(collection) = collection else { return Ok(Vec::new()) };
    let count = unsafe { collection.GetFontFamilyCount() };
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let Ok(family) = (unsafe { collection.GetFontFamily(i) }) else { continue };
        let Ok(names) = (unsafe { family.GetFamilyNames() }) else { continue };
        if let Some(n) = localized(&names, &locale) {
            out.push(n);
        }
    }
    Ok(out)
}

/// The name for `locale`, else `en-us`, else the first one.
fn localized(names: &IDWriteLocalizedStrings, locale: &str) -> Option<String> {
    let find = |l: &str| -> Option<u32> {
        if l.is_empty() {
            return None;
        }
        let (mut index, mut exists) = (0u32, BOOL(0));
        let name = HSTRING::from(l);
        unsafe { names.FindLocaleName(PCWSTR(name.as_ptr()), &mut index, &mut exists).ok()? };
        exists.as_bool().then_some(index)
    };
    let count = unsafe { names.GetCount() };
    let index = find(locale).or_else(|| find("en-us")).or((count > 0).then_some(0))?;
    let len = unsafe { names.GetStringLength(index).ok()? } as usize;
    let mut buf = vec![0u16; len + 1];
    unsafe { names.GetString(index, &mut buf).ok()? };
    Some(String::from_utf16_lossy(buf.get(..len).unwrap_or(&[])))
}

/// Sorted case-insensitively, empty names and case-insensitive duplicates dropped.
pub fn sorted_unique(names: Vec<String>) -> Vec<String> {
    let mut v: Vec<(String, String)> = names.into_iter().map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).map(|n| (n.to_lowercase(), n)).collect();
    v.sort();
    v.dedup_by(|a, b| a.0 == b.0);
    v.into_iter().map(|(_, n)| n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_include_segoe_ui_and_are_sorted() {
        let f = families();
        assert!(f.iter().any(|n| n == "Segoe UI"), "{} families", f.len());
        assert!(f.windows(2).all(|w| w[0].to_lowercase() <= w[1].to_lowercase()));
    }

    #[test]
    fn names_sort_without_case_and_lose_duplicates() {
        let got = sorted_unique(vec!["b".into(), "A".into(), "".into(), "a".into(), "C".into(), " ".into()]);
        assert_eq!(got, ["A", "b", "C"]);
    }
}
