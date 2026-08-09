#!/usr/bin/env bash
# Host provisioning for the rn-site replica set. Run on nexus, as the operator,
# before the first deploy and after any change to the Compose model or the node
# identities.
#
#   deploy/provision.sh [node ...]      # default: all three
#
# This is deliberately NOT part of CD. Continuous delivery only ever rewrites an
# immutable digest into image.env (procedures/deployment.md: an operator
# provisions, a pipeline cuts over). Running it twice is safe.
#
# Hiqlite state and its two cluster secrets are provisioned here, never by the
# release pipeline. Nexus holds the canonical secret copies; every voter gets
# the same values and its own durable state directory.
set -euo pipefail

if [[ "$(hostname -s)" != "nexus" ]]; then
    echo "provision.sh must run on nexus: it holds the ssh aliases and the registry pull token." >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR=/data/apps/rn-site
# Verified unassigned on nexus, nyc and delenda. Do not reuse 9750 — that is
# universe's identity, and a container uid is only ever numeric.
RUNTIME_UID=9751
RUNTIME_GID=9751
REGISTRY_CLIENT=forgejo-registry-pull
CLUSTER_SECRET_DIR=/etc/rn-site/cluster-secrets
HQL_NODES='1@100.88.31.199:8100@100.88.31.199:8200,2@100.88.223.144:8100@100.88.223.144:8200,3@100.88.252.202:8100@100.88.252.202:8200'

# short name : mesh ip : ssh target ("" means this host)
#
# Order is the rolling order used by the fleet playbook: nexus, nyc, delenda.
NODES=(
    "1:nexus:100.88.31.199:"
    "2:nyc:100.88.223.144:nyc"
    "3:delenda:100.88.252.202:del"
)

log() { echo "==> $*"; }

# Generate once on nexus. Re-running provisioning copies the existing values;
# silently rotating either secret would partition every node not changed in the
# same instant and is therefore never an automatic operation.
ensure_cluster_secrets() {
    sudo install -d -o root -g root -m 0700 "$CLUSTER_SECRET_DIR"
    for name in hiqlite-raft hiqlite-api; do
        if ! sudo test -s "$CLUSTER_SECRET_DIR/$name"; then
            log "generating $name cluster secret"
            openssl rand -hex 32 \
                | sudo install -o root -g root -m 0400 /dev/stdin "$CLUSTER_SECRET_DIR/$name"
        fi
    done
}

# --- delenda only: the registry pull identity -------------------------------
# nexus and nyc are Debian hosts provisioned with the forge credential helper
# and its root-only service-account token. delenda is NixOS, came back from a
# 36-day outage as a clean base, and has no /etc/isoastra at all — so it cannot
# authenticate a digest pull until these two files exist.
#
# They are pushed from nexus rather than declared in delenda's flake because the
# token is a secret and delenda holds no age key; this mirrors how the forge
# standby is fed. /etc/isoastra is not a nix-managed path, so writing it here
# does not fight the host's own configuration.
#
# The helper stores no password: it exchanges the service-account token for a
# 15-minute JWT on every call, so revoking that token in Kanidm stops pulls
# without touching this host.
provision_delenda_registry_client() {
    local target="$1"

    log "installing the registry pull client on delenda"
    ssh "$target" sudo install -d -o root -g root -m 0755 /etc/isoastra /etc/isoastra/bin
    ssh "$target" sudo install -d -o root -g root -m 0700 /etc/isoastra/secrets

    sudo cat /usr/local/bin/docker-credential-isoastra \
        | ssh "$target" sudo install -o root -g root -m 0755 /dev/stdin \
            /etc/isoastra/bin/docker-credential-isoastra
    sudo cat "/etc/isoastra/secrets/$REGISTRY_CLIENT.token" \
        | ssh "$target" sudo install -o root -g root -m 0600 /dev/stdin \
            "/etc/isoastra/secrets/$REGISTRY_CLIENT.token"

    # Prove the credential actually mints before a deploy depends on it. A dead
    # token fails here, where it is one line of output, rather than mid-rollout
    # as an opaque "unauthorized" from the registry.
    ssh "$target" "printf 'git.isoastra.com' | sudo /etc/isoastra/bin/docker-credential-isoastra get >/dev/null" \
        && log "delenda can mint a registry credential"
}

