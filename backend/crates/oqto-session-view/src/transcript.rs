//! Everything a client holds of one conversation: entries from views, pages and the cache,
//! and the gaps between known ranges.
//!
//! Same rules as upstream's `ConversationTranscript` and byoagnt's client: a merge only grows
//! the transcript, and ranges are never computed from entry ids beyond exclusive bounds
//! (ids have holes).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::frames::Entry;

/// "Unbounded" for the exclusive lower bound and the newest known id.
const NONE: i64 = i64::MIN;

fn signed(id: u64) -> i64 {
    i64::try_from(id).unwrap_or(i64::MAX)
}

fn unsigned(id: i64) -> Option<u64> {
    u64::try_from(id).ok()
}

/// Entries with ids strictly between `after` and `up_to` are missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Newest id known before the gap (exclusive).
    pub after: i64,
    /// Oldest id known after the gap (exclusive).
    #[serde(rename = "upTo")]
    pub up_to: i64,
}

impl Gap {
    /// `min` of the ascending read that fills it.
    pub fn read_min(&self) -> u64 {
        unsigned(self.after.saturating_add(1)).unwrap_or(0)
    }

    /// `max` of the ascending read that fills it.
    pub fn read_max(&self) -> u64 {
        unsigned(self.up_to.saturating_sub(1)).unwrap_or(0)
    }
}

/// Serializable transcript state, as kept in a cache. `None` stands for "unbounded".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TranscriptRecord {
    /// Held entries in id order.
    pub entries: Vec<Entry>,
    /// Missing ranges.
    pub gaps: Vec<Gap>,
    /// Exclusive lower bound of the known range; `None` when the start is known.
    pub low: Option<i64>,
    /// Newest id the known range reaches.
    pub tail: Option<i64>,
}

/// One row to render, in id order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row<'a> {
    /// Older entries may exist above.
    Earlier,
    /// A held entry.
    Entry(&'a Entry),
    /// Missing entries between two known ranges.
    Gap(Gap),
}

