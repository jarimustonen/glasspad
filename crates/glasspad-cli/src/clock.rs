use chrono::{DateTime, Utc};
use glasspad::time::Clock;

/// Wall-clock adapter kept at the side-effecting CLI boundary.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
