//! Turning the provisioned environment into a hiqlite node configuration.
//!
//! The variables are the ones `deploy/provision.sh` writes into each host's
//! `node.env` and `deploy/compose.yaml` injects, and they are a contract:
//!
//! | variable | meaning |
//! |---|---|
//! | `RN_SITE_HQL_NODE_ID` | which voter this process is |
//! | `RN_SITE_HQL_NODES` | the whole peer map, `id@raft@api` comma separated |
//! | `RN_SITE_HQL_SECRET_RAFT` / `_RAFT_FILE` | raft secret, value or mounted file |
//! | `RN_SITE_HQL_SECRET_API` / `_API_FILE` | API secret, value or mounted file |
//! | `RN_SITE_HQL_ADDR_RAFT` / `_ADDR_API` | what this process listens on |
//! | `RN_SITE_HQL_LEARNER_ONLY` | join without becoming a voter |
//! | `RN_SITE_HQL_LOCAL_CLUSTER` | this peer map is one host (see below) |
//!
//! Dev needs none of it: one node on loopback with built-in secrets.

use std::fs;

use hiqlite::Node;

use crate::config::{AppConfig, Mode};
use crate::db::DbError;

/// Where this process listens for its two peer protocols. Defaults are the
/// loopback pair a single dev node uses; the container overrides both so the
/// published mesh ports reach a listener that is not bound to 127.0.0.1.
pub const DEFAULT_ADDR_RAFT: &str = "127.0.0.1:8100";
pub const DEFAULT_ADDR_API: &str = "127.0.0.1:8200";

/// Secrets a dev node uses. They are not secret and are not meant to be: a
/// single loopback node has no peer to authenticate, and inventing a value here
/// would only make `cargo run` need a keyring.
const DEV_SECRET_RAFT: &str = "rn-site-dev-raft-secret";
const DEV_SECRET_API: &str = "rn-site-dev-api-secret";

/// The parts of a hiqlite `NodeConfig` that come from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topology {
    pub node_id: u64,
    pub nodes: Vec<NodeAddr>,
    pub secret_raft: String,
    pub secret_api: String,
    pub learner_only: bool,
}

/// `Node` without hiqlite's `Debug`/`PartialEq` gaps, so tests can assert on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAddr {
    pub id: u64,
    pub addr_raft: String,
    pub addr_api: String,
}

impl From<&NodeAddr> for Node {
    fn from(node: &NodeAddr) -> Self {
        Self {
            id: node.id,
            addr_raft: node.addr_raft.clone(),
            addr_api: node.addr_api.clone(),
        }
    }
}

/// Read the topology for `cfg.mode` out of the process environment.
pub fn from_env(cfg: &AppConfig, addr_raft: &str, addr_api: &str) -> Result<Topology, DbError> {
    match cfg.mode {
        Mode::Dev => {
            tracing::warn!("hiqlite is using the built-in single-node dev topology and secrets");
            Ok(Topology {
                node_id: 1,
                nodes: vec![NodeAddr {
                    id: 1,
                    addr_raft: addr_raft.to_string(),
                    addr_api: addr_api.to_string(),
                }],
                secret_raft: DEV_SECRET_RAFT.to_string(),
                secret_api: DEV_SECRET_API.to_string(),
                learner_only: false,
            })
        }
        Mode::Prod => {
            let node_id = required("RN_SITE_HQL_NODE_ID")?
                .parse()
                .map_err(|_| DbError::config("RN_SITE_HQL_NODE_ID must be an integer"))?;
            Ok(Topology {
                nodes: parse_nodes_allowing(
                    &required("RN_SITE_HQL_NODES")?,
                    node_id,
                    flag("RN_SITE_HQL_LOCAL_CLUSTER")?,
                )?,
                node_id,
                secret_raft: secret("RN_SITE_HQL_SECRET_RAFT", "RN_SITE_HQL_SECRET_RAFT_FILE")?,
                secret_api: secret("RN_SITE_HQL_SECRET_API", "RN_SITE_HQL_SECRET_API_FILE")?,
                learner_only: flag("RN_SITE_HQL_LEARNER_ONLY")?,
            })
        }
    }
}

