//! Who else may read this document, and who owns it.
//!
//! A share names a *subject*: one of the reader's own groups, chosen from a
//! list, or a person by the public id they were given. There is no directory
//! and no search — a person's id is theirs to hand out, and a page that let
//! you look one up would be an enumeration endpoint with a nicer label.
//!
//! `Transfer` is here rather than beside the title because it is the rare and
//! irreversible one: it moves the row to another owner and the only way back
//! is that owner transferring it again.

use leptos::prelude::*;
use rn_api::commands::{Revoke, Share, Transfer};
use rn_api::whoami::DocRole;
use rn_ui::{Commit, Live, use_whoami};

use crate::api::{Refusal, attempt};
use crate::parts::{Act, Note, Section, titled, when};
use crate::rows::{Group, Share as Grant};

#[component]
pub fn Sharing(
    /// The document being shared, as its public id.
    #[prop(into)]
    document: Signal<String>,
) -> impl IntoView {
    let groups = Live::<Group>::subscribe("groups", &[]);
    let shares = Live::<Grant>::subscribe("shares", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let subject = RwSignal::new(String::new());
    let typed = RwSignal::new(String::new());
    let relation = RwSignal::new("viewer".to_owned());

    let share = Callback::new(move |_: u64| {
        let wanted = {
            let picked = subject.get_untracked();
            if picked.is_empty() {
                typed.get_untracked()
            } else {
                picked
            }
        };
        let (Ok(resource), Ok(subject_id)) = (document.get_untracked().parse(), wanted.parse())
        else {
            refusal.set(Some(Refusal::Declined));
            return;
        };
        attempt(
            Share {
                resource,
                subject: subject_id,
                relation: role_of(&relation.get_untracked()),
            },
            refusal,
            move |_| typed.set(String::new()),
        );
    });

    let granted = move || {
        let id = document.get();
        let rows: Vec<Grant> = shares
            .rows()
            .into_iter()
            .filter(|row| row.document == id)
            .collect();
        if rows.is_empty() {
            return view! { <p class="quiet">"Nobody else."</p> }.into_any();
        }
        rows.into_iter()
            .map(|row| {
                let id = id.clone();
                let subject = row.subject.clone();
                let relation = row.relation.clone();
                let revoke = Callback::new(move |()| {
                    let (Ok(resource), Ok(subject)) = (id.parse(), subject.parse()) else {
                        return;
                    };
                    attempt(
                        Revoke {
                            resource,
                            subject,
                            relation: role_of(&relation),
                        },
                        refusal,
                        |_| {},
                    );
                });
                view! {
                    <tr>
                        <td class="p1">{row.display.clone()}</td>
                        <td class="p1">{titled(&row.relation)}</td>
                        <td class="p3 mono num">{when(row.at)}</td>
                        <td class="p1 does">
                            <Act label="Revoke" undo=true on_act=revoke />
                        </td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };

    let options = move || {
        groups
            .rows()
            .into_iter()
            .map(|group| {
                view! { <option value=group.public_id.clone()>{group.display.clone()}</option> }
            })
            .collect_view()
    };

    view! {
        <Section title="Shared with">
            <table class="tbl">
                <thead>
                    <tr>
                        <th class="p1" scope="col">"Subject"</th>
                        <th class="p1" scope="col">"May"</th>
                        <th class="p3" scope="col">"Since"</th>
                        <th class="p1" scope="col">""</th>
                    </tr>
                </thead>
                <tbody>{granted}</tbody>
            </table>
            <div class="inline">
                <div class="field">
                    <label for="share-group">"One of your groups"</label>
                    <select
                        id="share-group"
                        on:change=move |event| subject.set(event_target_value(&event))
                    >
                        <option value="">"—"</option>
                        {options}
                    </select>
                </div>
                <div class="field">
                    <label for="share-person">"Or a person, by their id"</label>
                    <input
                        id="share-person"
                        type="text"
                        prop:value=move || typed.get()
                        on:input=move |event| typed.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="subject" />
                </div>
                <div class="field">
                    <label for="share-relation">"May"</label>
                    <select
                        id="share-relation"
                        on:change=move |event| relation.set(event_target_value(&event))
                    >
                        <option value="viewer">"Read"</option>
                        <option value="commenter">"Read and comment"</option>
                        <option value="editor">"Read, comment and edit"</option>
                    </select>
                </div>
                <Commit
                    label="Share"
                    version=Signal::derive(|| 0)
                    on_commit=share
                    disabled=Signal::derive(move || {
                        subject.get().is_empty() && typed.get().trim().is_empty()
                    })
                />
            </div>
            <Handing document=document refusal=refusal />
            <Note refusal=refusal />
        </Section>
    }
}

/// Transfer: the owner changes, and nothing else about the row does.
#[component]
fn Handing(
    #[prop(into)] document: Signal<String>,
    refusal: RwSignal<Option<Refusal>>,
) -> impl IntoView {
    let who = use_whoami();
    let to = RwSignal::new(String::new());
    let typed = RwSignal::new(String::new());

    let hand = Callback::new(move |_: u64| {
        let wanted = {
            let picked = to.get_untracked();
            if picked.is_empty() {
                typed.get_untracked()
            } else {
                picked
            }
        };
        let (Ok(resource), Ok(owner)) = (document.get_untracked().parse(), wanted.parse()) else {
            refusal.set(Some(Refusal::Declined));
            return;
        };
        attempt(
            Transfer {
                resource,
                to: owner,
            },
            refusal,
            move |_| typed.set(String::new()),
        );
    });

    let organizations = move || {
        who.get()
            .map(|whoami| {
                whoami
                    .organizations
                    .iter()
                    .map(|org| {
                        view! {
                            <option value=org.public_id.to_string()>{org.display.clone()}</option>
                        }
                    })
                    .collect_view()
            })
            .into_any()
    };

    view! {
        <div class="inline">
            <div class="field">
                <label for="transfer-to">"Hand it to an organization"</label>
                <select id="transfer-to" on:change=move |event| to.set(event_target_value(&event))>
                    <option value="">"—"</option>
                    {organizations}
                </select>
            </div>
            <div class="field">
                <label for="transfer-person">"Or a person, by their id"</label>
                <input
                    id="transfer-person"
                    type="text"
                    prop:value=move || typed.get()
                    on:input=move |event| typed.set(event_target_value(&event))
                />
                <Note refusal=refusal field="to" />
            </div>
            <Commit
                label="Transfer"
                version=Signal::derive(|| 0)
                on_commit=hand
                disabled=Signal::derive(move || {
                    to.get().is_empty() && typed.get().trim().is_empty()
                })
            />
        </div>
    }
}

fn role_of(raw: &str) -> DocRole {
    match raw {
        "commenter" => DocRole::Commenter,
        "editor" => DocRole::Editor,
        _ => DocRole::Viewer,
    }
}
