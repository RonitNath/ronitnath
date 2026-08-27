//! The route × principal matrix, the negative space, and what the API is
//! allowed to say.
//!
//! The matrix is not a list somebody maintains beside the router. Every
//! `.route("…")` in `crates/server/src` is read out of the source and matched
//! against [`CASES`]: a route added without a matrix row fails
//! `every_route_has_a_matrix_row` rather than quietly shipping unguarded. The
//! shells' wildcard routes are built with `format!`, so they are declared by
//! `rn_site::shell::SHELLS` and checked against that.

mod harness;

use std::collections::BTreeSet;

use axum::http::StatusCode;
use rn_site::shell::SHELLS;
use tokio::sync::OnceCell;

static NODE: OnceCell<rn_site::AppState> = OnceCell::const_new();

async fn state() -> rn_site::AppState {
    harness::node(&NODE, "127.0.0.1:8196", "127.0.0.1:8296").await
}

/// The principals the whole surface is tested against.
///
/// `Unauthorized` is a member holding no organization and no operator
/// relation; it is the same cookie shape as `Member` and a different answer on
/// `/org` and `/platform`, which is the distinction the matrix exists to pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    Anonymous,
    Member,
    OrgOperator,
    PlatformOperator,
    /// A link's bearer token, presented to a cookie surface.
    BearerOnApi,
    /// A member's cookie, presented to the bearer surface.
    CookieOnLinks,
}

const EVERYONE: &[Who] = &[
    Who::Anonymous,
    Who::Member,
    Who::OrgOperator,
    Who::PlatformOperator,
    Who::BearerOnApi,
    Who::CookieOnLinks,
];

/// Whether this principal arrives holding a session cookie at all.
fn signed_in(who: Who) -> bool {
    !matches!(who, Who::Anonymous | Who::BearerOnApi)
}

