//! What a relation is, what nests inside what, and which kinds admit which.
//!
//! Two separate things live here and it is worth keeping them apart.
//!
//! **Nesting** is a property of the relations themselves and is applied in
//! code, never in SQL: a row says `editor` and nothing else, and the question
//! "may this principal view it" is answered by [`Relation::covers`]. Putting
//! the ladder in the database would mean writing three rows where one is
//! meant, and a `Share` that forgot one would produce a grant that is stronger
//! or weaker than the word the caller used.
//!
//! **The vocabulary** is a property of a *kind*: which relations that kind
//! admits, and which subject kinds each of those relations accepts. It is the
//! seam the kernel report calls "products plug in" — a patient record admits
//! `viewer` for a person and an organization and refuses `@public` outright,
//! and it refuses it because the kind's own table says so rather than because
//! every command that writes one remembered to check.

use crate::domain::Vocabulary as SchemaVocabulary;

/// One relation. The whole vocabulary of this deployment, and the same list
/// the `relation_vocabulary_insert` trigger admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relation {
    /// May read.
    Viewer,
    /// May read and comment.
    Commenter,
    /// May read, comment and edit — and share.
    Editor,
    /// Owns. Covers every other relation on the same object.
    Owner,
    /// Belongs to a group or an organization.
    Member,
    /// Belongs and administers membership.
    Admin,
    /// The subject may see this person's contact details. Scoped to a group
    /// and to nothing else.
    Contact,
    /// Platform administration, held as `platform:* #operator @person`.
    Operator,
}

impl Relation {
    /// Every relation, in the order the schema trigger lists them.
    pub const ALL: &'static [Self] = &[
        Self::Viewer,
        Self::Commenter,
        Self::Editor,
        Self::Owner,
        Self::Member,
        Self::Admin,
        Self::Contact,
        Self::Operator,
    ];

    /// The word the row stores.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Commenter => "commenter",
            Self::Editor => "editor",
            Self::Owner => "owner",
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Contact => "contact",
            Self::Operator => "operator",
        }
    }

    /// Read a relation back, refusing anything the trigger would have.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| r.as_str() == raw)
    }

    /// Which ladder this relation sits on, and how high.
    ///
    /// `owner` is the top of both ladders, which is the whole of what "the
    /// owner may do anything to their own thing" means here.
    const fn rung(self) -> (Ladder, u8) {
        match self {
            Self::Viewer => (Ladder::Document, 0),
            Self::Commenter => (Ladder::Document, 1),
            Self::Editor => (Ladder::Document, 2),
            Self::Member => (Ladder::Membership, 0),
            Self::Admin => (Ladder::Membership, 1),
            Self::Owner => (Ladder::Both, 3),
            Self::Contact => (Ladder::Contact, 0),
            Self::Operator => (Ladder::Operator, 0),
        }
    }

    /// Whether holding `self` is enough to do what `wanted` allows.
    ///
    /// Reflexive, transitive and antisymmetric — asserted by a proptest, since
    /// a nesting relation that is not a partial order is a permission system
    /// with a cycle in it.
    pub const fn covers(self, wanted: Self) -> bool {
        let (mine, my_rung) = self.rung();
        let (theirs, their_rung) = wanted.rung();
        mine.same(theirs) && my_rung >= their_rung
    }
}

/// Which chain of relations a rung belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ladder {
    /// viewer < commenter < editor.
    Document,
    /// member < admin.
    Membership,
    /// The top of both.
    Both,
    /// contact, alone.
    Contact,
    /// operator, alone.
    Operator,
}

impl Ladder {
    const fn same(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Document, Self::Document)
                | (Self::Membership, Self::Membership)
                | (Self::Contact, Self::Contact)
                | (Self::Operator, Self::Operator)
                | (Self::Both, Self::Both | Self::Document | Self::Membership)
        )
    }
}

