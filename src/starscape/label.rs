//! Ambient "where in the world" caption — nearest fetched city to the
//! current sub-observer point (platform-scope §Starscape — Grounding:
//! "show the nearest city to the current sub-observer point ... so the
//! viewer knows roughly where on Earth they're looking out from").
//!
//! The catalog is a separately cached static asset rather than 100 KB embedded
//! in every wasm/server binary. SSR therefore emits an honestly empty label;
//! hydration fills it only after a validated fetch succeeds, avoiding a flash
//! of incorrect location text. See `live` for loading and update behavior.

use leptos::prelude::*;

#[cfg(any(feature = "hydrate", test))]
use super::{City, CityCatalog};

#[cfg(feature = "hydrate")]
mod live;

/// Below this distance the observer counts as directly over the city.
/// GeoNames `cities15000` entries are population-15,000+ places, whose
/// built-up extent plus a margin for the ~28km ground-track step between
/// re-samples (see `live::UPDATE_INTERVAL_MS`) comfortably fits inside
/// 50km without reaching into "next city over" territory for the dataset's
/// typical urban spacing.
#[cfg(any(feature = "hydrate", test))]
const OVER_THRESHOLD_KM: f64 = 50.0;

/// Real-time cadence between re-samples, used by `live` and by the test that
/// proves the readout actually moves. At the 60× simulation speed half a
/// second of wall clock is 30 simulated seconds — roughly 28km of ground
/// track, which lands in the second decimal of a degree, so the coordinates
/// visibly tick without the text becoming a blur.
#[cfg(any(feature = "hydrate", test))]
const UPDATE_INTERVAL_MS: i32 = 500;

/// Position first, place second: the coordinates tick continuously and are
/// the thing that makes the label read as live, while the city is the thing
/// that makes them mean something.
#[cfg(any(feature = "hydrate", test))]
fn format_grounding(lat_deg: f64, lon_deg: f64, city: Option<&City<'_>>) -> String {
    let position = format_position(lat_deg, lon_deg);
    match city {
        Some(city) => format!("{position} · {}", format_place(city)),
        None => position,
    }
}

/// Signed degrees are how the code carries a position and not how anyone
/// reads one. Two decimals is about a kilometre — fine enough that the digits
/// visibly move as the observer travels, coarse enough not to imply a
/// precision the great-circle track doesn't have.
#[cfg(any(feature = "hydrate", test))]
fn format_position(lat_deg: f64, lon_deg: f64) -> String {
    // Round before choosing the hemisphere, so a longitude of -0.001 prints
    // as "0.00° E" rather than the jarring "0.00° W".
    let lat = (lat_deg * 100.0).round() / 100.0;
    let lon = (lon_deg * 100.0).round() / 100.0;
    format!(
        "{:.2}° {}, {:.2}° {}",
        lat.abs(),
        if lat < 0.0 { 'S' } else { 'N' },
        lon.abs(),
        if lon < 0.0 { 'W' } else { 'E' },
    )
}

/// "over" for the common near-a-city case, otherwise an honest distance. Most
/// of Earth is ocean, so the distance branch is the common case in practice —
/// the copy states only what `nearest_city` actually knows (a city name and a
/// distance), not a guess about terrain ("in the middle of the Pacific") the
/// data doesn't provide.
#[cfg(any(feature = "hydrate", test))]
fn format_place(city: &City<'_>) -> String {
    if city.distance_km < OVER_THRESHOLD_KM {
        format!("over {}, {}", city.name, city.country)
    } else {
        format!(
            "{} km from {}, {}",
            format_km(city.distance_km),
            city.name,
            city.country
        )
    }
}

/// Thousands-separated, rounded to the nearest km — `1403.8821` is not
/// something a human reads at a glance.
#[cfg(any(feature = "hydrate", test))]
fn format_km(km: f64) -> String {
    let rounded = km.round().max(0.0) as u64;
    let digits = rounded.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.chars().rev().collect()
}

