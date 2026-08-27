//! The audit's filters, and the index each one rides.
//!
//! `audit` is the change feed, it is the audit log, and by ruling it has no
//! retention — so it is the one table in this schema whose size is the
//! deployment's whole history. A filter over it with no index is not slow, it
//! is unusable, and it becomes unusable exactly on the deployment that has
//! most need of it.
//!
//! There are five controls — actor, hat, object, command and a time range,
//! whose two ends are two parameters — so sixty-four ways to combine them.
//! Sixty-four statements would be sixty-four chances to forget an index, so
//! instead there are **six**: one per *lead*, and the lead
//! is whichever filter is present and most selective. The lead is the seek;
//! everything else rides along as a predicate on the rows that seek already
//! narrowed to, expressed as `$n IS NULL OR …` so one statement serves every
//! combination that shares its lead.
//!
//! | lead | index |
//! |---|---|
//! | object | `audit_object`'s own primary key `(kind, id, audit_id)` |
//! | actor | `audit_actor_idx (actor_identity_id, id)` |
//! | hat | `audit_acting_as_idx (acting_as, id)` |
//! | command | `audit_command_idx (command, id)` |
//! | time | `audit_at_idx (at)` |
//! | none | `audit`'s own primary key, walked backwards from `before` |
//!
//! The order of preference is the order of that table, and it is selectivity:
//! an object names a handful of rulings, an actor names one person's work, a
//! command names every time anybody ran it, and a day names a day. A caller
//! who names both gets the narrower seek and the other as a predicate.
//!
//! `tests/explain.rs` generates all sixty-four combinations from the screen's
//! own control list and asserts every one of them plans as a seek, so a filter
//! added without an index fails the build rather than review.

use rn_api::PublicId;
use rn_kernel::ids::{
    self, Factor, Group, IdKey, Identity, Link, OidcClient, Organization, Person, Resource,
    Service, Session,
};
use rn_kernel::store::Value;

use super::Params;

/// No filter: the primary key, walked backwards. This is the live tail.
pub const TAIL: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE a.id < $1 \
       AND ($2 IS NULL OR a.actor_identity_id = $2) \
       AND ($3 IS NULL OR a.acting_as = $3) \
       AND ($4 IS NULL OR a.command = $4) \
       AND ($5 IS NULL OR a.at >= $5) \
       AND ($6 IS NULL OR a.at < $6) \
     ORDER BY a.id DESC LIMIT $7";

/// Led by the actor: `audit_actor_idx (actor_identity_id, id)`, which is both
/// the seek and the newest-first order inside it.
pub const BY_ACTOR: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE a.actor_identity_id = $1 AND a.id < $2 \
       AND ($3 IS NULL OR a.acting_as = $3) \
       AND ($4 IS NULL OR a.command = $4) \
       AND ($5 IS NULL OR a.at >= $5) \
       AND ($6 IS NULL OR a.at < $6) \
     ORDER BY a.id DESC LIMIT $7";

/// Led by the hat — the party a command was attributed to, which after
/// `ActAs` and under impersonation is not the actor.
pub const BY_HAT: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE a.acting_as = $1 AND a.id < $2 \
       AND ($3 IS NULL OR a.actor_identity_id = $3) \
       AND ($4 IS NULL OR a.command = $4) \
       AND ($5 IS NULL OR a.at >= $5) \
       AND ($6 IS NULL OR a.at < $6) \
     ORDER BY a.id DESC LIMIT $7";

/// Led by the command name: `audit_command_idx (command, id)`.
pub const BY_COMMAND: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE a.command = $1 AND a.id < $2 \
       AND ($3 IS NULL OR a.actor_identity_id = $3) \
       AND ($4 IS NULL OR a.acting_as = $4) \
       AND ($5 IS NULL OR a.at >= $5) \
       AND ($6 IS NULL OR a.at < $6) \
     ORDER BY a.id DESC LIMIT $7";

