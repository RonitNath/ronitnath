//! The five steps of `Merge(absorbed → survivor)`, as statements.
//!
//! The kernel report writes the operation out as five steps in one
//! transaction, and this module is those five steps and nothing else — no
//! authorisation, no proof, no audit row. [`confirm`](super::confirm),
//! [`rule`](super::rule) and [`split`](super::split) each decide *whether*,
//! then push these statements into their own batch together with their own
//! audit row, so the change and the record of it commit or fail together.
//!
//! ## One guard, on every statement
//!
//! [`Store::commit`](crate::store::Store::commit) can tell "the world moved"
//! from "a statement was wrong" only if every statement in the batch carries
//! the same guard. Here it is `the absorbed party is still an active person`,
//! and the statement that flips it — the status update — is the last mutation
//! in the batch. So a second merge of a pair that has already merged writes
//! nothing at all rather than half of something, which is what makes
//! `merge twice = merge once` a property of the batch rather than of a check
//! somebody remembered to run.
//!
//! ## Why the absorbed person's own rows survive
//!
//! Step 2 and step 3 are a *union*: the survivor gains the absorbed person's
//! memberships and grants. The absorbed rows are then left where they are.
//! They authorise nobody — a merged party is in no principal's subject set,
//! because every identity now resolves to the survivor — and they are the only
//! record of what that person held, which is what [`split`](super::split)
//! copies back when a merge has to be undone.
//!
//! ## Ownership follows the person
//!
//! `Transfer` is the only command that changes an owner; a merge is not a
//! change of owner but a change of *who that owner is*, so it moves
//! `resource.owner_party_id` from the absorbed person to the survivor and
//! unions the `#owner` relation rows across with every other grant. Without
//! it a merged person's documents would be owned by a party no principal can
//! ever expand to, and `list_visible` would stop returning them to the human
//! who still has them open in a tab.
//!
//! Only persons are ever merged. An organization the absorbed person owned
//! changes hands here — its `resource` row does — but the organization itself
//! is not absorbed and its party row is untouched.

use crate::Timestamp;
use crate::cmd::Batch;
use crate::domain::Vocabulary;
use crate::ids::{Id, Identity, Person};
use crate::store::{Expect, Stmt, Value};

use super::LinkMethod;

/// Step 1 — a `person_link` row for every identity of the absorbed person,
/// remembering where it came from so the merge can be undone.
const LINKS: &str = "INSERT INTO person_link \
     (identity_id, person_id, method, evidence, asserted_by, from_person_id, at) \
     SELECT i.id, $1, $2, $3, $4, $5, $6 FROM identity i \
     WHERE i.person_id = $5 \
       AND EXISTS (SELECT 1 FROM party WHERE id = $5 AND kind = 'person' AND status = 'active')";

/// Step 2 — memberships, unioned, strongest role winning.
///
/// The rank is spelled out twice rather than hidden in a function, because
/// SQLite has no user functions here and the nesting `member < admin < owner`
/// is the same one `check()` applies in Rust.
const MEMBERSHIPS: &str = "INSERT INTO membership (group_id, party_id, role, at) \
     SELECT m.group_id, $1, m.role, $2 FROM membership m \
     WHERE m.party_id = $3 \
       AND EXISTS (SELECT 1 FROM party WHERE id = $3 AND kind = 'person' AND status = 'active') \
     ON CONFLICT (group_id, party_id) DO UPDATE SET role = \
       CASE WHEN (CASE excluded.role WHEN 'owner' THEN 3 WHEN 'admin' THEN 2 ELSE 1 END) \
               > (CASE membership.role WHEN 'owner' THEN 3 WHEN 'admin' THEN 2 ELSE 1 END) \
            THEN excluded.role ELSE membership.role END";

/// Step 3 — grants the absorbed person held, unioned onto the survivor.
///
/// `OR IGNORE` is the union: the UNIQUE index on
/// `(object, relation, subject)` means a grant the survivor already had is
/// skipped rather than duplicated, and because roles nest in code, holding
/// both `viewer` and `editor` rows *is* holding the stronger of the two.
const RELATIONS_AS_SUBJECT: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT r.object_kind, r.object_id, r.relation, 'person', $1, r.granted_by, $2 \
     FROM relation r WHERE r.subject_kind = 'person' AND r.subject_id = $3 \
       AND EXISTS (SELECT 1 FROM party WHERE id = $3 AND kind = 'person' AND status = 'active')";

/// Step 3, the other side — grants *about* the absorbed person, such as
/// `person:P #contact @group:X`.
const RELATIONS_AS_OBJECT: &str = "INSERT OR IGNORE INTO relation \
     (object_kind, object_id, relation, subject_kind, subject_id, granted_by, at) \
     SELECT 'person', $1, r.relation, r.subject_kind, r.subject_id, r.granted_by, $2 \
     FROM relation r WHERE r.object_kind = 'person' AND r.object_id = $3 \
       AND EXISTS (SELECT 1 FROM party WHERE id = $3 AND kind = 'person' AND status = 'active')";

/// Step 3½ — ownership follows the person.
///
/// `resource.owner_party_id` is the one column a merge moves that `Transfer`
/// otherwise owns exclusively, and the exception is the orchestrator's ruling:
/// the owner is not changing, the *person* is. The `#owner` relation rows come
/// across with every other grant in [`RELATIONS_AS_SUBJECT`], so the column and
/// the row that mirrors it move in the same batch and cannot disagree.
///
/// An organization the absorbed person owned moves too, because what moves is
/// its `resource` row's owner — the organization itself is never merged, only
/// persons are, and its party row is untouched.
const RESOURCES: &str = "UPDATE resource SET owner_party_id = $1 WHERE owner_party_id = $2 \
     AND EXISTS (SELECT 1 FROM party WHERE id = $2 AND kind = 'person' AND status = 'active')";

