//! `POST /auth/dev` — sign in as the platform operator, without a password.
//!
//! One button under the sign-in form, for the person developing this. It is a
//! bypass of the only control the model has, so it is gated twice and the two
//! gates are of different kinds:
//!
//! * **Compiled out.** This module, its route, its button and
//!   [`rn_kernel::dev`] behind it are all `#[cfg(debug_assertions)]`. A
//!   release build does not contain the handler, so no configuration, no
//!   environment and no mistake can reach it — which is the negative proof
//!   `procedures/engineering.md` §Security asks release builds to carry.
//! * **Off unless asked.** In a debug build it still answers `404` unless
//!   `RN_SITE__DEV=1` *and* the process is in dev mode. A `404` rather than a
//!   `403`: a refusal that distinguishes "not allowed" from "not there" tells
//!   a prober that the route exists, and this route existing is the whole
//!   finding.
//!
//! What it does is real. There is no shadow principal and no impersonation
//! flag on the session: the handler resolves the deployment's platform
//! operator, minting one through [`rn_api::commands::Register`] and
//! [`rn_kernel::bootstrap_operator`] if the deployment has none yet, and then
//! opens an ordinary session row for that identity through the kernel. The
//! cookie is the same cookie with the same attributes; the audit row says
//! `dev-sign-in`, so the tail records that this session was not earned.

use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::post;
use rn_api::commands::Register;
use rn_kernel::cmd::{self, Ctx};
use rn_kernel::domain::SESSION_TTL;
use rn_kernel::ids::{Id, Identity, Person};
use rn_kernel::store::{Count, Reads};
use rn_kernel::{Event, Principal, bind};

use crate::api::{decline, origin};
use crate::auth::{see_other, session};
use crate::state::AppState;

/// Where the bypass lands: the tier the operator relation opens.
const LANDS: &str = "/platform";

/// The person this deployment invents when it has no operator to sign in as.
///
/// `.invalid` is reserved by RFC 2606 and can never be a deliverable address,
/// which is what keeps a dev database's operator from colliding with anybody
/// real if the file it lives in is ever handed somewhere it should not be.
const DEV_EMAIL: &str = "dev-operator@example.invalid";
const DEV_DISPLAY: &str = "Dev operator";

/// An active identity of whoever holds `platform:* #operator`.
///
/// Lowest id, so a person with several registrations signs in as the same one
/// every time rather than as whichever row the planner happened to return.
const OPERATOR: &str = "SELECT i.id AS n FROM relation r \
                        JOIN identity i ON i.person_id = r.subject_id \
                        WHERE r.object_kind = 'platform' AND r.object_id = 0 \
                          AND r.relation = 'operator' AND r.subject_kind = 'person' \
                          AND i.status = 'active' \
                        ORDER BY i.id LIMIT 1";

/// The identity registered at an address, if one is.
const IDENTITY_OF_EMAIL: &str = "SELECT identity_id AS n FROM factor \
                                 WHERE kind = 'email' AND value = $1";

/// The person an identity belongs to.
const PERSON_OF_IDENTITY: &str = "SELECT person_id AS n FROM identity \
                                  WHERE id = $1 AND person_id IS NOT NULL";

/// The button, rendered under the sign-in commit it is an alternative to.
///
/// Quiet rather than a second `commit`: it is not a peer of signing in, and a
/// page with two primary actions has none. No label above it and no sentence
/// beside it — the interface states, it does not explain.
pub const BUTTON: &str =
    r#"<button type="submit" class="quiet" form="dev-sign-in">Sign in as operator</button>"#;

/// The form the button submits, named by its `form=` attribute because a form
/// cannot nest inside the sign-in form the button sits in.
pub const FORM: &str = r#"<form id="dev-sign-in" method="post" action="/auth/dev" hidden></form>"#;

pub fn router() -> Router<AppState> {
    Router::new().route("/auth/dev", post(sign_in_as_operator))
}

