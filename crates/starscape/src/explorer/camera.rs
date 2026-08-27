//! Where the atlas is looking, and how a sky direction becomes a pixel.
//!
//! The atlas works in the catalog's own J2000 frame rather than the landing's
//! horizon frame: every star it can draw — the bright catalog, the mid catalog
//! and the regional tiles — is stored as a J2000 unit vector, and converting
//! three million of them per frame to look at the same picture from a different
//! basis would be arithmetic for its own sake. The view *enters* through the
//! landing's zenith, so opening the atlas continues the sky the visitor was
//! already looking at; after that it is a sky map, not a horizon.

/// The tightest and widest fields of view. One degree is about four times the
/// full moon; 115° is a whole-sky glance.
pub const MIN_FOV_DEG: f64 = 1.0;
pub const MAX_FOV_DEG: f64 = 115.0;

/// One sidereal day in milliseconds — how fast the sky drifts through a fixed
/// view when nothing is being tracked.
const SIDEREAL_DAY_MS: f64 = 86_164_090.5;

/// The clock multiplier at the widest view. Zooming in slows time in
/// proportion, so a tight field does not smear while you look at it.
const MAX_RATE: f64 = 60.0;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Right ascension of the view centre, radians.
    pub ra: f64,
    /// Declination of the view centre, radians.
    pub dec: f64,
    /// Vertical field of view, degrees.
    pub fov: f64,
    /// Whether the view is held on a named star rather than drifting.
    pub tracking: bool,
}

impl Camera {
    /// A view centred on a direction in the catalog frame. The direction is
    /// normalised here rather than trusted: a caller that hands over a vector
    /// a thousandth off unit would otherwise get a view a thousandth off the
    /// star it named, which is a whole frame at one degree of field.
    #[must_use]
    pub fn looking_at(forward: [f64; 3], fov: f64, tracking: bool) -> Self {
        let norm = forward[0].hypot(forward[1]).hypot(forward[2]);
        let forward = if norm > 0.0 {
            [forward[0] / norm, forward[1] / norm, forward[2] / norm]
        } else {
            [0.0, 0.0, 1.0]
        };
        Self {
            ra: forward[1].atan2(forward[0]),
            dec: forward[2].clamp(-1.0, 1.0).asin(),
            fov: fov.clamp(MIN_FOV_DEG, MAX_FOV_DEG),
            tracking,
        }
    }

    #[must_use]
    pub fn forward(&self) -> [f64; 3] {
        let (sin_dec, cos_dec) = self.dec.sin_cos();
        let (sin_ra, cos_ra) = self.ra.sin_cos();
        [cos_dec * cos_ra, cos_dec * sin_ra, sin_dec]
    }

    /// How fast simulated time runs at this zoom. A one-degree field would
    /// otherwise sweep its own width in under a second.
    #[must_use]
    pub fn rate(&self) -> f64 {
        (MAX_RATE * self.fov / MAX_FOV_DEG).max(1.0)
    }

    /// Multiply the field of view, clamped. Returns the new field.
    pub fn zoom(&mut self, factor: f64) -> f64 {
        self.fov = (self.fov * factor).clamp(MIN_FOV_DEG, MAX_FOV_DEG);
        self.fov
    }

    /// Drag the sky by a pixel delta. Panning is a deliberate act, so it also
    /// releases any tracked star: the view is now the visitor's, not the
    /// catalog's.
    pub fn pan(&mut self, dx: f64, dy: f64, width: f64, height: f64) {
        self.tracking = false;
        let scale = self.fov.to_radians() * 1.6;
        if width > 0.0 {
            self.ra = (self.ra - dx * scale / width).rem_euclid(std::f64::consts::TAU);
        }
        if height > 0.0 {
            self.dec = (self.dec + dy * scale / height)
                .clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        }
    }

    /// Let the sky drift by `sim_dt_ms` of simulated time. A tracked star is
    /// held: the whole point of tracking it is that it does not wander off.
    pub fn drift(&mut self, sim_dt_ms: f64) {
        if self.tracking {
            return;
        }
        self.ra = (self.ra + sim_dt_ms * std::f64::consts::TAU / SIDEREAL_DAY_MS)
            .rem_euclid(std::f64::consts::TAU);
    }

    /// The per-frame projection for a viewport of `width` x `height` CSS pixels.
    #[must_use]
    pub fn projector(&self, width: f64, height: f64) -> Projector {
        let (sin_dec, cos_dec) = self.dec.sin_cos();
        let (sin_ra, cos_ra) = self.ra.sin_cos();
        Projector {
            forward: self.forward(),
            right: [-sin_ra, cos_ra, 0.0],
            up: [-sin_dec * cos_ra, -sin_dec * sin_ra, cos_dec],
            focal: 1.0 / (self.fov.to_radians() / 2.0).tan(),
            aspect: if height > 0.0 { width / height } else { 1.0 },
            width,
            height,
        }
    }
}

