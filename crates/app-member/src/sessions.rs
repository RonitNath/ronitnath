//! `/app/sessions` — the devices this person is signed in on.

use leptos::prelude::*;
use rn_api::commands::RevokeSession;
use rn_ui::{Live, PageHead};

use crate::api::{Refusal, attempt};
use crate::parts::{Act, Note, when};
use crate::rows::Session;

#[component]
pub fn Sessions() -> impl IntoView {
    let sessions = Live::<Session>::subscribe("sessions", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let rows = move || {
        let rows = sessions.rows();
        if rows.is_empty() {
            return view! {
                <tr>
                    <td class="empty" colspan="5">"No sessions."</td>
                </tr>
            }
            .into_any();
        }
        rows.into_iter()
            .map(|row| {
                let id = row.public_id.clone();
                let current = row.current;
                let revoke = Callback::new(move |()| {
                    let id = id.clone();
                    let Ok(session) = id.parse() else {
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
                });
                view! {
                    <tr>
                        <td class="p1 mono num">{when(row.created_at)}</td>
                        <td class="p2 mono num">{when(row.last_seen_at)}</td>
                        <td class="p3 mono num">{when(row.expires_at)}</td>
                        <td class="p1 you">{if row.current { "This device" } else { "" }}</td>
                        <td class="p1 does">
                            <Act
                                label=if current { "Sign out" } else { "Revoke" }
                                undo=true
                                on_act=revoke
                            />
                        </td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };

    view! {
        <PageHead title="Sessions" />
        <div class="sheet">
            <table class="tbl">
            <thead>
                <tr>
                    <th class="p1" scope="col">"Signed in"</th>
                    <th class="p2" scope="col">"Last seen"</th>
                    <th class="p3" scope="col">"Ends"</th>
                    <th class="p1" scope="col">""</th>
                    <th class="p1" scope="col">""</th>
                </tr>
            </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
        <Note refusal=refusal />
    }
}
