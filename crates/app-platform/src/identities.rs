//! `/platform/identities` — every registration, and what one holds.
//!
//! An identity is a registration, not a human, so the list is about where it
//! came from and what it has proven: its source, its home zone, the person it
//! resolved to, and the kinds of factor on it. A factor's value is never here
//! and never was — an address arrives masked, and a password has no half worth
//! showing.
//!
//! The drill-in is the two things an operator opens a registration to see: the
//! sessions it is carrying, each revocable, and the match candidates raised
//! about it.

use leptos::prelude::*;
use rn_api::commands::RevokeSession;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Line, Panel, State, drill, refusal, run_with, when};
use crate::rows::{Factor, Identity, IdentityDetail};

/// What a registration's factors read as in one cell: the kinds it holds, and
/// whether each is proven.
fn factors(held: &[Factor]) -> String {
    if held.is_empty() {
        return "—".to_owned();
    }
    held.iter()
        .map(|factor| match (&factor.hint, factor.verified) {
            (Some(hint), true) => format!("{} {hint} proven", factor.kind),
            (Some(hint), false) => format!("{} {hint}", factor.kind),
            (None, true) => format!("{} proven", factor.kind),
            (None, false) => factor.kind.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[component]
pub fn Identities() -> impl IntoView {
    let live = rn_ui::Live::<Identity>::subscribe("platform-identities", &[]);
    let selected = RwSignal::new(None::<String>);
    let detail = drill::<IdentityDetail>("platform-identity", selected);

    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("Person", |row: &Identity| {
            row.person
                .as_ref()
                .map_or_else(|| "unresolved".to_owned(), |person| person.display.clone())
        }),
        Column::new("Source", |row: &Identity| row.source.clone()),
        Column::new("Status", |row: &Identity| row.status.clone()),
        Column::new("Factors", |row: &Identity| factors(&row.factors))
            .priority(Priority::Secondary),
        Column::new("Zone", |row: &Identity| row.home_zone.clone()).priority(Priority::Tertiary),
        Column::new("Created", |row: &Identity| when(row.created_at))
            .mono()
            .priority(Priority::Tertiary),
        Column::new("Id", |row: &Identity| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Identity| {
        selected.set(Some(row.public_id.to_string()));
    });

    view! {
        <PageHead title="Identities">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="Nobody has registered on this deployment."
                    on_row=open
                />
            </div>
            <Show when=move || detail.get().is_some()>
                {move || {
                    detail
                        .get()
                        .map(|identity| {
                            view! { <Detail identity=identity selected=selected then=detail.refresh() /> }
                        })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(
    identity: IdentityDetail,
    selected: RwSignal<Option<String>>,
    then: Callback<()>,
) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let title = identity
        .person
        .as_ref()
        .map_or_else(|| identity.source.clone(), |person| person.display.clone());
    let facts = vec![
        ("Source", identity.source.clone()),
        ("Zone", identity.home_zone.clone()),
        ("Created", when(identity.created_at)),
        (
            "Person",
            identity
                .person
                .as_ref()
                .map_or_else(|| "unresolved".to_owned(), |p| p.public_id.to_string()),
        ),
        ("Id", identity.public_id.to_string()),
    ];

    let factor_lines = identity
        .factors
        .clone()
        .into_iter()
        .map(|factor| {
            let proven = if factor.verified {
                "proven"
            } else {
                "unproven"
            };
            view! {
                <Line
                    lead=factor.hint.clone().unwrap_or_else(|| factor.kind.clone())
                    trail=factor.kind.clone()
                >
                    <State value=proven />
                </Line>
            }
        })
        .collect_view();

    let sessions = identity
        .sessions
        .clone()
        .into_iter()
        .map(|session| {
            let id = session.public_id.clone();
            let run = run_with(move || {
                let session = id.clone();
                async move {
                    rn_ui::invoke::<RevokeSession, serde_json::Value>(RevokeSession { session })
                        .await
                        .map(|_| ())
                        .map_err(|error| refusal(&error))
                }
            });
            view! {
                <Line
                    lead=session.public_id.to_string()
                    trail=format!("as {} \u{00b7} seen {}", session.acting_display, when(session.last_seen_at))
                >
                    <Act label="Revoke" run=run then=then />
                </Line>
            }
        })
        .collect_view();

    let candidates = identity
        .candidates
        .clone()
        .into_iter()
        .map(|candidate| {
            let status = candidate.status.clone();
            view! {
                <Line
                    lead=candidate.public_id.to_string()
                    trail=format!("{} \u{00b7} {:.2}", candidate.signal, candidate.score)
                >
                    <State value=status />
                </Line>
            }
        })
        .collect_view();

    let counts = (
        identity.factors.len(),
        identity.sessions.len(),
        identity.candidates.len(),
    );

    view! {
        <Panel title=title on_close=close>
            <div class="panel-state">
                <State value=identity.status.clone() />
            </div>
            <Facts facts=facts />
            <Group label="Factors" count=counts.0 empty="It has proven nothing.">
                {factor_lines}
            </Group>
            <Group label="Sessions" count=counts.1 empty="It is carrying no live session.">
                {sessions}
            </Group>
            <Group label="Candidates" count=counts.2 empty="Nothing has been proposed about it.">
                {candidates}
            </Group>
        </Panel>
    }
}
