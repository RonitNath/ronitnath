//! Rasterising the atlas: a pixel buffer, filled in Rust, handed over once.
//!
//! The atlas draws hundreds of thousands of stars a frame, and the expensive
//! part of doing that through a 2D context is not the arithmetic — it is one
//! `fillStyle` assignment and one `arc` call per star, each a crossing into
//! JavaScript. So nothing here calls the context per star: the frame is
//! rasterised into an RGBA buffer and presented with a single `putImageData`.
//!
//! The buffer is transparent where there are no stars, which is what lets the
//! canvas element's own CSS background be the sky's ground — the ground colour
//! then comes from the theme's tokens rather than from a colour literal parsed
//! back out of a computed style.

use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

/// Device pixels per CSS pixel, capped. Past 2x the extra memory buys nothing
/// a star a pixel and a half wide can show.
const MAX_SCALE: f64 = 2.0;

/// How bright a star of a given magnitude is drawn. Magnitude is a logarithmic
/// scale running backwards, hence the negative exponent.
#[must_use]
pub fn alpha(mag: f64) -> f64 {
    (0.22 + 1.1 * 10f64.powf(-0.4 * mag)).min(1.0)
}

/// How wide it is drawn. The `sqrt(fov)` term is what makes zooming in feel
/// like approaching the sky rather than scaling a picture of it: the stars
/// swell as the field narrows.
#[must_use]
pub fn radius(mag: f64, fov: f64) -> f64 {
    (1.2 * 10f64.powf(-0.12 * mag) * (115.0 / fov.max(0.001)).sqrt()).clamp(0.4, 5.0)
}

pub struct Surface {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    buffer: Vec<u8>,
    width: usize,
    height: usize,
    scale: f64,
    css: (f64, f64),
}

impl Surface {
    pub fn create(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("no 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        Ok(Self {
            canvas,
            ctx,
            buffer: Vec::new(),
            width: 0,
            height: 0,
            scale: 1.0,
            css: (0.0, 0.0),
        })
    }

    /// The frame's size in CSS pixels — what the projector works in.
    #[must_use]
    pub fn css_size(&self) -> (f64, f64) {
        self.css
    }

    /// Match the buffer to the viewport. A no-op when nothing changed, because
    /// resizing reallocates and a resize event fires in bursts.
    pub fn resize(&mut self, css_width: f64, css_height: f64, device_ratio: f64) {
        let scale = device_ratio.clamp(1.0, MAX_SCALE);
        let width = (css_width * scale).round().max(1.0) as usize;
        let height = (css_height * scale).round().max(1.0) as usize;
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.css = (css_width, css_height);
        self.buffer = vec![0; width * height * 4];
        self.canvas.set_width(width as u32);
        self.canvas.set_height(height as u32);
        let _ = self
            .canvas
            .style()
            .set_property("width", &format!("{css_width}px"));
        let _ = self
            .canvas
            .style()
            .set_property("height", &format!("{css_height}px"));
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0);
    }

    /// Draw one star at a CSS-pixel position.
    ///
    /// The buffer is *not* premultiplied — `putImageData` writes the bytes
    /// through untouched and the browser composites the canvas over its own
    /// background afterwards. So the colour channels hold the star's colour as
    /// it should appear, and the alpha channel holds how much of it to let
    /// through; multiplying the colour by the weight as well would dim every
    /// star by its own opacity twice and leave a black sky.
    ///
    /// Overlapping stars accumulate opacity, and the colour follows whichever
    /// contribution is the strongest — a cluster reads as one brighter glow
    /// rather than as whichever star happened to be drawn last.
    pub fn splat(&mut self, x: f64, y: f64, radius: f64, color: [u8; 3], alpha: f64) {
        let (cx, cy, r) = (x * self.scale, y * self.scale, radius * self.scale);
        let (left, right) = ((cx - r - 0.5).floor() as i64, (cx + r + 0.5).ceil() as i64);
        let (top, bottom) = ((cy - r - 0.5).floor() as i64, (cy + r + 0.5).ceil() as i64);
        for py in top.max(0)..=bottom.min(self.height as i64 - 1) {
            for px in left.max(0)..=right.min(self.width as i64 - 1) {
                let dx = px as f64 + 0.5 - cx;
                let dy = py as f64 + 0.5 - cy;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > r + 0.5 {
                    continue;
                }
                // Antialiased disc coverage: solid inside, feathered over the
                // last pixel. A linear falloff from the centre instead would
                // make the common case — a star smaller than one pixel —
                // almost invisible, because no pixel centre ever sits on it.
                let coverage = (r + 0.5 - distance).clamp(0.0, 1.0);
                let weight = alpha * coverage;
                if weight <= 0.004 {
                    continue;
                }
                let at = ((py as usize) * self.width + px as usize) * 4;
                let opacity = (weight * 255.0) as u8;
                if opacity > self.buffer[at + 3] {
                    self.buffer[at..at + 3].copy_from_slice(&color);
                }
                self.buffer[at + 3] = self.buffer[at + 3].saturating_add(opacity);
            }
        }
    }

    /// Hand the frame to the canvas. One call, whatever the star count.
    pub fn present(&self) -> Result<(), JsValue> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }
        let image = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.buffer),
            self.width as u32,
            self.height as u32,
        )?;
        self.ctx.put_image_data(&image, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brighter_star_is_drawn_bigger_and_more_opaque() {
        assert!(alpha(-1.5) > alpha(2.0));
        assert!(alpha(2.0) > alpha(11.0));
        assert!(radius(-1.5, 115.0) > radius(6.0, 115.0));
    }

    #[test]
    fn every_magnitude_stays_inside_the_drawable_range() {
        for mag in [-2.0, 0.0, 6.5, 9.0, 12.0, 20.0] {
            assert!((0.0..=1.0).contains(&alpha(mag)), "{mag}");
            assert!((0.4..=5.0).contains(&radius(mag, 115.0)), "{mag}");
            assert!((0.4..=5.0).contains(&radius(mag, 1.0)), "{mag}");
        }
    }

    #[test]
    fn zooming_in_swells_the_stars_rather_than_leaving_them_at_a_pixel() {
        let wide = radius(9.0, 115.0);
        let tight = radius(9.0, 4.0);
        assert!(tight > wide, "{tight} is not larger than {wide}");
    }
}
