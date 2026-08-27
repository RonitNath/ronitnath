//! Products: what this build can serve, and what this deployment has turned
//! on.
//!
//! Two halves that must not be one. The **catalogue** is compiled in — a
//! product's slug, its name, what it is for, and the routes it mounts — and
//! the **enablement** is a row. A slug that is not in the binary cannot be
//! enabled, which is the argument [`crate::relation::vocabulary`] already
//! makes about unregistered kinds: a typo refuses the command rather than
//! writing a row nothing will ever read. And a slug with **no row is
//! disabled**, so a release that adds a product does not turn it on across
//! every deployment the moment they upgrade.
//!
//! ## The projection
//!
//! [`ProductSet`] is what a request actually asks. It is a bitmask over the
//! catalogue's own order, held on every node, replaced when the change feed
//! says a toggle happened (`server::sub::invalidate`) and read by the route
//! gate (`server::product::gate`). One relaxed atomic load and a shift is the
//! whole cost of a gated request.
//!
//! It is a bitmask rather than an `ArcSwap<…>` of a richer structure — the
//! shape requirement B5.3 names — because everything the gate has to know
//! about a product is one bit, the catalogue it indexes is compiled in beside
//! it, and an atomic word is a smaller thing to be wrong about than a swapped
//! allocation. Nothing else in the projection would have a reader: the
//! products *screen* reads the table, because what it renders — when, and by
//! whom — is not in the mask and must not be stale.
//!
//! The mask holds no truth of its own. It is derived from [`ENABLED_SQL`] and
//! can always be derived again, which is what makes losing a wake-up cost
//! latency instead of correctness.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Outcome;
use crate::store::{Cursor, FromRow, Reads, RowError};

/// One product this build carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Product {
    /// What a command names it by, and what the row is keyed on.
    pub slug: &'static str,
    /// What a screen calls it.
    pub display: &'static str,
    /// One sentence: what turning it on gives a reader.
    pub summary: &'static str,
    /// The routes it mounts, as they are written in the router.
    ///
    /// Rendered on the products screen, so that turning something off states
    /// what it is about to take away rather than leaving an operator to find
    /// out from a 404.
    pub mounts: &'static [&'static str],
}

/// The compiled-in catalogue: every product this binary can serve.
pub type Catalogue = &'static [Product];

/// This build's catalogue.
///
/// One entry, and that is the honest count rather than a placeholder for
/// three. A product is a feature area with routes of its own, and inventing
/// the *rows* for features this deployment has not built would make the
/// products screen state what turning them off would take away — which would
/// be nothing, which would be a lie. The deployment's other named products
/// (`presence`, `events` — `docs/stories/platform-admin.md` story 5) arrive as
/// entries here when the features arrive, and until then they are exactly the
/// case B5.1 is about: a slug the binary does not carry cannot be enabled.
pub const CATALOGUE: Catalogue = &[Product {
    slug: "starscape",
    display: "Starscape",
    summary: "Tonight's sky as a page of its own, with no card in front of it.",
    mounts: &["/sky"],
}];

/// How many products a [`ProductSet`] can hold.
///
/// The mask is one word. A catalogue that outgrows it is a compile-time
/// decision to widen the word, not a runtime surprise — `the_catalogue_fits`
/// is the assertion that says so.
pub const CAPACITY: usize = u64::BITS as usize;

/// The product this slug names, if this build carries one.
#[must_use]
pub fn find(slug: &str) -> Option<&'static Product> {
    CATALOGUE.iter().find(|product| product.slug == slug)
}

/// Where a slug sits in the catalogue — which is also its bit.
#[must_use]
pub fn index_of(slug: &str) -> Option<usize> {
    CATALOGUE.iter().position(|product| product.slug == slug)
}

/// Which products are on, as one node currently believes.
///
/// Every node holds one. It is replaced — never mutated in place — when the
/// feed carries [`crate::Event::ProductEnabled`] or
/// [`crate::Event::ProductDisabled`], and on nothing else.
#[derive(Debug, Default)]
pub struct ProductSet(AtomicU64);

impl ProductSet {
    /// Nothing enabled: what a node holds before it has read the table, and
    /// the safe direction — a product reads as off until this node knows
    /// otherwise.
    #[must_use]
    pub const fn empty() -> Self {
        Self(AtomicU64::new(0))
    }