/// The listen addresses, defaulted for a bare `cargo run`.
#[must_use]
pub fn listen_addrs() -> (String, String) {
    (
        env_or("RN_SITE_HQL_ADDR_RAFT", DEFAULT_ADDR_RAFT),
        env_or("RN_SITE_HQL_ADDR_API", DEFAULT_ADDR_API),
    )
}

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn required(name: &str) -> Result<String, DbError> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DbError::config(format!("mode=prod requires {name}")))
}

fn flag(name: &str) -> Result<bool, DbError> {
    match std::env::var(name).ok().as_deref().map(str::trim) {
        None | Some("") | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(_) => Err(DbError::config(format!("{name} must be true/false or 1/0"))),
    }
}

/// A mounted file wins over a process variable: Compose hands the secret in as
/// a file exactly so it never appears in `docker inspect` or an env dump.
fn secret(value_name: &str, file_name: &str) -> Result<String, DbError> {
    let secret = match std::env::var(file_name) {
        Ok(path) if !path.trim().is_empty() => fs::read_to_string(path.trim())
            .map_err(|source| DbError::Io {
                path: path.trim().to_string(),
                source,
            })?
            .trim()
            .to_string(),
        _ => required(value_name)?,
    };
    if secret.len() < 16 {
        return Err(DbError::config(format!(
            "{value_name} must contain at least 16 characters"
        )));
    }
    Ok(secret)
}

/// Parse `id@raft-address@api-address` entries separated by commas.
///
/// The odd-and-at-least-three rule is not raft's — raft is happy with two — it
/// is ours: a two- or four-voter cluster carries a strictly larger quorum for
/// no extra fault tolerance, and having typed one is a provisioning mistake we
/// would rather find at boot. Growing three to five means listing both new
/// nodes before either joins.
pub fn parse_nodes(value: &str, local_id: u64) -> Result<Vec<NodeAddr>, DbError> {
    parse_nodes_allowing(value, local_id, false)
}

