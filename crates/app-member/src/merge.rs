//! `/app/merge` — the pairs a signal has proposed about this person's own
//! registrations, and the proof only this person can give.
//!
//! A signal proposes; proof disposes (`docs/kernel/index.html` §Merge is first
//! class). The proof this page can offer is the self-link one: holding a live
//! session on both registrations at once, which is the same evidence as
//! signing in twice.
//!
//! It is not complete in this build, and the missing half is the server's.
//! `ConfirmMatch` takes the *other* registration's session token, and nothing
//! hands a browser one: `SignIn` puts its token in an `HttpOnly` cookie —
//! replacing the caller's own — and returns only the identity it minted for.
//! So the form below signs in to the other registration and offers the proof;
//! where both registrations already resolve to one person, or share a verified
//! factor, it lands, and otherwise it declines and says so. Closing it needs a
//! reply that carries the second session, which is a change to the command
//! surface and not to this page.

use leptos::prelude::*;
use rn_api::commands::{ConfirmMatch, SignIn, Split};
use rn_ui::{Commit, Live, PageHead, use_whoami};

use crate::api::{Refusal, attempt, run};
use crate::parts::{Act, Note, Pair, Section, when};
use crate::rows::{Candidate, Identity};

#[component]
pub fn Merge() -> impl IntoView {
    let candidates = Live::<Candidate>::subscribe("matches", &[]);
    let identities = Live::<Identity>::subscribe("identities", &[]);
    let who = use_whoami();
    let refusal = RwSignal::new(None::<Refusal>);

    let queue = move || {
        let rows = candidates.rows();
        if rows.is_empty() {
            return view! { <p class="quiet">"Nothing is proposed about you."</p> }.into_any();
        }
        rows.into_iter()
            .map(|row| view! { <Proposed row=row refusal=refusal /> })
            .collect_view()
            .into_any()
    };

    let person = move || {
        who.get().and_then(|whoami| whoami.person).map(|person| {
            view! {
                <dl class="kv">
                    <Pair label="Person">{person.display.clone()}</Pair>
                    <Pair label="Id">
                        <span class="mono">{person.public_id.to_string()}</span>
                    </Pair>
                </dl>
            }
        })
    };

    let made_of = move || {
        let rows = identities.rows();
        let splittable = rows.len() > 1;
        rows.into_iter()
            .map(|row| {
                let id = row.public_id.clone();
                let split = Callback::new(move |()| {
                    let Ok(identity) = id.parse() else {
                        return;
                    };
                    attempt(
                        Split {
                            identity,
                            evidence: "detached by the person themselves".to_owned(),
                        },
                        refusal,
                        |_| {},
                    );
                });
                view! {
                    <Pair label=row.source.clone()>
                        <span class="actions">
                            <span class="mono">{row.public_id.clone()}</span>
                            <Act
                                label="Split off"
                                undo=true
                                on_act=split
                                disabled=Signal::derive(move || !splittable)
                            />
                        </span>
                    </Pair>
                }
            })
            .collect_view()
    };

    view! {
        <PageHead title="Merge" />
        <div class="sections">
            <Section title="Proposed" wide=true>{queue}</Section>
            <Section title="You are">{person}</Section>
            <Section title="Made of">
                <dl class="kv">{made_of}</dl>
            </Section>
        </div>
        <Note refusal=refusal />
    }
}

/// One proposed pair, and the proof form that would confirm it.
#[component]
fn Proposed(row: Candidate, refusal: RwSignal<Option<Refusal>>) -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let candidate = row.public_id.clone();

    let confirm = Callback::new(move |_: u64| {
        let candidate = candidate.clone();
        let (address, secret) = (email.get_untracked(), password.get_untracked());
        leptos::task::spawn_local(async move {
            let Ok(candidate) = candidate.parse() else {
                return;
            };
            // Signing in to the other registration is the proof. The token it
            // mints goes into the cookie and not into this reply, so what is
            // sent below is the candidate alone, and the kernel falls back to
            // the factor both registrations have verified.
            if !address.is_empty() {
                let _ = run(&SignIn {
                    email: address,
                    password: secret,
                })
                .await;
            }
            match run(&ConfirmMatch {
                candidate,
                other_session: None,
            })
            .await
            {
                Ok(_) => refusal.set(None),
                Err(refused) => refusal.set(Some(refused)),
            }
        });
    });

    let signal = row.signal.replace('_', " ");
    view! {
        <div class="section">
            <dl class="kv">
                <Pair label="Signal">{signal}</Pair>
                <Pair label="Score">
                    <span class="figure">{format!("{:.2}", row.score)}</span>
                </Pair>
                <Pair label="Other registration">
                    {row.display.clone().unwrap_or_else(|| row.other.clone())}
                </Pair>
                <Pair label="Proposed">{when(row.created_at)}</Pair>
            </dl>
            <div class="inline">
                <div class="field">
                    <label for="merge-email">"The other registration's address"</label>
                    <input
                        id="merge-email"
                        type="email"
                        prop:value=move || email.get()
                        on:input=move |event| email.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="email" />
                </div>
                <div class="field">
                    <label for="merge-password">"Its password"</label>
                    <input
                        id="merge-password"
                        type="password"
                        prop:value=move || password.get()
                        on:input=move |event| password.set(event_target_value(&event))
                    />
                    <Note refusal=refusal field="password" />
                </div>
                <Commit label="Confirm" version=Signal::derive(|| 0) on_commit=confirm />
            </div>
        </div>
    }
}
