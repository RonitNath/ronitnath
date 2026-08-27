//! `/app/invitations` — the links this person minted, and what became of them.
//!
//! The token is not on this page and never can be: it existed in the clear
//! once, in the reply to the `Invite` that minted it, and the row keeps only
//! its SHA-256. What is here is the link's *public id*, which names the row
//! without naming the secret — and that is what makes withdrawing one
//! possible at all. An invitation sent to the wrong address used to have to be
//! waited out.
//!
//! Only an unclaimed link can be withdrawn, so the verb is absent on the
//! others rather than present and refused. Taking one back after somebody
//! walked through it would say nothing about the membership they now hold,
//! which is what `Leave` is for.

use leptos::prelude::*;
use rn_api::commands::RevokeLink;
use rn_ui::{Column, Live, PageHead, Priority, RowAction, Table};

use crate::api::{Refusal, attempt};
use crate::parts::{Note, titled, when};
use crate::rows::Invitation;

#[component]
pub fn Invitations() -> impl IntoView {
    let invitations = Live::<Invitation>::subscribe("invitations", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let columns = vec![
        Column::new("Group", |row: &Invitation| row.container.display.clone()),
        Column::new("Role", |row: &Invitation| titled(&row.role)).priority(Priority::Secondary),
        Column::new("State", |row: &Invitation| {
            if row.claimed_at.is_some() {
                "claimed"
            } else {
                "unclaimed"
            }
            .to_owned()
        })
        .state(),
        Column::new("Claimed by", |row: &Invitation| {
            row.claimed_by.clone().unwrap_or_default()
        })
        .priority(Priority::Secondary),
        Column::new("Claimed", |row: &Invitation| {
            row.claimed_at.map(when).unwrap_or_default()
        })
        .mono()
        .priority(Priority::Tertiary),
        Column::new("Expires", |row: &Invitation| when(row.expires_at))
            .mono()
            .priority(Priority::Tertiary),
    ];

    let withdraw = RowAction::new(
        "Withdraw",
        Callback::new(move |row: Invitation| {
            let Ok(link) = row.id.parse() else {
                return;
            };
            attempt(RevokeLink { link }, refusal, |_| ());
        }),
    )
    .when(|row: &Invitation| row.claimed_at.is_none())
    .undo();

    let rows = Signal::derive(move || invitations.rows());

    view! {
        <PageHead title="Invitations" />
        <div class="sheet">
            <Table
                rows=rows
                columns=columns
                empty="No invitations. Mint one from a group."
                actions=vec![withdraw]
            />
        </div>
        <Note refusal=refusal />
    }
}
