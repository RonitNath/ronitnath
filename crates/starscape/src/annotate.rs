//! Placing the named-star callouts.
//!
//! A callout is only worth drawing if the star it names is actually in the
//! frame, so this projects the named catalog through the same view matrix the
//! GPU uses and keeps the ones that land inside a margin. When nothing clears
//! that margin — which happens at the narrow end of a phone viewport — the
//! best above-horizon star is *forced* to the edge rather than the sky going
//! unlabelled, and it says so, because a clamped position is not a measurement
//! of where the star is.

use crate::sky::FOCAL;

/// A star's position in normalised device coordinates, ready to be turned into
/// a CSS offset by the caller that knows the element's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Index into the named catalog that was passed in.
    pub star: usize,
    /// The star's J2000 unit vector — the direction the atlas opens on when
    /// this label is clicked, carried here so the click needs no second lookup.
    pub position: [f64; 3],
    /// `-1.0 ..= 1.0`, left to right.
    pub x: f64,
    /// `-1.0 ..= 1.0`, bottom to top.
    pub y: f64,
    /// The star is above the horizon but outside the comfortable margin, and
    /// this position has been clamped to the edge to keep a label on screen.
    pub forced: bool,
}

/// Inside this fraction of the frame a callout has room for its two lines
/// without colliding with the edge.
const MARGIN_X: f64 = 0.88;
const MARGIN_Y: f64 = 0.82;

/// Stars below this much of the zenith cosine are too near the horizon: the
/// projection stretches without bound there and the label would slide off.
const MIN_ZENITH_COS: f64 = 0.12;

/// The hero sits in the middle of the frame, and a label behind it is not a
/// label — it is two texts on top of each other. Stars projecting into this
/// band are passed over the way an off-frame star is; something else in the
/// catalog is almost always available, and when nothing is, the forced
/// placement lands on the edge, which is outside it by construction.
const KEEP_OUT: (f64, f64) = (0.42, 0.24);

/// Project `stars` (J2000 unit vectors) and return up to `limit` placements,
/// brightest-first in catalog order.
#[must_use]
pub fn place(stars: &[[f64; 3]], matrix: [f32; 9], aspect: f64, limit: usize) -> Vec<Placement> {
    let mut inside = Vec::new();
    let mut best_forced: Option<Placement> = None;

    for (star, vector) in stars.iter().enumerate() {
        let project = |row: usize| {
            (0..3)
                .map(|axis| f64::from(matrix[axis * 3 + row]) * vector[axis])
                .sum::<f64>()
        };
        let (vx, vy, vz) = (project(0), project(1), project(2));
        if vz <= MIN_ZENITH_COS {
            continue;
        }
        let x = vx / vz * FOCAL as f64 / aspect;
        let y = vy / vz * FOCAL as f64;
        if x.abs() < KEEP_OUT.0 && y.abs() < KEEP_OUT.1 {
            continue;
        }
        if x.abs() <= MARGIN_X && y.abs() <= MARGIN_Y {
            inside.push(Placement {
                star,
                position: *vector,
                x,
                y,
                forced: false,
            });
            if inside.len() == limit {
                return inside;
            }
        } else if best_forced.is_none() {
            best_forced = Some(Placement {
                star,
                position: *vector,
                x: x.clamp(-MARGIN_X, MARGIN_X),
                y: y.clamp(-MARGIN_Y, MARGIN_Y),
                forced: true,
            });
        }
    }

    if inside.is_empty() {
        return best_forced.into_iter().collect();
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{NamedCatalog, position};
    use crate::sky::{SIM_EPOCH_MS, view_matrix};
    use crate::track::{TRACK_PERIOD_MS, observer_at};

    fn named_vectors() -> Vec<[f64; 3]> {
        let bright = include_bytes!("../../../static/stars/bright.bin");
        NamedCatalog::parse(include_str!("../../../static/stars/named.json"))
            .expect("named.json")
            .stars
            .iter()
            .map(|star| position(bright, star.bright_index).expect("an in-range index"))
            .collect()
    }

    #[test]
    fn every_point_of_the_orbit_labels_the_sky_on_a_desktop_and_on_a_phone() {
        let stars = named_vectors();
        let mut forced_seen = false;
        for sample in 0..2_000 {
            let sim_ms = SIM_EPOCH_MS + TRACK_PERIOD_MS * f64::from(sample) / 2_000.0;
            let (lat, lon) = observer_at(sim_ms);
            let matrix = view_matrix(sim_ms, lat, lon);
            for aspect in [1440.0 / 900.0, 390.0 / 844.0] {
                let placed = place(&stars, matrix, aspect, 3);
                assert!(!placed.is_empty(), "unlabelled sky at sample {sample}");
                for placement in &placed {
                    assert!(
                        placement.x.abs() >= KEEP_OUT.0 || placement.y.abs() >= KEEP_OUT.1,
                        "a label landed on the hero at sample {sample}: {placement:?}"
                    );
                }
                forced_seen |= placed.iter().any(|p| p.forced);
            }
        }
        assert!(forced_seen, "the forced-label path was never exercised");
    }

    #[test]
    fn a_forced_placement_is_clamped_to_the_margin_and_admits_it() {
        // One star just outside the horizontal margin and nothing else.
        let matrix = view_matrix(SIM_EPOCH_MS, 0.0, 0.0);
        let edge = |x: f64| {
            // Undo the projection for a chosen ndc x on the view axis, then
            // express it back in J2000 through the transposed (orthonormal)
            // basis.
            let view = [x / (FOCAL as f64), 0.0, 1.0];
            let norm = (view[0] * view[0] + 1.0).sqrt();
            std::array::from_fn(|axis| {
                (0..3)
                    .map(|row| f64::from(matrix[axis * 3 + row]) * view[row] / norm)
                    .sum::<f64>()
            })
        };
        let placed = place(&[edge(1.5)], matrix, 1.0, 3);
        assert_eq!(placed.len(), 1);
        assert!(placed[0].forced);
        assert!(placed[0].x.abs() <= MARGIN_X);
        // The clamp moves the label, never the star it names.
        assert_eq!(placed[0].position, edge(1.5));
    }

    #[test]
    fn a_star_below_the_horizon_is_never_labelled() {
        let matrix = view_matrix(SIM_EPOCH_MS, 0.0, 0.0);
        let straight_down = std::array::from_fn(|axis| -f64::from(matrix[axis * 3 + 2]));
        assert!(place(&[straight_down], matrix, 1.6, 3).is_empty());
    }

    #[test]
    fn the_limit_is_respected_and_comfortable_placements_win_over_forced_ones() {
        let stars = named_vectors();
        let (lat, lon) = observer_at(SIM_EPOCH_MS);
        let placed = place(&stars, view_matrix(SIM_EPOCH_MS, lat, lon), 1.6, 2);
        assert!(placed.len() <= 2);
        assert!(placed.iter().all(|p| !p.forced));
    }
}
