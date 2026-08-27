//! `/app/documents/<id>` — one document, edited the way this system edits
//! anything.
//!
//! There is no save button. The title and the body sync themselves — on blur,
//! or after three seconds of quiet — carrying `expected_rev`, the revision the
//! client read. The server guards on it, so two people editing the same draft
//! produce one write and one decline rather than one write that ate the other,
//! and the decline lands beside the field that caused it with the fresh text
//! already re-read underneath.
//!
//! `Publish` is the one button, because publishing is a state transition and
//! not a save: it carries the revision it is promoting.

mod share;
mod words;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use rn_api::commands::{EditDocument, PublishDocument};
use rn_ui::{Commit, Live, PageHead, TextField, sync_with};

use crate::api::{Refusal, attempt, run};
use crate::parts::{Note, Pair, Section, titled, when};
use crate::rows::Document;
use share::Sharing;
use words::Words;

#[component]
pub fn Editor() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| params.with(|params| params.get("id").unwrap_or_default()));

    let document = RwSignal::new(None::<Document>);
    let rev = RwSignal::new(0i64);
    let refusal = RwSignal::new(None::<Refusal>);

    // The list is live, so an edit somebody else committed arrives here as a
    // revision this page has not seen — which is when it re-reads the words.
    let documents = Live::<Document>::subscribe("documents", &[]);

    let reread = move || {
        let wanted = id.get_untracked();
        spawn_local(async move {
            if let Ok(rows) =
                rn_ui::query::<Vec<Document>>("document", &[("id", wanted.as_str())]).await
                && let Some(row) = rows.into_iter().next()
            {
                rev.set(row.draft_rev);
                document.set(Some(row));
            }
        });
    };

    Effect::new(move |_| {
        // Re-run when the route changes, and when a diff moves this row's
        // revision past the one this page is holding.
        let wanted = id.get();
        let theirs = documents
            .rows()
            .into_iter()
            .find(|row| row.public_id == wanted)
            .map(|row| row.draft_rev);
        let mine = rev.get_untracked();
        if document.get_untracked().is_none() || theirs.is_some_and(|theirs| theirs > mine) {
            reread();
        }
    });

    let edit = move |title: Option<String>, body: Option<String>| {
        let document_id = id.get_untracked();
        async move {
            let Ok(document_id) = document_id.parse() else {
                return Err("That is not a document.".to_owned());
            };
            let args = EditDocument {
                document: document_id,
                expected_rev: rev.get_untracked(),
                title,
                body,
            };
            match run(&args).await {
                Ok(result) => {
                    if let Some(now) = result.get("rev").and_then(serde_json::Value::as_i64) {
                        rev.set(now);
                    }
                    Ok(())
                }
                Err(Refusal::Declined) => {
                    reread();
                    Err("Changed elsewhere. This is the current text.".to_owned())
                }
                Err(other) => Err(other.message()),
            }
        }
    };

    let title_sync = sync_with(move |value: String| edit(Some(value), None));
    let body_sync = sync_with(move |value: String| edit(None, Some(value)));

    let publish = Callback::new(move |version: u64| {
        let _ = version;
        let Ok(document_id) = id.get_untracked().parse() else {
            return;
        };
        attempt(
            PublishDocument {
                document: document_id,
            },
            refusal,
            move |_| reread(),
        );
    });

    let facts = move || {
        document.get().map(|row| {
            view! {
                <dl class="kv">
                    <Pair label="Owner">
                        {format!("{} ({})", row.owner.display, row.owner.kind)}
                    </Pair>
                    <Pair label="Yours">{titled(&row.relation)}</Pair>
                    <Pair label="State">{titled(&row.status)}</Pair>
                    <Pair label="Revision">
                        <span class="figure">
                            {row.draft_rev} {row.published_rev.map(|at| format!(" · published {at}"))}
                        </span>
                    </Pair>
                    <Pair label="Created">{when(row.created_at)}</Pair>
                </dl>
            }
        })
    };

    let words = move || {
        let row = document.get()?;
        let editable = row.may_edit;
        Some(view! {
            <div class="editor">
                <TextField
                    label="Title"
                    value=Signal::derive(move || {
                        document.get().map(|row| row.title).unwrap_or_default()
                    })
                    sync=title_sync.clone()
                />
                <Words
                    value=Signal::derive(move || {
                        document.get().and_then(|row| row.body).unwrap_or_default()
                    })
                    sync=body_sync.clone()
                    readonly=!editable
                />
            </div>
        })
    };

    view! {
        <PageHead title="Document">
            <Commit
                label="Publish"
                version=Signal::derive(move || rev.get().max(0) as u64)
                on_commit=publish
                disabled=Signal::derive(move || {
                    document.get().is_none_or(|row| !row.may_edit || !row.unpublished)
                })
            />
        </PageHead>
        <Note refusal=refusal />
        <div class="sections">
            <Section title="Words" wide=true>{words}</Section>
            <Section title="About">{facts}</Section>
            <Sharing document=Signal::derive(move || id.get()) />
        </div>
    }
}