# --- per-host runtime layout -------------------------------------------------
provision_node() {
    local node_id="$1" name="$2" mesh_ip="$3" target="$4"
    log "provisioning $name ($mesh_ip)"

    # Everything below runs as one root shell on the target so a partially
    # applied host is not a possible outcome of a dropped connection.
    local script
    script=$(cat <<REMOTE
set -euo pipefail

# The data parent must already exist. On the cells and on delenda /data lives on
# the root filesystem rather than a separate volume; that is the existing
# convention on those hosts, not an accident of this script — which is also why
# there is no mountpoint guard here.
test -d /data/apps || { echo "/data/apps missing on \$(hostname -s)" >&2; exit 1; }

install -d -o root -g root -m 0755 $APP_DIR $APP_DIR/oci
install -d -o $RUNTIME_UID -g $RUNTIME_GID -m 0700 $APP_DIR/state
install -d -o root -g root -m 0700 $APP_DIR/secrets

# Identity, not secret: kept in the clear so a reviewer can read which node a
# host believes it is without decrypting anything. The app reports RN_SITE_NODE
# on /readyz, and MESH_IP is what the published port binds to.
cat > $APP_DIR/oci/node.env <<'ENV'
RN_SITE_NODE=$name
MESH_IP=$mesh_ip
RN_SITE_HQL_NODE_ID=$node_id
RN_SITE_HQL_NODES=$HQL_NODES
ENV
chown root:root $APP_DIR/oci/node.env
chmod 0644 $APP_DIR/oci/node.env

# Placeholder only on first run — the deploy role owns this file afterwards and
# overwriting it here would silently roll the host back.
if [ ! -f $APP_DIR/oci/image.env ]; then
    printf '# Previous verified digest (rollback target): none\nRN_SITE_IMAGE=\n' > $APP_DIR/oci/image.env
    chown root:root $APP_DIR/oci/image.env
    chmod 0644 $APP_DIR/oci/image.env
fi

# Registry auth for the digest pull. No password is stored: the credential
# helper exchanges the root-owned service-account token for a short-lived JWT on
# every call.
install -d -o root -g root -m 0700 /etc/rn-site/docker-auth
printf '{"credHelpers": {"git.isoastra.com": "isoastra"}}\n' > /etc/rn-site/docker-auth/config.json
chown root:root /etc/rn-site/docker-auth/config.json
chmod 0600 /etc/rn-site/docker-auth/config.json

# The helper lives in /usr/local/bin on the Debian hosts and in /etc/isoastra/bin
# on delenda, where /usr/local is not a thing. The deploy role puts both on PATH.
command -v docker-credential-isoastra >/dev/null 2>&1 \
    || test -x /etc/isoastra/bin/docker-credential-isoastra \
    || { echo "docker-credential-isoastra is missing on \$(hostname -s)" >&2; exit 1; }

# The runtime identity is numeric and lives only inside the image, but a name
# collision on the host would be a real surprise later, so say what we found.
getent passwd $RUNTIME_UID >/dev/null && echo "note: uid $RUNTIME_UID is a named account on \$(hostname -s)"
getent group  $RUNTIME_GID >/dev/null && echo "note: gid $RUNTIME_GID is a named group on \$(hostname -s)"
true
REMOTE
    )

    # Each file is fed through stdin rather than interpolated into a shell
    # command line, so nothing ever lands in a process listing or shell history.
    local run=(sudo bash -s)
    local put_compose=(sudo install -o root -g root -m 0644 /dev/stdin "$APP_DIR/oci/compose.yaml")
    local check=(sudo stat -c '%n=%U:%G=mode.%a' "$APP_DIR/oci/node.env" "$APP_DIR/oci/compose.yaml" "$APP_DIR/state" "$APP_DIR/secrets/hiqlite-raft" "$APP_DIR/secrets/hiqlite-api")

    if [[ -n "$target" ]]; then
        [[ "$name" == "delenda" ]] && provision_delenda_registry_client "$target"
        run=(ssh "$target" "${run[@]}")
        put_compose=(ssh "$target" "${put_compose[@]}")
        check=(ssh "$target" "${check[@]}")
    fi

    printf '%s\n' "$script" | "${run[@]}"
    "${put_compose[@]}" < "$REPO_ROOT/deploy/compose.yaml"

    # Compose file-backed secrets retain the source file's numeric ownership.
    # UID 9751 can read them inside the container; no unrelated host account can.
    for secret in hiqlite-raft hiqlite-api; do
        if [[ -n "$target" ]]; then
            sudo cat "$CLUSTER_SECRET_DIR/$secret" \
                | ssh "$target" sudo install -o "$RUNTIME_UID" -g "$RUNTIME_GID" -m 0400 /dev/stdin "$APP_DIR/secrets/$secret"
        else
            sudo install -o "$RUNTIME_UID" -g "$RUNTIME_GID" -m 0400 \
                "$CLUSTER_SECRET_DIR/$secret" "$APP_DIR/secrets/$secret"
        fi
    done
    "${check[@]}"
}

ensure_cluster_secrets
selected=("$@")
for entry in "${NODES[@]}"; do
    IFS=: read -r node_id name mesh_ip target <<<"$entry"
    if [[ ${#selected[@]} -gt 0 ]]; then
        printf '%s\n' "${selected[@]}" | grep -qx "$name" || continue
    fi
    provision_node "$node_id" "$name" "$mesh_ip" "$target"
done

log "done. Deploy a digest with the fleet playbook, never by editing image.env by hand."
