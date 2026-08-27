//! A person's handle: their `preferred_username`, and the only human-chosen
//! name in the model.
//!
//! It is not an id. Ids are derived, encrypted and unguessable
//! ([`crate::ids`]); a handle is short, memorable, typed by the person it
//! belongs to, and changeable — which is exactly why it can never be what
//! anything addresses a row by. What it *is* is the name other sites will call
//! somebody, so it is unique deployment-wide and it never stops resolving: a
//! merge keeps the survivor's and files the absorbed one in
//! `party_handle_alias`, because a handle that stopped resolving is a URL that
//! used to name a person.
//!
//! The rule is one sentence: 3–32 characters of lowercase `a-z`, `0-9` and
//! `-`, not starting or ending with `-`.

use crate::error::Outcome;
use crate::ids::{Id, Person};
use crate::store::{Count, Reads};
use crate::{Invalid, bind};

/// Shortest a handle may be.
pub const MIN: usize = 3;
/// Longest a handle may be.
pub const MAX: usize = 32;

/// Read a handle a caller sent, refusing anything the rule does not admit.
///
/// It trims and lowercases first, because "Ronit " and "ronit" are the same
/// intention and refusing the first teaches nobody anything.
pub fn validate(raw: &str) -> Result<String, Invalid> {
    let handle = raw.trim().to_lowercase();
    if handle.is_empty() {
        return Err(Invalid::Missing("handle"));
    }
    if handle.chars().count() < MIN {
        return Err(Invalid::BadHandle);
    }
    if handle.chars().count() > MAX {
        return Err(Invalid::TooLong {
            field: "handle",
            limit: MAX,
        });
    }
    let shaped = handle
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !handle.starts_with('-')
        && !handle.ends_with('-');
    if shaped {
        Ok(handle)
    } else {
        Err(Invalid::BadHandle)
    }
}

/// A handle to prefill a registration form with, from an address's local part.
///
/// A suggestion and nothing more: it is put in the field, the person may
/// change it, and it is validated like anything else they type. A local part
/// two people share is a collision one of them is told about at the form,
/// which is the right place to find out.
#[must_use]
pub fn suggest(email: &str) -> String {
    shape(email.split('@').next().unwrap_or(email))
}

/// A handle derived from a *whole* address, for a fixture and for the dev
/// operator: two accounts at two domains that share a local part want two
/// handles, and a test that had to invent them would be a test about handles.
#[must_use]
pub fn fixture(email: &str) -> String {
    shape(email)
}

fn shape(email: &str) -> String {
    let mut out = String::with_capacity(MAX);
    let mut last_dash = true;
    for c in email.trim().to_lowercase().chars() {
        let mapped = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if mapped == '-' && last_dash {
            continue;
        }
        last_dash = mapped == '-';
        out.push(mapped);
        if out.chars().count() == MAX {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    while out.chars().count() < MIN {
        out.push('0');
    }
    out
}

/// Whether a handle is taken — by a live person or by a merge that absorbed
/// one. Both, because a handle that was somebody's is not free to be somebody
/// else's: the old one still resolves.
pub const TAKEN_SQL: &str = "SELECT (SELECT count(*) FROM party WHERE handle = $1) \
     + (SELECT count(*) FROM party_handle_alias WHERE handle = $1) AS n";

/// Claim a handle for a person. Guarded on the handle still being free, so two
/// registrations racing for one write one row rather than both.
/// A person's own alias does not block them: a handle they gave up is one
/// they may take back, which is the other half of never freeing it.
pub const SET_SQL: &str = "UPDATE party SET handle = $2 WHERE id = $1 \
     AND NOT EXISTS (SELECT 1 FROM party WHERE handle = $2 AND id <> $1) \
     AND NOT EXISTS (SELECT 1 FROM party_handle_alias WHERE handle = $2 AND person_id <> $1)";

/// The handle a person answers to, resolving an absorbed one to the survivor.
pub const RESOLVE_SQL: &str = "SELECT handle FROM party WHERE id = $1";

/// Whether anybody holds this handle.
pub async fn taken(store: &impl Reads, handle: &str) -> Outcome<bool> {
    Ok(store
        .query::<Count>(TAKEN_SQL, bind![handle])
        .await?
        .first()
        .is_some_and(|count| count.0 > 0))
}

/// One person's handle, if they have chosen one.
pub async fn of_person(store: &impl Reads, person: Id<Person>) -> Outcome<Option<String>> {
    struct Handle(Option<String>);
    impl crate::store::FromRow for Handle {
        fn from_row(row: &mut impl crate::store::Cursor) -> Result<Self, crate::store::RowError> {
            Ok(Self(row.text_opt("handle")?))
        }
    }
    Ok(store
        .query_opt::<Handle>(RESOLVE_SQL, bind![person])
        .await?
        .and_then(|row| row.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_is_lowercase_letters_digits_and_inner_dashes() {
        assert_eq!(validate("ronit"), Ok("ronit".to_owned()));
        assert_eq!(validate("  Ronit-Nath "), Ok("ronit-nath".to_owned()));
        assert_eq!(validate("a1-b2-c3"), Ok("a1-b2-c3".to_owned()));
        assert_eq!(validate(""), Err(Invalid::Missing("handle")));
        assert_eq!(validate("ab"), Err(Invalid::BadHandle));
        assert_eq!(validate("-ronit"), Err(Invalid::BadHandle));
        assert_eq!(validate("ronit-"), Err(Invalid::BadHandle));
        assert_eq!(validate("ronit nath"), Err(Invalid::BadHandle));
        assert_eq!(validate("ronit_nath"), Err(Invalid::BadHandle));
        assert_eq!(validate("ronit@isoastra"), Err(Invalid::BadHandle));
        assert_eq!(
            validate(&"a".repeat(MAX + 1)),
            Err(Invalid::TooLong {
                field: "handle",
                limit: MAX
            })
        );
    }

    #[test]
    fn a_suggestion_is_always_something_the_rule_admits() {
        for email in [
            "ronit@isoastra.com",
            "a@b.test",
            "Ronit.Nath+oidc@Example.TEST",
            "----@----",
            &format!("{}@example.test", "x".repeat(60)),
        ] {
            for suggested in [suggest(email), fixture(email)] {
                assert_eq!(
                    validate(&suggested),
                    Ok(suggested.clone()),
                    "{email} suggested {suggested}"
                );
            }
        }
        assert_eq!(suggest("ronit@isoastra.com"), "ronit");
        assert_eq!(suggest("Ronit.Nath+oidc@Example.TEST"), "ronit-nath-oidc");
        assert_eq!(fixture("ronit@isoastra.com"), "ronit-isoastra-com");
        assert_ne!(
            fixture("ronit@a.test"),
            fixture("ronit@b.test"),
            "one local part at two domains is two fixtures"
        );
    }
}
