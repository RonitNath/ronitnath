//! `AddFactor` — add a factor to the acting identity.
//!
//! `passkey` and `oidc` are in the schema and refused here. That is not an
//! oversight to be tidied away: the column exists so those features land
//! without a migration on a live cluster, and accepting a value no code can
//! verify would be a factor that proves nothing.

use rn_api::commands::AddFactor;

use super::{Applied, Batch, Ctx, member, run};
use crate::domain::{EMAIL_LIMIT, FactorKind, PASSWORD_MIN, looks_like_email, normalize_email};
use crate::error::{Invalid, Outcome};
use crate::event::Committed;
use crate::feed::Feed;
use crate::merge::target_identity;
use crate::password;
use crate::store::{Sql, Value};

const FACTOR: &str = "INSERT INTO factor (identity_id, kind, value, created_at) \
                      SELECT $1, $2, $3, $4 \
                      WHERE EXISTS (SELECT 1 FROM identity WHERE id = $1 AND status = 'active') \
                      RETURNING id";

const AUDIT: &str = "INSERT INTO audit \
                     (key, command, actor_identity_id, acting_as, at, request_digest, payload) \
                     SELECT $1, 'add-factor', $2, $3, $4, $5, \
                            json_object('event', 'add-factor', 'identity', $6, 'factor', $7, \
                                        'kind', $8) \
                     WHERE changes() > 0";

/// Add an email or password factor to the acting identity.
pub async fn add_factor<S: Sql, F: Feed>(
    ctx: &Ctx<'_, S, F>,
    args: &AddFactor,
) -> Outcome<Committed> {
    let (actor, acting_as, _) = member(&ctx.principal)?;
    // The acting identity by default, one of the person's others when the
    // caller names it — `crate::merge::recovery` is where that rule lives.
    let identity =
        target_identity(&ctx.store.reads(), actor, acting_as, args.identity.as_ref()).await?;
    let kind = FactorKind::from(args.kind);
    if !kind.is_built() {
        return Err(Invalid::UnsupportedFactor.into());
    }

    let value = match kind {
        FactorKind::Email => {
            let email = normalize_email(&args.value);
            if !looks_like_email(&email) {
                return Err(Invalid::NotAnEmail.into());
            }
            if email.len() > EMAIL_LIMIT {
                return Err(Invalid::TooLong {
                    field: "email",
                    limit: EMAIL_LIMIT,
                }
                .into());
            }
            email
        }
        FactorKind::Password => {
            if args.value.chars().count() < PASSWORD_MIN {
                return Err(Invalid::PasswordTooShort(PASSWORD_MIN).into());
            }
            password::hash(&args.value)
                .map_err(|err| crate::KernelError::Invariant(err.to_string()))?
        }
        FactorKind::Passkey | FactorKind::Oidc => return Err(Invalid::UnsupportedFactor.into()),
    };
    let now = ctx.now();

    let Applied { committed, .. } = run(ctx, args, async || {
        let mut batch = Batch::new();
        let factor = batch.one(
            FACTOR,
            vec![
                Value::from(identity),
                Value::from(<FactorKind as crate::domain::Vocabulary>::as_str(kind)),
                Value::from(value.as_str()),
                Value::from(now),
            ],
        );
        batch.one(
            AUDIT,
            vec![
                Value::from(ctx.key.to_string()),
                Value::from(actor),
                Value::from(acting_as),
                Value::from(now),
                Value::from(crate::audit::digest_of(args)),
                Value::from(identity),
                factor.column("id"),
                Value::from(<FactorKind as crate::domain::Vocabulary>::as_str(kind)),
            ],
        );
        Ok(batch)
    })
    .await?;
    Ok(committed)
}
