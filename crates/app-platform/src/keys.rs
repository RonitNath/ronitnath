//! `/platform/keys` — the signing keys, and the two decisions about them.
//!
//! A rotation is cheap and a retirement is not. `RotateSigningKey` mints the
//! next key and moves the current one to `retiring`, which is the overlap that
//! makes a rotation a non-event for a relying party: the key stops signing and
//! goes on verifying. `RetireKey` ends that, and ending it is what makes every
//! token signed under that key unverifiable — so the number beside each row is
//! **live tokens**, counted from the rows the deployment holds, and it is the
//! same count the command itself refuses on.
//!
//! Which is why *retire* is offered only where it can succeed and *force* is a
//! separate control with its own reason: a forced retire is the operator
//! saying "break them", and that sentence belongs in the audit row.

use leptos::prelude::*;
use rn_api::commands::{RetireKey, RotateSigningKey};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, refusal, run_with, when};
use crate::rows::Key;

/// The key table, live. Used whole by `/platform/keys` and by the deployment
/// screen, which draws the same rows beside the node table.
#[component]
pub fn KeyTable() -> impl IntoView {
    let live = rn_ui::Live::<Key>::subscribe("platform-keys", &[]);
    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("kid", |row: &Key| row.kid.clone()).mono(),
        Column::new("Status", |row: &Key| row.status.clone()).state(),
        Column::new("Age", |row: &Key| format!("{}d", row.age_days))
            .mono()
            .titled(|row: &Key| when(row.created_at)),
        Column::new("Tokens alive", |row: &Key| {
            row.signed_tokens_alive.to_string()
        })
        .mono(),
        Column::new("Retired", |row: &Key| {
            row.retired_at.map_or_else(|| "\u{2014}".to_owned(), when)
        })
        .mono()
        .priority(Priority::Secondary),
    ];
    view! {
        <Table
            rows=rows
            columns=columns
            empty="This deployment has minted no signing key, which means it has issued no token."
        />
    }
}

#[component]
pub fn Keys() -> impl IntoView {
    let live = rn_ui::Live::<Key>::subscribe("platform-keys", &[]);
    let rows = Signal::derive(move || live.rows());
    let selected = RwSignal::new(None::<Key>);

    let columns = vec![
        Column::new("kid", |row: &Key| row.kid.clone()).mono(),
        Column::new("Status", |row: &Key| row.status.clone()).state(),
        Column::new("Age", |row: &Key| format!("{}d", row.age_days))
            .mono()
            .titled(|row: &Key| when(row.created_at)),
        Column::new("Tokens alive", |row: &Key| {
            row.signed_tokens_alive.to_string()
        })
        .mono(),
        Column::new("Minted", |row: &Key| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Retired", |row: &Key| {
            row.retired_at.map_or_else(|| "\u{2014}".to_owned(), when)
        })
        .mono()
        .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Key| selected.set(Some(row)));

    view! {
        <PageHead title="Keys">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="This deployment has minted no signing key, which means it has issued no token."
                    on_row=open
                />
                <Rotate />
            </div>
            <Show when=move || selected.get().is_some()>
                {move || selected.get().map(|key| view! { <Retire key=key selected=selected /> })}
            </Show>
        </div>
    }
}

/// Mint the next key. The active one becomes `retiring` in the same
/// transaction, which is the whole of what an overlap is.
#[component]
fn Rotate() -> impl IntoView {
    let run = run_with(move || async move {
        rn_ui::invoke::<RotateSigningKey, serde_json::Value>(RotateSigningKey {})
            .await
            .map(|_| ())
            .map_err(|error| refusal(&error))
    });
    view! {
        <section class="group">
            <h3>"Rotate"</h3>
            <div class="actions">
                <Act label="Rotate key" run=run />
            </div>
        </section>
    }
}

/// The retire decision, and the number that makes it safe.
#[component]
fn Retire(key: Key, selected: RwSignal<Option<Key>>) -> impl IntoView {
    use crate::panel::{Facts, Panel, State};
    let close = Callback::new(move |()| selected.set(None));
    let reason = RwSignal::new(String::new());
    let alive = key.signed_tokens_alive;
    let retiring = key.status == "retiring";

    let facts = vec![
        ("Status", key.status.clone()),
        ("Minted", when(key.created_at)),
        ("Age", format!("{} days", key.age_days)),
        ("Tokens alive", alive.to_string()),
    ];

    // Stated once, under the field it is about: both controls wait on the
    // reason, and only one of them is offered at a time.
    let blocked = Signal::derive(move || {
        if !retiring {
            Some(
                "Only a retiring key can be retired. The active key is what everything is being signed under."
                    .to_owned(),
            )
        } else if reason.get().trim().is_empty() {
            Some("A reason is required: it lands on the audit row.".to_owned())
        } else {
            None
        }
    });

    let retire = |kid: String, force: bool, reason: RwSignal<String>| {
        run_with(move || {
            let kid = kid.clone();
            let reason = reason.get_untracked().trim().to_owned();
            async move {
                rn_ui::invoke::<RetireKey, serde_json::Value>(RetireKey { kid, force, reason })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    };

    let forced = retire(key.kid.clone(), true, reason);
    let plain = retire(key.kid.clone(), false, reason);

    view! {
        <Panel title=key.kid.clone() on_close=close>
            <div class="panel-state">
                <State value=key.status.clone() />
            </div>
            <Facts facts=facts />
            <section class="group">
                <h3>"Retire"</h3>
                <p class="none">
                    {if alive > 0 {
                        format!(
                            "{alive} token{} signed under this key {} still alive. Retiring it makes {} unverifiable.",
                            if alive == 1 { "" } else { "s" },
                            if alive == 1 { "is" } else { "are" },
                            if alive == 1 { "it" } else { "them" },
                        )
                    } else {
                        "No token signed under this key is still alive.".to_owned()
                    }}
                </p>
                <input
                    type="text"
                    class="reason"
                    aria-label="Reason"
                    placeholder="Reason"
                    prop:value=move || reason.get()
                    on:input=move |event| reason.set(event_target_value(&event))
                />
                <div class="actions">
                    <Show
                        when=move || alive == 0
                        fallback=move || {
                            view! {
                                <Act
                                    label="Retire anyway"
                                    run=forced.clone()
                                    blocked=blocked
                                />
                            }
                        }
                    >
                        <Act label="Retire" run=plain.clone() blocked=blocked />
                    </Show>
                </div>
            </section>
        </Panel>
    }
}
