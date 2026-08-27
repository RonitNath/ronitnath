//! `/platform/resources` — the resource registry, and what is held on one.
//!
//! Every ownable thing has a row here and the kind tables hang off it 1:1, so
//! this is the one list from which "who owns what" is answerable without
//! asking each product.
//!
//! The drill-in reads the relation store from the *object* side — every
//! subject that holds anything on this resource — which is the opposite
//! direction to `check()`. Revoking takes exactly the row that was written and
//! no other path the subject may also hold: somebody who also reaches the
//! resource through a group keeps that path, because that grant is a different
//! decision made by somebody who is not being asked here.

use leptos::prelude::*;
use rn_api::commands::{Revoke, Transfer};
use rn_api::whoami::DocRole;
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Panel, State, drill, refusal, run_with, when};
use crate::rows::{GrantedRelation, Resource, ResourceDetail};

/// The relations a `Revoke` can name. `owner` is not among them: ownership
/// moves through `Transfer` and nowhere else.
fn revocable(relation: &str) -> Option<DocRole> {
    match relation {
        "viewer" => Some(DocRole::Viewer),
        "commenter" => Some(DocRole::Commenter),
        "editor" => Some(DocRole::Editor),
        "contact" => Some(DocRole::Contact),
        _ => None,
    }
}

#[component]
pub fn Resources() -> impl IntoView {
    let live = rn_ui::Live::<Resource>::subscribe("platform-resources", &[]);
    let selected = RwSignal::new(None::<String>);
    let detail = drill::<ResourceDetail>("platform-resource", selected);

    let rows = Signal::derive(move || live.rows());
    let columns = vec![
        Column::new("Kind", |row: &Resource| row.kind.clone()),
        Column::new("Owner", |row: &Resource| {
            format!("{} ({})", row.owner_display, row.owner_kind)
        }),
        Column::new("Status", |row: &Resource| row.status.clone()),
        Column::new("Zone", |row: &Resource| row.home_zone.clone()).priority(Priority::Secondary),
        Column::new("Created", |row: &Resource| when(row.created_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Resource| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Resource| {
        selected.set(Some(row.public_id.to_string()));
    });

    view! {
        <PageHead title="Resources">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="Nothing ownable has been created on this deployment."
                    on_row=open
                />
            </div>
            <Show when=move || detail.get().is_some()>
                {move || {
                    detail
                        .get()
                        .map(|resource| {
                            view! { <Detail resource=resource selected=selected then=detail.refresh() /> }
                        })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(
    resource: ResourceDetail,
    selected: RwSignal<Option<String>>,
    then: Callback<()>,
) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let facts = vec![
        ("Kind", resource.kind.clone()),
        (
            "Owner",
            format!("{} ({})", resource.owner_display, resource.owner_kind),
        ),
        (
            "Owner id",
            resource
                .owner
                .as_ref()
                .map_or_else(|| "\u{2014}".to_owned(), ToString::to_string),
        ),
        ("Zone", resource.home_zone.clone()),
        ("Created", when(resource.created_at)),
        ("Id", resource.public_id.to_string()),
    ];
    let id = resource.public_id.to_string();
    let grants = resource
        .relations
        .clone()
        .into_iter()
        .map(|granted| {
            view! { <Grant resource=id.clone() granted=granted.clone() then=then /> }
        })
        .collect_view();
    let count = resource.relations.len();

    view! {
        <Panel title=resource.public_id.to_string() on_close=close>
            <div class="panel-state">
                <State value=resource.status.clone() />
            </div>
            <Facts facts=facts />
            <Group
                label="Relations"
                count=count
                empty="Nobody holds anything on it but its owner, and ownership is a column."
            >
                {grants}
            </Group>
            <Transferrer resource=resource.public_id.to_string() then=then />
        </Panel>
    }
}

/// One grant, with the one thing that can be done to it.
#[component]
fn Grant(resource: String, granted: GrantedRelation, then: Callback<()>) -> impl IntoView {
    let subject = granted
        .subject
        .as_ref()
        .map_or_else(|| granted.subject_kind.clone(), ToString::to_string);
    let role = revocable(&granted.relation);
    let blocked = Signal::derive({
        let relation = granted.relation.clone();
        let named = granted.subject.is_some();
        move || {
            if role.is_none() {
                Some(format!("`{relation}` is not withdrawn by Revoke."))
            } else if !named {
                Some("This subject has no id to name.".to_owned())
            } else {
                None
            }
        }
    });
    let run = run_with({
        let resource = resource.clone();
        let subject = granted.subject.clone();
        move || {
            let resource = resource.clone();
            let subject = subject.clone();
            async move {
                let (Ok(resource), Some(subject), Some(relation)) =
                    (resource.parse(), subject, role)
                else {
                    return Err("that grant cannot be named".to_owned());
                };
                rn_ui::invoke::<Revoke, serde_json::Value>(Revoke {
                    resource,
                    subject,
                    relation,
                })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
            }
        }
    });
    view! {
        <div class="line">
            <span class="line-lead mono">{subject}</span>
            <span class="line-trail">
                {format!("{} \u{00b7} {}", granted.subject_kind, when(granted.at))}
            </span>
            <span class="role">{granted.relation.clone()}</span>
            <Act label="Revoke" run=run blocked=blocked then=then />
        </div>
    }
}

/// Ownership moves through this command and nowhere else.
#[component]
fn Transferrer(resource: String, then: Callback<()>) -> impl IntoView {
    let to = RwSignal::new(String::new());
    let blocked = Signal::derive(move || {
        let raw = to.get();
        let raw = raw.trim();
        if raw.is_empty() {
            Some("A person or an organization to hand it to.".to_owned())
        } else if raw.parse::<rn_api::PublicId>().is_err() {
            Some("That is not a public id.".to_owned())
        } else {
            None
        }
    });
    let run = run_with(move || {
        let resource = resource.clone();
        let raw = to.get_untracked().trim().to_owned();
        async move {
            let (Ok(resource), Ok(to)) = (resource.parse(), raw.parse()) else {
                return Err("that is not a pair of ids".to_owned());
            };
            rn_ui::invoke::<Transfer, serde_json::Value>(Transfer { resource, to })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
        }
    });
    view! {
        <section class="group">
            <h3>"Transfer"</h3>
            <div class="actions">
                <input
                    type="text"
                    class="reason"
                    aria-label="New owner"
                    placeholder="p_… or o_…"
                    prop:value=move || to.get()
                    on:input=move |event| to.set(event_target_value(&event))
                />
                <Act label="Transfer" run=run blocked=blocked then=then />
            </div>
        </section>
    }
}
