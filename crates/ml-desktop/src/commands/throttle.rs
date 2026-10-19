//! Sends at most one event per interval.
//!
//! An installation of a modern version of the game fetches thousands of
//! files. Sending an event for every one of them would cost the frontend
//! more than the download costs the launcher, so only a few reports per
//! second go out — and the last one always goes out, so the interface
//! never shows a bar that stopped at 99 %.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Throttles the events the frontend is told about.
pub struct Throttle {
    /// The channel the events go out on.
    channel: &'static str,
    /// The shortest time between two reports.
    interval: Duration,
    /// When the last report went out.
    last: Mutex<Option<Instant>>,
}

impl Throttle {
    /// A throttle that lets a report through every `interval`.
    #[must_use]
    pub fn new(channel: &'static str, interval: Duration) -> Self {
        Self {
            channel,
            interval,
            last: Mutex::new(None),
        }
    }

    /// Whether a report is due, and remember that it went out.
    fn due(&self, now: Instant) -> bool {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let due = last.is_none_or(|then| now.duration_since(then) >= self.interval);
        if due {
            *last = Some(now);
        }
        due
    }

    /// Send the event, if enough time has passed since the last one.
    pub fn report<E: Serialize + Clone>(&self, app: &AppHandle, event: E) {
        if !self.due(Instant::now()) {
            return;
        }
        let _ = app.emit(self.channel, &event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_report_goes_out() {
        let throttle = Throttle::new("test", Duration::from_secs(10));
        assert!(throttle.due(Instant::now()));
    }

    #[test]
    fn a_report_right_after_the_last_one_is_held_back() {
        let throttle = Throttle::new("test", Duration::from_secs(10));
        assert!(throttle.due(Instant::now()));
        assert!(!throttle.due(Instant::now()));
    }

    #[test]
    fn a_report_after_the_interval_goes_out() {
        let throttle = Throttle::new("test", Duration::from_millis(1));
        assert!(throttle.due(Instant::now()));
        std::thread::sleep(Duration::from_millis(5));
        assert!(throttle.due(Instant::now()));
    }
}
