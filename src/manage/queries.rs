//! Read-only queries behind the data browser.
//!
//! Every query joins internal ids away before the data leaves this module:
//! the [`Cell`]s a query returns hold `public_id`s, emails, and names only.
//! Redaction happens here too — the password query keeps the PHC algorithm
//! and drops the rest; the session query keeps a digest prefix — so the
//! renderer never holds a secret it could accidentally print.

use hiqlite::{Client, Error as HiqliteError, params};

/// How many rows a model page shows, newest first. The page says so when the
/// table holds more.
pub const ROW_LIMIT: i64 = 200;

/// Every durable data model the browser can show.
///
/// `ALL` is the index page's source of truth: adding a table to the schema
/// means adding a variant here, and the match arms below refuse to compile
/// until the new model can be counted, queried, and described.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataModel {
    Identities,
    IdentityEmails,
    IdentityPasswords,
    Accounts,
    AccountMemberships,
    MembershipCapabilities,
    Sessions,
}

impl DataModel {
    pub const ALL: &[Self] = &[
        Self::Identities,
        Self::IdentityEmails,
        Self::IdentityPasswords,
        Self::Accounts,
        Self::AccountMemberships,
        Self::MembershipCapabilities,
        Self::Sessions,
    ];

    /// URL path segment under `/manage/`.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Identities => "identities",
            Self::IdentityEmails => "identity-emails",
            Self::IdentityPasswords => "identity-passwords",
            Self::Accounts => "accounts",
            Self::AccountMemberships => "account-memberships",
            Self::MembershipCapabilities => "membership-capabilities",
            Self::Sessions => "sessions",
        }
    }

    #[must_use]
    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.slug() == slug)
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Identities => "Identities",
            Self::IdentityEmails => "Identity emails",
            Self::IdentityPasswords => "Identity passwords",
            Self::Accounts => "Accounts",
            Self::AccountMemberships => "Account memberships",
            Self::MembershipCapabilities => "Membership capabilities",
            Self::Sessions => "Sessions",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Identities => "People and services that can hold credentials and sessions.",
            Self::IdentityEmails => "Addresses attached to identities; the normalized form is the unique login key.",
            Self::IdentityPasswords => "One credential per identity. Only the hash algorithm is shown here.",
            Self::Accounts => "Ownership boundaries. Every identity gets a primary account at registration.",
            Self::AccountMemberships => "Who belongs to which account, and the role whose bundle grants their default capabilities.",
            Self::MembershipCapabilities => "Explicit capability grants — the auditable exceptions on top of role bundles.",
            Self::Sessions => "Live sign-ins. A revoked session has no row; only a digest prefix of the token is stored or shown.",
        }
    }

    /// Count query per model. Static strings because the SQL the client takes
    /// must be `'static` — and because a table name must never be data.
    const fn count_sql(self) -> &'static str {
        match self {
            Self::Identities => "SELECT COUNT(*) AS n FROM identities",
            Self::IdentityEmails => "SELECT COUNT(*) AS n FROM identity_emails",
            Self::IdentityPasswords => "SELECT COUNT(*) AS n FROM identity_passwords",
            Self::Accounts => "SELECT COUNT(*) AS n FROM accounts",
            Self::AccountMemberships => "SELECT COUNT(*) AS n FROM account_memberships",
            Self::MembershipCapabilities => "SELECT COUNT(*) AS n FROM membership_capabilities",
            Self::Sessions => "SELECT COUNT(*) AS n FROM sessions",
        }
    }
}

impl std::fmt::Display for DataModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}

/// One display value. The renderer decides markup per kind; the data here is
/// plain text, escaped at render time.
#[derive(Clone, Debug)]
pub enum Cell {
    /// Machine-given value: UUID, capability name, digest prefix.
    Mono(String),
    /// Human text: email, account name, user agent.
    Text(String),
    /// Enum-ish value rendered quietly: kind, status, role.
    Tag(String),
    /// Unix-millis timestamp.
    Time(i64),
    /// Column is nullable and this row has no value.
    None,
}

impl Cell {
    fn opt_time(ms: Option<i64>) -> Self {
        ms.map_or(Self::None, Self::Time)
    }

    fn opt_text(s: Option<String>) -> Self {
        s.map_or(Self::None, Self::Text)
    }

    /// Plain-text form, for the CLI. The HTML form lives in the renderer.
    #[must_use]
    pub fn plain(&self) -> String {
        match self {
            Self::Mono(v) | Self::Text(v) | Self::Tag(v) => v.clone(),
            Self::Time(ms) => fmt_utc(*ms),
            Self::None => "—".to_string(),
        }
    }
}

