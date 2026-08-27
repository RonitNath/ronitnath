//! `/platform/matches` — the match queue, and what a ruling on one produced.
//!
//! Every row is a *question*: two registrations, a signal that suggested they
//! are one human, and a score that orders the queue and decides nothing. No
//! threshold merges anybody — proof does — so the score is rendered as the
//! ranking it is and the disposition sits beside it.
//!
//! Ruling is the operator's half of that. `RuleMatch` carries evidence and the
//! kernel refuses it empty; so does this panel, and it says why rather than
//! greying a button out. The evidence lands on every `person_link` row the
//! merge writes and on the audit row, both append-only, so a ruling stays
//! attributable for as long as the rows exist — which is what the drill-in
//! shows once it has been made.

use leptos::prelude::*;
use rn_api::commands::{RuleMatch, Split};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Line, Panel, State, drill, refusal, run_with, when};
use crate::rows::{Candidate, CandidateDetail, PersonRef};

/// How a side of the pair reads: the person it resolves to, or that it does
/// not resolve to one.
fn side(person: Option<&PersonRef>) -> String {
    person.map_or_else(|| "unresolved".to_owned(), |person| person.display.clone())
}

#[component]
pub fn Matches() -> impl IntoView {
    let live = rn_ui::Live::<Candidate>::subscribe("platform-matches", &[]);
    let selected = RwSignal::new(None::<String>);
    let detail = drill::<CandidateDetail>("platform-match", selected);

    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("A", |row: &Candidate| side(row.person_a.as_ref())),
        Column::new("B", |row: &Candidate| side(row.person_b.as_ref())),
        Column::new("Signal", |row: &Candidate| row.signal.clone()),
        Column::new("Score", |row: &Candidate| format!("{:.2}", row.score)).mono(),
        Column::new("Status", |row: &Candidate| row.status.clone()),
        Column::new("Raised", |row: &Candidate| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Candidate| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Candidate| {
        selected.set(Some(row.public_id.to_string()));
    });

    view! {
        <PageHead title="Matches">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="No pair has been proposed. Signals raise these on the observation lane, and an operator can raise one by hand."
                    on_row=open
                />
            </div>
            <Show when=move || detail.get().is_some()>
                {move || {
                    detail
                        .get()
                        .map(|candidate| {
                            view! { <Detail candidate=candidate selected=selected then=detail.refresh() /> }
                        })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(
    candidate: CandidateDetail,
    selected: RwSignal<Option<String>>,
    then: Callback<()>,
) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let one_person = match (&candidate.person_a, &candidate.person_b) {
        (Some(a), Some(b)) => a.public_id == b.public_id,
        _ => false,
    };
    let facts = vec![
        ("Signal", candidate.signal.clone()),
        ("Score", format!("{:.2}", candidate.score)),
        ("Raised", when(candidate.created_at)),
        ("A", candidate.identity_a.to_string()),
        ("B", candidate.identity_b.to_string()),
        (
            "Person A",
            candidate
                .person_a
                .as_ref()
                .map_or_else(|| "unresolved".to_owned(), |p| p.public_id.to_string()),
        ),
        (
            "Person B",
            candidate
                .person_b
                .as_ref()
                .map_or_else(|| "unresolved".to_owned(), |p| p.public_id.to_string()),
        ),
        ("Id", candidate.public_id.to_string()),
    ];

    let links = candidate
        .links
        .clone()
        .into_iter()
        .map(|link| {
            let method = link.method.clone();
            let evidence = link.evidence.clone();
            view! {
                <div class="line link">
                    <span class="line-lead mono">{link.identity.to_string()}</span>
                    <span class="line-trail">
                        {format!("\u{2192} {} \u{00b7} {}", link.person, when(link.at))}
                    </span>
                    <State value=method />
                    {evidence.map(|evidence| view! { <p class="evidence">{evidence}</p> })}
                </div>
            }
        })
        .collect_view();

    let aliases = candidate
        .alias
        .clone()
        .into_iter()
        .map(|alias| {
            view! {
                <Line
                    lead=alias.was.to_string()
                    trail=format!("\u{2192} {} \u{00b7} {}", alias.now, when(alias.at))
                />
            }
        })
        .collect_view();

    let counts = (candidate.links.len(), candidate.alias.len());
    let splits = candidate.identity_a.clone();
    let split_b = candidate.identity_b.clone();

    view! {
        <Panel title=format!("{} \u{00d7} {}", side(candidate.person_a.as_ref()), side(candidate.person_b.as_ref())) on_close=close>
            <div class="panel-state">
                <State value=candidate.status.clone() />
                <Show when=move || one_person>
                    <span class="resolved">"one person"</span>
                </Show>
            </div>
            <Facts facts=facts />
            <Ruling
                candidate=candidate.public_id.to_string()
                status=candidate.status.clone()
                then=then
            />
            <Group label="Person links" count=counts.0 empty="Neither registration has ever been attached to a person by a ruling.">
                {links}
            </Group>
            <Group label="Alias" count=counts.1 empty="No id has been absorbed, so nothing redirects.">
                {aliases}
            </Group>
            <section class="group">
                <h3>"Split"</h3>
                <p class="none">
                    "Detach one registration onto a person of its own. The reason lands on the new link row."
                </p>
                <div class="group-rows">
                    <Splitter identity=splits.to_string() then=then />
                    <Splitter identity=split_b.to_string() then=then />
                </div>
            </section>
        </Panel>
    }
}

/// The ruling: evidence, and the two ways it can go.
#[component]
fn Ruling(candidate: String, status: String, then: Callback<()>) -> impl IntoView {
    let evidence = RwSignal::new(String::new());
    let open = status == "proposed";
    // Stated once, under the field it is about, because both buttons wait on
    // the same thing and the same sentence twice is not twice as clear.
    let missing = Signal::derive(move || {
        if !open {
            Some("This candidate has already been ruled on.".to_owned())
        } else if evidence.get().trim().is_empty() {
            Some("Evidence is required: it lands on the link row and the audit row.".to_owned())
        } else {
            None
        }
    });
    let held = Signal::derive(move || missing.get().is_some());

    let rule = |candidate: String, same_person: bool, evidence: RwSignal<String>| {
        run_with(move || {
            let candidate = candidate.clone();
            let evidence = evidence.get_untracked().trim().to_owned();
            async move {
                let Ok(candidate) = candidate.parse() else {
                    return Err("that is not a candidate id".to_owned());
                };
                rn_ui::invoke::<RuleMatch, serde_json::Value>(RuleMatch {
                    candidate,
                    same_person,
                    evidence,
                })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
            }
        })
    };

    view! {
        <section class="group ruling">
            <h3>"Ruling"</h3>
            <textarea
                class="evidence-field"
                aria-label="Evidence"
                placeholder="What did you rely on?"
                rows="3"
                prop:value=move || evidence.get()
                on:input=move |event| evidence.set(event_target_value(&event))
            ></textarea>
            <p class="none">{move || missing.get()}</p>
            <div class="actions row">
                <Act
                    label="Same person"
                    run=rule(candidate.clone(), true, evidence)
                    held=held
                    then=then
                />
                <Act label="Two people" run=rule(candidate, false, evidence) held=held then=then />
            </div>
        </section>
    }
}

/// Undo a merge for one registration.
#[component]
fn Splitter(identity: String, then: Callback<()>) -> impl IntoView {
    let evidence = RwSignal::new(String::new());
    let held = Signal::derive(move || evidence.get().trim().is_empty());
    let name = identity.clone();
    let run = run_with(move || {
        let identity = identity.clone();
        let evidence = evidence.get_untracked().trim().to_owned();
        async move {
            let Ok(identity) = identity.parse() else {
                return Err("that is not an identity id".to_owned());
            };
            rn_ui::invoke::<Split, serde_json::Value>(Split { identity, evidence })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
        }
    });
    view! {
        <div class="line split-act">
            <span class="line-lead mono">{name}</span>
            <input
                type="text"
                class="reason"
                aria-label="Reason"
                placeholder="Reason"
                prop:value=move || evidence.get()
                on:input=move |event| evidence.set(event_target_value(&event))
            />
            <Act label="Split" run=run held=held then=then />
        </div>
    }
}
