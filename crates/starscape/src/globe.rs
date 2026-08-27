//! The mini-globe: where on Earth this sky is being seen from, and the one
//! control that changes it.
//!
//! An orthographic textured sphere, oriented so the observer's subpoint is
//! always the point facing the viewer. That orientation is the whole design:
//! the globe is not a map with a dot on it, it is the same viewpoint the sky
//! above is drawn from, seen from outside.
//!
//! Clicking picks a point and travels there; dragging spins the Earth under the
//! viewpoint and takes it with you. Both write the same manual observer that
//! [`crate::observer`] owns, so the sky, the marker and the grounding readout
//! cannot disagree about where the viewer is.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, HtmlCanvasElement, ImageBitmap, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram,
    WebGlTexture, WebGlUniformLocation,
};

use crate::sky::{lat_lon, unit_vector};
use crate::{dom, telemetry};

/// Fraction of the canvas half-width the sphere fills, leaving room for the
/// marker's halo at the limb.
const RADIUS: f32 = 0.92;
/// Sphere tessellation. 48x32 is under a thousand triangles and shows no
/// faceting at the size this is drawn.
const SEGMENTS: (usize, usize) = (48, 32);
/// Degrees of rotation per pixel of drag.
const DRAG_SENSITIVITY: f64 = 0.45;

const VERT: &str = r#"#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
uniform mat3 u_orient;
uniform float u_radius;
uniform float u_point;
out vec2 v_uv;
out vec3 v_view;
void main() {
    vec3 p = u_orient * a_pos;
    v_uv = a_uv;
    v_view = p;
    // Orthographic: the sphere is small enough on screen that perspective
    // would only add a distortion nobody asked for. z carries depth only.
    gl_Position = vec4(p.x * u_radius, p.y * u_radius, -p.z * 0.5, 1.0);
    gl_PointSize = u_point;
}
"#;

const FRAG: &str = r#"#version 300 es
precision highp float;
in vec2 v_uv;
in vec3 v_view;
uniform sampler2D u_map;
uniform float u_marker;
uniform vec3 u_ink;
out vec4 frag;
void main() {
    if (u_marker > 0.5) {
        // A round marker, feathered so it does not alias at the limb.
        vec2 c = gl_PointCoord * 2.0 - 1.0;
        float d = dot(c, c);
        if (d > 1.0) discard;
        frag = vec4(u_ink, 1.0 - smoothstep(0.35, 1.0, d));
        return;
    }
    vec3 albedo = texture(u_map, v_uv).rgb;
    // Terminator: the lit face is the one turned toward the viewer, so the
    // limb falls off rather than ending in a hard circle.
    float lambert = clamp(v_view.z * 0.85 + 0.30, 0.0, 1.0);
    float limb = smoothstep(0.0, 0.18, v_view.z);
    frag = vec4(albedo * lambert, limb);
}
"#;

pub struct Globe {
    canvas: HtmlCanvasElement,
    gl: Gl,
    program: WebGlProgram,
    vertices: WebGlBuffer,
    indices: WebGlBuffer,
    index_count: i32,
    marker: WebGlBuffer,
    map: WebGlTexture,
    u_orient: Option<WebGlUniformLocation>,
    u_radius: Option<WebGlUniformLocation>,
    u_point: Option<WebGlUniformLocation>,
    u_marker: Option<WebGlUniformLocation>,
    u_map: Option<WebGlUniformLocation>,
    u_ink: Option<WebGlUniformLocation>,
}

impl Globe {
    pub fn create(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("webgl2 is unavailable"))?
            .dyn_into()?;
        gl.enable(Gl::DEPTH_TEST);
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let program = crate::program::link(&gl, VERT, FRAG)?;
        let (mesh, indices) = sphere(SEGMENTS.0, SEGMENTS.1);

