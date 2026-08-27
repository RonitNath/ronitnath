//! Posting a command from this bundle.
//!
//! `rn_ui::api` is the client and this is the two lines on top of it that a
//! page wants: run the command, and put whatever it refused into a signal so
//! the refusal can be drawn beside the control that caused it.
//!
//! There used to be a second HTTP client here. It existed for one reason —
//! the shared client folded a `422` into a status number and dropped the body,
//! and a member page is mostly forms, so this bundle read the per-field
//! complaints itself. `rn_ui::ApiError::Invalid` carries them now, which is
//! where a rule the whole product contract states belongs.

use leptos::prelude::*;
use rn_api::Command;
use serde::Serialize;
use serde_json::Value;

pub use rn_ui::ApiError as Refusal;

/// Post a command and read the result it returned.
///
/// The route comes from the args type, so a body cannot reach the wrong
/// endpoint, and the key is minted per call, so a retry after a dropped
/// response replays rather than repeats.
pub async fn run<A: Command + Serialize>(args: A) -> Result<Value, Refusal> {
    rn_ui::invoke::<A, Value>(args)
        .await
        .map(|committed| committed.result)
}

/// Run a command and put whatever it refused into `note`, so a page can show
/// the refusal beside the control that caused it.
pub fn attempt<A, F>(args: A, note: RwSignal<Option<Refusal>>, then: F)
where
    A: Command + Serialize + 'static,
    F: FnOnce(Value) + 'static,
{
    leptos::task::spawn_local(async move {
        match run(args).await {
            Ok(result) => {
                note.set(None);
                then(result);
            }
            Err(refusal) => note.set(Some(refusal)),
        }
    });
}
