//! `rn-site admin` — operator commands against the local database.
//!
//! These open the same hiqlite node the server does, on the same data
//! directory and ports, so they must run **while the server is stopped**; a
//! second node on a held directory fails to bind rather than corrupting
//! anything. That constraint is acceptable because these commands exist for
//! bootstrap and break-glass moments — creating the first `manage`-capable
//! identity, granting a capability no UI can grant yet — not for routine
//! administration, which belongs in `/manage` once it grows write surfaces.
//!
//! Reads reuse [`crate::manage::queries`], so `admin list` shows exactly what
//! `/manage` shows — one definition of every view, no drift between the CLI
//! and the page.

use clap::Subcommand;
use hiqlite::Client;

use crate::auth::{Capability, store};
use crate::manage::{DataModel, queries};
use crate::operations::{config::AppConfig, db};

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// Create an identity with a primary account (the registration path).
    CreateIdentity {
        #[arg(long)]
        email: String,
        /// Omit to read the password from stdin (one line, not echoed back).
        #[arg(long)]
        password: Option<String>,
        /// Capabilities to grant the new membership, e.g. `--grant manage`.
        #[arg(long)]
        grant: Vec<String>,
    },
    /// Grant a capability to the primary-account membership of an address.
    Grant {
        #[arg(long)]
        email: String,
        #[arg(long)]
        capability: String,
    },
    /// Show a data model (the same view `/manage` renders), or list them all.
    List {
        /// Model slug, e.g. `identities` or `sessions`. Omit for the index.
        model: Option<String>,
    },
}

/// Open the database, run one command, and shut the node down cleanly.
pub async fn run(cfg: &AppConfig, command: AdminCommand) -> Result<(), String> {
    let db = db::open_and_migrate(cfg)
        .await
        .map_err(|err| format!("open database (is the server still running?): {err}"))?;
    let result = execute(&db, command).await;
    if let Err(err) = db.shutdown().await {
        tracing::warn!(%err, "hiqlite shutdown failed after admin command");
    }
    result
}

async fn execute(db: &Client, command: AdminCommand) -> Result<(), String> {
    match command {
        AdminCommand::CreateIdentity {
            email,
            password,
            grant,
        } => {
            // Parse every capability before any write, so a typo cannot leave
            // a half-granted identity behind.
            let capabilities = parse_capabilities(&grant)?;
            let password = match password {
                Some(password) => password,
                None => read_password_from_stdin()?,
            };

            let registration = store::register(db, &email, &password)
                .await
                .map_err(|err| format!("create identity: {err}"))?;
            println!("identity {}", registration.identity_public_id);

            for capability in capabilities {
                store::grant_capability(
                    db,
                    registration.account_id,
                    registration.identity_id,
                    capability,
                )
                .await
                .map_err(|err| format!("grant {capability}: {err}"))?;
                println!("granted {capability}");
            }
            Ok(())
        }
        AdminCommand::Grant { email, capability } => {
            let capability = parse_capability(&capability)?;
            let (account_id, identity_id) = store::membership_for_email(db, &email)
                .await
                .map_err(|err| format!("look up membership: {err}"))?
                .ok_or_else(|| format!("no primary-account membership for {email}"))?;
            store::grant_capability(db, account_id, identity_id, capability)
                .await
                .map_err(|err| format!("grant {capability}: {err}"))?;
            println!("granted {capability} to {email}");
            Ok(())
        }
        AdminCommand::List { model } => match model {
            None => {
                for model in DataModel::ALL {
                    let count = queries::count(db, *model)
                        .await
                        .map_err(|err| format!("count {model}: {err}"))?;
                    println!("{:<26} {:>6}  {}", model.slug(), count, model.description());
                }
                Ok(())
            }
            Some(slug) => {
                let model = DataModel::parse(&slug).ok_or_else(|| {
                    format!(
                        "no model named `{slug}`; one of: {}",
                        DataModel::ALL
                            .iter()
                            .map(|m| m.slug())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
                let table = queries::rows(db, model)
                    .await
                    .map_err(|err| format!("query {model}: {err}"))?;
                print_table(&table);
                Ok(())
            }
        },
    }
}

fn parse_capability(s: &str) -> Result<Capability, String> {
    Capability::parse(s).ok_or_else(|| {
        format!(
            "unknown capability `{s}`; one of: {}",
            Capability::ALL
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn parse_capabilities(raw: &[String]) -> Result<Vec<Capability>, String> {
    raw.iter().map(|s| parse_capability(s)).collect()
}

/// One line from stdin. Plain read — no termios games — so it works when
/// piped as well as typed; the prompt goes to stderr so stdout stays parseable.
fn read_password_from_stdin() -> Result<String, String> {
    use std::io::BufRead;
    eprint!("password: ");
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|err| format!("read password from stdin: {err}"))?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err("empty password".to_string());
    }
    Ok(password)
}

/// Fixed-width text table over the same cells the web view renders.
fn print_table(table: &queries::TableData) {
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(queries::Cell::plain).collect())
        .collect();

    let mut widths: Vec<usize> = table.columns.iter().map(|c| c.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let header: Vec<String> = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{c:<width$}", width = widths[i]))
        .collect();
    println!("{}", header.join("  "));
    println!(
        "{}",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in &rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| format!("{cell:<width$}", width = widths[i]))
            .collect();
        println!("{}", line.join("  ").trim_end());
    }
    if rows.is_empty() {
        println!("(no rows)");
    }
}