/// Step 4 — anything that already resolved to the absorbed person now resolves
/// past it, so a chain stays one hop long.
const ALIAS_REPOINT: &str = "UPDATE person_alias SET person_id = $1 WHERE person_id = $2 \
     AND EXISTS (SELECT 1 FROM party WHERE id = $2 AND kind = 'person' AND status = 'active')";

/// Step 4 — the absorbed person's own id keeps resolving.
const ALIAS_INSERT: &str = "INSERT INTO person_alias (old_person_id, person_id, at) \
     SELECT $1, $2, $3 \
     WHERE EXISTS (SELECT 1 FROM party WHERE id = $1 AND kind = 'person' AND status = 'active')";

/// Step 4 — the identities themselves. `person_link` is the record; this
/// column is the projection every read path uses.
const IDENTITIES: &str = "UPDATE identity SET person_id = $1 WHERE person_id = $2 \
     AND EXISTS (SELECT 1 FROM party WHERE id = $2 AND kind = 'person' AND status = 'active')";

/// Step 4 — a session is not ended, moved or re-minted: it keeps its identity
/// and its token. What changes is the party it speaks as, because that party
/// is about to be `merged` and a principal may not act as one.
const SESSIONS: &str = "UPDATE session SET acting_as = $1 WHERE acting_as = $2 \
     AND EXISTS (SELECT 1 FROM party WHERE id = $2 AND kind = 'person' AND status = 'active')";

/// Step 4 — and the guard everything else carries flips here, last.
const ABSORBED: &str = "UPDATE party SET status = 'merged' \
     WHERE id = $1 AND kind = 'person' AND status = 'active'";

/// Attach an identity that has no person of its own. Not a merge: nothing is
/// absorbed, so there is no alias and no status change.
const LINK_ROW: &str = "INSERT INTO person_link \
     (identity_id, person_id, method, evidence, asserted_by, from_person_id, at) \
     SELECT $1, $2, $3, $4, $5, NULL, $6 \
     WHERE EXISTS (SELECT 1 FROM identity WHERE id = $1 AND person_id IS NULL)";

/// And the projection, carrying the same guard.
const LINK_IDENTITY: &str =
    "UPDATE identity SET person_id = $1 WHERE id = $2 AND person_id IS NULL";

/// One person absorbing another, as a set of statements.
#[derive(Debug, Clone)]
pub struct Merge {
    /// The person that remains.
    pub survivor: Id<Person>,
    /// The person that becomes an alias of it.
    pub absorbed: Id<Person>,
    /// What proved the two are one human.
    pub method: LinkMethod,
    /// What the proof was, when a person had to write it down.
    pub evidence: Option<String>,
    /// Who asserted it, when anybody did.
    pub asserted_by: Option<Id<Identity>>,
    /// The instant the whole command is stamped with.
    pub at: Timestamp,
}

impl Merge {
    /// Push the five steps onto a batch, in order.
    ///
    /// The caller's audit row goes on afterwards and reads `changes() > 0`,
    /// which is the status update immediately above it.
    pub fn push(&self, batch: &mut Batch) {
        let survivor = Value::from(self.survivor);
        let absorbed = Value::from(self.absorbed);
        let at = Value::from(self.at);
        let evidence = Value::from(self.evidence.clone());
        let asserted_by = Value::from(self.asserted_by);

        batch.push(Stmt {
            sql: LINKS,
            params: vec![
                survivor.clone(),
                Value::from(self.method.as_str()),
                evidence,
                asserted_by,
                absorbed.clone(),
                at.clone(),
            ],
            // A person always has at least one identity: it is a person
            // because an identity resolved to it.
            expect: Expect::AtLeast(1),
        });
        for sql in [MEMBERSHIPS, RELATIONS_AS_SUBJECT, RELATIONS_AS_OBJECT] {
            batch.any(sql, vec![survivor.clone(), at.clone(), absorbed.clone()]);
        }
        batch.any(RESOURCES, vec![survivor.clone(), absorbed.clone()]);
        batch.any(ALIAS_REPOINT, vec![survivor.clone(), absorbed.clone()]);
        batch.one(
            ALIAS_INSERT,
            vec![absorbed.clone(), survivor.clone(), at.clone()],
        );
        batch.push(Stmt {
            sql: IDENTITIES,
            params: vec![survivor.clone(), absorbed.clone()],
            expect: Expect::AtLeast(1),
        });
        batch.any(SESSIONS, vec![survivor, absorbed.clone()]);
        batch.one(ABSORBED, vec![absorbed]);
    }
}

/// An identity with no person of its own, joining one.
#[derive(Debug, Clone)]
pub struct Link {
    /// The unresolved identity.
    pub identity: Id<Identity>,
    /// The person it joins.
    pub person: Id<Person>,
    /// What proved it.
    pub method: LinkMethod,
    /// The proof, written down.
    pub evidence: Option<String>,
    /// Who asserted it.
    pub asserted_by: Option<Id<Identity>>,
    /// The instant the command is stamped with.
    pub at: Timestamp,
}

impl Link {
    /// Push the link and its projection onto a batch.
    pub fn push(&self, batch: &mut Batch) {
        batch.one(
            LINK_ROW,
            vec![
                Value::from(self.identity),
                Value::from(self.person),
                Value::from(self.method.as_str()),
                Value::from(self.evidence.clone()),
                Value::from(self.asserted_by),
                Value::from(self.at),
            ],
        );
        batch.one(
            LINK_IDENTITY,
            vec![Value::from(self.person), Value::from(self.identity)],
        );
    }
}
