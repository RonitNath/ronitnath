//! The domain vocabularies and the schema must say the same thing.
//!
//! Each test reads the CHECK constraint out of `sqlite_master` and compares it
//! with the Rust enum. Adding a status to the enum without adding it to the
//! schema fails here, and so does the reverse — which is the failure that
//! would otherwise be found by a `500` in production.

use super::*;
use crate::bind;
use crate::store::{Cursor, FromRow, Reads, RowError};

#[derive(Debug)]
struct Ddl(String);

impl FromRow for Ddl {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("sql")?))
    }
}

async fn ddl(table: &str) -> String {
    let store = crate::store::tests::store();
    store
        .query_one::<Ddl>(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = $1",
            bind![table],
        )
        .await
        .expect("the table exists")
        .0
}

/// Pull the `IN (…)` list a column's CHECK constraint carries.
fn admitted(ddl: &str, column: &str) -> Vec<String> {
    let line = ddl
        .lines()
        .find(|line| line.trim_start().starts_with(column) && line.contains("CHECK"))
        .unwrap_or_else(|| panic!("no CHECK constraint on {column}"));
    let open = line.find("IN (").expect("a CHECK with an IN list") + 4;
    let close = line[open..].find(')').expect("a closed IN list") + open;
    line[open..close]
        .split(',')
        .map(|v| v.trim().trim_matches('\'').to_owned())
        .collect()
}

async fn agrees<V: Vocabulary>() {
    let (table, column) = V::COLUMN;
    let schema = admitted(&ddl(table).await, column);
    let code: Vec<String> = V::ALL.iter().map(|v| v.as_str().to_owned()).collect();
    // The enum, plus whatever the vocabulary declares the constraint admits
    // and no command writes. A surplus that is not declared is a failure; a
    // declared one is a frozen migration's debt, written down where this test
    // can read it.
    let mut accounted = code.clone();
    accounted.extend(V::ADMITTED_UNPRODUCED.iter().map(|v| (*v).to_owned()));
    accounted.sort();
    let mut listed = schema.clone();
    listed.sort();
    assert_eq!(
        listed, accounted,
        "{table}.{column}: the schema and the enum disagree"
    );
    for value in &code {
        assert!(V::parse(value).is_some());
    }
    // Unproduced means unreadable too: parsing one back would be reading a
    // value no command can have written.
    for value in V::ADMITTED_UNPRODUCED {
        assert!(
            V::parse(value).is_none(),
            "{table}.{column}: {value} is declared unproduced and still parses"
        );
        assert!(
            schema.iter().any(|listed| listed == value),
            "{table}.{column}: {value} is declared unproduced and the schema \
             does not admit it, so the declaration is stale"
        );
    }
    assert!(V::parse("something else").is_none());
}

#[tokio::test]
async fn party_kind_agrees_with_the_schema() {
    agrees::<PartyKind>().await;
}

#[tokio::test]
async fn party_status_agrees_with_the_schema() {
    agrees::<PartyStatus>().await;
}

#[tokio::test]
async fn identity_status_agrees_with_the_schema() {
    agrees::<IdentityStatus>().await;
}

#[tokio::test]
async fn factor_kind_agrees_with_the_schema() {
    agrees::<FactorKind>().await;
}

#[test]
fn only_two_factor_kinds_have_code_behind_them() {
    assert!(FactorKind::Email.is_built() && FactorKind::Password.is_built());
    assert!(!FactorKind::Passkey.is_built() && !FactorKind::Oidc.is_built());
}

#[test]
fn an_address_is_normalised_the_way_a_provider_treats_it() {
    assert_eq!(
        normalize_email("  Ronit@Isoastra.COM "),
        "ronit@isoastra.com"
    );
    assert_eq!(normalize_email("a@b.c"), "a@b.c");
}

#[test]
fn what_counts_as_an_address_is_shallow_and_says_so() {
    for good in ["a@b.co", "ronit@isoastra.com", "x.y+z@mail.example.org"] {
        assert!(looks_like_email(good), "{good}");
    }
    for bad in [
        "", "a", "a@b", "@b.co", "a@.co", "a@b.", "a b@c.de", "a@b@c.de",
    ] {
        assert!(!looks_like_email(bad), "{bad}");
    }
}

#[test]
fn a_session_expires_and_slides() {
    let session = SessionRow {
        id: crate::ids::Id::new(1),
        identity_id: crate::ids::Id::new(1),
        acting_as: crate::ids::Id::new(1),
        expires_at: 1_000 + SESSION_TTL,
        created_at: 1_000,
        last_seen_at: 1_000,
    };
    assert!(session.is_live(1_000));
    assert!(!session.is_live(1_000 + SESSION_TTL));
    assert!(!session.wants_renewal(1_000), "fresh, nothing to write");
    assert!(session.wants_renewal(1_000 + RENEW_AFTER + 1));
}

#[test]
fn a_link_is_claimed_once_and_then_never_again() {
    let mut link = LinkRow {
        id: crate::ids::Id::new(1),
        expires_at: 2_000,
        claimed_by_identity_id: None,
        verifies_factor_id: Some(crate::ids::Id::new(7)),
    };
    assert!(link.is_claimable(1_000));
    assert!(!link.is_claimable(2_000), "expired");
    link.claimed_by_identity_id = Some(crate::ids::Id::new(3));
    assert!(!link.is_claimable(1_000), "claimed");
}