/// The client's transcript of one conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    held: Vec<Entry>,
    ids: BTreeSet<u64>,
    gaps: Vec<Gap>,
    started: bool,
    low: i64,
    tail: i64,
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcript {
    /// Nothing held, nothing known.
    pub const fn new() -> Self {
        Self {
            held: Vec::new(),
            ids: BTreeSet::new(),
            gaps: Vec::new(),
            started: false,
            low: NONE,
            tail: NONE,
        }
    }

    /// Rebuild from a cache record.
    pub fn from_record(record: TranscriptRecord) -> Self {
        let mut held = record.entries;
        held.sort_by_key(Entry::id);
        held.dedup_by_key(|entry| entry.id());
        let ids = held.iter().map(Entry::id).collect();
        Self {
            held,
            ids,
            gaps: record.gaps,
            started: true,
            low: record.low.unwrap_or(NONE),
            tail: record.tail.unwrap_or(NONE),
        }
    }

    /// The state for a cache.
    pub fn to_record(&self) -> TranscriptRecord {
        TranscriptRecord {
            entries: self.held.clone(),
            gaps: self.gaps.clone(),
            low: (self.low != NONE).then_some(self.low),
            tail: (self.tail != NONE).then_some(self.tail),
        }
    }

    /// Whether any range is known.
    pub const fn begun(&self) -> bool {
        self.started
    }

    /// Newest id the transcript reaches; attach with `from = newest + 1` to continue it.
    pub fn newest(&self) -> Option<u64> {
        if self.tail == NONE {
            None
        } else {
            unsigned(self.tail)
        }
    }

    /// The inclusive `max` of the next earlier read, while older entries may exist.
    pub fn earlier_bound(&self) -> Option<u64> {
        if self.started && self.low != NONE {
            unsigned(self.low)
        } else {
            None
        }
    }

    /// Held entries in id order.
    pub fn entries(&self) -> &[Entry] {
        &self.held
    }

    /// Missing ranges in id order.
    pub fn gaps(&self) -> &[Gap] {
        &self.gaps
    }

    /// Whether the entry with `id` is held.
    pub fn has(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    /// One view value: its window joins the transcript. Returns whether anything changed.
    pub fn merge(&mut self, window: &[Entry], before: Option<u64>) -> bool {
        let hi = window
            .last()
            .map(|e| signed(e.id()))
            .or(before.map(signed))
            .unwrap_or(NONE);
        self.cover(before.map_or(NONE, signed), hi, window)
    }

    /// A gap page: `known` is the exclusive bound the page continues, `max` the read's
    /// inclusive max, `more` whether the reply carried `next`.
    pub fn cover_ascending(&mut self, known: u64, max: u64, items: &[Entry], more: bool) -> bool {
        let hi = match items.last() {
            Some(newest) if more => signed(newest.id()),
            _ => signed(max),
        };
        self.cover(signed(known), hi, items)
    }

    /// An earlier page, newest first, at or below the inclusive `max`.
    pub fn cover_descending(&mut self, max: u64, items: &[Entry], more: bool) -> bool {
        let ascending: Vec<Entry> = items.iter().rev().cloned().collect();
        let lo = match ascending.first() {
            Some(oldest) if more => signed(oldest.id()).saturating_sub(1),
            _ => NONE,
        };
        self.cover(lo, signed(max), &ascending)
    }

    /// Every entry with an id in (`lo`, `hi`] is known; `added` are those entries in id order.
    fn cover(&mut self, lo: i64, hi: i64, added: &[Entry]) -> bool {
        let was_empty = self.held.is_empty();
        let mut fresh: Vec<Entry> = added
            .iter()
            .filter(|e| !self.ids.contains(&e.id()))
            .cloned()
            .collect();
        fresh.dedup_by_key(|e| e.id());
        let mut changed = !fresh.is_empty();
        let previous_newer = |hi: i64, held: &[Entry]| {
            held.iter()
                .find(|e| signed(e.id()) > hi)
                .map(|e| signed(e.id()))
        };
        let upper_neighbour = if hi != NONE && hi < self.low {
            previous_newer(hi, &self.held)
        } else {
            None
        };
        if !fresh.is_empty() {
            for entry in &fresh {
                self.ids.insert(entry.id());
            }
            let appended = self
                .held
                .last()
                .is_none_or(|last| fresh[0].id() > last.id());
            self.held.extend(fresh);
            if !appended {
                self.held.sort_by_key(Entry::id);
            }
        }
        if !self.started || (was_empty && self.gaps.is_empty() && lo >= self.tail) {
            changed = changed || !self.started || lo != self.low || hi > self.tail;
            self.started = true;
            self.low = lo;
            self.tail = self.tail.max(hi);
        } else if lo > self.tail {
            self.gaps.push(Gap {
                after: self.tail,
                up_to: lo.saturating_add(1),
            });
            self.tail = hi;
            changed = true;
        } else if hi != NONE && hi < self.low {
            let up_to = upper_neighbour.unwrap_or(self.low.saturating_add(1));
            self.gaps.insert(0, Gap { after: hi, up_to });
            self.low = lo;
            changed = true;
        } else {
            if let Some(gaps) = self.subtract(lo, hi) {
                self.gaps = gaps;
                changed = true;
            }
            if lo < self.low {
                self.low = lo;
                changed = true;
            }
            if hi > self.tail {
                self.tail = hi;
                changed = true;
            }
        }
        changed
    }

    /// The gaps without (`lo`, `hi`]; `None` when none changes.
    fn subtract(&self, lo: i64, hi: i64) -> Option<Vec<Gap>> {
        if hi <= lo {
            return None;
        }
        let mut changed = false;
        let mut gaps = Vec::with_capacity(self.gaps.len() + 1);
        for gap in &self.gaps {
            if hi <= gap.after || lo.saturating_add(1) >= gap.up_to {
                gaps.push(*gap);
                continue;
            }
            changed = true;
            if lo > gap.after {
                let first_known = self
                    .held
                    .iter()
                    .find(|e| signed(e.id()) > lo)
                    .map_or(lo.saturating_add(1), |e| signed(e.id()));
                gaps.push(Gap {
                    after: gap.after,
                    up_to: gap.up_to.min(first_known),
                });
            }
            if hi.saturating_add(1) < gap.up_to {
                gaps.push(Gap {
                    after: hi,
                    up_to: gap.up_to,
                });
            }
        }
        changed.then_some(gaps)
    }

    /// Rows in id order: `Earlier` while the start is unknown, then entries with every gap in
    /// place.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut rows = Vec::with_capacity(self.held.len() + self.gaps.len() + 1);
        if self.earlier_bound().is_some() {
            rows.push(Row::Earlier);
        }
        let mut next = 0;
        for entry in &self.held {
            while next < self.gaps.len() && self.gaps[next].after < signed(entry.id()) {
                rows.push(Row::Gap(self.gaps[next]));
                next += 1;
            }
            rows.push(Row::Entry(entry));
        }
        rows.extend(self.gaps[next..].iter().copied().map(Row::Gap));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: u64) -> Entry {
        match Entry::new(json!({"id": id, "conversationId": 1, "kind": "pi.user"})) {
            Ok(entry) => entry,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn a_later_window_opens_a_gap_and_pages_fill_it() {
        let mut t = Transcript::new();
        t.merge(&[entry(7), entry(11)], None);
        assert_eq!(t.earlier_bound(), None);
        t.merge(&[entry(24), entry(27)], Some(23));
        assert_eq!(
            t.gaps(),
            &[Gap {
                after: 11,
                up_to: 24
            }]
        );
        assert_eq!((t.gaps()[0].read_min(), t.gaps()[0].read_max()), (12, 23));
        t.cover_ascending(11, 23, &[entry(12), entry(15)], true);
        assert_eq!(
            t.gaps(),
            &[Gap {
                after: 15,
                up_to: 24
            }]
        );
        t.cover_ascending(15, 23, &[entry(16), entry(23)], false);
        assert!(t.gaps().is_empty());
        assert_eq!(t.entries().len(), 8);
    }

    #[test]
    fn earlier_pages_reach_the_start() {
        let mut t = Transcript::new();
        t.merge(&[entry(30), entry(31)], Some(29));
        assert_eq!(t.earlier_bound(), Some(29));
        t.cover_descending(29, &[entry(29), entry(20)], true);
        assert_eq!(t.earlier_bound(), Some(19));
        t.cover_descending(19, &[entry(3)], false);
        assert_eq!(t.earlier_bound(), None);
        assert!(matches!(t.rows().first(), Some(Row::Entry(_))));
    }

    #[test]
    fn record_round_trip() {
        let mut t = Transcript::new();
        t.merge(&[entry(5)], Some(4));
        let back = Transcript::from_record(t.to_record());
        assert_eq!(back, t);
    }
}
