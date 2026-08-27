//! The tier shell: a rail when there is room for one, a drawer when there is
//! not, and the same navigation model either way.
//!
//! The tier *is* the app (kernel report §Surfaces): there is no nav filtered by
//! capability inside a bundle, and the scope switcher offers only the tiers
//! `whoami` says this principal holds. Everything the chrome draws comes from
//! that one DTO.

mod theme;

use leptos::html::Button;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{A, Router};

use rn_api::{Tier, Whoami};

pub use theme::{Mode, ThemeToggle};

use crate::api;

/// One entry in the rail. `path` is relative to the tier's base, so a bundle
/// names its pages once and the shell builds the hrefs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavItem {
    /// The path under the tier base — `""` for the tier's home.
    pub path: &'static str,
    /// A human word for the page.
    pub label: &'static str,
}

impl NavItem {
    /// One entry: where it goes under the tier base, and what it is called.
    pub const fn new(path: &'static str, label: &'static str) -> Self {
        Self { path, label }
    }
}

/// The URL prefix a tier's bundle is mounted at.
pub const fn tier_base(tier: Tier) -> &'static str {
    match tier {
        Tier::Member => "/app",
        Tier::Org => "/org",
        Tier::Platform => "/platform",
    }
}

/// What the scope switcher calls a tier.
pub const fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Member => "Personal",
        Tier::Org => "Organization",
        Tier::Platform => "Platform",
    }
}

/// The `whoami` this bundle is running as, once it has arrived.
pub fn use_whoami() -> RwSignal<Option<Whoami>> {
    use_context().expect("use_whoami outside a <Shell/>")
}

/// The tier shell. Mount it once, at the root of a bundle, around that
/// bundle's `<Routes/>`.
#[component]
pub fn Shell(
    /// Which bundle this is.
    tier: Tier,
    /// The rail's entries, in order.
    nav: &'static [NavItem],
    /// The bundle's routes.
    children: Children,
) -> impl IntoView {
    let who = RwSignal::new(None::<Whoami>);
    provide_context(who);
    spawn_local(async move {
        if let Ok(whoami) = api::whoami().await {
            who.set(Some(whoami));
        }
    });

    view! {
        <Router base=tier_base(tier)>
            <Chrome tier=tier nav=nav>
                {children()}
            </Chrome>
        </Router>
    }
}

/// Everything inside the router: the rail, the narrow-viewport drawer, and the
/// content column.
#[component]
fn Chrome(tier: Tier, nav: &'static [NavItem], children: Children) -> impl IntoView {
    let who = use_whoami();
    let open = RwSignal::new(false);
    let trigger = NodeRef::<Button>::new();

    // Closing always returns focus to the control that opened the drawer.
    let close = move || {
        if open.get_untracked() {
            open.set(false);
            if let Some(button) = trigger.get_untracked() {
                let _ = button.focus();
            }
        }
    };

    let _escape = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" {
            close();
        }
    });

    let base = tier_base(tier);

    let links = nav
        .iter()
        .map(|item| {
            let href = format!("{base}{}", item.path);
            view! {
                <A href=href exact=item.path.is_empty()>
                    {item.label}
                </A>
            }
        })
        .collect_view();

    let scopes = move || {
        who.get()
            .map(|whoami| {
                whoami
                    .tiers
                    .iter()
                    .map(|held| {
                        let current = (*held == tier).then_some("page");
                        view! {
                            <a href=tier_base(*held) aria-current=current>
                                {tier_label(*held)}
                            </a>
                        }
                    })
                    .collect_view()
            })
            .into_any()
    };

    // Who you are leads; the registration you signed in with is a line under
    // it, and only when it says something the name does not — a resolved
    // identity is named by its person, so repeating it would be the same words
    // twice. The organization you are acting as sits under both.
    let identity = move || {
        who.get().map(|whoami| {
            let name = whoami.person.as_ref().map_or_else(
                || whoami.identity.display.clone(),
                |person| person.display.clone(),
            );
            let registration =
                (whoami.identity.display != name).then_some(whoami.identity.display.clone());
            let acting = (whoami.acting_as.display != name).then_some(whoami.acting_as.display);
            view! {
                <span class="who">{name}</span>
                <span class="registration">{registration}</span>
                <span class="acting">{acting}</span>
            }
        })
    };

    view! {
        <div class="shell" data-drawer=move || if open.get() { "open" } else { "shut" }>
            <header class="topbar">
                <button
                    type="button"
                    node_ref=trigger
                    aria-expanded=move || open.get().to_string()
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    "Sections"
                </button>
                <span class="where">{tier_label(tier)}</span>
            </header>
            <nav class="rail" aria-label="Sections">
                <div class="rail-identity">{identity}</div>
                <div class="rail-scope">{scopes}</div>
                <div class="rail-nav" on:click=move |_| close()>
                    {links}
                </div>
                <div class="rail-foot">
                    <ThemeToggle />
                    <form method="post" action="/auth/sign-out">
                        <button type="submit">"Sign out"</button>
                    </form>
                </div>
            </nav>
            <div class="scrim" on:click=move |_| close()></div>
            <main class="content">{children()}</main>
        </div>
    }
}

/// A page's heading. The only thing above it is the rail.
#[component]
pub fn PageHead(
    /// The page's identity, as an `h1`.
    #[prop(into)]
    title: String,
    /// Anything that belongs on the same line — a count, a commit button.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="page-head">
            <h1>{title}</h1>
            {children.map(|children| children())}
        </div>
    }
}
