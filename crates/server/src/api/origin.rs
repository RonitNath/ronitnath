//! Same-origin, asked of the browser rather than inferred from a header it
//! may not have sent.
//!
//! `Origin` equality is the obvious check and it is wrong here. A page served
//! under `Referrer-Policy: no-referrer` posts its own form with `Origin: null`
//! — so an equality check refuses the site's own forms while a cross-site post
//! that happens to omit the header sails through. `Sec-Fetch-Site` is the
//! browser's own statement about the relationship between the initiator and
//! the target, it is forbidden to script, and it arrives on every request a
//! current browser makes.
//!
//! Two values pass. `same-origin` is the site's own fetch or form. `none` is a
//! user-initiated navigation with no initiator at all — typing the URL,
//! opening a bookmark — which no other site can cause. `cross-site`,
//! `same-site` and an absent header all fail: a request that will not say
//! where it came from does not get to write.

use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use axum::response::Response;

/// The header a browser states its initiator relationship in.
pub const HEADER: &str = "sec-fetch-site";

/// Whether this request may mutate.
#[must_use]
pub fn may_mutate(headers: &HeaderMap) -> bool {
    matches!(
        headers.get(HEADER).and_then(|value| value.to_str().ok()),
        Some("same-origin" | "none")
    )
}

/// The permission to write, as an extractor.
///
/// It goes first in a handler's argument list, ahead of the body, because
/// extractors run in order: a cross-site post must be refused before its body
/// is parsed, not after — otherwise the answer to "may I write here" is a
/// `415` about the content type, which is both the wrong answer and a
/// different one from the refusal every other route gives.
#[derive(Debug, Clone, Copy)]
pub struct SameOrigin;

impl<S: Send + Sync> FromRequestParts<S> for SameOrigin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        if may_mutate(&parts.headers) {
            Ok(Self)
        } else {
            Err(crate::api::decline::forbidden())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn with(site: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, HeaderValue::from_str(site).unwrap());
        headers
    }

    #[test]
    fn the_sites_own_forms_and_a_typed_url_may_write() {
        assert!(may_mutate(&with("same-origin")));
        assert!(may_mutate(&with("none")));
    }

    #[test]
    fn a_sibling_subdomain_and_a_stranger_may_not() {
        assert!(!may_mutate(&with("same-site")));
        assert!(!may_mutate(&with("cross-site")));
    }

    #[test]
    fn a_request_that_will_not_say_where_it_came_from_may_not() {
        assert!(!may_mutate(&HeaderMap::new()));
        assert!(!may_mutate(&with("")));
        assert!(!may_mutate(&with("SAME-ORIGIN")), "the value is lowercase");
    }
}