/// Led by the time range, on `audit_at_idx`. Both ends are always bound — an
/// open end is the beginning or the end of time — so it is one range on one
/// index rather than two optional halves the planner has to reason about.
pub const BY_TIME: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit a \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE a.at >= $1 AND a.at < $2 AND a.id < $3 \
       AND ($4 IS NULL OR a.actor_identity_id = $4) \
       AND ($5 IS NULL OR a.acting_as = $5) \
       AND ($6 IS NULL OR a.command = $6) \
     ORDER BY a.id DESC LIMIT $7";

/// Led by the object: everything one ruling was *about*.
///
/// The side table leads because it is the selective one — a party has a
/// handful of rulings and the log has the deployment's whole history — and
/// `audit` is then reached by its own primary key, one row at a time.
pub const BY_OBJECT: &str = "SELECT a.id, a.command, a.actor_identity_id, a.acting_as, a.at, \
     a.payload, p.display_name AS actor_display, \
     act.kind AS acting_kind, act.display_name AS acting_display \
     FROM audit_object o JOIN audit a ON a.id = o.audit_id \
     LEFT JOIN identity i ON i.id = a.actor_identity_id \
     LEFT JOIN party p ON p.id = i.person_id \
     LEFT JOIN party act ON act.id = a.acting_as \
     WHERE o.kind = $1 AND o.id = $2 AND a.id < $3 \
       AND ($4 IS NULL OR a.actor_identity_id = $4) \
       AND ($5 IS NULL OR a.acting_as = $5) \
       AND ($6 IS NULL OR a.command = $6) \
       AND ($7 IS NULL OR a.at >= $7) \
       AND ($8 IS NULL OR a.at < $8) \
     ORDER BY a.id DESC LIMIT $9";

/// Every control the audit screen draws, by the parameter name it sends.
///
/// The list is the *screen's*, restated here so the generated index test can
/// walk its powerset: sixty-four combinations, each planned and asserted to be
/// a seek. `crates/app-platform/src/audit.rs` draws exactly these six and
/// sends exactly these names.
pub const CONTROLS: &[&str] = &["actor", "hat", "object", "command", "from", "to"];

/// The end of time, for a range with no upper bound. `audit.at` is unix
/// seconds, so this is not a value any row can hold.
const FOREVER: i64 = i64::MAX;

/// What the controls asked for, once every public id has been decrypted.
///
/// A parameter that does not decode is *dropped* rather than carried as a
/// literal: a forged actor id would otherwise become a statement parameter
/// that matches nothing, and "no rows" and "that filter was nonsense" are two
/// different answers. Dropped, the query answers the question that was
/// actually asked, which is the unfiltered one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// The registration that ran the command.
    pub actor: Option<i64>,
    /// The party it was attributed to.
    pub hat: Option<i64>,
    /// A row it was about, as `audit_object` names it.
    pub object: Option<(&'static str, i64)>,
    /// The command's route name, checked against the vocabulary.
    pub command: Option<String>,
    /// The inclusive start of the range.
    pub from: Option<i64>,
    /// The exclusive end of it.
    pub to: Option<i64>,
}

impl Filter {
    /// Read the controls out of a request.
    #[must_use]
    pub fn of(key: &IdKey, params: &Params) -> Self {
        Self {
            actor: params
                .actor
                .as_deref()
                .and_then(|raw| decode_one::<Identity>(key, raw)),
            hat: params.hat.as_deref().and_then(|raw| party_row(key, raw)),
            object: params.object.as_deref().and_then(|raw| object_of(key, raw)),
            // Already checked against `ALL_COMMAND_NAMES` on the way in
            // (`Params::from_pairs`), so this is a name the contract declares.
            command: params.command.clone(),
            from: params.from,
            to: params.to,
        }
    }