        let vertices = gl.create_buffer().ok_or("create_buffer")?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&vertices));
        unsafe {
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::view(&mesh),
                Gl::STATIC_DRAW,
            );
        }

        let index_buffer = gl.create_buffer().ok_or("create_buffer")?;
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&index_buffer));
        unsafe {
            gl.buffer_data_with_array_buffer_view(
                Gl::ELEMENT_ARRAY_BUFFER,
                &js_sys::Uint16Array::view(&indices),
                Gl::STATIC_DRAW,
            );
        }

        let marker = gl.create_buffer().ok_or("create_buffer")?;
        let map = gl.create_texture().ok_or("create_texture")?;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&map));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            1,
            1,
            0,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            // A neutral blue until the texture lands: the globe reads as Earth
            // from the first frame rather than as a black hole in the corner.
            Some(&[24, 42, 78, 255]),
        )?;
        for (name, value) in [
            (Gl::TEXTURE_WRAP_S, Gl::REPEAT),
            (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
            (Gl::TEXTURE_MIN_FILTER, Gl::LINEAR),
            (Gl::TEXTURE_MAG_FILTER, Gl::LINEAR),
        ] {
            gl.tex_parameteri(Gl::TEXTURE_2D, name, value as i32);
        }

        let at = |name: &str| gl.get_uniform_location(&program, name);
        Ok(Self {
            u_orient: at("u_orient"),
            u_radius: at("u_radius"),
            u_point: at("u_point"),
            u_marker: at("u_marker"),
            u_map: at("u_map"),
            u_ink: at("u_ink"),
            index_count: i32::try_from(indices.len()).expect("the sphere fits an i32 index count"),
            program,
            vertices,
            indices: index_buffer,
            marker,
            map,
            canvas,
            gl,
        })
    }

    pub async fn load_texture(&self, url: &str) -> Result<(), JsValue> {
        let window = dom::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let response = dom::fetch(url).await?;
        let blob: Blob = JsFuture::from(response.blob()?).await?.dyn_into()?;
        let bitmap: ImageBitmap = JsFuture::from(window.create_image_bitmap_with_blob(&blob)?)
            .await?
            .dyn_into()?;
        self.gl.bind_texture(Gl::TEXTURE_2D, Some(&self.map));
        self.gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            &bitmap,
        )?;
        telemetry::increment("globe", "textures", 1.0);
        telemetry::event("globe-texture-loaded");
        Ok(())
    }

    pub fn draw(&self, lat_deg: f64, lon_deg: f64) {
        let Some(window) = dom::window() else { return };
        let dpr = window.device_pixel_ratio().max(1.0);
        let size = (f64::from(self.canvas.client_width()).max(1.0) * dpr) as u32;
        if size == 0 {
            return;
        }
        if self.canvas.width() != size || self.canvas.height() != size {
            self.canvas.set_width(size);
            self.canvas.set_height(size);
        }

        let gl = &self.gl;
        gl.viewport(0, 0, size as i32, size as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        gl.use_program(Some(&self.program));

        let orient = orientation(lat_deg, lon_deg);
        gl.uniform_matrix3fv_with_f32_array(self.u_orient.as_ref(), false, &orient);
        gl.uniform1f(self.u_radius.as_ref(), RADIUS);
        gl.uniform1f(self.u_point.as_ref(), 0.0);
        gl.uniform1f(self.u_marker.as_ref(), 0.0);
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.map));
        gl.uniform1i(self.u_map.as_ref(), 0);

        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vertices));
        gl.vertex_attrib_pointer_with_i32(0, 3, Gl::FLOAT, false, 20, 0);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(1, 2, Gl::FLOAT, false, 20, 12);
        gl.enable_vertex_attrib_array(1);
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.indices));
        gl.draw_elements_with_i32(Gl::TRIANGLES, self.index_count, Gl::UNSIGNED_SHORT, 0);

        // The marker is the observer's own position, so it is always the point
        // facing the camera — drawn last, in front of the sphere it sits on.
        gl.disable(Gl::DEPTH_TEST);
        gl.disable_vertex_attrib_array(1);
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.marker));
        let vertex: [f32; 3] = [0.0, 0.0, 1.0];
        unsafe {
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::view(&vertex),
                Gl::DYNAMIC_DRAW,
            );
        }
        gl.vertex_attrib_pointer_with_i32(0, 3, Gl::FLOAT, false, 0, 0);
        gl.enable_vertex_attrib_array(0);
        gl.uniform1f(self.u_point.as_ref(), (7.0 * dpr) as f32);
        gl.uniform1f(self.u_marker.as_ref(), 1.0);
        let ink: [f32; 3] = if dom::is_light_theme() {
            [1.0, 0.93, 0.72]
        } else {
            [1.0, 0.42, 0.33]
        };
        gl.uniform3fv_with_f32_array(self.u_ink.as_ref(), &ink);
        gl.draw_arrays(Gl::POINTS, 0, 1);
        gl.enable(Gl::DEPTH_TEST);

        telemetry::increment("globe", "ticks", 1.0);
    }

    /// Which point on Earth is under a click, if the click hit the sphere.
    #[must_use]
    pub fn point_at(
        &self,
        client_x: f64,
        client_y: f64,
        lat_deg: f64,
        lon_deg: f64,
    ) -> Option<(f64, f64)> {
        let bounds = self.canvas.get_bounding_client_rect();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return None;
        }
        let x = (2.0 * (client_x - bounds.left()) / bounds.width() - 1.0) / f64::from(RADIUS);
        let y = (1.0 - 2.0 * (client_y - bounds.top()) / bounds.height()) / f64::from(RADIUS);
        unproject(x, y, lat_deg, lon_deg)
    }
}

