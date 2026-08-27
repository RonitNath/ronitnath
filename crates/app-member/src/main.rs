//! `/app` bundle: what any signed-in person gets.
//!
//! The tier is the app (`docs/kernel/index.html` §Surfaces): everything here is
//! reachable by any member, and nothing here is filtered by a capability —
//! what a page can *do* is decided by the kernel when a command reaches it, and
//! what a page can *see* is decided by the query, which only ever answers about
//! the reader. This file is the route table and nothing else.

mod api;
mod documents;
mod editor;
mod groups;
mod home;
mod identities;
mod invitations;
mod merge;
mod parts;
mod rows;
mod sessions;

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, Shell};

/// The rail, in order.
const NAV: &[NavItem] = &[
    NavItem::new("", "Home"),
    NavItem::new("/identities", "Identities"),
    NavItem::new("/sessions", "Sessions"),
    NavItem::new("/groups", "Groups"),
    NavItem::new("/documents", "Documents"),
    NavItem::new("/invitations", "Invitations"),
    NavItem::new("/merge", "Merge"),
];

fn main() {
    rn_ui::start();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Shell tier=Tier::Member nav=NAV>
            <Routes fallback=Decline>
                <Route path=path!("") view=home::Home />
                <Route path=path!("/identities") view=identities::Identities />
                <Route path=path!("/sessions") view=sessions::Sessions />
                <Route path=path!("/groups") view=groups::Groups />
                <Route path=path!("/documents") view=documents::Documents />
                <Route path=path!("/documents/:id") view=editor::Editor />
                <Route path=path!("/invitations") view=invitations::Invitations />
                <Route path=path!("/merge") view=merge::Merge />
            </Routes>
        </Shell>
    }
}