    /// The set these bits describe.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(AtomicU64::new(bits))
    }

    /// Replace the whole set. The only writer is the feed consumer.
    pub fn replace(&self, bits: u64) {
        self.0.store(bits, Ordering::Release);
    }

    /// The bits, as they stand.
    #[must_use]
    pub fn bits(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    /// Whether the product at this catalogue position is on.
    ///
    /// The gate's whole question: one atomic load and a shift. `index` comes
    /// from the catalogue, so it is in range by construction; an out-of-range
    /// one reads as off rather than panicking, because a router that
    /// half-answers is worse than one that declines.
    #[must_use]
    pub fn at(&self, index: usize) -> bool {
        index < CAPACITY && self.bits() & (1 << index) != 0
    }

    /// Whether the product this slug names is on. For a caller that has a
    /// slug rather than a mounted route — the bundle's rail, a test.
    #[must_use]
    pub fn holds(&self, slug: &str) -> bool {
        index_of(slug).is_some_and(|index| self.at(index))
    }

    /// The enabled slugs, in catalogue order.
    #[must_use]
    pub fn slugs(&self) -> Vec<&'static str> {
        let bits = self.bits();
        CATALOGUE
            .iter()
            .enumerate()
            .filter(|(index, _)| bits & (1 << index) != 0)
            .map(|(_, product)| product.slug)
            .collect()
    }
}

/// The enabled set, as the projection reads it: one indexed scan of a table
/// with one row per product anybody has ever decided about.
///
/// A slug the table carries and the binary does not is skipped rather than
/// rejected — that is a node running an older release than the one that
/// enabled it, and refusing to boot over it would turn a rollout into an
/// outage.
pub const ENABLED_SQL: &str = "SELECT slug FROM product WHERE enabled = 1";

/// One `slug` column.
struct Slug(String);

impl FromRow for Slug {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("slug")?))
    }
}

/// Read the enabled set out of the database.
///
/// Called once at boot and once per toggle per node, and toggles are a human
/// action — so this is allowed to be a whole small table rather than a diff.
///
/// # Errors
///
/// Whatever the store says. A caller that cannot read keeps the mask it has,
/// which is stale rather than wrong-way-open.
pub async fn read(store: &impl Reads) -> Outcome<u64> {
    let rows = store.query::<Slug>(ENABLED_SQL, Vec::new()).await?;
    Ok(rows
        .into_iter()
        .filter_map(|Slug(slug)| index_of(&slug))
        .fold(0u64, |bits, index| bits | (1 << index)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_fits_in_one_word_and_names_nothing_twice() {
        assert!(
            CATALOGUE.len() <= CAPACITY,
            "the catalogue has outgrown the projection's word; widen ProductSet"
        );
        let mut slugs: Vec<&str> = CATALOGUE.iter().map(|product| product.slug).collect();
        let count = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), count, "a slug appears twice in the catalogue");
    }

    #[test]
    fn every_product_states_what_it_mounts() {
        for product in CATALOGUE {
            assert!(!product.mounts.is_empty(), "{} mounts nothing", product.slug);
            for path in product.mounts {
                assert!(path.starts_with('/'), "{path} is not a route");
            }
            assert!(!product.display.is_empty());
            assert!(!product.summary.is_empty());
            assert!(
                product
                    .slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{} is not a kebab-case slug",
                product.slug
            );
        }
    }

    #[test]
    fn a_slug_this_build_does_not_carry_is_not_in_the_catalogue() {
        assert!(find("starscape").is_some());
        assert!(find("events").is_none());
        assert!(index_of("events").is_none());
    }

    #[test]
    fn an_empty_set_holds_nothing_and_a_replaced_one_holds_what_it_was_given() {
        let set = ProductSet::empty();
        assert!(!set.holds("starscape"));
        assert!(set.slugs().is_empty());

        let index = index_of("starscape").expect("the catalogue carries it");
        set.replace(1 << index);
        assert!(set.holds("starscape"));
        assert!(set.at(index));
        assert_eq!(set.slugs(), vec!["starscape"]);

        set.replace(0);
        assert!(!set.holds("starscape"));
    }

    #[test]
    fn a_bit_beyond_the_catalogue_reads_as_off() {
        let set = ProductSet::from_bits(u64::MAX);
        assert!(!set.at(CAPACITY));
        assert!(!set.at(CAPACITY + 1));
    }
}