/// The world-to-view rotation that puts `(lat, lon)` at the centre of the disc
/// with north up: rows are east, north and up at that point.
#[must_use]
pub fn orientation(lat_deg: f64, lon_deg: f64) -> [f32; 9] {
    let (sin_lat, cos_lat) = lat_deg.to_radians().sin_cos();
    let (sin_lon, cos_lon) = lon_deg.to_radians().sin_cos();
    let rows = [
        [-sin_lon, cos_lon, 0.0],
        [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
        [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat],
    ];
    let mut out = [0.0f32; 9];
    for (row, values) in rows.iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            out[col * 3 + row] = *value as f32;
        }
    }
    out
}

/// Turn a point on the unit disc back into a point on Earth, given which point
/// the disc is currently centred on. `None` outside the sphere.
#[must_use]
pub fn unproject(x: f64, y: f64, centre_lat: f64, centre_lon: f64) -> Option<(f64, f64)> {
    let squared = x * x + y * y;
    if !squared.is_finite() || squared > 1.0 {
        return None;
    }
    let view = [x, y, (1.0 - squared).sqrt()];
    let orient = orientation(centre_lat, centre_lon);
    // The basis is orthonormal, so its transpose is its inverse.
    Some(lat_lon(std::array::from_fn(|axis| {
        (0..3)
            .map(|row| f64::from(orient[axis * 3 + row]) * view[row])
            .sum()
    })))
}

/// Where a drag of `(dx, dy)` pixels from `(lat, lon)` leaves the viewpoint.
/// Dragging right spins the Earth right, which moves the viewpoint west — the
/// globe behaves like an object under the hand, not like a scroll bar.
#[must_use]
pub fn drag_to(lat_deg: f64, lon_deg: f64, dx: f64, dy: f64) -> (f64, f64) {
    (
        (lat_deg + dy * DRAG_SENSITIVITY).clamp(-89.5, 89.5),
        crate::sky::normalize_lon_deg(lon_deg - dx * DRAG_SENSITIVITY),
    )
}

