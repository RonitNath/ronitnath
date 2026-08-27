//! `/org` bundle: organization administration.
//!
//! Held by an operator of at least one organization. The pages land with U3;
//! this is the route table and the chrome.

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, PageHead, Shell};

/// The rail, in order.
const NAV: &[NavItem] = &[
    NavItem::new("", "Overview"),
    NavItem::new("/members", "Members"),
    NavItem::new("/groups", "Groups"),
    NavItem::new("/documents", "Documents"),
    NavItem::new("/invitations", "Invitations"),
    NavItem::new("/audit", "Audit"),
];

fn main() {
    rn_ui::start();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Shell tier=Tier::Org nav=NAV>
            <Routes fallback=Decline>
                <Route path=path!("") view=|| page("Overview") />
                <Route path=path!("/members") view=|| page("Members") />
                <Route path=path!("/groups") view=|| page("Groups") />
                <Route path=path!("/documents") view=|| page("Documents") />
                <Route path=path!("/invitations") view=|| page("Invitations") />
                <Route path=path!("/audit") view=|| page("Audit") />
            </Routes>
        </Shell>
    }
}

fn page(title: &'static str) -> impl IntoView {
    view! { <PageHead title=title /> }
}