/// `2026-08-09 07:14` from unix millis, UTC. (Howard Hinnant's civil-date
/// algorithm; no chrono dependency for one format.)
#[must_use]
pub fn fmt_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);

    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        tod / 3600,
        (tod % 3600) / 60
    )
}

/// Column headers plus the rows to show, newest first.
#[derive(Debug)]
pub struct TableData {
    pub columns: &'static [&'static str],
    pub rows: Vec<Vec<Cell>>,
}

/// Total rows in the model's backing table.
pub async fn count(db: &Client, model: DataModel) -> Result<i64, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct CountRow {
        n: i64,
    }
    let row: CountRow = db.query_as_one(model.count_sql(), params!()).await?;
    Ok(row.n)
}

/// The latest [`ROW_LIMIT`] rows of one model, display-ready.
pub async fn rows(db: &Client, model: DataModel) -> Result<TableData, HiqliteError> {
    match model {
        DataModel::Identities => identities(db).await,
        DataModel::IdentityEmails => identity_emails(db).await,
        DataModel::IdentityPasswords => identity_passwords(db).await,
        DataModel::Accounts => accounts(db).await,
        DataModel::AccountMemberships => account_memberships(db).await,
        DataModel::MembershipCapabilities => membership_capabilities(db).await,
        DataModel::Sessions => sessions(db).await,
    }
}

async fn identities(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        public_id: String,
        kind: String,
        display_name: Option<String>,
        status: String,
        created_at: i64,
        updated_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT public_id, kind, display_name, status, created_at, updated_at
             FROM identities ORDER BY id DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Public id", "Kind", "Display name", "Status", "Created", "Updated"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Mono(r.public_id),
                    Cell::Tag(r.kind),
                    Cell::opt_text(r.display_name),
                    Cell::Tag(r.status),
                    Cell::Time(r.created_at),
                    Cell::Time(r.updated_at),
                ]
            })
            .collect(),
    })
}

async fn identity_emails(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        email: String,
        email_normalized: String,
        identity_public_id: String,
        verified_at: Option<i64>,
        is_primary: i64,
        created_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT e.email, e.email_normalized, i.public_id AS identity_public_id,
                    e.verified_at, e.is_primary, e.created_at
             FROM identity_emails e
             JOIN identities i ON i.id = e.identity_id
             ORDER BY e.id DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Email", "Normalized", "Identity", "Verified", "Primary", "Created"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Text(r.email),
                    Cell::Text(r.email_normalized),
                    Cell::Mono(r.identity_public_id),
                    Cell::opt_time(r.verified_at),
                    Cell::Tag(if r.is_primary != 0 { "primary" } else { "secondary" }.into()),
                    Cell::Time(r.created_at),
                ]
            })
            .collect(),
    })
}

async fn identity_passwords(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        identity_public_id: String,
        password_hash: String,
        created_at: i64,
        rotated_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT i.public_id AS identity_public_id, p.password_hash,
                    p.created_at, p.rotated_at
             FROM identity_passwords p
             JOIN identities i ON i.id = p.identity_id
             ORDER BY p.identity_id DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Identity", "Algorithm", "Created", "Rotated"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Mono(r.identity_public_id),
                    // The hash dies here: only the PHC algorithm name survives
                    // into display data.
                    Cell::Tag(phc_algorithm(&r.password_hash).to_string()),
                    Cell::Time(r.created_at),
                    Cell::Time(r.rotated_at),
                ]
            })
            .collect(),
    })
}

/// Algorithm name out of a PHC string (`$argon2id$...` → `argon2id`).
fn phc_algorithm(phc: &str) -> &str {
    phc.split('$').nth(1).filter(|a| !a.is_empty()).unwrap_or("unknown")
}

async fn accounts(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        public_id: String,
        kind: String,
        name: String,
        status: String,
        primary_for: Option<String>,
        created_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT a.public_id, a.kind, a.name, a.status,
                    i.public_id AS primary_for, a.created_at
             FROM accounts a
             LEFT JOIN identities i ON i.id = a.primary_for_identity_id
             ORDER BY a.id DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Public id", "Kind", "Name", "Status", "Primary for", "Created"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Mono(r.public_id),
                    Cell::Tag(r.kind),
                    Cell::Text(r.name),
                    Cell::Tag(r.status),
                    r.primary_for.map_or(Cell::None, Cell::Mono),
                    Cell::Time(r.created_at),
                ]
            })
            .collect(),
    })
}

