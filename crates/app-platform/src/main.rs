//! `/platform` bundle: platform operators only.
//!
//! The surface that replaces the old `/manage`: an audience of one today. The
//! pages land with U4; this is the route table and the chrome.

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, PageHead, Shell};

/// The rail, in order.
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
                <Route path=path!("") view=|| page("Parties") />
                <Route path=path!("/identities") view=|| page("Identities") />
                <Route path=path!("/sessions") view=|| page("Sessions") />
                <Route path=path!("/audit") view=|| page("Audit") />
                <Route path=path!("/matches") view=|| page("Matches") />
                <Route path=path!("/resources") view=|| page("Resources") />
                <Route path=path!("/cluster") view=|| page("Cluster") />
            </Routes>
        </Shell>
    }
}

fn page(title: &'static str) -> impl IntoView {
    view! { <PageHead title=title /> }
}
