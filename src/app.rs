use leptos::prelude::*;
use leptos_meta::{Meta, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    ParamSegment, StaticSegment,
    components::{Route, Router, Routes},
    hooks::use_params_map,
};
use serde::{Deserialize, Serialize};

// The nav table is decided from capabilities, which only the server build has.
#[cfg(feature = "ssr")]
use crate::auth::{Capability, CapabilitySet};
use crate::starscape::{CityLabel, MiniGlobe, StarScapeControls, Starscape};

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

/// Release identity shared by the server-rendered page and hydrated client.
///
/// Assets keep stable URLs and explicitly revalidate. This build-time value is
/// instead used by the realtime release watcher, so a browser can safely
/// reload when it is running an older client than the connected server.
pub const APP_VERSION: &str = match option_env!("SOURCE_GIT_HASH") {
    Some(hash) => hash,
    None => "dev",
};

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
        <Meta name="rn-app-version" content=APP_VERSION/>
        <Stylesheet id="leptos" href="/pkg/rn-site.css"/>
        <Stylesheet href="/css/atmosphere.css"/>
        <Stylesheet href="/css/starscape.css"/>
        <Stylesheet href="/css/site.css"/>
        <Stylesheet href="/css/manage.css"/>
        <Title text="Ronit Nath"/>

        <crate::realtime::RealtimeWatcher/>

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
                    <Route path=StaticSegment("protected") view=ProtectedPage/>
                    <Route path=StaticSegment("manage") view=ManageIndexPage/>
                    <Route
                        path=(StaticSegment("manage"), ParamSegment("model"))
                        view=ManageModelPage
                    />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn ProtectedPage() -> impl IntoView {
    #[cfg(feature = "ssr")]
    let summary = {
        use crate::auth::SessionContext;
        use_context::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<SessionContext>().cloned())
            .map(|session| {
                format!(
                    "identity={} account={} capabilities={}",
                    session.identity_public_id,
                    session.account_public_id,
                    session.capabilities.to_log_string()
                )
            })
            .unwrap_or_else(|| "session unavailable".to_string())
    };
    #[cfg(not(feature = "ssr"))]
    let summary = String::new();

    view! {
        <section class="viewport-center viewport-center-stack">
            <h1 class="home-title text-2xl font-semibold">"Protected"</h1>
            <p>{summary}</p>
        </section>
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManageCard {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub count: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ManageCell {
    Mono(String),
    Text(String),
    Tag(String),
    Reference { label: String, public_id: String },
    Time(String),
    None,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManageTable {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub total: i64,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<ManageCell>>,
}

#[server(endpoint = "manage-index-data")]
async fn manage_index_data() -> Result<Vec<ManageCard>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        require_manage_server_fn().await?;
        let state = expect_context::<crate::auth::AuthState>();
        let mut cards = Vec::with_capacity(crate::manage::DataModel::ALL.len());
        for model in crate::manage::DataModel::ALL {
            cards.push(ManageCard {
                slug: model.slug().to_string(),
                title: model.title().to_string(),
                description: model.description().to_string(),
                count: crate::manage::queries::count(&state.db, *model)
                    .await
                    .map_err(server_error)?,
            });
        }
        Ok(cards)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(endpoint = "manage-model-data")]
async fn manage_model_data(slug: String) -> Result<ManageTable, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        require_manage_server_fn().await?;
        let state = expect_context::<crate::auth::AuthState>();
        let Some(model) = crate::manage::DataModel::parse(&slug) else {
            if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
                response.set_status(axum::http::StatusCode::NOT_FOUND);
            }
            return Err(server_error("no such model"));
        };
        let total = crate::manage::queries::count(&state.db, model)
            .await
            .map_err(server_error)?;
        let table = crate::manage::queries::rows(&state.db, model)
            .await
            .map_err(server_error)?;
        Ok(ManageTable {
            slug: model.slug().to_string(),
            title: model.title().to_string(),
            description: model.description().to_string(),
            total,
            columns: table
                .columns
                .iter()
                .map(|column| (*column).to_string())
                .collect(),
            rows: table
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(ManageCell::from).collect())
                .collect(),
        })
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[cfg(feature = "ssr")]
impl From<crate::manage::queries::Cell> for ManageCell {
    fn from(cell: crate::manage::queries::Cell) -> Self {
        use crate::manage::queries::Cell;
        match cell {
            Cell::Mono(value) => Self::Mono(value),
            Cell::Text(value) => Self::Text(value),
            Cell::Tag(value) => Self::Tag(value),
            Cell::Ref { label, public_id } => Self::Reference { label, public_id },
            Cell::Time(value) => Self::Time(crate::manage::queries::fmt_utc(value)),
            Cell::None => Self::None,
        }
    }
}