    /// Whether any control is set at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.actor.is_none()
            && self.hat.is_none()
            && self.object.is_none()
            && self.command.is_none()
            && self.from.is_none()
            && self.to.is_none()
    }

    /// The statement this combination rides, and its parameters.
    ///
    /// One arm per lead, in order of selectivity. Everything not leading is
    /// bound as an optional predicate, in the order that statement declares.
    #[must_use]
    pub fn plan(&self, before: i64, limit: i64) -> (&'static str, Vec<Value>) {
        let actor = Value::from(self.actor);
        let hat = Value::from(self.hat);
        let command = Value::from(self.command.clone());
        let from = Value::from(self.from);
        let to = Value::from(self.to);
        match (self.object.as_ref(), self.actor, self.hat, &self.command) {
            (Some((kind, id)), ..) => (
                BY_OBJECT,
                vec![
                    Value::from(*kind),
                    Value::from(*id),
                    Value::from(before),
                    actor,
                    hat,
                    command,
                    from,
                    to,
                    Value::from(limit),
                ],
            ),
            (None, Some(actor_id), ..) => (
                BY_ACTOR,
                vec![
                    Value::from(actor_id),
                    Value::from(before),
                    hat,
                    command,
                    from,
                    to,
                    Value::from(limit),
                ],
            ),
            (None, None, Some(hat_id), _) => (
                BY_HAT,
                vec![
                    Value::from(hat_id),
                    Value::from(before),
                    actor,
                    command,
                    from,
                    to,
                    Value::from(limit),
                ],
            ),
            (None, None, None, Some(name)) => (
                BY_COMMAND,
                vec![
                    Value::from(name.clone()),
                    Value::from(before),
                    actor,
                    hat,
                    from,
                    to,
                    Value::from(limit),
                ],
            ),
            _ if self.from.is_some() || self.to.is_some() => (
                BY_TIME,
                vec![
                    Value::from(self.from.unwrap_or(0)),
                    Value::from(self.to.unwrap_or(FOREVER)),
                    Value::from(before),
                    actor,
                    hat,
                    command,
                    Value::from(limit),
                ],
            ),
            _ => (
                TAIL,
                vec![
                    Value::from(before),
                    actor,
                    hat,
                    command,
                    from,
                    to,
                    Value::from(limit),
                ],
            ),
        }
    }
}

/// One tag, or nothing.
fn decode_one<T: ids::Public>(key: &IdKey, raw: &str) -> Option<i64> {
    let id: PublicId = raw.parse().ok()?;
    ids::decode::<T>(key, &id).ok().map(|id| id.get())
}

/// A party rowid under whichever of the four tags it was minted with.
fn party_row(key: &IdKey, raw: &str) -> Option<i64> {
    decode_one::<Person>(key, raw)
        .or_else(|| decode_one::<Organization>(key, raw))
        .or_else(|| decode_one::<Group>(key, raw))
        .or_else(|| decode_one::<Service>(key, raw))
}

