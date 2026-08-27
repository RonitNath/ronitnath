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
        Event::ActingAs { .. } => 4,
        Event::FactorAdded { .. } => 5,
        Event::FactorRemoved { .. } => 6,
        Event::EmailVerified { .. } => 7,
        Event::PartyDisabled { .. } => 8,
        Event::PartyEnabled { .. } => 9,
        Event::MatchProposed { .. } => 10,
        Event::MatchRejected { .. } => 11,
        Event::PersonMerged { .. } => 12,
        Event::IdentityLinked { .. } => 13,
        Event::PersonSplit { .. } => 14,
        Event::OrganizationCreated { .. } => 15,
        Event::GroupCreated { .. } => 16,
        Event::Invited { .. } => 17,
        Event::LinkRevoked { .. } => 18,
        Event::LinkClaimed { .. } => 19,
        Event::RoleSet { .. } => 20,
        Event::MemberRemoved { .. } => 21,
        Event::Left { .. } => 22,
        Event::Shared { .. } => 23,
        Event::Revoked { .. } => 24,
        Event::Transferred { .. } => 25,
        Event::DocumentCreated { .. } => 26,
        Event::DocumentEdited { .. } => 27,
        Event::DocumentPublished { .. } => 28,
        Event::HandleSet { .. } => 29,
        Event::ClientRegistered { .. } => 30,
        Event::ClientUpdated { .. } => 31,
        Event::ClientSecretRotated { .. } => 32,
        Event::ClientDeleted { .. } => 33,
        Event::SigningKeyRotated { .. } => 34,
        Event::Authorized { .. } => 35,
        Event::CodeExchanged { .. } => 36,
        Event::TokenRefreshed { .. } => 37,
        Event::ServiceTokenIssued { .. } => 38,
        Event::TokenRevoked { .. } => 39,
        Event::ConsentRevoked { .. } => 40,
        Event::SessionEnded { .. } => 41,
        Event::OperatorGranted { .. } => 42,
        Event::OperatorRevoked { .. } => 43,
        Event::ReAuthenticated { .. } => 44,
        Event::Impersonated { .. } => 45,
        Event::ImpersonationEnded { .. } => 46,
        Event::SigningKeyRetired { .. } => 47,
    }
}

/// How many there are. Stated once, asserted against both lists.
const VARIANTS: usize = 48;

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
            clients: vec![Id::new(1)],
        },
        Event::SessionRevoked {
            identity: Id::new(1),
            session: Id::new(1),
            clients: Vec::new(),
        },
        Event::ActingAs {
            identity: Id::new(1),
            session: Id::new(1),
            party: Id::new(1),
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
        Event::PartyDisabled {
            party: Id::new(1),
            clients: Vec::new(),
        },
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
        Event::LinkRevoked {
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
        Event::MemberRemoved {
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
        Event::HandleSet { person: Id::new(1) },
        Event::ClientRegistered {
            client: Id::new(1),
            owner: Some(Id::new(1)),
        },
        Event::ClientUpdated { client: Id::new(1) },
        Event::ClientSecretRotated { client: Id::new(1) },
        Event::ClientDeleted { client: Id::new(1) },
        Event::SigningKeyRotated {
            kid: "a-kid".to_owned(),
        },
        Event::Authorized {
            client: Id::new(1),
            person: Id::new(1),
        },
        Event::CodeExchanged { client: Id::new(1) },
        Event::TokenRefreshed { client: Id::new(1) },
        Event::ServiceTokenIssued {
            client: Id::new(1),
            service: Id::new(1),
        },
        Event::TokenRevoked { client: Id::new(1) },
        Event::ConsentRevoked {
            client: Id::new(1),
            person: Id::new(1),
        },
        Event::SessionEnded {
            identity: Id::new(1),
            session: Id::new(1),
            clients: vec![Id::new(2), Id::new(3)],
        },
        Event::OperatorGranted { person: Id::new(1) },
        Event::OperatorRevoked { person: Id::new(1) },
        Event::ReAuthenticated {
            identity: Id::new(1),
            session: Id::new(1),
        },
        Event::Impersonated {
            operator: Id::new(1),
            person: Id::new(2),
            session: Id::new(3),
        },
        Event::ImpersonationEnded {
            operator: Id::new(1),
            person: Id::new(2),
            session: Id::new(3),
        },
        Event::SigningKeyRetired {
            kid: "a-kid".to_owned(),
            forced: true,
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

    let disabled = Event::PartyDisabled {
        party: Id::new(2),
        clients: Vec::new(),
    };
    assert_eq!(disabled.touches_identity(), None);
    assert_eq!(disabled.touches_person(), Some(Id::new(2)));
}
