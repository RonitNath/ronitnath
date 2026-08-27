//! `/auth` — the one page a person can reach without being anybody yet.
//!
//! Server-rendered, because it has to work before any bundle is allowed to
//! load and because a sign-in form that needs JavaScript is a sign-in form
//! that fails for the reader whose script did not arrive. Signing in and
//! registering share the page: they are the same decision from the visitor's
//! side, and two pages would mean guessing which one to send them to.
//!
//! Both posts go through [`crate::api::cmd::invoke`], so a form post and a
//! bundle's `POST /api/cmd/<name>` execute the identical dispatch — the auth
//! pages have no second registration path to drift from the first.
//!
//! Every refusal reads the same. The kernel spends an argon2 verification
//! against a dummy hash on the unknown-address path (`kernel::cmd::sign_in`),
//! so a wrong password and an address nobody has registered take the same time
//! as well as saying the same thing.

#[cfg(debug_assertions)]
mod dev;
pub mod next;
pub mod session;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use rn_api::commands::{Register, SignIn, SignOut};
use rn_kernel::{Invalid, KernelError, Principal};
use serde::Deserialize;

use crate::api::cmd::{self, CommandError};
use crate::api::decline;
use crate::api::origin::SameOrigin;
use crate::config::AppConfig;
use crate::presence::theme_for;
use crate::state::AppState;

pub use session::{Session, Visitor};

pub fn router() -> Router<AppState> {
    let router = Router::new()
        .route("/auth", get(page))
        .route("/auth/register", post(register))
        .route("/auth/sign-in", post(sign_in))
        .route("/auth/sign-out", post(sign_out));
    // The developer bypass, merged only into a debug build. In a release
    // build there is no route, no handler and no kernel entry behind it —
    // `crates/server/tests/surface.rs` asserts the `404` in both profiles.
    #[cfg(debug_assertions)]
    let router = router.merge(dev::router());
    router
}

/// Whether this build *and* this configuration serve `POST /auth/dev`.
///
/// Written once, read by the handler and by the page that renders its button,
/// so the button cannot appear over a route that would refuse it. The release
/// arm is a constant `false` rather than an absent function, because the
/// template field it feeds has to exist in both profiles for the page to
/// compile at all.
/// The button and its form, as this build and this configuration render them.
///
/// A release build has neither, and has never heard of either: the strings
/// live in [`dev`], which it does not compile.
fn dev_markup(config: &AppConfig) -> (&'static str, &'static str) {
    #[cfg(not(debug_assertions))]
    let _ = config;
    #[cfg(debug_assertions)]
    if serves_dev_sign_in(config) {
        return (dev::BUTTON, dev::FORM);
    }
    ("", "")
}

/// The runtime half of the gate: the flag, and a process that is in dev mode.
///
/// Both, because either alone is a mistake somebody could make on a
/// deployment — a stray variable in an environment, or a `config.toml` copied
/// from a laptop. The compile half is that this function only exists here.
#[cfg(debug_assertions)]
#[must_use]
fn serves_dev_sign_in(config: &AppConfig) -> bool {
    config.dev && config.mode == crate::config::Mode::Dev
}

/// One form's typed values and whatever went wrong in it.
///
/// Two of these, because the page carries two forms and an error belongs to
/// the one it came from: rendering "that is not a sign-in we can complete"
/// under the register button would be a lie about which attempt failed.
#[derive(Debug, Default)]
struct Fields {
    display_name: String,
    handle: String,
    email: String,
    error_display_name: String,
    error_email: String,
    error_handle: String,
    error_password: String,
    error_form: String,
}

/// The page. Labels, two forms, and whatever went wrong beside the field it
/// went wrong in.
#[derive(Template, WebTemplate)]
#[template(path = "auth.html")]
struct AuthPage {
    theme: &'static str,
    version: String,
    /// Where to go afterwards, already validated as a path on this origin.
    next: String,
    /// Which form the reader was last in, so it opens where they left off.
    focus: &'static str,
    sign_in: Fields,
    register: Fields,
    /// The developer sign-in button, or nothing.
    ///
    /// Markup rather than a flag over a template branch, and that is the
    /// point: a branch leaves its literals in the compiled template whatever
    /// the flag says, so a release binary would carry the words
    /// `action="/auth/dev"` for a route it does not serve. These two strings
    /// live in [`dev`], which a release build does not compile.
    dev_button: &'static str,
    /// The form that button submits — it cannot nest inside the sign-in form.
    dev_form: &'static str,
}

impl AuthPage {
    fn new(state: &AppState, headers: &HeaderMap, next: String) -> Self {
        let markup = dev_markup(&state.config);
        Self {
            theme: theme_for(headers).as_str(),
            version: state.version.to_string(),
            next,
            focus: "sign-in",
            sign_in: Fields::default(),
            register: Fields::default(),
            dev_button: markup.0,
            dev_form: markup.1,
        }
    }

    fn focused(&mut self) -> &mut Fields {
        if self.focus == "register" {
            &mut self.register
        } else {
            &mut self.sign_in
        }
    }

    /// Place a failure on the field it belongs to, or on the form.
    fn refuse(mut self, error: &CommandError) -> (StatusCode, Self) {
        let status = match error {
            CommandError::Kernel(KernelError::Invalid(invalid)) => {
                let message = invalid.to_string();
                let field = decline::field_of(invalid);
                let fields = self.focused();
                match field {
                    "display name" => fields.error_display_name = message,
                    "email" => fields.error_email = message,
                    "handle" => fields.error_handle = message,
                    "password" => fields.error_password = message,
                    _ => fields.error_form = message,
                }
                StatusCode::UNPROCESSABLE_ENTITY
            }
            CommandError::Malformed(_) => {
                self.focused().error_form = "that form could not be read".to_owned();
                StatusCode::UNPROCESSABLE_ENTITY
            }
            // The uniform refusal, and it stays uniform: a wrong password, an
            // address nobody holds and a disabled identity are one sentence.
            _ => {
                self.focused().error_form = "that did not work".to_owned();
                StatusCode::FORBIDDEN
            }
        };
        (status, self)
    }
}

