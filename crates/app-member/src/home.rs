//! `/app` — who you are here, and what you have just done.

use leptos::prelude::*;
use rn_ui::{Column, Live, PageHead, Priority, Table, use_whoami};

use crate::parts::{Pair, Section, titled, when};
use crate::rows::Audited;

#[component]
pub fn Home() -> impl IntoView {
    let who = use_whoami();
    let audit = Live::<Audited>::subscribe("audit", &[]);

    // Who you are leads. The registration you signed in with is a second line
    // and only when it says something the name does not — a resolved identity
    // is named by its person, so repeating it would be the same words twice.
    let identity = move || {
        who.get().map(|whoami| {
            let name = whoami.person.as_ref().map_or_else(
                || whoami.identity.display.clone(),
                |person| person.display.clone(),
            );
            let registration =
                (whoami.identity.display != name).then_some(whoami.identity.display.clone());
            let person = whoami.person.map(|person| person.public_id.to_string());
            view! {
                <dl class="kv">
                    <Pair label="Name">{name}</Pair>
                    {registration
                        .map(|display| view! { <Pair label="Registration">{display}</Pair> })}
                    {person
                        .map(|id| {
                            view! {
                                <Pair label="Person">
                                    <span class="mono">{id}</span>
                                </Pair>
                            }
                        })}
                    <Pair label="Session ends">{when(whoami.session_expires_at)}</Pair>
                </dl>
            }
        })
    };

    let organizations = move || {
        who.get()
            .map(|whoami| {
                if whoami.organizations.is_empty() {
                    return view! { <p class="quiet">"You belong to no organization."</p> }
                        .into_any();
                }
                whoami
                    .organizations
                    .clone()
                    .into_iter()
                    .map(|org| {
                        let role = format!("{:?}", org.role).to_lowercase();
                        view! {
                            <dl class="kv">
                                <Pair label=titled(&role)>{org.display}</Pair>
                            </dl>
                        }
                    })
                    .collect_view()
                    .into_any()
            })
            .into_any()
    };

    let rows = Signal::derive(move || {
        let mut rows = audit.rows();
        rows.reverse();
        rows
    });

    view! {
        <PageHead title="Home" />
        <div class="sections">
            <Section title="You">{identity}</Section>
            <Section title="Organizations">{organizations}</Section>
            <Section title="Your commands" wide=true>
                <Table
                    rows=rows
                    columns=vec![
                        Column::new("When", |row: &Audited| when(row.at)).mono(),
                        Column::new("Command", |row: &Audited| row.command.clone()),
                        Column::new("Offset", |row: &Audited| row.offset.to_string())
                            .mono()
                            .priority(Priority::Secondary),
                    ]
                    empty="Nothing yet. Every command you run lands here."
                    per_page=15
                />
            </Section>
        </div>
    }
}
