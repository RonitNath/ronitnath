//! One real hiqlite node, and the principals the suites run requests as.
//!
//! Every fixture goes through the kernel's own commands — registering somebody
//! here runs `cmd::register`, so a test cannot prove a registration path the
//! product does not have. The two things the kernel has no command for yet are
//! seeded as rows: an organization membership (K2's `CreateOrganization` and
//! `SetRole`) and the platform operator relation (seeded by an operator in
//! production). Both are written exactly as the schema describes them, and
//! `whoami` reads them through the same statements it always would.

#![allow(dead_code)]

use std::path::PathBuf;

use axum_test::TestServer;
use rn_kernel::cmd::{self, Ctx};
use rn_kernel::domain::Token;
use rn_kernel::ids::{Id, Identity, Person};

use rn_kernel::store::{Cursor, FromRow, Reads, RowError};
use rn_kernel::{Principal, bind};
use rn_site::config::{AppConfig, Mode};
use rn_site::db::{self, Migrations, Tuning};
use rn_site::state::AppState;
use tokio::sync::OnceCell;

/// A test password, long enough to be accepted and obviously not a real one.
pub const PASSWORD: &str = "an obviously fake test password";

/// The sentinel the secret-canary test looks for. Nothing but a test ever
/// sends it, so finding it anywhere is finding it somewhere it does not belong.
pub const CANARY: &str = "canary-8f3a1c9e-never-logged-never-echoed";

/// The runtime every test in a binary shares.
///
/// A `#[tokio::test]` builds and drops a runtime per test, and hiqlite's raft
/// tasks are spawned on whichever one booted the node — so the second test to
/// run would find a client whose raft had been shut down underneath it
/// ("raft stopped"). One runtime for the binary, outliving every test, is what
/// makes a shared node a shared node.
static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

/// Run a test body on the shared runtime.
pub fn run<F: std::future::Future>(future: F) -> F::Output {
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("a test runtime")
        })
        .block_on(future)
}

/// Boot one node for a whole test binary, on the ports it was given.
///
/// One node per binary rather than per test: hiqlite's formation delay is paid
/// once, and the tests share a database the way the routes share one process.
pub async fn node(cell: &'static OnceCell<AppState>, raft: &str, api: &str) -> AppState {
    node_on(cell, raft, api, rn_kernel::store::Clock::System).await
}

/// The same node on a clock the caller supplies, for the suites that need to
/// ask what the deployment looks like five minutes from now.
pub async fn node_on(
    cell: &'static OnceCell<AppState>,
    raft: &str,
    api: &str,
    clock: rn_kernel::store::Clock,
) -> AppState {
    cell.get_or_init(|| async {
        let directory = Box::leak(Box::new(
            tempfile::tempdir().expect("a temporary data directory"),
        ));
        let config = AppConfig {
            mode: Mode::Dev,
            db_path: directory.path().join("db.sqlite"),
            addr: "127.0.0.1:0".parse().expect("a loopback address"),
            static_dir: PathBuf::from("../../static"),
            id_key: Some("000102030405060708090a0b0c0d0e0f".to_string()),
        };
        let migrations = Migrations::embedded::<rn_kernel::Migrations>()
            .expect("the kernel's migration history");
        let db = db::open_on(&config, raft, api, Tuning::fast_single_node(), &migrations)
            .await
            .expect("a single dev node");
        AppState::with_clock(db, config, clock)
    })
    .await
    .clone()
}

/// A server over the whole surface, as `main` composes it.
pub fn server(state: &AppState) -> TestServer {
    TestServer::new(rn_site::router(state.clone()))
}

/// Somebody who exists, with a live session.
#[derive(Debug, Clone)]
pub struct Somebody {
    /// Their session cookie's value.
    pub token: String,
    /// Their identity.
    pub identity: Id<Identity>,
    /// The person the identity resolved to.
    pub person: Id<Person>,
}

