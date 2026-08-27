//! `/platform/links` and `/platform/consents` — the standing grants nobody
//! has to sign in to use.
//!
//! An invitation is a bearer secret: whoever holds the token gets the role,
//! without an account, without a password, and without anybody watching. A
//! consent is the same shape at a different layer — a relying party holding a
//! refresh token acts as somebody for as long as the consent stands. Both are
//! therefore things an operator has to be able to see all of and end one of,
//! which is what these two pages are.
//!
//! What the invitation list never carries is the token. The row is named by
//! its public id; an admin withdrawing somebody else's invitation has no
//! business holding the secret that would let them claim it, and the query
//! does not select it.
//!
//! `Revoke` sits in the row rather than behind a panel. These are the verbs
//! the list exists to run — one per row, on a row whose whole content is
//! already on screen — and a panel per row would be a click each to reach the
//! same button. The rare and destructive work stays behind a side affordance;
//! withdrawing an invitation is neither.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_api::commands::RevokeLink;
use rn_ui::{Column, PageHead, Priority, RowAction, Table};

use crate::panel::when;
use crate::rows::{Consent, Invitation};

#[component]
pub fn Links() -> impl IntoView {
    let live = rn_ui::Live::<Invitation>::subscribe("platform-links", &[]);
    let rows = Signal::derive(move || live.rows());
    let note = RwSignal::new(None::<String>);

    let columns = vec![
        Column::new("Container", |row: &Invitation| {
            row.container_display
                .clone()
                .unwrap_or_else(|| row.container_kind.clone())
        })
        .titled(|row: &Invitation| {
            row.container
                .as_ref()
                .map_or_else(String::new, ToString::to_string)
        }),
        Column::new("Role", |row: &Invitation| row.role.clone()),
        Column::new("State", |row: &Invitation| row.state.clone()).state(),
        Column::new("Minted by", |row: &Invitation| {
            row.minted_by
                .clone()
                .unwrap_or_else(|| "\u{2014}".to_owned())
        }),
        Column::new("Claimed by", |row: &Invitation| {
            row.claimed_by
                .clone()
                .unwrap_or_else(|| "\u{2014}".to_owned())
        })
        .priority(Priority::Secondary),
        Column::new("Expires", |row: &Invitation| when(row.expires_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Invitation| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];

    // Offered only where it can do something: a claimed link is spent, and a
    // control that lies until you use it is worse than no control.
    let revoke = RowAction::new(
        "Revoke",
        Callback::new(move |row: Invitation| {
            let link = row.public_id.clone();
            spawn_local(async move {
                note.set(
                    rn_ui::invoke::<RevokeLink, serde_json::Value>(RevokeLink { link })
                        .await
                        .err()
                        .map(|error| crate::panel::refusal(&error)),
                );
            });
        }),
    )
    .when(|row: &Invitation| row.claimed_at.is_none())
    .undo();

    view! {
        <PageHead title="Invitations">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <Table
            rows=rows
            columns=columns
            empty="No invitation has been minted that has not been claimed, expired or withdrawn."
            actions=vec![revoke]
        />
        <p class="none" data-state=move || note.get().map(|_| "invalid")>
            {move || note.get()}
        </p>
    }
}

#[component]
pub fn Consents() -> impl IntoView {
    let live = rn_ui::Live::<Consent>::subscribe("platform-consents", &[]);
    let rows = Signal::derive(move || live.rows());

    let columns = vec![
        Column::new("Client", |row: &Consent| row.client_name.clone())
            .titled(|row: &Consent| row.client.to_string()),
        Column::new("Person", |row: &Consent| row.person_display.clone())
            .titled(|row: &Consent| row.person.to_string()),
        Column::new("Handle", |row: &Consent| {
            row.handle.clone().unwrap_or_else(|| "\u{2014}".to_owned())
        })
        .mono()
        .priority(Priority::Secondary),
        Column::new("Scopes", |row: &Consent| row.scopes.clone()).mono(),
        Column::new("Granted", |row: &Consent| when(row.at))
            .mono()
            .priority(Priority::Secondary),
    ];

    view! {
        <PageHead title="Consents">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <Table
            rows=rows
            columns=columns
            empty="Nobody has authorised a relying party on this deployment."
        />
    }
}
