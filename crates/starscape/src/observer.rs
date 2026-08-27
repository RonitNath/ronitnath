//! Whose sky is on screen: the canonical orbit, or a place the viewer picked.
//!
//! The default observer is the shared, server-synchronised track — every
//! browser on the site is looking out from the same point at the same instant.
//! Dragging the globe overrides that, and the override is deliberately
//! tab-local: it is a viewer's own detour, not a change to the site.
//!
//! A move is interpolated along the great circle rather than snapped, and
//! "resume orbit" is the same interpolation with the manual point cleared when
//! it lands. Under reduced motion the duration is zero, which makes both an
//! immediate assignment without a second code path.

use crate::track::{great_circle_lerp, observer_at};

/// How long a hand-driven move takes, when motion is not reduced.
pub const TRANSITION_MS: f64 = 800.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Transition {
    from: (f64, f64),
    started_at_ms: f64,
    duration_ms: f64,
    /// Clear the manual observer when this lands: the move is a return to orbit.
    clear_at_end: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Observer {
    manual: Option<(f64, f64)>,
    transition: Option<Transition>,
}

impl Observer {
    /// Where the sky is being viewed from, resolving any move in flight.
    ///
    /// Takes `&mut self` because a landed transition is retired here: the
    /// alternative is a timer whose only job is to notice that a lerp finished.
    #[must_use]
    pub fn resolve(&mut self, sim_ms: f64, now_ms: f64) -> (f64, f64) {
        let target = self.manual.unwrap_or_else(|| observer_at(sim_ms));
        let Some(transition) = self.transition else {
            return target;
        };
        let progress = if transition.duration_ms <= 0.0 {
            1.0
        } else {
            ((now_ms - transition.started_at_ms) / transition.duration_ms).clamp(0.0, 1.0)
        };
        if progress >= 1.0 {
            self.transition = None;
            if transition.clear_at_end {
                self.manual = None;
                return observer_at(sim_ms);
            }
            return target;
        }
        great_circle_lerp(transition.from, target, progress)
    }

    /// The viewer's chosen point, if they have one. `None` means the shared orbit.
    #[must_use]
    pub fn manual(&self) -> Option<(f64, f64)> {
        self.manual
    }

    #[must_use]
    pub fn is_manual(&self) -> bool {
        self.manual.is_some()
    }

    /// Move to a chosen point, travelling there from wherever the view is now.
    pub fn set(&mut self, lat: f64, lon: f64, sim_ms: f64, now_ms: f64, duration_ms: f64) {
        if !lat.is_finite() || !lon.is_finite() {
            return;
        }
        let from = self.resolve(sim_ms, now_ms);
        self.manual = Some((lat.clamp(-90.0, 90.0), crate::sky::normalize_lon_deg(lon)));
        self.transition = Some(Transition {
            from,
            started_at_ms: now_ms,
            duration_ms,
            clear_at_end: false,
        });
    }

    /// Travel back to the shared orbit and hand control of the view back to it.
    pub fn resume(&mut self, sim_ms: f64, now_ms: f64, duration_ms: f64) {
        if self.manual.is_none() {
            return;
        }
        let from = self.resolve(sim_ms, now_ms);
        self.transition = Some(Transition {
            from,
            started_at_ms: now_ms,
            duration_ms,
            clear_at_end: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::SIM_EPOCH_MS;

    const NOW: f64 = 1_800_000_000_000.0;
    const SYDNEY: (f64, f64) = (-33.8688, 151.2093);

    fn near(a: (f64, f64), b: (f64, f64), tolerance: f64) -> bool {
        (a.0 - b.0).abs() < tolerance && (a.1 - b.1).abs() < tolerance
    }

    #[test]
    fn with_no_choice_made_the_view_follows_the_shared_orbit() {
        let mut observer = Observer::default();
        assert!(!observer.is_manual());
        assert_eq!(
            observer.resolve(SIM_EPOCH_MS, NOW),
            crate::track::observer_at(SIM_EPOCH_MS)
        );
    }

    #[test]
    fn a_chosen_point_is_reached_at_the_end_of_its_travel_and_not_before() {
        let mut observer = Observer::default();
        observer.set(SYDNEY.0, SYDNEY.1, SIM_EPOCH_MS, NOW, TRANSITION_MS);
        let start = observer.resolve(SIM_EPOCH_MS, NOW);
        assert!(
            !near(start, SYDNEY, 1.0),
            "the move started already arrived"
        );

        let midway = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2.0);
        assert!(!near(midway, start, 1e-6) && !near(midway, SYDNEY, 1e-6));

        assert!(near(
            observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS),
            SYDNEY,
            1e-6
        ));
        assert!(near(
            observer.manual().expect("a chosen point"),
            SYDNEY,
            1e-9
        ));
    }

    #[test]
    fn reduced_motion_makes_the_same_call_an_immediate_assignment() {
        let mut observer = Observer::default();
        observer.set(SYDNEY.0, SYDNEY.1, SIM_EPOCH_MS, NOW, 0.0);
        assert!(near(observer.resolve(SIM_EPOCH_MS, NOW), SYDNEY, 1e-9));
    }

    #[test]
    fn resuming_the_orbit_travels_back_and_then_releases_the_view() {
        let mut observer = Observer::default();
        observer.set(SYDNEY.0, SYDNEY.1, SIM_EPOCH_MS, NOW, 0.0);
        let _ = observer.resolve(SIM_EPOCH_MS, NOW);
        observer.resume(SIM_EPOCH_MS, NOW, TRANSITION_MS);

        // Still the viewer's, still travelling.
        assert!(observer.is_manual());
        let _ = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2.0);
        assert!(observer.is_manual());

        let landed = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS);
        assert!(!observer.is_manual());
        assert!(near(landed, crate::track::observer_at(SIM_EPOCH_MS), 1e-9));
    }

    #[test]
    fn resuming_an_orbit_that_was_never_left_changes_nothing() {
        let mut observer = Observer::default();
        observer.resume(SIM_EPOCH_MS, NOW, TRANSITION_MS);
        assert_eq!(observer, Observer::default());
    }

    #[test]
    fn a_longitude_past_the_antimeridian_is_normalised_rather_than_stored_raw() {
        let mut observer = Observer::default();
        observer.set(0.0, 200.0, SIM_EPOCH_MS, NOW, 0.0);
        assert!(near(
            observer.manual().expect("a chosen point"),
            (0.0, -160.0),
            1e-9
        ));
    }

    #[test]
    fn a_coordinate_that_is_not_a_number_is_ignored_rather_than_poisoning_the_view() {
        let mut observer = Observer::default();
        observer.set(f64::NAN, 0.0, SIM_EPOCH_MS, NOW, 0.0);
        assert!(!observer.is_manual());
    }

    #[test]
    fn a_second_move_starts_from_where_the_first_one_had_got_to() {
        let mut observer = Observer::default();
        observer.set(SYDNEY.0, SYDNEY.1, SIM_EPOCH_MS, NOW, TRANSITION_MS);
        let midway = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2.0);
        observer.set(
            0.0,
            0.0,
            SIM_EPOCH_MS,
            NOW + TRANSITION_MS / 2.0,
            TRANSITION_MS,
        );
        let restarted = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2.0);
        assert!(near(restarted, midway, 1e-6), "{restarted:?} vs {midway:?}");
    }
}
