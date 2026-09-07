/** The GLSL the star pass runs.
 *
 * Kept beside `stars-gl.ts` rather than in it: the shaders are the *picture* —
 * the profile, the tone curve, the projection — and the class around them is
 * buffer and framebuffer bookkeeping. Reading one should not mean scrolling
 * past the other.
 *
 * What the numbers in here mean, and why they are those numbers, is in
 * `tuning.ts`; every one of them arrives as a uniform.
 */

export const POINT_VERT = `#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_mag;
layout(location = 2) in vec3 a_color;

uniform mat3 u_view;
uniform float u_f;
uniform float u_aspect;
uniform float u_dpr;
uniform float u_mag_ref;
uniform float u_extinction_k;
uniform float u_glare_k;
uniform float u_glare_exp;
uniform float u_sigma;
uniform float u_floor;
uniform float u_max_px;
uniform float u_whiten;
uniform float u_twilight;

out vec3 v_color;
out float v_flux;
out float v_size_css;

void main() {
    vec3 view = u_view * a_pos;
    if (view.z <= 0.08) {
        // Below the horizon: park it outside the clip volume rather than
        // spend a fragment on it.
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        gl_PointSize = 0.0;
        return;
    }

    // Linear flux against this theme's reference magnitude, through the
    // plane-parallel airmass the band shader uses for the same sky.
    float airmass = 1.0 / max(view.z, 0.05);
    float flux = pow(10.0, -0.4 * (a_mag - u_mag_ref + u_extinction_k * (airmass - 1.0)));
    // Twilight keeps its stars high in the sky; near the bottom of the frame
    // the page is nearly white and a star there is a grey speck.
    flux *= mix(1.0, smoothstep(0.20, 0.70, view.z), u_twilight);

    // The point is sized by where its glare wing falls below what the screen
    // can show, which is the only reason a bright star is a wide one.
    float wing = sqrt(pow(flux, u_glare_exp) * u_glare_k / u_floor);
    float size_css = min(u_max_px, 2.0 * max(max(2.0, 3.0 * u_sigma), wing));

    v_size_css = size_css;
    v_flux = flux;
    v_color = mix(a_color, vec3(1.0), u_whiten);
    gl_PointSize = size_css * u_dpr;
    gl_Position = vec4(view.x * u_f / u_aspect, view.y * u_f, 0.0, view.z);
}
`;

export const POINT_FRAG = `#version 300 es
precision highp float;

in vec3 v_color;
in float v_flux;
in float v_size_css;

uniform float u_dpr;
uniform float u_sigma;
uniform float u_glare_k;
uniform float u_glare_exp;
uniform float u_eps;
/** 1 when there is no float target to accumulate into and the tone curve has
 * to be folded in here instead. */
uniform float u_fold;
uniform float u_exposure;

out vec4 frag;

void main() {
    vec2 offset = gl_PointCoord - 0.5;
    float radius = length(offset);
    float device_px = radius * v_size_css * u_dpr;
    float css_px = radius * v_size_css;

    // Normalised so the core integrates to the star's flux however few pixels
    // it lands on: a sub-pixel star sums rather than vanishing.
    float core = exp(-0.5 * device_px * device_px / (u_sigma * u_sigma))
        / (6.28318530718 * u_sigma * u_sigma);
    float wing = u_glare_k / ((css_px + u_eps) * (css_px + u_eps));
    float value = v_flux * core + pow(v_flux, u_glare_exp) * wing;

    // The quad has to end somewhere; fading the profile out before it does is
    // what keeps the largest stars from carrying a visible square rim.
    value *= 1.0 - smoothstep(0.42, 0.5, radius);

    vec3 light = v_color * value;
    if (u_fold > 0.5) light = 1.0 - exp(-light * u_exposure);
    frag = vec4(light, max(max(light.r, light.g), light.b));
}
`;

export const TONE_VERT = `#version 300 es
layout(location = 0) in vec2 a_xy;
out vec2 v_uv;
void main() {
    v_uv = a_xy * 0.5 + 0.5;
    gl_Position = vec4(a_xy, 0.0, 1.0);
}
`;

export const TONE_FRAG = `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_hdr;
uniform float u_exposure;
out vec4 frag;

void main() {
    // Per channel, so a core hundreds of times over white goes to white while
    // its wings stay on the linear part of the curve and keep their colour.
    vec3 light = 1.0 - exp(-max(texture(u_hdr, v_uv).rgb, 0.0) * u_exposure);
    // Alpha covers the colour it carries: this canvas composites over the
    // dusk gradient in the light theme.
    frag = vec4(light, max(max(light.r, light.g), light.b));
}
`;

/** The highlight ring a clicked callout puts on its star. A circle of 64
 * segments generated from `gl_VertexID`, so it needs no buffer at all. */
export const RING_VERT = `#version 300 es
uniform vec2 u_centre_px;
uniform vec2 u_viewport;
uniform float u_radius_px;
void main() {
    float angle = 6.28318530718 * float(gl_VertexID) / 64.0;
    vec2 at = u_centre_px + u_radius_px * vec2(cos(angle), sin(angle));
    gl_Position = vec4(2.0 * at / u_viewport - 1.0, 0.0, 1.0);
}
`;

export const RING_FRAG = `#version 300 es
precision highp float;
uniform vec4 u_ink;
out vec4 frag;
void main() { frag = u_ink; }
`;

export const POINT_UNIFORMS = [
  'u_view',
  'u_f',
  'u_aspect',
  'u_dpr',
  'u_mag_ref',
  'u_extinction_k',
  'u_glare_k',
  'u_glare_exp',
  'u_sigma',
  'u_floor',
  'u_max_px',
  'u_whiten',
  'u_twilight',
  'u_eps',
  'u_fold',
  'u_exposure',
] as const;
