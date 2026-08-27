//! `/org/invitations` — every link minted into this organization's containers.
//!
//! A minted token exists in the clear exactly once, in the reply to the
//! `Invite` that minted it, because the row keeps only its SHA-256. So this
//! page is not a list of links: it is a list of their fates — who minted one,
//! into what, when it runs out, and who walked through it.
//!
//! What it can still do is take one back. A link has a public id, which names
//! the row rather than the secret, so an admin can withdraw an invitation they
//! never held the token for. Selecting a row opens the panel that offers it,
//! which is the same shape the members page uses: a table cell produces text,
//! so a verb lives under the table rather than inside it.

use leptos::prelude::*;
use rn_api::commands::RevokeLink;
use rn_ui::{Column, Live, PageHead, Priority, Table};

use crate::bits::{Note, act, at, on};
use crate::rows::Invitation;

/// The organization's invitations.
#[component]
pub fn Invitations(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let live = Live::<Invitation>::subscribe("org-invitations", &[("org", &org)]);
    let columns = vec![
        Column::new("Into", |row: &Invitation| row.container_display.clone()),
        Column::new("Role", |row: &Invitation| row.role.clone()),
        Column::new("Claimed by", |row: &Invitation| {
            row.claimed_by
                .clone()
                .unwrap_or_else(|| if row.live { "unclaimed" } else { "expired" }.to_owned())
        }),
        Column::new("Minted by", |row: &Invitation| {
            row.minted_by.clone().unwrap_or_default()
        })
        .priority(Priority::Secondary),
        Column::new("Minted", |row: &Invitation| on(row.created_at)).priority(Priority::Secondary),
        Column::new("Expires", |row: &Invitation| on(row.expires_at)).priority(Priority::Tertiary),
    ];
    let rows = Signal::derive(move || live.rows());
    let chosen = RwSignal::new(None::<String>);
    let selected = move || {
        let id = chosen.get()?;
        live.rows().into_iter().find(|row| row.id == id)
    };
    view! {
        <PageHead title="Invitations" />
        <Table
            rows=rows
            columns=columns
            empty="No link has been minted into this organization or its groups."
            on_row=Callback::new(move |row: Invitation| chosen.set(Some(row.id)))
        />
        {move || selected().map(|invitation| view! { <Fate invitation=invitation /> })}
    }
}

/// One invitation, and the only thing that can still be done to it.
#[component]
fn Fate(invitation: Invitation) -> impl IntoView {
    let note = RwSignal::new(None::<String>);
    let id = invitation.id.clone();
    // Claimed or expired, there is nothing to withdraw: the membership it
    // produced is `Leave`'s business, and a dead link already opens nothing.
    let live = invitation.live;
    let claimed = invitation.claimed_by.clone();
    view! {
        <div class="panel">
            <h2>{invitation.container_display.clone()}</h2>
            <p class="held">
                {format!("{} · minted {}", invitation.role, at(invitation.created_at))}
            </p>
            <Show
                when=move || live
                fallback=move || {
                    let held = claimed
                        .clone()
                        .map_or_else(
                            || "This link has run out.".to_owned(),
                            |who| format!("{who} walked through this link."),
                        );
                    view! { <p class="held">{held}</p> }
                }
            >
                <button
                    type="button"
                    class="commit"
                    on:click={
                        let id = id.clone();
                        move |_| {
                            let Ok(link) = id.parse() else {
                                return;
                            };
                            act(note, RevokeLink { link }, |_| ());
                        }
                    }
                >
                    "Withdraw"
                </button>
            </Show>
            <Note note=note />
        </div>
    }
}
