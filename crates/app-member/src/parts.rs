//! The small shapes every member page is built from.
//!
//! Nothing here is a component the other bundles would want — those live in
//! `rn_ui`. These are the four the member tier repeats: a titled block, a
//! label/value list, a word that runs a command, and a string a human has to
//! carry somewhere else by hand.

use leptos::prelude::*;

use crate::api::Refusal;

/// One block of a page. The heading is a word for the block, never a sentence
/// about it.
#[component]
pub fn Section(
    /// What the block is.
    #[prop(into)]
    title: String,
    /// Whether it spans the whole content column rather than sharing the row.
    #[prop(optional)]
    wide: bool,
    /// The block.
    children: Children,
) -> impl IntoView {
    let class = if wide { "section wide" } else { "section" };
    view! {
        <section class=class>
            <h2>{title}</h2>
            {children()}
        </section>
    }
}

/// A label and its value.
#[component]
pub fn Pair(
    /// The label.
    #[prop(into)]
    label: String,
    /// The value.
    children: Children,
) -> impl IntoView {
    view! {
        <dt>{label}</dt>
        <dd>{children()}</dd>
    }
}

/// A word that runs a command.
///
/// Not a `commit` — a commit is a state transition and wears the accent block.
/// These are the row-level verbs: revoke, leave, remove, split.
#[component]
pub fn Act(
    /// The verb and its object.
    #[prop(into)]
    label: String,
    /// What it does.
    #[prop(into)]
    on_act: Callback<()>,
    /// Whether it takes something away, which the ink says.
    #[prop(optional)]
    undo: bool,
    /// Preconditions the caller has not met.
    #[prop(optional, into)]
    disabled: Signal<bool>,
) -> impl IntoView {
    let weight = undo.then_some("undo");
    view! {
        <button
            type="button"
            class="act"
            data-weight=weight
            disabled=move || disabled.get()
            on:click=move |_| on_act.run(())
        >
            {label}
        </button>
    }
}

/// A string somebody has to move by hand, with the button that moves it.
#[component]
pub fn Carry(
    /// The string.
    #[prop(into)]
    value: Signal<String>,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    let copy = move |_| {
        if let Some(clipboard) = web_sys::window().map(|window| window.navigator().clipboard()) {
            let _ = clipboard.write_text(&value.get_untracked());
            copied.set(true);
        }
    };
    view! {
        <div class="carry">
            <code>{move || value.get()}</code>
            <button type="button" class="act" on:click=copy>
                {move || if copied.get() { "Copied" } else { "Copy" }}
            </button>
        </div>
    }
}

/// The refusal a control produced, in the place the control is.
#[component]
pub fn Note(
    /// What the last attempt refused with.
    #[prop(into)]
    refusal: Signal<Option<Refusal>>,
    /// Show only what the server said about this field, if it named one.
    ///
    /// A signal rather than a string, because one control can stand for two
    /// fields: the factor form's single input carries an address or a secret,
    /// and which one the server will name follows the kind beside it.
    #[prop(optional, into)]
    field: Option<Signal<String>>,
) -> impl IntoView {
    let text = move || {
        let refusal = refusal.get()?;
        match &field {
            Some(field) => refusal.about(&field.get()),
            None => Some(refusal.message()),
        }
    };
    let state = move || text().map(|_| "invalid");
    view! {
        <span class="note" data-state=state>
            {text}
        </span>
    }
}

/// A unix second, as a person reads it.
///
/// Local time, because every reader of this page is looking at their own
/// devices and their own documents, and the only useful answer to "when" is
/// theirs. Minutes, not seconds: nothing on these pages turns on a second.
#[must_use]
pub fn when(at: i64) -> String {
    if at <= 0 {
        return String::new();
    }
    let date = js_sys::Date::new(&(((at as f64) * 1000.0).into()));
    let pad = |n: u32| format!("{n:02}");
    format!(
        "{}-{}-{} {}:{}",
        date.get_full_year(),
        pad(date.get_month() + 1),
        pad(date.get_date()),
        pad(date.get_hours()),
        pad(date.get_minutes()),
    )
}

/// A word for a relation or a role, as a page says it.
#[must_use]
pub fn titled(raw: &str) -> String {
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
