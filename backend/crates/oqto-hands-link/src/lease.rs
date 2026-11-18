//! Execution lease with destination-issued renewal challenges (DESIGN.md 6).
//!
//! The destination offers a FIXED deadline (its now + lease) bound to a
//! single-use challenge. The guard's answer can only confirm that already
//! offered deadline; it never starts a fresh TTL on receipt. Only the newest
//! challenge is answerable, so an old queued renewal cannot revive a lease
//! after a partition and revoke, and nothing renews an expired lease.
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub sequence: u64,
    pub challenge: String,
    /// Absolute deadline on the destination's suspend-aware clock.
    pub deadline_ms: u64,
}

#[derive(Debug)]
pub struct Lease {
    lease_ms: u64,
    deadline_ms: u64,
    next_sequence: u64,
    outstanding: Option<Offer>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenewError {
    Expired,
    NotOutstanding,
}

impl Lease {
    pub fn start(now_ms: u64, lease_ms: u32) -> Self {
        Self {
            lease_ms: u64::from(lease_ms),
            deadline_ms: now_ms.saturating_add(u64::from(lease_ms)),
            next_sequence: 1,
            outstanding: None,
        }
    }
    pub fn alive(&self, now_ms: u64) -> bool {
        now_ms < self.deadline_ms
    }
    pub fn deadline_ms(&self) -> u64 {
        self.deadline_ms
    }
    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.deadline_ms.saturating_sub(now_ms)
    }
    /// Offer the next fixed deadline. Replaces (invalidates) any older offer.
    pub fn offer(&mut self, now_ms: u64, challenge: String) -> Result<Offer> {
        ensure!(self.alive(now_ms), "lease expired; nothing to renew");
        let offer = Offer {
            sequence: self.next_sequence,
            challenge,
            deadline_ms: now_ms.saturating_add(self.lease_ms),
        };
        self.next_sequence += 1;
        self.outstanding = Some(offer.clone());
        Ok(offer)
    }
    /// Accept the guard's answer to the newest offer while still alive.
    pub fn answer(
        &mut self,
        now_ms: u64,
        sequence: u64,
        challenge: &str,
    ) -> std::result::Result<u64, RenewError> {
        if !self.alive(now_ms) {
            self.outstanding = None;
            return Err(RenewError::Expired);
        }
        match self.outstanding.take() {
            Some(offer) if offer.sequence == sequence && offer.challenge == challenge => {
                self.deadline_ms = self.deadline_ms.max(offer.deadline_ms);
                Ok(self.deadline_ms)
            }
            other => {
                self.outstanding = other;
                Err(RenewError::NotOutstanding)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_answer_cannot_extend_past_the_offered_deadline() -> Result<()> {
        let mut lease = Lease::start(1_000, 10_000);
        let offer = lease.offer(4_000, "c1".into())?;
        assert_eq!(offer.deadline_ms, 14_000);
        // The answer arrives 5 s late: the deadline is the offered one, not
        // receipt time + lease.
        assert_eq!(lease.answer(9_000, offer.sequence, "c1"), Ok(14_000));
        assert!(lease.alive(13_999) && !lease.alive(14_000));
        Ok(())
    }

    #[test]
    fn answers_after_expiry_or_to_old_offers_are_refused() -> Result<()> {
        let mut lease = Lease::start(0, 2_000);
        let old = lease.offer(500, "old".into())?;
        let new = lease.offer(1_000, "new".into())?;
        assert_eq!(
            lease.answer(1_100, old.sequence, "old"),
            Err(RenewError::NotOutstanding)
        );
        // A replayed or forged answer leaves the newest offer answerable.
        assert_eq!(
            lease.answer(1_100, new.sequence, "forged"),
            Err(RenewError::NotOutstanding)
        );
        assert_eq!(lease.answer(1_200, new.sequence, "new"), Ok(3_000));
        // Single use.
        assert_eq!(
            lease.answer(1_300, new.sequence, "new"),
            Err(RenewError::NotOutstanding)
        );
        let late = lease.offer(2_500, "late".into())?;
        assert_eq!(
            lease.answer(4_600, late.sequence, "late"),
            Err(RenewError::Expired)
        );
        assert!(lease.offer(4_600, "x".into()).is_err());
        Ok(())
    }

    #[test]
    fn wall_clock_rollback_cannot_extend_a_lease() -> Result<()> {
        // The lease never reads the wall clock; a manual monotonic clock
        // standing in for CLOCK_BOOTTIME only moves forward.
        let clock = crate::clock::ManualClock::new(100);
        use crate::clock::Clock;
        let lease = Lease::start(clock.now_ms(), 2_000);
        clock.advance(2_000);
        assert!(!lease.alive(clock.now_ms()));
        Ok(())
    }
}
