//! `/platform` — every party, and what one party is made of.
//!
//! A party is the thing that can own, be granted, or belong, so the drill-in
//! is the four answers that follow: the registrations that resolve to it, the
//! containers it belongs to, what it owns, and what it has been granted.
//!
//! `Disable` and `Enable` act on a *person*. An organization and a group are
//! parties too and this command does not decode their ids — the kernel refuses
//! them rather than half-understanding one — so the button says so instead of
//! offering an action that would come back declined.

use leptos::prelude::*;
use rn_api::commands::{Disable, Enable};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Line, Panel, State, drill, refusal, run_with, when};
use crate::rows::{Party, PartyDetail};

#[component]
pub fn Parties() -> impl IntoView {
    let live = rn_ui::Live::<Party>::subscribe("platform-parties", &[]);
    let selected = RwSignal::new(None::<String>);
    let detail = drill::<PartyDetail>("platform-party", selected);

    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("Name", |row: &Party| row.display.clone()),
        Column::new("Kind", |row: &Party| row.kind.clone()),
        Column::new("Status", |row: &Party| row.status.clone()),
        Column::new("Created", |row: &Party| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Party| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Party| {
        selected.set(Some(row.public_id.to_string()));
    });

    view! {
        <PageHead title="Parties">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
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
/// deleted. Empty, the command is not offered.
#[component]
fn Status(party: PartyDetail, then: Callback<()>) -> impl IntoView {
    let reason = RwSignal::new(String::new());
    let is_person = party.kind == "person";
    let disabled = party.status == "disabled";

    let party_id = party.public_id.clone();
    let blocked = Signal::derive(move || {
        if !is_person {
            Some("Only a person is disabled by this command.".to_owned())
        } else if disabled {
            None
        } else if reason.get().trim().is_empty() {
            Some("A reason is required: it lands on the audit row.".to_owned())
        } else {
            None
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
                <Show when=move || is_person && !disabled>
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
