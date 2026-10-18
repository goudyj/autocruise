//! Request pacing: serializes calls and enforces a minimum interval.

use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Ensures at least `min_interval` between two consecutive [`Throttle::wait`] returns.
#[derive(Debug)]
pub struct Throttle {
    min_interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl Throttle {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last: Mutex::new(None),
        }
    }

    /// Waits until the next request is allowed. Callers are served one at a time.
    pub async fn wait(&self) {
        let mut last = self.last.lock().await;
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn spaces_out_calls() {
        let throttle = Throttle::new(Duration::from_millis(500));
        let start = Instant::now();
        throttle.wait().await;
        throttle.wait().await;
        throttle.wait().await;
        assert!(start.elapsed() >= Duration::from_millis(1000));
    }
}
