//! Guarded, primary-key-preserving import from the legacy v21 events database.
//!
//! This is intentionally a cross-schema operation. The source is attached by a
//! read-only SQLite URI and every target write, including audience backfill and
//! import metadata, is committed in one transaction.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Acquire, AssertSqlSafe, Row};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::Store;
use crate::auth::session::hash_token;

pub const IMPORTED_TABLES: &[&str] = &[
    "identities",
    "accounts",
    "memberships",
    "factors",
    "people",
    "events",
    "event_links",
    "schedule_items",
    "attendance",
    "segment_rsvps",
];

#[derive(Debug, Serialize, Deserialize)]
struct ImportMetadata {
    source: String,
    counts: BTreeMap<String, i64>,
}

#[derive(Debug)]
pub struct VerifyCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug)]
struct LinkForVerification {
    token_plain: String,
    event_id: i64,
    revoked_at: Option<String>,
}

impl Store {
    /// Imports a legacy migration-v21 database into a fresh migration-v33
    /// target. IDs and all legacy values are copied verbatim.
    pub async fn import_legacy_db(&self, source: &Path) -> anyhow::Result<()> {
        let source = canonical_source(source)?;
        validate_legacy_source(&source).await?;

        let mut connection = self.pool.acquire().await?;
        // A failed prior attempt may have returned this pooled connection
        // with its read-only attachment still present. Cleanup is idempotent.
        let _ = sqlx::query("DETACH DATABASE legacy")
            .execute(&mut *connection)
            .await;
        let mut tx = connection.begin().await?;

        let occupied: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM identities) +
                    (SELECT count(*) FROM accounts) +
                    (SELECT count(*) FROM people) +
                    (SELECT count(*) FROM events)",
        )
        .fetch_one(&mut *tx)
        .await?;
        if occupied != 0 {
            bail!(
                "refusing legacy import: target is not empty (identities/accounts/people/events must all have zero rows); use a fresh migrated database before /signup"
            );
        }

        let uri = read_only_uri(&source);
        sqlx::query("ATTACH DATABASE ?1 AS legacy")
            .bind(&uri)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("attach legacy source read-only: {}", source.display()))?;

        let version: Option<i64> =
            sqlx::query_scalar("SELECT MAX(version) FROM legacy._sqlx_migrations")
                .fetch_one(&mut *tx)
                .await
                .context("legacy source has no sqlx migration history")?;
        if version != Some(21) {
            bail!("legacy source must be at migration version 21, found {version:?}");
        }

        // Explicit column lists document the complete v21 -> v33 mapping.
        // New target columns use their declared defaults except the two called
        // out by the import contract (purpose and recovery_email).
        for statement in [
            "INSERT INTO identities (id, kind, display_name, created_at, deleted_at)
             SELECT id, kind, display_name, created_at, deleted_at FROM legacy.identities",
            "INSERT INTO accounts (id, name, kind, created_at, deleted_at, purpose)
             SELECT id, name, kind, created_at, deleted_at, 'primary' FROM legacy.accounts",
            "INSERT INTO memberships (id, identity_id, account_id, role, created_at)
             SELECT id, identity_id, account_id, role, created_at FROM legacy.memberships",
            "INSERT INTO factors (id, identity_id, kind, external_id, secret_hash, metadata,
                                  verified_at, last_used_at, created_at)
             SELECT id, identity_id, kind, external_id, secret_hash, metadata,
                    verified_at, last_used_at, created_at FROM legacy.factors",
            "INSERT INTO people (id, account_id, name, group_label, contact, notes,
                                 created_at, nickname, recovery_email)
             SELECT id, account_id, name, group_label, contact, notes,
                    created_at, nickname, NULL FROM legacy.people",
            "INSERT INTO events (id, account_id, slug, title, tagline, starts_at, ends_at,
                                 timezone, status, summary, area_name, address,
                                 entry_instructions, private_details, created_at, updated_at,
                                 headcount, notice_html, quick_plan_html)
             SELECT id, account_id, slug, title, tagline, starts_at, ends_at,
                    timezone, status, summary, area_name, address,
                    entry_instructions, private_details, created_at, updated_at,
                    headcount, notice_html, quick_plan_html FROM legacy.events",
            "INSERT INTO event_links (id, account_id, event_id, person_id, token_hash,
                                      token_plain, label, tier, revoked_at, uses,
                                      last_used_at, created_at)
             SELECT id, account_id, event_id, person_id, token_hash,
                    token_plain, label, tier, revoked_at, uses,
                    last_used_at, created_at FROM legacy.event_links",
            "INSERT INTO schedule_items (id, account_id, event_id, sort_order, time_label,
                                         title, detail, tier, segment_key)
             SELECT id, account_id, event_id, sort_order, time_label,
                    title, detail, tier, segment_key FROM legacy.schedule_items",
            "INSERT INTO attendance (id, account_id, event_id, person_id, status,
                                     party_size, note, updated_at)
             SELECT id, account_id, event_id, person_id, status,
                    party_size, note, updated_at FROM legacy.attendance",
            "INSERT INTO segment_rsvps (id, account_id, schedule_item_id, person_id,
                                        status, updated_at, paid, attended)
             SELECT id, account_id, schedule_item_id, person_id,
                    status, updated_at, paid, attended FROM legacy.segment_rsvps",
        ] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }

        sqlx::query(
            "INSERT INTO audience_policies (account_id, subject_type, subject_id, public_level)
             SELECT e.account_id, 'event', e.id,
                    CASE WHEN EXISTS (
                        SELECT 1 FROM event_links l
                        WHERE l.account_id = e.account_id AND l.event_id = e.id
                          AND l.tier = 'public' AND l.revoked_at IS NULL
                    ) THEN 'summary' ELSE 'hidden' END
             FROM events e",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO audience_person_overrides
                 (account_id, policy_id, person_id, override_kind, level)
             SELECT l.account_id, p.id, l.person_id, 'include', 'full'
             FROM event_links l
             JOIN audience_policies p
               ON p.account_id = l.account_id
              AND p.subject_type = 'event' AND p.subject_id = l.event_id
             WHERE l.tier = 'private' AND l.person_id IS NOT NULL
               AND l.revoked_at IS NULL",
        )
        .execute(&mut *tx)
        .await?;

        let mut counts = BTreeMap::new();
        for &table in IMPORTED_TABLES {
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT count(*) FROM legacy.{table}"
            )))
            .fetch_one(&mut *tx)
            .await?;
            counts.insert(table.to_owned(), count);
        }
        let metadata = serde_json::to_string(&ImportMetadata {
            source: source.to_string_lossy().into_owned(),
            counts,
        })?;
        sqlx::query(
            "INSERT INTO audit_log (action, entity, detail)
             VALUES ('legacy.import', 'database', ?1)",
        )
        .bind(metadata)
        .execute(&mut *tx)
        .await?;

        // DETACH cannot run while a transaction is active. The import is
        // complete at commit; attachment cleanup is best-effort and cannot
        // turn a committed import into a reported failure.
        tx.commit().await?;
        let _ = sqlx::query("DETACH DATABASE legacy")
            .execute(&mut *connection)
            .await;
        Ok(())
    }

    /// Runs non-mutating import checks. Link checks use the same hash lookup
    /// as the HTTP resolver, without its use-counter side effect.
    pub async fn verify_import(&self) -> anyhow::Result<Vec<VerifyCheck>> {
        let detail: Option<String> = sqlx::query_scalar(
            "SELECT detail FROM audit_log WHERE action = 'legacy.import'
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let metadata: ImportMetadata = serde_json::from_str(
            detail
                .as_deref()
                .context("no legacy import metadata found; run import-legacy-db first")?,
        )?;
        let source = PathBuf::from(&metadata.source);
        let source_pool = open_read_only(&source).await?;

        let mut checks = Vec::new();
        for &table in IMPORTED_TABLES {
            let source_count: i64 =
                sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                    .fetch_one(&source_pool)
                    .await?;
            let target_count: i64 =
                sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                    .fetch_one(&self.pool)
                    .await?;
            let captured = metadata.counts.get(table).copied();
            checks.push(VerifyCheck {
                name: format!("count:{table}"),
                passed: target_count == source_count && captured == Some(source_count),
                detail: format!(
                    "source={source_count} target={target_count} captured={captured:?}"
                ),
            });
        }

        let rows =
            sqlx::query("SELECT token_plain, event_id, revoked_at FROM event_links ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        let mut live_ok = 0usize;
        let mut revoked_ok = 0usize;
        let mut resolver_failures = Vec::new();
        for row in rows {
            let link = LinkForVerification {
                token_plain: row.try_get("token_plain")?,
                event_id: row.try_get("event_id")?,
                revoked_at: row.try_get("revoked_at")?,
            };
            let resolved = self
                .resolve_event_link_read_only(&hash_token(&link.token_plain))
                .await?;
            if link.revoked_at.is_some() {
                if resolved.is_none() {
                    revoked_ok += 1;
                } else {
                    resolver_failures.push("revoked link resolved".to_owned());
                }
            } else if resolved.as_ref().map(|value| value.event_id) == Some(link.event_id) {
                live_ok += 1;
            } else {
                resolver_failures.push("live link did not resolve to its event".to_owned());
            }
        }
        checks.push(VerifyCheck {
            name: "resolver:all-links".into(),
            passed: resolver_failures.is_empty(),
            detail: format!(
                "live_resolved={live_ok} revoked_not_found={revoked_ok} failures={}",
                resolver_failures.len()
            ),
        });

        push_scalar_check(
            &mut checks,
            "audience:one-policy-per-event",
            sqlx::query_scalar(
                "SELECT count(*) FROM events e
                 WHERE (SELECT count(*) FROM audience_policies p
                        WHERE p.subject_type='event' AND p.subject_id=e.id) <> 1",
            )
            .fetch_one(&self.pool)
            .await?,
            "events with policy count other than one",
        );

        let expected_overrides: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM event_links
             WHERE tier='private' AND person_id IS NOT NULL AND revoked_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        let actual_overrides: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audience_person_overrides
             WHERE override_kind='include' AND level='full'",
        )
        .fetch_one(&self.pool)
        .await?;
        let incorrect_overrides: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audience_person_overrides o
             JOIN audience_policies p ON p.id=o.policy_id
             WHERE p.subject_type <> 'event' OR o.override_kind <> 'include'
                OR o.level <> 'full' OR NOT EXISTS (
                    SELECT 1 FROM event_links l
                    WHERE l.account_id=o.account_id AND l.event_id=p.subject_id
                      AND l.person_id=o.person_id AND l.tier='private'
                      AND l.revoked_at IS NULL
                )",
        )
        .fetch_one(&self.pool)
        .await?;
        checks.push(VerifyCheck {
            name: "audience:private-overrides".into(),
            passed: actual_overrides == expected_overrides && incorrect_overrides == 0,
            detail: format!(
                "expected={expected_overrides} actual={actual_overrides} incorrect={incorrect_overrides}"
            ),
        });

        push_scalar_check(
            &mut checks,
            "audience:public-level",
            sqlx::query_scalar(
                "SELECT count(*) FROM audience_policies p
                 WHERE p.subject_type='event' AND p.public_level <>
                   CASE WHEN EXISTS (
                     SELECT 1 FROM event_links l
                     WHERE l.account_id=p.account_id AND l.event_id=p.subject_id
                       AND l.tier='public' AND l.revoked_at IS NULL
                   ) THEN 'summary' ELSE 'hidden' END",
            )
            .fetch_one(&self.pool)
            .await?,
            "policies inconsistent with live public links",
        );

        push_scalar_check(
            &mut checks,
            "sessions:dropped",
            sqlx::query_scalar("SELECT count(*) FROM sessions")
                .fetch_one(&self.pool)
                .await?,
            "target session rows",
        );
        push_scalar_check(
            &mut checks,
            "owner:primary-purpose",
            sqlx::query_scalar(
                "SELECT CASE WHEN count(*) > 0
                              AND sum(CASE WHEN a.purpose <> 'primary' THEN 1 ELSE 0 END) = 0
                             THEN 0 ELSE 1 END
                 FROM memberships m JOIN accounts a ON a.id=m.account_id
                 WHERE m.role='owner'",
            )
            .fetch_one(&self.pool)
            .await?,
            "missing owner or owner memberships on non-primary accounts",
        );

        source_pool.close().await;
        Ok(checks)
    }
}

