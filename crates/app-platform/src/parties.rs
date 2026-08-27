//! `/platform` — every party, and what one party is made of.
//!
//! A party is the thing that can own, be granted, or belong, so the drill-in
//! is the four answers that follow: the registrations that resolve to it, the
//! containers it belongs to, what it owns, and what it has been granted.
//!
//! One box sits above the table and it is not a filter: the filter box in the
//! table searches the page that arrived, and this searches the *deployment* —
//! three exact seeks, on a handle, on an email factor and on a public id, with
//! no prefix scan over `display_name`, which has no index. A miss is empty,
//! because an operator searching for somebody who is not there is not being
//! probed.
//!
//! A row opens the person's own page (`/platform/parties/<id>`, frame F3), not
//! a panel: what the deployment knows about a human is nine lists, and nine
//! lists in a twenty-four-rem column is a scroll.
//!
//! `Disable` and `Enable` act on any party. A person is theirs and an
//! operator's; an organization is its owner's and an operator's; a group is
//! whoever owns it. An operator may do any of it, which is what this page is,
//! so the control is offered on every row and the one precondition left is the
//! reason — which is not decoration: it is what the audit row records about
//! why every session that party held was deleted.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use rn_api::commands::{Disable, Enable};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Line, Panel, State, drill, refusal, run_with, when};
use crate::rows::{Found, Party, PartyDetail};

