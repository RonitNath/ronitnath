//! One refusal, two status codes, and the one thing a caller is told instead.
//!
//! [`rn_kernel::Decline`] carries nothing, and neither does the body here: the
//! difference between "no such document" and "not yours" is precisely the fact
//! an attacker is trying to learn. `403` and `404` therefore return *identical
//! bytes* — the status is chosen so a browser behaves (a shell 404s, an API
//! call 403s) and says nothing more than the browser needs.
//!
//! [`rn_kernel::Invalid`] is the exception, and it is safe to be specific
//! about: it describes the shape of a request the caller itself sent, and a
//! form has to render it next to the field that failed.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rn_api::Decline;
use rn_kernel::{Invalid, KernelError};
use serde::Serialize;

/// One field's complaint, as a form renders it and as JSON carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    /// Which field of the request.
    pub field: &'static str,
    /// What is wrong with it, in the caller's terms.
    pub message: String,
}

/// The `422` body: everything wrong with the request's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Invalidated {
    /// The field complaints, in the order they were found.
    pub invalid: Vec<FieldError>,
}

/// Which field an [`Invalid`] is about.
///
/// Exhaustive on purpose: a new validation failure has to say where it
/// belongs, because a form that cannot place an error renders it nowhere.
#[must_use]
pub fn field_of(invalid: &Invalid) -> &'static str {
    match invalid {
        Invalid::Missing(field) | Invalid::TooLong { field, .. } => field,
        Invalid::NotAnEmail => "email",
        Invalid::PasswordTooShort(_) => "password",
        Invalid::UnsupportedFactor => "kind",
        Invalid::BadHandle => "handle",
    }
}

/// The one refusal, as a `403`.
#[must_use]
pub fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(Decline::default())).into_response()
}

/// The one refusal, as a `404` — same bytes, different status.
#[must_use]
pub fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(Decline::default())).into_response()
}

/// The one refusal, as a `503`: this node could not answer in time.
///
/// A `503` rather than the `408` a generic timeout layer sends, and the
/// difference is the honest one. `408` says the *client* was slow. What this
/// answers is a node that took its whole budget and did not come back — which
/// on a raft cluster means one thing, no quorum to commit with, and that is
/// the server's condition, not the caller's. The body is the uniform decline
/// because a refusal is a refusal: a caller learns that its command did not
/// happen, and nothing about why the cluster is unhappy.
#[must_use]
pub fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, Json(Decline::default())).into_response()
}

/// A sensitive command on a session that has not seen a password lately.
///
/// The one status this surface uses that is not a uniform decline, a `422` or
/// a `409`, and the exception is deliberate: the caller is already inside the
/// platform tier, so nothing is disclosed by telling them, and the body is the
/// same bytes as every other refusal — what carries the information is the
/// status alone, which is the least a caller can be told and still act.
#[must_use]
pub fn reauth() -> Response {
    (StatusCode::UNAUTHORIZED, Json(Decline::default())).into_response()
}

/// A replayed idempotency key carrying a different body.
#[must_use]
pub fn conflict() -> Response {
    (StatusCode::CONFLICT, Json(Decline::default())).into_response()
}

/// A malformed request, with the field complaints a form can render.
#[must_use]
pub fn invalid(invalid: &Invalid) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(Invalidated {
            invalid: vec![FieldError {
                field: field_of(invalid),
                message: invalid.to_string(),
            }],
        }),
    )
        .into_response()
}

/// Every kernel failure, as the response it is allowed to be.
///
/// An [`KernelError::Invariant`] is the one case that is nobody's fault but
/// ours: it is logged loudly and answered with the same refusal as everything
/// else, because a caller can do nothing with it and a `500` would tell an
/// attacker which inputs reach a bug.
#[must_use]
pub fn from_kernel(error: &KernelError) -> Response {
    match error {
        KernelError::Invalid(invalid) => self::invalid(invalid),
        KernelError::Conflict => conflict(),
        KernelError::ReAuthRequired => reauth(),
        KernelError::Decline(_) => forbidden(),
        KernelError::Store(store) => {
            tracing::error!(%store, "the database refused a command");
            forbidden()
        }
        KernelError::Invariant(message) => {
            tracing::error!(invariant = %message, "kernel invariant violated");
            forbidden()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forbidden_and_a_not_found_are_the_same_bytes() {
        let body = serde_json::to_string(&Decline::default()).unwrap();
        assert_eq!(body, r#"{"decline":"declined"}"#);
        assert_eq!(forbidden().status(), StatusCode::FORBIDDEN);
        assert_eq!(not_found().status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn every_validation_failure_names_a_field_a_form_can_point_at() {
        for (invalid, field) in [
            (Invalid::Missing("display name"), "display name"),
            (
                Invalid::TooLong {
                    field: "email",
                    limit: 254,
                },
                "email",
            ),
            (Invalid::NotAnEmail, "email"),
            (Invalid::PasswordTooShort(12), "password"),
            (Invalid::UnsupportedFactor, "kind"),
            (Invalid::BadHandle, "handle"),
        ] {
            assert_eq!(field_of(&invalid), field);
        }
    }

    #[test]
    fn a_kernel_decline_never_becomes_a_server_error() {
        assert_eq!(
            from_kernel(&KernelError::Invariant("bug".into())).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            from_kernel(&KernelError::Conflict).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            from_kernel(&KernelError::from(Invalid::NotAnEmail)).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
