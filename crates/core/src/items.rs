//! The right side of the bar (spec: M2 §2): which items, in which order, where.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RightItem {
    /// What is playing; shown only while something is.
    Media,
    Layout,
    Network,
    Volume,
    /// The magnifier: opens Spotlight.
    Search,
    Date,
    Time,
}

impl RightItem {
    fn is_clock(self) -> bool {
        matches!(self, RightItem::Date | RightItem::Time)
    }
}

/// `items.right` from the config; unknown names are skipped, and
/// the first occurrence of a name wins.
pub fn parse_right(names: &[String]) -> Vec<RightItem> {
    let mut out = Vec::new();
    for n in names {
        let item = match n.to_ascii_lowercase().as_str() {
            "media" => RightItem::Media,
            "layout" => RightItem::Layout,
            "network" => RightItem::Network,
            "volume" => RightItem::Volume,
            "search" => RightItem::Search,
            "date" => RightItem::Date,
            "time" => RightItem::Time,
            _ => continue,
        };
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Lays `items` (with widths) out right-to-left ending at `right_edge`, `gap` apart. When status
/// items come before the first clock item, a separator of width `separator` sits between the
/// groups (with a gap on each side); its centre x is returned.
pub fn place_right(items: &[(RightItem, f32)], right_edge: f32, gap: f32, separator: f32) -> (Vec<(RightItem, f32)>, Option<f32>) {
    let first_clock = items.iter().position(|(i, _)| i.is_clock());
    let mut xs = vec![(RightItem::Time, 0.0); items.len()];
    let mut sep = None;
    let mut x = right_edge;
    for i in (0..items.len()).rev() {
        let (item, w) = items[i];
        x -= w;
        xs[i] = (item, x);
        if i == 0 {
            break;
        }
        x -= gap;
        if Some(i) == first_clock {
            x -= separator;
            sep = Some(x + separator / 2.0);
            x -= gap;
        }
    }
    (xs, sep)
}

/// Moves the item at `from` so it ends up at index `to` (both into the current list);
/// an out-of-range index does nothing.
pub fn reorder<T>(v: &mut Vec<T>, from: usize, to: usize) {
    if from >= v.len() || to >= v.len() || from == to {
        return;
    }
    let item = v.remove(from);
    v.insert(to, item);
}

/// The slot a dragged row lands in when its centre is at `y`: the number of rows whose midpoint
/// lies above `y`, clamped to the last row (0 for no rows, a NaN `y` or a negative one).
pub fn drop_index(row_tops: &[f32], row_h: f32, y: f32) -> usize {
    let above = row_tops.iter().filter(|&&t| t + row_h / 2.0 < y).count();
    above.min(row_tops.len().saturating_sub(1))
}
