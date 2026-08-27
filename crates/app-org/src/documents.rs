//! `/org/documents` — the documents the organization owns.
//!
//! The editor is the member tier's: a field the reader types into syncs itself
//! and there is no save button, and the only button is the commit that moves a
//! draft to published. The edit carries the revision the client read, so two
//! people in the same draft produce one write and one refusal rather than one
//! silent overwrite.

use leptos::prelude::*;
use rn_api::DocRole;
use rn_api::commands::{CreateDocument, EditDocument, PublishDocument, Revoke, Share, Transfer};
use rn_ui::{Column, Commit, PageHead, Priority, Table, TextArea, TextField, sync_with};

use crate::bits::{Aside, Choice, Note, act, on, refusal};
use crate::rows::Document;
use crate::scope::use_scope;

/// The organization's documents.
#[component]
pub fn Documents(
    /// The organization's public id.
    org: String,
) -> impl IntoView {
    let live = use_scope().live::<Document>("org-documents", &[("org", &org)]);
    let chosen = RwSignal::new(None::<String>);
    let columns = vec![
        Column::new("Title", |row: &Document| row.title.clone()),
        Column::new("Status", |row: &Document| row.status.clone()).state(),
        // Whether the draft is ahead of what is published is a different fact
        // from what the document *is*, and folding the two into one cell was
        // what stopped the status being a word the ink could colour.
        Column::new("Draft", |row: &Document| {
            if row.unpublished { "ahead" } else { "" }.to_owned()
        }),
        Column::new("Revision", |row: &Document| row.draft_rev.to_string())
            .mono()
            .priority(Priority::Secondary),
        Column::new("Created", |row: &Document| on(row.created_at)).priority(Priority::Tertiary),
    ];
    let rows = Signal::derive(move || live.rows());
    let selected = move || {
        let id = chosen.get()?;
        live.rows().into_iter().find(|row| row.public_id == id)
    };

    view! {
        <PageHead title="Documents" />
        <NewDocument org=org.clone() />
        <Table
            rows=rows
            columns=columns
            empty="This organization owns no documents yet."
            on_row=Callback::new(move |row: Document| chosen.set(Some(row.public_id)))
        />
        {move || selected().map(|document| view! { <Editor document=document /> })}
    }
}

/// Making one, owned by the organization rather than by the person making it.
#[component]
fn NewDocument(org: String) -> impl IntoView {
    let title = RwSignal::new(String::new());
    let note = RwSignal::new(None::<String>);
    view! {
        <div class="asides">
            <Aside title="New document">
                <label class="choice">
                    <span>"Title"</span>
                    <input
                        type="text"
                        prop:value=move || title.get()
                        on:input=move |event| title.set(event_target_value(&event))
                    />
                </label>
                <button
                    type="button"
                    class="commit"
                    disabled=move || title.get().trim().is_empty()
                    on:click=move |_| {
                        let Ok(owner) = org.parse() else {
                            return;
                        };
                        act(
                            note,
                            CreateDocument {
                                title: title.get_untracked().trim().to_owned(),
                                body: String::new(),
                                owner: Some(owner),
                            },
                            move |_| title.set(String::new()),
                        );
                    }
                >
                    "Create document"
                </button>
                <Note note=note />
            </Aside>
        </div>
    }
}

/// One document: its words, who it is shared with, and where it goes next.
#[component]
fn Editor(document: Document) -> impl IntoView {
    let id = document.public_id.clone();
    let rev = document.draft_rev;
    let title = Signal::derive({
        let held = document.title.clone();
        move || held.clone()
    });
    let body = Signal::derive({
        let held = document.body.clone();
        move || held.clone()
    });
    let publish_note = RwSignal::new(None::<String>);

    let title_sync = {
        let id = id.clone();
        sync_with(move |value: String| {
            let id = id.clone();
            async move { edit(&id, rev, Some(value), None).await }
        })
    };
    let body_sync = {
        let id = id.clone();
        sync_with(move |value: String| {
            let id = id.clone();
            async move { edit(&id, rev, None, Some(value)).await }
        })
    };

    view! {
        <div class="panel editor">
            <TextField label="Title" value=title sync=title_sync />
            <TextArea label="Body" value=body sync=body_sync rows=12 />
            <Commit
                label="Publish document"
                version=Signal::derive(move || rev as u64)
                disabled=Signal::derive(move || !document.unpublished)
                on_commit=Callback::new({
                    let id = id.clone();
                    move |_| {
                        let Ok(document) = id.parse() else {
                            return;
                        };
                        act(publish_note, PublishDocument { document }, |_| ());
                    }
                })
            />
            <Note note=publish_note />
            <Sharing document=document.clone() />
        </div>
    }
}

