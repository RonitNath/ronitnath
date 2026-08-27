//! `ConfirmMatch` — the self-link merge: proof a person supplies about
//! themselves.
//!
//! There are exactly two things this command accepts as proof, and both are
//! things only the person can produce:
//!
//! * **The other identity's credentials.** The caller is signed in on one and
//!   types the address and password of the other. It is the same evidence as
//!   signing in twice, and it is the form a *browser* can give: a page has no
//!   way to hold two session cookies at once. Verified here, against the same
//!   argon2id hash `SignIn` reads and with the same dummy verification on the
//!   miss, so a wrong address does not answer faster than a wrong password.
//!   **No session is created and nothing is handed back.** Proving you could
//!   sign in is not signing in.
//! * **Live sessions on both identities.** The caller supplies the session
//!   token of the other. The same claim, for an API caller that can hold two
//!   tokens at once.
//! * **A factor both have verified.** The same address, proven on each side by
//!   a message that was delivered and clicked. Unverified factors are not
//!   admissible here: two people can *claim* an address and only one can
//!   prove it.
//!
//! A signal is not on that list, and no combination of signals is either. The
//! candidate row says which question was asked; this command is the only
//! answer to it that a person can give.

use rn_api::commands::ConfirmMatch;

use super::candidate::{self, CandidateStatus};
use super::recovery::owned_identity;
use super::{LinkMethod, Plan, apply};
use crate::bind;
use crate::cmd::{Applied, Batch, Ctx, member, run};
use crate::domain::{Token, Vocabulary};
use crate::error::{Outcome, decline};
use crate::event::Committed;
use crate::feed::Feed;
use crate::ids::{self, Id, Identity, MatchCandidate};
use crate::store::{Count, Reads, Sql, Value};

/// The candidate is closed by the same transaction that merges. `Expect::Any`
/// rather than a guard of its own: the two always move together, so a second
/// caller meets the *merge* guard first and re-reads.
const CANDIDATE: &str =
    "UPDATE match_candidate SET status = 'confirmed' WHERE id = $1 AND status = 'proposed'";

const AUDIT_MERGE: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'confirm-match', $2, $3, $4, $5, \
            json_object('event', 'confirm-match', 'candidate', $6, 'survivor', $7, \
                        'absorbed', $8, 'method', $9) \
     WHERE changes() > 0";

const AUDIT_LINK: &str = "INSERT INTO audit \
     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
     SELECT $1, 'confirm-match', $2, $3, $4, $5, \
            json_object('event', 'confirm-match', 'candidate', $6, 'identity', $7, \
                        'person', $8, 'method', $9) \
     WHERE changes() > 0";

/// The identity an address belongs to, and the password hash it proves
/// against. The statement `SignIn` uses, for the same reason: the partial
/// unique index on `factor(value) WHERE kind = 'email'` makes it a seek.
pub const CREDENTIALS_SQL: &str = crate::cmd::SIGN_IN_SQL;

/// A live session, by its token digest.
pub const SESSION_SQL: &str =
    "SELECT identity_id AS n FROM session WHERE token_hash = $1 AND expires_at > $2";

/// A factor value both identities have proven.
///
/// `kind` must match as well as `value`: a password hash two identities happen
/// to share says only that two people chose the same password, which is a fact
/// about passwords and not about people.
pub const SHARED_FACTOR_SQL: &str = "SELECT count(*) AS n FROM factor a \
     JOIN factor b ON b.value = a.value AND b.kind = a.kind \
     WHERE a.identity_id = $1 AND b.identity_id = $2 \
       AND a.verified_at IS NOT NULL AND b.verified_at IS NOT NULL \
       AND a.kind IN ('email', 'oidc')";

/// Confirm a queued pair with the person's own proof.
pub async fn confirm_match<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &ConfirmMatch,
) -> Outcome<Committed> {
    let (identity, acting_as, _) = member(&ctx.principal)?;
    let Ok(candidate) = ids::decode::<MatchCandidate>(ctx.store.ids(), &args.candidate) else {
        return decline();
    };
    let Some(row) = candidate::by_id(&ctx.store.reads(), candidate).await? else {
        return decline();
    };
    if row.status != CandidateStatus::Proposed {
        return decline();
    }

    // Which side of the pair the caller holds — their own registration, or one
    // their person already absorbed.
    let mine = if row.other(identity).is_some() {
        identity
    } else if owned_identity(&ctx.store.reads(), acting_as, row.identity_a).await? {
        row.identity_a
    } else if owned_identity(&ctx.store.reads(), acting_as, row.identity_b).await? {
        row.identity_b
    } else {
        return decline();
    };
    let Some(other) = row.other(mine) else {
        return decline();
    };

    let method = match proof(ctx, mine, other, args).await? {
        Some(method) => method,
        None => return decline(),
    };
    let plan = super::plan(&ctx.store.reads(), mine, other).await?;
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        batch.any(CANDIDATE, bind![candidate]);
        push(&mut batch, plan, method, ctx, args, candidate, now)?;
        Ok(batch)
    })
    .await?;
    Ok(committed)
}