async fn account_memberships(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        account_name: String,
        account_public_id: String,
        identity_public_id: String,
        email: Option<String>,
        role: String,
        created_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT a.name AS account_name, a.public_id AS account_public_id,
                    i.public_id AS identity_public_id,
                    (SELECT e.email FROM identity_emails e
                     WHERE e.identity_id = m.identity_id AND e.is_primary = 1) AS email,
                    m.role, m.created_at
             FROM account_memberships m
             JOIN accounts a ON a.id = m.account_id
             JOIN identities i ON i.id = m.identity_id
             ORDER BY m.created_at DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Account", "Account id", "Member", "Identity", "Role", "Created"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Text(r.account_name),
                    Cell::Mono(r.account_public_id),
                    Cell::opt_text(r.email),
                    Cell::Mono(r.identity_public_id),
                    Cell::Tag(r.role),
                    Cell::Time(r.created_at),
                ]
            })
            .collect(),
    })
}

async fn membership_capabilities(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        capability: String,
        account_name: String,
        account_public_id: String,
        identity_public_id: String,
        granted_at: i64,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT c.capability, a.name AS account_name,
                    a.public_id AS account_public_id,
                    i.public_id AS identity_public_id, c.granted_at
             FROM membership_capabilities c
             JOIN accounts a ON a.id = c.account_id
             JOIN identities i ON i.id = c.identity_id
             ORDER BY c.granted_at DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Capability", "Account", "Account id", "Identity", "Granted"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    Cell::Mono(r.capability),
                    Cell::Text(r.account_name),
                    Cell::Mono(r.account_public_id),
                    Cell::Mono(r.identity_public_id),
                    Cell::Time(r.granted_at),
                ]
            })
            .collect(),
    })
}

async fn sessions(db: &Client) -> Result<TableData, HiqliteError> {
    #[derive(serde::Deserialize)]
    struct R {
        token_hash: String,
        identity_public_id: String,
        account_public_id: String,
        created_at: i64,
        expires_at: i64,
        last_seen_at: i64,
        user_agent: Option<String>,
    }
    let rows: Vec<R> = db
        .query_as(
            "SELECT s.token_hash, i.public_id AS identity_public_id,
                    a.public_id AS account_public_id,
                    s.created_at, s.expires_at, s.last_seen_at, s.user_agent
             FROM sessions s
             JOIN identities i ON i.id = s.identity_id
             JOIN accounts a ON a.id = s.account_id
             ORDER BY s.id DESC LIMIT $1",
            params!(ROW_LIMIT),
        )
        .await?;
    Ok(TableData {
        columns: &["Digest", "Identity", "Account", "Created", "Expires", "Last seen", "User agent"],
        rows: rows
            .into_iter()
            .map(|r| {
                vec![
                    // The stored value is already a digest, not the bearer
                    // token — but even the digest is cut to a prefix so the
                    // page cannot be used as a lookup table against a dump.
                    Cell::Mono(format!("{}…", &r.token_hash[..r.token_hash.len().min(8)])),
                    Cell::Mono(r.identity_public_id),
                    Cell::Mono(r.account_public_id),
                    Cell::Time(r.created_at),
                    Cell::Time(r.expires_at),
                    Cell::Time(r.last_seen_at),
                    Cell::opt_text(r.user_agent),
                ]
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_round_trips_its_slug() {
        for model in DataModel::ALL {
            assert_eq!(DataModel::parse(model.slug()), Some(*model));
        }
        assert_eq!(DataModel::parse("nope"), None);
        assert_eq!(DataModel::parse("Identities"), None, "slugs are exact, not case-folded");
    }

    #[test]
    fn utc_formatting_hits_known_instants() {
        assert_eq!(fmt_utc(0), "1970-01-01 00:00");
        // 2026-08-09 00:00:00 UTC.
        assert_eq!(fmt_utc(1_786_233_600_000), "2026-08-09 00:00");
        // Leap-day 2024-02-29 12:30 UTC.
        assert_eq!(fmt_utc(1_709_209_800_000), "2024-02-29 12:30");
        // Pre-epoch stays sane rather than underflowing.
        assert_eq!(fmt_utc(-86_400_000), "1969-12-31 00:00");
    }

    #[test]
    fn phc_algorithm_survives_odd_inputs() {
        assert_eq!(phc_algorithm("$argon2id$v=19$m=19456"), "argon2id");
        assert_eq!(phc_algorithm("not-a-phc"), "unknown");
        assert_eq!(phc_algorithm(""), "unknown");
        assert_eq!(phc_algorithm("$$"), "unknown");
    }
}
