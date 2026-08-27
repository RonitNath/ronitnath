//! Shipped sky-look uniforms. The universe project drives these from a live
//! `t` panel; rn-site inlines the shipped defaults only.

/// `(field, default)` — GLSL uniform `u_<field>`.
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
pub fn defaults() -> Values {
    let mut values = [0.0; COUNT];
    for (i, spec) in SPEC.iter().enumerate() {
        values[i] = spec.1;
    }
    values
}

#[must_use]
pub fn values() -> Values {
    defaults()
}
