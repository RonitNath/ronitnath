//! `/platform/parties/<id>` — everything the deployment knows about one human,
//! and the controls an operator has over them.
//!
//! A page rather than a panel, because the answer is nine lists and a panel
//! twenty-four rem wide would be nine lists in a column. Board frame **F3**.
//!
//! ## Addresses are masked
//!
//! A factor's `hint` is the masked local part — `r…t@` — and the value never
//! leaves the server. That is the rule `whoami` already applies to an
//! unresolved registration, applied here for the reason this page makes
//! sharper: an operator console that printed every email in the deployment is
//! a console that leaks the deployment the moment somebody screenshots it, and
//! this page is the one most likely to be screenshotted onto a support ticket.
//!
//! Revealing one is a control, not a toggle, and it writes its own audit row —
//! reading somebody's address is a thing the operator *did*. That command does
//! not exist in this build (`reveal-factor` is a kernel command and the kernel
//! is another leg's), so the control is absent rather than present and
//! refused.
//!
//! ## The cascade is named before it happens
//!
//! `Disable` deletes every session and token the party holds and puts their
//! outstanding invitations to sleep. The confirm reads `platform-cascade` and
//! names those four counts, as they were at the time of asking; the audit row
//! the command writes carries what it actually ended, and where the two differ
//! the row is the truth.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use rn_api::commands::{Disable, Enable, RevokeSession};

use crate::panel::{Act, Facts, Group, Line, State, refusal, run_with, when};
use crate::rows::{Cascade, Person};

#[component]
pub fn PersonPage() -> impl IntoView {
    let params = use_params_map();
    let found = RwSignal::new(None::<Person>);
    let cascade = RwSignal::new(None::<Cascade>);
    let missing = RwSignal::new(false);
    let revision = RwSignal::new(0u64);

    Effect::new(move |_| {
        revision.track();
        let Some(id) = params.read().get("id") else {
            return;
        };
        spawn_local(async move {
            let party = rn_ui::query::<Vec<Person>>("platform-party", &[("subject", &id)]).await;
            let counts =
                rn_ui::query::<Vec<Cascade>>("platform-cascade", &[("subject", &id)]).await;
            match party {
                Ok(mut rows) => {
                    missing.set(rows.is_empty());
                    found.set(rows.pop());
                }
                Err(_) => missing.set(true),
            }
            cascade.set(counts.ok().and_then(|mut rows| rows.pop()));
        });
    });
    let again = Callback::new(move |()| revision.update(|r| *r += 1));

    view! {
        {move || match (found.get(), missing.get()) {
            (Some(person), _) => {
                view! { <Detail person=person cascade=cascade then=again /> }.into_any()
            }
            (None, true) => {
                view! { <p class="none">"No party on this deployment has that id."</p> }.into_any()
            }
            (None, false) => view! { <p class="none">"Reading."</p> }.into_any(),
        }}
    }
}

