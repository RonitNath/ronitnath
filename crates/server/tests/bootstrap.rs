//! The first platform operator, taken by the running process.
//!
//! `rn-site bootstrap-operator` opens the database directly, and hiqlite holds
//! an exclusive lock on its data directory — so on a formed cluster there is no
//! moment at which it can run. `RN_SITE__BOOTSTRAP_OPERATOR_EMAIL` is the same
//! grant taken while the node is serving, which is the state a cutover leaves
//! it in.
//!
//! What is proven here is the whole of it: nothing happens before that address
//! registers, the grant lands the moment it does, and a second one is never
//! written — which is the kernel's refusal rather than a rule repeated in the
//! server, so this asserts the row count and not a code path.

mod harness;

use rn_kernel::bind;
use rn_kernel::store::{Count, Reads};
use rn_site::state::AppState;
use tokio::sync::OnceCell;

static NODE: OnceCell<AppState> = OnceCell::const_new();

/// The address this deployment says its operator will register at.
const CONFIGURED: &str = "first-operator@example.invalid";

async fn state() -> AppState {
    harness::node_with(
        &NODE,
        "127.0.0.1:8193",
        "127.0.0.1:8293",
        rn_kernel::store::Clock::System,
        Some(CONFIGURED.to_owned()),
    )
    .await
}

/// How many people operate this deployment. The relation is the only form
/// platform administration has, so this is the whole answer.
async fn operators(state: &AppState) -> i64 {
    state
        .store
        .reads()
        .query::<Count>(
            "SELECT count(*) AS n FROM relation \
             WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator'",
            bind![],
        )
        .await
        .expect("the operator relation")[0]
        .0
}

#[test]
fn the_configured_address_becomes_the_operator_when_it_registers_and_never_twice() {
    harness::run(async {
        let state = state().await;

        // Before anybody registers it, the address names nobody. A deployment
        // that granted on a name rather than on a registration would be a
        // deployment where typing the variable is the whole of the authority.
        assert!(
            !rn_site::bootstrap::ensure(&state).await,
            "nothing to grant to yet"
        );
        assert_eq!(operators(&state).await, 0);

        // Somebody else registering is not it either.
        harness::register(&state, "Bystander", "bystander@example.invalid").await;
        assert!(!rn_site::bootstrap::ensure(&state).await);
        assert_eq!(operators(&state).await, 0);

        // The configured address registers, through the real command.
        let operator = harness::register(&state, "First Operator", CONFIGURED).await;
        assert!(
            rn_site::bootstrap::ensure(&state).await,
            "the grant is taken the moment the person exists"
        );
        assert_eq!(operators(&state).await, 1);

        // And it is that person, not merely somebody.
        let held = state
            .store
            .reads()
            .query::<Count>(
                "SELECT count(*) AS n FROM relation \
                 WHERE object_kind = 'platform' AND object_id = 0 AND relation = 'operator' \
                   AND subject_kind = 'person' AND subject_id = $1",
                bind![operator.person],
            )
            .await
            .expect("the operator's own row")[0]
            .0;
        assert_eq!(held, 1);

        // Every subsequent call does nothing. The variable stays set on a
        // running deployment, so "nothing" has to be the steady state rather
        // than a second, quieter grant per boot.
        for _ in 0..3 {
            assert!(!rn_site::bootstrap::ensure(&state).await);
        }
        assert_eq!(operators(&state).await, 1);

        // The tier follows from the relation, which is the point of granting
        // it: this person can now load `/platform`, and could not before.
        let server = harness::server(&state);
        let shell = server
            .get("/platform")
            .add_header("cookie", format!("rn_session={}", operator.token))
            .await;
        assert_eq!(
            shell.status_code(),
            200,
            "the grant is what the platform tier is"
        );
    });
}
