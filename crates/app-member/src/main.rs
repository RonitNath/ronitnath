//! `/app` bundle: what any signed-in person gets.
//!
//! The tier is the app: everything here is reachable by any member, and the
//! pages themselves land with U2. This is the route table and the chrome.

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, PageHead, Shell};

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
                <Route path=path!("") view=|| page("Home") />
                <Route path=path!("/identities") view=|| page("Identities") />
                <Route path=path!("/sessions") view=|| page("Sessions") />
                <Route path=path!("/groups") view=|| page("Groups") />
                <Route path=path!("/documents") view=|| page("Documents") />
                <Route path=path!("/invitations") view=|| page("Invitations") />
                <Route path=path!("/merge") view=|| page("Merge") />
            </Routes>
        </Shell>
    }
}

fn page(title: &'static str) -> impl IntoView {
    view! { <PageHead title=title /> }
}