/// What a URL is, and whether it has to be minted fresh for each request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Url {
    Fixed(&'static str),
    /// `{token}` is replaced with a freshly minted, unclaimed link.
    FreshLink(&'static str),
}

struct Case {
    /// The pattern as `.route(…)` spells it — what the source scan matches.
    pattern: &'static str,
    method: &'static str,
    url: Url,
    /// A form or JSON body, for the posts.
    body: Option<&'static str>,
    content_type: &'static str,
    /// Whether running this case ends the session it arrived with, so the next
    /// principal needs a fresh one rather than the corpse of the last.
    ends_session: bool,
    /// The status each principal must get.
    expect: fn(Who) -> u16,
}

const OK: u16 = 200;
const FOUND: u16 = 302;
const SEE_OTHER: u16 = 303;
const BAD_REQUEST: u16 = 400;
const FORBIDDEN: u16 = 403;
const NOT_FOUND: u16 = 404;
const UNPROCESSABLE: u16 = 422;

const PUBLIC: fn(Who) -> u16 = |_| OK;
const MEMBERS_ONLY: fn(Who) -> u16 = |who| if signed_in(who) { OK } else { FORBIDDEN };

/// A `/platform` read: the operator relation, and the same refusal for
/// everybody else however they arrived.
const OPERATORS_ONLY: fn(Who) -> u16 = |who| {
    if who == Who::PlatformOperator {
        OK
    } else {
        FORBIDDEN
    }
};

const CASES: &[Case] = &[
    // --- public: ops, presence, assets ------------------------------------
    case("/healthz", "GET", Url::Fixed("/healthz"), PUBLIC),
    case("/readyz", "GET", Url::Fixed("/readyz"), PUBLIC),
    case("/version", "GET", Url::Fixed("/version"), PUBLIC),
    case("/", "GET", Url::Fixed("/"), PUBLIC),
    case("/favicon.ico", "GET", Url::Fixed("/favicon.ico"), PUBLIC),
    case("/tokens.css", "GET", Url::Fixed("/tokens.css"), PUBLIC),
    case(
        "/static/{*path}",
        "GET",
        Url::Fixed("/static/pages.css"),
        PUBLIC,
    ),
    case(
        "/pkg/starscape/{*path}",
        "GET",
        Url::Fixed("/pkg/starscape/nothing.js"),
        |_| NOT_FOUND,
    ),
    case(
        "/app/pkg/{*path}",
        "GET",
        Url::Fixed("/app/pkg/nothing.js"),
        |_| NOT_FOUND,
    ),
    case(
        "/org/pkg/{*path}",
        "GET",
        Url::Fixed("/org/pkg/nothing.js"),
        |_| NOT_FOUND,
    ),
    case(
        "/platform/pkg/{*path}",
        "GET",
        Url::Fixed("/platform/pkg/nothing.js"),
        |_| NOT_FOUND,
    ),
    // --- auth --------------------------------------------------------------
    // A signed-in reader is sent on rather than shown a form they have no use
    // for; everyone else gets the page.
    case("/auth", "GET", Url::Fixed("/auth"), |who| {
        if signed_in(who) { SEE_OTHER } else { OK }
    }),
    Case {
        pattern: "/auth/register",
        method: "POST",
        url: Url::Fixed("/auth/register"),
        // An empty name is refused for the shape of the request, by everyone:
        // registering is public, and it is the *validation* that answers.
        body: Some("display_name=&email=&password="),
        content_type: FORM,
        ends_session: false,
        expect: |_| UNPROCESSABLE,
    },
    Case {
        pattern: "/auth/sign-in",
        method: "POST",
        url: Url::Fixed("/auth/sign-in"),
        body: Some("email=nobody%40example.invalid&password=wrong"),
        content_type: FORM,
        ends_session: false,
        // The uniform refusal, for everybody, however they arrived.
        expect: |_| FORBIDDEN,
    },
    Case {
        pattern: "/auth/sign-out",
        method: "POST",
        url: Url::Fixed("/auth/sign-out"),
        body: Some("next=%2F"),
        content_type: FORM,
        ends_session: true,
        expect: |who| if signed_in(who) { SEE_OTHER } else { FORBIDDEN },
    },
    // The developer bypass. `404` for everybody here in *both* build profiles,
    // and for two different reasons: a release build does not compile the
    // route at all, and this debug build was not told `RN_SITE__DEV=1`. The
    // row is unconditional precisely because the answer is — a refusal that
    // told the two apart would be the leak the route is gated against.
    Case {
        pattern: "/auth/dev",
        method: "POST",
        url: Url::Fixed("/auth/dev"),
        body: Some(""),
        content_type: FORM,
        ends_session: false,
        expect: |_| NOT_FOUND,
    },
    // --- shells ------------------------------------------------------------
    case("/app", "GET", Url::Fixed("/app"), |who| {
        if signed_in(who) { OK } else { FOUND }
    }),
    case("/app/", "GET", Url::Fixed("/app/"), |who| {
        if signed_in(who) { OK } else { FOUND }
    }),
    case("/app/{*rest}", "GET", Url::Fixed("/app/sessions"), |who| {
        if signed_in(who) { OK } else { FOUND }
    }),
    case("/org", "GET", Url::Fixed("/org"), org_shell),
    case("/org/", "GET", Url::Fixed("/org/"), org_shell),
    case("/org/{*rest}", "GET", Url::Fixed("/org/groups"), org_shell),
    case("/platform", "GET", Url::Fixed("/platform"), platform_shell),
    case(
        "/platform/",
        "GET",
        Url::Fixed("/platform/"),
        platform_shell,
    ),
    case(
        "/platform/{*rest}",
        "GET",
        Url::Fixed("/platform/people"),
        platform_shell,
    ),
    // --- api ---------------------------------------------------------------
    case(
        "/api/whoami",
        "GET",
        Url::Fixed("/api/whoami"),
        MEMBERS_ONLY,
    ),
    case(
        "/api/q/{name}",
        "GET",
        Url::Fixed("/api/q/sessions"),
        MEMBERS_ONLY,
    ),
    // The node's own account of itself: a literal segment that wins over the
    // capture above, and the operator relation re-read on every request.
    case(
        "/api/q/cluster",
        "GET",
        Url::Fixed("/api/q/cluster"),
        OPERATORS_ONLY,
    ),
    Case {
        pattern: "/api/cmd/{name}",
        method: "POST",
        url: Url::Fixed("/api/cmd/add-factor"),
        // A factor kind the schema admits and no command produces: refused for
        // its shape by a member, and refused for who is asking by everyone
        // else — which is the discrimination this row exists to make.
        body: Some(
            r#"{"key":"3f1a6c52-0000-4000-8000-000000000001","kind":"passkey","value":"x"}"#,
        ),
        content_type: JSON,
        ends_session: false,
        expect: |who| {
            if signed_in(who) {
                UNPROCESSABLE
            } else {
                FORBIDDEN
            }
        },
    },
    // An upgrade route answers a plain GET with `400`, which is a browser
    // telling itself it asked wrongly — never a hint about authorisation.
    case("/api/sub", "GET", Url::Fixed("/api/sub"), |who| {
        if signed_in(who) {
            BAD_REQUEST
        } else {
            FORBIDDEN
        }
    }),
    // --- links -------------------------------------------------------------
    // The bearer surface. The token is what opens it; the cookie decides only
    // what claiming it would do.
    case(
        "/links/{token}",
        "GET",
        Url::FreshLink("/links/{token}"),
        |_| OK,
    ),
    Case {
        pattern: "/links/{token}/claim",
        method: "POST",
        url: Url::FreshLink("/links/{token}/claim"),
        body: Some(""),
        content_type: FORM,
        ends_session: false,
        // Signed in: the grant lands and the reader is sent to their shell.
        // Not signed in: sent to sign in, and the link is still unclaimed.
        expect: |_| SEE_OTHER,
    },
];

const FORM: &str = "application/x-www-form-urlencoded";
const JSON: &str = "application/json";

const fn case(
    pattern: &'static str,
    method: &'static str,
    url: Url,
    expect: fn(Who) -> u16,
) -> Case {
    Case {
        pattern,
        method,
        url,
        body: None,
        content_type: FORM,
        ends_session: false,
        expect,
    }
}

/// `/org` redirects the anonymous and 404s the authenticated-unauthorized.
fn org_shell(who: Who) -> u16 {
    match who {
        Who::OrgOperator => OK,
        _ if signed_in(who) => NOT_FOUND,
        _ => FOUND,
    }
}

/// `/platform` tells everybody without it the same thing: nothing is here.
fn platform_shell(who: Who) -> u16 {
    if who == Who::PlatformOperator {
        OK
    } else {
        NOT_FOUND
    }
}

#[test]
fn every_route_has_a_matrix_row() {
    harness::run(async {
        let declared: BTreeSet<&str> = CASES.iter().map(|case| case.pattern).collect();
        let mut missing = Vec::new();
        for route in routes_in_source() {
            if !declared.contains(route.as_str()) {
                missing.push(route);
            }
        }
        assert!(
            missing.is_empty(),
            "these routes have no row in the matrix: {missing:?}"
        );

        // The shells build their wildcard routes with `format!`, so they are
        // declared by the shell table rather than found by the scan.
        for shell in SHELLS {
            for pattern in [
                shell.path.to_owned(),
                format!("{}/", shell.path),
                format!("{}/{{*rest}}", shell.path),
            ] {
                assert!(
                    declared.contains(pattern.as_str()),
                    "{pattern} has no row in the matrix"
                );
            }
        }
    });
}

/// Every `.route("…"` literal under `crates/server/src`.
fn routes_in_source() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    walk(
        std::path::Path::new("src"),
        &mut |_path: &std::path::Path, source: &str| {
            let source = code_only(source);
            let mut rest = source.as_str();
            while let Some(at) = rest.find(".route(\"") {
                rest = &rest[at + ".route(\"".len()..];
                if let Some(end) = rest.find('"') {
                    found.insert(rest[..end].to_owned());
                }
            }
        },
    );
    assert!(
        found.len() > 10,
        "the source scan found almost nothing ({found:?}); it is not reading the tree"
    );
    found
}

