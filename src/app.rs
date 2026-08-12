use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{Route, Router, Routes},
};

// The nav table is decided from capabilities, which only the server build has.
#[cfg(feature = "ssr")]
use crate::auth::{Capability, CapabilitySet};
use crate::starscape::{CityLabel, MiniGlobe, Starscape};

const THEME_CSS: &str = r#"
:root { color-scheme: dark; --bg: oklch(0.06 0.005 240); --fg: oklch(0.96 0.002 80); --hero-fg: oklch(0.63 0.235 27); }
@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) { color-scheme: light; --bg: oklch(0.97 0.003 240); --fg: oklch(0.15 0.010 240); --hero-fg: oklch(0.80 0.135 80); }
}
:root[data-theme="light"] { color-scheme: light; --bg: oklch(0.97 0.003 240); --fg: oklch(0.15 0.010 240); --hero-fg: oklch(0.80 0.135 80); }
:root[data-theme="dark"] { color-scheme: dark; --bg: oklch(0.06 0.005 240); --fg: oklch(0.96 0.002 80); --hero-fg: oklch(0.63 0.235 27); }
html { background: var(--bg); color: var(--fg); }
"#;

const THEME_JS: &str = r#"
(function () {
  try {
    var saved = localStorage.getItem("theme");
    var theme = saved === "light" || saved === "dark"
      ? saved
      : matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", theme);
  } catch (e) {
    document.documentElement.setAttribute("data-theme", "dark");
  }
})();
"#;

fn server_now_ms() -> f64 {
    #[cfg(feature = "ssr")]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after 1970")
            .as_millis() as f64
    }
    #[cfg(not(feature = "ssr"))]
    {
        0.0
    }
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <style>{THEME_CSS}</style>
                <script>{THEME_JS}</script>
                <AutoReload options=options.clone() />
                <HydrationScripts options islands=true/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let epoch_ms = server_now_ms();

    view! {
        <Stylesheet id="leptos" href="/pkg/rn-site.css"/>
        <Stylesheet href="/css/atmosphere.css"/>
        <Stylesheet href="/css/starscape.css"/>
        <Stylesheet href="/css/site.css"/>
        <Title text="Ronit Nath"/>

        <Atmosphere/>
        <Starscape epoch_ms=epoch_ms/>
        <div class="sky-chrome">
            <MiniGlobe epoch_ms=epoch_ms/>
            <CityLabel epoch_ms=epoch_ms/>
        </div>

        <header class="topbar">
            <a href="/" class="icon-button home-button" aria-label="Home">
                <span class="icon-button-glyph" aria-hidden="true">
                    <svg
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M3 10.5 12 3l9 7.5"></path>
                        <path d="M5.5 9.5V20h13V9.5"></path>
                    </svg>
                </span>
            </a>
            <div class="auth">
                <ThemeToggle/>
                {nav_links()}
            </div>
        </header>

        <Router>
            <main class="site-main">
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=StaticSegment("auth") view=AuthPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// A top-bar destination and what a session must hold to be offered it.
///
/// `requires: None` is a page open to everyone. These are server-rendered pages
/// behind real guards, not Leptos routes, so this table decides only what is
/// *offered*: [`crate::auth::guard::require_capability`] re-checks on every
/// request, and no amount of wrong here can widen what a session may reach.
#[cfg(feature = "ssr")]
struct NavLink {
    href: &'static str,
    label: &'static str,
    requires: Option<Capability>,
}

#[cfg(feature = "ssr")]
const NAV_LINKS: &[NavLink] = &[
    NavLink {
        href: "/protected",
        label: "Protected",
        requires: Some(Capability::TestAuth),
    },
    NavLink {
        href: "/manage",
        label: "Manage",
        requires: Some(Capability::Manage),
    },
    NavLink {
        href: "/auth",
        label: "Authenticate",
        requires: None,
    },
];

/// The nav entries a holder of `capabilities` may open. `None` is anonymous.
///
/// A link the caller cannot follow is not hidden with CSS or dropped on the
/// client — it is never rendered, so the markup carries no evidence that
/// `/manage` exists. Absent capabilities mean absent links, so a missing
/// session layer degrades to the signed-out nav rather than to an open one.
#[cfg(feature = "ssr")]
fn visible_nav(capabilities: Option<&CapabilitySet>) -> impl Iterator<Item = &'static NavLink> {
    NAV_LINKS.iter().filter(move |link| match link.requires {
        None => true,
        Some(capability) => capabilities.is_some_and(|held| held.has(capability)),
    })
}

/// The nav for this request, from the session
/// [`crate::auth::guard::attach_session`] resolved before the render.
#[cfg(feature = "ssr")]
fn nav_links() -> leptos::prelude::AnyView {
    use crate::auth::SessionContext;

    let session = use_context::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<SessionContext>().cloned());

    visible_nav(session.as_ref().map(|session| &session.capabilities))
        .map(|link| {
            view! {
                // Plain anchors, not <A>: a full navigation is what puts the
                // request through the guard middleware.
                <a href=link.href class="auth-link">
                    {link.label}
                </a>
            }
        })
        .collect_view()
        .into_any()
}