/// What the caller proved, if anything.
async fn proof<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    mine: Id<Identity>,
    other: Id<Identity>,
    args: &ConfirmMatch,
) -> Outcome<Option<LinkMethod>> {
    if let Some(method) = by_credentials(ctx, other, args).await? {
        return Ok(Some(method));
    }
    if let Some(raw) = args.other_session.as_deref() {
        let token = Token::from_wire(raw);
        let rows = ctx
            .store
            .query::<Count>(SESSION_SQL, bind![token.digest().to_vec(), ctx.now()])
            .await?;
        if rows.first().is_some_and(|row| row.0 == other.get()) {
            return Ok(Some(LinkMethod::SelfLink));
        }
    }
    let shared = ctx
        .store
        .query::<Count>(SHARED_FACTOR_SQL, bind![mine, other])
        .await?;
    Ok(shared
        .first()
        .is_some_and(|row| row.0 > 0)
        .then_some(LinkMethod::Factor))
}

/// The other identity's own credentials, verified here.
///
/// Every refusal is `None` and every path does the same work: an address that
/// belongs to nobody, one that belongs to somebody who is not the other half
/// of this pair, and a wrong password all spend one argon2id verification, so
/// the timing says nothing about which it was. No session is minted, nothing
/// is written, and the caller's own cookie is untouched.
async fn by_credentials<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    other: Id<Identity>,
    args: &ConfirmMatch,
) -> Outcome<Option<LinkMethod>> {
    let (Some(email), Some(password)) =
        (args.other_email.as_deref(), args.other_password.as_deref())
    else {
        return Ok(None);
    };
    let email = crate::domain::normalize_email(email);
    let candidate = ctx
        .store
        .query_opt::<Credentials>(CREDENTIALS_SQL, bind![email.as_str()])
        .await?;
    let Some(candidate) =
        candidate.filter(|row| row.identity_id == other && row.status == "active")
    else {
        crate::password::verify_dummy_blocking(password).await;
        return Ok(None);
    };
    let Some(phc) = candidate.phc.as_deref() else {
        crate::password::verify_dummy_blocking(password).await;
        return Ok(None);
    };
    if !crate::password::verify_blocking(password, phc).await {
        return Ok(None);
    }
    Ok(Some(LinkMethod::SelfLink))
}

/// What the credentials lookup reads back.
struct Credentials {
    identity_id: Id<Identity>,
    status: String,
    phc: Option<String>,
}

impl crate::store::FromRow for Credentials {
    fn from_row(row: &mut impl crate::store::Cursor) -> Result<Self, crate::store::RowError> {
        Ok(Self {
            identity_id: row.id("identity_id")?,
            status: row.text("status")?,
            phc: row.text_opt("phc")?,
        })
    }
}

/// The mutations and the audit row for whichever shape the pair has.
fn push<S: Sql, F: Feed>(
    batch: &mut Batch,
    plan: Plan,
    method: LinkMethod,
    ctx: &Ctx<'_, S, F>,
    args: &ConfirmMatch,
    candidate: Id<MatchCandidate>,
    now: crate::Timestamp,
) -> Outcome<()> {
    let (identity, acting_as, _) = member(&ctx.principal)?;
    let head = vec![
        Value::from(ctx.key.to_string()),
        Value::from(identity),
        Value::from(acting_as),
        Value::from(now),
        Value::from(crate::audit::digest_of(args)),
        Value::from(candidate),
    ];
    match plan {
        Plan::Absorb { survivor, absorbed } => {
            apply::Merge {
                survivor,
                absorbed,
                method,
                evidence: None,
                asserted_by: Some(identity),
                at: now,
            }
            .push(batch);
            let mut params = head;
            params.extend([
                Value::from(survivor),
                Value::from(absorbed),
                Value::from(method.as_str()),
            ]);
            batch.one(AUDIT_MERGE, params);
        }
        Plan::Link {
            identity: unresolved,
            person,
        } => {
            apply::Link {
                identity: unresolved,
                person,
                method,
                evidence: None,
                asserted_by: Some(identity),
                at: now,
            }
            .push(batch);
            let mut params = head;
            params.extend([
                Value::from(unresolved),
                Value::from(person),
                Value::from(method.as_str()),
            ]);
            batch.one(AUDIT_LINK, params);
        }
        // Already one person. Nothing to write, and writing the candidate
        // alone would be a merge event with no merge in it.
        Plan::Nothing => return decline(),
    }
    Ok(())
}
