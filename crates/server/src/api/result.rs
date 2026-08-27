//! Every event, as the public ids the caller is allowed to read back.
//!
//! One arm per [`Event`] variant, exhaustively: a kernel that grows an event
//! and a server that forgets to name it would answer `null` to a caller that
//! needs the id it just created, so the match has no wildcard and a new
//! variant is a compile error here rather than a silent hole.
//!
//! Three rules hold across all of them.
//!
//! * **Public ids only.** Every identifier is derived through the server's id
//!   key ([`Id::public`]), so nothing on this wire is a rowid.
//! * **A secret is never a name.** A link has a public id and it is not its
//!   token: `Invited` reports the row so a page can revoke it, and the token
//!   rides beside the result, once, on the call that minted it.
//! * **No secrets, no addresses.** A factor's value, a session's token and an
//!   operator's evidence never appear; the id of the row that carries them
//!   does.
//!
//! One id cannot be derived here at all. Several events type a party field as
//! `Id<Person>` because `party` is the *table*, not the kind — a transferred
//! resource may go to an organization, a group's owner usually is one — and
//! the four kinds share no tag, so rendering those under the person tag would
//! hand a caller back an id that is not the one it sent. [`ambiguous_party`]
//! names that row and the caller resolves it against `party.kind` (see
//! [`super::party`]) before calling in, which keeps this function pure and its
//! tests free of a database.

use rn_api::PublicId;
use rn_kernel::Event;
use rn_kernel::domain::Vocabulary as _;
use rn_kernel::ids::{Group, Id, IdKey, Organization, Person, Resource, Service};
use serde_json::{Value, json};

/// The `party` row an event names whose kind the event does not fix.
///
/// At most one per event, and only these five: everywhere else the command's
/// own semantics settle it — a merge is between persons, a claimant is the
/// person who claimed, an organization's founder is a person.
#[must_use]
pub const fn ambiguous_party(event: &Event) -> Option<i64> {
    match event {
        // A group in an organization is owned by the organization.
        Event::GroupCreated { owner, .. }
        // A document may be owned by an organization.
        | Event::DocumentCreated { owner, .. } => Some(owner.get()),
        // The whole point of `Transfer` is that the new owner may be either.
        Event::Transferred { to, .. } => Some(to.get()),
        // A member may be a person, an organization or a group.
        Event::RoleSet { party, .. } => Some(party.get()),
        // Any party may be disabled, and the four kinds share no tag.
        Event::PartyDisabled { party, .. } | Event::PartyEnabled { party } => Some(party.get()),
        // A session may speak as a person or as an organization.
        Event::ActingAs { party, .. } => Some(party.get()),
        _ => None,
    }
}