/// What `GET /auth` reads off its own query.
#[derive(Debug, Deserialize)]
struct Next {
    next: Option<String>,
    /// Show the form even to somebody who is already signed in.
    ///
    /// One caller: the OpenID authorization endpoint, for `prompt=login` and
    /// for a `max_age` this session is older than. Without it that endpoint
    /// loops — it sends the reader here, this page sends a member straight on,
    /// and the endpoint asks for a fresh authentication again, forever.
    ///
    /// It is not a way to sign in as somebody else while signed in: the form
    /// posts to `/auth/sign-in` like any other, which mints a new session and
    /// hands back a new cookie.
    reauth: Option<String>,
    /// The address to prefill, from the authorization endpoint's `login_hint`.
    /// A suggestion in a field the reader may overwrite, and nothing else —
    /// it proves nothing and it is not remembered.
    email: Option<String>,
}

async fn page(
    State(state): State<AppState>,
    session: Visitor,
    Query(query): Query<Next>,
    headers: HeaderMap,
) -> Response {
    let next = next::validate(query.next.as_deref());
    let reauth = query.reauth.is_some();
    if !reauth && matches!(session.principal, Principal::Member { .. }) {
        return see_other(&next, None, 0);
    }
    let mut page = AuthPage::new(&state, &headers, next);
    if let Some(email) = query.email.as_deref().map(str::trim)
        && !email.is_empty()
    {
        page.sign_in.email = email.to_owned();
    }
    accept_hint(page.into_response())
}

#[derive(Debug, Deserialize)]
struct RegisterForm {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    handle: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    next: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    _: SameOrigin,
    headers: HeaderMap,
    Form(form): Form<RegisterForm>,
) -> Response {
    let next = next::validate(form.next.as_deref());
    let args = Register {
        display_name: form.display_name.clone(),
        // Empty means the reader let the prefill stand and their script did
        // not run: the suggestion the field would have carried is the one the
        // server uses, so a form that arrives without JavaScript registers.
        handle: if form.handle.trim().is_empty() {
            rn_kernel::oidc::handle::suggest(&form.email)
        } else {
            form.handle.clone()
        },
        email: form.email.clone(),
        password: form.password,
    };
    match cmd::invoke(&state, Principal::Anonymous, &args).await {
        Ok(executed) => {
            let offset = executed.committed.offset;
            see_other(&next, executed.cookie(state.config.mode), offset)
        }
        Err(error) => {
            let mut page = AuthPage::new(&state, &headers, next);
            page.focus = "register";
            // The password is never echoed back into the document; the two
            // fields that are not secrets are, so a typo is a correction
            // rather than a retype.
            page.register.display_name = form.display_name;
            page.register.handle = form.handle;
            page.register.email = form.email;
            let (status, page) = page.refuse(&error);
            (status, page).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct SignInForm {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    next: Option<String>,
}

async fn sign_in(
    State(state): State<AppState>,
    _: SameOrigin,
    headers: HeaderMap,
    Form(form): Form<SignInForm>,
) -> Response {
    let next = next::validate(form.next.as_deref());
    let args = SignIn {
        email: form.email.clone(),
        password: form.password,
    };
    match cmd::invoke(&state, Principal::Anonymous, &args).await {
        Ok(executed) => {
            let offset = executed.committed.offset;
            see_other(&next, executed.cookie(state.config.mode), offset)
        }
        Err(error) => {
            let mut page = AuthPage::new(&state, &headers, next);
            // An empty address is worth naming; a wrong one is not.
            if form.email.trim().is_empty() {
                page.sign_in.error_email = Invalid::Missing("email").to_string();
            }
            page.sign_in.email = form.email;
            let (status, page) = page.refuse(&error);
            (status, page).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct SignOutForm {
    next: Option<String>,
}

async fn sign_out(
    State(state): State<AppState>,
    _: SameOrigin,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<SignOutForm>,
) -> Response {
    // Where a sign-out lands is the landing page, not the page they were on:
    // that page is one they no longer hold.
    let next = next::validate(form.next.as_deref().or(Some("/")));
    match cmd::invoke(&state, session.principal, &SignOut {}).await {
        Ok(executed) => {
            session::forget(&state, &headers);
            let offset = executed.committed.offset;
            see_other(&next, executed.cookie(state.config.mode), offset)
        }
        Err(error) => error.response(),
    }
}

/// A `303`, so the browser turns a post into a get and the back button does
/// not offer to send the form again.
///
/// It carries the offset the command landed at
/// (`api::cmd::confirmed::OFFSET_HEADER`), which a redirect has no body to
/// say it in. That is what lets a caller register on one node and immediately
/// command another without being declined for a person that node has not
/// applied yet.
pub(crate) fn see_other(
    location: &str,
    cookie: Option<String>,
    offset: rn_kernel::Offset,
) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response();
    if let Some(cookie) = cookie.and_then(|value| value.parse().ok()) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    cmd::stamp_offset(&mut response, offset);
    response
}

/// Ask for the colour-scheme hint, and say the document depends on it, so the
/// *next* navigation is server-rendered in the right theme.
fn accept_hint(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        "accept-ch",
        header::HeaderValue::from_static("Sec-CH-Prefers-Color-Scheme"),
    );
    headers.insert(
        header::VARY,
        header::HeaderValue::from_static("Sec-CH-Prefers-Color-Scheme, Cookie"),
    );
    response
}