/// A source file with its comments removed.
///
/// Prose is not code: this module's own doc comment shows a `.route(…)` that
/// must *not* compile, and a scan that read it would demand a matrix row for a
/// route nobody serves.
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with("#[doc")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn walk(dir: &std::path::Path, each: &mut impl FnMut(&std::path::Path, &str)) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|error| panic!("{dir:?}: {error}"));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, each);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = std::fs::read_to_string(&path).expect("a source file");
            each(&path, &source);
        }
    }
}

/// Every crate's `src`, which is what "in the tree" means.
///
/// The suite runs with `crates/server` as its working directory, so the
/// workspace's crates are one level up. Read from the filesystem rather than
/// from a list: a crate added next leg is walked without anybody remembering
/// to add it here.
fn every_crate_src() -> Vec<std::path::PathBuf> {
    let crates = std::path::Path::new("..");
    let mut roots: Vec<_> = std::fs::read_dir(crates)
        .expect("the workspace's crates directory")
        .flatten()
        .map(|entry| entry.path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    roots.sort();
    assert!(
        roots.len() >= 8,
        "only {} crate(s) found; the walk is looking in the wrong place",
        roots.len()
    );
    roots
}

#[test]
fn the_whole_surface_answers_every_principal_the_way_the_contract_says() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);

        let member = harness::register(&state, "Member", "matrix-member@example.invalid").await;
        let org = harness::register(&state, "Operator", "matrix-org@example.invalid").await;
        harness::operate_organization(&state, org.person, "Matrix Organization").await;
        let platform =
            harness::register(&state, "Platform", "matrix-platform@example.invalid").await;
        harness::operate_platform(&state, platform.person).await;
        let bearer_token = harness::mint_link(&state, member.identity).await;

        let account = |who: Who| match who {
            Who::Anonymous | Who::BearerOnApi => None,
            Who::Member | Who::CookieOnLinks => {
                Some((member.token.clone(), "matrix-member@example.invalid"))
            }
            Who::OrgOperator => Some((org.token.clone(), "matrix-org@example.invalid")),
            Who::PlatformOperator => {
                Some((platform.token.clone(), "matrix-platform@example.invalid"))
            }
        };

        let mut checked = 0;
        for case in CASES {
            for &who in EVERYONE {
                // A case that ends the session it used gets a fresh one, minted
                // through `SignIn` — otherwise the second principal to sign out
                // would be presenting a session the first one already deleted.
                let cookie = match account(who) {
                    None => None,
                    Some((_, email)) if case.ends_session => {
                        Some(harness::sign_in(&state, email).await)
                    }
                    Some((token, _)) => Some(token),
                };
                let url = match case.url {
                    Url::Fixed(url) => url.to_owned(),
                    Url::FreshLink(template) => {
                        let fresh = harness::mint_link(&state, member.identity).await;
                        template.replace("{token}", &fresh)
                    }
                };

                let mut request = match case.method {
                    "GET" => server.get(&url),
                    "POST" => server.post(&url),
                    other => panic!("{other} is not a method this matrix sends"),
                };
                // Every request states its initiator, so the matrix measures
                // authorisation rather than the same-origin guard. The guard has
                // its own tests below.
                request = request.add_header("sec-fetch-site", "same-origin");
                if let Some(token) = &cookie {
                    request = request.add_header("cookie", format!("rn_session={token}"));
                }
                if who == Who::BearerOnApi {
                    // A link token, offered every way a caller could offer one.
                    request = request
                        .add_header("authorization", format!("Bearer {bearer_token}"))
                        .add_header("x-rn-link", bearer_token.clone());
                }
                if let Some(body) = case.body {
                    request = request.text(body).content_type(case.content_type);
                }

                let response = request.await;
                assert_eq!(
                    response.status_code().as_u16(),
                    (case.expect)(who),
                    "{} {url} as {who:?} answered {} — body {}",
                    case.method,
                    response.status_code(),
                    response.text()
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            CASES.len() * EVERYONE.len(),
            "the matrix did not run in full"
        );
    });
}

