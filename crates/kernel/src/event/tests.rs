//! What an event promises: one vocabulary with the command routes, one
//! direction out of a payload, and a projection that says whose cached
//! resolution a change invalidates.

use super::*;

/// One index per variant.
///
/// The point of it is the `match`: adding a variant to [`Event`] fails to
/// compile here until somebody gives it a number, and the test below then
/// fails until [`every_variant`] carries a sample of it. That is what keeps
/// "every variant names the command that produced it" true of *every* variant
/// rather than of the ones somebody remembered.
const fn index(event: &Event) -> usize {
    match event {
        Event::Registered { .. } => 0,
        Event::SignedIn { .. } => 1,
        Event::SignedOut { .. } => 2,
        Event::SessionRevoked { .. } => 3,
        Event::FactorAdded { .. } => 4,
        Event::FactorRemoved { .. } => 5,
        Event::EmailVerified { .. } => 6,
        Event::PartyDisabled { .. } => 7,
        Event::PartyEnabled { .. } => 8,
        Event::MatchProposed { .. } => 9,
        Event::MatchRejected { .. } => 10,
        Event::PersonMerged { .. } => 11,
        Event::IdentityLinked { .. } => 12,
        Event::PersonSplit { .. } => 13,
        Event::OrganizationCreated { .. } => 14,
        Event::GroupCreated { .. } => 15,
        Event::Invited { .. } => 16,
        Event::LinkClaimed { .. } => 17,
        Event::RoleSet { .. } => 18,
        Event::Left { .. } => 19,
        Event::Shared { .. } => 20,
        Event::Revoked { .. } => 21,
        Event::Transferred { .. } => 22,
        Event::DocumentCreated { .. } => 23,
        Event::DocumentEdited { .. } => 24,
        Event::DocumentPublished { .. } => 25,
    }
}

/// How many there are. Stated once, asserted against both lists.
const VARIANTS: usize = 26;

/// One of each, with ids that are obviously placeholders.
fn every_variant() -> Vec<Event> {
    vec![
        Event::Registered {
            identity: Id::new(1),
            person: Id::new(1),
            session: Id::new(1),
        },
        Event::SignedIn {
            identity: Id::new(1),
            session: Id::new(1),
        },
        Event::SignedOut {
            identity: Id::new(1),
            session: Id::new(1),
        },
        Event::SessionRevoked {
            identity: Id::new(1),
            session: Id::new(1),
        },
        Event::FactorAdded {
            identity: Id::new(1),
            factor: Id::new(1),
            kind: FactorKind::Email,
        },
        Event::FactorRemoved {
            identity: Id::new(1),
            factor: Id::new(1),
        },
        Event::EmailVerified {
            identity: Id::new(1),
            factor: Id::new(1),
        },
        Event::PartyDisabled { party: Id::new(1) },
        Event::PartyEnabled { party: Id::new(1) },
        Event::MatchProposed {
            candidate: Id::new(1),
        },
        Event::MatchRejected {
            candidate: Id::new(1),
        },
        Event::PersonMerged {
            survivor: Id::new(1),
            absorbed: Id::new(2),
            method: LinkMethod::SelfLink,
            candidate: Some(Id::new(1)),
        },
        Event::IdentityLinked {
            identity: Id::new(1),
            person: Id::new(1),
            method: LinkMethod::Operator,
        },
        Event::PersonSplit {
            identity: Id::new(1),
            person: Id::new(1),
        },
        Event::OrganizationCreated {
            organization: Id::new(1),
            resource: Id::new(1),
            owner: Id::new(1),
        },
        Event::GroupCreated {
            group: Id::new(1),
            resource: Id::new(1),
            owner: Id::new(1),
        },
        Event::Invited {
            container: Id::new(1),
            link: Id::new(1),
        },
        Event::LinkClaimed {
            container: Id::new(1),
            identity: Id::new(1),
            party: Id::new(1),
        },
        Event::RoleSet {
            container: Id::new(1),
            party: Id::new(1),
        },
        Event::Left {
            container: Id::new(1),
            party: Id::new(1),
        },
        Event::Shared {
            object_kind: "document".to_owned(),
            object_id: 1,
            relation: Relation::Viewer,
        },
        Event::Revoked {
            object_kind: "document".to_owned(),
            object_id: 1,
            relation: Relation::Viewer,
        },
        Event::Transferred {
            resource: Id::new(1),
            to: Id::new(1),
        },
        Event::DocumentCreated {
            document: Id::new(1),
            owner: Id::new(1),
        },
        Event::DocumentEdited {
            document: Id::new(1),
            rev: 1,
        },
        Event::DocumentPublished {
            document: Id::new(1),
            rev: 1,
        },
    ]
}

#[test]
fn a_payload_the_database_wrote_reads_back_as_the_event() {
    let payload = r#"{"event":"register","identity":3,"person":2,"session":9}"#;
    assert_eq!(
        Event::from_payload(payload),
        Some(Event::Registered {
            identity: Id::new(3),
            person: Id::new(2),
            session: Id::new(9),
        })
    );
}

#[test]
fn every_variant_names_the_command_that_produced_it() {
    let events = every_variant();
    assert_eq!(events.len(), VARIANTS);

    // Every sample is a different variant, and between them they are all of
    // them: `index` is exhaustive by compilation, so a full, duplicate-free
    // set of indices is a full set of variants.
    let mut seen: Vec<usize> = events.iter().map(index).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, (0..VARIANTS).collect::<Vec<_>>());

    for event in &events {
        let name = event.name();
        assert!(
            rn_api::commands::ALL_COMMAND_NAMES.contains(&name),
            "{name} is not a command route"
        );
    }
}

#[test]
fn a_payload_from_a_build_that_knew_more_is_skipped_rather_than_guessed() {
    assert_eq!(Event::from_payload(r#"{"event":"merge"}"#), None);
    assert_eq!(Event::from_payload(r#"{"event":"register"}"#), None);
    assert_eq!(Event::from_payload("not json"), None);
}

#[test]
fn an_event_says_whose_cached_resolution_it_invalidates() {
    let registered = Event::Registered {
        identity: Id::new(3),
        person: Id::new(2),
        session: Id::new(9),
    };
    assert_eq!(registered.touches_identity(), Some(Id::new(3)));
    assert_eq!(registered.touches_person(), Some(Id::new(2)));

    let disabled = Event::PartyDisabled { party: Id::new(2) };
    assert_eq!(disabled.touches_identity(), None);
    assert_eq!(disabled.touches_person(), Some(Id::new(2)));
}
