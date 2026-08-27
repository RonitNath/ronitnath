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
use crate::presence::theme_for;
use crate::state::AppState;

pub use session::{Session, Visitor};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth", get(page))
        .route("/auth/register", post(register))
        .route("/auth/sign-in", post(sign_in))
        .route("/auth/sign-out", post(sign_out))
}

/// One form's typed values and whatever went wrong in it.
///
/// Two of these, because the page carries two forms and an error belongs to
/// the one it came from: rendering "that is not a sign-in we can complete"
/// under the register button would be a lie about which attempt failed.
#[derive(Debug, Default)]
struct Fields {
    display_name: String,
    email: String,
    error_display_name: String,
    error_email: String,
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
}

impl AuthPage {
    fn new(state: &AppState, headers: &HeaderMap, next: String) -> Self {
        Self {
            theme: theme_for(headers).as_str(),
            version: state.version.to_string(),
            next,
            focus: "sign-in",
            sign_in: Fields::default(),
            register: Fields::default(),
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

#[derive(Debug, Deserialize)]
struct Next {
    next: Option<String>,
}

async fn page(
    State(state): State<AppState>,
    session: Visitor,
    Query(query): Query<Next>,
    headers: HeaderMap,
) -> Response {
    let next = next::validate(query.next.as_deref());
    if matches!(session.principal, Principal::Member { .. }) {
        return see_other(&next, None);
    }
    accept_hint(AuthPage::new(&state, &headers, next).into_response())
}

#[derive(Debug, Deserialize)]
struct RegisterForm {
    #[serde(default)]
    display_name: String,
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
        email: form.email.clone(),
        password: form.password,
    };
    match cmd::invoke(&state, Principal::Anonymous, &args).await {
        Ok(executed) => see_other(&next, executed.cookie(state.config.mode)),
        Err(error) => {
            let mut page = AuthPage::new(&state, &headers, next);
            page.focus = "register";
            // The password is never echoed back into the document; the two
            // fields that are not secrets are, so a typo is a correction
            // rather than a retype.
            page.register.display_name = form.display_name;
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
        Ok(executed) => see_other(&next, executed.cookie(state.config.mode)),
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
            see_other(&next, executed.cookie(state.config.mode))
        }
        Err(error) => error.response(),
    }
}

/// A `303`, so the browser turns a post into a get and the back button does
/// not offer to send the form again.
fn see_other(location: &str, cookie: Option<String>) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response();
    if let Some(cookie) = cookie.and_then(|value| value.parse().ok()) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
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
