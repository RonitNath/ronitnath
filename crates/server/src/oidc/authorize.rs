//! `GET|POST /oidc/authorize` — where a person decides.
//!
//! The endpoint's order is its security property. The client and the redirect
//! URI are established first and alone; only then may anything be answered by
//! redirecting, because a redirect to an unverified URI is an open redirect
//! carrying this deployment's name. After that the request is digested
//! ([`super::request`]), the person is found, and the two questions that need
//! a human — *is this the right person* and *do they agree* — are asked in
//! that order.
//!
//! `prompt=none` never renders anything. Every case that would have is one of
//! the four errors OpenID Connect Core §3.1.2.6 names, redirected like any
//! other, which is the whole point of the parameter: a hidden iframe asking
//! "is this person still signed in" must never paint.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, http::StatusCode};
use rn_api::commands::Authorize;
use rn_kernel::oidc::{ClientRow, client as registry, consent, subject};
use rn_kernel::store::Reads as _;
use rn_kernel::{Principal, oidc::paths};
use serde::Deserialize;

use super::error::{Code, OauthError};
use super::page::{self, ConsentPage, Field};
use super::request::{Authorization, Prompt};
use super::url;
use crate::api::origin::SameOrigin;
use crate::auth::session::Visitor;
use crate::state::AppState;

/// `GET /oidc/authorize`.
pub async fn get(
    State(state): State<AppState>,
    visitor: Visitor,
    headers: HeaderMap,
    Query(query): Query<super::request::Query>,
) -> Response {
    run(&state, &visitor, &headers, query, None).await
}

/// The decision the consent page posts back.
#[derive(Debug, Deserialize)]
pub struct Decision {
    /// `allow` or anything else, which is a refusal.
    #[serde(default)]
    pub decision: String,
    /// The request, carried across as hidden fields.
    #[serde(flatten)]
    pub query: super::request::Query,
}

/// `POST /oidc/authorize` — Allow, or Deny.
pub async fn post(
    State(state): State<AppState>,
    _: SameOrigin,
    visitor: Visitor,
    headers: HeaderMap,
    Form(form): Form<Decision>,
) -> Response {
    let allowed = form.decision == "allow";
    run(&state, &visitor, &headers, form.query, Some(allowed)).await
}