#[cfg(test)]
fn label_text(catalog: &CityCatalog, sim_ms: f64) -> String {
    let (lat_deg, lon_deg) = super::observer_at(sim_ms);
    format_grounding(lat_deg, lon_deg, catalog.nearest(lat_deg, lon_deg).as_ref())
}

/// `epoch_ms` is the server clock at render (same prop shape as
/// `Starscape`) and cancels client clock skew after the city asset arrives.
#[island]
pub fn CityLabel(epoch_ms: f64) -> impl IntoView {
    let text = RwSignal::new(String::new());

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        live::start(text, epoch_ms);
        #[cfg(not(feature = "hydrate"))]
        let _ = epoch_ms;
    });

    // Deliberately NOT aria-hidden, unlike the purely decorative
    // canvas/starfield: this is real informational text (a place name).
    // Deliberately NOT an aria-live region either: the city changes every
    // few minutes as the observer travels, and announcing that on a timer
    // with no corresponding user action is exactly the screen-reader
    // nuisance design/review.md's "motion noise" entry warns against. A
    // screen reader that scans the page reads whichever city is current at
    // that moment — useful once, not naggy on a loop.
    view! { <p class="grounding">{move || text.get()}</p> }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_km_adds_thousands_separators_and_rounds() {
        assert_eq!(format_km(1_403.882_1), "1,404");
        assert_eq!(format_km(999.4), "999");
        assert_eq!(format_km(1_000.0), "1,000");
        assert_eq!(format_km(0.2), "0");
    }

    #[test]
    fn format_place_switches_at_the_over_threshold() {
        let mut city = City {
            name: "Testville",
            country: "TS",
            lat_deg: 0.0,
            lon_deg: 0.0,
            distance_km: OVER_THRESHOLD_KM - 0.01,
        };
        assert_eq!(format_place(&city), "over Testville, TS");

        city.distance_km = OVER_THRESHOLD_KM;
        assert_eq!(format_place(&city), "50 km from Testville, TS");
    }

    #[test]
    fn format_position_names_the_hemisphere_for_each_quadrant() {
        assert_eq!(format_position(37.7749, -122.4194), "37.77° N, 122.42° W");
        assert_eq!(format_position(-33.8688, 151.2093), "33.87° S, 151.21° E");
        // Rounding decides the hemisphere, so a hair below the equator or the
        // prime meridian never prints a signed-zero "0.00° S".
        assert_eq!(format_position(-0.001, -0.004), "0.00° N, 0.00° E");
    }

    #[test]
    fn grounding_leads_with_position_and_survives_an_empty_catalog() {
        let city = City {
            name: "Reykjavík",
            country: "IS",
            lat_deg: 64.14,
            lon_deg: -21.9,
            distance_km: 12.0,
        };
        assert_eq!(
            format_grounding(64.1, -21.8, Some(&city)),
            "64.10° N, 21.80° W · over Reykjavík, IS"
        );
        assert_eq!(format_grounding(64.1, -21.8, None), "64.10° N, 21.80° W");
    }

    #[test]
    fn the_label_moves_between_consecutive_refreshes() {
        // The point of the coordinate readout is that it is live. One update
        // interval of simulated travel must change the rendered text — a
        // regression that froze the sample time would still render a
        // plausible label, and only this catches it.
        let catalog = CityCatalog::parse(include_bytes!("../../public/cities/cities.bin").to_vec())
            .expect("valid city fixture");
        let t = super::super::SIM_EPOCH_MS;
        let step = f64::from(UPDATE_INTERVAL_MS) * super::super::SPEED;
        assert_ne!(label_text(&catalog, t), label_text(&catalog, t + step));
    }

    #[test]
    fn label_text_is_nonempty_after_catalog_load() {
        let catalog = CityCatalog::parse(include_bytes!("../../public/cities/cities.bin").to_vec())
            .expect("valid city fixture");
        assert!(!label_text(&catalog, super::super::SIM_EPOCH_MS).is_empty());
    }
}
