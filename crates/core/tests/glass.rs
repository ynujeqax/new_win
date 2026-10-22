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
