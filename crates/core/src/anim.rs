//! Tweens for drawn animations (a gliding pill, icon strokes): the UI asks for the value at a
//! moment and repaints while any tween is running, so idle costs nothing.

/// Ease-out cubic from `from` to `to`, the same curve as the DWM animations in `newwin-ui`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start_ms: u64,
    dur_ms: u32,
}

impl Tween {
    pub fn at_rest(v: f32) -> Tween {
        Tween { from: v, to: v, start_ms: 0, dur_ms: 0 }
    }

    /// Heads for `target` from wherever it is at `now_ms`; the same target changes nothing.
    pub fn to(&mut self, target: f32, now_ms: u64, dur_ms: u32) {
        if target == self.to {
            return;
        }
        *self = Tween { from: self.value(now_ms), to: target, start_ms: now_ms, dur_ms };
    }

    pub fn value(&self, now_ms: u64) -> f32 {
        if self.dur_ms == 0 {
            return self.to;
        }
        let t = (now_ms.saturating_sub(self.start_ms) as f32 / self.dur_ms as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * (1.0 - (1.0 - t).powi(3))
    }

    pub fn running(&self, now_ms: u64) -> bool {
        self.dur_ms != 0 && now_ms < self.start_ms + u64::from(self.dur_ms)
    }

    pub fn target(&self) -> f32 {
        self.to
    }
}
