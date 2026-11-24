//! Rate measurement (frames or updates per second).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Length of the measurement window.
const WINDOW: Duration = Duration::from_secs(1);

/// Measures how often something happens, over the last second.
#[derive(Debug, Clone, Default)]
pub(crate) struct RateMeter {
    ticks: VecDeque<Instant>,
}

impl RateMeter {
    /// Records one occurrence at `now`.
    pub(crate) fn record(&mut self, now: Instant) {
        self.ticks.push_back(now);
        while let Some(&oldest) = self.ticks.front() {
            if now.saturating_duration_since(oldest) <= WINDOW {
                break;
            }
            self.ticks.pop_front();
        }
    }

    /// Occurrences per second, or 0 when fewer than two were recorded.
    pub(crate) fn rate(&self) -> f64 {
        let (Some(first), Some(last)) = (self.ticks.front(), self.ticks.back()) else {
            return 0.0;
        };
        let span = last.saturating_duration_since(*first).as_secs_f64();
        if span <= 0.0 {
            return 0.0;
        }
        // `ticks.len()` is small (bounded by the rate limits), so the conversion is exact.
        let intervals = u32::try_from(self.ticks.len() - 1).unwrap_or(u32::MAX);
        f64::from(intervals) / span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_a_steady_rate() {
        let start = Instant::now();
        let mut meter = RateMeter::default();
        assert!(meter.rate().abs() < f64::EPSILON);
        for i in 0..=120 {
            meter.record(start + Duration::from_millis(i * 20));
        }
        assert!((meter.rate() - 50.0).abs() < 0.01, "{}", meter.rate());
    }

    #[test]
    fn forgets_old_ticks() {
        let start = Instant::now();
        let mut meter = RateMeter::default();
        meter.record(start);
        meter.record(start + Duration::from_secs(5));
        assert!(meter.rate().abs() < f64::EPSILON);
    }
}
