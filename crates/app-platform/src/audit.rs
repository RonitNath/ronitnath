//! `/platform/audit` — the whole log, newest first, live at the head.
//!
//! `audit.id` is the change-feed offset, so the first column is the offset
//! itself, in mono with tabular figures: it is the number an operator quotes,
//! resumes a subscription from, and finds a command by. The tail is live
//! because a command's own offset is what arrives on the feed.
//!
//! A row expands to the typed event its command's transaction wrote. That is
//! where a ruling's evidence is, and where a disable's reason is — the whole
//! point of the log is that the reasoning is attached to the change, not to a
//! memory of it.

use leptos::prelude::*;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Facts, Panel, when};
use crate::rows::Audit;

#[component]
pub fn AuditLog() -> impl IntoView {
    let live = rn_ui::Live::<Audit>::subscribe("platform-audit", &[]);
    let selected = RwSignal::new(None::<Audit>);

    // The store keys rows by a zero-padded offset, so its order is
    // chronological; an operator wants the newest first.
    let rows = Signal::derive(move || {
        let mut rows = live.rows();
        rows.reverse();
        rows
    });
    let columns = vec![
        Column::new("Offset", |row: &Audit| row.offset.to_string()).mono(),
        Column::new("Command", |row: &Audit| row.command.clone()),
        Column::new("Actor", |row: &Audit| {
            row.actor_display
                .clone()
                .or_else(|| row.actor.as_ref().map(ToString::to_string))
                .unwrap_or_else(|| "\u{2014}".to_owned())
        }),
        Column::new("At", |row: &Audit| when(row.at))
            .mono()
            .priority(Priority::Secondary),
        // The name, the way the Actor column beside it reads: an operator
        // console that shows an id where a name is available, in the same row,
        // is the interface failing to state. The id is still one hover away,
        // because it is what an operator quotes.
        Column::new("Acting as", |row: &Audit| {
            row.acting_display
                .clone()
                .or_else(|| row.acting_as.as_ref().map(ToString::to_string))
                .unwrap_or_else(|| "\u{2014}".to_owned())
        })
        .titled(|row: &Audit| {
            row.acting_as
                .as_ref()
                .map_or_else(String::new, ToString::to_string)
        })
        .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Audit| selected.set(Some(row)));

    view! {
        <PageHead title="Audit">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="No command has run on this deployment."
                    on_row=open
                />
            </div>
            <Show when=move || selected.get().is_some()>
                {move || selected.get().map(|entry| view! { <Detail entry=entry selected=selected /> })}
            </Show>
        </div>
    }
}

#[component]
fn Detail(entry: Audit, selected: RwSignal<Option<Audit>>) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let facts = vec![
        ("Offset", entry.offset.to_string()),
        ("At", when(entry.at)),
        (
            "Actor",
            entry
                .actor
                .as_ref()
                .map_or_else(|| "\u{2014}".to_owned(), ToString::to_string),
        ),
        (
            "Acting as",
            match (&entry.acting_display, &entry.acting_as) {
                // The panel is the drill-in, so it carries both: the name to
                // read and the id to copy.
                (Some(display), Some(id)) => format!("{display} · {id}"),
                (None, Some(id)) => id.to_string(),
                _ => "\u{2014}".to_owned(),
            },
        ),
    ];
    let payload =
        serde_json::to_string_pretty(&entry.payload).unwrap_or_else(|_| entry.payload.to_string());
    view! {
        <Panel title=entry.command.clone() on_close=close>
            <Facts facts=facts />
            <section class="group">
                <h3>"Event"</h3>
                <pre class="payload mono">{payload}</pre>
            </section>
        </Panel>
    }
}