async fn sign_in_as_operator(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // The runtime gate comes first, ahead of the same-origin check, so that
    // every way of asking gets the same `404`. An extractor would have run
    // before this and answered `403` to a cross-site probe, which is the one
    // answer that says "there is something here".
    if !super::serves_dev_sign_in(&state.config) {
        return decline::not_found();
    }
    if !origin::may_mutate(&headers) {
        return decline::forbidden();
    }

    let Some(identity) = operator_identity(&state).await else {
        tracing::warn!("the developer sign-in has no operator to sign in as");
        return decline::not_found();
    };
    let ctx = context(&state);
    let minted = match rn_kernel::dev::sign_in_as(&ctx, identity).await {
        Ok(minted) => minted,
        Err(error) => {
            tracing::warn!(%error, "the developer sign-in was refused by the kernel");
            return decline::not_found();
        }
    };
    let cookie = minted
        .token
        .as_ref()
        .map(|token| session::set(token, SESSION_TTL, state.config.mode));
    tracing::warn!("a developer session was opened as the platform operator without a password");
    see_other(LANDS, cookie, minted.committed.offset)
}

/// The operator's identity, provisioning one if this deployment has none.
///
/// The order is the point. An existing operator is used as it is — the kernel
/// refuses a second `platform:* #operator` row, and a bypass that worked
/// around that refusal would be a second administrator nobody granted. Only
/// an empty deployment gets the invented one.
async fn operator_identity(state: &AppState) -> Option<Id<Identity>> {
    if let Some(identity) = one_id::<Identity>(state, OPERATOR, bind![]).await {
        return Some(identity);
    }
    let (identity, person) =
        match one_id::<Identity>(state, IDENTITY_OF_EMAIL, bind![DEV_EMAIL]).await {
            // The address is registered and does not operate anything — the
            // second press after the grant failed, or a database somebody seeded.
            Some(identity) => (
                identity,
                one_id::<Person>(state, PERSON_OF_IDENTITY, bind![identity]).await?,
            ),
            None => register_dev_operator(state).await?,
        };
    match rn_kernel::bootstrap_operator(state.store.as_ref(), person).await {
        Ok(()) => Some(identity),
        // The kernel's refusal, and it is load-bearing: somebody granted an
        // operator between the read above and this write.
        Err(error) => {
            tracing::warn!(%error, "the developer operator could not be granted");
            None
        }
    }
}

/// Register the dev operator through the real command.
///
/// The password is random and thrown away. Nothing signs in with it — this
/// route is the only way into the account — so the account cannot be reached
/// by guessing a password that was never chosen, and a dev database that
/// outlives its usefulness holds no credential anybody knows.
async fn register_dev_operator(state: &AppState) -> Option<(Id<Identity>, Id<Person>)> {
    let password = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    let executed = cmd::register(
        &context(state),
        &Register {
            display_name: DEV_DISPLAY.to_owned(),
            email: DEV_EMAIL.to_owned(),
            password,
        },
    )
    .await;
    match executed {
        Ok(minted) => match minted.committed.event {
            Event::Registered {
                identity, person, ..
            } => Some((identity, person)),
            _ => None,
        },
        Err(error) => {
            tracing::warn!(%error, "the developer operator could not be registered");
            None
        }
    }
}

/// One id out of a statement that selects it as `n`, or nothing.
async fn one_id<T: rn_kernel::ids::Table>(
    state: &AppState,
    sql: &'static str,
    params: Vec<rn_kernel::store::Value>,
) -> Option<Id<T>> {
    state
        .store
        .reads()
        .query::<Count>(sql, params)
        .await
        .inspect_err(|error| tracing::warn!(%error, "the developer sign-in could not read"))
        .ok()?
        .first()
        .map(|row| Id::new(row.0))
}

fn context(state: &AppState) -> Ctx<'_, hiqlite::Client, rn_kernel::feed::ClusterFeed> {
    Ctx {
        store: state.store.as_ref(),
        feed: state.feed.as_ref(),
        principal: Principal::Anonymous,
        key: uuid::Uuid::new_v4(),
    }
}
