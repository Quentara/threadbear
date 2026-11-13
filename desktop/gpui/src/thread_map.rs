// The app's threads, keyed by id, with a count of changes: anything that can
// change them goes through `DerefMut`, which counts, so the inbox's sections
// (`Threadbear::shown_sections`) are worked out again only when a thread
// changed, rather than every frame. With Mike's 1,400 threads, sorting copies
// of them all three or four times a frame was half of every frame's work, and
// a hover over a row is a frame (13 Nov).
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use threadbear_core::inbox::ThreadSummary;

#[derive(Default)]
pub(crate) struct ThreadMap {
    map: HashMap<String, ThreadSummary>,
    changes: Cell<u64>,
}

impl ThreadMap {
    /// Goes up whenever the threads might have changed.
    pub(crate) fn changes(&self) -> u64 {
        self.changes.get()
    }
}

impl Deref for ThreadMap {
    type Target = HashMap<String, ThreadSummary>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for ThreadMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.changes.set(self.changes.get() + 1);
        &mut self.map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_every_way_of_changing_it() {
        let mut m = ThreadMap::default();
        let before = m.changes();
        let _ = m.get("x");
        let _ = m.values().count();
        assert_eq!(m.changes(), before, "reading doesn't count");
        m.insert("a".into(), ThreadSummary { id: "a".into(), ..Default::default() });
        let after_insert = m.changes();
        assert!(after_insert > before);
        if let Some(t) = m.get_mut("a") {
            t.title = "x".into();
        }
        assert!(m.changes() > after_insert);
    }
}
