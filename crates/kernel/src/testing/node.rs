//! A real single-node hiqlite cluster, for the cases that are about raft.
//!
//! Almost everything in this crate is tested against the in-process engine,
//! which is the same SQLite running the same statements — faster, and enough
//! to prove the SQL. What it cannot prove is that a batch survives being
//! serialised through a raft log, that `Param::StmtOutput` means the same
//! thing on the leader as it does in-process, and that listen/notify actually
//! wakes a subscriber. That is what this is for, and why it is one test and
//! one benchmark rather than the default engine.
//!
//! Production bootstrap belongs to `crates/server`; this is deliberately the
//! smallest node that will start.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use hiqlite::{Client, Node as HiqliteNode, NodeConfig};

use crate::ids::IdKey;
use crate::store::{Clock, Migrations, Store};

/// The cache raft group's variants. hiqlite needs at least one, and the change
/// feed's wake-ups ride the group rather than a variant, so there is exactly
/// one and it names what the group is for.
#[derive(Debug, hiqlite::macros::CacheVariants)]
pub enum KernelCache {
    /// The change feed's notifications.
    Feed,
}

/// A running single-node cluster and the directory it lives in.
///
/// Dropping it stops the node and removes the directory: a test that left a
/// data directory behind would make the next run of the same test start
/// against somebody else's raft state.
pub struct Node {
    /// The store over the live client. An `Arc` so a feed can hold it too
    /// without the node giving up ownership of its data directory.
    pub store: Arc<Store<Client>>,
    dir: tempfile::TempDir,
}

impl Node {
    /// Start a node on free loopback ports and apply every migration.
    ///
    /// # Errors
    ///
    /// If the node will not start or the migrations will not apply, which
    /// means the test cannot run.
    pub async fn start() -> Result<Self, hiqlite::Error> {
        let dir =
            tempfile::tempdir().map_err(|err| hiqlite::Error::Config(err.to_string().into()))?;
        let config = NodeConfig {
            node_id: 1,
            nodes: vec![HiqliteNode {
                id: 1,
                addr_raft: free_port(),
                addr_api: free_port(),
            }],
            data_dir: dir.path().to_string_lossy().into_owned().into(),
            filename_db: "kernel.sqlite".into(),
            // Obviously fake, loopback-only, and thrown away with the node.
            secret_raft: "rn-kernel-test-raft-secret".to_owned(),
            secret_api: "rn-kernel-test-api-secret".to_owned(),
            health_check_delay_secs: 0,
            cache_storage_disk: false,
            ..NodeConfig::default()
        };
        let config = fast_election(config);

        let client = hiqlite::start_node_with_cache::<KernelCache>(config).await?;
        wait_healthy(&client).await?;
        client.migrate::<Migrations>().await?;

        Ok(Self {
            store: Arc::new(Store::new(client, test_key(), Clock::System)),
            dir,
        })
    }

    /// The data directory, for a test that wants to look at it.
    pub fn path(&self) -> PathBuf {
        self.dir.path().to_owned()
    }
}

fn test_key() -> IdKey {
    IdKey::from_hex(super::TEST_ID_KEY).expect("the test key is well formed")
}

/// On a *fresh* data directory hiqlite waits `heartbeat_interval * 5` before
/// initialising each raft group, to be sure it is not rejoining a cluster
/// after losing its disk. Two groups at the 500 ms default is a flat five
/// seconds on every test boot. Lowering it is safe here and only here: one
/// node, on loopback, with no peer that could be mid-election.
fn fast_election(mut config: NodeConfig) -> NodeConfig {
    config.raft_config.heartbeat_interval = 50;
    config.raft_config.election_timeout_min = 150;
    config.raft_config.election_timeout_max = 300;
    config
}

/// `start_node_with_cache` returning means the raft groups exist, not that
/// they have a leader. Without this the first statement races the election and
/// fails as `LeaderChange`, which is a confusing way to learn the node simply
/// was not up yet.
async fn wait_healthy(client: &Client) -> Result<(), hiqlite::Error> {
    let started = std::time::Instant::now();
    loop {
        if client.is_healthy_db().await.is_ok() && client.is_healthy_cache().await.is_ok() {
            return Ok(());
        }
        if started.elapsed() > std::time::Duration::from_secs(30) {
            return Err(hiqlite::Error::Config("node did not elect a leader".into()));
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Ask the operating system for a port nobody is using.
///
/// # Panics
///
/// If loopback cannot be bound at all, which is not a condition a test can
/// work around.
fn free_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback is bindable");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    drop(listener);
    addr.to_string()
}
