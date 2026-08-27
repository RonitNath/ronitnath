//! The migration set, as a value, plus the integrity check that runs before it.
//!
//! The kernel owns the schema (`docs/rebuild/plan.md` §Workspace). Until K1
//! hands over `rn_kernel::migrations()`, [`Migrations::default`] is the empty
//! set and boot applies nothing — the seam is a value the caller passes in, so
//! adopting the kernel's set is one line at the call site and no change here.
//!
//! The integrity check is the part worth keeping verbatim in behaviour: a
//! migration whose recorded name or content hash no longer matches the embedded
//! history means the binary and the database disagree about what the schema
//! *is*, and continuing would apply the next migration onto a shape nobody has
//! described. It refuses to start instead.

use std::time::Duration;

use hiqlite::{AppliedMigration, Client, Params, params};
use tracing::info;

/// One schema step: `<id>_<name>.sql`, its sha-256, and its statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub id: u32,
    pub name: String,
    /// Lowercase hex sha-256 of `sql`, the same digest hiqlite records.
    pub hash: String,
    pub sql: String,
}

/// An ordered, gap-free migration history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Migrations(Vec<Migration>);

impl Migrations {
    /// Build from a `rust_embed` folder of `<id>_<name>.sql` files.
    pub fn embedded<T: rust_embed::Embed>() -> Result<Self, IntegrityError> {
        let mut steps = Vec::new();
        for filename in T::iter() {
            let filename = filename.into_owned();
            let (id, rest) = filename
                .split_once('_')
                .ok_or_else(|| IntegrityError::name(0, "<integer>_<name>.sql", &filename))?;
            let id = id
                .parse::<u32>()
                .map_err(|_| IntegrityError::name(0, "a positive integer", id))?;
            let name = rest
                .strip_suffix(".sql")
                .ok_or_else(|| IntegrityError::name(id, "a .sql suffix", &filename))?;
            let file =
                T::get(&filename).ok_or_else(|| IntegrityError::name(id, &filename, "missing"))?;
            steps.push(Migration {
                id,
                name: name.to_string(),
                hash: hex(&file.metadata.sha256_hash()),
                sql: String::from_utf8(file.data.into_owned())
                    .map_err(|_| IntegrityError::name(id, "utf-8 sql", "invalid utf-8"))?,
            });
        }
        steps.sort_by_key(|step| step.id);
        Self::sequenced(steps, "embedded sequence")
    }

