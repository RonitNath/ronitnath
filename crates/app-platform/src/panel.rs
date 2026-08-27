//! The pieces the seven platform pages are built from.
//!
//! An operator console is a table and a panel: the table is the deployment,
//! the panel is the one row being looked at. Everything here is that shape —
//! a drill-in that fetches when a row is chosen, a dense fact list, a nested
//! row list for the arrays inside a panel, and a button that runs a command
//! and says what happened next to itself.
//!
//! A state is a word in its own colour and never a tinted capsule, so
//! [`State`] renders text with a `data-state` attribute and `platform.css`
//! colours it. There is no status dot anywhere in this bundle.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsValue;

/// A command, as a button runs it: it either happened or it says why not.
pub type Run = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), String>>>> + Send + Sync>;

/// Wrap an async closure as a [`Run`].
pub fn run_with<F, Fut>(command: F) -> Run
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + 'static,
{
    Arc::new(move || Box::pin(command()) as Pin<Box<dyn Future<Output = Result<(), String>>>>)
}

/// An instant, as an operator reads one: local, to the minute, tabular.
///
/// Zero is not a time. `session.expires_at` is never zero, but a column that
/// rendered 1970 for a missing value would be a column that lies quietly.
#[must_use]
pub fn when(unix_seconds: i64) -> String {
    if unix_seconds <= 0 {
        return "—".to_owned();
    }
    let date = js_sys::Date::new(&JsValue::from_f64(unix_seconds as f64 * 1000.0));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        date.get_full_year(),
        date.get_month() + 1,
        date.get_date(),
        date.get_hours(),
        date.get_minutes()
    )
}

/// An open drill-in: the row, and the way to ask for it again.
///
/// A drill-in is read rather than subscribed — it is addressed by a parameter
/// the socket does not carry — so a command run from inside one has to say
/// when it has changed what the panel is showing. That is what
/// [`Drill::refresh`] is for, and every command in this bundle passes it: an
/// operator who rules on a candidate and sees nothing happen has no way to
/// tell a ruling from a refusal.
pub struct Drill<T: Send + Sync + 'static> {
    found: RwSignal<Option<T>>,
    revision: RwSignal<u64>,
}

impl<T: Send + Sync + 'static> Clone for Drill<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Drill<T> {}

impl<T: Clone + Send + Sync + 'static> Drill<T> {
    /// The row, as a reactive read.
    pub fn get(&self) -> Option<T> {
        self.found.get()
    }

    /// Ask for it again. Handed to a command as its `then`.
    pub fn refresh(self) -> Callback<()> {
        let revision = self.revision;
        Callback::new(move |()| revision.update(|r| *r += 1))
    }
}