#[component]
pub fn Parties() -> impl IntoView {
    let live = rn_ui::Live::<Party>::subscribe("platform-parties", &[]);
    let selected = RwSignal::new(None::<String>);
    let detail = drill::<PartyDetail>("platform-party", selected);
    let found = RwSignal::new(None::<Vec<Found>>);
    let navigate = use_navigate();

    // The deployment's own list, or what the one box found in it.
    let rows = Signal::derive(move || match found.get() {
        None => live.rows(),
        Some(hits) => hits
            .into_iter()
            .map(|hit| Party {
                public_id: hit.public_id,
                kind: hit.kind,
                display: hit.display,
                handle: hit.handle,
                status: hit.status,
                created_at: hit.created_at,
            })
            .collect(),
    });
    let columns = vec![
        Column::new("Name", |row: &Party| row.display.clone()),
        Column::new("Handle", |row: &Party| {
            row.handle.clone().unwrap_or_else(|| "\u{2014}".to_owned())
        })
        .mono(),
        Column::new("Kind", |row: &Party| row.kind.clone()),
        Column::new("Status", |row: &Party| row.status.clone()).state(),
        Column::new("Created", |row: &Party| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Party| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    // A person has a page; every other kind is a panel, because an
    // organization has no registrations, no factors and no merge history and
    // its drill-in is the four lists that already fit in one.
    let open = Callback::new(move |row: Party| {
        if row.kind == "person" {
            navigate(&format!("/parties/{}", row.public_id), Default::default());
        } else {
            selected.set(Some(row.public_id.to_string()));
        }
    });

    view! {
        <PageHead title="Parties">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <Find found=found />
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="No party has been registered on this deployment."
                    on_row=open
                />
            </div>
            <Show when=move || detail.get().is_some()>
                {move || {
                    detail
                        .get()
                        .map(|party| view! { <Detail party=party selected=selected then=detail.refresh() /> })
                }}
            </Show>
        </div>
    }
}

/// The one box: a handle, an address or a public id, exactly.
///
/// Emptying it puts the deployment's own list back, which is why the found
/// rows are an `Option` rather than a vector — no search and a search that
/// found nothing are two different screens.
#[component]
fn Find(found: RwSignal<Option<Vec<Found>>>) -> impl IntoView {
    let asked = RwSignal::new(String::new());
    let ask = move || {
        let q = asked.get_untracked().trim().to_owned();
        spawn_local(async move {
            if q.is_empty() {
                found.set(None);
                return;
            }
            found.set(
                rn_ui::query::<Vec<Found>>("platform-find", &[("q", q.as_str())])
                    .await
                    .ok(),
            );
        });
    };
    view! {
        <div class="find">
            <input
                type="search"
                class="filter"
                aria-label="Find a handle, an address or an id"
                placeholder="Handle, address or id"
                prop:value=move || asked.get()
                on:input=move |event| {
                    asked.set(event_target_value(&event));
                    if asked.get_untracked().trim().is_empty() {
                        found.set(None);
                    }
                }
                on:keydown=move |event: leptos::ev::KeyboardEvent| {
                    if event.key() == "Enter" {
                        ask();
                    }
                }
            />
            <button type="button" class="act" on:click=move |_| ask()>
                "Find"
            </button>
        </div>
    }
}

#[component]
fn Detail(
    party: PartyDetail,
    selected: RwSignal<Option<String>>,
    then: Callback<()>,
) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let facts = vec![
        ("Kind", party.kind.clone()),
        ("Created", when(party.created_at)),
        ("Id", party.public_id.to_string()),
    ];
    let identities = party
        .identities
        .clone()
        .into_iter()
        .map(|identity| {
            let status = identity.status.clone();
            view! {
                <Line lead=identity.public_id.to_string() trail=identity.source.clone()>
                    <State value=status />
                </Line>
            }
        })
        .collect_view();
    let memberships = party
        .memberships
        .clone()
        .into_iter()
        .map(|member| {
            view! {
                <Line lead=member.display.clone() trail=member.kind.clone()>
                    <span class="role">{member.role.clone()}</span>
                </Line>
            }
        })
        .collect_view();
    let resources = party
        .resources
        .clone()
        .into_iter()
        .map(|resource| {
            let status = resource.status.clone();
            view! {
                <Line lead=resource.public_id.to_string() trail=resource.kind.clone()>
                    <State value=status />
                </Line>
            }
        })
        .collect_view();
    let relations = party
        .relations
        .clone()
        .into_iter()
        .map(|held| {
            let object = held
                .object
                .as_ref()
                .map_or_else(|| held.object_kind.clone(), ToString::to_string);
            view! {
                <Line lead=object trail=held.object_kind.clone()>
                    <span class="role">{held.relation.clone()}</span>
                </Line>
            }
        })
        .collect_view();

    let counts = (
        party.identities.len(),
        party.memberships.len(),
        party.resources.len(),
        party.relations.len(),
    );

    view! {
        <Panel title=party.display.clone() on_close=close>
            <div class="panel-state">
                <State value=party.status.clone() />
            </div>
            <Facts facts=facts />
            <Group label="Identities" count=counts.0 empty="No registration resolves to this party.">
                {identities}
            </Group>
            <Group label="Memberships" count=counts.1 empty="It belongs to no organization or group.">
                {memberships}
            </Group>
            <Group label="Owns" count=counts.2 empty="It owns no registered resource.">
                {resources}
            </Group>
            <Group label="Granted" count=counts.3 empty="It has been granted nothing.">
                {relations}
            </Group>
            <Status party=party.clone() then=then />
        </Panel>
    }
}

/// The one status transition a party has, and the reason it carries.
///
/// `Disable` writes its reason onto the audit row, so the field is not
/// decoration: it is the record of why every session this party held was
/// deleted. Empty, the command is not offered. Re-enabling carries nothing —
/// the reason it is being undone is the audit row that disabled it.
#[component]
fn Status(party: PartyDetail, then: Callback<()>) -> impl IntoView {
    let reason = RwSignal::new(String::new());
    let disabled = party.status == "disabled";

    let party_id = party.public_id.clone();
    let blocked = Signal::derive(move || {
        if disabled || !reason.get().trim().is_empty() {
            None
        } else {
            Some("A reason is required: it lands on the audit row.".to_owned())
        }
    });

    let enable_id = party_id.clone();
    let run = if disabled {
        run_with(move || {
            let party = enable_id.clone();
            async move {
                rn_ui::invoke::<Enable, serde_json::Value>(Enable { party })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    } else {
        run_with(move || {
            let party = party_id.clone();
            let reason = reason.get_untracked().trim().to_owned();
            async move {
                rn_ui::invoke::<Disable, serde_json::Value>(Disable { party, reason })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    };

    view! {
        <section class="group">
            <h3>"Status"</h3>
            <div class="actions">
                <Show when=move || !disabled>
                    <input
                        type="text"
                        class="reason"
                        aria-label="Reason"
                        placeholder="Reason"
                        prop:value=move || reason.get()
                        on:input=move |event| reason.set(event_target_value(&event))
                    />
                </Show>
                <Act
                    label=if disabled { "Enable" } else { "Disable" }
                    run=run.clone()
                    blocked=blocked
                    then=then
                />
            </div>
        </section>
    }
}