/// What sort of thing a relation row is granted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubjectKind {
    /// A human.
    Person,
    /// A tenant.
    Organization,
    /// A set of parties.
    Group,
    /// A machine principal.
    Service,
    /// One registration, rather than the human behind it.
    Identity,
    /// An invitation link: whoever holds the token.
    Link,
    /// Everyone, signed in or not.
    Public,
    /// Anyone signed in.
    Authenticated,
}

impl SubjectKind {
    /// Whether this subject is a party row, and so has an id that means
    /// something on its own.
    pub const fn is_party(self) -> bool {
        matches!(
            self,
            Self::Person | Self::Organization | Self::Group | Self::Service
        )
    }

    /// Whether this subject has no row to point at, and so carries id `0`.
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Public | Self::Authenticated)
    }
}

impl SchemaVocabulary for SubjectKind {
    const ALL: &'static [Self] = &[
        Self::Person,
        Self::Organization,
        Self::Group,
        Self::Service,
        Self::Identity,
        Self::Link,
        Self::Public,
        Self::Authenticated,
    ];
    const COLUMN: (&'static str, &'static str) = ("relation", "subject_kind");

    fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Group => "group",
            Self::Service => "service",
            Self::Identity => "identity",
            Self::Link => "link",
            Self::Public => "public",
            Self::Authenticated => "authenticated",
        }
    }
}

/// What one relation of one kind accepts.
#[derive(Debug, Clone, Copy)]
pub struct Admits {
    /// The relation.
    pub relation: Relation,
    /// The subject kinds it may be granted to.
    pub subjects: &'static [SubjectKind],
}

/// One registered object kind and everything it admits.
#[derive(Debug, Clone, Copy)]
pub struct Kind {
    /// The name that goes in `relation.object_kind`.
    pub name: &'static str,
    /// Whether this kind's objects are `resource` rows. The party-backed kinds
    /// address their `party` row instead, which is what memberships point at.
    pub is_resource: bool,
    /// Whether membership rows answer questions about this kind — true for the
    /// two container kinds and nothing else.
    pub is_container: bool,
    /// What may be granted on it.
    pub admits: &'static [Admits],
}

impl Kind {
    /// Whether this kind admits `relation` at all.
    pub fn admits_relation(&self, relation: Relation) -> bool {
        self.admits.iter().any(|a| a.relation == relation)
    }

    /// Whether this kind admits `relation` granted to a `subject`.
    pub fn admits_subject(&self, relation: Relation, subject: SubjectKind) -> bool {
        self.admits
            .iter()
            .any(|a| a.relation == relation && a.subjects.contains(&subject))
    }
}

/// A set of registered kinds: kind → admitted relations → admitted subjects.
///
/// [`Vocabulary::KERNEL`] is what the kernel's own commands enforce. A product
/// registers its kinds by building one of these — which is why the type is a
/// value rather than a global, and why the test that a `patient` kind refuses
/// `@public` can be written without adding a patient table to this crate.
#[derive(Debug, Clone, Copy)]
pub struct Vocabulary(&'static [Kind]);

impl Vocabulary {
    /// Build a vocabulary from a list of kinds.
    pub const fn new(kinds: &'static [Kind]) -> Self {
        Self(kinds)
    }

    /// The kinds this kernel ships with (`super::kinds`).
    pub const KERNEL: Self = Self::new(super::kinds::KERNEL_KINDS);

    /// The registered kinds.
    pub const fn kinds(self) -> &'static [Kind] {
        self.0
    }

    /// Look a kind up by name.
    pub fn kind(self, name: &str) -> Option<&'static Kind> {
        self.0.iter().find(|k| k.name == name)
    }

    /// Whether this vocabulary admits `object_kind #relation @subject`.
    ///
    /// An unregistered kind admits nothing. That is deliberate: a typo in a
    /// kind name refuses the grant rather than writing a row nothing will ever
    /// read.
    pub fn admits(self, object_kind: &str, relation: Relation, subject: SubjectKind) -> bool {
        self.kind(object_kind)
            .is_some_and(|kind| kind.admits_subject(relation, subject))
    }
}
