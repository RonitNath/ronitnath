//! The tag vocabulary: one marker type per kind of public id.
//!
//! A tag is per *kind*, not per table, and the difference matters. `person`,
//! `organization`, `group` and `service` are all rows of `party`, and giving
//! them one tag would make `o_…` and `p_…` interchangeable — the exact
//! confusion the encryption is there to prevent. So each kind gets its own
//! constant and an organization id decrypted as a person id fails on the tag.
//!
//! The values are arbitrary and permanent: changing one renames every id of
//! that kind, which is a data migration and not a refactor.

use rn_api::ids::IdKind;

/// A row identifier's table: what [`Id`](super::Id) is generic over.
///
/// Implemented by every marker below, including the ones with no public form.
pub trait Table: Copy + 'static {
    /// The tag that goes in the plaintext's first eight bytes.
    const TAG: u64;
    /// The table's name, for `Debug` output.
    const NAME: &'static str;
}

/// A table whose rows are nameable from outside the process.
///
/// [`Id::public`](super::Id::public) and [`decode`](super::decode) are
/// available for exactly these; `audit` is `Table` but not `Public`, because
/// an audit row is addressed by its offset and nothing else.
///
/// `link` is both, and the two names are not the same thing. A link is
/// *claimed* by its bearer token, which is a secret and never an id; its
/// public id names the row, so that a page listing invitations can revoke one
/// without ever holding the token that would open it.
pub trait Public: Table {
    /// The wire vocabulary's kind, which fixes the prefix.
    const KIND: IdKind;
}

macro_rules! tables {
    ($(
        $(#[$meta:meta])*
        $name:ident = $tag:literal, $table:literal $(, $kind:expr)?;
    )+) => {$(
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name;

        impl Table for $name {
            const TAG: u64 = $tag;
            const NAME: &'static str = $table;
        }

        $(impl Public for $name {
            const KIND: IdKind = $kind;
        })?
    )+};
}

tables! {
    /// A human, resolved from one or more identities. A `party` row.
    Person = 0x9f2c_41d7_0000_0001, "party", IdKind::Person;
    /// A tenant and ownership boundary. A `party` row.
    Organization = 0x9f2c_41d7_0000_0002, "party", IdKind::Organization;
    /// A set of parties, granted but never owning. A `party` row.
    Group = 0x9f2c_41d7_0000_0003, "party", IdKind::Group;
    /// A machine principal. A `party` row.
    Service = 0x9f2c_41d7_0000_0004, "party", IdKind::Service;
    /// One source-scoped registration.
    Identity = 0x9f2c_41d7_0000_0005, "identity", IdKind::Identity;
    /// A factor proven for an identity.
    Factor = 0x9f2c_41d7_0000_0006, "factor", IdKind::Factor;
    /// A live session. Named publicly so a person can revoke a device they
    /// can see in a list; the token that *authenticates* it is separate.
    Session = 0x9f2c_41d7_0000_0007, "session", IdKind::Session;
    /// A row in the resource registry.
    Resource = 0x9f2c_41d7_0000_0008, "resource", IdKind::Resource;
    /// A pair of identities awaiting proof.
    MatchCandidate = 0x9f2c_41d7_0000_0009, "match_candidate", IdKind::MatchCandidate;
    /// A bearer link. Claimed by its token, named by this.
    Link = 0x9f2c_41d7_0000_000a, "link", IdKind::Link;
}
