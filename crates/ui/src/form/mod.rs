//! Fields that sync themselves, and the one button that is not a save button.
//!
//! Owner doctrine, in full: a field the reader can type into is synced to the
//! server as they type — on blur or after three seconds of quiet — so their
//! half-finished edit is there on whatever device they pick up next. There is
//! no save button. A commit exists only for a strict state transition (draft →
//! published, draft → real account), and it carries the client's edit version
//! so the server knows which edit it is being asked to promote.

mod debounce;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::future::LocalBoxFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

pub use debounce::{DEBOUNCE_MS, Debounce, Phase};

/// What a field does with an edit: run a command, and answer with either
/// nothing or the validation to show beside the field.
pub type SyncCommand =
    Arc<dyn Fn(String) -> LocalBoxFuture<'static, Result<(), String>> + Send + Sync>;

/// Wrap an async closure as a [`SyncCommand`].
pub fn sync_with<F, Fut>(command: F) -> SyncCommand
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + 'static,
{
    Arc::new(move |value| Box::pin(command(value)) as LocalBoxFuture<'static, Result<(), String>>)
}

/// The browser clock, in milliseconds.
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

/// A field's live state: its draft, its debounce, and the command it syncs
/// through. Copy, so event handlers can take it by value.
#[derive(Clone, Copy)]
struct Engine {
    state: RwSignal<Debounce>,
    draft: RwSignal<String>,
    command: StoredValue<SyncCommand>,
}

impl Engine {
    fn new(initial: String, command: SyncCommand) -> Self {
        Self {
            state: RwSignal::new(Debounce::new()),
            draft: RwSignal::new(initial),
            command: StoredValue::new(command),
        }
    }

    fn edited(self, value: String) {
        self.draft.set(value);
        self.state.update(|state| state.edited(now_ms()));
        set_timeout(
            move || self.poll(),
            Duration::from_millis(DEBOUNCE_MS.saturating_add(10)),
        );
    }

    fn blurred(self) {
        if let Some(version) = self.state.try_update(|s| s.blurred(now_ms())).flatten() {
            self.fire(version);
        }
    }

    /// Send anything the debounce has now released.
    fn poll(self) {
        if let Some(version) = self.state.try_update(|s| s.elapsed(now_ms())).flatten() {
            self.fire(version);
        }
    }

    fn fire(self, version: u64) {
        let command = self.command.get_value();
        let value = self.draft.get_untracked();
        spawn_local(async move {
            let outcome = command(value).await;
            self.state.update(|state| match outcome {
                Ok(()) => state.accepted(now_ms(), version),
                Err(reason) => state.rejected(now_ms(), version, reason),
            });
            // An edit made while that was in flight is due immediately.
            set_timeout(move || self.poll(), Duration::ZERO);
        });
    }

    /// The validation to show, and whether it is one.
    fn note(self) -> Option<String> {
        self.state.with(|state| state.note().map(str::to_owned))
    }
}

/// Each field needs an id to tie its label to its control.
fn next_id(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("{prefix}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// A line of text that syncs itself.
#[component]
pub fn TextField(
    /// The label. A human word for the thing, not the column name.
    #[prop(into)]
    label: String,
    /// The value the server holds.
    #[prop(into)]
    value: Signal<String>,
    /// The command that carries an edit.
    sync: SyncCommand,
    /// The input type: `text` unless the value is an address or a secret.
    #[prop(default = "text")]
    kind: &'static str,
) -> impl IntoView {
    let engine = Engine::new(value.get_untracked(), sync);
    adopt(engine, value);
    let id = next_id("text");
    view! {
        <div class="field">
            <label for=id.clone()>{label}</label>
            <input
                id=id
                type=kind
                prop:value=move || engine.draft.get()
                on:input=move |event| engine.edited(event_target_value(&event))
                on:blur=move |_| engine.blurred()
            />
            <Note engine=engine />
        </div>
    }
}

/// A choice from a fixed set, syncing on change.
#[component]
pub fn SelectField(
    /// The label.
    #[prop(into)]
    label: String,
    /// The value the server holds.
    #[prop(into)]
    value: Signal<String>,
    /// The choices, as `(value, label)`.
    options: Vec<(String, String)>,
    /// The command that carries an edit.
    sync: SyncCommand,
) -> impl IntoView {
    let engine = Engine::new(value.get_untracked(), sync);
    adopt(engine, value);
    let id = next_id("select");
    let options = options
        .into_iter()
        .map(|(option, text)| {
            let selected = {
                let option = option.clone();
                move || engine.draft.get() == option
            };
            view! {
                <option value=option.clone() selected=selected>
                    {text}
                </option>
            }
        })
        .collect_view();
    view! {
        <div class="field">
            <label for=id.clone()>{label}</label>
            <select
                id=id
                on:change=move |event| {
                    engine.edited(event_target_value(&event));
                    engine.blurred();
                }
            >
                {options}
            </select>
            <Note engine=engine />
        </div>
    }
}

/// A two-state field. It syncs on the click; there is nothing to debounce.
#[component]
pub fn ToggleField(
    /// The label.
    #[prop(into)]
    label: String,
    /// The value the server holds.
    #[prop(into)]
    value: Signal<bool>,
    /// The command that carries an edit. It receives `"true"` or `"false"`.
    sync: SyncCommand,
) -> impl IntoView {
    let text = Signal::derive(move || value.get().to_string());
    let engine = Engine::new(text.get_untracked(), sync);
    adopt(engine, text);
    let id = next_id("toggle");
    view! {
        <div class="field toggle">
            <input
                id=id.clone()
                type="checkbox"
                prop:checked=move || engine.draft.get() == "true"
                on:change=move |event| {
                    engine.edited(event_target_checked(&event).to_string());
                    engine.blurred();
                }
            />
            <label for=id>{label}</label>
            <Note engine=engine />
        </div>
    }
}

/// The commit button: a state transition, tagged with the edit it promotes.
///
/// Not a save button. If a screen has one of these next to a form, the form is
/// wrong.
#[component]
pub fn Commit(
    /// The verb and its object: "Publish document", "Create account".
    #[prop(into)]
    label: String,
    /// The client's edit version this commit is tagged to.
    #[prop(into)]
    version: Signal<u64>,
    /// Run the transition.
    #[prop(into)]
    on_commit: Callback<u64>,
    /// Preconditions the caller has not met yet.
    #[prop(optional, into)]
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class="commit"
            disabled=move || disabled.get()
            on:click=move |_| on_commit.run(version.get())
        >
            {label}
        </button>
    }
}

/// The validation line. It holds its space so the layout does not jump when a
/// complaint appears.
#[component]
fn Note(engine: Engine) -> impl IntoView {
    let note = move || engine.note();
    let state = move || note().map(|_| "invalid");
    view! {
        <span class="note" data-state=state>
            {note}
        </span>
    }
}

/// Take the server's value while the field is idle. A diff that lands mid-edit
/// must not overwrite what the reader is typing.
fn adopt(engine: Engine, value: Signal<String>) {
    Effect::new(move |_| {
        let incoming = value.get();
        if engine.state.with_untracked(|state| state.phase()) == Phase::Idle {
            engine.draft.set(incoming);
        }
    });
}
