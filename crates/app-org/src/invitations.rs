//! `/org/invitations` — every link minted into this organization's containers.
//!
//! A minted token exists in the clear exactly once, in the reply to the
//! `Invite` that minted it, because the row keeps only its SHA-256. So this
//! page is not a list of links: it is a list of their fates — who minted one,
//! into what, when it runs out, and who walked through it.

use leptos::prelude::*;
use rn_ui::{Column, Live, PageHead, Priority, Table};

use crate::bits::on;
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
    view! {
        <PageHead title="Invitations" />
        <Table
            rows=rows
            columns=columns
            empty="No link has been minted into this organization or its groups."
        />
    }
}
