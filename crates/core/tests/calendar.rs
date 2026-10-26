use newwin_core::calendar::{Cell, Month, grid, is_leap, weekday};

#[test]
fn weekdays() {
    assert_eq!(weekday(2026, 10, 26), 0, "Monday 26 October 2026");
    assert_eq!(weekday(2000, 1, 1), 5, "a Saturday");
    assert_eq!(weekday(2024, 2, 29), 3, "a Thursday");
}

#[test]
fn months_roll_over_years() {
    let dec = Month { year: 2026, month: 12 };
    assert_eq!(dec.next(), Month { year: 2027, month: 1 });
    assert_eq!(Month { year: 2027, month: 1 }.prev(), dec);
    assert_eq!(Month { year: 2024, month: 2 }.days(), 29);
    assert_eq!(Month { year: 2026, month: 2 }.days(), 28);
    assert!(is_leap(2000) && !is_leap(1900));
}

#[test]
fn a_monday_first_grid() {
    // October 2026 starts on a Thursday: Mon–Wed are 28–30 September.
    let g = grid(Month { year: 2026, month: 10 }, 0);
    assert_eq!(&g[..4], &[Cell { day: 28, this_month: false }, Cell { day: 29, this_month: false }, Cell { day: 30, this_month: false }, Cell { day: 1, this_month: true }]);
    assert_eq!(g[3 + 30], Cell { day: 31, this_month: true });
    assert_eq!(g[3 + 31], Cell { day: 1, this_month: false }, "then November");
    assert_eq!(g.iter().filter(|c| c.this_month).count(), 31);
}

#[test]
fn a_sunday_first_grid() {
    let g = grid(Month { year: 2026, month: 10 }, 6);
    assert_eq!(g[4], Cell { day: 1, this_month: true }, "Sun Mon Tue Wed, then Thursday the 1st");
    // A month starting on the first weekday has no leading days.
    let g = grid(Month { year: 2026, month: 6 }, 0);
    assert_eq!(g[0], Cell { day: 1, this_month: true }, "1 June 2026 is a Monday");
}
