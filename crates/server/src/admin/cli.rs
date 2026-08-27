//! `rn-site admin <cmd>` — the way in when the process is not running.
//!
//! The order of operations is the whole of the safety here, and it is not the
//! obvious one:
//!
//! 1. **Parse the arguments**, before anything is opened. A typo must not cost
//!    a database open.
//! 2. **Refuse if a node holds the directory** ([`super::lock`]), naming the
//!    pid. This is *before* the open and not a failure of it, because hiqlite
//!    with `auto-heal` on does not fail an open against a held directory — it
//!    concludes the previous process crashed and deletes the state machine.
//! 3. **Open**, run, close. The close is a real hiqlite shutdown, so the next
//!    thing to open the directory finds it clean.
//!
//! Everything that writes goes through [`super::run`] and therefore through
//! the same kernel command the browser reaches. What is *not* a command —
//! `backup`, `restore`, `wipe` — is here because it is not an act inside the
//! model at all: it is an act on the whole database, and there is no principal
//! whose authority could be asked about it. What stands in for authority is
//! that the caller has the data directory and the process is stopped.

use std::path::PathBuf;

use crate::config::{AppConfig, Mode};
use crate::db::{self, Migrations};
use crate::state::AppState;

use super::{Action, AdminError, Report};

/// The subcommand word. `main` looks for it before it opens anything.
pub const SUBCOMMAND: &str = "admin";

const USAGE: &str = "\
rn-site admin <command> [--as <email>]

  operators                       who holds platform:* #operator
  grant-operator <who> --reason <why> --as <email>
  revoke-operator <who> --reason <why> --as <email>
  sessions [--person <who>]       live sessions, newest first
  revoke-session <id> --as <email>
  products                        which products this deployment serves
  enable <slug> --as <email>
  disable <slug> --as <email>
  audit --tail <n>                the newest n rows of the whole history
  backup <dir>                    a logical backup, with a manifest
  restore <dir>                   put one back, onto an empty database
  wipe                            empty this deployment (dev only)

<who> is an address or a person's public id.
--as names who is accountable; it is required by everything that writes, and
it grants nothing — the command still asks what that person may do.
";

/// Whether this process was invoked as `rn-site admin …`, and with what.
#[must_use]
pub fn arguments() -> Option<Vec<String>> {
    let mut args = std::env::args().skip(1);
    (args.next().as_deref() == Some(SUBCOMMAND)).then(|| args.collect())
}

/// What the argument vector asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Request {
    /// One of the actions both front doors share.
    Act { action: Action, who: Option<String> },
    /// A whole-database act, which is not a command and has no principal.
    Backup(PathBuf),
    /// The same, in reverse.
    Restore(PathBuf),
    /// Empty this deployment. Dev only.
    Wipe,
}

/// Run the subcommand and return the process's exit code.
///
/// `0` did it, `1` refused or failed, `2` is a usage error — which is the
/// convention a shell script wrapping this can branch on without parsing
/// English.
pub async fn run(argv: Vec<String>, config: AppConfig, migrations: &Migrations) -> i32 {
    let request = match parse(&argv) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("rn-site admin: {error}");
            eprintln!("\n{USAGE}");
            return 2;
        }
    };

    // Before the open. See the module doc: with `auto-heal` on, opening a held
    // directory is not an error, it is a rebuild.
    let dir = config.db_dir();
    if let Err(error) = super::lock::refuse_if_held(&dir) {
        eprintln!("rn-site admin: {error}");
        return 1;
    }

    // `wipe` is refused on the configuration alone, before the database is
    // even opened. F18.3: a subcommand that can empty a deployment must read
    // the mode and decline in prod, and the decline is loud.
    if request == Request::Wipe && config.mode == Mode::Prod {
        eprintln!(
            "rn-site admin: refusing to wipe a deployment in mode=prod. This subcommand exists \
             for a disposable-dev instance and there is no flag that turns it on here — if this \
             really is a throwaway, set RN_SITE__MODE=dev on the invocation and it will run."
        );
        return 1;
    }

    let db = match db::open(&config, migrations).await {
        Ok(db) => db,
        Err(error) => {
            eprintln!("rn-site admin: database unavailable: {error}");
            return 1;
        }
    };
    let state = AppState::new(db.clone(), config);

    let outcome = execute(&state, request).await;
    crate::shutdown::close(db).await;

    match outcome {
        Ok(report) => {
            print!("{}", report.text);
            0
        }
        Err(error) => {
            eprintln!("rn-site admin: {error}");
            match error {
                AdminError::Usage(_) => 2,
                _ => 1,
            }
        }
    }
}

