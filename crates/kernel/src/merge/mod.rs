//! Merge: resolving identities onto one human, and undoing it.
//!
//! An identity is a registration, not a person — "registered here", "imported
//! from AcquiredCo", "signed in with Google". Deciding that two of them are
//! the same human is the one operation in this model that cannot be derived,
//! only *proved*, so the whole module is built around the split the kernel
//! report draws:
//!
//! * **Signals propose.** A shared verified factor value, the same OIDC
//!   subject, a claimed link, a name plus a shared group — every one of them
//!   raises a [`match_candidate`] row in `proposed` and stops there. The scan
//!   that raises them runs on the observation lane ([`scan`]), never on a
//!   request, and [`candidate::propose`] is the only statement in this crate
//!   that writes the table from a signal. A proptest proves no signal path can
//!   write `confirmed`.
//! * **Proof disposes.** [`confirm::confirm_match`] takes the person's own
//!   proof — live sessions on both identities, or a factor both have verified.
//!   [`rule::rule_match`] takes a platform operator's ruling with evidence,
//!   which lands on the `person_link` row and is therefore attributable
//!   afterwards. Nothing else merges anybody.
//!
//! Both proofs run the same five steps ([`apply`]) in one transaction, and
//! neither is irreversible: `person_link` is append-only (a trigger, not a
//! convention), `person_alias` keeps the absorbed person's URLs resolving
//! ([`resolve_person`]), and [`split::split`] detaches an identity onto a
//! person of its own with the rows it arrived with.
//!
//! Two things are deliberately *not* touched by a merge. Sessions keep
//! working, because a session binds an identity and merging humans is not a
//! reason to sign anybody out. Product rows keep pointing at their identity,
//! because they never referenced a person in the first place — which is what
//! makes the whole operation cheap enough to have a 50 ms budget at 10⁴ rows.
//!
//! [`match_candidate`]: candidate

pub mod apply;
pub mod candidate;
pub mod confirm;
pub mod recovery;
pub mod rule;
pub mod scan;
pub mod split;
#[cfg(test)]
mod tests;

pub use apply::Merge;
pub use candidate::{CandidateRow, CandidateStatus, propose_match, score};
pub use confirm::confirm_match;
pub use recovery::{owned_identity, target_identity};
pub use rule::rule_match;
pub use scan::scan_identity;
pub use split::split;

use crate::bind;
use crate::domain::Vocabulary;
use crate::error::Outcome;
use crate::ids::{Id, Identity, Person};
use crate::store::{Count, Cursor, FromRow, Reads, RowError};

/// How an identity came to be attached to a person: the `person_link.method`
/// vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkMethod {
    /// The person proved it themselves, by holding a session on both.
    SelfLink,
    /// A factor both identities have verified.
    Factor,
    /// A platform operator ruled, with evidence.
    Operator,
    /// The link arrived with the data. No command produces it in this cut; the
    /// value exists so an import lands without a migration.
    Import,
}

impl Vocabulary for LinkMethod {
    const ALL: &'static [Self] = &[Self::SelfLink, Self::Factor, Self::Operator, Self::Import];
    const COLUMN: (&'static str, &'static str) = ("person_link", "method");

    fn as_str(self) -> &'static str {
        match self {
            Self::SelfLink => "self",
            Self::Factor => "factor",
            Self::Operator => "operator",
            Self::Import => "import",
        }
    }
}

/// What a merge would have to do to a pair of identities.
///
/// Computed before the batch, from rows read outside it, so the transaction
/// carries statements and no decisions. Every arm but [`Self::Nothing`] is
/// guarded on the same condition, so a pair that moved between the read and
/// the commit writes nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// One identity has no person yet: attach it to the other's, which is not
    /// a merge — no person is absorbed and no alias is needed.
    Link {
        /// The unresolved identity.
        identity: Id<Identity>,
        /// The person it joins.
        person: Id<Person>,
    },
    /// Two people become one. The older survives: it is the one whose URLs,
    /// memberships and grants have been in circulation longest, and the alias
    /// row makes the younger keep resolving anyway.
    Absorb {
        /// The person that remains.
        survivor: Id<Person>,
        /// The person that becomes an alias of it.
        absorbed: Id<Person>,
    },
    /// The two identities already resolve to one person. Confirming that is
    /// not an error and not a change.
    Nothing,
}

