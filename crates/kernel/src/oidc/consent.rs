//! Consent: a relation row, plus the scopes it agreed to.
//!
//! "Has this person authorised this client" is `oidc_client:X #authorized
//! @person:Y`, which means it is answered by `check()` — the one indexed query
//! every other authorisation in the system goes through — rather than by a
//! second path with its own bugs. What a relation row cannot carry is *which
//! scopes*, so that is this table, read only after `check()` has said yes.
//!
//! Withdrawing a consent deletes the grant and revokes every token the client
//! holds for that person, in one transaction. A consent that could be
//! withdrawn while the tokens it produced kept working would be a button that
//! does nothing.

use rn_api::oidc::Scope;

use crate::Timestamp;
use crate::bind;
use crate::error::Outcome;
use crate::ids::{Id, OidcClient, Person};
use crate::store::{Cursor, FromRow, Reads, RowError};

/// One `oidc_consent` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentRow {
    /// Which client.
    pub client_id: Id<OidcClient>,
    /// Whose.
    pub person_id: Id<Person>,
    /// The `scope` string agreed to.
    pub scopes: String,
    /// When.
    pub at: Timestamp,
}

impl ConsentRow {
    /// The scopes, parsed. An unreadable one is empty rather than an error:
    /// the effect is that the person is asked again, which is the safe answer.
    #[must_use]
    pub fn granted(&self) -> Vec<Scope> {
        Scope::parse_list(&self.scopes).unwrap_or_default()
    }

    /// Whether everything asked for has already been agreed to.
    #[must_use]
    pub fn covers(&self, asked: &[Scope]) -> bool {
        let granted = self.granted();
        asked.iter().all(|scope| granted.contains(scope))
    }
}

impl FromRow for ConsentRow {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            client_id: row.id("client_id")?,
            person_id: row.id("person_id")?,
            scopes: row.text("scopes")?,
            at: row.int("at")?,
        })
    }
}

/// One consent — a seek on the primary key.
pub const BY_PAIR_SQL: &str = "SELECT client_id, person_id, scopes, at FROM oidc_consent \
     WHERE client_id = $1 AND person_id = $2";

/// Record one, widening what was agreed to rather than replacing it: a person
/// who has already agreed to `email` and now agrees to `profile` has agreed to
/// both, and the second grant must not quietly take the first away.
pub const UPSERT_SQL: &str = "INSERT INTO oidc_consent (client_id, person_id, scopes, at) \
     VALUES ($1, $2, $3, $4) \
     ON CONFLICT (client_id, person_id) DO UPDATE SET scopes = $3, at = $4";

/// Withdraw one.
pub const DELETE_SQL: &str = "DELETE FROM oidc_consent WHERE client_id = $1 AND person_id = $2";

/// Every client one person has authorised, for the page that lists them.
pub const BY_PERSON_SQL: &str = "SELECT c.id AS client_id, k.person_id, k.scopes, k.at, \
     c.client_name, c.client_uri \
     FROM oidc_consent k JOIN oidc_client c ON c.id = k.client_id \
     WHERE k.person_id = $1 AND c.deleted_at IS NULL ORDER BY c.client_name";

/// Read a consent.
pub async fn load(
    store: &impl Reads,
    client: Id<OidcClient>,
    person: Id<Person>,
) -> Outcome<Option<ConsentRow>> {
    Ok(store
        .query_opt::<ConsentRow>(BY_PAIR_SQL, bind![client, person])
        .await?)
}

/// The union of what was already agreed to and what is being agreed to now.
#[must_use]
pub fn widen(existing: Option<&ConsentRow>, asked: &[Scope]) -> Vec<Scope> {
    let mut all = existing.map(ConsentRow::granted).unwrap_or_default();
    for scope in asked {
        if !all.contains(scope) {
            all.push(*scope);
        }
    }
    all.sort_unstable();
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consent(scopes: &str) -> ConsentRow {
        ConsentRow {
            client_id: Id::new(1),
            person_id: Id::new(1),
            scopes: scopes.to_owned(),
            at: 0,
        }
    }

    #[test]
    fn a_consent_covers_only_what_it_says() {
        let agreed = consent("openid email");
        assert!(agreed.covers(&[Scope::Openid]));
        assert!(agreed.covers(&[Scope::Openid, Scope::Email]));
        assert!(!agreed.covers(&[Scope::Openid, Scope::Profile]));
        assert!(agreed.covers(&[]));
    }

    #[test]
    fn agreeing_again_widens_and_never_narrows() {
        let agreed = consent("openid email");
        assert_eq!(
            widen(Some(&agreed), &[Scope::Profile]),
            vec![Scope::Openid, Scope::Profile, Scope::Email]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );
        assert_eq!(widen(None, &[Scope::Openid]), vec![Scope::Openid]);
        assert_eq!(
            widen(Some(&agreed), &[]),
            vec![Scope::Openid, Scope::Email],
            "and agreeing to nothing takes nothing away"
        );
    }

    #[test]
    fn an_unreadable_scope_string_is_read_as_nothing_agreed_to() {
        assert!(consent("openid nonsense").granted().is_empty());
        assert!(!consent("openid nonsense").covers(&[Scope::Openid]));
    }
}