/// Who this document reaches, and how ownership leaves it.
#[component]
fn Sharing(document: Document) -> impl IntoView {
    let subject = RwSignal::new(String::new());
    let relation = RwSignal::new("viewer".to_owned());
    let note = RwSignal::new(None::<String>);
    let transfer_note = RwSignal::new(None::<String>);
    let to = RwSignal::new(String::new());
    let id = document.public_id.clone();
    let grants = document.shared.clone();

    let run = Callback::new(move |grant: bool| {
        let Ok(resource) = id.parse() else {
            return;
        };
        let Ok(who) = subject.get_untracked().trim().parse() else {
            note.set(Some("That is not a public id.".to_owned()));
            return;
        };
        let role = match relation.get_untracked().as_str() {
            "commenter" => DocRole::Commenter,
            "editor" => DocRole::Editor,
            _ => DocRole::Viewer,
        };
        if grant {
            act(
                note,
                Share {
                    resource,
                    subject: who,
                    relation: role,
                },
                |_| (),
            );
        } else {
            act(
                note,
                Revoke {
                    resource,
                    subject: who,
                    relation: role,
                },
                |_| (),
            );
        }
    });
    let transfer_id = document.public_id.clone();

    view! {
        <section class="block">
            <h3>"Shared with"</h3>
            <ul class="grants">
                {grants
                    .into_iter()
                    .map(|grant| {
                        let who = grant
                            .display
                            .clone()
                            .unwrap_or_else(|| grant.subject_kind.clone());
                        view! {
                            <li>
                                <span>{who}</span>
                                <span class="relation">{grant.relation}</span>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
            <div class="minting">
                <label class="choice wide">
                    <span>"Share with"</span>
                    <input
                        type="text"
                        spellcheck="false"
                        placeholder="p_… o_… g_…"
                        prop:value=move || subject.get()
                        on:input=move |event| subject.set(event_target_value(&event))
                    />
                </label>
                <Choice
                    label="May"
                    options=vec![
                        ("viewer", "view"),
                        ("commenter", "comment"),
                        ("editor", "edit"),
                    ]
                    value=relation
                />
                <button type="button" class="commit" on:click=move |_| run.run(true)>
                    "Share"
                </button>
                <button type="button" class="quiet" on:click=move |_| run.run(false)>
                    "Revoke"
                </button>
                <Note note=note />
            </div>
            <div class="asides">
                <Aside title="Transfer document">
                    <label class="choice wide">
                        <span>"New owner"</span>
                        <input
                            type="text"
                            spellcheck="false"
                            placeholder="p_… o_…"
                            prop:value=move || to.get()
                            on:input=move |event| to.set(event_target_value(&event))
                        />
                    </label>
                    <button
                        type="button"
                        class="commit"
                        disabled=move || to.get().trim().is_empty()
                        on:click=move |_| {
                            let (Ok(resource), Ok(id)) = (
                                transfer_id.parse(),
                                to.get_untracked().trim().parse(),
                            ) else {
                                transfer_note.set(Some("That is not a public id.".to_owned()));
                                return;
                            };
                            act(
                                transfer_note,
                                Transfer { resource, to: id },
                                move |_| to.set(String::new()),
                            );
                        }
                    >
                        "Transfer"
                    </button>
                    <Note note=transfer_note />
                </Aside>
            </div>
        </section>
    }
}

/// One edit, sent against the revision the client read.
async fn edit(
    id: &str,
    expected_rev: i64,
    title: Option<String>,
    body: Option<String>,
) -> Result<(), String> {
    let Ok(document) = id.parse() else {
        return Err("That is not a document id.".to_owned());
    };
    rn_ui::invoke::<EditDocument, serde_json::Value>(EditDocument {
        document,
        expected_rev,
        title,
        body,
    })
    .await
    .map(|_| ())
    .map_err(|error| refusal(&error))
}
