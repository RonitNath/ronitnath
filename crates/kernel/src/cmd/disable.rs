//! `Disable` — stop a party authenticating and being a usable subject.
//!
//! Not deletion. The model does not delete a party: rows across every product
//! reference it, `person_alias` keeps its old URLs resolving, and a merge may
//! yet absorb it. Disabling is reversible by [`enable`](super::enable), and
//! the two together are the whole lifecycle.
//!
//! Any party may be disabled — a person, an organization, a group — and who
//! may do it depends on which. A person is theirs to disable, and a platform
//! operator's. An organization or a group belongs to somebody, so its owner
//! may, and so may an operator. "Platform operator" is a relation row
//! (`platform:* #operator @person:R`), not a column and not a role enum — the
//! kernel report's ruling, and the reason nothing in this crate has an
//! `is_admin`.

use rn_api::commands::Disable;

use rn_api::ids::PublicId;

use super::{Applied, Batch, Ctx, refs, run};
use crate::authority::{self, Want};
use crate::bind;
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Group, Id, Organization, Person};
use crate::org::MemberRole;
use crate::resource;
use crate::store::{Reads, Sql};

const PARTY: &str = "UPDATE party SET status = 'disabled' WHERE id = $1 AND status = 'active'";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'disable', $2, $3, $4, $5, \
                            json_object('event', 'disable', 'party', $6, 'reason', $7, \
                                        'clients', json($8)) \
                     WHERE changes() > 0";

/// Every session of every identity resolved onto the party goes with it — a
/// disabled person that keeps answering from an open tab is not disabled. This
/// runs after the audit row, because it is a consequence of the decision
/// rather than part of it, and its count carries no information.
///
/// `person_id` is the whole of it, and the `OR i.id = $1` that used to sit
/// beside it was a bug rather than a fallback: it was meant for an unresolved
/// registration acting as itself, but `identity` and `party` are different
/// tables with independent rowids, so it signed out whoever happened to be
/// identity N whenever party N was disabled. An unresolved registration has no
/// `party` row at all, so the update above already writes nothing for one and
/// there is nothing here to catch.
const SESSIONS: &str =
    "DELETE FROM session WHERE identity_id IN (SELECT id FROM identity WHERE person_id = $1)";

/// Move a party to `disabled`.
pub async fn disable<S: Sql, F: Feed>(ctx: &Ctx<'_, S, F>, args: &Disable) -> Outcome<Committed> {
    // An impersonated session may not change what the person is, or who
    // may become them (`authority::FORBIDDEN_WHILE_IMPERSONATING`).
    authority::not_impersonating(&ctx.principal)?;
    let (identity, _person, acting_as) = refs::actor(&ctx.principal)?;
    let target = authorised(ctx, &args.party).await?;
    let now = ctx.now();
    // Every relying party holding a token for this party. A disable ends every
    // session at once, so there is no single `sid` to name and the logout
    // tokens carry `sub` alone — which is exactly what "this subject, wherever
    // they are" means.
    let targets = crate::oidc::token::logout_targets_of_person(&ctx.store.reads(), target).await?;
    let clients = crate::oidc::token::target_ids_json(&targets);

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.one(PARTY, bind![target]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                target,
                args.reason.as_str(),
                clients.as_str()
            ],
        );
        // The tokens go before the sessions they reference, and the codes with
        // them: `oidc_token.session_id` points at a row that is about to stop
        // existing.
        batch.any(crate::oidc::token::DELETE_PERSON_TOKENS_SQL, bind![target]);
        batch.any(crate::oidc::token::DELETE_PERSON_CODES_SQL, bind![target]);
        batch.any(SESSIONS, bind![target]);
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// The party a status command names, once this principal has proved it may
/// move it.
///
/// Three refusals, one answer: an id that does not decrypt under any party
/// kind, one naming a row that is not there, and one this principal does not
/// administer all decline identically — telling them apart is how a caller
/// enumerates a deployment.
///
/// Each of the three kinds ends in [`authority::allows`], so the operator
/// clause is the same sentence here as everywhere else rather than the
/// hand-written `is_platform_operator` this file used to carry twice.
pub(super) async fn authorised<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    named: &PublicId,
) -> Outcome<Id<Person>> {
    let key = ctx.store.ids();
    // A person: themselves, or an operator.
    if let Ok(person) = ids::decode::<Person>(key, named) {
        authority::require(ctx, Want::Themselves(person)).await?;
        return Ok(person);
    }
    // An organization: its owner, or an operator. An organization is its own
    // root group, so the membership that says "owner" is on its own party id.
    if let Ok(organization) = ids::decode::<Organization>(key, named) {
        let party: Id<Person> = Id::new(organization.get());
        authority::require(
            ctx,
            Want::Role {
                container: party,
                role: MemberRole::Owner,
            },
        )
        .await?;
        return Ok(party);
    }
    // A group: whoever owns it. That is a person or an organization, and for
    // an organization the rule is the organization's own — its owner.
    let Ok(group) = ids::decode::<Group>(key, named) else {
        return decline();
    };
    let Some(row) = resource::of_group(ctx.store, group).await? else {
        return decline();
    };
    let party: Id<Person> = Id::new(group.get());
    let (_, person, _) = super::member(&ctx.principal)?;
    if authority::allows(ctx, Want::Settled(person == row.owner_party_id)).await?
        || authority::allows(
            ctx,
            Want::Role {
                container: row.owner_party_id,
                role: MemberRole::Owner,
            },
        )
        .await?
    {
        return Ok(party);
    }
    decline()
}
