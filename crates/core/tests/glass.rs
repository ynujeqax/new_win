use newwin_core::glass::*;

fn solid(w: usize, h: usize, bgr: [u8; 3]) -> Vec<u8> {
    (0..w * h).flat_map(|_| [bgr[0], bgr[1], bgr[2], 255]).collect()
}

#[test]
fn capture_area_grows_and_clips() {
    let out = PxRect { left: 0, top: 0, right: 2560, bottom: 1440 };
    let bar = PxRect { left: 6, top: 6, right: 2554, bottom: 36 };
    assert_eq!(capture_area(bar, out), Some(PxRect { left: 0, top: 0, right: 2560, bottom: 60 }));
    let mid = PxRect { left: 200, top: 200, right: 1400, bottom: 234 };
    assert_eq!(capture_area(mid, out), Some(PxRect { left: 176, top: 176, right: 1424, bottom: 258 }));
    let off = PxRect { left: 3000, top: 10, right: 3100, bottom: 40 };
    assert_eq!(capture_area(off, out), None);
}

#[test]
fn solid_color_stays_solid() {
    let img = blur_strip(&solid(64, 16, [10, 120, 250]), 64 * 4, 64, 16);
    assert_eq!((img.width, img.height), (16, 4));
    assert!(img.bgra.chunks(4).all(|p| p == [10, 120, 250, 255]));
}

#[test]
fn pitch_and_remainder_are_respected() {
    // 66x18 with a 300-byte pitch: 2 extra columns/rows are dropped, padding bytes ignored.
    let mut src = vec![0u8; 300 * 18];
    for y in 0..18 {
        for x in 0..66 {
            src[y * 300 + x * 4..][..4].copy_from_slice(&[50, 50, 50, 255]);
        }
    }
    let img = blur_strip(&src, 300, 66, 18);
    assert_eq!((img.width, img.height), (16, 4));
    assert!(img.bgra.chunks(4).all(|p| p == [50, 50, 50, 255]));
}

#[test]
fn a_bright_spot_spreads_symmetrically_and_edges_do_not_darken() {
    let (w, h) = (64, 64);
    let mut src = solid(w, h, [0, 0, 0]);
    for y in 28..36 {
        for x in 28..36 {
            src[(y * w + x) * 4..][..3].copy_from_slice(&[255, 255, 255]);
        }
    }
    let img = blur_strip(&src, w * 4, w, h);
    let at = |x: usize, y: usize| img.bgra[(y * img.width + x) * 4];
    assert!(at(7, 7) > at(5, 7) && at(5, 7) > 0);
    // The spot covers small pixels 7 and 8, so 5 and 10 are equally far from it.
    assert_eq!(at(5, 7), at(10, 7));
    assert_eq!(at(7, 5), at(7, 10));
    let edge = blur_strip(&solid(w, h, [200, 200, 200]), w * 4, w, h);
    assert_eq!(edge.bgra[0], 200);
}

#[test]
fn hits_only_when_a_changed_rect_overlaps() {
    let area = PxRect { left: 100, top: 0, right: 200, bottom: 50 };
    assert!(!any_hit(&[], &area));
    assert!(!any_hit(&[PxRect { left: 0, top: 0, right: 100, bottom: 50 }], &area));
    assert!(any_hit(&[PxRect { left: 199, top: 49, right: 300, bottom: 60 }], &area));
}

#[test]
fn change_filter_passes_first_and_changed_images_only() {
    let mut f = ChangeFilter::default();
    let a = Image { width: 1, height: 1, bgra: vec![1, 2, 3, 255] };
    assert!(f.changed(&a));
    assert!(!f.changed(&a));
    assert!(f.changed(&Image { width: 1, height: 1, bgra: vec![1, 2, 4, 255] }));
    assert!(f.changed(&Image { width: 2, height: 1, bgra: vec![1, 2, 4, 255, 1, 2, 4, 255] }));
}

fn img(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 3]) -> Image {
    let mut bgra = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let p = f(x, y);
            bgra.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
    }
    Image { width: w, height: h, bgra }
}

fn px_at(i: &Image, x: usize, y: usize) -> [u8; 3] {
    let o = (y * i.width + x) * 4;
    [i.bgra[o], i.bgra[o + 1], i.bgra[o + 2]]
}

#[test]
fn saturate_spreads_colors_and_keeps_grays() {
    let mut gray = img(2, 1, |_, _| [90, 90, 90]);
    saturate(&mut gray, SATURATION);
    assert_eq!(px_at(&gray, 0, 0), [90, 90, 90]);
    let mut c = img(1, 1, |_, _| [200, 150, 100]);
    saturate(&mut c, 1.0);
    assert_eq!(px_at(&c, 0, 0), [200, 150, 100]);
    saturate(&mut c, SATURATION);
    let [b, g, r] = px_at(&c, 0, 0);
    assert!(b > 200 && r < 100, "{b} {g} {r}");
    let mut hot = img(1, 1, |_, _| [255, 10, 10]);
    saturate(&mut hot, SATURATION);
    assert_eq!(px_at(&hot, 0, 0)[0], 255);
}

// A 400x80 capture area (100x20 small pixels) around a 352x32 capsule.
const AREA: PxRect = PxRect { left: 0, top: 0, right: 400, bottom: 80 };
const WIN: PxRect = PxRect { left: 24, top: 24, right: 376, bottom: 56 };

#[test]
fn refraction_leaves_flat_images_alone() {
    let mut flat = img(100, 20, |_, _| [70, 80, 90]);
    refract(&mut flat, AREA, WIN, 16.0);
    assert!(flat.bgra.chunks(4).all(|p| p == [70, 80, 90, 255]));
}

#[test]
fn edges_bend_in_what_is_outside_and_the_middle_stays_clear() {
    // Blue grows to the right: sampling further out shows up as darker on the left edge and
    // brighter on the right edge.
    let orig = img(100, 20, |x, _| [(x * 2) as u8, 0, 0]);
    let mut lens = orig.clone();
    refract(&mut lens, AREA, WIN, 16.0);
    assert_eq!(px_at(&lens, 50, 10), px_at(&orig, 50, 10));
    assert!(px_at(&lens, 7, 10)[0] < px_at(&orig, 7, 10)[0]);
    assert!(px_at(&lens, 92, 10)[0] > px_at(&orig, 92, 10)[0]);
    // Outside the shape nothing changes (it is clipped away anyway).
    assert_eq!(px_at(&lens, 2, 2), px_at(&orig, 2, 2));
}
