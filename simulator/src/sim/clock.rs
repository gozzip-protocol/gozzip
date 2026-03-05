use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::types::SimTime;

// ── SimClock ─────────────────────────────────────────────────────────

/// A virtual clock for the simulation.
///
/// In deterministic mode, time is controlled explicitly via `advance_to`
/// and `advance_by`. In wall-clock mode, `now()` returns elapsed real time
/// since creation.
#[derive(Clone)]
pub struct SimClock {
    current_micros: Arc<AtomicU64>,
    deterministic: bool,
    start_wall: std::time::Instant,
}

impl SimClock {
    /// Create a new clock starting at time 0.
    ///
    /// If `deterministic` is true, time only advances via explicit calls.
    /// If false, `now()` returns wall-clock elapsed time.
    pub fn new(deterministic: bool) -> Self {
        Self {
            current_micros: Arc::new(AtomicU64::new(0)),
            deterministic,
            start_wall: std::time::Instant::now(),
        }
    }

    /// Return the current simulation time in seconds.
    pub fn now(&self) -> SimTime {
        if self.deterministic {
            let micros = self.current_micros.load(Ordering::Relaxed);
            micros as f64 / 1_000_000.0
        } else {
            self.start_wall.elapsed().as_secs_f64()
        }
    }

    /// Set the clock to an absolute time (deterministic mode).
    pub fn advance_to(&self, time: SimTime) {
        let micros = (time * 1_000_000.0) as u64;
        self.current_micros.store(micros, Ordering::Relaxed);
    }

    /// Increment the clock by a delta (deterministic mode).
    pub fn advance_by(&self, delta: SimTime) {
        let delta_micros = (delta * 1_000_000.0) as u64;
        self.current_micros.fetch_add(delta_micros, Ordering::Relaxed);
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_clock() {
        let clock = SimClock::new(true);
        assert!((clock.now() - 0.0).abs() < f64::EPSILON);

        clock.advance_to(100.5);
        assert!((clock.now() - 100.5).abs() < 1e-6);
    }

    #[test]
    fn test_advance_by() {
        let clock = SimClock::new(true);
        clock.advance_by(10.0);
        clock.advance_by(5.0);
        assert!((clock.now() - 15.0).abs() < 1e-6);
    }
}
