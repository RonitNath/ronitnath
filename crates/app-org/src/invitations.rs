//! `/org/invitations` — every link minted into this organization's containers.
//!
//! A minted token exists in the clear exactly once, in the reply to the
//! `Invite` that minted it, because the row keeps only its SHA-256. So this
//! page is not a list of links: it is a list of their fates — who minted one,
//! into what, when it runs out, and who walked through it.
//!
//! What it can still do is take one back. A link has a public id, which names
//! the row rather than the secret, so an admin can withdraw an invitation they
//! never held the token for. The verb is in the row, because withdrawing one
//! is the only thing this page exists to do and a panel between the reader and
//! it is a click that says nothing. It is absent on a link that has been
//! claimed or has run out: there is nothing left to withdraw, and the kernel
//! refuses both.

use leptos::prelude::*;
use rn_api::commands::RevokeLink;
use rn_ui::{Column, PageHead, Priority, RowAction, Table};

use crate::bits::{Note, act, at, on};
use crate::rows::Invitation;
use crate::scope::use_scope;

/// The organization's invitations.
#[component]
pub fn Invitations(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let live = use_scope().live::<Invitation>("org-invitations", &[("org", &org)]);
    let note = RwSignal::new(None::<String>);
    let columns = vec![
        // What it joins and what became of it survive a phone; who and when
        // are the first things a narrow reader gives up, because the verb in
        // the row is worth more than either and has to fit beside them.
        Column::new("Into", |row: &Invitation| row.container_display.clone()),
        Column::new("State", state).state(),
        Column::new("Role", |row: &Invitation| row.role.clone()).priority(Priority::Secondary),
        Column::new("Claimed by", |row: &Invitation| {
            row.claimed_by.clone().unwrap_or_default()
        })
        .priority(Priority::Secondary),
        Column::new("Minted by", |row: &Invitation| {
            row.minted_by.clone().unwrap_or_default()
        })
        .priority(Priority::Tertiary),
        Column::new("Minted", |row: &Invitation| on(row.created_at)).priority(Priority::Tertiary),
        Column::new("Expires", |row: &Invitation| on(row.expires_at)).priority(Priority::Tertiary),
    ];
    let withdraw = RowAction::new(
        "Withdraw",
        Callback::new(move |row: Invitation| {
            let Ok(link) = row.id.parse() else {
                return;
            };
            act(note, RevokeLink { link }, |_| ());
        }),
    )
    .when(|row: &Invitation| row.live)
    .undo();

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
            actions=vec![withdraw]
        />
        <Note note=note />
        {move || selected().map(|invitation| view! { <Fate invitation=invitation /> })}
    }
}

/// What became of a link, as one word: claimable, walked through, or run out.
fn state(row: &Invitation) -> String {
    if row.claimed_by.is_some() || row.claimed_at.is_some() {
        "claimed"
    } else if row.live {
        "unclaimed"
    } else {
        "expired"
    }
    .to_owned()
}

/// One invitation's fate, in the words the row could not fit.
///
/// The verb is in the row; this is what a reader opens a row to learn — when
/// it was minted, and what happened to it. There is nothing to do here,
/// because there is nothing that can be done to a link but withdraw it.
#[component]
fn Fate(invitation: Invitation) -> impl IntoView {
    let held = if invitation.live {
        "This link can still be claimed.".to_owned()
    } else {
        invitation.claimed_by.clone().map_or_else(
            || "This link has run out.".to_owned(),
            |who| format!("{who} walked through this link."),
        )
    };
    view! {
        <div class="panel">
            <h2>{invitation.container_display.clone()}</h2>
            <p class="held">
                {format!("{} \u{00b7} minted {}", invitation.role, at(invitation.created_at))}
            </p>
            <p class="held">{held}</p>
        </div>
    }
}
