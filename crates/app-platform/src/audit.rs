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
//!
//! ## The filters, and why an unfiltered page is the live one
//!
//! Five controls — actor, hat, object, command, and a time range — each riding
//! an index (`api::query::platform_filters`). `audit` is the change feed, it is
//! the audit log, and by ruling it has no retention, so a filter over it with
//! no index is not slow: it is unusable, and it becomes unusable on exactly
//! the deployment that most needs it.
//!
//! With no filter set the page is the live tail, subscribed at the head,
//! because a command's own offset is what arrives on the feed. With one set it
//! is a *read*: a filtered view is a question about history, and a
//! subscription that pushed rows into it would be pushing rows the question
//! did not ask for. The controls say which of the two you are looking at by
//! being empty or not.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Facts, Panel, when};
use crate::rows::Audit;

/// What the controls hold. Empty strings are absent parameters.
///
/// The six names below are the whole of what this page can send, and
/// `api::query::platform_filters::CONTROLS` is the same six on the server
/// side: the generated index test walks that list's powerset, so a control
/// added here without an index there fails the build.
#[derive(Clone, Default, PartialEq, Eq)]
struct Controls {
    actor: String,
    hat: String,
    object: String,
    command: String,
    from: String,
    to: String,
}

impl Controls {
    /// Whether anything is set. Nothing set is the live tail.
    fn is_empty(&self) -> bool {
        self.pairs().is_empty()
    }

    /// The query string this combination asks for.
    ///
    /// A blank control is left out rather than sent empty: the server reads a
    /// missing parameter as "no filter" and an unparseable one as the same, so
    /// sending nothing is the honest spelling of nothing.
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        for (name, value) in [
            ("actor", &self.actor),
            ("hat", &self.hat),
            ("object", &self.object),
            ("command", &self.command),
        ] {
            if !value.trim().is_empty() {
                pairs.push((name, value.trim().to_owned()));
            }
        }
        for (name, day) in [("from", &self.from), ("to", &self.to)] {
            if let Some(seconds) = midnight(day) {
                pairs.push((name, seconds.to_string()));
            }
        }
        pairs
    }
}

/// A `YYYY-MM-DD` from a date control, as the unix second that day begins at
/// locally. An empty or unparseable control is no bound at all.
fn midnight(day: &str) -> Option<i64> {
    if day.trim().is_empty() {
        return None;
    }
    let parsed = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(day));
    let millis = parsed.get_time();
    if millis.is_nan() {
        return None;
    }
    Some((millis / 1000.0) as i64)
}

#[component]
pub fn AuditLog() -> impl IntoView {
    let live = rn_ui::Live::<Audit>::subscribe("platform-audit", &[]);
    let selected = RwSignal::new(None::<Audit>);
    let controls = RwSignal::new(Controls::default());
    let filtered = RwSignal::new(Vec::<Audit>::new());

    // A filtered view is a read, and it is re-read whenever a control moves.
    Effect::new(move |_| {
        let asked = controls.get();
        if asked.is_empty() {
            filtered.set(Vec::new());
            return;
        }
        spawn_local(async move {
            let owned = asked.pairs();
            let pairs: Vec<(&str, &str)> = owned
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            if let Ok(rows) = rn_ui::query::<Vec<Audit>>("platform-audit", &pairs).await {
                filtered.set(rows);
            }
        });
    });

    // The store keys rows by a zero-padded offset, so its order is
    // chronological; an operator wants the newest first. A filtered read
    // already arrives newest first, because the statement says so.
    let rows = Signal::derive(move || {
        if controls.get().is_empty() {
            let mut rows = live.rows();
            rows.reverse();
            rows
        } else {
            filtered.get()
        }
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
        <Filters controls=controls />
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

/// The five controls, on one line.
///
/// The three id fields take a public id, which is what an operator copies out
/// of a row beside them; the command is a select over the vocabulary the
/// contract declares, because a filter naming a command that does not exist is
/// a caller bug rather than a search.
#[component]
fn Filters(controls: RwSignal<Controls>) -> impl IntoView {
    let field = move |label: &'static str,
                      read: fn(&Controls) -> String,
                      write: fn(&mut Controls, String)| {
        view! {
            <input
                type="text"
                class="filter mono"
                aria-label=label
                placeholder=label
                prop:value=move || read(&controls.get())
                on:input=move |event| controls.update(|c| write(c, event_target_value(&event)))
            />
        }
    };
    let day = move |label: &'static str,
                    read: fn(&Controls) -> String,
                    write: fn(&mut Controls, String)| {
        view! {
            <input
                type="date"
                class="filter mono"
                aria-label=label
                prop:value=move || read(&controls.get())
                on:input=move |event| controls.update(|c| write(c, event_target_value(&event)))
            />
        }
    };
    view! {
        <div class="filters">
            {field("Actor", |c| c.actor.clone(), |c, v| c.actor = v)}
            {field("Hat", |c| c.hat.clone(), |c, v| c.hat = v)}
            {field("Object", |c| c.object.clone(), |c, v| c.object = v)}
            <select
                class="filter"
                aria-label="Command"
                on:change=move |event| controls.update(|c| c.command = event_target_value(&event))
            >
                <option value="">"Command"</option>
                {rn_api::commands::ALL_COMMAND_NAMES
                    .iter()
                    .map(|name| {
                        view! {
                            <option
                                value=*name
                                selected=move || controls.get().command == *name
                            >
                                {*name}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
            {day("From", |c| c.from.clone(), |c, v| c.from = v)}
            {day("To", |c| c.to.clone(), |c, v| c.to = v)}
            <button
                type="button"
                class="act"
                on:click=move |_| controls.set(Controls::default())
            >
                "Clear"
            </button>
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
