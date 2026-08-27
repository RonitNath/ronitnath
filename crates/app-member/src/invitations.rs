//! `/app/invitations` — the links this person minted, and what became of them.
//!
//! The token is not on this page and never can be: it existed in the clear
//! once, in the reply to the `Invite` that minted it, and the row keeps only
//! its SHA-256. What is here is the link's *public id*, which names the row
//! without naming the secret — and that is what makes withdrawing one
//! possible at all. An invitation sent to the wrong address used to have to be
//! waited out.
//!
//! Only an unclaimed, unexpired link can be withdrawn. Taking one back after
//! somebody walked through it would say nothing about the membership they now
//! hold, which is what `Leave` is for.

use leptos::prelude::*;
use rn_api::commands::RevokeLink;
use rn_ui::{Live, PageHead};

use crate::api::{Refusal, attempt};
use crate::parts::{Act, Note, titled, when};
use crate::rows::Invitation;

#[component]
pub fn Invitations() -> impl IntoView {
    let invitations = Live::<Invitation>::subscribe("invitations", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let rows = move || {
        let rows = invitations.rows();
        if rows.is_empty() {
            return view! {
                <tr>
                    <td class="empty" colspan="6">"No invitations. Mint one from a group."</td>
                </tr>
            }
            .into_any();
        }
        rows.into_iter()
            .map(|row| {
                let id = row.id.clone();
                let open = row.claimed_at.is_none();
                let withdraw = Callback::new(move |()| {
                    let Ok(link) = id.parse() else {
                        return;
                    };
                    attempt(RevokeLink { link }, refusal, |_| ());
                });
                view! {
                    <tr>
                        <td class="p1">{row.container.display.clone()}</td>
                        <td class="p1">{titled(&row.role)}</td>
                        <td class="p1">{row.claimed_by.clone().unwrap_or_default()}</td>
                        <td class="p2 mono num">
                            {row.claimed_at.map(when).unwrap_or_default()}
                        </td>
                        <td class="p3 mono num">{when(row.expires_at)}</td>
                        <td class="p1 does">
                            <Act label="Withdraw" undo=true on_act=withdraw disabled=!open />
                        </td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };

    view! {
        <PageHead title="Invitations" />
        <div class="sheet">
            <table class="tbl">
                <thead>
                    <tr>
                        <th class="p1" scope="col">"Group"</th>
                        <th class="p1" scope="col">"Role"</th>
                        <th class="p1" scope="col">"Claimed by"</th>
                        <th class="p2" scope="col">"Claimed"</th>
                        <th class="p3" scope="col">"Expires"</th>
                        <th class="p1" scope="col">""</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
        <Note refusal=refusal />
    }
}
