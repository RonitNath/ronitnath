//! A body of text that syncs itself.
//!
//! `rn_ui::TextField` is this for one line, and a document's body is not one
//! line. What it is *not* is a second editing doctrine: the debounce is
//! `rn_ui::form::Debounce`, so this textarea releases an edit on exactly the
//! same terms a field does — on blur, or after three seconds of quiet — and a
//! diff that lands mid-edit is adopted only while the reader is idle.
//!
//! The gap this fills is rn-ui's: a `TextArea` beside `TextField` would leave
//! nothing here but a `rows` attribute.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_ui::form::{DEBOUNCE_MS, Debounce, Phase, SyncCommand};

#[component]
pub fn Words(
    /// The text the server holds.
    #[prop(into)]
    value: Signal<String>,
    /// The command that carries an edit.
    sync: SyncCommand,
    /// Whether this reader may change it.
    #[prop(optional)]
    readonly: bool,
) -> impl IntoView {
    let state = RwSignal::new(Debounce::new());
    let draft = RwSignal::new(value.get_untracked());
    let command = StoredValue::new(sync);

    let fire = move |version: u64| {
        let run = command.get_value();
        let text = draft.get_untracked();
        spawn_local(async move {
            let outcome = run(text).await;
            state.update(|state| match outcome {
                Ok(()) => state.accepted(now_ms(), version),
                Err(reason) => state.rejected(now_ms(), version, reason),
            });
        });
    };

    let poll = move || {
        if let Some(version) = state.try_update(|state| state.elapsed(now_ms())).flatten() {
            fire(version);
        }
    };

    let edited = move |event: leptos::ev::Event| {
        draft.set(event_target_value(&event));
        state.update(|state| state.edited(now_ms()));
        set_timeout(poll, Duration::from_millis(DEBOUNCE_MS.saturating_add(10)));
    };

    let blurred = move |_| {
        if let Some(version) = state.try_update(|state| state.blurred(now_ms())).flatten() {
            fire(version);
        }
    };

    // The server's text is taken while the reader is idle and never while they
    // are typing: a diff must not overwrite what is under the cursor.
    Effect::new(move |_| {
        let incoming = value.get();
        if state.with_untracked(|state| state.phase()) == Phase::Idle {
            draft.set(incoming);
        }
    });

    let note = move || state.with(|state| state.note().map(str::to_owned));
    view! {
        <div class="field">
            <label for="document-body">"Body"</label>
            <textarea
                id="document-body"
                readonly=readonly
                prop:value=move || draft.get()
                on:input=edited
                on:blur=blurred
            ></textarea>
            <span class="note" data-state=move || note().map(|_| "invalid")>
                {note}
            </span>
        </div>
    }
}

fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}
