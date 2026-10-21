//! Fitting app-name labels into the bar's left area.

#[derive(Debug, PartialEq)]
pub struct LabelFit {
    pub widths: Vec<f32>,
    /// Index of the first label that moved into the `»` overflow popover.
    pub overflow_from: Option<usize>,
}

const EPS: f32 = 0.001;

/// Shrinks the longest labels first (down to `min_width`); if even that does not fit,
/// keeps the longest prefix that fits next to an overflow button.
pub fn fit_labels(natural: &[f32], available: f32, min_width: f32, gap: f32, overflow_button: f32) -> LabelFit {
    let gaps = |n: usize| gap * n.saturating_sub(1) as f32;
    let space = available - gaps(natural.len());
    if natural.iter().sum::<f32>() <= space + EPS {
        return LabelFit { widths: natural.to_vec(), overflow_from: None };
    }

    let shrunk: Vec<f32> = natural.iter().map(|&w| w.min(min_width)).collect();
    if shrunk.iter().sum::<f32>() <= space + EPS {
        let cap = water_level(natural, space).max(min_width);
        return LabelFit { widths: natural.iter().map(|&w| w.min(cap)).collect(), overflow_from: None };
    }

    let budget = available - overflow_button - gap;
    let mut used = 0.0;
    let mut k = 0;
    for (i, &w) in shrunk.iter().enumerate() {
        let next = used + w + if i > 0 { gap } else { 0.0 };
        if next > budget + EPS {
            break;
        }
        used = next;
        k = i + 1;
    }
    LabelFit { widths: shrunk[..k].to_vec(), overflow_from: Some(k) }
}

/// Largest cap `c` such that `Σ min(w, c) == space`.
fn water_level(widths: &[f32], space: f32) -> f32 {
    let mut sorted = widths.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut remaining = space;
    for (i, &w) in sorted.iter().enumerate() {
        let share = remaining / (sorted.len() - i) as f32;
        if w > share {
            return share;
        }
        remaining -= w;
    }
    f32::MAX
}
