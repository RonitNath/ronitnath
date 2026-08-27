//! The grounding readout: where on Earth the viewer is looking out from.
//!
//! Position first, place second. The coordinates tick continuously and are what
//! makes the line read as live; the city is what makes them mean something.
//! The copy states only what the lookup actually knows — a name and a distance —
//! and never guesses at terrain.

use crate::cities::City;

/// Under this distance the observer counts as directly over the city. GeoNames
/// `cities15000` entries are places of 15,000 people or more, whose built-up
/// extent plus the ~28 km of ground track between samples fits inside 50 km
/// without reaching into the next city over at this dataset's spacing.
const OVER_THRESHOLD_KM: f64 = 50.0;

/// Wall-clock milliseconds between re-samples. At 60x, half a second of real
/// time is 30 simulated seconds — about 28 km of track, which lands in the
/// second decimal of a degree: the digits visibly move without becoming a blur.
pub const UPDATE_INTERVAL_MS: i32 = 500;

#[must_use]
pub fn grounding(lat_deg: f64, lon_deg: f64, city: Option<&City<'_>>) -> String {
    let position = position(lat_deg, lon_deg);
    match city {
        Some(city) => format!("{position} · {}", place(city)),
        None => position,
    }
}

/// Signed degrees are how the code carries a position, not how anyone reads
/// one. Two decimals is about a kilometre: fine enough that the digits move as
/// the observer travels, coarse enough not to imply a precision the track does
/// not have.
#[must_use]
pub fn position(lat_deg: f64, lon_deg: f64) -> String {
    // Round before choosing the hemisphere, so -0.001 prints as "0.00° E"
    // rather than the jarring "0.00° W".
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

fn place(city: &City<'_>) -> String {
    if city.distance_km < OVER_THRESHOLD_KM {
        format!("over {}, {}", city.name, city.country)
    } else {
        format!(
            "{} km from {}, {}",
            km(city.distance_km),
            city.name,
            city.country
        )
    }
}

/// Thousands-separated and rounded: `1403.8821` is not something a human reads
/// at a glance.
fn km(distance: f64) -> String {
    let digits = (distance.round().max(0.0) as u64).to_string();
    let mut grouped: Vec<char> = Vec::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i.is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::CityCatalog;
    use crate::sky::{SIM_EPOCH_MS, SPEED};
    use crate::track::observer_at;

    fn city(distance_km: f64) -> City<'static> {
        City {
            name: "Testville",
            country: "TS",
            distance_km,
        }
    }

    #[test]
    fn distances_are_rounded_and_grouped_the_way_a_person_reads_them() {
        assert_eq!(km(1_403.882_1), "1,404");
        assert_eq!(km(999.4), "999");
        assert_eq!(km(1_000.0), "1,000");
        assert_eq!(km(0.2), "0");
        assert_eq!(km(12_345_678.0), "12,345,678");
    }

    #[test]
    fn the_wording_switches_at_the_over_threshold() {
        assert_eq!(place(&city(OVER_THRESHOLD_KM - 0.01)), "over Testville, TS");
        assert_eq!(place(&city(OVER_THRESHOLD_KM)), "50 km from Testville, TS");
    }

    #[test]
    fn every_quadrant_names_its_hemisphere_and_never_prints_a_signed_zero() {
        assert_eq!(position(37.7749, -122.4194), "37.77° N, 122.42° W");
        assert_eq!(position(-33.8688, 151.2093), "33.87° S, 151.21° E");
        assert_eq!(position(-0.001, -0.004), "0.00° N, 0.00° E");
    }

    #[test]
    fn the_readout_leads_with_position_and_survives_a_catalog_that_never_loaded() {
        let city = City {
            name: "Reykjavík",
            country: "IS",
            distance_km: 12.0,
        };
        assert_eq!(
            grounding(64.1, -21.8, Some(&city)),
            "64.10° N, 21.80° W · over Reykjavík, IS"
        );
        assert_eq!(grounding(64.1, -21.8, None), "64.10° N, 21.80° W");
    }

    #[test]
    fn the_readout_actually_changes_between_two_consecutive_refreshes() {
        // The point of the coordinate readout is that it is live. A regression
        // that froze the sample time would still render a plausible label, and
        // only this catches it.
        let catalog =
            CityCatalog::parse(include_bytes!("../../../static/cities/cities.bin").to_vec())
                .expect("the shipped city catalog");
        let text = |sim_ms: f64| {
            let (lat, lon) = observer_at(sim_ms);
            grounding(lat, lon, catalog.nearest(lat, lon).as_ref())
        };
        let step = f64::from(UPDATE_INTERVAL_MS) * SPEED;
        assert_ne!(text(SIM_EPOCH_MS), text(SIM_EPOCH_MS + step));
    }
}
