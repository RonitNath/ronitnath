//! `/platform/sessions` — every live session on the deployment.
//!
//! A session row *is* the session: there is no status, because ending one is
//! deleting it. So the length of this list is the honest answer to "how many
//! people are signed in", and it is folded into the title rather than
//! captioned beneath it.
//!
//! A session binds an identity, never a person, which is why the registration
//! and what it is *acting as* are two columns: the same registration can be
//! speaking as itself or as an organization it belongs to.
//!
//! Revoking is behind the row rather than in it — a column of buttons over
//! every session on a deployment is a column of accidents — so choosing a row
//! opens it, and the panel is where it ends. Nothing is fetched to open one:
//! the row already carries everything a revocation needs.

use leptos::prelude::*;
use rn_api::commands::RevokeSession;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Panel, refusal, run_with, when};
use crate::rows::Session;

#[component]
pub fn Sessions() -> impl IntoView {
    let live = rn_ui::Live::<Session>::subscribe("platform-sessions", &[]);
    let selected = RwSignal::new(None::<Session>);

    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("Person", |row: &Session| {
            row.person
                .clone()
                .unwrap_or_else(|| "unresolved".to_owned())
        }),
        Column::new("Acting as", |row: &Session| {
            format!("{} ({})", row.acting_display, row.acting_kind)
        }),
        Column::new("Last seen", |row: &Session| when(row.last_seen_at)).mono(),
        Column::new("Created", |row: &Session| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Expires", |row: &Session| when(row.expires_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Identity", |row: &Session| row.identity.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Session| selected.set(Some(row)));

    view! {
        <PageHead title="Sessions">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table rows=rows columns=columns empty="Nobody is signed in." on_row=open />
            </div>
            <Show when=move || selected.get().is_some()>
                {move || {
                    selected.get().map(|session| view! { <Detail session=session selected=selected /> })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(session: Session, selected: RwSignal<Option<Session>>) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let title = session
        .person
        .clone()
        .unwrap_or_else(|| session.identity.to_string());
    let facts = vec![
        ("Identity", session.identity.to_string()),
        (
            "Acting as",
            format!("{} ({})", session.acting_display, session.acting_kind),
        ),
        ("Created", when(session.created_at)),
        ("Last seen", when(session.last_seen_at)),
        ("Expires", when(session.expires_at)),
        ("Id", session.public_id.to_string()),
    ];
    let id = session.public_id.clone();
    let run = run_with(move || {
        let session = id.clone();
        async move {
            rn_ui::invoke::<RevokeSession, serde_json::Value>(RevokeSession { session })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
        }
    });
    view! {
        <Panel title=title on_close=close>
            <Facts facts=facts />
            <div class="actions">
                <Act label="Revoke" run=run />
            </div>
        </Panel>
    }
}