/// The endpoint, for both methods.
///
/// `decision` is `None` on the way in and `Some` on the way back from the
/// consent page — which is the only difference between the two, because a
/// person who has already agreed and a person who has just agreed are the
/// same person from here on.
async fn run(
    state: &AppState,
    visitor: &Visitor,
    headers: &HeaderMap,
    query: super::request::Query,
    decision: Option<bool>,
) -> Response {
    // (1) The client. Nothing may be redirected until this holds.
    let Ok(client) = established(state, &query.client_id).await else {
        return page::refuse(
            state,
            headers,
            &OauthError::new(
                Code::InvalidClient,
                "that is not a client of this deployment",
            ),
        );
    };
    // (2) The redirect URI, matched against the registration.
    if !client.accepts_redirect(&query.redirect_uri) {
        return page::refuse(
            state,
            headers,
            &OauthError::new(
                Code::InvalidRequest,
                "that redirect_uri is not one this client registered",
            ),
        );
    }
    let redirect_uri = query.redirect_uri.clone();
    let sent_state = query.state.clone();
    let bounce =
        |error: &OauthError| redirect_error(state, &redirect_uri, sent_state.as_deref(), error);

    // (3) Everything else about the request. From here every failure lands at
    // the client, because both of the above hold.
    let asked = match query.digest(&client.metadata.scopes) {
        Ok(asked) => asked,
        Err(error) => return bounce(&error),
    };

    // (4) The person.
    let Principal::Member { person, .. } = &visitor.principal else {
        return match asked.prompt {
            Prompt::None => bounce(&OauthError::new(
                Code::LoginRequired,
                "no session, and prompt=none forbids asking for one",
            )),
            _ => sign_in(&query, false),
        };
    };
    let Some(person) = *person else {
        // A registration that has not resolved to a person has no `sub` to be
        // pairwise about. It cannot happen — registration resolves in its own
        // transaction — and guessing would be worse than saying so.
        return bounce(&OauthError::new(
            Code::ServerError,
            "that session has no person",
        ));
    };

    // (5) Is this the right person, and recently enough?
    match freshness(state, visitor, &asked, person).await {
        Freshness::Fine => {}
        Freshness::Refuse(error) => return bounce(&error),
        Freshness::Reauthenticate => {
            return match asked.prompt {
                Prompt::None => bounce(&OauthError::new(
                    Code::LoginRequired,
                    "that session is older than max_age, and prompt=none forbids asking",
                )),
                _ => sign_in(&query, true),
            };
        }
    }

    // (6) Will this client have them at all?
    match rn_kernel::oidc::client::load(&state.store.reads(), client.id).await {
        Ok(Some(_)) => {}
        _ => return bounce(&OauthError::new(Code::ServerError, "that client is gone")),
    }
    if !admits(state, &client, person).await {
        return bounce(&OauthError::new(
            Code::AccessDenied,
            "that client is for members of its organization",
        ));
    }

    // (7) Do they agree?
    if decision == Some(false) {
        return bounce(&OauthError::new(Code::AccessDenied, "the person declined"));
    }
    if decision.is_none() && !already_agreed(state, &client, person, &asked).await {
        return match asked.prompt {
            Prompt::None => bounce(&OauthError::new(
                Code::ConsentRequired,
                "consent has not been given, and prompt=none forbids asking",
            )),
            _ => consent_page(state, headers, &client, &asked, &query),
        };
    }

    // (8) The code.
    mint(state, visitor, &asked, &redirect_uri, sent_state.as_deref()).await
}

/// The client a `client_id` names, live.
async fn established(state: &AppState, client_id: &str) -> Result<ClientRow, ()> {
    let id = registry::decode_client_id(state.ids(), client_id).map_err(|_| ())?;
    registry::load(&state.store.reads(), id)
        .await
        .map_err(|_| ())?
        .ok_or(())
}

/// Whether a `members_only` client will have this person.
async fn admits(
    state: &AppState,
    client: &ClientRow,
    person: rn_kernel::Id<rn_kernel::ids::Person>,
) -> bool {
    if !client.metadata.members_only {
        return true;
    }
    let reads = state.store.reads();
    match client.owner_party_id {
        None => rn_kernel::cmd::is_platform_operator(&reads, person)
            .await
            .unwrap_or(false),
        Some(owner) if owner == person => true,
        Some(owner) => rn_kernel::org::role_of(&reads, rn_kernel::Id::new(owner.get()), person)
            .await
            .unwrap_or(None)
            .is_some(),
    }
}

/// Whether the consent page can be skipped.
///
/// A trusted client is first-party: the deployment is already the party being
/// consented to, and asking somebody to agree to give their own name to their
/// own site is a dialogue nobody learns anything from. Everyone else needs a
/// standing consent that covers everything being asked for — a client that
/// widens its request asks again, for the new part.
async fn already_agreed(
    state: &AppState,
    client: &ClientRow,
    person: rn_kernel::Id<rn_kernel::ids::Person>,
    asked: &Authorization,
) -> bool {
    if asked.prompt == Prompt::Consent {
        return false;
    }
    if client.metadata.trusted {
        return true;
    }
    consent::load(&state.store.reads(), client.id, person)
        .await
        .unwrap_or(None)
        .is_some_and(|row| row.covers(&asked.scopes))
}

/// Whether this session is the one the request will accept.
enum Freshness {
    /// It is.
    Fine,
    /// It is not, and asking would help.
    Reauthenticate,
    /// It is not, and asking would not help.
    Refuse(OauthError),
}

