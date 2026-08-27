//! The small pieces every `/org` page uses.
//!
//! Nothing here decorates. A timestamp is rendered as a date because unix
//! seconds are not a fact a reader holds; a machine value gets a copy button
//! because anything a human must transfer by hand does; a rare or destructive
//! workflow gets a side affordance rather than a place in the page's flow.

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_ui::ApiError;

/// A date, from unix seconds. The clock is the browser's, which is the only
/// one a reader can check against.
#[must_use]
pub fn on(at: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(at as f64 * 1000.0));
    let text = date.to_locale_date_string("en-CA", &js_sys::Object::new());
    text.as_string().unwrap_or_default()
}

/// A date and a time, for rows where the order within a day is the point.
#[must_use]
pub fn at(instant: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(instant as f64 * 1000.0));
    let day = date
        .to_locale_date_string("en-CA", &js_sys::Object::new())
        .as_string()
        .unwrap_or_default();
    let time = date
        .to_locale_time_string("en-GB")
        .as_string()
        .unwrap_or_default();
    format!("{day} {time}")
}

/// What a refusal reads as beside the control that caused it.
///
/// One line, because an `/org` control is one control: a `422` naming three
/// fields is three sentences, and there is nowhere else on these panels to put
/// them.
#[must_use]
pub fn refusal(error: &ApiError) -> String {
    error.message()
}

/// A value a reader has to move by hand, with the button that moves it.
#[component]
pub fn Copyable(
    /// The text itself.
    #[prop(into)]
    value: Signal<String>,
    /// What the control is called, for a screen reader.
    #[prop(into)]
    label: String,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    view! {
        <span class="copyable">
            <code>{move || value.get()}</code>
            <button
                type="button"
                aria-label=label
                on:click=move |_| {
                    let text = value.get_untracked();
                    let _ = window().navigator().clipboard().write_text(&text);
                    copied.set(true);
                }
            >
                {move || if copied.get() { "Copied" } else { "Copy" }}
            </button>
        </span>
    }
}

/// A rare or destructive workflow, folded away until it is wanted.
#[component]
pub fn Aside(
    /// The summary line — the verb, not a description of the verb.
    #[prop(into)]
    title: String,
    /// The workflow.
    children: Children,
) -> impl IntoView {
    view! {
        <details class="aside">
            <summary>{title}</summary>
            <div class="aside-body">{children()}</div>
        </details>
    }
}

/// A line that holds its space, so a refusal appearing does not move the page.
#[component]
pub fn Note(
    /// What to say, or nothing.
    #[prop(into)]
    note: Signal<Option<String>>,
) -> impl IntoView {
    view! {
        <span class="note" data-state=move || note.get().map(|_| "invalid")>
            {move || note.get()}
        </span>
    }
}

/// Run a command, and put whatever it refused into `note`.
///
/// Every action on these pages is one of these: the command is the write, the
/// note beside the control is the whole of the feedback, and success says
/// nothing because the rows it moved are already live.
pub fn act<C, F>(note: RwSignal<Option<String>>, args: C, done: F)
where
    C: rn_api::Command + serde::Serialize + 'static,
    F: Fn(serde_json::Value) + 'static,
{
    spawn_local(async move {
        match rn_ui::invoke::<C, serde_json::Value>(args).await {
            Ok(committed) => {
                note.set(None);
                done(committed.result);
            }
            Err(error) => note.set(Some(refusal(&error))),
        }
    });
}

/// A `<select>` over a fixed vocabulary, for the panels a `Table` cell cannot
/// hold a control in.
#[component]
pub fn Choice(
    /// The label.
    #[prop(into)]
    label: String,
    /// The choices, as `(value, text)`.
    options: Vec<(&'static str, &'static str)>,
    /// The value in force.
    value: RwSignal<String>,
) -> impl IntoView {
    let options = options
        .into_iter()
        .map(|(option, text)| {
            let selected = move || value.get() == option;
            view! {
                <option value=option selected=selected>
                    {text}
                </option>
            }
        })
        .collect_view();
    view! {
        <label class="choice">
            <span>{label}</span>
            <select on:change=move |event| value.set(event_target_value(&event))>{options}</select>
        </label>
    }
}