#[cfg(feature = "ssr")]
async fn require_manage_server_fn() -> Result<(), ServerFnError> {
    use crate::auth::{Capability, session, store};
    use axum::http::header;

    let state = expect_context::<crate::auth::AuthState>();
    let token = use_context::<axum::http::request::Parts>()
        .and_then(|parts| parts.headers.get(header::COOKIE).cloned())
        .and_then(|value| value.to_str().ok().map(str::to_string))
        .and_then(|cookies| session::token_from_cookie_header(&cookies));
    let Some(token) = token else {
        return Err(server_error("not authenticated"));
    };
    let allowed = store::resolve_session(&state.db, &token)
        .await
        .map_err(server_error)?
        .is_some_and(|session| session.capabilities.has(Capability::Manage));
    if allowed {
        Ok(())
    } else {
        Err(server_error("not permitted"))
    }
}

#[cfg(feature = "ssr")]
fn server_error(message: impl ToString) -> ServerFnError {
    ServerFnError::ServerError(message.to_string())
}

fn manage_shell(active: Option<String>, content: impl IntoView) -> impl IntoView {
    let models = [
        ("identities", "Identities"),
        ("identity-emails", "Identity emails"),
        ("identity-passwords", "Identity passwords"),
        ("accounts", "Accounts"),
        ("account-memberships", "Account memberships"),
        ("membership-capabilities", "Membership capabilities"),
        ("sessions", "Sessions"),
        ("resource-grants", "Resource grants"),
    ];
    view! {
        <section class="manage-page">
            <header class="manage-bar">
                <a class="manage-brand" href="/manage">"Manage"</a>
                <span class="manage-note">"times UTC · latest 200 rows per model"</span>
                <nav class="manage-bar-links">
                    <a href="/protected">"Session"</a>
                    <a href="/">"ronitnath.com"</a>
                </nav>
            </header>
            <div class="manage-frame">
                <nav class="manage-rail" aria-label="Data models">
                    {models.into_iter().map(|(slug, title)| {
                        let class = (active.as_deref() == Some(slug)).then_some("active");
                        view! { <a href=format!("/manage/{slug}") class=class>{title}</a> }
                    }).collect_view()}
                </nav>
                <div class="manage-main">{content.into_any()}</div>
            </div>
        </section>
    }
}

#[island]
fn ManageIndexIsland() -> impl IntoView {
    let data = Resource::new(|| (), |_| manage_index_data());
    install_manage_refetch(data, None);
    manage_shell(
        None,
        view! {
            <Suspense fallback=|| view! { <p>"Loading data models…"</p> }>
                {move || Suspend::new(async move {
                    match data.await {
                        Ok(cards) => view! {
                            <h1>"Data models"</h1>
                            <p class="manage-lede">"Every durable table in the application, read-only. Select a model to inspect its rows."</p>
                            <div class="manage-cards">
                                {cards.into_iter().map(|card| view! {
                                    <a class="manage-card" href=format!("/manage/{}", card.slug)>
                                        <span class="manage-card-head">
                                            <span>{card.title}</span>
                                            <span class="manage-count">{card.count}</span>
                                        </span>
                                        <span class="manage-card-desc">{card.description}</span>
                                    </a>
                                }).collect_view()}
                            </div>
                        }.into_any(),
                        Err(_) => view! { <p>"The data store could not be reached."</p> }.into_any(),
                    }
                })}
            </Suspense>
        },
    )
}

#[component]
fn ManageIndexPage() -> impl IntoView {
    view! { <ManageIndexIsland/> }
}

#[island]
fn ManageModelIsland(slug: String) -> impl IntoView {
    let requested = slug.clone();
    let data = Resource::new(move || requested.clone(), manage_model_data);
    install_manage_refetch(data, Some(slug.clone()));
    manage_shell(
        Some(slug),
        view! {
            <Suspense fallback=|| view! { <p>"Loading rows…"</p> }>
                {move || Suspend::new(async move {
                    match data.await {
                        Ok(table) => render_manage_table(table).into_any(),
                        Err(_) => view! { <h1>"No such model"</h1><p>"The model is unavailable or does not exist."</p> }.into_any(),
                    }
                })}
            </Suspense>
        },
    )
}

