//! The four things an operator can change from outside the web tier.
//!
//! Every one of them goes through [`crate::api::cmd::invoke`] — the same
//! dispatch `POST /api/cmd/<name>` reaches — with the principal
//! [`super::actor`] minted. **Nothing here authorises anything.** The command
//! does, inside its own transaction, which is why an address that does not
//! hold `platform:* #operator` is refused here exactly as it would be in a
//! browser, and why every one of these leaves an audit row with an actor on
//! it.

use rn_kernel::Offset;
use serde_json::json;

use super::actor::Actor;
use super::{Action, AdminError, Report};
use crate::state::AppState;

/// Run one of the four write actions as `actor`.
///
/// # Errors
///
/// [`AdminError::Command`] carrying what the kernel said, and
/// [`AdminError::NoSuchPerson`] when the target names nobody.
pub async fn run(state: &AppState, actor: &Actor, action: &Action) -> Result<Report, AdminError> {
    let key = state.ids();
    match action {
        Action::GrantOperator { person, reason } => {
            let target = super::person_of(state, person).await?;
            let args = rn_api::commands::GrantOperator {
                person: target.public(key),
                reason: reason.clone(),
            };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{person} now holds platform:* #operator\n"),
                body: json!({ "granted": args.person.as_str(), "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::RevokeOperator { person, reason } => {
            let target = super::person_of(state, person).await?;
            let args = rn_api::commands::RevokeOperator {
                person: target.public(key),
                reason: reason.clone(),
            };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{person} no longer holds platform:* #operator\n"),
                body: json!({ "revoked": args.person.as_str(), "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::RevokeSession { session } => {
            let id = session.parse().map_err(|_| {
                AdminError::Usage(format!("{session} is not a session's public id"))
            })?;
            let args = rn_api::commands::RevokeSession { session: id };
            let executed = invoke(state, actor, &args).await?;
            Ok(Report {
                text: format!("{session} is ended\n"),
                body: json!({ "revoked": session, "offset": executed }),
                offset: Some(executed),
            })
        }
        Action::SetProduct { slug, .. } => Err(AdminError::NotYet(format!(
            "there is no command that turns {slug} on or off in this build: requirement B5's \
             product commands are leg P2's and have not landed on this branch"
        ))),
        Action::Operators | Action::Sessions { .. } | Action::Products | Action::Audit { .. } => {
            unreachable!("reads do not reach the write path")
        }
    }
}

/// Run one command as the actor, through the API's own dispatch.
async fn invoke<A: rn_api::Command + serde::Serialize>(
    state: &AppState,
    actor: &Actor,
    args: &A,
) -> Result<Offset, AdminError> {
    crate::api::cmd::invoke(state, actor.principal.clone(), args)
        .await
        .map(|executed| executed.committed.offset)
        .map_err(|error| AdminError::Command(describe(&error)))
}

/// What a command refusal says to somebody at a terminal.
fn describe(error: &crate::api::cmd::CommandError) -> String {
    use crate::api::cmd::CommandError;
    use rn_kernel::KernelError;
    match error {
        CommandError::Kernel(KernelError::Decline(_)) => {
            "the kernel declined it. The commonest reason from here is that --as does not hold \
             platform:* #operator; the others are that the target is not what the command \
             expects, or that the last operator cannot be revoked"
                .to_owned()
        }
        CommandError::Kernel(KernelError::ReAuthRequired) => {
            "the actor session was not fresh enough, which cannot happen from here — report it"
                .to_owned()
        }
        CommandError::Kernel(other) => other.to_string(),
        CommandError::Unbound => "no such command in this build".to_owned(),
        CommandError::Malformed(what) => format!("malformed arguments: {what}"),
    }
}
