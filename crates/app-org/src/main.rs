//! `/org` bundle: organization administration.
//!
//! Held by an operator of at least one organization. The tier is the app —
//! there is no navigation filtered by role inside it — so what varies between
//! two readers of this bundle is not which pages they see but which
//! organization they are looking at, which is [`scope`]'s job.

mod audit;
mod bits;
mod documents;
mod groups;
mod invitations;
mod members;
mod minting;
mod overview;
mod rows;
mod scope;

use leptos::prelude::*;
use leptos_router::components::{Route, Routes};
use leptos_router::path;

use rn_api::Tier;
use rn_ui::{Decline, NavItem, Shell};

use scope::{Scoped, Switcher};

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
    scope::provide_scope();
    view! {
        <Shell tier=Tier::Org nav=NAV>
            <Switcher />
            <Routes fallback=Decline>
                <Route
                    path=path!("")
                    view=|| view! { <Scoped view=|org| view! { <overview::Overview org=org /> } /> }
                />
                <Route
                    path=path!("/members")
                    view=|| view! { <Scoped view=|org| view! { <members::Members org=org /> } /> }
                />
                <Route
                    path=path!("/groups")
                    view=|| view! { <Scoped view=|org| view! { <groups::Groups org=org /> } /> }
                />
                <Route
                    path=path!("/documents")
                    view=|| {
                        view! { <Scoped view=|org| view! { <documents::Documents org=org /> } /> }
                    }
                />
                <Route
                    path=path!("/invitations")
                    view=|| {
                        view! {
                            <Scoped view=|org| view! { <invitations::Invitations org=org /> } />
                        }
                    }
                />
                <Route
                    path=path!("/audit")
                    view=|| view! { <Scoped view=|org| view! { <audit::Audit org=org /> } /> }
                />
            </Routes>
        </Shell>
    }
}
