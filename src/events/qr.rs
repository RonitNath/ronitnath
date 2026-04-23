use qrcode::QrCode;
use qrcode::render::svg;

use crate::events::errors::{EventError, Result};

pub(crate) fn svg_for_url(url: &str) -> Result<String> {
    let code =
        QrCode::new(url.as_bytes()).map_err(|err| EventError::InvalidInput(err.to_string()))?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}