/// A public id, as `audit_object` names the row it points at.
///
/// The vocabulary is `crate::audit::object`'s, and the mapping is the one it
/// documents: a person, an organization and a group are all `party` rows, so
/// all three answer under that kind — which is right, because a caller asking
/// about a party is asking about the party and not about which of the four
/// kinds it happens to be.
#[must_use]
pub fn object_of(key: &IdKey, raw: &str) -> Option<(&'static str, i64)> {
    use rn_kernel::audit::object;
    party_row(key, raw)
        .map(|id| (object::PARTY, id))
        .or_else(|| decode_one::<Session>(key, raw).map(|id| (object::SESSION, id)))
        .or_else(|| decode_one::<Resource>(key, raw).map(|id| (object::RESOURCE, id)))
        .or_else(|| decode_one::<Link>(key, raw).map(|id| (object::LINK, id)))
        .or_else(|| decode_one::<Identity>(key, raw).map(|id| (object::IDENTITY, id)))
        .or_else(|| decode_one::<Factor>(key, raw).map(|id| (object::FACTOR, id)))
        .or_else(|| decode_one::<OidcClient>(key, raw).map(|id| (object::CLIENT, id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::ids::Id;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    fn params(pairs: &[(&str, &str)]) -> Params {
        Params::from_pairs(pairs.iter().copied())
    }

    #[test]
    fn no_filter_walks_the_primary_key_backwards() {
        let filter = Filter::default();
        assert!(filter.is_empty());
        let (sql, args) = filter.plan(i64::MAX, 100);
        assert_eq!(sql, TAIL);
        assert_eq!(args.len(), 7);
        assert_eq!(args[1], Value::Null, "an absent actor is bound as NULL");
    }

    #[test]
    fn the_lead_is_the_most_selective_control_that_is_set() {
        let key = key();
        let person = Id::<Person>::new(3).public(&key).as_str().to_owned();
        let identity = Id::<Identity>::new(4).public(&key).as_str().to_owned();

        let by_command = Filter::of(&key, &params(&[("command", "revoke-session")]));
        assert_eq!(by_command.plan(1, 1).0, BY_COMMAND);

        let by_hat = Filter::of(
            &key,
            &params(&[("hat", &person), ("command", "revoke-session")]),
        );
        assert_eq!(by_hat.plan(1, 1).0, BY_HAT, "a hat outranks a command");

        let by_actor = Filter::of(
            &key,
            &params(&[("actor", &identity), ("hat", &person), ("command", "share")]),
        );
        assert_eq!(by_actor.plan(1, 1).0, BY_ACTOR, "an actor outranks a hat");

        let by_object = Filter::of(
            &key,
            &params(&[
                ("object", &person),
                ("actor", &identity),
                ("command", "share"),
            ]),
        );
        assert_eq!(
            by_object.plan(1, 1).0,
            BY_OBJECT,
            "an object outranks everything"
        );

        let by_time = Filter::of(&key, &params(&[("from", "1800000000")]));
        let (sql, args) = by_time.plan(1, 1);
        assert_eq!(sql, BY_TIME);
        assert_eq!(
            args[1],
            Value::Int(FOREVER),
            "an open end is the end of time"
        );
    }

    #[test]
    fn a_forged_or_malformed_id_is_dropped_rather_than_bound() {
        let key = key();
        // Well-formed, minted under another deployment's key.
        let other = IdKey::from_hex("0f0e0d0c0b0a09080706050403020100").expect("a second key");
        let forged = Id::<Person>::new(3).public(&other).as_str().to_owned();
        let filter = Filter::of(&key, &params(&[("actor", &forged), ("hat", "p_junk")]));
        assert!(
            filter.is_empty(),
            "a filter nobody could have meant is no filter"
        );
        assert_eq!(filter.plan(1, 1).0, TAIL);

        // A command the contract does not declare is dropped on the way in.
        assert_eq!(
            Filter::of(&key, &params(&[("command", "obliterate")])),
            Filter::default()
        );
    }

    #[test]
    fn every_public_kind_the_side_table_carries_maps_to_its_own_word() {
        use rn_kernel::audit::object;
        let key = key();
        let of = |raw: String| object_of(&key, &raw);
        assert_eq!(
            of(Id::<Person>::new(1).public(&key).as_str().to_owned()),
            Some((object::PARTY, 1))
        );
        assert_eq!(
            of(Id::<Organization>::new(2).public(&key).as_str().to_owned()),
            Some((object::PARTY, 2)),
            "an organization is a party row and is asked about as one"
        );
        assert_eq!(
            of(Id::<Session>::new(3).public(&key).as_str().to_owned()),
            Some((object::SESSION, 3))
        );
        assert_eq!(
            of(Id::<Resource>::new(4).public(&key).as_str().to_owned()),
            Some((object::RESOURCE, 4))
        );
        assert_eq!(
            of(Id::<Link>::new(5).public(&key).as_str().to_owned()),
            Some((object::LINK, 5))
        );
        assert_eq!(
            of(Id::<Identity>::new(6).public(&key).as_str().to_owned()),
            Some((object::IDENTITY, 6))
        );
        assert_eq!(
            of(Id::<Factor>::new(7).public(&key).as_str().to_owned()),
            Some((object::FACTOR, 7))
        );
        assert_eq!(
            of(Id::<OidcClient>::new(8).public(&key).as_str().to_owned()),
            Some((object::CLIENT, 8))
        );
        assert_eq!(of("not-an-id".to_owned()), None);
    }
}
