//! The right side of the bar (spec: M2 §2): which items, in which order, where.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RightItem {
    Layout,
    Network,
    Volume,
    Date,
    Time,
}

impl RightItem {
    fn is_clock(self) -> bool {
        matches!(self, RightItem::Date | RightItem::Time)
    }
}

/// `items.right` from the config; `search` (Spotlight, M3) and unknown names are skipped, and
/// the first occurrence of a name wins.
pub fn parse_right(names: &[String]) -> Vec<RightItem> {
    let mut out = Vec::new();
    for n in names {
        let item = match n.to_ascii_lowercase().as_str() {
            "layout" => RightItem::Layout,
            "network" => RightItem::Network,
            "volume" => RightItem::Volume,
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
