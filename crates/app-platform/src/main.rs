//! `/platform` bundle: platform operators only.
//!
//! The surface that replaces the old operator console, and an audience of one
//! today. Twelve pages, and most of them are the same shape: a live table of
//! the whole deployment, and a panel about the row being looked at. Two are
//! not — the deployment screen is readings rather than rows, and a person has
//! a page of their own, because what the system knows about a human is nine
//! lists and nine lists do not fit in a panel.
//!
//! The tier *is* the app (`docs/kernel/index.html` §Surfaces), so nothing here
//! is filtered by capability — a reader who can load this bundle holds
//! `platform:* #operator`, and every query re-reads that relation anyway. What
//! the server refuses, the bundle shows refused; it never hides a control to
//! imply an authorisation it does not have.

mod audit;
mod clients;
mod deployment;
mod grants;
mod identities;
mod keys;
mod matches;
mod operators;
mod panel;
mod parties;
mod person;
mod resources;
mod rows;
mod sessions;

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, Shell};

/// The rail, in order: what the deployment is, who may administer it, who
/// exists, how they signed up, who is here now, what happened, what is
/// unresolved, what is owned, what is granted without signing in, and what
/// this issuer has registered.
const NAV: &[NavItem] = &[
    NavItem::new("/deployment", "Deployment"),
    NavItem::new("/operators", "Operators"),
    NavItem::new("", "Parties"),
    NavItem::new("/identities", "Identities"),
    NavItem::new("/sessions", "Sessions"),
    NavItem::new("/audit", "Audit"),
    NavItem::new("/matches", "Matches"),
    NavItem::new("/resources", "Resources"),
    NavItem::new("/links", "Invitations"),
    NavItem::new("/consents", "Consents"),
    NavItem::new("/clients", "Clients"),
    NavItem::new("/keys", "Keys"),
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
                <Route path=path!("/parties/:id") view=person::PersonPage />
                <Route path=path!("/identities") view=identities::Identities />
                <Route path=path!("/sessions") view=sessions::Sessions />
                <Route path=path!("/audit") view=audit::AuditLog />
                <Route path=path!("/matches") view=matches::Matches />
                <Route path=path!("/resources") view=resources::Resources />
                <Route path=path!("/links") view=grants::Links />
                <Route path=path!("/consents") view=grants::Consents />
                <Route path=path!("/clients") view=clients::Clients />
                <Route path=path!("/keys") view=keys::Keys />
                <Route path=path!("/operators") view=operators::Operators />
                <Route path=path!("/deployment") view=deployment::Deployment />
                // The node's own account of itself is a section of the
                // deployment screen now, and the path it used to have still
                // resolves rather than falling through to the decline.
                <Route path=path!("/cluster") view=deployment::Deployment />
            </Routes>
        </Shell>
    }
}
