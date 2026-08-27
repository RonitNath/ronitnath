//! The shipped sky-look uniforms.
//!
//! Every constant in the magnitude-to-pixel response is a uniform rather than a
//! literal in the shader, so the look can be driven from a panel without a
//! rebuild. rn-site ships the defaults and nothing else drives them.

/// `(field, default)` — the GLSL uniform is `u_<field>`.
pub const SPEC: &[(&str, f32)] = &[
    ("size_base", 0.8),
    ("size_scale", 3.2),
    ("size_exp", 0.25),
    ("size_max", 9.0),
    ("alpha_base", 0.38),
    ("alpha_scale", 0.72),
    ("alpha_exp", 0.42),
    ("alpha_max", 1.0),
    ("glow", 2.5),
    ("halo", 0.0),
    ("extinction_k", 0.28),
    ("band_gain", 0.78),
    ("band_shape", 2.2),
];

pub const COUNT: usize = SPEC.len();
pub type Values = [f32; COUNT];

#[must_use]
pub fn values() -> Values {
    let mut values = [0.0; COUNT];
    for (slot, (_, default)) in values.iter_mut().zip(SPEC) {
        *slot = *default;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_uniform_the_shaders_read_has_a_shipped_default() {
        for (field, _) in SPEC {
            let uniform = format!("uniform float u_{field};");
            assert!(
                crate::shaders::STAR_VERT.contains(&uniform)
                    || crate::shaders::SKY_FRAG.contains(&uniform),
                "no shader declares {uniform}"
            );
        }
        assert_eq!(values().len(), SPEC.len());
    }
}
