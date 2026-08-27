//! `/org/members` — the memberships on the organization's own party.
//!
//! An organization is its own root group, so this is `membership` rows whose
//! container is the organization, rendered by the same table and the same
//! panel a group's members are.
//!
//! The role control is under the table rather than inside it, and that is a
//! shape the table forces: a column produces text. Selecting a row opens the
//! panel, the select syncs the moment it changes, and a refusal — the last
//! owner cannot be demoted — is shown beside the select that asked for it.

use leptos::prelude::*;
use rn_api::MemberRole;
use rn_api::commands::{Leave, SetRole};
use rn_ui::{Column, Live, PageHead, Priority, SelectField, Table, sync_with};

use crate::bits::{Note, act, on, refusal};
use crate::rows::Member;

/// The organization's members.
#[component]
pub fn Members(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    view! {
        <PageHead title="Members" />
        <Roster container=org.clone() container_name="organization".to_owned() org=org />
    }
}

/// A container's membership: the table, and the panel for the selected row.
///
/// Used by this page for the organization and by the groups page for a group,
/// because "who is in this and what do they hold" is one screen either way.
#[component]
pub fn Roster(
    /// The container's public id — an organization or one of its groups.
    container: String,
    /// What the container is, for the empty sentence.
    container_name: String,
    /// The organization every query is scoped by.
    org: String,
) -> impl IntoView {
    let params: Vec<(&str, &str)> = if container == org {
        vec![("org", &org)]
    } else {
        vec![("org", &org), ("group", &container)]
    };
    let live = Live::<Member>::subscribe("org-members", &params);
    let chosen = RwSignal::new(None::<String>);

    let columns = vec![
        Column::new("Member", |row: &Member| row.display.clone()),
        Column::new("Role", |row: &Member| row.role.clone()),
        Column::new("Since", |row: &Member| on(row.since)).priority(Priority::Secondary),
        Column::new("Status", |row: &Member| row.status.clone()).priority(Priority::Tertiary),
        Column::new("Identifier", |row: &Member| row.public_id.clone())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let rows = Signal::derive(move || live.rows());
    let empty = format!("Nobody belongs to this {container_name} yet.");

    let selected = move || {
        let id = chosen.get()?;
        live.rows().into_iter().find(|row| row.public_id == id)
    };
    let container_for_panel = container.clone();

    view! {
        <Table
            rows=rows
            columns=columns
            empty=empty
            on_row=Callback::new(move |row: Member| chosen.set(Some(row.public_id)))
        />
        {move || {
            selected()
                .map(|member| {
                    view! { <Seat member=member container=container_for_panel.clone() /> }
                })
        }}
    }
}

/// One member's role, and the way out.
#[component]
fn Seat(member: Member, container: String) -> impl IntoView {
    let leaving = RwSignal::new(None::<String>);
    let role = Signal::derive({
        let role = member.role.clone();
        move || role.clone()
    });
    let last_owner = member.last_owner;
    let settable = member.settable;
    let me = member.me;

    let sync = {
        let container = container.clone();
        let party = member.public_id.clone();
        sync_with(move |wanted: String| {
            let container = container.clone();
            let party = party.clone();
            async move {
                let (Ok(group), Ok(party)) = (container.parse(), party.parse()) else {
                    return Err("That is not a public id.".to_owned());
                };
                let role = match wanted.as_str() {
                    "admin" => MemberRole::Admin,
                    "owner" => MemberRole::Owner,
                    _ => MemberRole::Member,
                };
                rn_ui::invoke::<SetRole, serde_json::Value>(SetRole { group, party, role })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        })
    };

    view! {
        <div class="panel">
            <h2>{member.display.clone()}</h2>
            <Show
                when=move || settable
                fallback=move || {
                    view! {
                        <p class="held">
                            {if last_owner {
                                "The only owner cannot be demoted."
                            } else {
                                "Not yours to change."
                            }}
                        </p>
                    }
                }
            >
                <SelectField
                    label="Role"
                    value=role
                    options=vec![
                        ("member".to_owned(), "member".to_owned()),
                        ("admin".to_owned(), "admin".to_owned()),
                    ]
                    sync=sync.clone()
                />
            </Show>
            <Show when=move || me>
                <button
                    type="button"
                    class="commit"
                    on:click={
                        let container = container.clone();
                        move |_| {
                            let Ok(group) = container.parse() else {
                                return;
                            };
                            act(leaving, Leave { group }, |_| ());
                        }
                    }
                >
                    "Leave"
                </button>
                <Note note=leaving />
            </Show>
        </div>
    }
}
