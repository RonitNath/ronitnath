//! One frame of the atlas: advance the clock, draw three catalogs, present.
//!
//! The three levels are painted in disjoint magnitude bands — naked eye up to
//! 6.5, mid down to 9, regional below that. The bands are the reason the same
//! star does not get drawn twice: the deep catalog is a superset of the others
//! in coverage, and letting the levels overlap would put a second, brighter,
//! slightly differently-positioned copy of every bright star on the screen.

use std::rc::Rc;

use crate::explorer::lod::{self, Star};
use crate::explorer::{Atlas, paint};
use crate::{dom, telemetry};

/// Repaint at 20 Hz. The sky moves slowly even at sixty times real time, and
/// the frame budget is better spent on stars than on frames.
const PAINT_INTERVAL_MS: f64 = 50.0;
/// While paused — including the whole of a reduced-motion session — the atlas
/// still repaints slowly, so a tile that arrives becomes visible. Nothing
/// moves: this is the same picture with more of it filled in.
const IDLE_INTERVAL_MS: f64 = 250.0;

/// The longest step the clock takes in one frame. A backgrounded tab that
/// returns after a minute must not jump the sky a simulated hour.
const MAX_STEP_MS: f64 = 100.0;

/// Brightest magnitude each catalog is allowed to draw, so the bands do not
/// overlap the ones already drawn beneath them.
const MID_FLOOR: f32 = 6.5;
const DEEP_FLOOR: f32 = 9.0;

/// How long a newly arrived level takes to fade up.
const FADE_MS: f64 = 350.0;

impl Atlas {
    /// Queue the next frame.
    pub fn schedule(self: &Rc<Self>) {
        let atlas = Rc::clone(self);
        dom::on_next_frame(move || atlas.tick());
    }

    fn tick(self: Rc<Self>) {
        if self.closed.get() {
            return;
        }
        let now = dom::now_ms();
        let elapsed = (now - self.last_frame.get()).clamp(0.0, MAX_STEP_MS);
        self.last_frame.set(now);

        if !self.paused.get() {
            let camera = self.camera.get();
            let step = elapsed * camera.rate();
            self.sim_ms.set(self.sim_ms.get() + step);
            let mut drifted = camera;
            drifted.drift(step);
            self.camera.set(drifted);
        }

        let interval = if self.paused.get() {
            IDLE_INTERVAL_MS
        } else {
            PAINT_INTERVAL_MS
        };
        if now - self.last_paint.get() >= interval {
            self.last_paint.set(now);
            // Keep asking. Only a handful of tile requests may be in flight at
            // once, so one ask per steer fills the middle of the view and
            // leaves its edges empty until the next gesture — the sky then
            // looks like it has a hole in it wherever you last stopped moving.
            self.tiles
                .request(&self.camera.get(), self.aspect_of_surface());
            self.draw(now);
        }
        self.schedule();
    }

    /// Paint the frame.
    pub fn draw(&self, now: f64) {
        let camera = self.camera.get();
        let (width, height) = self.surface.borrow().css_size();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let projector = camera.projector(width, height);
        let mut surface = self.surface.borrow_mut();
        surface.clear();

        let mut visible = 0usize;
        for star in &self.base {
            visible += usize::from(plot(&mut surface, &projector, star, camera.fov, 1.0));
        }

        if camera.fov <= lod::MID_FOV_DEG
            && let Some((stars, loaded_at)) = self.tiles.mid()
        {
            let fade = self.fade(now, loaded_at);
            for star in stars.iter().filter(|star| star.mag > MID_FLOOR) {
                visible += usize::from(plot(&mut surface, &projector, star, camera.fov, fade));
            }
        }

        if camera.fov <= lod::DEEP_FOV_DEG {
            for id in self.tiles.wanted(&camera, self.aspect_of(width, height)) {
                let Some((stars, loaded_at)) = self.tiles.resident(id) else {
                    continue;
                };
                let fade = self.fade(now, loaded_at);
                for star in stars.iter().filter(|star| star.mag > DEEP_FLOOR) {
                    visible += usize::from(plot(&mut surface, &projector, star, camera.fov, fade));
                }
            }
        }

        let _ = surface.present();
        drop(surface);

        self.visible.set(visible);
        telemetry::number("explorer", "visibleStars", visible as f64);
        if self.first_frame_at.get() == 0.0 {
            self.first_frame_at.set(dom::now_ms());
            telemetry::mark("explorer", "firstFrameAtMs");
            telemetry::event("atlas-first-frame");
        }
    }

    /// How far a level has faded in. Reduced motion gets the picture, not the
    /// fade — a level appearing is information, and animating it is decoration.
    fn fade(&self, now: f64, loaded_at: f64) -> f64 {
        if self.reduced {
            return 1.0;
        }
        ((now - loaded_at) / FADE_MS).clamp(0.0, 1.0)
    }

    fn aspect_of(&self, width: f64, height: f64) -> f64 {
        if height > 0.0 { width / height } else { 1.6 }
    }

    fn aspect_of_surface(&self) -> f64 {
        let (width, height) = self.surface.borrow().css_size();
        self.aspect_of(width, height)
    }
}

/// Draw one star, reporting whether it landed in the frame.
fn plot(
    surface: &mut paint::Surface,
    projector: &crate::explorer::camera::Projector,
    star: &Star,
    fov: f64,
    fade: f64,
) -> bool {
    let Some((x, y)) = projector.project(star.pos) else {
        return false;
    };
    let magnitude = f64::from(star.mag);
    surface.splat(
        x,
        y,
        paint::radius(magnitude, fov),
        star.color,
        paint::alpha(magnitude) * fade,
    );
    true
}
