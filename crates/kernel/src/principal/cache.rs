//! The warm half of principal resolution.
//!
//! Resolving a cookie costs three indexed queries. Every authenticated request
//! makes them, and they answer the same thing until something about that
//! identity changes — so the answer is cached, keyed by the session's digest,
//! and thrown away when the change feed says a command touched the identity or
//! the person behind it. The budget the kernel report sets is ≤ 2 ms cold and
//! 50 µs warm; the warm number is what this file exists for.
//!
//! ## Why two generations rather than a linked list
//!
//! A textbook LRU is a hash map over an intrusive list, and the list is the
//! part that costs unsafe code or a slab of indices to get right. What is
//! actually needed here is a *bound* and a bias toward recency, not exact LRU
//! order — evicting a session that has not been seen in a while is free, since
//! the cold path re-resolves it in under 2 ms. So: two generations. Reads
//! promote from cold to hot; when hot fills, cold is dropped and hot becomes
//! cold. Everything is O(1), nothing is unsafe, and the memory bound is twice
//! the capacity.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ids::{Id, Identity, Person};

/// A digest-keyed cache entry: whatever the caller wants to keep warm.
pub type Digest = [u8; 32];

/// A bounded cache of resolved principals.
#[derive(Debug)]
pub struct Cache<T> {
    inner: Mutex<Generations<T>>,
    capacity: usize,
}

#[derive(Debug)]
struct Generations<T> {
    hot: HashMap<Digest, Entry<T>>,
    cold: HashMap<Digest, Entry<T>>,
}

#[derive(Debug, Clone)]
struct Entry<T> {
    value: T,
    /// Who the entry is about, so a change feed event can find it without
    /// knowing which cookie it was cached under.
    identity: Id<Identity>,
    person: Option<Id<Person>>,
}

impl<T: Clone> Cache<T> {
    /// A cache holding between `capacity` and `2 * capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Generations {
                hot: HashMap::with_capacity(capacity),
                cold: HashMap::new(),
            }),
            capacity: capacity.max(1),
        }
    }

    /// Look up a session digest, promoting a cold hit.
    pub fn get(&self, key: &Digest) -> Option<T> {
        let mut inner = self.lock();
        if let Some(entry) = inner.hot.get(key) {
            return Some(entry.value.clone());
        }
        let entry = inner.cold.remove(key)?;
        let value = entry.value.clone();
        inner.hot.insert(*key, entry);
        Some(value)
    }

    /// Remember a resolution, evicting the older generation if hot is full.
    pub fn put(&self, key: Digest, value: T, identity: Id<Identity>, person: Option<Id<Person>>) {
        let mut inner = self.lock();
        if inner.hot.len() >= self.capacity {
            inner.cold = std::mem::take(&mut inner.hot);
        }
        inner.hot.insert(
            key,
            Entry {
                value,
                identity,
                person,
            },
        );
    }

    /// Forget everything about one identity.
    pub fn forget_identity(&self, identity: Id<Identity>) {
        let mut inner = self.lock();
        inner.hot.retain(|_, e| e.identity != identity);
        inner.cold.retain(|_, e| e.identity != identity);
    }

    /// Forget everything about one person — every identity resolved onto it.
    pub fn forget_person(&self, person: Id<Person>) {
        let mut inner = self.lock();
        inner.hot.retain(|_, e| e.person != Some(person));
        inner.cold.retain(|_, e| e.person != Some(person));
    }

    /// Forget one session.
    pub fn forget(&self, key: &Digest) {
        let mut inner = self.lock();
        inner.hot.remove(key);
        inner.cold.remove(key);
    }

    /// Forget everything. For a release that changed what a resolution means.
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.hot.clear();
        inner.cold.clear();
    }

    /// How many entries are held, across both generations.
    pub fn len(&self) -> usize {
        let inner = self.lock();
        inner.hot.len() + inner.cold.len()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Generations<T>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(n: u8) -> Digest {
        [n; 32]
    }

    fn cache() -> Cache<&'static str> {
        Cache::new(2)
    }

    #[test]
    fn a_hit_comes_back_and_a_miss_does_not() {
        let cache = cache();
        cache.put(digest(1), "one", Id::new(1), Some(Id::new(10)));
        assert_eq!(cache.get(&digest(1)), Some("one"));
        assert_eq!(cache.get(&digest(2)), None);
    }

    #[test]
    fn the_cache_is_bounded_and_keeps_what_was_touched() {
        let cache = cache();
        for n in 1..=2 {
            cache.put(digest(n), "x", Id::new(n.into()), None);
        }
        // Touching 1 keeps it reachable across the generation flip.
        assert_eq!(cache.get(&digest(1)), Some("x"));
        for n in 3..=6 {
            cache.put(digest(n), "x", Id::new(n.into()), None);
        }
        assert!(
            cache.len() <= 4,
            "two generations of two, never more: {}",
            cache.len()
        );
        assert_eq!(cache.get(&digest(6)), Some("x"), "the newest is warm");
    }

    #[test]
    fn a_change_to_an_identity_forgets_every_session_it_had() {
        let cache = cache();
        cache.put(digest(1), "a", Id::new(7), Some(Id::new(70)));
        cache.put(digest(2), "b", Id::new(7), Some(Id::new(70)));
        cache.forget_identity(Id::new(7));
        assert!(cache.is_empty(), "both sessions of that identity are gone");
    }

    #[test]
    fn a_change_to_a_person_forgets_every_identity_resolved_onto_it() {
        let cache = cache();
        cache.put(digest(1), "a", Id::new(1), Some(Id::new(70)));
        cache.put(digest(2), "b", Id::new(2), Some(Id::new(70)));
        cache.put(digest(3), "c", Id::new(3), None);
        cache.forget_person(Id::new(70));
        assert_eq!(cache.get(&digest(1)), None);
        assert_eq!(cache.get(&digest(2)), None);
        assert_eq!(cache.get(&digest(3)), Some("c"), "unresolved is untouched");
    }

    #[test]
    fn forgetting_one_session_leaves_the_others() {
        let cache = cache();
        cache.put(digest(1), "a", Id::new(1), None);
        cache.put(digest(2), "b", Id::new(2), None);
        cache.forget(&digest(1));
        assert_eq!(cache.get(&digest(1)), None);
        assert_eq!(cache.get(&digest(2)), Some("b"));
        cache.clear();
        assert!(cache.is_empty());
    }
}
