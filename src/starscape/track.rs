//! Earth-fixed great-circle observer track.
//!
//! The track is a route over the rotating Earth, not an inertial orbit. This
//! module returns geographic latitude/longitude in the Earth-fixed frame;
//! `view_matrix` composes that longitude with GMST to rotate the inertial sky
//! into the local horizon frame. Applying Earth rotation here as well would
//! double-count it.
//!
//! The chosen great circle has a 63° inclination to the equator, so it passes
//! through San Francisco while reaching a broad ±63° latitude band without
//! dwelling near the poles.
//!
//! One lap is exactly half a sidereal day of simulated time (11.97 simulated
//! hours — a ~12 minute on-screen journey at the global 60× speed-up, half
//! the sky's rotation period). "Half a sidereal day" rather than a round 12
//! hours because `sim_time_ms` resyncs the clock by whole sidereal days: an
//! exact divisor makes that resync a whole number of laps, so the observer's
//! position is continuous across the seam just as the sky's orientation is.
//! A period that did not divide it would teleport the viewer mid-flight.

use super::{SF_LAT_DEG, SF_LON_DEG, SIDEREAL_DAY_MS, SIM_EPOCH_MS};

pub const TRACK_INCLINATION_DEG: f64 = 63.0;
pub const TRACK_PERIOD_MS: f64 = SIDEREAL_DAY_MS / 2.0;

/// Earth-fixed observer subpoint for the accelerated simulation time.
///
/// Returns `(lat_deg, lon_deg)`, with longitude normalized to `[-180, 180)`.
/// At `SIM_EPOCH_MS` the result is exactly San Francisco.
pub fn observer_at(sim_ms: f64) -> (f64, f64) {
    let elapsed_ms = (sim_ms - SIM_EPOCH_MS).rem_euclid(TRACK_PERIOD_MS);
    if elapsed_ms == 0.0 {
        return (SF_LAT_DEG, SF_LON_DEG);
    }

    let (start, tangent) = track_basis();
    let phase = std::f64::consts::TAU * elapsed_ms / TRACK_PERIOD_MS;
    let (sin_phase, cos_phase) = phase.sin_cos();
    let p = [
        start[0] * cos_phase + tangent[0] * sin_phase,
        start[1] * cos_phase + tangent[1] * sin_phase,
        start[2] * cos_phase + tangent[2] * sin_phase,
    ];

    let lat = p[2].clamp(-1.0, 1.0).asin().to_degrees();
    let lon = p[1].atan2(p[0]).to_degrees();
    (lat, normalize_lon_deg(lon))
}

fn track_basis() -> ([f64; 3], [f64; 3]) {
    let lat = SF_LAT_DEG.to_radians();
    let lon = SF_LON_DEG.to_radians();
    let (sin_lat, cos_lat) = lat.sin_cos();
    let (sin_lon, cos_lon) = lon.sin_cos();

    let start = [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat];

    // Build the normal of the great-circle plane. For an inclination i, the
    // plane normal's z component is cos(i). The horizontal component is split
    // into local radial/east terms and constrained to be perpendicular to SF.
    let inc = TRACK_INCLINATION_DEG.to_radians();
    let normal_z = inc.cos();
    let radial_component = -normal_z * lat.tan();
    let east_component = (inc.sin().powi(2) - radial_component.powi(2))
        .max(0.0)
        .sqrt();
    let radial = [cos_lon, sin_lon, 0.0];
    let east = [-sin_lon, cos_lon, 0.0];
    let normal = normalize([
        radial_component * radial[0] + east_component * east[0],
        radial_component * radial[1] + east_component * east[1],
        normal_z,
    ]);
    let tangent = normalize(cross(normal, start));
    (start, tangent)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / norm, v[1] / norm, v[2] / norm]
}

fn normalize_lon_deg(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn central_angle_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
        let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
        let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
        let dlat = lat2 - lat1;
        let dlon = (lon2 - lon1 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        (2.0 * h.sqrt().asin()).to_degrees()
    }

    fn geographic_vector(point: (f64, f64)) -> [f64; 3] {
        let (lat, lon) = (point.0.to_radians(), point.1.to_radians());
        [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
    }

    #[test]
    fn general_track_formula_converges_to_san_francisco() {
        // A positive epsilon bypasses observer_at's exact-phase fast path.
        let near_start = observer_at(SIM_EPOCH_MS + 0.001);
        assert!(central_angle_deg(near_start, (SF_LAT_DEG, SF_LON_DEG)) < 1e-8);
    }

    #[test]
    fn observer_track_is_periodic_away_from_fast_path() {
        let phase = TRACK_PERIOD_MS * 0.371;
        let first = observer_at(SIM_EPOCH_MS + phase);
        let later = observer_at(SIM_EPOCH_MS + phase + TRACK_PERIOD_MS);
        // 1e-8° is 0.04 milliarcseconds. The bound is float rounding in the
        // phase modulo, not physics: TRACK_PERIOD_MS is a sidereal fraction,
        // not a round number of milliseconds.
        assert!(central_angle_deg(first, later) < 1e-8);
    }

    #[test]
    fn every_track_point_stays_in_one_great_circle_plane() {
        let (start, tangent) = track_basis();
        let normal = cross(start, tangent);
        for i in 1..64 {
            let point = geographic_vector(observer_at(
                SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(i) / 64.0,
            ));
            let plane_error: f64 = normal.iter().zip(point).map(|(a, b)| a * b).sum();
            assert!(plane_error.abs() < 1e-12, "phase {i}: {plane_error}");
        }
    }

    #[test]
    fn observer_moves_at_equal_angular_speed() {
        let step = TRACK_PERIOD_MS / 16.0;
        let expected = 360.0 / 16.0;
        for i in 0..16 {
            // Offset avoids making the first segment depend on the fast path.
            let a = observer_at(SIM_EPOCH_MS + 123.0 + step * f64::from(i));
            let b = observer_at(SIM_EPOCH_MS + 123.0 + step * f64::from(i + 1));
            let angle = central_angle_deg(a, b);
            assert!((angle - expected).abs() < 1e-8, "segment {i}: {angle}");
        }
    }

    #[test]
    fn observer_track_reaches_documented_latitude_spread() {
        let mut min_lat = 90.0;
        let mut max_lat = -90.0;
        for i in 0..720 {
            let (lat, _) = observer_at(SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(i) / 720.0);
            min_lat = f64::min(min_lat, lat);
            max_lat = f64::max(max_lat, lat);
        }
        assert!(max_lat > 62.9, "max latitude {max_lat}");
        assert!(min_lat < -62.9, "min latitude {min_lat}");
    }

    #[test]
    fn observer_track_is_continuous_at_small_steps() {
        let step = TRACK_PERIOD_MS / 720.0;
        let mut prev = observer_at(SIM_EPOCH_MS);
        for i in 1..=720 {
            let next = observer_at(SIM_EPOCH_MS + step * f64::from(i));
            let angle = central_angle_deg(prev, next);
            assert!(
                angle < 0.51,
                "step {i} jumped {angle}° from {prev:?} to {next:?}"
            );
            prev = next;
        }
    }
}
