//! Where the real sky is, at a given instant, from a given place on Earth.
//!
//! The catalog holds J2000/ICRS unit vectors. Getting one onto the screen is
//! two rotations: precession from J2000 to the mean equinox of date, then the
//! horizon transform for the observer's latitude and local sidereal time.
//! Earth's rotation is composed exactly once, here — the observer track
//! deliberately returns Earth-*fixed* coordinates so it cannot double-count it.
//!
//! The clock runs 60x wall time so the drift is perceptible, but the simulated
//! *date* is pulled back every full rotation instead of running away: the
//! accumulated lead is taken modulo one sidereal day, and GMST has exactly that
//! period, so the sky either side of the seam is identical to the bit. Nothing
//! on screen moves; only the date resets. This is tonight's sky spun fast, not
//! an invented decade's.

/// Fixed simulation epoch, 2026-01-01T00:00:00Z. Sky state is a pure function
/// of (server time, this constant, [`SPEED`]) and nothing else.
pub const SIM_EPOCH_MS: f64 = 1_767_225_600_000.0;
pub const SPEED: f64 = 60.0;
pub const SF_LAT_DEG: f64 = 37.7749;
pub const SF_LON_DEG: f64 = -122.4194;

/// One sidereal day, derived from [`gmst_rad`]'s own rotation rate rather than
/// quoted from a table, so the resync lands on an exact GMST period.
pub const SIDEREAL_DAY_MS: f64 = 86_400_000.0 * std::f64::consts::TAU / 6.300_388_098_984_89;

/// Focal length of the zenith-looking projection, shared by both GPU passes.
pub const FOCAL: f32 = 0.8391;

/// Accelerated simulation time for a wall-clock unix time.
#[must_use]
pub fn sim_time_ms(now_ms: f64) -> f64 {
    now_ms + ((now_ms - SIM_EPOCH_MS) * (SPEED - 1.0)).rem_euclid(SIDEREAL_DAY_MS)
}

/// Reconstruct server wall time from a client timestamp plus the offset sampled
/// when the bundle mounted. Sampling at mount rather than after initialisation
/// means asset fetch, decode and GPU latency cannot make the sky stale.
#[must_use]
pub fn synced_sim_time_ms(server_epoch_ms: f64, client_mount_ms: f64, client_now_ms: f64) -> f64 {
    sim_time_ms(client_now_ms + server_epoch_ms - client_mount_ms)
}

/// Greenwich mean sidereal time in radians. Low-precision series: arcminute
/// accuracy is orders of magnitude beyond what a background sky needs.
#[must_use]
pub fn gmst_rad(unix_ms: f64) -> f64 {
    let days_since_j2000 = unix_ms / 86_400_000.0 - 10_957.5;
    (4.894_961_212_823_058 + 6.300_388_098_984_89 * days_since_j2000)
        .rem_euclid(std::f64::consts::TAU)
}

/// Column-major mat3 taking J2000/ICRS unit vectors into the observer's
/// mean-of-date view frame: x = screen-right, y = north (screen-up),
/// z = zenith (forward).
///
/// Looking *up* with north at the top puts east on the LEFT, because
/// screen-right is forward x up = -east. That is the planetarium orientation;
/// a +east basis would render the mirrored "globe seen from outside" sky.
#[must_use]
pub fn view_matrix(unix_ms: f64, lat_deg: f64, lon_deg: f64) -> [f32; 9] {
    let lst = gmst_rad(unix_ms) + lon_deg.to_radians();
    let (sin_l, cos_l) = lst.sin_cos();
    let (sin_p, cos_p) = lat_deg.to_radians().sin_cos();

    let horizon = [
        [sin_l, -cos_l, 0.0],
        [-sin_p * cos_l, -sin_p * sin_l, cos_p],
        [cos_p * cos_l, cos_p * sin_l, sin_p],
    ];
    let view = mat_mul(horizon, precession_at(unix_ms));

    let mut out = [0.0f32; 9];
    for row in 0..3 {
        for col in 0..3 {
            out[col * 3 + row] = view[row][col] as f32;
        }
    }
    out
}

