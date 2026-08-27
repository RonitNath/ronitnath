//! The decline page.
//!
//! `403` and `404` are one event in this application: the server sends the
//! same bytes for both so that a caller cannot learn what exists by reading the
//! difference, and the browser shows the same page for the same reason. It
//! renders inside the shell — a reader who took a wrong turn keeps their
//! navigation.

use leptos::prelude::*;

use crate::shell::PageHead;

/// The uniform not-found / not-yours page.
#[component]
pub fn Decline() -> impl IntoView {
    view! {
        <PageHead title="Not available" />
        <div class="decline">
            <p>"This page does not exist, or is not yours to see."</p>
        </div>
    }
}
