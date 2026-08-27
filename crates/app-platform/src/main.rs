//! `/platform` bundle: platform operators only.
//!
//! The surface that replaces the old operator console, and an audience of one
//! today. Seven pages, and each of them is the same shape: a live table of the
//! whole deployment, and a panel about the row being looked at.
//!
//! The tier *is* the app (`docs/kernel/index.html` §Surfaces), so nothing here
//! is filtered by capability — a reader who can load this bundle holds
//! `platform:* #operator`, and every query re-reads that relation anyway. What
//! the server refuses, the bundle shows refused; it never hides a control to
//! imply an authorisation it does not have.

mod audit;
mod cluster;
mod identities;
mod matches;
mod panel;
mod parties;
mod resources;
mod rows;
mod sessions;

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, Shell};

/// The rail, in order: who exists, how they signed up, who is here now, what
/// happened, what is unresolved, what is owned, and what the node is.
const NAV: &[NavItem] = &[
    NavItem::new("", "Parties"),
    NavItem::new("/identities", "Identities"),
    NavItem::new("/sessions", "Sessions"),
    NavItem::new("/audit", "Audit"),
    NavItem::new("/matches", "Matches"),
    NavItem::new("/resources", "Resources"),
    NavItem::new("/cluster", "Cluster"),
];

fn main() {
    rn_ui::start();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Shell tier=Tier::Platform nav=NAV>
            <Routes fallback=Decline>
                <Route path=path!("") view=parties::Parties />
                <Route path=path!("/identities") view=identities::Identities />
                <Route path=path!("/sessions") view=sessions::Sessions />
                <Route path=path!("/audit") view=audit::AuditLog />
                <Route path=path!("/matches") view=matches::Matches />
                <Route path=path!("/resources") view=resources::Resources />
                <Route path=path!("/cluster") view=cluster::Cluster />
            </Routes>
        </Shell>
    }
}
