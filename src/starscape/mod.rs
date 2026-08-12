//! Real-sky starscape (platform-scope §Starscape). Renders the actual
//! Gaia/Hipparcos sky from a traveling Earth-surface observer, with Earth
//! rotation accelerated 60× off server-synced time so every client sees the
//! identical sky and the drift is perceptible (full revolution ≈ 24 min).
//! The simulated date resyncs to real time once per rotation — see
//! `sim_time_ms` for why that seam is invisible — so this is tonight's sky
//! spun fast, not an invented decade's. IAU 1976 precession still rotates the
//! catalog's J2000/ICRS vectors to mean-of-date before Earth rotation.
//!
//! The observer track is a 63°-inclined great circle in Earth-fixed geographic
//! coordinates, starting exactly at San Francisco and completing one lap every
//! half sidereal day of simulated time — chosen so the resync is a whole
//! number of laps and the ground track is continuous across it too.
//! `observer_at` returns only the surface subpoint; Earth rotation is applied
//! once in `view_matrix` through GMST + longitude when the inertial star
//! vectors are transformed into the local horizon frame.
//!
//! Behind the point stars, a second pass paints the Milky Way from
//! `public/sky/milkyway.webp` — Gaia star counts binned onto an equal-area
//! grid (tools/mwcat.py), not procedural noise. That map is fixed on the
//! celestial sphere: Earth is nine orders of magnitude too small a baseline to
//! shift it, so what changes with the observer's position is only which part
//! clears the horizon and how much atmosphere it shines through.
//!
//! This slice: traveling viewpoint, separately fetched GeoNames nearest-city
//! lookup, single bright-star asset, Milky Way map, WebGL points, and
//! grounding label. Deferred (docs/contract.md): clickable stars, LOD tiles.

use leptos::prelude::*;

#[cfg(feature = "hydrate")]
mod bridge;
mod cities;
mod globe;
mod label;
#[cfg(feature = "hydrate")]
mod render;
#[cfg(any(feature = "hydrate", test))]
mod shaders;
#[cfg(any(feature = "hydrate", test))]
mod star_asset;
#[cfg(feature = "hydrate")]
mod telemetry;
mod track;
// Only `render` reads this, and `render` is browser-only, so compiling it into
// the `ssr` test build leaves every item in it dead — five warnings that the
// `-D warnings` CI gate turns into a failure.
#[cfg(feature = "hydrate")]
mod tuning;

pub use cities::{City, CityCatalog, EARTH_MEAN_RADIUS_KM};
pub use globe::MiniGlobe;
pub use label::CityLabel;
pub use track::{TRACK_INCLINATION_DEG, TRACK_PERIOD_MS, observer_at};

/// Fixed simulation epoch: 2026-01-01T00:00:00Z. Sky state is a pure
/// function of (server time, this constant, SPEED) — nothing else.
pub const SIM_EPOCH_MS: f64 = 1_767_225_600_000.0;
pub const SPEED: f64 = 60.0;
pub const SF_LAT_DEG: f64 = 37.7749;
pub const SF_LON_DEG: f64 = -122.4194;

/// One sidereal day, derived from `gmst_rad`'s own rotation rate rather than
/// quoted from a table, so the resync below lands on an exact GMST period.
pub const SIDEREAL_DAY_MS: f64 = 86_400_000.0 * std::f64::consts::TAU / 6.300_388_098_984_89;

/// Accelerated simulation time for a given wall-clock unix time.
///
/// The sky runs 60× fast, but the *date* is pulled back to real time every
/// full rotation instead of running away (it used to reach 2061 by August
/// 2026, since a 60× clock ages 60 years per year). The trick is that the
/// accumulated lead is taken modulo exactly one sidereal day: GMST has that
/// period, so at the instant the lead wraps, the sky's orientation either
/// side of the seam is identical to the bit — nothing on screen moves. Only
/// the date resets, and precession over one day is 0.06 arcsec, which is
/// four orders of magnitude below a pixel.
///
/// Consequences worth stating plainly: the rate is still exactly 60× between
/// resyncs, the simulated date is never more than one day ahead of the real
/// one, and a resync lands every `SIDEREAL_DAY_MS / (SPEED - 1)` ≈ 24.3 real
/// minutes. So the sky overhead is tonight's sky, spun fast — not a
/// plausible-looking sky from an invented decade.
pub fn sim_time_ms(now_ms: f64) -> f64 {
    now_ms + ((now_ms - SIM_EPOCH_MS) * (SPEED - 1.0)).rem_euclid(SIDEREAL_DAY_MS)
}