/// The identity's person, if it has been resolved to one.
pub const PERSON_OF_SQL: &str = "SELECT person_id AS n FROM identity WHERE id = $1";

/// Which person an identity currently resolves to.
pub async fn person_of(store: &impl Reads, identity: Id<Identity>) -> Outcome<Option<Id<Person>>> {
    let rows = store
        .query::<OptionalId>(PERSON_OF_SQL, bind![identity])
        .await?;
    Ok(rows.first().and_then(|row| row.0).map(Id::new))
}

/// A nullable id column read as itself.
struct OptionalId(Option<i64>);

impl FromRow for OptionalId {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.int_opt("n")?))
    }
}

/// Work out what merging these two identities would mean.
///
/// A pair whose identities are the same row, or whose people already agree, is
/// [`Plan::Nothing`] rather than a refusal: the caller asked for a state that
/// already holds.
pub async fn plan(store: &impl Reads, a: Id<Identity>, b: Id<Identity>) -> Outcome<Plan> {
    if a == b {
        return Ok(Plan::Nothing);
    }
    let (left, right) = (person_of(store, a).await?, person_of(store, b).await?);
    Ok(match (left, right) {
        (Some(left), Some(right)) if left == right => Plan::Nothing,
        // The older person survives: the smaller rowid is the earlier row.
        (Some(left), Some(right)) => Plan::Absorb {
            survivor: left.min(right),
            absorbed: left.max(right),
        },
        (None, Some(person)) => Plan::Link {
            identity: a,
            person,
        },
        (Some(person), None) => Plan::Link {
            identity: b,
            person,
        },
        // Neither has a person. Nothing in this leg produces such an identity
        // — `Register` always mints a party — and inventing one here would be
        // a registration by another name.
        (None, None) => Plan::Nothing,
    })
}

/// One alias hop. The chain is walked in Rust rather than in a recursive CTE
/// because the loop is bounded and every hop is a primary-key seek.
pub const ALIAS_SQL: &str = "SELECT person_id AS n FROM person_alias WHERE old_person_id = $1";

/// How far [`resolve_person`] will follow aliases before it stops.
///
/// A merge repoints the aliases it absorbs at the new survivor, so a chain is
/// one hop in practice. The bound is what makes a cycle — which the schema
/// does not prevent and a restore from two halves of a backup could produce —
/// a stall of eight queries rather than a hang.
pub const MAX_ALIAS_HOPS: usize = 8;

/// Follow `person_alias` to the person an id resolves to today.
///
/// This is why an old URL keeps working after a merge: the absorbed person's
/// public id still decodes to its own row, and every read that matters asks
/// this first.
pub async fn resolve_person(store: &impl Reads, person: Id<Person>) -> Outcome<Id<Person>> {
    let mut at = person;
    for _ in 0..MAX_ALIAS_HOPS {
        let rows = store.query::<Count>(ALIAS_SQL, bind![at]).await?;
        match rows.first() {
            Some(next) if next.0 != at.get() => at = Id::new(next.0),
            _ => return Ok(at),
        }
    }
    Ok(at)
}

/// Every identity of a person, oldest first.
pub const IDENTITIES_SQL: &str = "SELECT id FROM identity WHERE person_id = $1 ORDER BY id";

/// The identities a person is made of.
pub async fn identities_of(store: &impl Reads, person: Id<Person>) -> Outcome<Vec<Id<Identity>>> {
    Ok(store
        .query::<crate::store::RowId<Identity>>(IDENTITIES_SQL, bind![person])
        .await?
        .into_iter()
        .map(|row| row.0)
        .collect())
}
