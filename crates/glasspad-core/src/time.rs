//! Deterministic time seam for pure domain decisions.

use chrono::{DateTime, Duration, Utc};

/// Supplies the current UTC time to domain logic.
///
/// Implementations live at the edge: production uses the CLI crate's system
/// clock, while tests can supply a fixed value without reading wall-clock time.
pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

/// Compute the oldest timestamp retained by a retention policy.
pub fn retention_cutoff(clock: &impl Clock, retention: Duration) -> DateTime<Utc> {
    clock.now() - retention
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[test]
    fn cutoff_uses_injected_clock() {
        let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
        let clock = FixedClock(now);
        assert_eq!(
            retention_cutoff(&clock, Duration::days(90)),
            Utc.with_ymd_and_hms(2026, 5, 22, 12, 0, 0).unwrap()
        );
    }
}