/// A frozen camera, ready to turn sky directions into pixels.
#[derive(Debug, Clone, Copy)]
pub struct Projector {
    pub forward: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub focal: f64,
    pub aspect: f64,
    pub width: f64,
    pub height: f64,
}

impl Projector {
    /// Where a unit direction lands, or `None` if it is behind the view or
    /// outside the frame. The margin is deliberate: a star just off-frame still
    /// contributes the glow of its own radius.
    #[must_use]
    pub fn project(&self, pos: [f32; 3]) -> Option<(f64, f64)> {
        let p = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
        let z = dot(p, self.forward);
        if z <= 0.0 {
            return None;
        }
        let x = dot(p, self.right) / z * self.focal / self.aspect;
        let y = dot(p, self.up) / z * self.focal;
        if x.abs() > 1.05 || y.abs() > 1.05 {
            return None;
        }
        Some(((x + 1.0) * self.width / 2.0, (1.0 - y) * self.height / 2.0))
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: [f64; 3]) -> [f32; 3] {
        let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [
            (v[0] / norm) as f32,
            (v[1] / norm) as f32,
            (v[2] / norm) as f32,
        ]
    }

    #[test]
    fn the_direction_the_view_was_opened_on_lands_in_the_middle_of_the_frame() {
        for forward in [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.3, 0.4, 0.866]] {
            let camera = Camera::looking_at(forward, 30.0, true);
            let (x, y) = camera
                .projector(1_440.0, 900.0)
                .project(unit(forward))
                .expect("the centre of the view is in the view");
            // A tolerance of a thousandth of a pixel: the catalog stores
            // directions as f32, and the rounding is worth more than that.
            assert!((x - 720.0).abs() < 1e-3, "{x}");
            assert!((y - 450.0).abs() < 1e-3, "{y}");
        }
    }

    #[test]
    fn a_star_behind_the_view_or_outside_the_frame_is_not_drawn() {
        let camera = Camera::looking_at([1.0, 0.0, 0.0], 20.0, true);
        let projector = camera.projector(1_440.0, 900.0);
        assert!(projector.project([-1.0, 0.0, 0.0]).is_none());
        assert!(projector.project(unit([0.0, 1.0, 0.0])).is_none());
        assert!(projector.project(unit([1.0, 0.05, 0.0])).is_some());
    }

    #[test]
    fn zooming_in_narrows_the_frame_and_both_ends_are_clamped() {
        let mut camera = Camera::looking_at([1.0, 0.0, 0.0], 115.0, false);
        let wide = camera.projector(1_000.0, 1_000.0).focal;
        camera.zoom(0.25);
        assert!(camera.projector(1_000.0, 1_000.0).focal > wide);
        for _ in 0..40 {
            camera.zoom(0.5);
        }
        assert_eq!(camera.fov, MIN_FOV_DEG);
        for _ in 0..40 {
            camera.zoom(2.0);
        }
        assert_eq!(camera.fov, MAX_FOV_DEG);
    }

    #[test]
    fn time_slows_in_proportion_to_the_zoom_and_never_stops() {
        let wide = Camera::looking_at([1.0, 0.0, 0.0], MAX_FOV_DEG, false);
        assert!((wide.rate() - MAX_RATE).abs() < 1e-9);
        let tight = Camera::looking_at([1.0, 0.0, 0.0], MIN_FOV_DEG, false);
        assert!(tight.rate() >= 1.0 && tight.rate() < 2.0);
    }

    #[test]
    fn panning_moves_the_view_and_releases_the_star_it_was_holding() {
        let mut camera = Camera::looking_at([1.0, 0.0, 0.0], 30.0, true);
        camera.pan(120.0, 0.0, 1_440.0, 900.0);
        assert!(!camera.tracking);
        assert!(camera.ra > 0.0);

        // Declination cannot be dragged past the pole.
        for _ in 0..200 {
            camera.pan(0.0, 400.0, 1_440.0, 900.0);
        }
        assert!((camera.dec - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn a_tracked_star_is_held_while_an_untracked_sky_drifts_by_one_turn_a_day() {
        let mut held = Camera::looking_at([1.0, 0.0, 0.0], 20.0, true);
        held.drift(SIDEREAL_DAY_MS / 4.0);
        assert_eq!(held.ra, 0.0);

        let mut drifting = Camera::looking_at([1.0, 0.0, 0.0], 20.0, false);
        drifting.drift(SIDEREAL_DAY_MS);
        assert!(drifting.ra < 1e-6 || (std::f64::consts::TAU - drifting.ra) < 1e-6);
        drifting.drift(SIDEREAL_DAY_MS / 2.0);
        assert!((drifting.ra - std::f64::consts::PI).abs() < 1e-6);
    }
}
