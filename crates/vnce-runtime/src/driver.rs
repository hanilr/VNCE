//! Drivers: where time and input events come from.
//!
//! [`TerminalDriver`] uses the system clock and the real terminal. [`ScriptedDriver`]
//! simulates both, so a [`Game`](crate::Game) can be tested deterministically with
//! [`run_with`](crate::run_with).

use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

use vnce_term::Event;

/// A source of time and input events for the game loop.
pub trait Driver {
    /// The current time.
    fn now(&mut self) -> Instant;

    /// Waits up to `timeout` for an input event. Returns `Ok(None)` if none arrived.
    ///
    /// # Errors
    ///
    /// Returns an error if reading input fails.
    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<Event>>;
}

/// A mutable reference to a driver is a driver too, so it can be inspected after
/// [`run_with`](crate::run_with) returns.
impl<D: Driver + ?Sized> Driver for &mut D {
    fn now(&mut self) -> Instant {
        (**self).now()
    }

    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        (**self).poll_event(timeout)
    }
}

/// The real driver: system clock and terminal input.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalDriver;

impl Driver for TerminalDriver {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        vnce_term::input::poll(timeout)
    }
}

/// A simulated driver for tests.
///
/// Time only moves when the loop waits for events (and during scripted stalls), so runs are
/// fully deterministic.
///
/// ```
/// use std::time::Duration;
/// use vnce_runtime::ScriptedDriver;
/// use vnce_term::Event;
///
/// let mut driver = ScriptedDriver::new();
/// driver.push_event(Duration::from_millis(100), Event::FocusLost);
/// ```
#[derive(Debug, Clone)]
pub struct ScriptedDriver {
    start: Instant,
    now: Instant,
    /// Events sorted by time since `start`.
    events: VecDeque<(Duration, Event)>,
    /// Stalls (time since `start`, length) sorted by time.
    stalls: VecDeque<(Duration, Duration)>,
    min_wait: Duration,
}

impl ScriptedDriver {
    /// Creates a driver whose clock starts now, with no events.
    #[must_use]
    pub fn new() -> Self {
        let start = Instant::now();
        Self {
            start,
            now: start,
            events: VecDeque::new(),
            stalls: VecDeque::new(),
            min_wait: Duration::ZERO,
        }
    }

    /// Schedules `event` to arrive `at` after the start.
    pub fn push_event(&mut self, at: Duration, event: Event) {
        let index = self.events.partition_point(|(time, _)| *time <= at);
        self.events.insert(index, (at, event));
    }

    /// Simulates a stall (for example the process being suspended): the first time the
    /// clock is read after `at`, it jumps forward by `length`.
    pub fn push_stall(&mut self, at: Duration, length: Duration) {
        let index = self.stalls.partition_point(|(time, _)| *time <= at);
        self.stalls.insert(index, (at, length));
    }

    /// Makes every wait take at least `min_wait`, so that time also moves forward with
    /// [`FpsLimit::Unlimited`](crate::FpsLimit::Unlimited).
    pub fn set_min_wait(&mut self, min_wait: Duration) {
        self.min_wait = min_wait;
    }

    /// Simulated time since the start.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.now - self.start
    }

    /// Number of scheduled events that have not been delivered yet.
    #[must_use]
    pub fn pending_events(&self) -> usize {
        self.events.len()
    }
}

impl Default for ScriptedDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver for ScriptedDriver {
    fn now(&mut self) -> Instant {
        while let Some(&(at, length)) = self.stalls.front() {
            if self.start + at > self.now {
                break;
            }
            self.now += length;
            self.stalls.pop_front();
        }
        self.now
    }

    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        let deadline = self.now + timeout;
        if let Some((at, _)) = self.events.front() {
            let due = self.start + *at;
            if due <= deadline {
                self.now = self.now.max(due);
                return Ok(self.events.pop_front().map(|(_, event)| event));
            }
        }
        self.now = deadline.max(self.now + self.min_wait);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn events_arrive_in_order_and_move_time() {
        let mut driver = ScriptedDriver::new();
        driver.push_event(MS * 30, Event::FocusGained);
        driver.push_event(MS * 10, Event::FocusLost);

        assert_eq!(driver.poll_event(MS * 5).unwrap(), None);
        assert_eq!(driver.elapsed(), MS * 5);
        assert_eq!(driver.poll_event(MS * 100).unwrap(), Some(Event::FocusLost));
        assert_eq!(driver.elapsed(), MS * 10);
        assert_eq!(
            driver.poll_event(MS * 100).unwrap(),
            Some(Event::FocusGained)
        );
        assert_eq!(driver.elapsed(), MS * 30);
        assert_eq!(driver.pending_events(), 0);
    }

    #[test]
    fn overdue_events_do_not_move_time_backwards() {
        let mut driver = ScriptedDriver::new();
        driver.push_event(MS * 10, Event::FocusLost);
        driver.poll_event(Duration::ZERO).unwrap();
        driver.push_stall(Duration::ZERO, MS * 50);
        driver.now();
        assert_eq!(
            driver.poll_event(Duration::ZERO).unwrap(),
            Some(Event::FocusLost)
        );
        assert_eq!(driver.elapsed(), MS * 50);
    }

    #[test]
    fn stalls_and_min_wait() {
        let mut driver = ScriptedDriver::new();
        driver.push_stall(MS * 10, MS * 100);
        driver.poll_event(MS * 20).unwrap();
        assert_eq!(driver.elapsed(), MS * 20);
        driver.now();
        assert_eq!(driver.elapsed(), MS * 120);

        driver.set_min_wait(MS);
        driver.poll_event(Duration::ZERO).unwrap();
        assert_eq!(driver.elapsed(), MS * 121);
    }
}