/// Reconstruct server wall time from a client timestamp and the offset sampled
/// at island mount. Keeping mount time outside async initialization means asset
/// fetch/decode/GPU latency cannot make the simulated sky stale.
pub fn synced_sim_time_ms(server_epoch_ms: f64, client_mount_ms: f64, client_now_ms: f64) -> f64 {
    sim_time_ms(client_now_ms + server_epoch_ms - client_mount_ms)
}

/// Greenwich mean sidereal time in radians (low-precision series; arcminute
/// accuracy is far beyond what a background sky needs).
pub fn gmst_rad(unix_ms: f64) -> f64 {
    let days_since_j2000 = unix_ms / 86_400_000.0 - 10_957.5;
    let gmst = 4.894_961_212_823_058 + 6.300_388_098_984_89 * days_since_j2000;
    gmst.rem_euclid(std::f64::consts::TAU)
}

/// IAU 1976/Lieske rotation from J2000 to mean equator/equinox of date.
///
/// `t` is Julian centuries TT from J2000. The axis matrices use the passive
/// reference-frame convention, so this is exactly
/// `Rz(-z) · Ry(theta) · Rz(-zeta)` from the Lieske formulation.
fn precession_matrix(t: f64) -> [[f64; 3]; 3] {
    let arcsec_to_rad = std::f64::consts::PI / (180.0 * 3_600.0);
    let zeta = (2_306.218_1 * t + 0.301_88 * t.powi(2) + 0.017_998 * t.powi(3)) * arcsec_to_rad;
    let z = (2_306.218_1 * t + 1.094_68 * t.powi(2) + 0.018_203 * t.powi(3)) * arcsec_to_rad;
    let theta = (2_004.310_9 * t - 0.426_65 * t.powi(2) - 0.041_833 * t.powi(3)) * arcsec_to_rad;

    fn rz(angle: f64) -> [[f64; 3]; 3] {
        let (s, c) = angle.sin_cos();
        [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]]
    }
    fn ry(angle: f64) -> [[f64; 3]; 3] {
        let (s, c) = angle.sin_cos();
        [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
    }

    mat_mul(mat_mul(rz(-z), ry(theta)), rz(-zeta))
}

fn mat_mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = (0..3).map(|k| a[row][k] * b[k][col]).sum();
        }
    }
    out
}

fn precession_at(unix_ms: f64) -> [[f64; 3]; 3] {
    // TT−UTC is currently 69.184 s. Future leap seconds are unknowable; even
    // a minute of error changes these century-scale angles by <0.001 arcsec.
    const TT_MINUS_UTC_MS: f64 = 69_184.0;
    const J2000_UNIX_MS: f64 = 946_728_000_000.0;
    const JULIAN_CENTURY_MS: f64 = 36_525.0 * 86_400_000.0;
    let t = (unix_ms + TT_MINUS_UTC_MS - J2000_UNIX_MS) / JULIAN_CENTURY_MS;
    precession_matrix(t)
}

