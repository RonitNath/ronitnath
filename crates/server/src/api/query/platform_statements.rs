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
        (
            "audit.tail",
            super::platform_audit::TAIL,
            bind![i64::MAX, page],
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
    ]
}
