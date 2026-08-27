//! Every statement the platform queries run, in one list.
//!
//! The "no full scan" gate lives in `tests/platform.rs`, in another crate's
//! directory, and it can only assert a plan it can reach. Listing the
//! statements here rather than exporting each module's constants one by one
//! keeps that list complete by construction: a statement added to a platform
//! query and left out of this file is a statement the gate never sees, and
//! `every_platform_statement_is_planned` counts them.

use rn_kernel::bind;
use rn_kernel::store::Value;

use super::platform::PAGE;
use super::platform_filters::Filter;

/// Every statement, named, with parameters shaped like the ones a request
/// supplies.
#[must_use]
pub fn statements() -> Vec<(&'static str, &'static str, Vec<Value>)> {
    let page = PAGE as i64;
    vec![
        (
            "parties.list",
            super::platform_parties::PARTIES,
            bind![page],
        ),
        ("parties.one", super::platform_parties::ONE, bind![1i64]),
        (
            "parties.identities",
            super::platform_parties::IDENTITIES,
            bind![1i64],
        ),
        (
            "parties.memberships",
            super::platform_parties::MEMBERSHIPS,
            bind![1i64],
        ),
        (
            "parties.owned",
            super::platform_parties::OWNED,
            bind![1i64, page],
        ),
        (
            "parties.as_subject",
            super::platform_parties::AS_SUBJECT,
            bind!["person:1"],
        ),
        (
            "identities.list",
            super::platform_identities::IDENTITIES,
            bind![page],
        ),
        (
            "identities.one",
            super::platform_identities::ONE,
            bind![1i64],
        ),
        (
            "identities.factors_of_page",
            super::platform_identities::FACTORS_OF_PAGE,
            bind![page],
        ),
        (
            "identities.factors_of_one",
            super::platform_identities::FACTORS_OF_ONE,
            bind![1i64],
        ),
        (
            "identities.sessions",
            super::platform_identities::SESSIONS,
            bind![1i64],
        ),
        (
            "identities.candidates",
            super::platform_identities::CANDIDATES,
            bind![1i64],
        ),
        (
            "sessions.list",
            super::platform_sessions::SESSIONS,
            bind![page],
        ),
        ("matches.list", super::platform_matches::QUEUE, bind![page]),
        ("matches.one", super::platform_matches::ONE, bind![1i64]),
        (
            "matches.person_of",
            super::platform_matches::PERSON_OF,
            bind![1i64],
        ),
        (
            "matches.links",
            super::platform_matches::LINKS,
            bind![1i64, 2i64],
        ),
        (
            "matches.alias",
            super::platform_matches::ALIAS,
            bind![1i64, 2i64],
        ),
        (
            "resources.list",
            super::platform_resources::RESOURCES,
            bind![page],
        ),
        ("resources.one", super::platform_resources::ONE, bind![1i64]),
        (
            "resources.party_of",
            super::platform_resources::PARTY_OF,
            bind![1i64],
        ),
        (
            "resources.as_object",
            super::platform_resources::AS_OBJECT,
            bind!["document", 1i64],
        ),
        ("nodes.list", super::platform_nodes::NODES, bind![]),
        ("feed.head", super::platform_nodes::HEAD, bind![]),
        ("feed.today", super::platform_nodes::TODAY, bind![1i64]),
        ("keys.list", super::platform_keys::KEYS, bind![page]),
        (
            "keys.alive",
            rn_kernel::cmd::RETIRE_ALIVE_SQL,
            bind![1i64, 1_800_000_000_i64],
        ),
        (
            "clients.list",
            super::platform_keys::CLIENTS,
            bind![1_800_000_000_i64, page],
        ),
        ("links.list", super::platform_links::LINKS, bind![page]),
        (
            "consents.list",
            super::platform_links::CONSENTS,
            bind![page],
        ),
        (
            "consents.of_client",
            super::platform_links::CONSENTS_OF_CLIENT,
            bind![1i64, page],
        ),
        (
            "cascade.sessions",
            super::platform_links::CASCADE_SESSIONS,
            bind![1i64],
        ),
        (
            "cascade.tokens",
            super::platform_links::CASCADE_TOKENS,
            bind![1i64, 1_800_000_000_i64],
        ),
        (
            "cascade.links",
            super::platform_links::CASCADE_LINKS,
            bind![1i64],
        ),
        (
            "cascade.consents",
            super::platform_links::CASCADE_CONSENTS,
            bind![1i64],
        ),
        (
            "person.factors",
            super::platform_person::FACTORS,
            bind![1i64],
        ),
        (
            "person.sessions",
            super::platform_person::SESSIONS,
            bind![1i64],
        ),
        (
            "person.consents",
            super::platform_person::CONSENTS,
            bind![1i64],
        ),
        ("person.merges", super::platform_person::MERGES, bind![1i64]),
        (
            "person.aliases",
            super::platform_person::ALIASES,
            bind![1i64],
        ),
        (
            "person.handle_aliases",
            super::platform_person::HANDLE_ALIASES,
            bind![1i64],
        ),
        ("find.handle", super::platform_find::BY_HANDLE, bind!["bea"]),
        (
            "find.email",
            super::platform_find::BY_EMAIL,
            bind!["bea@example.test"],
        ),
        ("find.id", super::platform_find::BY_ID, bind![1i64]),
        (
            "find.person_of_identity",
            super::platform_find::PERSON_OF_IDENTITY,
            bind![1i64],
        ),
    ]
    .into_iter()
    .chain(audit())
    .collect()
}

/// The audit's six statements, built from the filter that chooses each one.
///
/// Named here rather than written out, so the gate plans exactly the SQL a
/// request runs: a statement listed by hand could drift from the one
/// [`Filter::plan`] hands the store, and the drift would be invisible until it
/// was a scan on somebody's deployment.
fn audit() -> Vec<(&'static str, &'static str, Vec<Value>)> {
    let page = PAGE as i64;
    let led = |filter: Filter| filter.plan(i64::MAX, page);
    let mut all = Vec::new();
    for (name, filter) in [
        ("audit.tail", Filter::default()),
        (
            "audit.by_actor",
            Filter {
                actor: Some(1),
                ..Filter::default()
            },
        ),
        (
            "audit.by_hat",
            Filter {
                hat: Some(1),
                ..Filter::default()
            },
        ),
        (
            "audit.by_command",
            Filter {
                command: Some("share".to_owned()),
                ..Filter::default()
            },
        ),
        (
            "audit.by_time",
            Filter {
                from: Some(1_800_000_000),
                ..Filter::default()
            },
        ),
        (
            "audit.by_object",
            Filter {
                object: Some((rn_kernel::audit::object::PARTY, 1)),
                ..Filter::default()
            },
        ),
    ] {
        let (sql, args) = led(filter);
        all.push((name, sql, args));
    }
    all
}