/// Register somebody through the real command.
pub async fn register(state: &AppState, display: &str, email: &str) -> Somebody {
    let minted = cmd::register(
        &ctx(state, Principal::Anonymous),
        &rn_api::commands::Register {
            display_name: display.to_owned(),
            email: email.to_owned(),
            password: PASSWORD.to_owned(),
        },
    )
    .await
    .expect("a registration");
    let token = minted.token.expect("a fresh mint returns its token");
    let resolved = rn_kernel::principal::resolve(&state.store.reads(), &token)
        .await
        .expect("the session resolves");
    let Principal::Member {
        identity, person, ..
    } = resolved.principal
    else {
        panic!("a registration produces a member");
    };
    Somebody {
        token: token.expose().to_owned(),
        identity,
        person: person.expect("a registration resolves its person"),
    }
}

/// Mint a fresh session for somebody who already exists, through the real
/// command. Used where a case ends the session it arrived with.
pub async fn sign_in(state: &AppState, email: &str) -> String {
    let minted = cmd::sign_in(
        &ctx(state, Principal::Anonymous),
        &rn_api::commands::SignIn {
            email: email.to_owned(),
            password: PASSWORD.to_owned(),
        },
    )
    .await
    .expect("a sign-in");
    minted
        .token
        .expect("a fresh mint returns its token")
        .expose()
        .to_owned()
}

fn ctx(
    state: &AppState,
    principal: Principal,
) -> Ctx<'_, hiqlite::Client, rn_kernel::feed::ClusterFeed> {
    Ctx {
        store: state.store.as_ref(),
        feed: state.feed.as_ref(),
        principal,
        key: uuid::Uuid::new_v4(),
    }
}

struct RowId(i64);

impl FromRow for RowId {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self(row.int("id")?))
    }
}

/// Make somebody an operator of a fresh organization.
///
/// K2 owns `CreateOrganization` and `SetRole`; until they land the rows are
/// written directly, exactly as migration 1 describes them. `whoami` and the
/// shell router read them through the statements they always use.
pub async fn operate_organization(state: &AppState, person: Id<Person>, display: &str) {
    state
        .store
        .execute(
            "INSERT INTO party (kind, display_name, status, created_at) \
             VALUES ('organization', $1, 'active', $2)",
            bind![display, 1_800_000_000_i64],
        )
        .await
        .expect("an organization row");
    let organization = state
        .store
        .reads()
        .query_one::<RowId>(
            "SELECT id FROM party WHERE kind = 'organization' AND display_name = $1",
            bind![display],
        )
        .await
        .expect("the organization it just wrote");
    state
        .store
        .execute(
            "INSERT INTO membership (group_id, party_id, role, at) VALUES ($1, $2, 'admin', $3)",
            bind![organization.0, person, 1_800_000_000_i64],
        )
        .await
        .expect("a membership row");
}

/// Grant `platform:* #operator @person:P` — the only form platform
/// administration has (`docs/kernel/index.html`).
pub async fn operate_platform(state: &AppState, person: Id<Person>) {
    state
        .store
        .execute(
            "INSERT INTO relation \
             (object_kind, object_id, relation, subject_kind, subject_id, at) \
             VALUES ('platform', 0, 'operator', 'person', $1, $2)",
            bind![person, 1_800_000_000_i64],
        )
        .await
        .expect("an operator relation");
}

/// Mint a fresh, unclaimed verification link for somebody's email factor, and
/// return its bearer token.
pub async fn mint_link(state: &AppState, identity: Id<Identity>) -> String {
    let factor = state
        .store
        .reads()
        .query_one::<RowId>(
            "SELECT id FROM factor WHERE identity_id = $1 AND kind = 'email'",
            bind![identity],
        )
        .await
        .expect("the identity's email factor");
    let token: Token = cmd::mint_verification(state.store.as_ref(), Id::new(factor.0))
        .await
        .expect("a verification link");
    token.expose().to_owned()
}

/// A body a form post carries.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={}", encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_owned(),
            other => format!("%{other:02X}"),
        })
        .collect()
}
