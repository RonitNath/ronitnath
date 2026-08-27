//! The provisioned environment is a contract, and a node that misreads it
//! forms a cluster with itself. Every shape here fails closed at boot.
//!
//! These mutate the process environment, so they hold one lock and run in one
//! binary. Nothing else in the suite reads `RN_SITE_HQL_*`.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use rn_site::config::{AppConfig, Mode};
use rn_site::db::cluster;

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Set the whole `RN_SITE_HQL_*` block, clearing anything not named.
fn with_env(pairs: &[(&str, &str)]) -> MutexGuard<'static, ()> {
    let guard = ENVIRONMENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in [
        "RN_SITE_HQL_NODE_ID",
        "RN_SITE_HQL_NODES",
        "RN_SITE_HQL_SECRET_RAFT",
        "RN_SITE_HQL_SECRET_RAFT_FILE",
        "RN_SITE_HQL_SECRET_API",
        "RN_SITE_HQL_SECRET_API_FILE",
        "RN_SITE_HQL_LEARNER_ONLY",
        "RN_SITE_HQL_LOCAL_CLUSTER",
    ] {
        // Safe here: the lock makes this the only thread touching the
        // environment, and no other test binary reads these names.
        unsafe { std::env::remove_var(name) };
    }
    for (name, value) in pairs {
        unsafe { std::env::set_var(name, value) };
    }
    guard
}

fn prod() -> AppConfig {
    AppConfig {
        public_origin: "http://127.0.0.1:3004".to_owned(),
        public_name: None,
        oidc_key: Some("00".repeat(32)),
        mode: Mode::Prod,
        db_path: PathBuf::from("/var/lib/rn-site/db.sqlite"),
        addr: "0.0.0.0:3160".parse().expect("a bind address"),
        static_dir: PathBuf::from("/app/static"),
        id_key: Some("000102030405060708090a0b0c0d0e0f".to_string()),
        bootstrap_operator_email: None,
        dev: false,
    }
}

const THREE: &str = "1@10.0.0.1:8100@10.0.0.1:8200,\
                     2@10.0.0.2:8100@10.0.0.2:8200,\
                     3@10.0.0.3:8100@10.0.0.3:8200";

const SECRET: &str = "0123456789abcdef0123456789abcdef";

fn provisioned() -> Vec<(&'static str, &'static str)> {
    vec![
        ("RN_SITE_HQL_NODE_ID", "2"),
        ("RN_SITE_HQL_NODES", THREE),
        ("RN_SITE_HQL_SECRET_RAFT", SECRET),
        ("RN_SITE_HQL_SECRET_API", SECRET),
    ]
}

#[test]
fn a_provisioned_node_reads_its_whole_topology_out_of_the_environment() {
    let _guard = with_env(&provisioned());
    let topology = cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200")
        .expect("the provisioned three-voter map");
    assert_eq!(topology.node_id, 2);
    assert_eq!(topology.nodes.len(), 3);
    assert_eq!(topology.nodes[1].addr_raft, "10.0.0.2:8100");
    assert_eq!(topology.secret_raft, SECRET);
    assert!(!topology.learner_only);
}

