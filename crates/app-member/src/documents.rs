//! `/app/documents` — everything this person can reach, and the one command
//! that makes another.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use rn_api::commands::CreateDocument;
use rn_ui::{Column, Commit, Live, PageHead, Priority, Table};

use crate::api::{Refusal, attempt};
use crate::parts::{Note, Section, titled, when};
use crate::rows::Document;

#[component]
pub fn Documents() -> impl IntoView {
    let documents = Live::<Document>::subscribe("documents", &[]);
    let refusal = RwSignal::new(None::<Refusal>);
    let navigate = use_navigate();

    let open = {
        let navigate = navigate.clone();
        Callback::new(move |row: Document| {
            navigate(&format!("/documents/{}", row.public_id), Default::default());
        })
    };

    let title = RwSignal::new(String::new());
    let create = Callback::new(move |_: u64| {
        let navigate = navigate.clone();
        attempt(
            CreateDocument {
                title: title.get_untracked(),
                body: String::new(),
                owner: None,
            },
            refusal,
            move |result| {
                title.set(String::new());
                if let Some(id) = result.get("document").and_then(|id| id.as_str()) {
                    navigate(&format!("/documents/{id}"), Default::default());
                }
            },
        );
    });

    let rows = Signal::derive(move || documents.rows());

    view! {
        <PageHead title="Documents" />
        <div class="sections">
            <Section title="New document">
                <div class="inline">
                    <div class="field">
                        <label for="document-title">"Title"</label>
                        <input
                            id="document-title"
                            type="text"
                            prop:value=move || title.get()
                            on:input=move |event| title.set(event_target_value(&event))
                        />
                        <Note refusal=refusal field="title" />
                    </div>
                    <Commit
                        label="Create document"
                        version=Signal::derive(|| 0)
                        on_commit=create
                        disabled=Signal::derive(move || title.get().trim().is_empty())
                    />
                </div>
            </Section>
            <Section title="Documents" wide=true>
                <Table
                    rows=rows
                    columns=vec![
                        Column::new("Title", |row: &Document| row.title.clone()),
                        Column::new("State", |row: &Document| row.status.clone()).state(),
                        // The draft's revision and the published one. "Draft,
                        // unpublished changes" was two status words for a
                        // document that had never been published — a fact
                        // about nothing, said twice.
                        Column::new("Revision", |row: &Document| row.draft_rev.to_string())
                            .mono()
                            .priority(Priority::Secondary),
                        Column::new(
                                "Published",
                                |row: &Document| {
                                    row.published_rev
                                        .map_or_else(
                                            || "\u{2014}".to_owned(),
                                            |rev| rev.to_string(),
                                        )
                                },
                            )
                            .mono()
                            .priority(Priority::Secondary),
                        Column::new("Yours", |row: &Document| titled(&row.relation))
                            .priority(Priority::Secondary),
                        Column::new("Owner", |row: &Document| row.owner.display.clone())
                            .priority(Priority::Secondary),
                        Column::new("Created", |row: &Document| when(row.created_at))
                            .mono()
                            .priority(Priority::Tertiary),
                    ]
                    empty="No documents yet. Make one, or wait to be shared one."
                    on_row=open
                />
            </Section>
        </div>
    }
}
