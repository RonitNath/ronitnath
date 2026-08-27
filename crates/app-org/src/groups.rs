//! `/org/groups` — the groups the organization owns, and what is inside one.
//!
//! A group is the unit three different things are written against: membership,
//! contact scoping (`person:P #contact @group:X`), and invitation links. So
//! the drill-in is those three, in that order, under the roster the members
//! page already renders.

use leptos::prelude::*;
use rn_api::commands::CreateGroup;
use rn_ui::{Column, Live, PageHead, Priority, Table};

use crate::bits::{Aside, Note, act, on};
use crate::members::Roster;
use crate::minting::Mint;
use crate::rows::{Contact, Group, Invitation};

/// The organization's groups.
#[component]
pub fn Groups(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let live = Live::<Group>::subscribe("org-groups", &[("org", &org)]);
    let chosen = RwSignal::new(None::<String>);
    let columns = vec![
        Column::new("Group", |row: &Group| row.display.clone()),
        Column::new("Members", |row: &Group| row.members.to_string()).mono(),
        Column::new("Created", |row: &Group| on(row.created_at)).priority(Priority::Secondary),
        Column::new("Identifier", |row: &Group| row.public_id.clone())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let rows = Signal::derive(move || live.rows());
    let selected = move || {
        let id = chosen.get()?;
        live.rows().into_iter().find(|row| row.public_id == id)
    };
    let for_panel = org.clone();

    view! {
        <PageHead title="Groups" />
        <NewGroup org=org.clone() />
        <Table
            rows=rows
            columns=columns
            empty="This organization owns no groups yet."
            on_row=Callback::new(move |row: Group| chosen.set(Some(row.public_id)))
        />
        {move || {
            selected()
                .map(|group| view! { <Inside group=group org=for_panel.clone() /> })
        }}
    }
}

/// Making one.
#[component]
fn NewGroup(org: String) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let note = RwSignal::new(None::<String>);
    view! {
        <div class="asides">
            <Aside title="New group">
                <label class="choice">
                    <span>"Name"</span>
                    <input
                        type="text"
                        prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event))
                    />
                </label>
                <button
                    type="button"
                    class="commit"
                    disabled=move || name.get().trim().is_empty()
                    on:click=move |_| {
                        let Ok(organization) = org.parse() else {
                            return;
                        };
                        act(
                            note,
                            CreateGroup {
                                display_name: name.get_untracked().trim().to_owned(),
                                organization: Some(organization),
                            },
                            move |_| name.set(String::new()),
                        );
                    }
                >
                    "Create group"
                </button>
                <Note note=note />
            </Aside>
        </div>
    }
}

/// One group: who is in it, whose contact details reach it, and the links that
/// let somebody join.
#[component]
fn Inside(group: Group, org: String) -> impl IntoView {
    view! {
        <div class="drill">
            <h2>{group.display.clone()}</h2>
            <Roster
                container=group.public_id.clone()
                container_name="group".to_owned()
                org=org.clone()
            />
            <Contacts group=group.public_id.clone() org=org.clone() />
            <Links group=group.public_id.clone() org=org />
        </div>
    }
}

/// Whose contact details are visible inside this group.
#[component]
fn Contacts(group: String, org: String) -> impl IntoView {
    let live = Live::<Contact>::subscribe("org-contacts", &[("org", &org), ("group", &group)]);
    let columns = vec![
        Column::new("Contact", |row: &Contact| row.display.clone()),
        Column::new("Status", |row: &Contact| row.status.clone()).priority(Priority::Secondary),
    ];
    let rows = Signal::derive(move || live.rows());
    view! {
        <section class="block">
            <h3>"Contacts"</h3>
            <Table
                rows=rows
                columns=columns
                empty="Nobody has scoped their contact details into this group."
                per_page=10
            />
        </section>
    }
}

/// The invitation links into this group.
#[component]
fn Links(group: String, org: String) -> impl IntoView {
    let live =
        Live::<Invitation>::subscribe("org-invitations", &[("org", &org), ("group", &group)]);
    let columns = vec![
        Column::new("Role", |row: &Invitation| row.role.clone()),
        Column::new("Minted", |row: &Invitation| on(row.created_at)),
        Column::new("Claimed by", |row: &Invitation| {
            row.claimed_by.clone().unwrap_or_default()
        })
        .priority(Priority::Secondary),
        Column::new("Expires", |row: &Invitation| on(row.expires_at)).priority(Priority::Tertiary),
    ];
    let rows = Signal::derive(move || live.rows());

    view! {
        <section class="block">
            <h3>"Invitations"</h3>
            <Mint container=group />
            <Table
                rows=rows
                columns=columns
                empty="No link has been minted into this group."
                per_page=10
            />
        </section>
    }
}
