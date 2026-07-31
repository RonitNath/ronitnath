//! Resolves the session cookie once per request, up front — everything
//! downstream ([`crate::auth::extract::AccountScope`], the nav) reads the
//! result out of request extensions instead of re-querying.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::CookieJar;

use crate::auth::session;
use crate::state::AppState;
use crate::store::sessions::SessionContext;

pub async fn attach_session(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let cookie_name = session::cookie_name(state.auth_config().cookie_secure);
    let jar = CookieJar::from_headers(request.headers());

    let context: Option<SessionContext> = match jar.get(cookie_name) {
        Some(cookie) => {
            let token_hash = session::hash_token(cookie.value());
            match state.store().find_session_context(&token_hash).await {
                Ok(Some(ctx)) => {
                    // Simplest-correct choice for a template: touch on
                    // every authenticated request rather than throttling
                    // to once/minute. A write-per-request is cheap at
                    // sqlite/demo scale; add throttling here if a fork's
                    // traffic ever makes it worth the extra complexity.
                    let _ = state.store().touch_session(ctx.session_id).await;
                    Some(ctx)
                }
                _ => None,
            }
        }
        None => None,
    };

    request.extensions_mut().insert(context);
    next.run(request).await
}
