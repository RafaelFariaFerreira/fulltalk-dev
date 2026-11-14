//! The most recent transcriptions, newest first.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// How many transcriptions are kept.
pub const DEFAULT_CAPACITY: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub text: String,
    /// Unix timestamp (seconds) of when it was transcribed.
    pub created_at: u64,
}

/// A bounded, de-duplicated list of recent transcriptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    entries: VecDeque<Entry>,
    #[serde(skip, default = "default_capacity")]
    capacity: usize,
}

const fn default_capacity() -> usize {
    DEFAULT_CAPACITY
}

impl Default for History {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl History {
    /// # Panics
    /// If `capacity` is zero.
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "history capacity must be positive");
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Adds a transcription on top. Blank text is ignored; text already present
    /// moves to the top instead of appearing twice. Returns whether it was added.
    pub fn push(&mut self, text: &str, created_at: u64) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.entries.retain(|e| e.text != text);
        self.entries.push_front(Entry {
            text: text.to_owned(),
            created_at,
        });
        self.entries.truncate(self.capacity);
        true
    }

    /// Entries, newest first.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Re-applies the in-memory capacity after deserializing (the file may hold more).
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.entries.truncate(self.capacity);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn texts(h: &History) -> Vec<&str> {
        h.iter().map(|e| e.text.as_str()).collect()
    }

    #[test]
    fn newest_entry_comes_first() {
        let mut h = History::default();
        h.push("primeira", 1);
        h.push("segunda", 2);
        assert_eq!(texts(&h), ["segunda", "primeira"]);
    }

    #[test]
    fn keeps_only_the_last_five() {
        let mut h = History::default();
        for i in 1..=7 {
            h.push(&format!("t{i}"), i);
        }
        assert_eq!(texts(&h), ["t7", "t6", "t5", "t4", "t3"]);
    }

    #[test]
    fn ignores_blank_text() {
        let mut h = History::default();
        assert!(!h.push("   ", 1));
        assert!(h.is_empty());
    }

    #[test]
    fn repeated_text_moves_to_top_instead_of_duplicating() {
        let mut h = History::default();
        h.push("a", 1);
        h.push("b", 2);
        h.push("a", 3);
        assert_eq!(texts(&h), ["a", "b"]);
        assert_eq!(h.get(0).unwrap().created_at, 3);
    }

    #[test]
    fn stores_trimmed_text() {
        let mut h = History::default();
        h.push("  oi  ", 1);
        assert_eq!(texts(&h), ["oi"]);
    }

    #[test]
    fn round_trips_through_json_and_respects_capacity() {
        let mut big = History::with_capacity(10);
        for i in 0..10 {
            big.push(&i.to_string(), i);
        }
        let json = serde_json::to_string(&big).unwrap();
        let loaded: History = serde_json::from_str::<History>(&json).unwrap().normalized();
        assert_eq!(loaded.len(), DEFAULT_CAPACITY);
        assert_eq!(loaded.get(0).unwrap().text, "9");
    }

    proptest! {
        #[test]
        fn never_exceeds_capacity_nor_holds_duplicates(items in proptest::collection::vec("[a-c ]{0,3}", 0..40)) {
            let mut h = History::default();
            for (i, item) in items.iter().enumerate() {
                h.push(item, i as u64);
            }
            prop_assert!(h.len() <= DEFAULT_CAPACITY);
            let mut seen = std::collections::HashSet::new();
            for e in h.iter() {
                prop_assert!(!e.text.is_empty());
                prop_assert!(seen.insert(e.text.clone()));
            }
        }
    }
}