#[test]
fn a_link_token_is_nobody_on_the_api_and_a_cookie_opens_no_link() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Holder", "bearer-holder@example.invalid").await;
        let token = harness::mint_link(&state, somebody.identity).await;

        // The bearer surface is the only place a link token means anything.
        for path in ["/api/whoami", "/api/q/sessions"] {
            let response = server
                .get(path)
                .add_header("authorization", format!("Bearer {token}"))
                .add_header("cookie", format!("rn_session={token}"))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::FORBIDDEN,
                "{path} accepted a link token"
            );
        }

        // And a cookie does not open a link that is not one.
        for candidate in ["not-a-token", &somebody.token] {
            let response = server
                .get(&format!("/links/{candidate}"))
                .add_header("cookie", format!("rn_session={}", somebody.token))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::NOT_FOUND,
                "/links/{candidate} opened for a session token"
            );
        }
    });
}

#[test]
fn nothing_mutates_without_the_browser_saying_where_it_came_from() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Origin", "origin-guard@example.invalid").await;
        let cookie = format!("rn_session={}", somebody.token);

        let mutations: &[(&str, &str, &str)] = &[
            ("/auth/register", FORM, "display_name=x&email=x&password=x"),
            ("/auth/sign-in", FORM, "email=x&password=x"),
            ("/auth/sign-out", FORM, ""),
            (
                "/api/cmd/add-factor",
                JSON,
                r#"{"key":"3f1a6c52-0000-4000-8000-000000000002","kind":"email","value":"x"}"#,
            ),
        ];

        for (path, content_type, body) in mutations {
            // An `Origin` header is not enough, and its absence is not a pass:
            // a page under `Referrer-Policy: no-referrer` sends `Origin: null`
            // for its own form.
            for site in ["cross-site", "same-site"] {
                let response = server
                    .post(path)
                    .add_header("cookie", cookie.clone())
                    .add_header("origin", "https://ronitnath.com")
                    .add_header("sec-fetch-site", site.to_string())
                    .text((*body).to_string())
                    .content_type(content_type)
                    .await;
                assert_eq!(
                    response.status_code(),
                    StatusCode::FORBIDDEN,
                    "{path} accepted a {site} write"
                );
            }
            let silent = server
                .post(path)
                .add_header("cookie", cookie.clone())
                .text((*body).to_string())
                .content_type(content_type)
                .await;
            assert_eq!(
                silent.status_code(),
                StatusCode::FORBIDDEN,
                "{path} accepted a write that would not say where it came from"
            );
        }
    });
}

