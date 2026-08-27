//! What an identity is called when no person names it.
//!
//! The rule, in one place because it is one rule: a resolved identity is
//! named by its person, and an unresolved one by its address *masked*. The
//! source is never a label — it names the deployment or the issuer a
//! registration came from, so it says the same word to everybody.

use rn_kernel::Outcome;
use rn_kernel::bind;
use rn_kernel::ids::{Id, Identity};
use rn_kernel::store::{Cursor, FromRow, Reads, RowError};

/// The address an unresolved identity is masked from. One row: the email
/// factor is unique deployment-wide, and the oldest is the one it registered
/// with.
const EMAIL: &str = "SELECT value FROM factor \
                     WHERE identity_id = $1 AND kind = 'email' ORDER BY id LIMIT 1";

/// One column of one row: the address, so it can be masked and thrown away.
struct Address(String);

impl FromRow for Address {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.text("value")?))
    }
}

pub(super) async fn address_of(
    reads: &impl Reads,
    identity: Id<Identity>,
) -> Outcome<Option<String>> {
    Ok(reads
        .query_opt::<Address>(EMAIL, bind![identity])
        .await?
        .map(|row| row.0))
}

/// What an identity with no person is called.
///
/// The first and last characters of the local part and the `@` that says it is
/// an address at all: enough for somebody to recognise their own registration
/// in a list, and not enough for a reader of that list to write to it. A local
/// part of one or two characters reveals nothing, because there the first and
/// the last *are* the address; an identity with no address at all is not
/// described as having one.
pub(super) fn masked(address: Option<&str>) -> String {
    let Some(local) = address.map(|address| address.split('@').next().unwrap_or_default()) else {
        return "…".to_owned();
    };
    let mut chars = local.chars();
    match (chars.next(), chars.next_back(), local.chars().count()) {
        (Some(first), Some(last), count) if count >= 3 => format!("{first}…{last}@"),
        _ => "…@".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unresolved_identity_is_named_by_a_mask_and_never_by_an_address() {
        assert_eq!(masked(Some("ronit@isoastra.com")), "r…t@");
        assert_eq!(masked(Some("a.very.long.local.part@example.test")), "a…t@");
        // Two characters are their own first and last, so nothing is shown.
        assert_eq!(masked(Some("ab@example.test")), "…@");
        assert_eq!(masked(Some("r@example.test")), "…@");
        assert_eq!(masked(Some("@example.test")), "…@");
        // No address at all is not described as one.
        assert_eq!(masked(None), "…");

        for address in ["ronit@isoastra.com", "ab@example.test"] {
            let shown = masked(Some(address));
            let domain = address.split_once('@').expect("an address").1;
            assert!(!shown.contains(domain), "{shown} carried the domain");
            assert!(
                shown.matches('@').count() == 1 && shown.ends_with('@'),
                "{shown} is not a masked local part"
            );
        }
    }
}
