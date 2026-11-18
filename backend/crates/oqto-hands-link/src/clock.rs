//! Suspend-aware monotonic time for lease deadlines (DESIGN.md section 6).
//! `std::time::Instant` stops during system sleep on Linux (CLOCK_MONOTONIC)
//! and macOS (mach_absolute_time), so a lease measured with it would survive a
//! suspend. Linux CLOCK_BOOTTIME and macOS CLOCK_MONOTONIC include sleep time;
//! wall-clock changes affect neither.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub trait Clock: Send + Sync {
    /// Milliseconds since an arbitrary fixed point, advancing during suspend.
    fn now_ms(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SuspendAwareClock;

impl Clock for SuspendAwareClock {
    fn now_ms(&self) -> u64 {
        #[cfg(target_os = "linux")]
        let id = rustix::time::ClockId::Boottime;
        #[cfg(not(target_os = "linux"))]
        let id = rustix::time::ClockId::Monotonic;
        let time = rustix::time::clock_gettime(id);
        // Both fields are non-negative for these clocks.
        (time.tv_sec.max(0) as u64)
            .saturating_mul(1000)
            .saturating_add((time.tv_nsec.max(0) as u64) / 1_000_000)
    }
}

/// Manually advanced clock for deterministic lease tests.
#[derive(Clone, Debug, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    pub fn new(start: u64) -> Self {
        Self(Arc::new(AtomicU64::new(start)))
    }
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspend_aware_clock_is_monotonic() {
        let clock = SuspendAwareClock;
        let first = clock.now_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(clock.now_ms() >= first + 4);
    }
}
