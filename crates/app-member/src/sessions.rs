//! `/app/sessions` — the devices this person is signed in on.
//!
//! A session row *is* the session: there is no status, and ending one is
//! deleting it. So the verb is in the row — this page exists to run it — and
//! the one on the device reading the page says "Sign out", because that is
//! what revoking your own session is.

use leptos::prelude::*;
use rn_api::commands::RevokeSession;
use rn_ui::{Column, Live, PageHead, Priority, RowAction, Table};

use crate::api::{Refusal, attempt};
use crate::parts::{Note, when};
use crate::rows::Session;

#[component]
pub fn Sessions() -> impl IntoView {
    let sessions = Live::<Session>::subscribe("sessions", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let columns = vec![
        Column::new("Signed in", |row: &Session| when(row.created_at)).mono(),
        Column::new("Last seen", |row: &Session| when(row.last_seen_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Ends", |row: &Session| when(row.expires_at))
            .mono()
            .priority(Priority::Tertiary),
        Column::new("Device", |row: &Session| {
            if row.current { "This device" } else { "" }.to_owned()
        }),
    ];

    let end = |label: &'static str, current: bool| {
        RowAction::new(
            label,
            Callback::new(move |row: Session| {
                let Ok(session) = row.public_id.parse() else {
                    return;
                };
                attempt(RevokeSession { session }, refusal, move |_| {
                    // Revoking the session you are reading with is signing
                    // out, so the browser leaves the bundle rather than
                    // staying on a page it can no longer read.
                    if current && let Some(window) = web_sys::window() {
                        let _ = window.location().assign("/auth");
                    }
                });
            }),
        )
        .when(move |row: &Session| row.current == current)
        .undo()
    };

    let rows = Signal::derive(move || sessions.rows());

    view! {
        <PageHead title="Sessions" />
        <div class="sheet">
            <Table
                rows=rows
                columns=columns
                empty="No sessions. Signing in on a device puts one here."
                actions=vec![end("Sign out", true), end("Revoke", false)]
            />
        </div>
        <Note refusal=refusal />
    }
}
