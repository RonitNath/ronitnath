//! `/app/invitations` — the links this person minted, and what became of them.
//!
//! There is no revoke here and that is not an omission on this page. An
//! invitation's grant is held by a `link` subject, and a link has no public id
//! — it *is* its token — so `Revoke` has no way to name one. Letting the
//! invitation expire is the whole of withdrawing it in this build.

use leptos::prelude::*;
use rn_ui::{Column, Live, PageHead, Priority, Table};

use crate::parts::{titled, when};
use crate::rows::Invitation;

#[component]
pub fn Invitations() -> impl IntoView {
    let invitations = Live::<Invitation>::subscribe("invitations", &[]);
    let rows = Signal::derive(move || invitations.rows());

    view! {
        <PageHead title="Invitations" />
        <div class="sheet">
            <Table
            rows=rows
            columns=vec![
                Column::new("Group", |row: &Invitation| row.container.display.clone()),
                Column::new("Role", |row: &Invitation| titled(&row.role)),
                Column::new(
                    "Claimed by",
                    |row: &Invitation| row.claimed_by.clone().unwrap_or_default(),
                ),
                Column::new(
                    "Claimed",
                    |row: &Invitation| row.claimed_at.map(when).unwrap_or_default(),
                )
                    .mono()
                    .priority(Priority::Secondary),
                Column::new("Expires", |row: &Invitation| when(row.expires_at))
                    .mono()
                    .priority(Priority::Tertiary),
            ]
                empty="No invitations. Mint one from a group."
            />
        </div>
    }
}
