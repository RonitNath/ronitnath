//! `/platform/operators` — who holds `platform:* #operator`, and who granted
//! it to them.
//!
//! Platform administration is a relation and not a column, so this is a list of
//! relation rows: `platform:0 #operator @person:P`, one per operator, with the
//! person whose identity wrote it.
//!
//! The first row has nobody who granted it, and that is not missing data. The
//! bootstrap creates the first operator out of nothing — it fires only on a
//! deployment that has none, and refuses the moment one exists — so there was
//! nobody to attribute it to. The cell says **granted by configuration**,
//! because that is what happened.
//!
//! Revoking is behind the panel and carries a reason. Revoking the last one is
//! refused by the kernel and the refusal appears beside the button: a
//! deployment with no operator is one nobody can grant the first of again.

use leptos::prelude::*;
use rn_api::commands::RevokeOperator;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Panel, State, refusal, run_with, when};
use crate::rows::Operator;

/// How an operator's grant reads when nobody granted it.
const BY_CONFIGURATION: &str = "granted by configuration";

/// Who granted this one, as the cell says it.
#[must_use]
pub fn granted_by(row: &Operator) -> String {
    match (row.granted_by, &row.granted_display) {
        (true, Some(name)) => name.clone(),
        // Granted by somebody whose registration no longer resolves to a
        // person: the row is still theirs, and the deployment cannot name them.
        (true, None) => "an operator".to_owned(),
        (false, _) => BY_CONFIGURATION.to_owned(),
    }
}

#[component]
pub fn Operators() -> impl IntoView {
    let live = rn_ui::Live::<Operator>::subscribe("platform-operators", &[]);
    let selected = RwSignal::new(None::<Operator>);
    let rows = Signal::derive(move || live.rows());

    let columns = vec![
        Column::new("Operator", |row: &Operator| row.display.clone()),
        Column::new("Handle", |row: &Operator| {
            row.handle.clone().unwrap_or_else(|| "\u{2014}".to_owned())
        })
        .mono(),
        Column::new("Status", |row: &Operator| row.status.clone()).state(),
        Column::new("Granted by", granted_by),
        Column::new("Granted", |row: &Operator| when(row.at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Operator| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Operator| selected.set(Some(row)));

    view! {
        <PageHead title="Operators">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="Nobody holds the operator relation, which is a deployment nobody can administer: the bootstrap only fires on an empty one."
                    on_row=open
                />
            </div>
            <Show when=move || selected.get().is_some()>
                {move || {
                    selected
                        .get()
                        .map(|operator| view! { <Detail operator=operator selected=selected /> })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(operator: Operator, selected: RwSignal<Option<Operator>>) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let reason = RwSignal::new(String::new());
    let facts = vec![
        (
            "Handle",
            operator
                .handle
                .clone()
                .unwrap_or_else(|| "\u{2014}".to_owned()),
        ),
        ("Granted by", granted_by(&operator)),
        ("Granted", when(operator.at)),
        ("Id", operator.public_id.to_string()),
    ];
    let blocked = Signal::derive(move || {
        reason
            .get()
            .trim()
            .is_empty()
            .then(|| "A reason is required: it lands on the audit row.".to_owned())
    });
    let person = operator.public_id.clone();
    let run = run_with(move || {
        let person = person.clone();
        let reason = reason.get_untracked().trim().to_owned();
        async move {
            rn_ui::invoke::<RevokeOperator, serde_json::Value>(RevokeOperator { person, reason })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
        }
    });

    view! {
        <Panel title=operator.display.clone() on_close=close>
            <div class="panel-state">
                <State value=operator.status.clone() />
            </div>
            <Facts facts=facts />
            <section class="group">
                <h3>"Revoke"</h3>
                <div class="actions">
                    <input
                        type="text"
                        class="reason"
                        aria-label="Reason"
                        placeholder="Reason"
                        prop:value=move || reason.get()
                        on:input=move |event| reason.set(event_target_value(&event))
                    />
                    <Act label="Revoke operator" run=run blocked=blocked />
                </div>
            </section>
        </Panel>
    }
}
