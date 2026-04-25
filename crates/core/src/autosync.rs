use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AutoSyncScheduler {
    interval: Option<Duration>,
    next_run_at: Option<Instant>,
}

impl Default for AutoSyncScheduler {
    fn default() -> Self {
        Self {
            interval: None,
            next_run_at: None,
        }
    }
}

impl AutoSyncScheduler {
    pub fn new(interval_seconds: u64) -> Self {
        let mut scheduler = Self::default();
        scheduler.set_interval_seconds(interval_seconds);
        scheduler
    }

    pub fn interval_seconds(&self) -> u64 {
        self.interval.map(|value| value.as_secs()).unwrap_or(0)
    }

    pub fn set_interval_seconds(&mut self, interval_seconds: u64) {
        self.interval = if interval_seconds == 0 {
            None
        } else {
            Some(Duration::from_secs(interval_seconds))
        };
        self.reset();
    }

    pub fn is_enabled(&self) -> bool {
        self.interval.is_some()
    }

    pub fn reset(&mut self) {
        self.next_run_at = self.interval.map(|interval| Instant::now() + interval);
    }

    pub fn consume_due(&mut self) -> bool {
        match (self.interval, self.next_run_at) {
            (Some(interval), Some(next_run_at)) if Instant::now() >= next_run_at => {
                self.next_run_at = Some(Instant::now() + interval);
                true
            }
            _ => false,
        }
    }

    pub fn is_due(&self) -> bool {
        matches!(
            (self.interval, self.next_run_at),
            (Some(_), Some(next_run_at)) if Instant::now() >= next_run_at
        )
    }

    pub fn seconds_until_next_run(&self) -> Option<u64> {
        self.next_run_at.map(|next_run_at| {
            next_run_at
                .saturating_duration_since(Instant::now())
                .as_secs()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AutoSyncScheduler;

    #[test]
    fn disabled_scheduler_is_not_due() {
        let mut scheduler = AutoSyncScheduler::new(0);
        assert!(!scheduler.is_enabled());
        assert!(!scheduler.consume_due());
        assert_eq!(scheduler.seconds_until_next_run(), None);
    }

    #[test]
    fn reset_restarts_countdown() {
        let mut scheduler = AutoSyncScheduler::new(1);
        assert!(scheduler.is_enabled());
        let before = scheduler.seconds_until_next_run().unwrap();
        scheduler.reset();
        let after = scheduler.seconds_until_next_run().unwrap();
        assert!(after >= before.saturating_sub(1));
    }
}
