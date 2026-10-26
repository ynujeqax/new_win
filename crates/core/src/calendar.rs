//! The calendar panel's month grid (1.0.5): which day goes in which cell.

/// A year and month (1..=12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Month {
    pub year: i32,
    pub month: u32,
}

impl Month {
    pub fn next(self) -> Month {
        if self.month == 12 { Month { year: self.year + 1, month: 1 } } else { Month { month: self.month + 1, ..self } }
    }

    pub fn prev(self) -> Month {
        if self.month == 1 { Month { year: self.year - 1, month: 12 } } else { Month { month: self.month - 1, ..self } }
    }

    pub fn days(self) -> u32 {
        match self.month {
            4 | 6 | 9 | 11 => 30,
            2 if is_leap(self.year) => 29,
            2 => 28,
            _ => 31,
        }
    }
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Day of the week, 0 = Monday … 6 = Sunday (Sakamoto's method).
pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let sunday_first = (y + y / 4 - y / 100 + y / 400 + T[month as usize - 1] + day as i32).rem_euclid(7);
    ((sunday_first + 6) % 7) as u32
}

/// One cell of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub day: u32,
    /// Belongs to the month shown (else the tail of the previous or the head of the next).
    pub this_month: bool,
}

/// Six weeks of seven days starting on `first_weekday` (0 = Monday … 6 = Sunday): the month
/// with the days around it, so the grid never changes height.
pub fn grid(m: Month, first_weekday: u32) -> [Cell; 42] {
    let lead = (weekday(m.year, m.month, 1) + 7 - first_weekday % 7) % 7;
    let prev_days = m.prev().days();
    let mut cells = [Cell { day: 0, this_month: false }; 42];
    for (i, c) in cells.iter_mut().enumerate() {
        let i = i as u32;
        *c = if i < lead {
            Cell { day: prev_days - lead + 1 + i, this_month: false }
        } else if i - lead < m.days() {
            Cell { day: i - lead + 1, this_month: true }
        } else {
            Cell { day: i - lead - m.days() + 1, this_month: false }
        };
    }
    cells
}
