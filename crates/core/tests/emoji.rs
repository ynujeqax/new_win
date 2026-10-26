use newwin_core::emoji::{all, search, usage_key};

fn none(_: &str) -> u32 {
    0
}

fn glyphs(q: &str, max: usize) -> Vec<&'static str> {
    search(q, max, &none).iter().map(|e| e.glyph).collect()
}

#[test]
fn smile_finds_smiling_faces_first() {
    let found = search("smile", 8, &none);
    assert_eq!(found.len(), 8);
    assert!(found[0].en.contains("face") && (found[0].en.contains("smil") || found[0].en.contains("grin")), "{}", found[0].en);
    let mut distinct: Vec<_> = found.iter().map(|e| e.glyph).collect();
    distinct.dedup();
    assert_eq!(distinct.len(), 8, "no emoji twice");
    assert!(found.iter().all(|e| e.en.contains("fac") || e.en.contains("smil") || e.en.contains("grin")), "{:?}", found.iter().map(|e| e.en).collect::<Vec<_>>());
}

#[test]
fn ukrainian_heart() {
    assert!(glyphs("серце", 5).contains(&"❤️"), "{:?}", glyphs("серце", 5));
}

#[test]
fn name_prefix_beats_keyword() {
    let first = search("cat", 5, &none)[0];
    assert!(first.en.starts_with("cat"), "{}", first.en);
}

#[test]
fn launches_lift_equal_matches() {
    // "heart" is a whole word of many names: the one copied before leads them.
    let lots = |k: &str| if k == "emoji:💜" { 5 } else { 0 };
    let found: Vec<_> = search("heart", 3, &lots).iter().map(|e| e.glyph).collect();
    assert_eq!(found[0], "💜", "{found:?}");
    assert_eq!(search("heart", 3, &none)[0].glyph, "❤️", "popular first without launches");
}

#[test]
fn empty_query_gives_nothing() {
    assert!(search("", 8, &none).is_empty());
    assert!(search("   ", 8, &none).is_empty());
}

#[test]
fn no_skin_tones() {
    let tones = '\u{1F3FB}'..='\u{1F3FF}';
    assert!(all().iter().all(|e| !e.glyph.chars().any(|c| tones.contains(&c))));
}

#[test]
fn every_emoji_has_both_names() {
    assert!(all().len() > 1800, "{}", all().len());
    assert!(all().iter().all(|e| !e.en.is_empty() && !e.uk.is_empty()));
    let ua = all().iter().find(|e| e.glyph == "🇺🇦").expect("the Ukrainian flag");
    assert_eq!(ua.uk, "прапор: Україна");
}

#[test]
fn query_is_case_insensitive_and_multiword() {
    assert_eq!(glyphs("Red Heart", 1), ["❤️"]);
    assert!(glyphs("thumbs up", 3).contains(&"👍"));
}

#[test]
fn usage_key_is_prefixed() {
    let e = all().iter().find(|e| e.glyph == "😀").unwrap();
    assert_eq!(usage_key(e), "emoji:😀");
}

#[test]
fn fast_enough() {
    let queries = ["s", "sm", "smi", "smil", "smile", "у", "ус", "усм", "heart", "серце"];
    let start = std::time::Instant::now();
    for _ in 0..20 {
        for q in queries {
            std::hint::black_box(search(q, 8, &none));
        }
    }
    let ms = start.elapsed().as_millis();
    let budget = if cfg!(debug_assertions) { 2000 } else { 200 };
    assert!(ms < budget, "200 searches took {ms} ms");
}
