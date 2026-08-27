//! `SetHandle` — choose or change the acting person's `preferred_username`.
//!
//! A handle that changes hands is an impersonation waiting to happen: the
//! reader who knew `@ronit` at one site would meet somebody else at the next.
//! So the old one is not freed. It is filed in `party_handle_alias` against
//! the same person, which keeps it resolving and keeps it unavailable — and
//! because the alias names *them*, they may take it back.

use rn_api::commands::SetHandle;

use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, refs, run};
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::oidc::handle;
use crate::store::{Reads, Sql};

/// File the handle a person is giving up, so nobody else may take it and they
/// may take it back. `INSERT OR IGNORE`, because a person who has held a
/// handle before already has the row.
const ALIAS: &str = "INSERT OR IGNORE INTO party_handle_alias (handle, person_id, at) \
     SELECT handle, id, $2 FROM party WHERE id = $1 AND handle IS NOT NULL";

const AUDIT: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'set-handle', $2, $3, $4, $5, \
            json_object('event', 'set-handle', 'person', $6) \
     WHERE changes() > 0";

/// Take a handle.
pub async fn set_handle<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &SetHandle,
) -> Outcome<Committed> {
    let (identity, person, acting_as) = refs::actor(&ctx.principal)?;
    let wanted = handle::validate(&args.handle)?;

    // Already theirs: nothing to do, and doing it anyway would file their own
    // handle as an alias of itself and then fail to set it.
    if handle::of_person(&ctx.store.reads(), person)
        .await?
        .as_deref()
        == Some(wanted.as_str())
    {
        return decline();
    }
    // A handle somebody else holds — or held — declines. It is not a secret
    // that a handle is taken (that is what a handle is *for*), and the refusal
    // is uniform anyway because the same command refuses a malformed one
    // differently: `Invalid` says the shape is wrong, a decline says no.
    if taken_by_another(ctx, &wanted, person).await? {
        return decline();
    }
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.any(ALIAS, bind![person, now]);
        // The guarded statement: the handle was free when it was read and is
        // still free here, or the whole batch writes nothing and `run` retries.
        batch.one(handle::SET_SQL, bind![person, wanted.as_str()]);
        batch.one(
            AUDIT,
            bind![
                ctx.key.to_string(),
                identity,
                acting_as,
                now,
                crate::audit::digest_of(args),
                person
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// Whether anybody other than this person holds or has held this handle.
async fn taken_by_another<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    wanted: &str,
    person: crate::ids::Id<crate::ids::Person>,
) -> Outcome<bool> {
    const SQL: &str = "SELECT (SELECT count(*) FROM party WHERE handle = $1 AND id <> $2) \
         + (SELECT count(*) FROM party_handle_alias WHERE handle = $1 AND person_id <> $2) AS n";
    Ok(ctx
        .store
        .reads()
        .query::<crate::store::Count>(SQL, bind![wanted, person])
        .await?
        .first()
        .is_some_and(|count| count.0 > 0))
}