/// IAU 1976/Lieske rotation from J2000 to the mean equator and equinox of date.
/// `t` is Julian centuries TT from J2000; the axis matrices use the passive
/// convention, so this is exactly `Rz(-z) . Ry(theta) . Rz(-zeta)`.
fn precession_matrix(t: f64) -> [[f64; 3]; 3] {
    let arcsec = std::f64::consts::PI / (180.0 * 3_600.0);
    let zeta = (2_306.218_1 * t + 0.301_88 * t.powi(2) + 0.017_998 * t.powi(3)) * arcsec;
    let z = (2_306.218_1 * t + 1.094_68 * t.powi(2) + 0.018_203 * t.powi(3)) * arcsec;
    let theta = (2_004.310_9 * t - 0.426_65 * t.powi(2) - 0.041_833 * t.powi(3)) * arcsec;

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

fn precession_at(unix_ms: f64) -> [[f64; 3]; 3] {
    // TT-UTC is 69.184 s today. Future leap seconds are unknowable, and even a
    // minute of error moves these century-scale angles by under a milliarcsec.
    const TT_MINUS_UTC_MS: f64 = 69_184.0;
    const J2000_UNIX_MS: f64 = 946_728_000_000.0;
    const JULIAN_CENTURY_MS: f64 = 36_525.0 * 86_400_000.0;
    precession_matrix((unix_ms + TT_MINUS_UTC_MS - J2000_UNIX_MS) / JULIAN_CENTURY_MS)
}

fn mat_mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (row, target) in out.iter_mut().enumerate() {
        for (col, cell) in target.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[row][k] * b[k][col]).sum();
        }
    }
    out
}

/// Unit vector for a latitude/longitude on a sphere, in the same frame the
/// track and the globe both use.
#[must_use]
pub fn unit_vector(lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let (sin_lat, cos_lat) = lat_deg.to_radians().sin_cos();
    let (sin_lon, cos_lon) = lon_deg.to_radians().sin_cos();
    [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat]
}

/// The inverse of [`unit_vector`], with longitude normalised to `[-180, 180)`.
#[must_use]
pub fn lat_lon(v: [f64; 3]) -> (f64, f64) {
    (
        v[2].clamp(-1.0, 1.0).asin().to_degrees(),
        normalize_lon_deg(v[1].atan2(v[0]).to_degrees()),
    )
}

