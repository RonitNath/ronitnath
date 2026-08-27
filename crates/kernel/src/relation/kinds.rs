//! The kinds this kernel registers, and what each of them admits.
//!
//! This is the worked example of the report's "products plug in": a product
//! adds its kinds by building a [`Vocabulary`](super::Vocabulary) of its own,
//! in exactly this shape, and gets persons, organizations, groups,
//! invitations, sharing and `check()` without adding a rule to any of them.
//! A kind that refuses `@public` refuses it because its row here says so.

use super::vocabulary::{Admits, Kind, Relation, SubjectKind};

/// Every party kind, for the relations that accept "any party".
const PARTIES: &[SubjectKind] = &[
    SubjectKind::Person,
    SubjectKind::Organization,
    SubjectKind::Group,
];

/// A person, an organization or a group, plus an invitation link and the two
/// open subjects — what a shareable document accepts.
const SHAREABLE: &[SubjectKind] = &[
    SubjectKind::Person,
    SubjectKind::Organization,
    SubjectKind::Group,
    SubjectKind::Link,
    SubjectKind::Public,
    SubjectKind::Authenticated,
];

/// Who may be named as the owner of a resource: a person or an organization.
/// A group is absent here for the same reason the schema has a trigger about
/// it — groups are granted, never owners.
const OWNERS: &[SubjectKind] = &[SubjectKind::Person, SubjectKind::Organization];

/// What a container (an organization or a group) admits: roles for parties,
/// and an invitation link holding the role it was minted for.
const CONTAINER: &[Admits] = &[
    Admits {
        relation: Relation::Member,
        subjects: &[
            SubjectKind::Person,
            SubjectKind::Organization,
            SubjectKind::Group,
            SubjectKind::Link,
        ],
    },
    Admits {
        relation: Relation::Admin,
        subjects: &[
            SubjectKind::Person,
            SubjectKind::Organization,
            SubjectKind::Group,
            SubjectKind::Link,
        ],
    },
    Admits {
        relation: Relation::Owner,
        subjects: OWNERS,
    },
];

pub(super) const KERNEL_KINDS: &[Kind] = &[
    Kind {
        name: "document",
        is_resource: true,
        is_container: false,
        admits: &[
            Admits {
                relation: Relation::Viewer,
                subjects: SHAREABLE,
            },
            Admits {
                relation: Relation::Commenter,
                subjects: SHAREABLE,
            },
            Admits {
                relation: Relation::Editor,
                subjects: &[
                    SubjectKind::Person,
                    SubjectKind::Organization,
                    SubjectKind::Group,
                ],
            },
            Admits {
                relation: Relation::Owner,
                subjects: OWNERS,
            },
        ],
    },
    Kind {
        name: "organization",
        is_resource: false,
        is_container: true,
        admits: CONTAINER,
    },
    Kind {
        name: "group",
        is_resource: false,
        is_container: true,
        admits: CONTAINER,
    },
    Kind {
        name: "person",
        is_resource: false,
        is_container: false,
        admits: &[Admits {
            // A person's contact details are visible inside a group and
            // nowhere else, so the only subject this accepts is a group.
            relation: Relation::Contact,
            subjects: &[SubjectKind::Group],
        }],
    },
    Kind {
        name: "oidc_client",
        is_resource: false,
        is_container: false,
        admits: &[Admits {
            // Consent. Only a person may hold it: a client is authorised by
            // the human whose claims it will read, never by a group they
            // happen to belong to.
            relation: Relation::Authorized,
            subjects: &[SubjectKind::Person],
        }],
    },
    Kind {
        name: "platform",
        is_resource: false,
        is_container: false,
        admits: &[Admits {
            relation: Relation::Operator,
            subjects: &[SubjectKind::Person],
        }],
    },
];

/// The party kinds a relation may be granted to, for callers building their
/// own vocabulary.
pub const PARTY_SUBJECTS: &[SubjectKind] = PARTIES;
