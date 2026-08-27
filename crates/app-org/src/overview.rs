//! `/org` — the organization itself.
//!
//! Four facts and three counts. The counts are folded into the names of the
//! pages that hold them, so "Members 12" is the link and there is no caption
//! under it saying what a member is.
//!
//! The two things that end an organization's arrangement — handing it on, and
//! taking it out of service — are behind a side affordance rather than in the
//! page's flow, because they are done once and read every day.

use leptos::prelude::*;
use rn_api::commands::{Disable, Enable, Transfer};
use rn_ui::PageHead;

use crate::bits::{Aside, Copyable, Note, act, on};
use crate::rows::Org;
use crate::scope::use_scope;

/// The overview, for one organization.
#[component]
pub fn Overview(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let live = use_scope().live::<Org>("org", &[("org", &org)]);
    move || {
        let Some(row) = live.rows().into_iter().next() else {
            return view! { <PageHead title="Organization" /> }.into_any();
        };
        view! {
            <PageHead title=row.display.clone() />
            <Facts row=row.clone() />
            <Sections row=row.clone() />
            <Ownership row=row.clone() />
        }
        .into_any()
    }
}

/// Who owns it, when it started, and the id everything else is addressed by.
#[component]
fn Facts(row: Org) -> impl IntoView {
    let id = row.public_id.clone();
    view! {
        <dl class="facts">
            <dt>"Owner"</dt>
            <dd>{row.owner.display.clone()}</dd>
            <dt>"Founded"</dt>
            <dd class="num">{on(row.created_at)}</dd>
            <dt>"Status"</dt>
            <dd>{row.status.clone()}</dd>
            <dt>"You"</dt>
            <dd>{row.my_role.clone()}</dd>
            <dt>"Identifier"</dt>
            <dd>
                <Copyable value=id label="Copy the organization id" />
            </dd>
        </dl>
    }
}

/// The three pages under this one, each named with what it holds.
#[component]
fn Sections(row: Org) -> impl IntoView {
    view! {
        <nav class="sections">
            <a href="/org/members">"Members " <span class="num">{row.members}</span></a>
            <a href="/org/groups">"Groups " <span class="num">{row.groups}</span></a>
            <a href="/org/documents">"Documents " <span class="num">{row.documents}</span></a>
        </nav>
    }
}

/// Handing the organization on, and taking it out of service.
#[component]
fn Ownership(row: Org) -> impl IntoView {
    let to = RwSignal::new(String::new());
    let transfer_note = RwSignal::new(None::<String>);
    let status_note = RwSignal::new(None::<String>);
    let resource = StoredValue::new(row.resource.clone());
    let party = StoredValue::new(row.public_id.clone());
    let disabled = row.status == "disabled";
    let owned = row.owned_by_me;

    view! {
        <div class="asides">
            <Show when=move || owned>
                <Aside title="Transfer ownership">
                    <label class="choice wide">
                        <span>"New owner"</span>
                        <input
                            type="text"
                            spellcheck="false"
                            placeholder="p_… o_…"
                            prop:value=move || to.get()
                            on:input=move |event| to.set(event_target_value(&event))
                        />
                    </label>
                    <button
                        type="button"
                        class="commit"
                        disabled=move || to.get().trim().is_empty()
                        on:click=move |_| {
                            let Ok(id) = to.get_untracked().trim().parse() else {
                                transfer_note.set(Some("That is not a public id.".to_owned()));
                                return;
                            };
                            let Ok(resource) = resource.get_value().parse() else {
                                return;
                            };
                            act(
                                transfer_note,
                                Transfer { resource, to: id },
                                move |_| to.set(String::new()),
                            );
                        }
                    >
                        "Transfer ownership"
                    </button>
                    <Note note=transfer_note />
                </Aside>
            </Show>
            <Aside title=if disabled { "Enable organization" } else { "Disable organization" }>
                <button
                    type="button"
                    class="commit"
                    on:click=move |_| {
                        let Ok(id) = party.get_value().parse() else {
                            return;
                        };
                        if disabled {
                            act(status_note, Enable { party: id }, |_| ());
                        } else {
                            act(
                                status_note,
                                Disable {
                                    party: id,
                                    reason: "operator ruling".to_owned(),
                                },
                                |_| (),
                            );
                        }
                    }
                >
                    {if disabled { "Enable" } else { "Disable" }}
                </button>
                <Note note=status_note />
            </Aside>
        </div>
    }
}