/// The nav is server-rendered chrome; the wasm bundle never builds it.
#[cfg(not(feature = "ssr"))]
fn nav_links() -> leptos::prelude::AnyView {
    ().into_any()
}

#[component]
fn Atmosphere() -> impl IntoView {
    view! {
        <div class="starfield" aria-hidden="true">
            <div class="stars-dim"></div>
            <div class="stars-med"></div>
            <div class="stars-bright"></div>
        </div>
        <div class="nebula" aria-hidden="true"></div>
    }
}

/// Flip `data-theme` on <html>, persist the choice, and ask the sky for a
/// frame — under reduced motion the starscape does not repaint on its own.
fn flip_theme() {
    let Some(root) = document().document_element() else {
        return;
    };
    let current = root.get_attribute("data-theme").unwrap_or_default();
    let next = if current == "light" { "dark" } else { "light" };
    let _ = root.set_attribute("data-theme", next);
    #[cfg(feature = "hydrate")]
    if let Ok(event) = web_sys::Event::new("starscape-redraw") {
        let _ = window().dispatch_event(&event);
    }
    #[cfg(feature = "hydrate")]
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item("theme", next);
    }
}

#[island]
fn ThemeToggle() -> impl IntoView {
    let toggle = move |_| flip_theme();

    view! {
        <button class="theme-toggle" aria-label="Toggle color theme" on:click=toggle>
            <span class="theme-toggle-icon" aria-hidden="true">
                <svg
                    class="theme-icon-sun"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                >
                    <circle cx="12" cy="12" r="4"></circle>
                    <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"></path>
                </svg>
                <svg
                    class="theme-icon-moon"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
                </svg>
            </span>
        </button>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <section class="viewport-center home-hero">
            <div class="home-card">
                <h1>"Ronit Nath"</h1>
                <p class="tagline">
                    "My company: "
                    <a
                        class="company-link"
                        href="https://isoastra.com"
                        rel="noopener"
                        target="_blank"
                    >
                        "Isoastra"
                    </a>
                </p>
                <ul class="social-links">
                    <li>
                        <a
                            href="https://github.com/RonitNath"
                            rel="me noopener"
                            target="_blank"
                        >
                            "GitHub"
                        </a>
                    </li>
                    <li>
                        <a
                            href="https://instagram.com/ronit_nath"
                            rel="me noopener"
                            target="_blank"
                        >
                            "Instagram"
                        </a>
                    </li>
                    <li>
                        <a
                            href="https://linkedin.com/in/ronitn"
                            rel="me noopener"
                            target="_blank"
                        >
                            "LinkedIn"
                        </a>
                    </li>
                    <li>
                        <a href="mailto:ronit@isoastra.com">"Email"</a>
                    </li>
                </ul>
            </div>
        </section>
    }
}

