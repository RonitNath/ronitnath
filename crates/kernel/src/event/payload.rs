//! Reading an event back out of the payload the database wrote.
//!
//! One direction, one function. A fresh commit and an idempotent replay both
//! arrive here, so the two cannot answer differently — and a payload this
//! build does not understand is `None` rather than a panic, which is what a
//! consumer resuming across a release upgrade will meet.

use serde_json::Value as Json;

use super::Event;
use crate::domain::FactorKind;
use crate::ids::{Id, Identity, Person, Table};
use crate::merge::LinkMethod;
use crate::relation::Relation;

impl Event {
    /// Read an event out of an audit row's payload.
    ///
    /// A payload that does not parse is not a decline and not a panic: it is a
    /// row this build does not understand, which is what a consumer resuming
    /// across a release upgrade will meet.
    pub fn from_payload(payload: &str) -> Option<Self> {
        let json: Json = serde_json::from_str(payload).ok()?;
        Some(match json.get("event")?.as_str()? {
            "register" => Self::Registered {
                identity: field(&json, "identity")?,
                person: field(&json, "person")?,
                session: field(&json, "session")?,
            },
            "sign-in" => Self::SignedIn {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
            },
            "sign-out" => Self::SignedOut {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
                clients: ids(&json, "clients"),
            },
            "revoke-session" => Self::SessionRevoked {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
                clients: ids(&json, "clients"),
            },
            "end-session" => Self::SessionEnded {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
                clients: ids(&json, "clients"),
            },
            "act-as" => Self::ActingAs {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
                party: field(&json, "party")?,
            },
            "add-factor" => Self::FactorAdded {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
                kind: <FactorKind as crate::domain::Vocabulary>::parse(
                    json.get("kind")?.as_str()?,
                )?,
            },
            "remove-factor" => Self::FactorRemoved {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
            },
            "verify-email" => Self::EmailVerified {
                identity: field(&json, "identity")?,
                factor: field(&json, "factor")?,
            },
            "disable" => Self::PartyDisabled {
                party: field(&json, "party")?,
                clients: ids(&json, "clients"),
            },
            "enable" => Self::PartyEnabled {
                party: field(&json, "party")?,
            },
            "propose-match" => Self::MatchProposed {
                candidate: field(&json, "candidate")?,
            },
            // One tag, two shapes: a ruling either merges or closes the pair,
            // and `survivor` is what tells them apart.
            "rule-match" | "confirm-match" => match field::<Person>(&json, "survivor") {
                Some(survivor) => Self::PersonMerged {
                    survivor,
                    absorbed: field(&json, "absorbed")?,
                    method: <LinkMethod as crate::domain::Vocabulary>::parse(
                        json.get("method")?.as_str()?,
                    )?,
                    candidate: field(&json, "candidate"),
                },
                None => match field::<Identity>(&json, "identity") {
                    Some(identity) => Self::IdentityLinked {
                        identity,
                        person: field(&json, "person")?,
                        method: <LinkMethod as crate::domain::Vocabulary>::parse(
                            json.get("method")?.as_str()?,
                        )?,
                    },
                    None => Self::MatchRejected {
                        candidate: field(&json, "candidate")?,
                    },
                },
            },
            "split" => Self::PersonSplit {
                identity: field(&json, "identity")?,
                person: field(&json, "person")?,
            },
            "create-organization" => Self::OrganizationCreated {
                organization: field(&json, "organization")?,
                resource: field(&json, "resource")?,
                owner: field(&json, "owner")?,
            },
            "create-group" => Self::GroupCreated {
                group: field(&json, "group")?,
                resource: field(&json, "resource")?,
                owner: field(&json, "owner")?,
            },
            "invite" => Self::Invited {
                container: field(&json, "container")?,
                link: field(&json, "link")?,
            },
            "revoke-link" => Self::LinkRevoked {
                container: field(&json, "container")?,
                link: field(&json, "link")?,
            },
            "claim-link" => Self::LinkClaimed {
                container: field(&json, "container")?,
                identity: field(&json, "identity")?,
                party: field(&json, "party")?,
            },
            "set-role" => Self::RoleSet {
                container: field(&json, "container")?,
                party: field(&json, "party")?,
            },
            "remove-member" => Self::MemberRemoved {
                container: field(&json, "container")?,
                party: field(&json, "party")?,
            },
            "leave" => Self::Left {
                container: field(&json, "container")?,
                party: field(&json, "party")?,
            },
            "share" => Self::Shared {
                object_kind: json.get("object_kind")?.as_str()?.to_owned(),
                object_id: json.get("object_id")?.as_i64()?,
                relation: Relation::parse(json.get("relation")?.as_str()?)?,
            },
            "revoke" => Self::Revoked {
                object_kind: json.get("object_kind")?.as_str()?.to_owned(),
                object_id: json.get("object_id")?.as_i64()?,
                relation: Relation::parse(json.get("relation")?.as_str()?)?,
            },
            "transfer" => Self::Transferred {
                resource: field(&json, "resource")?,
                to: field(&json, "to")?,
            },
            "create-document" => Self::DocumentCreated {
                document: field(&json, "document")?,
                owner: field(&json, "owner")?,
            },
            "edit-document" => Self::DocumentEdited {
                document: field(&json, "document")?,
                rev: json.get("rev")?.as_i64()?,
            },
            "publish-document" => Self::DocumentPublished {
                document: field(&json, "document")?,
                rev: json.get("rev")?.as_i64()?,
            },
            "set-handle" => Self::HandleSet {
                person: field(&json, "person")?,
            },
            "register-client" => Self::ClientRegistered {
                client: field(&json, "client")?,
                // Absent is the deployment's own, which has no party row.
                owner: field(&json, "owner"),
            },
            "update-client" => Self::ClientUpdated {
                client: field(&json, "client")?,
            },
            "rotate-client-secret" => Self::ClientSecretRotated {
                client: field(&json, "client")?,
            },
            "delete-client" => Self::ClientDeleted {
                client: field(&json, "client")?,
            },
            "rotate-signing-key" => Self::SigningKeyRotated {
                kid: json.get("kid")?.as_str()?.to_owned(),
            },
            "retire-key" => Self::SigningKeyRetired {
                kid: json.get("kid")?.as_str()?.to_owned(),
                // Absent reads as an ordinary retire. The forced one always
                // says so, because it is the one that is a decision.
                forced: json.get("forced").and_then(Json::as_bool).unwrap_or(false),
            },
            "grant-operator" => Self::OperatorGranted {
                person: field(&json, "person")?,
            },
            "revoke-operator" => Self::OperatorRevoked {
                person: field(&json, "person")?,
            },
            "re-authenticate" => Self::ReAuthenticated {
                identity: field(&json, "identity")?,
                session: field(&json, "session")?,
            },
            "sign-in-as" => Self::Impersonated {
                operator: field(&json, "operator")?,
                person: field(&json, "person")?,
                session: field(&json, "session")?,
            },
            "end-impersonation" => Self::ImpersonationEnded {
                operator: field(&json, "operator")?,
                person: field(&json, "person")?,
                session: field(&json, "session")?,
            },
            "authorize" => Self::Authorized {
                client: field(&json, "client")?,
                person: field(&json, "person")?,
            },
            "exchange-code" => Self::CodeExchanged {
                client: field(&json, "client")?,
            },
            "refresh-token" => Self::TokenRefreshed {
                client: field(&json, "client")?,
            },
            "client-credentials" => Self::ServiceTokenIssued {
                client: field(&json, "client")?,
                service: field(&json, "service")?,
            },
            "revoke-token" => Self::TokenRevoked {
                client: field(&json, "client")?,
            },
            "enable-product" => Self::ProductEnabled {
                slug: json.get("slug")?.as_str()?.to_owned(),
            },
            "disable-product" => Self::ProductDisabled {
                slug: json.get("slug")?.as_str()?.to_owned(),
            },
            "revoke-consent" => Self::ConsentRevoked {
                client: field(&json, "client")?,
                person: field(&json, "person")?,
            },
            _ => return None,
        })
    }
}

/// One id out of a payload the database wrote. Generic, because a closure
/// would have to pick one table and every variant names a different one.
fn field<T: Table>(json: &Json, name: &str) -> Option<Id<T>> {
    json.get(name)?.as_i64().map(Id::new)
}

/// A JSON array of rowids out of a payload, as ids.
///
/// Absent reads as empty rather than as a failure: an audit row written before
/// this deployment had an OpenID Provider carries no `clients` member, and it
/// is still a sign-out.
fn ids<T: Table>(json: &Json, name: &str) -> Vec<Id<T>> {
    json.get(name)
        .and_then(Json::as_array)
        .map(|list| list.iter().filter_map(Json::as_i64).map(Id::new).collect())
        .unwrap_or_default()
}
