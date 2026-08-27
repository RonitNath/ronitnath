//! Who the session is speaking as.
//!
//! `ActAs` is attribution, not authority (`docs/kernel/index.html` §Acting as):
//! switching grants nothing and takes nothing away, and every authorisation in
//! the kernel is still read from the person. What it moves is the party an
//! audit row names — which is what lets an organization's history read as its
//! own rather than as a list of the people who happened to be signed in.
//!
//! It is therefore chrome and not a page. It sits in the rail beside the name
//! it changes, and the rail is where the answer is read back: the shell
//! already draws `acting_as` under the reader's own name, so the control and
//! its effect are the same three lines of the same column.
//!
//! Not to be confused with the `/org` switcher. That one chooses which
//! organization a page is *about* and is a URL scope; this one chooses who the
//! writes are *by*, and a reader can be looking at one organization while
//! speaking as another — the kernel does not tie them, and neither does this.

use leptos::prelude::*;
use leptos::task::spawn_local;

use rn_api::commands::ActAs;

use crate::api;

use super::use_whoami;

/// The value that means "myself" — no organization, so the person again.
const SELF: &str = "";

/// The rail's act-as control. Absent when there is nothing to switch to: a
/// person who belongs to no organization can only ever be themselves.
#[component]
pub fn ActingAs() -> impl IntoView {
    let who = use_whoami();
    let note = RwSignal::new(None::<String>);

    let choose = move |wanted: String| {
        let party = if wanted == SELF {
            None
        } else {
            match wanted.parse() {
                Ok(party) => Some(party),
                // The options are built from `whoami`'s own ids, so this is
                // unreachable rather than a user error worth wording.
                Err(_) => return,
            }
        };
        spawn_local(async move {
            match api::invoke::<ActAs, serde_json::Value>(ActAs { party }).await {
                // The switch lands on the *next* request, so the chrome has to
                // ask again rather than assume: the session row moved, and
                // what it now resolves to is the server's answer. The signal
                // is captured here rather than looked up after the await —
                // there is no reactive owner inside a spawned future, so
                // `use_context` in there finds nothing.
                Ok(_) => {
                    note.set(None);
                    match api::whoami().await {
                        Ok(fresh) => who.set(Some(fresh)),
                        Err(error) => note.set(Some(error.message())),
                    }
                }
                Err(error) => note.set(Some(error.message())),
            }
        });
    };

    move || {
        let whoami = who.get()?;
        if whoami.organizations.is_empty() {
            return None;
        }
        let person = whoami.person.as_ref()?;
        let current = whoami.acting_as.public_id.to_string();
        let mine = person.public_id.to_string();
        let options = std::iter::once((SELF.to_owned(), person.display.clone()))
            .chain(
                whoami
                    .organizations
                    .iter()
                    .map(|org| (org.public_id.to_string(), org.display.clone())),
            )
            .map(|(value, label)| {
                let selected = if value == SELF {
                    current == mine
                } else {
                    current == value
                };
                view! {
                    <option value=value selected=selected>
                        {label}
                    </option>
                }
            })
            .collect_view();
        Some(view! {
            <div class="rail-acting">
                <label for="acting-as">"Acting as"</label>
                <select
                    id="acting-as"
                    on:change=move |event| choose(event_target_value(&event))
                >
                    {options}
                </select>
                <span class="note" data-state=move || note.get().map(|_| "invalid")>
                    {move || note.get()}
                </span>
            </div>
        })
    }
}
