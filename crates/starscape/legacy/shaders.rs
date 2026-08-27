//! GLSL sources for the two starscape passes, kept out of `render.rs` so they
//! compile — and can be asserted about — without `web-sys`.
//!
//! The compositing invariant these must satisfy is documented on
//! [`super::render`]: every fragment writes an alpha that covers the colour it
//! carries. `tests` below pins it, because breaking it produces no error and
//! no visible symptom on many GPUs.

/// A single triangle large enough to cover the viewport — cheaper than a quad
/// and free of the diagonal seam two triangles produce under interpolation.
pub const SKY_VERT: &str = r#"#version 300 es
layout(location = 0) in vec2 a_xy;
out vec2 v_ndc;
void main() {
    v_ndc = a_xy;
    gl_Position = vec4(a_xy, 0.0, 1.0);
}
"#;

pub const SKY_FRAG: &str = r#"#version 300 es
precision highp float;
in vec2 v_ndc;
uniform mat3 u_view;
uniform float u_f;
uniform float u_aspect;
uniform float u_light;
uniform float u_reveal;
uniform sampler2D u_map;
// Driven by the `d` tuning panel; see starscape::tuning.
uniform float u_extinction_k;
uniform float u_band_gain;
uniform float u_band_shape;
out vec4 frag;

const float PI = 3.14159265359;

void main() {
    // Undo the star pass's projection to recover this pixel's view-space ray.
    // The camera looks at the zenith, so ray.z is the cosine of the zenith
    // angle: 1.0 straight up, about 0.38 in the corners (23° altitude).
    vec3 ray = normalize(vec3(v_ndc.x * u_aspect / u_f, v_ndc.y / u_f, 1.0));

    // Atmospheric extinction, 10^(-0.4·k·X) with a secant-z airmass. k = 0.28
    // mag/airmass is a humid low-altitude site rather than a mountaintop's
    // 0.20 — the honest end of the real range, chosen because it makes the
    // band visibly dim toward the frame edge the way it does from a city.
    float airmass = 1.0 / max(ray.z, 0.05);
    float extinction = pow(10.0, -0.4 * u_extinction_k * (airmass - 1.0));

    // The star pass maps J2000 -> view; texturing the sky needs the inverse.
    // The view basis is orthonormal, so its transpose is that inverse, and the
    // map can be sampled in the catalog's own equatorial frame — which is why
    // tools/mwcat.py bakes in equatorial coordinates and no galactic
    // transform appears anywhere in this shader.
    vec3 j2000 = transpose(u_view) * ray;
    vec2 uv = vec2(
        atan(j2000.y, j2000.x) / (2.0 * PI) + 0.5,
        0.5 - asin(clamp(j2000.z, -1.0, 1.0)) / PI
    );

    // Tone mapping lives here rather than in the bake so contrast stays
    // tunable against the live page. The exponent crushes the diffuse floor
    // (which otherwise reads as overcast cloud) and keeps the bright core.
    // Twilight needs the harder curve and less gain: it is painting onto a
    // lit sky, where the same amplitude that reads as the galaxy on black
    // reads as smog. On black the band is the picture, so it gets room.
    float shape = mix(u_band_shape, 2.7, u_light);
    float gain = mix(u_band_gain, 0.46, u_light);
    vec3 band = pow(texture(u_map, uv).rgb, vec3(shape)) * gain * extinction;

    // Twilight keeps the band only high in the sky, where the gradient behind
    // it is still deep blue; near the bottom of the frame the page is nearly
    // white and any band there would read as a smudge behind the links.
    band *= mix(1.0, 1.15 * smoothstep(0.10, 0.75, ray.z), u_light);
    band *= u_reveal;

    // Alpha must cover the colour it carries — see the module docs.
    frag = vec4(band, max(max(band.r, band.g), band.b));
}
"#;

pub const STAR_VERT: &str = r#"#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_mag;
layout(location = 2) in vec4 a_color;
uniform mat3 u_view;
uniform float u_f;
uniform float u_aspect;
uniform float u_dpr;
uniform float u_light;
// Driven by the `d` tuning panel; see starscape::tuning.
uniform float u_size_base;
uniform float u_size_scale;
uniform float u_size_exp;
uniform float u_size_max;
uniform float u_alpha_base;
uniform float u_alpha_scale;
uniform float u_alpha_exp;
uniform float u_alpha_max;
uniform float u_glow;
uniform float u_halo;
uniform float u_extinction_k;
out vec3 v_color;
out float v_alpha;
out float v_falloff;