/// Interleaved `[x, y, z, u, v]` vertices and a triangle index list.
fn sphere(lon_segments: usize, lat_segments: usize) -> (Vec<f32>, Vec<u16>) {
    let mut vertices = Vec::with_capacity((lon_segments + 1) * (lat_segments + 1) * 5);
    for row in 0..=lat_segments {
        let v = row as f64 / lat_segments as f64;
        let lat = 90.0 - v * 180.0;
        for column in 0..=lon_segments {
            let u = column as f64 / lon_segments as f64;
            let position = unit_vector(lat, u * 360.0 - 180.0);
            vertices.extend(position.iter().map(|value| *value as f32));
            // The equirectangular texture starts at the antimeridian, which is
            // where `lon = -180` puts u = 0.
            vertices.push(u as f32);
            vertices.push(v as f32);
        }
    }

    let mut indices = Vec::with_capacity(lon_segments * lat_segments * 6);
    let stride = lon_segments + 1;
    for row in 0..lat_segments {
        for column in 0..lon_segments {
            let a = u16::try_from(row * stride + column).expect("the sphere fits u16 indices");
            let b = a + 1;
            let c = a + u16::try_from(stride).expect("the sphere fits u16 indices");
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(orient: [f32; 9], v: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|row| {
            (0..3)
                .map(|axis| f64::from(orient[axis * 3 + row]) * v[axis])
                .sum()
        })
    }

    #[test]
    fn the_observers_own_position_is_the_point_facing_the_viewer() {
        for (lat, lon) in [(0.0, 0.0), (37.77, -122.42), (-33.87, 151.21), (89.0, 12.0)] {
            // The orientation is uploaded as f32, so the tolerance is single
            // precision — tighter would be testing the f32 mantissa.
            let view = apply(orientation(lat, lon), unit_vector(lat, lon));
            assert!(view[0].abs() < 1e-6 && view[1].abs() < 1e-6, "{view:?}");
            assert!((view[2] - 1.0).abs() < 1e-6, "{view:?}");
        }
    }

    #[test]
    fn north_is_up_and_east_is_to_the_right() {
        let orient = orientation(0.0, 0.0);
        let north_pole = apply(orient, unit_vector(90.0, 0.0));
        assert!(north_pole[1] > 0.99, "{north_pole:?}");
        let east = apply(orient, unit_vector(0.0, 10.0));
        assert!(east[0] > 0.0, "{east:?}");
    }

    #[test]
    fn a_click_at_the_centre_of_the_disc_is_the_point_already_shown() {
        for centre in [(0.0, 0.0), (37.77, -122.42), (-33.87, 151.21)] {
            let picked =
                unproject(0.0, 0.0, centre.0, centre.1).expect("the centre is on the sphere");
            assert!(
                (picked.0 - centre.0).abs() < 1e-5,
                "{picked:?} vs {centre:?}"
            );
            assert!(
                (picked.1 - centre.1).abs() < 1e-5,
                "{picked:?} vs {centre:?}"
            );
        }
    }

    #[test]
    fn a_click_off_the_disc_picks_nothing_rather_than_the_nearest_edge() {
        assert!(unproject(1.01, 0.0, 0.0, 0.0).is_none());
        assert!(unproject(0.8, 0.8, 0.0, 0.0).is_none());
        assert!(unproject(f64::NAN, 0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn a_click_above_the_centre_picks_a_point_further_north() {
        let (lat, _) = unproject(0.0, 0.5, 0.0, 0.0).expect("on the sphere");
        assert!(lat > 0.0, "{lat}");
    }

    #[test]
    fn dragging_right_moves_the_viewpoint_west_and_never_past_a_pole() {
        let (_, lon) = drag_to(0.0, 0.0, 100.0, 0.0);
        assert!(lon < 0.0, "{lon}");
        let (lat, _) = drag_to(80.0, 0.0, 0.0, 1_000.0);
        assert!((-90.0..=90.0).contains(&lat), "{lat}");
        // Across the antimeridian rather than off the end of the number line.
        let (_, wrapped) = drag_to(0.0, -179.0, 100.0, 0.0);
        assert!(
            (-180.0..180.0).contains(&wrapped) && wrapped > 0.0,
            "{wrapped}"
        );
    }

    #[test]
    fn the_sphere_mesh_is_closed_and_indexable_as_u16() {
        let (vertices, indices) = sphere(SEGMENTS.0, SEGMENTS.1);
        assert_eq!(vertices.len(), (SEGMENTS.0 + 1) * (SEGMENTS.1 + 1) * 5);
        assert_eq!(indices.len(), SEGMENTS.0 * SEGMENTS.1 * 6);
        let vertex_count = u16::try_from(vertices.len() / 5).expect("fits u16");
        assert!(indices.iter().all(|index| *index < vertex_count));
        for chunk in vertices.chunks_exact(5) {
            let norm: f32 = chunk[..3].iter().map(|c| c * c).sum();
            assert!((norm - 1.0).abs() < 1e-4, "{norm}");
            assert!((0.0..=1.0).contains(&chunk[3]) && (0.0..=1.0).contains(&chunk[4]));
        }
    }
}