/// Column-major mat3 taking J2000/ICRS catalog unit vectors to the observer's
/// mean-of-date view frame: x=screen-right, y=north(screen-up),
/// z=zenith(forward). Looking UP with north at the top puts east on the LEFT
/// (screen-right = forward × up = −east) — the standard planetarium
/// orientation; +east here would render the mirrored "star globe seen from
/// outside" sky. Precession is applied before the horizon transform.
pub fn view_matrix(unix_ms: f64, lat_deg: f64, lon_deg: f64) -> [f32; 9] {
    let lst = gmst_rad(unix_ms) + lon_deg.to_radians();
    let (sin_l, cos_l) = lst.sin_cos();
    let (sin_p, cos_p) = lat_deg.to_radians().sin_cos();

    let horizon = [
        [sin_l, -cos_l, 0.0], // right = −east: east renders screen-left
        [-sin_p * cos_l, -sin_p * sin_l, cos_p],
        [cos_p * cos_l, cos_p * sin_l, sin_p],
    ];
    let view = mat_mul(horizon, precession_at(unix_ms));

    // Rows right/north/zenith -> column-major for uniformMatrix3fv.
    [
        view[0][0] as f32,
        view[1][0] as f32,
        view[2][0] as f32,
        view[0][1] as f32,
        view[1][1] as f32,
        view[2][1] as f32,
        view[0][2] as f32,
        view[1][2] as f32,
        view[2][2] as f32,
    ]
}