/// The public ids a command produced, derived on the way out.
///
/// `party` is [`ambiguous_party`] already resolved, or `None` when this event
/// names no such row — and `None` where one was named means the row could not
/// be read, which renders as `null` rather than as an id under a guessed tag.
pub fn result_of(event: &Event, key: &IdKey, party: Option<PublicId>) -> Value {
    let ambiguous = || party.as_ref().map_or(Value::Null, |id| json!(id));
    match event {
        // --- identity -----------------------------------------------------
        Event::Registered {
            identity, person, ..
        } => json!({ "identity": identity.public(key), "person": person.public(key) }),
        Event::SignedIn { identity, .. } => json!({ "identity": identity.public(key) }),
        Event::SignedOut { session, .. }
        | Event::SessionRevoked { session, .. }
        | Event::SessionEnded { session, .. } => {
            json!({ "session": session.public(key) })
        }
        Event::FactorAdded { factor, .. }
        | Event::FactorRemoved { factor, .. }
        | Event::EmailVerified { factor, .. } => json!({ "factor": factor.public(key) }),
        // The party a session speaks as is any of the four kinds, so the
        // caller reads back the id it sent rather than the person's.
        Event::ActingAs { session, .. } => json!({
            "session": session.public(key),
            "party": ambiguous(),
        }),
        Event::PartyDisabled { .. } | Event::PartyEnabled { .. } => {
            json!({ "party": ambiguous() })
        }

        // --- merge --------------------------------------------------------
        Event::MatchProposed { candidate } | Event::MatchRejected { candidate } => {
            json!({ "candidate": candidate.public(key) })
        }
        Event::PersonMerged {
            survivor,
            absorbed,
            method,
            candidate,
        } => json!({
            "survivor": survivor.public(key),
            "absorbed": absorbed.public(key),
            "method": method.as_str(),
            "candidate": candidate.map(|candidate| candidate.public(key)),
        }),
        Event::IdentityLinked {
            identity,
            person,
            method,
        } => json!({
            "identity": identity.public(key),
            "person": person.public(key),
            "method": method.as_str(),
        }),
        Event::PersonSplit { identity, person } => json!({
            "identity": identity.public(key),
            "person": person.public(key),
        }),

        // --- organizations, groups, membership ----------------------------
        Event::OrganizationCreated {
            organization,
            resource,
            owner,
        } => json!({
            "organization": organization.public(key),
            "resource": resource.public(key),
            "owner": owner.public(key),
        }),
        Event::GroupCreated {
            group, resource, ..
        } => json!({
            "group": group.public(key),
            "resource": resource.public(key),
            "owner": ambiguous(),
        }),
        // The link's own id has no public form; the token is the only handle
        // there is and it is added by the reply, once, on the call that
        // minted it.
        Event::Invited { container, link } => json!({
            "container": container.public(key),
            "link": link.public(key),
        }),
        Event::LinkRevoked { container, link } => json!({
            "container": container.public(key),
            "link": link.public(key),
        }),
        Event::LinkClaimed {
            container,
            identity,
            party,
        } => json!({
            "container": container.public(key),
            "identity": identity.public(key),
            "party": party.public(key),
        }),
        Event::RoleSet { container, .. } => json!({
            "container": container.public(key),
            "party": ambiguous(),
        }),
        Event::MemberRemoved { container, party } | Event::Left { container, party } => json!({
            "container": container.public(key),
            "party": party.public(key),
        }),

        // --- relations and resources --------------------------------------
        Event::Shared {
            object_kind,
            object_id,
            relation,
        }
        | Event::Revoked {
            object_kind,
            object_id,
            relation,
        } => json!({
            "object_kind": object_kind,
            "object": object_id_of(object_kind, *object_id, key),
            "relation": relation.as_str(),
        }),
        Event::Transferred { resource, .. } => json!({
            "resource": resource.public(key),
            "to": ambiguous(),
        }),
        Event::DocumentCreated { document, .. } => json!({
            "document": document.public(key),
            "owner": ambiguous(),
        }),
        Event::DocumentEdited { document, rev } | Event::DocumentPublished { document, rev } => {
            json!({ "document": document.public(key), "rev": rev })
        }

        // --- the OpenID Provider ------------------------------------------
        // Never a secret. A code, an access token and a refresh token exist in
        // the clear once, in the answer the `/oidc/*` endpoints build from the
        // command's own return type — not here, which is a function of the
        // event and would emit them at `/api/cmd/<name>` as well.
        Event::HandleSet { person } => json!({ "person": person.public(key) }),
        Event::ClientRegistered { client, owner } => json!({
            "client": client.public(key),
            "owner": owner.map(|owner| owner.public(key)),
        }),
        Event::ClientUpdated { client }
        | Event::ClientSecretRotated { client }
        | Event::ClientDeleted { client }
        | Event::CodeExchanged { client }
        | Event::TokenRefreshed { client }
        | Event::TokenRevoked { client } => json!({ "client": client.public(key) }),
        Event::Authorized { client, person } | Event::ConsentRevoked { client, person } => json!({
            "client": client.public(key),
            "person": person.public(key),
        }),
        Event::ServiceTokenIssued { client, service } => json!({
            "client": client.public(key),
            "service": Id::<Service>::new(service.get()).public(key),
        }),
        // The `kid` is public — it is what the JWKS publishes each key by.
        Event::SigningKeyRotated { kid } => json!({ "kid": kid }),
        Event::SigningKeyRetired { kid, forced } => json!({ "kid": kid, "forced": forced }),

        // --- the platform operator ----------------------------------------
        Event::OperatorGranted { person } | Event::OperatorRevoked { person } => {
            json!({ "person": person.public(key) })
        }
        Event::ReAuthenticated { session, .. } => json!({ "session": session.public(key) }),
        // The person is named and the operator is not: the id a caller reads
        // back is the hat they are now wearing, and their own identity is
        // something they already hold. The reason lives on the audit row and
        // on the session, never in a reply.
        Event::Impersonated {
            person, session, ..
        }
        | Event::ImpersonationEnded {
            person, session, ..
        } => json!({
            "person": person.public(key),
            "session": session.public(key),
        }),
    }
}