#[test]
fn a_learner_joins_without_becoming_a_voter() {
    let mut env = provisioned();
    env.push(("RN_SITE_HQL_LEARNER_ONLY", "1"));
    let _guard = with_env(&env);
    assert!(
        cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200")
            .expect("a learner")
            .learner_only
    );
    // Shadowing the guard would not release it, and the lock is not reentrant.
    drop(_guard);

    let mut nonsense = provisioned();
    nonsense.push(("RN_SITE_HQL_LEARNER_ONLY", "perhaps"));
    let _guard = with_env(&nonsense);
    assert!(cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200").is_err());
}

#[test]
fn a_missing_or_malformed_peer_map_refuses_to_boot() {
    for (name, env) in [
        ("no node id", vec![("RN_SITE_HQL_NODES", THREE)]),
        ("no peer map", vec![("RN_SITE_HQL_NODE_ID", "1")]),
        (
            "a node id that is not a number",
            vec![("RN_SITE_HQL_NODE_ID", "two"), ("RN_SITE_HQL_NODES", THREE)],
        ),
        (
            "an even cluster",
            vec![
                ("RN_SITE_HQL_NODE_ID", "1"),
                (
                    "RN_SITE_HQL_NODES",
                    "1@10.0.0.1:8100@10.0.0.1:8200,2@10.0.0.2:8100@10.0.0.2:8200",
                ),
            ],
        ),
        (
            "a node absent from its own peer map",
            vec![("RN_SITE_HQL_NODE_ID", "9"), ("RN_SITE_HQL_NODES", THREE)],
        ),
        (
            "a peer address no peer could reach",
            vec![
                ("RN_SITE_HQL_NODE_ID", "1"),
                (
                    "RN_SITE_HQL_NODES",
                    "1@127.0.0.1:8100@127.0.0.1:8200,\
                     2@10.0.0.2:8100@10.0.0.2:8200,\
                     3@10.0.0.3:8100@10.0.0.3:8200",
                ),
            ],
        ),
        (
            "an entry missing its API address",
            vec![
                ("RN_SITE_HQL_NODE_ID", "1"),
                (
                    "RN_SITE_HQL_NODES",
                    "1@10.0.0.1:8100,\
                     2@10.0.0.2:8100@10.0.0.2:8200,\
                     3@10.0.0.3:8100@10.0.0.3:8200",
                ),
            ],
        ),
    ] {
        let mut env = env;
        env.push(("RN_SITE_HQL_SECRET_RAFT", SECRET));
        env.push(("RN_SITE_HQL_SECRET_API", SECRET));
        let _guard = with_env(&env);
        assert!(
            cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200").is_err(),
            "{name} was accepted"
        );
    }
}

#[test]
fn a_missing_or_trivial_cluster_secret_refuses_to_boot() {
    for secret in ["", "short"] {
        let mut env = provisioned();
        env.retain(|(name, _)| *name != "RN_SITE_HQL_SECRET_RAFT");
        env.push((
            "RN_SITE_HQL_SECRET_RAFT",
            Box::leak(secret.to_string().into_boxed_str()),
        ));
        let _guard = with_env(&env);
        assert!(
            cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200").is_err(),
            "a {} raft secret was accepted",
            if secret.is_empty() {
                "missing"
            } else {
                "trivial"
            }
        );
    }
}

#[test]
fn a_mounted_secret_file_is_read_and_outranks_the_variable() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("hiqlite-raft");
    // Trailing newline included on purpose: that is what an operator's
    // `openssl rand -hex 32 > file` produces, and it is not part of the secret.
    std::fs::write(&path, format!("{SECRET}\n")).expect("write the secret");

    let mut env = provisioned();
    env.retain(|(name, _)| *name != "RN_SITE_HQL_SECRET_RAFT");
    env.push((
        "RN_SITE_HQL_SECRET_RAFT",
        "the-variable-must-lose-to-the-file",
    ));
    let path_string = Box::leak(path.display().to_string().into_boxed_str());
    env.push(("RN_SITE_HQL_SECRET_RAFT_FILE", path_string));
    let _guard = with_env(&env);

    let topology = cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200")
        .expect("a file-provisioned secret");
    assert_eq!(topology.secret_raft, SECRET);
}

#[test]
fn a_secret_file_that_is_not_there_refuses_to_boot_rather_than_falling_back() {
    let mut env = provisioned();
    env.push(("RN_SITE_HQL_SECRET_RAFT_FILE", "/nonexistent/hiqlite-raft"));
    let _guard = with_env(&env);
    assert!(cluster::from_env(&prod(), "0.0.0.0:8100", "0.0.0.0:8200").is_err());
}

#[test]
fn a_single_host_cluster_needs_the_declaration_the_harness_makes() {
    let local = "1@127.0.0.1:8101@127.0.0.1:8201,\
                 2@127.0.0.1:8102@127.0.0.1:8202,\
                 3@127.0.0.1:8103@127.0.0.1:8203";
    let base = vec![
        ("RN_SITE_HQL_NODE_ID", "1"),
        ("RN_SITE_HQL_NODES", local),
        ("RN_SITE_HQL_SECRET_RAFT", SECRET),
        ("RN_SITE_HQL_SECRET_API", SECRET),
    ];

    let _guard = with_env(&base);
    assert!(cluster::from_env(&prod(), "127.0.0.1:8101", "127.0.0.1:8201").is_err());
    drop(_guard);

    let mut declared = base;
    declared.push(("RN_SITE_HQL_LOCAL_CLUSTER", "1"));
    let _guard = with_env(&declared);
    let topology = cluster::from_env(&prod(), "127.0.0.1:8101", "127.0.0.1:8201")
        .expect("a declared single-host cluster");
    assert_eq!(topology.nodes.len(), 3);
}

#[test]
fn dev_needs_no_provisioning_at_all() {
    let _guard = with_env(&[]);
    let config = AppConfig {
        public_origin: "http://127.0.0.1:3004".to_owned(),
        public_name: None,
        oidc_key: Some("00".repeat(32)),
        mode: Mode::Dev,
        db_path: PathBuf::from("data/db.sqlite"),
        addr: "127.0.0.1:3004".parse().expect("a loopback address"),
        static_dir: PathBuf::from("static"),
        id_key: None,
        bootstrap_operator_email: None,
        dev: false,
    };
    let topology = cluster::from_env(&config, "127.0.0.1:8100", "127.0.0.1:8200")
        .expect("the built-in dev topology");
    assert_eq!(topology.node_id, 1);
    assert_eq!(topology.nodes.len(), 1);
    assert!(!topology.secret_raft.is_empty());
}