#[test]
fn negative_space() {
    harness::run(async {
        let state = state().await;
        let server = harness::server(&state);
        let somebody = harness::register(&state, "Absent", "negative-space@example.invalid").await;

        // Deleted surfaces, from `docs/rebuild/plan.md` §KDR(D). They are absent
        // for an operator too: a route nobody serves is not a permission.
        for absent in [
            "/manage",
            "/manage/people",
            "/manage/anything/at/all",
            "/api/realtime",
            "/pkg/rn-site.js",
            "/dev-dashboard",
            "/metrics",
        ] {
            for cookie in [None, Some(&somebody.token)] {
                let mut request = server.get(absent);
                if let Some(token) = &cookie {
                    request = request.add_header("cookie", format!("rn_session={token}"));
                }
                assert_eq!(
                    request.await.status_code(),
                    StatusCode::NOT_FOUND,
                    "{absent} answered"
                );
            }
        }
    });
}

#[test]
fn the_deleted_vocabulary_is_gone_from_the_tree() {
    let mut found = Vec::new();
    for root in every_crate_src() {
        walk(&root, &mut |path: &std::path::Path, source: &str| {
            // Comments are stripped first. The deleted model is discussed in
            // prose all over this tree — that is the record of the decision,
            // and it is the *symbols* that must be gone.
            let code = code_only(source);
            for word in [
                "capability",
                "resource_grants",
                "manage",
                "dev_bypass",
                "realtime",
            ] {
                if code.contains(word) {
                    found.push(format!("{} in {}", word, path.display()));
                }
            }
        });
    }
    assert!(found.is_empty(), "the tree still carries {found:#?}");
}

#[test]
fn read_store_has_no_execute() {
    // The type-level half is the kernel's: `ReadStore` has no `execute` and no
    // `commit`, proven there by two `compile_fail` doctests. What is this
    // crate's to prove is that the query path is *given* one — so no file
    // reachable from a GET names a mutating call at all.
    let mut offenders = Vec::new();
    let mut handed_a_read_store = false;
    walk(
        std::path::Path::new("src/api/query"),
        &mut |_path: &std::path::Path, source: &str| {
            let code = code_only(source);
            for mutation in [".execute(", ".commit(", "INSERT ", "UPDATE ", "DELETE "] {
                if code.contains(mutation) {
                    offenders.push(mutation);
                }
            }
            handed_a_read_store |= code.contains("reads: &impl Reads");
        },
    );
    assert!(
        offenders.is_empty(),
        "the query module reaches a write: {offenders:?}"
    );
    assert!(
        handed_a_read_store,
        "a query handler must be handed a read-only store"
    );
}
