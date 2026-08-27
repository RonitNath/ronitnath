//! `/platform/products` — every product this build carries, and the one
//! decision an operator makes about each.
//!
//! The rows are the *catalogue*, not a table: a product with no row has never
//! been decided about and is off, and it is on this screen anyway, because a
//! products screen that only listed products somebody had already thought
//! about would be a screen you cannot use to think about the next one.
//!
//! **Routes is a column, not a detail.** Turning a product off takes its
//! routes away on every node the moment the feed carries it, so the screen
//! states which ones before it asks. The list comes from the compiled-in
//! catalogue, which is the same list the router is built from — so it cannot
//! promise a path the deployment does not serve.
//!
//! The panel holds no copy of the row. It reads the live one by slug, so the
//! state a reader is looking at while they press the button is the state the
//! diff last delivered — and after the toggle round-trips through `/api/cmd`,
//! the change arrives on `/api/sub` and repaints both the table and the panel
//! without a reload. That is B5's live half, visible.

use leptos::prelude::*;
use rn_api::commands::{DisableProduct, EnableProduct};
use rn_ui::{Column, PageHead, Priority, Table};
use serde::Deserialize;

use crate::panel::{Act, Facts, Panel, State, refusal, run_with, when};

/// A product, as `platform-products` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Product {
    pub slug: String,
    pub display: String,
    pub summary: String,
    pub enabled: bool,
    /// `on` or `off` — a word in its own colour, never a tinted capsule.
    pub state: String,
    /// Zero for a product nobody has decided about.
    pub changed_at: i64,
    /// Absent for the same reason, and for a row written by something with no
    /// party behind it.
    pub changed_by: Option<String>,
    /// The routes it mounts, from the catalogue.
    pub mounts: Vec<String>,
}

impl Product {
    /// The routes, as one line.
    fn routes(&self) -> String {
        self.mounts.join("  ")
    }
}

#[component]
pub fn Products() -> impl IntoView {
    let live = rn_ui::Live::<Product>::subscribe("platform-products", &[]);
    let selected = RwSignal::new(None::<String>);

    let rows = Signal::derive(move || live.rows());
    let on = Signal::derive(move || live.rows().iter().filter(|row| row.enabled).count());
    let columns = vec![
        Column::new("Product", |row: &Product| row.display.clone()),
        Column::new("Routes", Product::routes).mono(),
        Column::new("State", |row: &Product| row.state.clone()).state(),
        Column::new("Changed", |row: &Product| when(row.changed_at))
            .mono()
            .priority(Priority::Secondary),
        Column::new("Changed by", |row: &Product| {
            row.changed_by
                .clone()
                .unwrap_or_else(|| "never decided".to_owned())
        })
        .priority(Priority::Secondary),
        Column::new("Slug", |row: &Product| row.slug.clone())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Product| selected.set(Some(row.slug)));

    // The row the panel is about, read live rather than copied: a toggle
    // arrives as a diff, and the panel has to be looking at the row it moved.
    let chosen = Signal::derive(move || {
        let slug = selected.get()?;
        live.rows().into_iter().find(|row| row.slug == slug)
    });

    view! {
        <PageHead title="Products">
            <span class="count num">{move || format!("{} of {}", on.get(), rows.get().len())}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="This build carries no products."
                    on_row=open
                />
            </div>
            <Show when=move || chosen.get().is_some()>
                {move || chosen.get().map(|product| view! {
                    <Detail product=product selected=selected />
                })}
            </Show>
        </div>
    }
}

#[component]
fn Detail(product: Product, selected: RwSignal<Option<String>>) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    let enabled = product.enabled;
    let facts = vec![
        ("Slug", product.slug.clone()),
        ("Routes", product.routes()),
        (
            "Changed",
            if product.changed_at == 0 {
                "never decided".to_owned()
            } else {
                when(product.changed_at)
            },
        ),
        (
            "Changed by",
            product
                .changed_by
                .clone()
                .unwrap_or_else(|| "\u{2014}".to_owned()),
        ),
    ];
    let summary = product.summary.clone();
    // What the button is about to do, in routes rather than in adjectives.
    let consequence = if enabled {
        format!(
            "Turning it off stops {} answering on every node.",
            product.routes()
        )
    } else {
        format!("Turning it on mounts {} on every node.", product.routes())
    };

    let slug = product.slug.clone();
    let run = run_with(move || {
        let slug = slug.clone();
        async move {
            if enabled {
                rn_ui::invoke::<DisableProduct, serde_json::Value>(DisableProduct { slug })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            } else {
                rn_ui::invoke::<EnableProduct, serde_json::Value>(EnableProduct { slug })
                    .await
                    .map(|_| ())
                    .map_err(|error| refusal(&error))
            }
        }
    });

    view! {
        <Panel title=product.display.clone() on_close=close>
            <div class="panel-state">
                <State value=product.state.clone() />
            </div>
            <p class="summary">{summary}</p>
            <Facts facts=facts />
            <div class="actions">
                <Act label=if enabled { "Turn off" } else { "Turn on" } run=run />
            </div>
            <p class="none">{consequence}</p>
        </Panel>
    }
}
