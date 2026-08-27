//! `/app/identities` — the registrations resolved onto this person, and the
//! factors each one has proven.
//!
//! A factor's value never crosses the wire in either direction *back*: this
//! page can add one and remove one, and it can never read one. What it shows
//! is the kind and whether it is proven, which is the whole of what a person
//! needs to know about their own credentials.

use leptos::prelude::*;
use rn_api::commands::{AddFactor, Disable, FactorKind, RemoveFactor, VerifyEmail};
use rn_ui::{Commit, Live, PageHead, use_whoami};

use crate::api::{Refusal, attempt};
use crate::parts::{Act, Note, Pair, Section, titled, when};
use crate::rows::{Factor, Identity};

#[component]
pub fn Identities() -> impl IntoView {
    let identities = Live::<Identity>::subscribe("identities", &[]);
    let refusal = RwSignal::new(None::<Refusal>);

    let blocks = move || {
        identities
            .rows()
            .into_iter()
            .map(|row| view! { <Registration row=row refusal=refusal /> })
            .collect_view()
    };

    view! {
        <PageHead title="Identities" />
        <div class="sections">
            {blocks}
            <Adding refusal=refusal />
            <Proving refusal=refusal />
            <Leaving refusal=refusal />
        </div>
    }
}

/// One registration: where it came from, and what it has proven.
#[component]
fn Registration(row: Identity, refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let title = if row.current {
        "This registration"
    } else {
        "Registration"
    };
    let identity = row.public_id.clone();
    let factors = row
        .factors
        .iter()
        .cloned()
        .map(|factor| {
            let identity = identity.clone();
            view! { <Proven factor=factor identity=identity refusal=refusal /> }
        })
        .collect_view();

    view! {
        <Section title=title>
            <dl class="kv">
                <Pair label="Source">{row.source.clone()}</Pair>
                <Pair label="Home zone">{row.home_zone.clone()}</Pair>
                <Pair label="Status">{titled(&row.status)}</Pair>
                <Pair label="Registered">{when(row.created_at)}</Pair>
                <Pair label="Identity">
                    <span class="mono">{row.public_id.clone()}</span>
                </Pair>
            </dl>
            <dl class="kv">{factors}</dl>
        </Section>
    }
}

/// One factor, and the word that takes it away.
#[component]
fn Proven(factor: Factor, identity: String, refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let id = factor.public_id.clone();
    let owner = identity.clone();
    let remove = Callback::new(move |()| {
        let (Ok(factor), Ok(identity)) = (id.parse(), owner.parse()) else {
            return;
        };
        attempt(
            RemoveFactor {
                factor,
                identity: Some(identity),
            },
            refusal,
            |_| {},
        );
    });
    let state = if factor.verified {
        "verified"
    } else {
        "unverified"
    };
    view! {
        <Pair label=titled(&factor.kind)>
            <span class="actions">
                <span class="quiet">{state}</span>
                <Act label="Remove" undo=true on_act=remove />
            </span>
        </Pair>
    }
}

/// Adding a factor. A creation, so it commits rather than syncing: half an
/// address is not a credential anybody wants stored.
#[component]
fn Adding(refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let kind = RwSignal::new("email".to_owned());
    let value = RwSignal::new(String::new());

    let add = Callback::new(move |_: u64| {
        let kind = match kind.get_untracked().as_str() {
            "password" => FactorKind::Password,
            _ => FactorKind::Email,
        };
        attempt(
            AddFactor {
                kind,
                value: value.get_untracked(),
                identity: None,
            },
            refusal,
            move |_| value.set(String::new()),
        );
    });

    view! {
        <Section title="Add a factor">
            <div class="inline">
                <div class="field">
                    <label for="factor-kind">"Kind"</label>
                    <select
                        id="factor-kind"
                        on:change=move |event| kind.set(event_target_value(&event))
                    >
                        <option value="email">"Email"</option>
                        <option value="password">"Password"</option>
                    </select>
                </div>
                <div class="field">
                    <label for="factor-value">"Address or password"</label>
                    <input
                        id="factor-value"
                        type="text"
                        prop:value=move || value.get()
                        on:input=move |event| value.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="value" />
                </div>
                <Commit label="Add factor" version=Signal::derive(|| 0) on_commit=add />
            </div>
        </Section>
    }
}

/// Proving an address, with the token the message carried.
#[component]
fn Proving(refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let token = RwSignal::new(String::new());
    let verify = Callback::new(move |_: u64| {
        attempt(
            VerifyEmail {
                token: token.get_untracked(),
            },
            refusal,
            move |_| token.set(String::new()),
        );
    });
    view! {
        <Section title="Verify an address">
            <div class="inline">
                <div class="field">
                    <label for="verify-token">"Token"</label>
                    <input
                        id="verify-token"
                        type="text"
                        prop:value=move || token.get()
                        on:input=move |event| token.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="token" />
                </div>
                <Commit label="Verify" version=Signal::derive(|| 0) on_commit=verify />
            </div>
        </Section>
    }
}

/// Disabling yourself. Reversible by an operator, and it ends every session —
/// including the one reading this — so the browser leaves with it.
#[component]
fn Leaving(refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let who = use_whoami();
    let reason = RwSignal::new(String::new());
    let disable = Callback::new(move |_: u64| {
        let Some(person) = who.get_untracked().and_then(|whoami| whoami.person) else {
            return;
        };
        attempt(
            Disable {
                party: person.public_id,
                reason: reason.get_untracked(),
            },
            refusal,
            move |_| {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().assign("/auth");
                }
            },
        );
    });
    view! {
        <Section title="Disable this person">
            <div class="inline">
                <div class="field">
                    <label for="disable-reason">"Reason"</label>
                    <input
                        id="disable-reason"
                        type="text"
                        prop:value=move || reason.get()
                        on:input=move |event| reason.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="reason" />
                </div>
                <Commit
                    label="Disable"
                    version=Signal::derive(|| 0)
                    on_commit=disable
                    disabled=Signal::derive(move || reason.get().trim().is_empty())
                />
            </div>
        </Section>
    }
}
