//! Clock timer math.

/// Delay until just after the next minute boundary (+50 ms so the formatted minute has rolled over).
pub fn ms_until_next_minute(second: u16, millis: u16) -> u32 {
    let elapsed = u32::from(second.min(59)) * 1000 + u32::from(millis.min(999));
    60_000 - elapsed + 50
}