/// The starscape island: a fixed full-viewport canvas behind the page.
/// Progressive enhancement — until WebGL is live the CSS starfield shows;
/// on success the root gains `starscape-active` which hides it.
///
/// `epoch_ms` is public bootstrap data (server clock at render) used only
/// to cancel client clock skew.
#[island]
pub fn Starscape(epoch_ms: f64) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        if let Some(canvas) = canvas_ref.get() {
            telemetry::install();
            bridge::install_track_api(epoch_ms);
            // Defer the heavy WebGL + catalog work until after first paint.
            render::start_deferred(canvas, epoch_ms);
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = epoch_ms;
    });

    view! { <canvas class="starscape" node_ref=canvas_ref aria-hidden="true"></canvas> }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidereal_day_constant_matches_the_published_value() {
        // Derived from gmst_rad's rotation rate, so it must land on the
        // textbook 23h56m04.09s — otherwise the two have drifted apart and
        // the clock resync stops being a whole GMST period.
        assert!(
            (SIDEREAL_DAY_MS - 86_164_090.5).abs() < 1.0,
            "{SIDEREAL_DAY_MS}"
        );
    }

    #[test]
    fn gmst_matches_j2000_reference() {
        // At J2000.0 (2000-01-01T12:00Z, unix 946728000000) GMST ≈ 280.4606°.
        let deg = gmst_rad(946_728_000_000.0).to_degrees();
        assert!((deg - 280.4606).abs() < 0.01, "got {deg}");
    }

    #[test]
    fn gmst_period_is_sidereal_day() {
        let t0 = SIM_EPOCH_MS;
        let a = gmst_rad(t0);
        let b = gmst_rad(t0 + SIDEREAL_DAY_MS);
        let diff = (a - b).abs();
        let wrapped = diff.min(std::f64::consts::TAU - diff);
        assert!(wrapped < 1e-3, "sidereal-day drift {wrapped}");
    }

    fn rows(m: [f32; 9]) -> [[f64; 3]; 3] {
        [
            [m[0] as f64, m[3] as f64, m[6] as f64],
            [m[1] as f64, m[4] as f64, m[7] as f64],
            [m[2] as f64, m[5] as f64, m[8] as f64],
        ]
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    fn determinant(m: [[f64; 3]; 3]) -> f64 {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    #[test]
    fn simulation_clock_advances_exactly_sixty_times_wall_clock() {
        let wall_delta = 12_345.0;
        assert_eq!(
            sim_time_ms(SIM_EPOCH_MS + wall_delta) - sim_time_ms(SIM_EPOCH_MS),
            SPEED * wall_delta
        );
    }

    /// The real-time interval between two clock resyncs: the 59× lead the
    /// simulation builds over wall time needs this long to accumulate one
    /// whole sidereal day.
    const RESYNC_PERIOD_MS: f64 = SIDEREAL_DAY_MS / (SPEED - 1.0);

    #[test]
    fn simulated_date_never_runs_away_from_the_real_one() {
        // The bug this replaces: an unbounded 60× clock reached the year 2061
        // within months of launch. Sample a real year of wall time.
        let year_ms = 365.25 * 86_400_000.0;
        for i in 0..2_000 {
            let now = SIM_EPOCH_MS + year_ms * f64::from(i) / 2_000.0;
            let lead = sim_time_ms(now) - now;
            assert!(
                (0.0..SIDEREAL_DAY_MS).contains(&lead),
                "sample {i}: simulated clock led real time by {lead} ms"
            );
        }
    }

    #[test]
    fn the_sky_is_unchanged_across_a_clock_resync() {
        // The whole point of taking the lead modulo a sidereal day: the
        // simulated date jumps back a day at the seam, and the rendered sky
        // must not move at all. Straddle the first resync by a millisecond.
        let before = SIM_EPOCH_MS + RESYNC_PERIOD_MS - 1.0;
        let after = SIM_EPOCH_MS + RESYNC_PERIOD_MS + 1.0;
        assert!(
            sim_time_ms(after) < sim_time_ms(before),
            "expected the simulated date to step back at the seam"
        );

        // The observer does not teleport: 2 ms of real time is 120 ms
        // simulated, or 0.0005° of Earth rotation, and the lap count is whole.
        let (lat_a, lon_a) = observer_at(sim_time_ms(before));
        let (lat_b, lon_b) = observer_at(sim_time_ms(after));
        assert!((lat_a - lat_b).abs() < 1e-3, "{lat_a} vs {lat_b}");
        assert!((lon_a - lon_b).abs() < 1e-3, "{lon_a} vs {lon_b}");
    }

    #[test]
    fn stepping_the_clock_back_one_sidereal_day_leaves_the_sky_identical() {
        // This is the invariant the resync rests on, isolated from elapsed
        // time: the same instant rendered a sidereal day earlier must give the
        // same view. Only precession survives the subtraction, and over one
        // day that is 0.06 arcsec — far below any tolerance a pixel cares
        // about. If SIDEREAL_DAY_MS and gmst_rad's rate ever disagree, the
        // seam becomes a visible jump and this fails first.
        for sample in 0..8 {
            let t = SIM_EPOCH_MS + 987_654_321.0 * f64::from(sample);
            let (lat, lon) = observer_at(t);
            let now = view_matrix(t, lat, lon);
            let a_day_earlier = view_matrix(t - SIDEREAL_DAY_MS, lat, lon);
            for (i, (x, y)) in now.iter().zip(a_day_earlier.iter()).enumerate() {
                assert!(
                    (x - y).abs() < 1e-5,
                    "sample {sample} element {i}: {x} vs {y}"
                );
            }
        }
    }

    #[test]
    fn track_period_divides_the_resync_so_the_observer_does_not_teleport() {
        let laps = SIDEREAL_DAY_MS / TRACK_PERIOD_MS;
        assert!((laps - laps.round()).abs() < 1e-9, "{laps} laps per resync");
    }

    #[test]
    fn mount_sampled_clock_offset_ignores_five_second_init_delay() {
        let server = 1_800_000_000_000.0;
        let mount = 1_799_999_999_750.0;
        let after_init = mount + 5_000.0;
        assert_eq!(
            synced_sim_time_ms(server, mount, after_init),
            sim_time_ms(server + 5_000.0)
        );
    }

    #[test]
    fn precession_is_identity_at_j2000() {
        assert_eq!(
            precession_matrix(0.0),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
    }

    #[test]
    fn precession_is_a_proper_orthonormal_rotation() {
        let p = precession_matrix(1.25);
        for i in 0..3 {
            assert!((dot(p[i], p[i]) - 1.0).abs() < 1e-12);
            for j in i + 1..3 {
                assert!(dot(p[i], p[j]).abs() < 1e-12);
            }
        }
        assert!((determinant(p) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn view_matrix_is_orthonormal_with_intentional_negative_handedness() {
        let rows = rows(view_matrix(SIM_EPOCH_MS, SF_LAT_DEG, SF_LON_DEG));
        for (i, row) in rows.iter().enumerate() {
            assert!((dot(*row, *row) - 1.0).abs() < 1e-6, "row {i}");
            for (j, other) in rows.iter().enumerate().skip(i + 1) {
                assert!(dot(*row, *other).abs() < 1e-6, "rows {i},{j}");
            }
        }
        assert!((determinant(rows) + 1.0).abs() < 1e-6);
    }

    fn assert_hip_49669_alt_az(unix_ms: f64, expected_alt: f64, expected_az: f64) {
        // Regulus (HIP 49669), Hipparcos new reduction J2000 coordinates.
        // Expected fixtures were calculated independently with the scalar
        // precession and horizon equations in Jean Meeus, Astronomical
        // Algorithms (2nd ed.), chapters 21 and 13—not this matrix code path.
        let ra = 152.093_580_421_387_7_f64.to_radians();
        let dec = 11.967_195_190_324_96_f64.to_radians();
        let star = [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()];
        let m = rows(view_matrix(unix_ms, SF_LAT_DEG, SF_LON_DEG));
        let right = dot(m[0], star);
        let north = dot(m[1], star);
        let up = dot(m[2], star);
        let altitude = up.asin().to_degrees();
        let azimuth = (-right)
            .atan2(north)
            .rem_euclid(std::f64::consts::TAU)
            .to_degrees();
        assert!((altitude - expected_alt).abs() < 2e-5, "alt {altitude}");
        assert!((azimuth - expected_az).abs() < 2e-5, "az {azimuth}");
    }

    #[test]
    fn named_star_fixture_at_2026_epoch_includes_precession() {
        assert_hip_49669_alt_az(1_785_888_000_000.0, 46.841_404_782, 243.442_787_112);
    }

    #[test]
    fn named_star_fixture_at_2100_epoch_includes_accumulated_precession() {
        assert_hip_49669_alt_az(4_102_444_800_000.0, -40.539_846_249, 6.311_664_961);
    }

    #[test]
    fn earth_rotation_is_composed_exactly_once() {
        // At J2000 and Greenwich's equator, a star whose RA equals GMST is
        // exactly at zenith. Applying GMST in the track as well would fail.
        let t = 946_727_930_816.0; // TT J2000 expressed on the Unix UTC scale
        let ra = gmst_rad(t);
        let star = [ra.cos(), ra.sin(), 0.0];
        let m = rows(view_matrix(t, 0.0, 0.0));
        assert!(dot(m[0], star).abs() < 1e-6);
        assert!(dot(m[1], star).abs() < 1e-6);
        assert!((dot(m[2], star) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn observer_track_sweeps_a_variety_of_grounding_cities_over_one_lap() {
        // Falsifies a track/lookup regression that would leave the
        // grounding label reporting one city (or none) for an entire lap
        // instead of a plausible sweep of places at a 63°-inclined orbit.
        let catalog = CityCatalog::parse(include_bytes!("../../public/cities/cities.bin").to_vec())
            .expect("valid city fixture");
        let mut names = std::collections::HashSet::new();
        for i in 0..144 {
            let t = SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(i) / 144.0;
            let (lat, lon) = observer_at(t);
            if let Some(city) = catalog.nearest(lat, lon) {
                names.insert(city.name);
            }
        }
        assert!(
            names.len() >= 10,
            "expected a varied city sequence over one lap, got {names:?}"
        );
    }

    #[test]
    fn east_of_zenith_renders_screen_left() {
        // A star at the observer's declination, slightly larger RA than the
        // local sidereal time, sits just east of zenith. Looking up with
        // north at screen-top, east must be on the LEFT (negative x).
        let t = SIM_EPOCH_MS;
        let lst = gmst_rad(t) + SF_LON_DEG.to_radians();
        let (ra, dec) = (lst + 0.05, SF_LAT_DEG.to_radians());
        let star = [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()];
        let m = view_matrix(t, SF_LAT_DEG, SF_LON_DEG);
        let x = m[0] as f64 * star[0] + m[3] as f64 * star[1] + m[6] as f64 * star[2];
        assert!(x < 0.0, "east-of-zenith star at screen x = {x}");
    }
}