#[must_use]
pub fn normalize_lon_deg(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_sidereal_day_constant_agrees_with_the_rotation_rate_it_is_derived_from() {
        assert!(
            (SIDEREAL_DAY_MS - 86_164_090.5).abs() < 1.0,
            "{SIDEREAL_DAY_MS}"
        );
    }

    #[test]
    fn gmst_matches_its_published_value_at_j2000() {
        let deg = gmst_rad(946_728_000_000.0).to_degrees();
        assert!((deg - 280.4606).abs() < 0.01, "got {deg}");
    }

    #[test]
    fn the_simulation_clock_runs_exactly_sixty_times_wall_clock() {
        let delta = 12_345.0;
        assert_eq!(
            sim_time_ms(SIM_EPOCH_MS + delta) - sim_time_ms(SIM_EPOCH_MS),
            SPEED * delta
        );
    }

    #[test]
    fn the_simulated_date_never_runs_away_from_the_real_one() {
        // The bug this replaces: an unbounded 60x clock reached 2061 within
        // months of launch. Sample a real year of wall time.
        let year_ms = 365.25 * 86_400_000.0;
        for i in 0..2_000 {
            let now = SIM_EPOCH_MS + year_ms * f64::from(i) / 2_000.0;
            let lead = sim_time_ms(now) - now;
            assert!(
                (0.0..SIDEREAL_DAY_MS).contains(&lead),
                "sample {i} led by {lead} ms"
            );
        }
    }

    #[test]
    fn stepping_back_one_sidereal_day_leaves_the_sky_identical() {
        // The invariant the resync rests on. Only precession survives the
        // subtraction, and over one day that is 0.06 arcsec.
        for sample in 0..8 {
            let t = SIM_EPOCH_MS + 987_654_321.0 * f64::from(sample);
            let now = view_matrix(t, SF_LAT_DEG, SF_LON_DEG);
            let earlier = view_matrix(t - SIDEREAL_DAY_MS, SF_LAT_DEG, SF_LON_DEG);
            for (i, (x, y)) in now.iter().zip(earlier.iter()).enumerate() {
                assert!(
                    (x - y).abs() < 1e-5,
                    "sample {sample} element {i}: {x} vs {y}"
                );
            }
        }
    }

    #[test]
    fn precession_is_identity_at_j2000_and_a_proper_rotation_away_from_it() {
        assert_eq!(
            precession_matrix(0.0),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
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
    fn the_view_basis_is_orthonormal_and_deliberately_left_handed() {
        let rows = rows(view_matrix(SIM_EPOCH_MS, SF_LAT_DEG, SF_LON_DEG));
        for (i, row) in rows.iter().enumerate() {
            assert!((dot(*row, *row) - 1.0).abs() < 1e-6, "row {i}");
            for (j, other) in rows.iter().enumerate().skip(i + 1) {
                assert!(dot(*row, *other).abs() < 1e-6, "rows {i},{j}");
            }
        }
        assert!((determinant(rows) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn earth_rotation_is_composed_exactly_once() {
        // At J2000 on Greenwich's equator, a star whose RA equals GMST is at
        // zenith. Applying GMST in the observer track as well would fail this.
        let t = 946_727_930_816.0;
        let ra = gmst_rad(t);
        let star = [ra.cos(), ra.sin(), 0.0];
        let m = rows(view_matrix(t, 0.0, 0.0));
        assert!(dot(m[0], star).abs() < 1e-6);
        assert!(dot(m[1], star).abs() < 1e-6);
        assert!((dot(m[2], star) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_star_east_of_zenith_renders_on_the_screen_left() {
        let t = SIM_EPOCH_MS;
        let lst = gmst_rad(t) + SF_LON_DEG.to_radians();
        let (ra, dec) = (lst + 0.05, SF_LAT_DEG.to_radians());
        let star = unit_vector(dec.to_degrees(), ra.to_degrees());
        let m = view_matrix(t, SF_LAT_DEG, SF_LON_DEG);
        let x = f64::from(m[0]) * star[0] + f64::from(m[3]) * star[1] + f64::from(m[6]) * star[2];
        assert!(x < 0.0, "east-of-zenith star at screen x = {x}");
    }

    #[test]
    fn a_known_star_lands_where_an_independent_calculation_says_it_does() {
        // Regulus (HIP 49669), Hipparcos new reduction J2000. The expected
        // altitude and azimuth were computed with the scalar precession and
        // horizon equations in Meeus, Astronomical Algorithms ch. 21 and 13 —
        // not with this matrix path.
        let check = |unix_ms: f64, expected_alt: f64, expected_az: f64| {
            let star = unit_vector(11.967_195_190_324_96, 152.093_580_421_387_7);
            let m = rows(view_matrix(unix_ms, SF_LAT_DEG, SF_LON_DEG));
            let altitude = dot(m[2], star).asin().to_degrees();
            let azimuth = (-dot(m[0], star))
                .atan2(dot(m[1], star))
                .rem_euclid(std::f64::consts::TAU)
                .to_degrees();
            assert!((altitude - expected_alt).abs() < 2e-5, "alt {altitude}");
            assert!((azimuth - expected_az).abs() < 2e-5, "az {azimuth}");
        };
        check(1_785_888_000_000.0, 46.841_404_782, 243.442_787_112);
        check(4_102_444_800_000.0, -40.539_846_249, 6.311_664_961);
    }

    #[test]
    fn lat_lon_round_trips_through_the_unit_sphere_including_the_antimeridian() {
        for (lat, lon) in [
            (37.77, -122.42),
            (-33.87, 151.21),
            (0.0, 180.0),
            (64.7, -179.0),
        ] {
            let (back_lat, back_lon) = lat_lon(unit_vector(lat, lon));
            assert!((back_lat - lat).abs() < 1e-9, "{lat} -> {back_lat}");
            assert!(
                (normalize_lon_deg(back_lon - lon)).abs() < 1e-9,
                "{lon} -> {back_lon}"
            );
        }
    }
}