async fn execute(state: &AppState, request: Request) -> Result<Report, AdminError> {
    match request {
        Request::Act { action, who } => super::run(state, who.as_deref(), &action).await,
        Request::Backup(dir) => super::backup::write(state, &dir).await,
        Request::Restore(dir) => super::restore::apply(state, &dir).await,
        Request::Wipe => wipe(state).await,
    }
}

/// Empty every table this deployment owns, children first.
///
/// The walk's dependency order, reversed: a delete has to take the pointers
/// out before the rows they point at, which is exactly the order an insert
/// does not want. Nothing here is a command and nothing here writes an audit
/// row — there is no audit row to write it into afterwards.
async fn wipe(state: &AppState) -> Result<Report, AdminError> {
    use hiqlite::params;

    let mut order = super::backup::walk_order(state).await?;
    order.reverse();

    let mut emptied = 0u64;
    let mut text = String::new();
    for table in &order {
        let name = super::backup::identifier(table)?;
        let removed = state
            .db
            .execute(format!("DELETE FROM \"{name}\""), params!())
            .await
            .map_err(AdminError::store)?;
        if removed > 0 {
            text.push_str(&format!("  {name:<20} {removed}\n"));
        }
        emptied += removed as u64;
    }
    Ok(Report {
        text: format!(
            "wiped {emptied} rows over {} tables. This deployment now has no operator: the \
             configured bootstrap address takes it on the next registration, or the dev sign-in \
             invents one.\n{text}",
            order.len()
        ),
        body: serde_json::json!({ "wiped": emptied }),
        offset: None,
    })
}

// ------------------------------------------------------------------ parse ---

fn parse(argv: &[String]) -> Result<Request, String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut who: Option<String> = None;
    let mut person: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut tail: Option<usize> = None;

    let mut rest = argv.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--as" => who = Some(value(&mut rest, "--as")?),
            "--reason" => reason = Some(value(&mut rest, "--reason")?),
            "--person" => person = Some(value(&mut rest, "--person")?),
            "--tail" => {
                let raw = value(&mut rest, "--tail")?;
                tail = Some(
                    raw.parse()
                        .map_err(|_| format!("--tail wants a number, not {raw:?}"))?,
                );
            }
            "-h" | "--help" => return Err("here is what it does".to_owned()),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            word => positional.push(word),
        }
    }

    let (command, arguments) = positional
        .split_first()
        .ok_or_else(|| "name a command".to_owned())?;

    let one = |what: &str| -> Result<String, String> {
        match arguments {
            [only] => Ok((*only).to_owned()),
            [] => Err(format!("{command} wants {what}")),
            _ => Err(format!(
                "{command} wants one {what}, not {}",
                arguments.len()
            )),
        }
    };
    let none = || -> Result<(), String> {
        if arguments.is_empty() {
            Ok(())
        } else {
            Err(format!("{command} takes no arguments"))
        }
    };
    let why = || -> Result<String, String> {
        reason
            .clone()
            .ok_or_else(|| format!("{command} wants --reason: a delegation of full authority with no stated reason is a row nobody can account for afterwards"))
    };
    let act = |action: Action| Request::Act {
        action,
        who: who.clone(),
    };

    Ok(match *command {
        "operators" => {
            none()?;
            act(Action::Operators)
        }
        "grant-operator" => act(Action::GrantOperator {
            person: one("an address or a person's public id")?,
            reason: why()?,
        }),
        "revoke-operator" => act(Action::RevokeOperator {
            person: one("an address or a person's public id")?,
            reason: why()?,
        }),
        "sessions" => {
            none()?;
            act(Action::Sessions { person })
        }
        "revoke-session" => act(Action::RevokeSession {
            session: one("a session's public id")?,
        }),
        "products" => {
            none()?;
            act(Action::Products)
        }
        "enable" | "disable" => act(Action::SetProduct {
            slug: one("a product slug")?,
            enabled: *command == "enable",
        }),
        "audit" => {
            none()?;
            act(Action::Audit {
                tail: tail.unwrap_or(20),
            })
        }
        "backup" => Request::Backup(PathBuf::from(one("a directory")?)),
        "restore" => Request::Restore(PathBuf::from(one("a directory")?)),
        "wipe" => {
            none()?;
            Request::Wipe
        }
        other => return Err(format!("{other} is not a command")),
    })
}

