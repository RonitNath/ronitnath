//! `/platform/clients` — the relying parties registered against this issuer.
//!
//! Three commands live here and each of them takes something with it, so each
//! says what before it runs. Rotating a secret kills the old one immediately —
//! there is no overlap, because two live credentials is two things that could
//! have leaked and no way to tell which. Deleting a client takes its consents
//! and its live tokens, and the row already carries both numbers, so the
//! confirm names them rather than fetching them while somebody is clicking.
//!
//! The secret comes back exactly once. It is minted here and stored as a
//! digest, so this panel is the only place it will ever exist — replaying the
//! idempotency key returns the same reply with no secret in it, which is the
//! `Minted` contract and not an oversight. The reason is stated where the
//! secret is, because a reader who does not know it will not come back has no
//! way to guess.

use leptos::prelude::*;
use rn_api::commands::{DeleteClient, RotateClientSecret};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::panel::{Act, Facts, Group, Line, Panel, State, refusal, run_with, when};
use crate::rows::Client;

#[component]
pub fn Clients() -> impl IntoView {
    let live = rn_ui::Live::<Client>::subscribe("platform-clients", &[]);
    let selected = RwSignal::new(None::<String>);

    let rows = Signal::derive(move || live.rows());
    // The registry is live, so the open panel reads out of the same rows the
    // table does: a drill-in that fetched by parameter would be a second
    // answer about a row the subscription is already keeping current.
    let detail = Signal::derive(move || {
        let id = selected.get()?;
        live.rows()
            .into_iter()
            .find(|row| row.public_id.to_string() == id)
    });
    let columns = vec![
        Column::new("Name", |row: &Client| row.client_name.clone()),
        Column::new("Owner", |row: &Client| {
            row.owner_display
                .clone()
                .unwrap_or_else(|| "the deployment".to_owned())
        }),
        Column::new("Auth", |row: &Client| {
            row.token_endpoint_auth_method.clone()
        })
        .mono(),
        Column::new("Consents", |row: &Client| row.consents.to_string()).mono(),
        Column::new("Last issued", |row: &Client| {
            row.last_issued_at
                .map_or_else(|| "\u{2014}".to_owned(), when)
        })
        .mono()
        .priority(Priority::Secondary),
        Column::new("State", |row: &Client| {
            if row.withdrawn_at.is_some() {
                "withdrawn".to_owned()
            } else if row.trusted {
                "trusted".to_owned()
            } else {
                "registered".to_owned()
            }
        })
        .state(),
        Column::new("client_id", |row: &Client| row.public_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    let open = Callback::new(move |row: Client| selected.set(Some(row.public_id.to_string())));

    view! {
        <PageHead title="Clients">
            <span class="count num">{move || rows.get().len()}</span>
        </PageHead>
        <div class="split">
            <div class="split-main">
                <Table
                    rows=rows
                    columns=columns
                    empty="No relying party is registered. This deployment is its own issuer and nothing has asked it for a token."
                    on_row=open
                />
            </div>
            <Show when=move || detail.get().is_some()>
                {move || {
                    detail.get().map(|client| view! { <Detail client=client selected=selected /> })
                }}
            </Show>
        </div>
    }
}

#[component]
fn Detail(client: Client, selected: RwSignal<Option<String>>) -> impl IntoView {
    let close = Callback::new(move |()| selected.set(None));
    // Shown once, held only in this page's memory, and never asked for again.
    let secret = RwSignal::new(None::<String>);

    let facts = vec![
        (
            "Owner",
            client
                .owner_display
                .clone()
                .unwrap_or_else(|| "the deployment".to_owned()),
        ),
        ("Auth method", client.token_endpoint_auth_method.clone()),
        ("Registered", when(client.created_at)),
        (
            "Secret rotated",
            client
                .rotated_at
                .map_or_else(|| "\u{2014}".to_owned(), when),
        ),
        (
            "Last issued",
            client
                .last_issued_at
                .map_or_else(|| "\u{2014}".to_owned(), when),
        ),
        ("Live tokens", client.live_tokens.to_string()),
        ("client_id", client.public_id.to_string()),
    ];

    let redirects = client
        .redirect_uris
        .clone()
        .into_iter()
        .map(|uri| view! { <Line lead=uri trail=String::new() /> })
        .collect_view();
    let scopes = client
        .scopes
        .clone()
        .into_iter()
        .map(|scope| view! { <Line lead=scope trail=String::new() /> })
        .collect_view();
    let counts = (client.redirect_uris.len(), client.scopes.len());

    let rotating = client.public_id.clone();
    let rotate = run_with(move || {
        let client = rotating.clone();
        async move {
            rn_ui::command::<_, serde_json::Value>(
                "rotate-client-secret",
                RotateClientSecret { client },
            )
            .await
            .map(|committed| {
                // The one place the secret will ever be. A replay of the same
                // key answers with no secret at all, which is the contract.
                secret.set(
                    committed
                        .result
                        .get("client_secret")
                        .and_then(|secret| secret.as_str())
                        .map(ToOwned::to_owned),
                );
            })
            .map_err(|error| refusal(&error))
        }
    });

    let deleting = client.public_id.clone();
    let delete = run_with(move || {
        let client = deleting.clone();
        async move {
            rn_ui::invoke::<DeleteClient, serde_json::Value>(DeleteClient { client })
                .await
                .map(|_| ())
                .map_err(|error| refusal(&error))
        }
    });
    let takes = (client.consents, client.live_tokens);

    view! {
        <Panel title=client.client_name.clone() on_close=close>
            <div class="panel-state">
                <State value=if client.withdrawn_at.is_some() {
                    "withdrawn"
                } else if client.trusted {
                    "trusted"
                } else {
                    "registered"
                } />
            </div>
            <Facts facts=facts />
            <Group label="Redirect URIs" count=counts.0 empty="It has none, so no authorization response can land anywhere.">
                {redirects}
            </Group>
            <Group label="Scopes" count=counts.1 empty="It may ask for nothing.">
                {scopes}
            </Group>
            <section class="group">
                <h3>"Secret"</h3>
                <div class="actions">
                    <Act label="Rotate secret" run=rotate />
                </div>
                <Show when=move || secret.get().is_some()>
                    <p class="none">
                        "Shown once. It is stored as a digest, so this page is the only place it exists."
                    </p>
                    <pre class="payload mono">{move || secret.get().unwrap_or_default()}</pre>
                </Show>
            </section>
            <section class="group">
                <h3>"Withdraw"</h3>
                <p class="none">
                    {format!(
                        "Deleting it takes {} consent{} and {} live token{} with it.",
                        takes.0,
                        if takes.0 == 1 { "" } else { "s" },
                        takes.1,
                        if takes.1 == 1 { "" } else { "s" },
                    )}
                </p>
                <div class="actions">
                    <Act label="Delete client" run=delete />
                </div>
            </section>
        </Panel>
    }
}