/// Fetch one drill-in row whenever the selection changes, or on demand.
///
/// Selecting nothing clears it, rather than leaving the last row on screen
/// under a new heading.
pub fn drill<T>(query: &'static str, selected: RwSignal<Option<String>>) -> Drill<T>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static,
{
    let found = RwSignal::new(None::<T>);
    let revision = RwSignal::new(0u64);
    Effect::new(move |_| {
        revision.track();
        let Some(id) = selected.get() else {
            found.set(None);
            return;
        };
        spawn_local(async move {
            let rows = rn_ui::query::<Vec<T>>(query, &[("subject", id.as_str())]).await;
            found.set(rows.ok().and_then(|mut rows| rows.pop()));
        });
    });
    Drill { found, revision }
}

/// The drill-in column: what is open, and the way out of it.
#[component]
pub fn Panel(
    /// What this panel is about — an id, a name, a command.
    #[prop(into)]
    title: String,
    /// Clears the selection.
    #[prop(into)]
    on_close: Callback<()>,
    /// The panel's contents.
    children: Children,
) -> impl IntoView {
    view! {
        <aside class="panel">
            <div class="panel-head">
                <h2>{title}</h2>
                <button type="button" aria-label="Close" on:click=move |_| on_close.run(())>
                    "\u{00d7}"
                </button>
            </div>
            {children()}
        </aside>
    }
}

/// A dense list of label-and-value facts.
///
/// Label left, value right against a shared axis, because these are short
/// pairs read down a narrow column — the arrangement stops being right when
/// the gap outgrows the content, which is what the panel's width is for.
#[component]
pub fn Facts(
    /// The pairs, in order.
    facts: Vec<(&'static str, String)>,
) -> impl IntoView {
    let rows = facts
        .into_iter()
        .map(|(label, value)| {
            view! {
                <div class="fact">
                    <span class="fact-label">{label}</span>
                    <span class="fact-value mono">{value}</span>
                </div>
            }
        })
        .collect_view();
    view! { <div class="facts">{rows}</div> }
}

/// A named group inside a panel, with its count folded into the heading.
#[component]
pub fn Group(
    /// What the group is.
    #[prop(into)]
    label: String,
    /// How many rows it holds.
    count: usize,
    /// What to say when it holds none. Say what would be here and why it is
    /// not, because "none" tells a reader nothing.
    #[prop(into)]
    empty: String,
    /// The rows.
    children: Children,
) -> impl IntoView {
    view! {
        <section class="group">
            <h3>
                {label} <span class="count num">{count}</span>
            </h3>
            {if count == 0 {
                view! { <p class="none">{empty}</p> }.into_any()
            } else {
                view! { <div class="group-rows">{children()}</div> }.into_any()
            }}
        </section>
    }
}

/// One line inside a group: a lead, a trailing note, and whatever else.
#[component]
pub fn Line(
    /// The thing this line is about.
    #[prop(into)]
    lead: String,
    /// What is true of it.
    #[prop(into)]
    trail: String,
    /// A command, a state word, anything that belongs on the same line.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="line">
            <span class="line-lead mono">{lead}</span>
            <span class="line-trail">{trail}</span>
            {children.map(|children| children())}
        </div>
    }
}

/// A state, as a word in its own colour.
#[component]
pub fn State(
    /// The word the schema uses. Rendered as itself: `active`, `proposed`,
    /// `merged` — an operator reads the vocabulary, not a translation of it.
    #[prop(into)]
    value: String,
) -> impl IntoView {
    let word = value.clone();
    view! {
        <span class="state" data-state=value>
            {word}
        </span>
    }
}

/// A button that runs a command and says what happened beside itself.
///
/// The reason a command cannot run yet is shown as text rather than left
/// implicit behind a disabled control: a button that refuses and explains
/// nothing is a button a reader retries. Where two buttons share one
/// precondition the caller states it once and passes [`held`](Act) instead,
/// because the same sentence twice is not twice as clear.
#[component]
pub fn Act(
    /// The verb and its object: "Disable", "Rule same person".
    #[prop(into)]
    label: String,
    /// What to run.
    run: Run,
    /// A precondition the caller has not met, in the caller's terms.
    #[prop(optional, into)]
    blocked: Signal<Option<String>>,
    /// Held for a precondition the caller is stating itself.
    #[prop(optional, into)]
    held: Signal<bool>,
    /// Run when the command landed. A panel that a command changed has to be
    /// told to read itself again; nothing else knows that it should.
    #[prop(optional, into)]
    then: Option<Callback<()>>,
) -> impl IntoView {
    let note = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let run = StoredValue::new(run);
    let go = move |_| {
        if busy.get_untracked() || held.get_untracked() || blocked.get_untracked().is_some() {
            return;
        }
        busy.set(true);
        note.set(None);
        let command = run.get_value();
        spawn_local(async move {
            let outcome = command().await;
            busy.set(false);
            match outcome {
                Ok(()) => {
                    if let Some(then) = then {
                        then.run(());
                    }
                }
                Err(reason) => note.set(Some(reason)),
            }
        });
    };
    view! {
        <span class="act">
            <button
                type="button"
                disabled=move || busy.get() || held.get() || blocked.get().is_some()
                on:click=go
            >
                {label}
            </button>
            <span class="note" data-state=move || note.get().map(|_| "invalid")>
                {move || blocked.get().or_else(|| note.get())}
            </span>
        </span>
    }
}

/// What a bundle shows when a command was refused.
///
/// `403` and `404` are one event, and the client says so in one word: an
/// operator who could tell "no such row" from "not yours" could enumerate the
/// deployment by asking. A `422` is the other case and says which field it was
/// about — none of these panels has two fields for one command, so the whole
/// complaint goes beside the one control there is.
#[must_use]
pub fn refusal(error: &rn_ui::ApiError) -> String {
    error.message()
}