/// `max_age` and `id_token_hint`, which are the two ways an RP says something
/// about *which* authentication it will take.
async fn freshness(
    state: &AppState,
    visitor: &Visitor,
    asked: &Authorization,
    person: rn_kernel::Id<rn_kernel::ids::Person>,
) -> Freshness {
    if asked.prompt == Prompt::Login {
        return Freshness::Reauthenticate;
    }
    let reads = state.store.reads();
    let Some(session) = visitor.principal.session() else {
        return Freshness::Reauthenticate;
    };
    if let Some(max_age) = asked.max_age {
        let now = state.store.clock().now();
        match session_created(state, session).await {
            Some(at) if now - at <= max_age => {}
            _ => return Freshness::Reauthenticate,
        }
    }
    // The hint says which person the RP believes is here. It is validated as
    // ours — signature, issuer, audience — and may be expired: RP-initiated
    // logout and this both take an old one, because it is a *hint* about a
    // session, not a credential for it.
    if let Some(hint) = &asked.id_token_hint {
        let Some(hinted) = super::verified_hint(state, hint).await else {
            return Freshness::Refuse(OauthError::new(
                Code::InvalidRequest,
                "that id_token_hint was not issued here",
            ));
        };
        match subject::person_of(&reads, &hinted.sub).await {
            Ok(Some(named)) if named == person => {}
            Ok(_) => {
                return match asked.prompt {
                    Prompt::None => Freshness::Refuse(OauthError::new(
                        Code::AccountSelectionRequired,
                        "the session here is not the one that id_token_hint names",
                    )),
                    _ => Freshness::Reauthenticate,
                };
            }
            Err(_) => {
                return Freshness::Refuse(OauthError::new(
                    Code::ServerError,
                    "that hint could not be resolved",
                ));
            }
        }
    }
    Freshness::Fine
}

/// When a session was minted, which is when it last proved a factor.
async fn session_created(
    state: &AppState,
    session: rn_kernel::Id<rn_kernel::ids::Session>,
) -> Option<i64> {
    struct At(i64);
    impl rn_kernel::store::FromRow for At {
        fn from_row(
            row: &mut impl rn_kernel::store::Cursor,
        ) -> Result<Self, rn_kernel::store::RowError> {
            Ok(Self(row.int("created_at")?))
        }
    }
    state
        .store
        .reads()
        .query_opt::<At>(
            "SELECT created_at FROM session WHERE id = $1",
            rn_kernel::bind![session],
        )
        .await
        .ok()
        .flatten()
        .map(|at| at.0)
}

/// Send the reader to `/auth`, and back here afterwards.
///
/// `reauth` is what stops the round trip being a loop when there is already a
/// session: `/auth` sends a signed-in reader straight on, and `prompt=login`
/// means exactly "even so".
fn sign_in(query: &super::request::Query, reauth: bool) -> Response {
    let here = url::with_query(paths::AUTHORIZE, &fields(query, false));
    let mut pairs = vec![("next", here)];
    if reauth {
        pairs.push(("reauth", "1".to_owned()));
    }
    if let Some(hint) = &query.login_hint {
        pairs.push(("email", hint.clone()));
    }
    Redirect::to(&url::with_query("/auth", &pairs)).into_response()
}

/// The request, as parameters — for the round trip to `/auth` and back, and
/// for the consent page's hidden fields.
fn fields(query: &super::request::Query, include_prompt: bool) -> Vec<(&'static str, String)> {
    let mut pairs = vec![
        ("response_type", query.response_type.clone()),
        ("client_id", query.client_id.clone()),
        ("redirect_uri", query.redirect_uri.clone()),
        ("scope", query.scope.clone()),
    ];
    let optional: [(&'static str, Option<&String>); 6] = [
        ("state", query.state.as_ref()),
        ("nonce", query.nonce.as_ref()),
        ("code_challenge", query.code_challenge.as_ref()),
        (
            "code_challenge_method",
            query.code_challenge_method.as_ref(),
        ),
        ("login_hint", query.login_hint.as_ref()),
        ("id_token_hint", query.id_token_hint.as_ref()),
    ];
    for (name, value) in optional {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            pairs.push((name, value.clone()));
        }
    }
    if let Some(max_age) = query.max_age {
        pairs.push(("max_age", max_age.to_string()));
    }
    // `prompt` is deliberately dropped on the way to `/auth` and kept on the
    // consent form. Carried across a sign-in, `prompt=login` would ask the
    // reader to sign in again the moment they had, forever.
    if let Some(prompt) = query.prompt.as_ref().filter(|value| !value.is_empty())
        && include_prompt
    {
        pairs.push(("prompt", prompt.clone()));
    }
    pairs
}