fn value<'a>(rest: &mut impl Iterator<Item = &'a String>, flag: &str) -> Result<String, String> {
    rest.next()
        .map(ToOwned::to_owned)
        .filter(|value: &String| !value.is_empty())
        .ok_or_else(|| format!("{flag} wants a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Result<Request, String> {
        let argv: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        parse(&argv)
    }

    #[test]
    fn the_subcommand_list_is_the_one_the_requirement_names() {
        assert!(matches!(
            parsed("operators"),
            Ok(Request::Act {
                action: Action::Operators,
                who: None
            })
        ));
        assert!(matches!(
            parsed("sessions"),
            Ok(Request::Act {
                action: Action::Sessions { person: None },
                ..
            })
        ));
        assert!(matches!(parsed("products"), Ok(Request::Act { .. })));
        assert!(matches!(parsed("wipe"), Ok(Request::Wipe)));
        assert!(matches!(parsed("backup /tmp/x"), Ok(Request::Backup(_))));
        assert!(matches!(parsed("restore /tmp/x"), Ok(Request::Restore(_))));

        match parsed("audit --tail 5") {
            Ok(Request::Act {
                action: Action::Audit { tail },
                ..
            }) => assert_eq!(tail, 5),
            other => panic!("{other:?}"),
        }
        match parsed("audit") {
            Ok(Request::Act {
                action: Action::Audit { tail },
                ..
            }) => assert_eq!(tail, 20, "a tail with no number is a screenful"),
            other => panic!("{other:?}"),
        }
    }

    /// `--as` rides beside the command rather than being positional, so the
    /// same word means the same thing in every line.
    #[test]
    fn who_is_accountable_is_a_flag_and_the_reason_is_mandatory() {
        match parsed("grant-operator her@example.invalid --reason because --as me@example.invalid")
        {
            Ok(Request::Act {
                action: Action::GrantOperator { person, reason },
                who,
            }) => {
                assert_eq!(person, "her@example.invalid");
                assert_eq!(reason, "because");
                assert_eq!(who.as_deref(), Some("me@example.invalid"));
            }
            other => panic!("{other:?}"),
        }
        let refused = parsed("grant-operator her@example.invalid").expect_err("no reason");
        assert!(refused.contains("--reason"), "{refused}");
        assert!(parsed("revoke-operator her@example.invalid").is_err());
    }

    /// A usage error whose text does not name the problem is a usage error
    /// somebody has to read the source to understand.
    #[test]
    fn every_usage_refusal_names_the_part_that_was_wrong() {
        for (line, expected) in [
            ("", "name a command"),
            ("nonsense", "not a command"),
            ("operators extra", "takes no arguments"),
            ("revoke-session", "wants"),
            ("backup", "wants"),
            ("audit --tail lots", "--tail wants a number"),
            ("operators --as", "--as wants a value"),
            ("operators --nope", "unknown option"),
        ] {
            let refused = parsed(line).expect_err(line);
            assert!(
                refused.contains(expected),
                "{line:?} refused with {refused:?}, which does not mention {expected:?}"
            );
        }
    }

    /// Which subcommands write is what decides which ones demand `--as`, and
    /// getting it wrong in either direction is a bug: a write with no actor is
    /// an unaccountable row, and a read that demanded one would make the tool
    /// useless on a deployment with no operator — which is one of the two
    /// situations it exists for.
    #[test]
    fn reads_need_no_actor_and_writes_do() {
        let writes = [
            "grant-operator x@y.invalid --reason r",
            "revoke-operator x@y.invalid --reason r",
            "revoke-session s_AAAAAAAAAAAAAAAAAAAAAA",
            "enable documents",
            "disable documents",
        ];
        for line in writes {
            match parsed(line).expect(line) {
                Request::Act { action, .. } => assert!(action.writes(), "{line}"),
                other => panic!("{line}: {other:?}"),
            }
        }
        for line in ["operators", "sessions", "products", "audit"] {
            match parsed(line).expect(line) {
                Request::Act { action, .. } => assert!(!action.writes(), "{line}"),
                other => panic!("{line}: {other:?}"),
            }
        }
    }
}
