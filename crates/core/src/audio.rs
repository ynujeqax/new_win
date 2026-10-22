//! Volume on the bar (spec: M2 §2, §3.2): levels, wheel steps, the speaker icon.

/// One wheel notch.
const WHEEL_DELTA: i32 = 120;

/// Sound waves next to the speaker: none when silent, up to 3 near full volume.
pub fn waves_for(level: u8) -> u8 {
    match level {
        0 => 0,
        1..=33 => 1,
        34..=66 => 2,
        _ => 3,
    }
}

/// Endpoint volume scalar (0..=1) as a percentage.
pub fn percent(scalar: f32) -> u8 {
    if scalar.is_nan() { 0 } else { (scalar * 100.0).round().clamp(0.0, 100.0) as u8 }
}

/// Wheel input to whole notches; touchpads send small deltas, the remainder is kept.
#[derive(Default)]
pub struct Wheel {
    rest: i32,
}

impl Wheel {
    pub fn notches(&mut self, delta: i16) -> i32 {
        self.rest += i32::from(delta);
        let n = self.rest / WHEEL_DELTA;
        self.rest %= WHEEL_DELTA;
        n
    }
}

/// `level` moved by `notches` × `per_notch`, kept within 0..=100.
pub fn step(level: u8, notches: i32, per_notch: u8) -> u8 {
    (i32::from(level) + notches * i32::from(per_notch)).clamp(0, 100) as u8
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpeakerIcon {
    /// No output device (or the audio service is down).
    Unavailable,
    Level { waves: u8, muted: bool },
}

/// `state` = (level, muted) of the default device, if there is one.
pub fn speaker_icon(state: Option<(u8, bool)>) -> SpeakerIcon {
    match state {
        None => SpeakerIcon::Unavailable,
        Some((level, muted)) => SpeakerIcon::Level { waves: waves_for(level), muted },
    }
}