/// The consent page: a door with the client's name on it.
fn consent_page(
    state: &AppState,
    headers: &HeaderMap,
    client: &ClientRow,
    asked: &Authorization,
    query: &super::request::Query,
) -> Response {
    let (theme, version, deployment) = page::chrome(state, headers);
    // `prompt=consent` is *not* carried into the hidden fields: it did its job
    // by bringing the reader here, and carrying it would make Allow render the
    // same page again.
    let mut carried: Vec<Field> = fields(query, false)
        .into_iter()
        .map(|(name, value)| Field::new(name, value))
        .collect();
    carried.retain(|field| !field.value.is_empty());
    ConsentPage {
        theme,
        version,
        deployment,
        client: client.metadata.client_name.clone(),
        client_uri: client.metadata.client_uri.clone().unwrap_or_default(),
        scopes: asked.scopes.iter().copied().map(page::describe).collect(),
        fields: carried,
    }
    .into_response()
}

/// Mint the code and send it home.
async fn mint(
    state: &AppState,
    visitor: &Visitor,
    asked: &Authorization,
    redirect_uri: &str,
    sent_state: Option<&str>,
) -> Response {
    let args = Authorize {
        client_id: asked.client_id.clone(),
        redirect_uri: redirect_uri.to_owned(),
        scopes: asked.scopes.clone(),
        nonce: asked.nonce.clone(),
        code_challenge: asked.code_challenge.clone(),
    };
    let ctx = super::context(state, visitor.principal.clone());
    match rn_kernel::cmd::authorize(&ctx, &args).await {
        Ok(authorized) => {
            let Some(code) = authorized.code else {
                // A replay of the same idempotency key, which the endpoint
                // does not produce: the key is minted per request.
                return redirect_error(
                    state,
                    redirect_uri,
                    sent_state,
                    &OauthError::new(Code::ServerError, "no code was minted"),
                );
            };
            let mut pairs = vec![("code", code.expose().to_owned())];
            if let Some(sent) = sent_state {
                pairs.push(("state", sent.to_owned()));
            }
            // RFC 9207: the issuer, on the response, so a client with more
            // than one provider cannot be made to redeem a code at the wrong
            // one. Declared by `authorization_response_iss_parameter_supported`.
            pairs.push(("iss", state.provider.issuer().to_owned()));
            Redirect::to(&url::with_query(redirect_uri, &pairs)).into_response()
        }
        Err(error) => {
            tracing::debug!(%error, "an authorization was refused");
            redirect_error(
                state,
                redirect_uri,
                sent_state,
                &OauthError::new(Code::AccessDenied, "that authorization was refused"),
            )
        }
    }
}

/// The redirect form of a failure (RFC 6749 §4.1.2.1), with `iss`.
fn redirect_error(
    state: &AppState,
    redirect_uri: &str,
    sent_state: Option<&str>,
    error: &OauthError,
) -> Response {
    let mut pairs = vec![
        ("error", error.code.as_str().to_owned()),
        ("error_description", error.description.to_owned()),
    ];
    if let Some(sent) = sent_state {
        pairs.push(("state", sent.to_owned()));
    }
    pairs.push(("iss", state.provider.issuer().to_owned()));
    (
        StatusCode::FOUND,
        Redirect::to(&url::with_query(redirect_uri, &pairs)),
    )
        .into_response()
}
