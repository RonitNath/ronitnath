//! `/org/audit` — the commands that touched this organization.
//!
//! One row per audited command, forward-paginated by the change-feed offset
//! itself: the page asks for everything after the last offset it holds, which
//! is a range over `audit.id` rather than a sort of the table. The tail is
//! live, so a command run in another tab appears here without a reload.

use leptos::prelude::*;
use rn_api::commands::ALL_COMMAND_NAMES;
use rn_ui::{Column, Live, PageHead, Priority, Table};

use crate::bits::at;
use crate::rows::Audited;

/// The organization's audit tail.
#[component]
pub fn Audit(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let command = RwSignal::new(String::new());
    view! {
        <PageHead title="Audit" />
        <div class="minting">
            <label class="choice">
                <span>"Command"</span>
                <select on:change=move |event| command.set(event_target_value(&event))>
                    <option value="">"every command"</option>
                    {ALL_COMMAND_NAMES
                        .iter()
                        .map(|name| view! { <option value=*name>{*name}</option> })
                        .collect_view()}
                </select>
            </label>
        </div>
        {move || {
            let filter = command.get();
            view! { <Tail org=org.clone() command=filter.clone() /> }
        }}
    }
}

/// One filter's worth of the tail. Re-created when the filter changes, because
/// the filter is part of what the subscription asked for.
#[component]
fn Tail(org: String, command: String) -> impl IntoView {
    let params: Vec<(&str, &str)> = if command.is_empty() {
        vec![("org", &org)]
    } else {
        vec![("org", &org), ("command", &command)]
    };
    let live = Live::<Audited>::subscribe("org-audit", &params);
    let columns = vec![
        Column::new("Offset", |row: &Audited| row.offset.to_string()).mono(),
        Column::new("Command", |row: &Audited| row.command.clone()),
        Column::new("By", |row: &Audited| row.actor.clone().unwrap_or_default()),
        Column::new("At", |row: &Audited| at(row.at)).priority(Priority::Secondary),
    ];
    let rows = Signal::derive(move || live.rows());
    view! {
        <Table rows=rows columns=columns empty="Nothing has been done to this organization yet." />
    }
}