/// [`parse_nodes`], with the loopback refusal lifted.
///
/// A peer address no peer can reach is a provisioning mistake, and refusing it
/// at boot is the point of that check — a deployed node that formed a cluster
/// with itself is a much worse thing to discover later. `tools/cluster.sh`
/// runs three voters on one host on purpose, for the perf and resilience
/// drills, and says so by setting `RN_SITE_HQL_LOCAL_CLUSTER=1`. Nothing under
/// `deploy/` sets it, and an operator naming it is not making the mistake the
/// check exists to catch.
pub fn parse_nodes_allowing(
    value: &str,
    local_id: u64,
    allow_loopback: bool,
) -> Result<Vec<NodeAddr>, DbError> {
    let mut nodes: Vec<NodeAddr> = Vec::new();
    for entry in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let shape = || {
            DbError::config(format!(
                "invalid RN_SITE_HQL_NODES entry `{entry}`; expected id@raft@api"
            ))
        };
        let mut fields = entry.split('@');
        let id = fields
            .next()
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or_else(shape)?;
        let addr_raft = fields.next().filter(|s| !s.is_empty()).ok_or_else(shape)?;
        let addr_api = fields.next().filter(|s| !s.is_empty()).ok_or_else(shape)?;
        if fields.next().is_some() {
            return Err(shape());
        }
        if nodes.iter().any(|node| node.id == id) {
            return Err(DbError::config(format!(
                "RN_SITE_HQL_NODES contains duplicate node id {id}"
            )));
        }
        for address in [addr_raft, addr_api] {
            if !allow_loopback && address.starts_with("127.")
                || (!allow_loopback
                    && (address.starts_with("0.0.0.0") || address.starts_with("localhost")))
            {
                return Err(DbError::config(format!(
                    "a production peer address must be mesh-reachable, not `{address}`"
                )));
            }
        }
        nodes.push(NodeAddr {
            id,
            addr_raft: addr_raft.to_string(),
            addr_api: addr_api.to_string(),
        });
    }

    if nodes.len() < 3 || nodes.len().is_multiple_of(2) {
        return Err(DbError::config(format!(
            "RN_SITE_HQL_NODES must describe an odd cluster of at least 3 nodes; found {}",
            nodes.len()
        )));
    }
    if !nodes.iter().any(|node| node.id == local_id) {
        return Err(DbError::config(format!(
            "RN_SITE_HQL_NODE_ID {local_id} is absent from RN_SITE_HQL_NODES"
        )));
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE: &str = "1@10.0.0.1:8100@10.0.0.1:8200,\
                         2@10.0.0.2:8100@10.0.0.2:8200,\
                         3@10.0.0.3:8100@10.0.0.3:8200";

    #[test]
    fn a_well_formed_odd_peer_map_containing_this_node_parses() {
        let nodes = parse_nodes(THREE, 2).expect("three mesh voters");
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[1].id, 2);
        assert_eq!(nodes[1].addr_api, "10.0.0.2:8200");
    }

    #[test]
    fn a_node_absent_from_its_own_peer_map_is_refused() {
        assert!(parse_nodes(THREE, 4).is_err());
    }

    #[test]
    fn an_even_or_undersized_cluster_is_refused() {
        assert!(parse_nodes("1@10.0.0.1:8100@10.0.0.1:8200", 1).is_err());
        assert!(
            parse_nodes(
                "1@10.0.0.1:8100@10.0.0.1:8200,2@10.0.0.2:8100@10.0.0.2:8200",
                1
            )
            .is_err()
        );
    }

    #[test]
    fn a_loopback_peer_address_is_refused_because_no_peer_could_reach_it() {
        let local = THREE.replacen("10.0.0.1:8100", "127.0.0.1:8100", 1);
        assert!(parse_nodes(&local, 1).is_err());
        assert!(parse_nodes(&THREE.replacen("10.0.0.2", "0.0.0.0", 1), 1).is_err());
    }

    #[test]
    fn a_malformed_entry_is_refused_rather_than_silently_dropped() {
        for bad in [
            "1@10.0.0.1:8100,2@10.0.0.2:8100@10.0.0.2:8200,3@10.0.0.3:8100@10.0.0.3:8200",
            "x@10.0.0.1:8100@10.0.0.1:8200,2@10.0.0.2:8100@10.0.0.2:8200,3@a:1@b:2",
            "1@10.0.0.1:8100@10.0.0.1:8200@extra,2@10.0.0.2:8100@10.0.0.2:8200,3@10.0.0.3:8100@10.0.0.3:8200",
        ] {
            assert!(parse_nodes(bad, 1).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn a_single_host_cluster_is_admissible_only_when_it_says_so() {
        let local = "1@127.0.0.1:8101@127.0.0.1:8201,\
                     2@127.0.0.1:8102@127.0.0.1:8202,\
                     3@127.0.0.1:8103@127.0.0.1:8203";
        assert!(parse_nodes(local, 2).is_err());
        let nodes = parse_nodes_allowing(local, 2, true).expect("a declared local cluster");
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[1].addr_raft, "127.0.0.1:8102");
        // The rules that are not about reachability still apply.
        assert!(parse_nodes_allowing(local, 4, true).is_err());
        assert!(parse_nodes_allowing("1@127.0.0.1:8101@127.0.0.1:8201", 1, true).is_err());
    }

    #[test]
    fn a_duplicated_node_id_is_refused() {
        let duplicate = "1@10.0.0.1:8100@10.0.0.1:8200,\
                         1@10.0.0.2:8100@10.0.0.2:8200,\
                         3@10.0.0.3:8100@10.0.0.3:8200";
        assert!(parse_nodes(duplicate, 1).is_err());
    }
}