void main() {
    vec3 v = u_view * a_pos;
    // Below the horizon, or too faint for the twilight sky — where the
    // background is bright enough that a mag-6 star has nothing to show
    // against, so drawing it only adds grey haze.
    float limit = mix(99.0, 4.6, u_light);
    if (v.z <= 0.08 || a_mag > limit) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0); gl_PointSize = 0.0;
        // Every `out` must be written on every path. These points are never
        // rasterized, so the values are arbitrary — but leaving one unwritten
        // is undefined behaviour, and a stricter validator than the one this
        // was developed against may reject the program rather than the vertex.
        v_color = vec3(0.0); v_alpha = 0.0; v_falloff = 1.0; return;
    }
    gl_Position = vec4(v.x / v.z * u_f / u_aspect, v.y / v.z * u_f, 0.0, 1.0);

    float b = pow(10.0, -0.4 * a_mag);          // brightness relative to mag 0
    float airmass = 1.0 / max(v.z, 0.05);
    float extinction = pow(10.0, -0.4 * u_extinction_k * (airmass - 1.0));

    // The magnitude->pixel response. Every constant here now arrives as a
    // uniform so the `t` panel can drive it live; the defaults in
    // starscape::tuning::SPEC are what ships.
    //
    // The alpha ceiling is deliberately allowed to exceed 1.0: additive
    // blending then clips the core to white and leaves the star's colour in
    // the wings, which is what a genuinely bright star looks like. With the
    // ceiling pinned at 1.0 every star from Sirius to Vega renders identically.
    //
    // Twilight needs a different size response than night, because a star on a
    // bright sky wins on area rather than on contrast: a bigger floor so the
    // faint end survives at all, and a steeper curve so the bright end still
    // pulls away from it. These are multipliers on the live uniforms rather
    // than a second set of defaults, so a value tuned in the panel keeps its
    // twilight relationship when the theme flips underneath it.
    float size_base = u_size_base * mix(1.0, 4.0, u_light);
    float size_exp = u_size_exp * mix(1.0, 2.0, u_light);
    float size = clamp(size_base + u_size_scale * pow(b, size_exp), 1.0, u_size_max);
    float alpha = clamp(u_alpha_base + u_alpha_scale * pow(b, u_alpha_exp), 0.0, u_alpha_max) * extinction;

    // Twilight: push toward white (colour does not survive a bright sky) and
    // fade out the lower sky along with the band.
    v_color = mix(a_color.rgb, mix(a_color.rgb, vec3(1.0), 0.25), u_light);
    alpha *= mix(1.0, smoothstep(0.2, 0.7, v.z), u_light);

    gl_PointSize = u_dpr * size;
    v_alpha = alpha;

    // Halo width as a function of brightness, not just alpha. Once the core
    // saturates this is the only channel left that still says "brighter", and
    // it is how the eye actually separates a first-magnitude star from a
    // sixth. u_halo = 0 reproduces the old fixed falloff exactly.
    v_falloff = u_glow / (1.0 + u_halo * pow(b, 0.25));
}
"#;

pub const STAR_FRAG: &str = r#"#version 300 es
precision highp float;
in vec3 v_color;
in float v_alpha;
in float v_falloff;
out vec4 frag;
void main() {
    vec2 c = gl_PointCoord * 2.0 - 1.0;
    float d = dot(c, c);
    if (d > 1.0) discard;
    float glow = exp(-v_falloff * d);
    // Alpha must cover the colour it carries — see the module docs. Writing
    // zero here made the whole starscape vanish on some GPUs.
    vec3 light = v_color * v_alpha * glow;
    frag = vec4(light, max(max(light.r, light.g), light.b));
}
"#;

#[cfg(test)]
mod tests {
    use super::{SKY_FRAG, SKY_VERT, STAR_FRAG, STAR_VERT};

    /// GLSL ES 3.00 is not the default, and the `#version` line must be the
    /// very first thing in the source or compilation fails.
    #[test]
    fn every_stage_declares_glsl_es_300_on_its_first_line() {
        for source in [SKY_VERT, SKY_FRAG, STAR_VERT, STAR_FRAG] {
            assert_eq!(source.lines().next(), Some("#version 300 es"));
        }
    }

    /// A fragment writing colour with alpha 0 is out-of-gamut premultiplied
    /// output. Some GPUs pass it through and some clamp the colour away,
    /// which renders the starscape as a black page with nothing in the
    /// console. Alpha must be derived from the colour, never a literal.
    #[test]
    fn fragment_alpha_covers_the_colour_it_carries() {
        for source in [SKY_FRAG, STAR_FRAG] {
            let assignment = source
                .lines()
                .find(|line| line.trim_start().starts_with("frag = "))
                .expect("every fragment shader assigns frag");
            assert!(
                assignment.contains("max("),
                "alpha must be max(r, g, b) of the premultiplied colour, got: {assignment}"
            );
            assert!(
                !assignment.contains(", 0.0)"),
                "literal zero alpha is undefined premultiplied output: {assignment}"
            );
        }
    }

    #[test]
    fn galaxy_pass_has_an_explicit_reveal_uniform() {
        assert!(SKY_FRAG.contains("uniform float u_reveal;"));
        assert!(SKY_FRAG.contains("band *= u_reveal;"));
    }
}