/// A relation object's public id, under the tag its registered kind implies.
///
/// The kinds share no tag, so the id a caller reads back decrypts as the kind
/// it was granted on and as nothing else. `platform` is the singleton — row
/// `0`, no table, no id — and a kind this build does not register is `null`
/// rather than an id derived under a tag that would be a guess.
fn object_id_of(kind: &str, id: i64, key: &IdKey) -> Value {
    match kind {
        "document" => json!(Id::<Resource>::new(id).public(key)),
        "organization" => json!(Id::<Organization>::new(id).public(key)),
        "group" => json!(Id::<Group>::new(id).public(key)),
        "person" => json!(Id::<Person>::new(id).public(key)),
        "oidc_client" => json!(Id::<rn_kernel::ids::OidcClient>::new(id).public(key)),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rn_kernel::Relation;
    use rn_kernel::ids::MatchCandidate;
    use rn_kernel::merge::LinkMethod;

    fn key() -> IdKey {
        IdKey::from_hex("000102030405060708090a0b0c0d0e0f").expect("a test key")
    }

    #[test]
    fn a_merge_names_both_people_and_what_proved_it() {
        let key = key();
        let result = result_of(
            &Event::PersonMerged {
                survivor: Id::<Person>::new(1),
                absorbed: Id::<Person>::new(2),
                method: LinkMethod::SelfLink,
                candidate: Some(Id::<MatchCandidate>::new(3)),
            },
            &key,
            None,
        );
        assert!(
            result["survivor"]
                .as_str()
                .is_some_and(|id| id.starts_with("p_"))
        );
        assert_ne!(result["survivor"], result["absorbed"]);
        assert_eq!(result["method"], "self");
        assert!(
            result["candidate"]
                .as_str()
                .is_some_and(|id| id.starts_with("m_"))
        );
    }

    #[test]
    fn a_candidate_that_came_from_nowhere_is_null_rather_than_absent() {
        let result = result_of(
            &Event::PersonMerged {
                survivor: Id::<Person>::new(1),
                absorbed: Id::<Person>::new(2),
                method: LinkMethod::Operator,
                candidate: None,
            },
            &key(),
            None,
        );
        assert_eq!(result["candidate"], Value::Null);
    }

    #[test]
    fn an_invitation_names_its_link_row_and_never_the_secret_that_opens_it() {
        for event in [
            Event::Invited {
                container: Id::<Group>::new(4),
                link: Id::new(5),
            },
            Event::LinkRevoked {
                container: Id::<Group>::new(4),
                link: Id::new(5),
            },
        ] {
            let result = result_of(&event, &key(), None);
            assert!(
                result["container"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("g_"))
            );
            assert!(
                result["link"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("l_")),
                "the row a revocation would name is missing"
            );
            assert!(result.get("token").is_none(), "the reply adds the token");
        }
    }

    #[test]
    fn a_share_names_the_object_under_the_tag_its_kind_implies() {
        let key = key();
        let document = result_of(
            &Event::Shared {
                object_kind: "document".to_owned(),
                object_id: 7,
                relation: Relation::Editor,
            },
            &key,
            None,
        );
        assert_eq!(document["object_kind"], "document");
        assert_eq!(document["relation"], "editor");
        assert!(
            document["object"]
                .as_str()
                .is_some_and(|id| id.starts_with("r_"))
        );

        // The same row under another kind is another id, which is what stops
        // a document id from being read back as a group id.
        let group = result_of(
            &Event::Shared {
                object_kind: "group".to_owned(),
                object_id: 7,
                relation: Relation::Member,
            },
            &key,
            None,
        );
        assert_ne!(group["object"], document["object"]);

        // The singleton has no row and a kind this build cannot name has no
        // id it could honestly derive.
        for kind in ["platform", "something-else"] {
            let other = result_of(
                &Event::Shared {
                    object_kind: kind.to_owned(),
                    object_id: 0,
                    relation: Relation::Operator,
                },
                &key,
                None,
            );
            assert_eq!(other["object"], Value::Null, "{kind}");
        }
    }

    #[test]
    fn a_party_field_is_named_by_its_own_kind_and_never_by_its_table() {
        let key = key();
        let organization = Id::<Organization>::new(11).public(&key);
        let event = Event::Transferred {
            resource: Id::<Resource>::new(2),
            // The kernel types this `Id<Person>` because `party` is the table.
            // The row is an organization, and the reply has to say so.
            to: Id::<Person>::new(11),
        };
        assert_eq!(ambiguous_party(&event), Some(11));

        let result = result_of(&event, &key, Some(organization.clone()));
        assert_eq!(result["to"], json!(organization));
        assert!(result["to"].as_str().is_some_and(|id| id.starts_with("o_")));

        // Unresolved is null, never an id derived under a guessed tag: an id
        // that decodes as the wrong kind is worse than no id at all.
        assert_eq!(result_of(&event, &key, None)["to"], Value::Null);
    }

    #[test]
    fn only_the_events_whose_kind_is_open_ask_for_a_party_lookup() {
        assert_eq!(
            ambiguous_party(&Event::PersonSplit {
                identity: Id::new(1),
                person: Id::new(2),
            }),
            None,
            "a split is between persons, so nothing is open"
        );
        assert_eq!(
            ambiguous_party(&Event::RoleSet {
                container: Id::new(3),
                party: Id::new(4),
            }),
            Some(4),
            "a member may be a person, an organization or a group"
        );
    }

    #[test]
    fn a_document_edit_reports_the_revision_it_produced() {
        let result = result_of(
            &Event::DocumentEdited {
                document: Id::<Resource>::new(9),
                rev: 3,
            },
            &key(),
            None,
        );
        assert_eq!(result["rev"], 3);
        assert!(
            result["document"]
                .as_str()
                .is_some_and(|id| id.starts_with("r_"))
        );
    }
}