fn push_scalar_check(checks: &mut Vec<VerifyCheck>, name: &str, failures: i64, label: &str) {
    checks.push(VerifyCheck {
        name: name.to_owned(),
        passed: failures == 0,
        detail: format!("{label}={failures}"),
    });
}

fn canonical_source(source: &Path) -> anyhow::Result<PathBuf> {
    source
        .canonicalize()
        .with_context(|| format!("legacy source does not exist: {}", source.display()))
}

fn read_only_uri(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let normalized = normalized.strip_prefix("//?/").unwrap_or(&normalized);
    if normalized.as_bytes().get(1) == Some(&b':') {
        format!("file:///{normalized}?mode=ro&immutable=1")
    } else {
        format!("file:{normalized}?mode=ro&immutable=1")
    }
}

async fn open_read_only(path: &Path) -> anyhow::Result<sqlx::SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true)
        .immutable(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("open legacy source read-only: {}", path.display()))
}

async fn validate_legacy_source(path: &Path) -> anyhow::Result<()> {
    let pool = open_read_only(path).await?;
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .context("legacy source has no sqlx migration history")?;
    pool.close().await;
    if version != Some(21) {
        bail!("legacy source must be at migration version 21, found {version:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use sqlx::{Connection, SqliteConnection};

    use super::*;
    use crate::access::level::Level;
    use crate::auth::viewer::Viewer;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    #[tokio::test]
    async fn legacy_import_preserves_ids_resolves_links_and_backfills_once() {
        let path = std::env::temp_dir().join(format!(
            "ronitnath-v21-fixture-{}-{}.db",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        create_fixture(&path).await;
        let store = Store::connect_in_memory().await;

        store.import_legacy_db(&path).await.unwrap();

        let person = store.find_person(7, 42).await.unwrap().unwrap();
        assert_eq!(person.name, "Known Person");
        let live = store
            .resolve_event_link_read_only(&hash_token("person-live"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(live.event_id, 80);
        assert!(
            store
                .resolve_event_link_read_only(&hash_token("person-revoked"))
                .await
                .unwrap()
                .is_none()
        );

        let inputs = store
            .audience_inputs_for_event(7, 80, Some(42))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            inputs
                .level_for_direct_hit(
                    &Viewer::LinkHolder {
                        person_id: Some(42),
                        event_id: 80,
                    },
                    "private",
                )
                .unwrap(),
            Level::Full
        );
        let shared = store
            .resolve_event_link_read_only(&hash_token("shared-private"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shared.person_id, None);
        assert_eq!(
            inputs
                .level_for_direct_hit(
                    &Viewer::LinkHolder {
                        person_id: None,
                        event_id: 80,
                    },
                    &shared.tier,
                )
                .unwrap(),
            Level::Full
        );

        let checks = store.verify_import().await.unwrap();
        assert!(
            checks.iter().all(|check| check.passed),
            "failed checks: {checks:#?}"
        );
        let second = store.import_legacy_db(&path).await.unwrap_err().to_string();
        assert!(second.contains("target is not empty"), "{second}");

        let _ = std::fs::remove_file(path);
    }

    async fn create_fixture(path: &Path) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        sqlx::raw_sql(
            r#"
            PRAGMA foreign_keys=ON;
            CREATE TABLE _sqlx_migrations (version INTEGER NOT NULL);
            INSERT INTO _sqlx_migrations VALUES (21);
            CREATE TABLE identities (id INTEGER PRIMARY KEY NOT NULL, kind TEXT NOT NULL,
                display_name TEXT NOT NULL, created_at TEXT NOT NULL, deleted_at TEXT);
            CREATE TABLE accounts (id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL,
                kind TEXT NOT NULL, created_at TEXT NOT NULL, deleted_at TEXT);
            CREATE TABLE memberships (id INTEGER PRIMARY KEY NOT NULL, identity_id INTEGER NOT NULL,
                account_id INTEGER NOT NULL, role TEXT NOT NULL, created_at TEXT NOT NULL);
            CREATE TABLE factors (id INTEGER PRIMARY KEY NOT NULL, identity_id INTEGER NOT NULL,
                kind TEXT NOT NULL, external_id TEXT, secret_hash TEXT, metadata TEXT NOT NULL,
                verified_at TEXT, last_used_at TEXT, created_at TEXT NOT NULL);
            CREATE TABLE sessions (id INTEGER PRIMARY KEY NOT NULL, token_hash TEXT NOT NULL,
                csrf_token TEXT NOT NULL, identity_id INTEGER NOT NULL, account_id INTEGER NOT NULL,
                created_at TEXT NOT NULL, expires_at TEXT NOT NULL, last_seen_at TEXT NOT NULL,
                revoked_at TEXT, user_agent TEXT, ip TEXT);
            CREATE TABLE people (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                name TEXT NOT NULL, group_label TEXT NOT NULL, contact TEXT NOT NULL,
                notes TEXT NOT NULL, created_at TEXT NOT NULL, nickname TEXT NOT NULL);
            CREATE TABLE events (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                slug TEXT NOT NULL, title TEXT NOT NULL, tagline TEXT NOT NULL, starts_at TEXT NOT NULL,
                ends_at TEXT, timezone TEXT NOT NULL, status TEXT NOT NULL, summary TEXT NOT NULL,
                area_name TEXT NOT NULL, address TEXT NOT NULL, entry_instructions TEXT NOT NULL,
                private_details TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                headcount INTEGER, notice_html TEXT NOT NULL, quick_plan_html TEXT NOT NULL);
            CREATE TABLE event_links (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                event_id INTEGER NOT NULL, person_id INTEGER, token_hash TEXT NOT NULL,
                token_plain TEXT NOT NULL, label TEXT NOT NULL, tier TEXT NOT NULL, revoked_at TEXT,
                uses INTEGER NOT NULL, last_used_at TEXT, created_at TEXT NOT NULL);
            CREATE TABLE schedule_items (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                event_id INTEGER NOT NULL, sort_order INTEGER NOT NULL, time_label TEXT NOT NULL,
                title TEXT NOT NULL, detail TEXT NOT NULL, tier TEXT NOT NULL, segment_key TEXT);
            CREATE TABLE attendance (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                event_id INTEGER NOT NULL, person_id INTEGER NOT NULL, status TEXT NOT NULL,
                party_size INTEGER NOT NULL, note TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE TABLE segment_rsvps (id INTEGER PRIMARY KEY NOT NULL, account_id INTEGER NOT NULL,
                schedule_item_id INTEGER NOT NULL, person_id INTEGER NOT NULL, status TEXT NOT NULL,
                updated_at TEXT NOT NULL, paid INTEGER NOT NULL, attended INTEGER);
            INSERT INTO identities VALUES (5,'human','Owner','2026-01-01',NULL);
            INSERT INTO accounts VALUES (7,'Owner','personal','2026-01-01',NULL);
            INSERT INTO memberships VALUES (9,5,7,'owner','2026-01-01');
            INSERT INTO factors VALUES (11,5,'password','owner@example.test','fixture-hash','{}',NULL,NULL,'2026-01-01');
            INSERT INTO sessions VALUES (13,'old-session','old-csrf',5,7,'2026-01-01','2027-01-01','2026-01-01',NULL,NULL,NULL);
            INSERT INTO people VALUES (42,7,'Known Person','','','','2026-01-01','KP');
            INSERT INTO events VALUES (80,7,'private-event','Private Event','','2026-07-01',NULL,
                'America/Los_Angeles','published','Summary','Area','123 Full Address','Door','Private',
                '2026-01-01','2026-01-01',NULL,'','');
            INSERT INTO events VALUES (81,7,'public-event','Public Event','','2026-08-01',NULL,
                'America/Los_Angeles','published','Summary','Area','456 Address','','',
                '2026-01-01','2026-01-01',NULL,'','');
            INSERT INTO schedule_items VALUES (90,7,80,0,'6pm','Dinner','','private',NULL);
            INSERT INTO attendance VALUES (100,7,80,42,'going',1,'','2026-01-01');
            INSERT INTO segment_rsvps VALUES (110,7,90,42,'in','2026-01-01',0,NULL);
            "#,
        )
        .execute(&mut connection)
        .await
        .unwrap();

        for (id, event_id, person_id, plain, tier, revoked) in [
            (
                120_i64,
                80_i64,
                Some(42_i64),
                "person-live",
                "private",
                None,
            ),
            (121, 80, None, "shared-private", "private", None),
            (122, 81, None, "shared-public", "public", None),
            (
                123,
                80,
                Some(42),
                "person-revoked",
                "private",
                Some("2026-02-01"),
            ),
        ] {
            sqlx::query(
                "INSERT INTO event_links
                 (id,account_id,event_id,person_id,token_hash,token_plain,label,tier,revoked_at,uses,last_used_at,created_at)
                 VALUES (?1,7,?2,?3,?4,?5,'fixture',?6,?7,0,NULL,'2026-01-01')",
            )
            .bind(id)
            .bind(event_id)
            .bind(person_id)
            .bind(hash_token(plain))
            .bind(plain)
            .bind(tier)
            .bind(revoked)
            .execute(&mut connection)
            .await
            .unwrap();
        }
        connection.close().await.unwrap();
    }
}