#[component]
fn ManageModelPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("model").unwrap_or_default();
    view! { <ManageModelIsland slug=slug()/> }
}

fn render_manage_table(table: ManageTable) -> impl IntoView {
    let shown = table.rows.len() as i64;
    let count = if table.total > shown {
        format!("Showing the latest {shown} of {} rows", table.total)
    } else {
        format!("{} rows", table.total)
    };
    view! {
        <h1>{table.title}</h1>
        <p class="manage-lede">{table.description}</p>
        <p class="manage-count-line">{count}</p>
        <div class="manage-table-scroll" data-reload-state="manage-table">
            <table>
                <thead><tr>{table.columns.into_iter().map(|column| view! { <th>{column}</th> }).collect_view()}</tr></thead>
                <tbody>{table.rows.into_iter().map(|row| view! {
                    <tr>{row.into_iter().map(|cell| view! { <td>{render_manage_cell(cell)}</td> }).collect_view()}</tr>
                }).collect_view()}</tbody>
            </table>
        </div>
    }
}

fn render_manage_cell(cell: ManageCell) -> AnyView {
    match cell {
        ManageCell::Mono(value) => view! { <code>{value}</code> }.into_any(),
        ManageCell::Text(value) => value.into_any(),
        ManageCell::Tag(value) => view! { <span class="manage-tag">{value}</span> }.into_any(),
        ManageCell::Reference { label, public_id } => {
            view! { <span class="manage-ref" title=public_id>{label}</span> }.into_any()
        }
        ManageCell::Time(value) => view! { <span class="manage-time">{value}</span> }.into_any(),
        ManageCell::None => view! { <span class="manage-none">"—"</span> }.into_any(),
    }
}

fn install_manage_refetch<T>(resource: Resource<T>, model: Option<String>)
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let model_for_event = model.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let should_refetch = event
                .dyn_into::<web_sys::CustomEvent>()
                .ok()
                .map(|event| js_sys::Array::from(&event.detail()))
                .is_none_or(|models| {
                    model_for_event.as_ref().is_none_or(|wanted| {
                        models
                            .iter()
                            .any(|value| value.as_string().as_deref() == Some(wanted))
                    })
                });
            if should_refetch {
                refetch_manage(resource);
            }
        });
        let _ = window()
            .add_event_listener_with_callback("rn-data-changed", callback.as_ref().unchecked_ref());
        let _ = window().add_event_listener_with_callback(
            "rn-resync-required",
            callback.as_ref().unchecked_ref(),
        );
        callback.forget();

        let reconcile = Closure::<dyn FnMut()>::new(move || {
            if document().visibility_state() == web_sys::VisibilityState::Visible {
                refetch_manage(resource);
            }
        });
        let _ = window().set_interval_with_callback_and_timeout_and_arguments_0(
            reconcile.as_ref().unchecked_ref(),
            90_000,
        );
        reconcile.forget();
    }
    #[cfg(not(feature = "hydrate"))]
    drop((resource, model));
}

#[cfg(feature = "hydrate")]
fn refetch_manage<T>(resource: Resource<T>)
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    let x = window().scroll_x().unwrap_or(0.0);
    let y = window().scroll_y().unwrap_or(0.0);
    let table = document()
        .query_selector("[data-reload-state=manage-table]")
        .ok()
        .flatten();
    let table_x = table.as_ref().map(web_sys::Element::scroll_left);
    let table_y = table.as_ref().map(web_sys::Element::scroll_top);
    resource.refetch();

    spawn_local(async move {
        // A resource refresh can patch the existing element instead of replacing
        // it, so node identity is not a useful completion signal. Wait for the
        // new resource value and then yield once for Leptos to commit its DOM.
        let _ = resource.await;
        browser_delay(0).await;
        window().scroll_to_with_x_and_y(x, y);
        let current = document()
            .query_selector("[data-reload-state=manage-table]")
            .ok()
            .flatten();
        if let (Some(table_x), Some(table_y), Some(element)) = (table_x, table_y, current) {
            element.set_scroll_left(table_x);
            element.set_scroll_top(table_y);
        }
    });

    async fn browser_delay(ms: i32) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let callback = Closure::<dyn FnMut()>::once(move || {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            });
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                ms,
            );
            callback.forget();
        });
        let _ = JsFuture::from(promise).await;
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
        <StarScapeControls epoch_ms=server_now_ms()/>
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