#[component]
fn Detail(person: Person, cascade: RwSignal<Option<Cascade>>, then: Callback<()>) -> impl IntoView {
    let head = vec![
        (
            "Handle",
            person
                .handle
                .clone()
                .unwrap_or_else(|| "\u{2014}".to_owned()),
        ),
        ("Kind", person.kind.clone()),
        ("Registered", when(person.created_at)),
        ("Id", person.public_id.to_string()),
    ];

    let identities = person
        .identities
        .clone()
        .into_iter()
        .map(|identity| {
            let status = identity.status.clone();
            view! {
                <Line lead=identity.public_id.to_string() trail=identity.source.clone()>
                    <State value=status />
                </Line>
            }
        })
        .collect_view();

    // The mask is the whole content of the lead: `r…t@` is enough to match a
    // support call against a row and not enough to write to it. A factor with
    // no half worth showing — a password's hash, a passkey's credential — has
    // a dash there, and says so rather than repeating its own kind twice.
    //
    // The verified word is only shown for a kind that *can* be verified. A
    // password has no `verified_at` and never will, so calling it unverified
    // would be a word about something the deployment never witnessed.
    let factors = person
        .factors
        .clone()
        .into_iter()
        .map(|factor| {
            let verifiable = factor.kind == "email";
            let word = if factor.verified {
                "verified"
            } else {
                "unverified"
            };
            view! {
                <Line
                    lead=factor.hint.clone().unwrap_or_else(|| "\u{2014}".to_owned())
                    trail=factor.kind.clone()
                >
                    <Show when=move || verifiable>
                        <State value=word />
                    </Show>
                </Line>
            }
        })
        .collect_view();

    let sessions = person
        .sessions
        .clone()
        .into_iter()
        .map(|session| {
            let id = session.public_id.clone();
            let wearing = session.impersonated_by.is_some();
            let run = run_with(move || {
                let session = id.clone();
                async move {
                    rn_ui::invoke::<RevokeSession, serde_json::Value>(RevokeSession { session })
                        .await
                        .map(|_| ())
                        .map_err(|error| refusal(&error))
                }
            });
            view! {
                <Line
                    lead=session.acting_display.clone()
                    trail=format!(
                        "{} \u{00b7} expires {}",
                        when(session.last_seen_at),
                        when(session.expires_at),
                    )
                >
                    <Show when=move || wearing>
                        <State value="impersonated" />
                    </Show>
                    <Act label="Revoke" run=run.clone() then=then />
                </Line>
            }
        })
        .collect_view();

    let memberships = person
        .memberships
        .clone()
        .into_iter()
        .map(|member| {
            view! {
                <Line lead=member.display.clone() trail=member.kind.clone()>
                    <span class="role">{member.role.clone()}</span>
                </Line>
            }
        })
        .collect_view();

    let resources = person
        .resources
        .clone()
        .into_iter()
        .map(|resource| {
            let status = resource.status.clone();
            view! {
                <Line lead=resource.public_id.to_string() trail=resource.kind.clone()>
                    <State value=status />
                </Line>
            }
        })
        .collect_view();

    let relations = person
        .relations
        .clone()
        .into_iter()
        .map(|held| {
            let object = held
                .object
                .as_ref()
                .map_or_else(|| held.object_kind.clone(), ToString::to_string);
            view! {
                <Line lead=object trail=held.object_kind.clone()>
                    <span class="role">{held.relation.clone()}</span>
                </Line>
            }
        })
        .collect_view();

    let consents = person
        .consents
        .clone()
        .into_iter()
        .map(|consent| {
            view! {
                <Line
                    lead=consent.client_name.clone()
                    trail=format!("{} \u{00b7} {}", consent.scopes, when(consent.at))
                />
            }
        })
        .collect_view();

    // Append-only, so this is the whole history: every ruling that ever
    // attached a registration to this person, with what the operator relied on.
    let merges = person
        .merges
        .clone()
        .into_iter()
        .map(|merge| {
            let evidence = merge.evidence.clone();
            let method = merge.method.clone();
            view! {
                <div class="line link">
                    <span class="line-lead mono">{merge.identity.to_string()}</span>
                    <span class="line-trail">
                        {format!(
                            "{} \u{00b7} {}",
                            merge.asserted_display.clone().unwrap_or_else(|| "\u{2014}".to_owned()),
                            when(merge.at),
                        )}
                    </span>
                    <State value=method />
                    {evidence.map(|evidence| view! { <p class="evidence">{evidence}</p> })}
                </div>
            }
        })
        .collect_view();

    let aliases = person
        .aliases
        .clone()
        .into_iter()
        .map(|alias| {
            view! { <Line lead=alias.was.clone() trail=format!("{} \u{00b7} {}", alias.kind, when(alias.at)) /> }
        })
        .collect_view();

    let counts = (
        person.identities.len(),
        person.factors.len(),
        person.sessions.len(),
        person.memberships.len(),
        person.resources.len(),
        person.relations.len(),
        person.consents.len(),
        person.merges.len(),
        person.aliases.len(),
    );

    view! {
        <div class="page-head">
            <h1>{person.display.clone()}</h1>
            <State value=person.status.clone() />
            {person.handle.clone().map(|handle| view! { <span class="handle mono">{format!("@{handle}")}</span> })}
        </div>
        <Status person=person.clone() cascade=cascade then=then />
        <Facts facts=head />
        <div class="person">
            <Group label="Identities" count=counts.0 empty="No registration resolves to this person.">
                {identities}
            </Group>
            <Group label="Factors" count=counts.1 empty="They have proven nothing.">
                {factors}
            </Group>
            <Group label="Sessions" count=counts.2 empty="They are not signed in anywhere.">
                {sessions}
            </Group>
            <Group label="Memberships" count=counts.3 empty="They belong to no organization or group.">
                {memberships}
            </Group>
            <Group label="Owns" count=counts.4 empty="They own no registered resource.">
                {resources}
            </Group>
            <Group label="Granted" count=counts.5 empty="They have been granted nothing.">
                {relations}
            </Group>
            <Group label="Consents" count=counts.6 empty="They have authorised no relying party.">
                {consents}
            </Group>
            <Group label="Merge history" count=counts.7 empty="No ruling has ever attached a registration to them.">
                {merges}
            </Group>
            <Group label="Absorbed names" count=counts.8 empty="No id or handle resolves here from somewhere else.">
                {aliases}
            </Group>
        </div>
    }
}

/// Disable and enable, and the four counts a disable would end.
#[component]
fn Status(person: Person, cascade: RwSignal<Option<Cascade>>, then: Callback<()>) -> impl IntoView {
    let reason = RwSignal::new(String::new());
    let disabled = person.status == "disabled";
    let id = person.public_id.clone();

    let blocked = Signal::derive(move || {
        if disabled || !reason.get().trim().is_empty() {
            None
        } else {
            Some("A reason is required: it lands on the audit row.".to_owned())
        }
    });

    let enable_id = id.clone();
    let run = if disabled {
        run_with(move || {
            let party = enable_id.clone();
            async move {
                rn_ui::invoke::<Enable, serde_json::Value>(Enable { party })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    } else {
        run_with(move || {
            let party = id.clone();
            let reason = reason.get_untracked().trim().to_owned();
            async move {
                rn_ui::invoke::<Disable, serde_json::Value>(Disable { party, reason })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    };

    view! {
        <section class="group controls">
            <div class="actions">
                <Show when=move || !disabled>
                    <input
                        type="text"
                        class="reason"
                        aria-label="Reason"
                        placeholder="Reason"
                        prop:value=move || reason.get()
                        on:input=move |event| reason.set(event_target_value(&event))
                    />
                </Show>
                <Act
                    label=if disabled { "Enable" } else { "Disable" }
                    run=run.clone()
                    blocked=blocked
                    then=then
                />
            </div>
            <Show when=move || !disabled>
                {move || {
                    cascade
                        .get()
                        .map(|counts| {
                            view! {
                                <p class="none">
                                    {format!(
                                        "Ends {} session{}, {} token{}, {} invitation{} and leaves {} consent{} standing.",
                                        counts.sessions,
                                        plural(counts.sessions),
                                        counts.tokens,
                                        plural(counts.tokens),
                                        counts.links,
                                        plural(counts.links),
                                        counts.consents,
                                        plural(counts.consents),
                                    )}
                                </p>
                            }
                        })
                }}
            </Show>
        </section>
    }
}

/// The plural `s`, or nothing. One session is a session.
fn plural(count: i64) -> &'static str {
    if count == 1 { "" } else { "s" }
}