    fn sequenced(steps: Vec<Migration>, field: &'static str) -> Result<Self, IntegrityError> {
        for (index, step) in steps.iter().enumerate() {
            let expected = u32::try_from(index + 1).expect("migration count fits u32");
            if step.id != expected {
                return Err(IntegrityError {
                    migration_id: expected,
                    field,
                    expected: expected.to_string(),
                    actual: step.id.to_string(),
                });
            }
        }
        Ok(Self(steps))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// How long boot will keep trying to migrate a cluster that is still forming.
const PATIENCE: Duration = Duration::from_secs(30);

/// How long between attempts. An election is a second or two, so this is
/// several tries per election rather than one.
const RETRY: Duration = Duration::from_millis(500);

/// Apply everything not yet applied, refusing if the history has drifted.
///
/// Retried, because a node booting into a cluster meets two races that are
/// not failures: a write forwarded to a leader that is mid-election
/// (`ClientWriteError: has to forward request to: None, None`) and a read of
/// a table this node's own state machine has not applied yet (`no such table:
/// _migrations`). Both were seen during harness development, and both took
/// the process down on boot — which needs a supervisor to undo, and
/// `tools/cluster.sh` is not one (finding 7, `docs/perf/2026-08-27.md`).
pub async fn apply(client: &Client, migrations: &Migrations) -> Result<(), MigrateError> {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        match apply_once(client, migrations).await {
            Ok(()) => return Ok(()),
            Err(MigrateError::Hiqlite(error))
                if forming(&error) && std::time::Instant::now() < deadline =>
            {
                info!(%error, "the cluster is still forming; retrying the migration");
                tokio::time::sleep(RETRY).await;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Whether an error is a cluster that has not settled rather than one that
/// has refused. Matched on the message because hiqlite passes most of these
/// through as strings, and guessing the other way — treating an unrecognised
/// error as transient — would turn a real schema refusal into a 30-second
/// hang followed by the same refusal.
fn forming(error: &hiqlite::Error) -> bool {
    let message = error.to_string();
    message.contains("forward request to")
        || message.contains("no such table: _migrations")
        || message.contains("has no leader")
        || message.contains("Leader vote is in progress")
}

async fn apply_once(client: &Client, migrations: &Migrations) -> Result<(), MigrateError> {
    client
        .execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                 id   INTEGER PRIMARY KEY NOT NULL,
                 name TEXT    NOT NULL,
                 ts   INTEGER NOT NULL,
                 hash TEXT    NOT NULL
             )",
            params!(),
        )
        .await?;

    // Read back through the leader, which is where the `CREATE TABLE` above
    // was applied. A local read here asks this node's own state machine about
    // a table it may not have applied yet, and answers "no such table" for a
    // statement that has already committed — the same write and the same
    // read, disagreeing, because they were not the same consistency path.
    let applied: Vec<AppliedMigration> = client
        .query_consistent_map("SELECT * FROM _migrations ORDER BY id ASC", Params::new())
        .await?;
    check(&migrations.0, &applied)?;

    for step in migrations.0.iter().skip(applied.len()) {
        for result in client.batch(step.sql.clone()).await? {
            result?;
        }
        client
            .execute(
                "INSERT INTO _migrations (id, name, ts, hash) VALUES ($1, $2, $3, $4)",
                params!(
                    i64::from(step.id),
                    step.name.clone(),
                    now_secs(),
                    step.hash.clone()
                ),
            )
            .await?;
        info!(migration = %format!("{}_{}", step.id, step.name), "applied migration");
    }
    Ok(())
}

/// Compare what the database has applied with what this binary embeds.
fn check(embedded: &[Migration], applied: &[AppliedMigration]) -> Result<(), IntegrityError> {
    for (index, step) in applied.iter().enumerate() {
        let expected_id = u32::try_from(index + 1).expect("migration count fits u32");
        if step.id != expected_id {
            return Err(IntegrityError {
                migration_id: expected_id,
                field: "applied sequence",
                expected: expected_id.to_string(),
                actual: step.id.to_string(),
            });
        }
        let Some(known) = embedded.get(index) else {
            return Err(IntegrityError::name(
                step.id,
                &format!("{}_{}", step.id, step.name),
                "missing",
            ));
        };
        if known.name != step.name {
            return Err(IntegrityError {
                migration_id: step.id,
                field: "name",
                expected: known.name.clone(),
                actual: step.name.clone(),
            });
        }
        if known.hash != step.hash {
            return Err(IntegrityError {
                migration_id: step.id,
                field: "hash",
                expected: known.hash.clone(),
                actual: step.hash.clone(),
            });
        }
    }
    Ok(())
}

/// A previously applied migration no longer matches the embedded history.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("migration {migration_id}: {field} should be {expected}, database has {actual}")]
pub struct IntegrityError {
    pub migration_id: u32,
    pub field: &'static str,
    pub expected: String,
    pub actual: String,
}

impl IntegrityError {
    fn name(migration_id: u32, expected: &str, actual: &str) -> Self {
        Self {
            migration_id,
            field: "filename",
            expected: expected.to_string(),
            actual: actual.to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error(transparent)]
    Hiqlite(#[from] hiqlite::Error),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded(steps: &[(u32, &str, &str)]) -> Vec<Migration> {
        steps
            .iter()
            .map(|(id, name, hash)| Migration {
                id: *id,
                name: (*name).to_string(),
                hash: (*hash).to_string(),
                sql: String::new(),
            })
            .collect()
    }

    fn applied(steps: &[(u32, &str, &str)]) -> Vec<AppliedMigration> {
        steps
            .iter()
            .map(|(id, name, hash)| AppliedMigration {
                id: *id,
                name: (*name).to_string(),
                ts: 0,
                hash: (*hash).to_string(),
            })
            .collect()
    }

    #[test]
    fn an_appended_migration_is_the_normal_case_and_is_accepted() {
        let known = embedded(&[(1, "kernel", "aaa"), (2, "documents", "bbb")]);
        assert_eq!(check(&known, &applied(&[(1, "kernel", "aaa")])), Ok(()));
    }

    #[test]
    fn editing_an_already_applied_migration_refuses_the_boot() {
        let error = check(
            &embedded(&[(1, "kernel", "new")]),
            &applied(&[(1, "kernel", "old")]),
        )
        .unwrap_err();
        assert_eq!(error.field, "hash");
        assert_eq!(error.actual, "old");
    }

    #[test]
    fn renaming_an_already_applied_migration_refuses_the_boot() {
        let error = check(
            &embedded(&[(1, "parties", "aaa")]),
            &applied(&[(1, "kernel", "aaa")]),
        )
        .unwrap_err();
        assert_eq!(error.field, "name");
    }

    #[test]
    fn a_database_ahead_of_the_binary_refuses_the_boot() {
        let error = check(
            &embedded(&[(1, "kernel", "aaa")]),
            &applied(&[(1, "kernel", "aaa"), (2, "documents", "bbb")]),
        )
        .unwrap_err();
        assert_eq!(error.migration_id, 2);
        assert_eq!(error.actual, "missing");
    }

    #[test]
    fn a_gap_on_either_side_refuses_the_boot() {
        let gapped = vec![
            Migration {
                id: 1,
                name: "a".into(),
                hash: "x".into(),
                sql: String::new(),
            },
            Migration {
                id: 3,
                name: "c".into(),
                hash: "z".into(),
                sql: String::new(),
            },
        ];
        assert_eq!(
            Migrations::sequenced(gapped, "embedded sequence")
                .unwrap_err()
                .migration_id,
            2
        );
        let error = check(
            &embedded(&[(1, "a", "x"), (2, "b", "y"), (3, "c", "z")]),
            &applied(&[(1, "a", "x"), (3, "c", "z")]),
        )
        .unwrap_err();
        assert_eq!(error.field, "applied sequence");
    }

    #[test]
    fn the_seam_starts_empty_so_boot_applies_nothing_until_the_kernel_lands() {
        assert!(Migrations::default().is_empty());
        assert_eq!(check(&[], &[]), Ok(()));
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
