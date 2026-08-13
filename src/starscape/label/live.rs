//! Browser-only city-catalog loading and periodic re-sampling for `CityLabel`.

use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

use super::{CityCatalog, UPDATE_INTERVAL_MS};
use crate::starscape::{sim_time_ms, synced_sim_time_ms};

pub fn start(text: RwSignal<String>, server_epoch_ms: f64) {
    // This must precede the fetch: otherwise cold-cache latency shifts the
    // location label behind the server-synchronized canvas.
    let client_mount_ms = js_sys::Date::now();
    spawn_local(async move {
        if let Err(error) = load_and_run(text, server_epoch_ms, client_mount_ms).await {
            // The empty SSR label is the honest absent state on fetch failure.
            web_sys::console::warn_2(&"city catalog unavailable:".into(), &error);
        }
    });
}

async fn load_and_run(
    text: RwSignal<String>,
    server_epoch_ms: f64,
    client_mount_ms: f64,
) -> Result<(), wasm_bindgen::JsValue> {
    let response: Response = JsFuture::from(window().fetch_with_str("/cities/cities.bin"))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(format!("city asset HTTP {}", response.status()).into());
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let catalog = Rc::new(CityCatalog::parse(bytes).ok_or("invalid city asset")?);

    let media = window().match_media("(prefers-reduced-motion: reduce)")?;
    let reduced = media.as_ref().map(|query| query.matches()).unwrap_or(false);
    let frozen_at = Rc::new(Cell::new(reduced.then(|| sim_time_ms(server_epoch_ms))));
    refresh(
        &catalog,
        text,
        server_epoch_ms,
        client_mount_ms,
        frozen_at.get(),
    );

    let tick_catalog = Rc::clone(&catalog);
    let tick_frozen = Rc::clone(&frozen_at);
    let tick = Closure::<dyn FnMut()>::new(move || {
        // The low-frequency interval remains allocated for later preference
        // changes, but does no lookup or DOM work while motion is reduced.
        if tick_frozen.get().is_none() {
            refresh(&tick_catalog, text, server_epoch_ms, client_mount_ms, None);
        }
    });
    window().set_interval_with_callback_and_timeout_and_arguments_0(
        tick.as_ref().unchecked_ref(),
        UPDATE_INTERVAL_MS,
    )?;
    tick.forget();

    // Globe launches promise that the location label changes before the
    // explorer import can complete. The periodic refresh remains the normal
    // orbit cadence; this event is the synchronous manual-selection seam.
    let selected_catalog = Rc::clone(&catalog);
    let selected_frozen = Rc::clone(&frozen_at);
    let selected = Closure::<dyn FnMut()>::new(move || {
        refresh(
            &selected_catalog,
            text,
            server_epoch_ms,
            client_mount_ms,
            selected_frozen.get(),
        );
    });
    window().add_event_listener_with_callback(
        "starscape-observer-changed",
        selected.as_ref().unchecked_ref(),
    )?;
    selected.forget();

    if let Some(media) = media {
        let changed_catalog = Rc::clone(&catalog);
        let changed_frozen = Rc::clone(&frozen_at);
        let query = media.clone();
        let changed = Closure::<dyn FnMut()>::new(move || {
            let frozen = query
                .matches()
                .then(|| synced_sim_time_ms(server_epoch_ms, client_mount_ms, js_sys::Date::now()));
            changed_frozen.set(frozen);
            refresh(
                &changed_catalog,
                text,
                server_epoch_ms,
                client_mount_ms,
                frozen,
            );
        });
        media.add_event_listener_with_callback("change", changed.as_ref().unchecked_ref())?;
        changed.forget();
    }
    Ok(())
}

fn refresh(
    catalog: &CityCatalog,
    text: RwSignal<String>,
    server_epoch_ms: f64,
    client_mount_ms: f64,
    frozen_at: Option<f64>,
) {
    let sim_ms = frozen_at.unwrap_or_else(|| {
        synced_sim_time_ms(server_epoch_ms, client_mount_ms, js_sys::Date::now())
    });
    let sim_ms = crate::starscape::bridge::displayed_sim_time(sim_ms);
    let (lat_deg, lon_deg) = crate::starscape::bridge::observer_for(sim_ms);
    let next =
        super::format_grounding(lat_deg, lon_deg, catalog.nearest(lat_deg, lon_deg).as_ref());
    if text.get_untracked() != next {
        text.set(next);
    }
}
