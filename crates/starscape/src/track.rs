//! The travelling observer: a great circle over the rotating Earth.
//!
//! This is a route in Earth-*fixed* geographic coordinates, not an inertial
//! orbit. [`crate::sky::view_matrix`] composes the longitude returned here with
//! GMST; applying Earth rotation in this module as well would double-count it.
//!
//! The circle is inclined 63 degrees, which is what makes it pass through San
//! Francisco while reaching a +/-63 degree latitude band without dwelling near
//! a pole. One lap is exactly half a sidereal day of simulated time — an exact
//! divisor of the clock's resync period, so the observer's position is
//! continuous across that seam just as the sky's orientation is. A period that
//! did not divide it would teleport the viewer mid-flight.

use crate::sky::{SF_LAT_DEG, SF_LON_DEG, SIDEREAL_DAY_MS, SIM_EPOCH_MS, lat_lon, unit_vector};

pub const TRACK_INCLINATION_DEG: f64 = 63.0;
pub const TRACK_PERIOD_MS: f64 = SIDEREAL_DAY_MS / 2.0;

/// The Earth-fixed observer subpoint at a simulation time, `(lat, lon)` in
/// degrees. At [`SIM_EPOCH_MS`] the result is exactly San Francisco.
#[must_use]
pub fn observer_at(sim_ms: f64) -> (f64, f64) {
    let elapsed = (sim_ms - SIM_EPOCH_MS).rem_euclid(TRACK_PERIOD_MS);
    if elapsed == 0.0 {
        return (SF_LAT_DEG, SF_LON_DEG);
    }
    let (start, tangent) = basis();
    let (sin, cos) = (std::f64::consts::TAU * elapsed / TRACK_PERIOD_MS).sin_cos();
    lat_lon(std::array::from_fn(|i| start[i] * cos + tangent[i] * sin))
}

/// Start point and the unit tangent at it — the two vectors that span the
/// great-circle plane.
fn basis() -> ([f64; 3], [f64; 3]) {
    let lat = SF_LAT_DEG.to_radians();
    let lon = SF_LON_DEG.to_radians();
    let (sin_lon, cos_lon) = lon.sin_cos();
    let start = unit_vector(SF_LAT_DEG, SF_LON_DEG);

    // For inclination i the plane normal's z component is cos(i). Split the
    // horizontal part into local radial and east terms and constrain it to be
    // perpendicular to the start point.
    let inc = TRACK_INCLINATION_DEG.to_radians();
    let normal_z = inc.cos();
    let radial = -normal_z * lat.tan();
    let east = (inc.sin().powi(2) - radial.powi(2)).max(0.0).sqrt();
    let normal = normalize([
        radial * cos_lon - east * sin_lon,
        radial * sin_lon + east * cos_lon,
        normal_z,
    ]);
    (start, normalize(cross(normal, start)))
}

/// Shortest-path interpolation between two points on the sphere. Used when the
/// observer is moved by hand: a linear blend of latitude and longitude would
/// cut across the pole and jump the antimeridian.
#[must_use]
pub fn great_circle_lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    let (av, bv) = (unit_vector(a.0, a.1), unit_vector(b.0, b.1));
    let dot: f64 = av
        .iter()
        .zip(bv)
        .map(|(x, y)| x * y)
        .sum::<f64>()
        .clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle < 1e-9 {
        return a;
    }
    let scale = angle.sin();
    let (wa, wb) = (((1.0 - t) * angle).sin() / scale, (t * angle).sin() / scale);
    lat_lon(normalize(std::array::from_fn(|i| av[i] * wa + bv[i] * wb)))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn central_angle_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
        let (av, bv) = (unit_vector(a.0, a.1), unit_vector(b.0, b.1));
        av.iter()
            .zip(bv)
            .map(|(x, y)| x * y)
            .sum::<f64>()
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }

    #[test]
    fn the_general_formula_converges_on_san_francisco_at_the_epoch() {
        // A positive epsilon bypasses the exact-phase shortcut.
        let near_start = observer_at(SIM_EPOCH_MS + 0.001);
        assert!(central_angle_deg(near_start, (SF_LAT_DEG, SF_LON_DEG)) < 1e-6);
    }

    #[test]
    fn every_point_of_the_track_lies_in_one_great_circle_plane() {
        let (start, tangent) = basis();
        let normal = cross(start, tangent);
        for i in 1..64 {
            let point = observer_at(SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(i) / 64.0);
            let error: f64 = normal
                .iter()
                .zip(unit_vector(point.0, point.1))
                .map(|(a, b)| a * b)
                .sum();
            assert!(error.abs() < 1e-12, "phase {i}: {error}");
        }
    }

    #[test]
    fn the_observer_moves_at_a_constant_angular_speed() {
        let step = TRACK_PERIOD_MS / 16.0;
        for i in 0..16 {
            let a = observer_at(SIM_EPOCH_MS + 123.0 + step * f64::from(i));
            let b = observer_at(SIM_EPOCH_MS + 123.0 + step * f64::from(i + 1));
            assert!((central_angle_deg(a, b) - 22.5).abs() < 1e-6, "segment {i}");
        }
    }

    #[test]
    fn the_track_reaches_the_documented_latitude_spread() {
        let (mut min, mut max) = (90.0f64, -90.0f64);
        for i in 0..720 {
            let (lat, _) = observer_at(SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(i) / 720.0);
            min = min.min(lat);
            max = max.max(lat);
        }
        assert!(max > 62.9 && min < -62.9, "{min}..{max}");
    }

    #[test]
    fn the_lap_divides_the_clock_resync_so_the_observer_never_teleports() {
        let laps = SIDEREAL_DAY_MS / TRACK_PERIOD_MS;
        assert!((laps - laps.round()).abs() < 1e-9, "{laps} laps per resync");
    }

    #[test]
    fn a_manual_move_takes_the_shorter_way_round_the_antimeridian() {
        let midpoint = great_circle_lerp((0.0, 0.0), (0.0, 90.0), 0.5);
        assert!(midpoint.0.abs() < 1e-9);
        assert!((midpoint.1 - 45.0).abs() < 1e-9);

        let across = great_circle_lerp((0.0, 170.0), (0.0, -170.0), 0.5);
        assert!((across.1.abs() - 180.0).abs() < 1e-9, "{across:?}");
    }

    #[test]
    fn interpolating_onto_the_same_point_is_a_no_op_rather_than_a_division_by_zero() {
        let here = (37.77, -122.42);
        let (lat, lon) = great_circle_lerp(here, here, 0.5);
        assert!(lat.is_finite() && lon.is_finite());
        assert!((lat - here.0).abs() < 1e-9 && (lon - here.1).abs() < 1e-9);
    }
}