#[component]
fn AuthPage() -> impl IntoView {
    view! {
        <div class="viewport-center viewport-center-stack">
            <h1 class="home-title text-2xl font-semibold">"Authenticate"</h1>
            <LoginForm/>
            {dev_bypass_form()}
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod nav_tests {
    use super::*;
    use crate::auth::MembershipRole;

    fn labels(capabilities: Option<&CapabilitySet>) -> Vec<&'static str> {
        visible_nav(capabilities).map(|link| link.label).collect()
    }

    #[test]
    fn anonymous_is_offered_only_the_open_pages() {
        assert_eq!(labels(None), ["Authenticate"]);
    }

    #[test]
    fn a_fresh_registrant_is_not_offered_manage() {
        // Registration mints Owner, which implies `test-auth` and not `manage`,
        // so the account that just signed up must not be shown a door it would
        // be turned away from.
        let owner = CapabilitySet::resolve(MembershipRole::Owner, std::iter::empty());
        assert_eq!(labels(Some(&owner)), ["Protected", "Authenticate"]);
    }

    #[test]
    fn granting_manage_adds_its_link_and_nothing_else() {
        let admin = CapabilitySet::resolve(MembershipRole::Admin, std::iter::empty());
        assert_eq!(
            labels(Some(&admin)),
            ["Protected", "Manage", "Authenticate"]
        );
    }

    #[test]
    fn every_gated_link_names_a_capability_a_role_can_actually_hold() {
        // A link gated on a capability no role implies and no grant creates
        // would be permanently invisible — dead chrome that reads as a bug in
        // the guard rather than as a typo here.
        for link in NAV_LINKS {
            let Some(required) = link.requires else {
                continue;
            };
            assert!(
                Capability::ALL.contains(&required),
                "{} requires a capability outside Capability::ALL",
                link.href
            );
        }
    }
}

/// The dev-bypass button, or nothing.
///
/// Two gates, mirroring the route in `auth::routes`: the form only exists in
/// a debug-`ssr` build (`cfg`), and only renders when the process runs in
/// explicit `dev` mode (runtime). A release binary contains neither this form
/// nor the endpoint it posts to. Not an island — it is server-rendered or it
/// is absent, so the wasm bundle carries no trace of it either.
#[cfg(all(feature = "ssr", debug_assertions))]
fn dev_bypass_form() -> Option<leptos::prelude::AnyView> {
    use crate::operations::config::Mode;

    use_context::<crate::auth::AuthState>()
        .filter(|state| state.mode == Mode::Dev)
        .map(|_| {
            view! {
                <form
                    method="post"
                    action="/api/auth/dev-bypass"
                    class="flex w-full max-w-sm flex-col gap-1"
                >
                    <button
                        type="submit"
                        class="rounded border px-4 py-2 text-sm"
                        style="border-color: color-mix(in oklab, var(--fg) 25%, transparent); background: transparent; color: var(--fg-muted); cursor: pointer"
                    >
                        "Enter as dev admin"
                    </button>
                    <p class="text-sm" style="color: var(--fg-subtle)">
                        "Debug build in dev mode only — mints a "
                        <code>"manage"</code>
                        "-capable session without a credential."
                    </p>
                </form>
            }
            .into_any()
        })
}

#[cfg(not(all(feature = "ssr", debug_assertions)))]
fn dev_bypass_form() -> Option<leptos::prelude::AnyView> {
    None
}

/// The only thing this endpoint says on any failure.
///
/// Every decline uses the same words: no field-level hint, no distinction
/// between an address that is unknown and a password that is wrong, nothing to
/// enumerate and nothing to infer about what does or does not exist on the
/// other side.
const AUTH_REJECTED: &str = "Authentication failed.";

/// Verify a credential, install a session cookie, and redirect to `/protected`.
///
/// A decline is *content*, not a transport error: this returns `Ok` and HTTP
/// 200 rather than a `ServerFnError`, which the framework would render as a
/// 500. A 500 would break the page for anyone without JS, get counted as a
/// server fault by anything watching, and announce that the route is broken
/// rather than that it declined you. A database fault is deliberately folded
/// into the same decline — the alternative is an error branch whose wording
/// distinguishes it. On success nothing renders: the `Set-Cookie` and the
/// redirect are the whole response.
///
/// The store runs the email-verification gate and spends an argon2 verify on
/// unknown addresses, so failures are indistinguishable by timing too. The
/// endpoint is pinned so the URL is stable across builds; the generated
/// default appends a hash of the function signature.
#[server(endpoint = "authenticate")]
async fn authenticate(email: String, password: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use axum::http::header;

        use crate::auth::{AuthState, session, store};

        // Provided by `main` to both the server-fn handler and the SSR
        // renderer; absence is a wiring bug, not a runtime condition.
        let state = expect_context::<AuthState>();

        let user_agent = use_context::<axum::http::request::Parts>().and_then(|parts| {
            parts
                .headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        });

        match store::authenticate(&state.db, &email, &password).await {
            Ok(Some(registration)) => {
                match store::create_session(&state.db, &registration, user_agent).await {
                    Ok(token) => {
                        let response = expect_context::<leptos_axum::ResponseOptions>();
                        let cookie = session::set_cookie_header(&token, state.cookie_security);
                        response.insert_header(
                            header::SET_COOKIE,
                            cookie.parse().expect("cookie header is ascii"),
                        );
                        leptos_axum::redirect("/protected", false);
                    }
                    Err(err) => {
                        tracing::warn!(%err, "session creation failed after a valid credential");
                    }
                }
            }
            Ok(None) => {
                tracing::debug!("authenticate declined");
            }
            Err(err) => {
                tracing::warn!(%err, "authenticate errored; declining");
            }
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        drop((email, password));
    }
    Ok(())
}

#[island]
fn LoginForm() -> impl IntoView {
    let submit = ServerAction::<Authenticate>::new();
    // Any settled outcome renders the same constant — deliberately including a
    // network or decoding failure. Rendering the error's own `Display` would
    // leak the framework's "error running server function: ..." wrapper and
    // would let a failed request be told apart from a declined one.
    let declined = move || submit.value().get().is_some();

    view! {
        <ActionForm action=submit attr:class="flex w-full max-w-sm flex-col gap-4">
            <label class="flex flex-col gap-1 text-sm" style="color: var(--fg)">
                "Email"
                <input
                    type="email"
                    name="email"
                    required
                    class="rounded border px-3 py-2"
                    style="border-color: color-mix(in oklab, var(--fg) 25%, transparent); background: color-mix(in oklab, var(--bg) 70%, transparent); color: var(--fg)"
                />
            </label>
            <label class="flex flex-col gap-1 text-sm" style="color: var(--fg)">
                "Password"
                <input
                    type="password"
                    name="password"
                    required
                    class="rounded border px-3 py-2"
                    style="border-color: color-mix(in oklab, var(--fg) 25%, transparent); background: color-mix(in oklab, var(--bg) 70%, transparent); color: var(--fg)"
                />
            </label>
            <button
                type="submit"
                class="rounded px-4 py-2 text-sm font-medium"
                style="background: var(--fg); color: var(--bg)"
                disabled=move || submit.pending().get()
            >
                "Log in"
            </button>
            <Show when=declined>
                <p class="text-sm" role="alert" style="color: var(--fg-subtle)">
                    {AUTH_REJECTED}
                </p>
            </Show>
        </ActionForm>
    }
}
