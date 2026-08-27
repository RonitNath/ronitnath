//! Validation for the two fetched star assets.
//!
//! `bright.bin` is the GPU catalog: an 8-byte `STR1` header and then one
//! 20-byte record per star — three f32 of unit position, one f32 magnitude,
//! four bytes of RGBA colour — sorted brightest first so a partial stream is
//! still the brightest sky rather than a random subset of it.
//!
//! `named.json` is the annotation catalog: a small IAU/SIMBAD-attributed list
//! that points at records in `bright.bin` by index.

use serde::Deserialize;

const MAGIC: &[u8; 4] = b"STR1";
pub const HEADER_LEN: usize = 8;
pub const STRIDE: usize = 20;
const MAX_STARS: usize = 1_000_000;

/// Parse and check the header, returning the star count it declares.
pub fn header_count(bytes: &[u8]) -> Result<usize, &'static str> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
        return Err("bad star asset header");
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| "bad star count")?) as usize;
    if count == 0 || count > MAX_STARS || count > i32::MAX as usize {
        return Err("star count out of bounds");
    }
    Ok(count)
}

/// Full structural validation: exact length, and every record a finite unit
/// vector. A star at the origin or with a NaN coordinate renders as a
/// full-screen artefact rather than as nothing, which is why this is checked
/// rather than trusted.
pub fn validate(bytes: &[u8]) -> Result<usize, &'static str> {
    let count = header_count(bytes)?;
    let total = HEADER_LEN
        .checked_add(
            count
                .checked_mul(STRIDE)
                .ok_or("star asset length overflow")?,
        )
        .ok_or("star asset length overflow")?;
    if bytes.len() != total {
        return Err("star asset length mismatch");
    }
    for record in bytes[HEADER_LEN..].chunks_exact(STRIDE) {
        let at = |offset: usize| {
            f32::from_le_bytes(record[offset..offset + 4].try_into().expect("four bytes"))
        };
        let (x, y, z, magnitude) = (at(0), at(4), at(8), at(12));
        let norm_squared = x * x + y * y + z * z;
        if !magnitude.is_finite() || !(0.98..=1.02).contains(&norm_squared) {
            return Err("invalid star record");
        }
    }
    Ok(count)
}

/// The J2000 unit vector of the star at `index`, read straight out of the
/// fetched bytes so the annotations need no second copy of the catalog.
#[must_use]
pub fn position(bytes: &[u8], index: usize) -> Option<[f64; 3]> {
    let offset = HEADER_LEN.checked_add(index.checked_mul(STRIDE)?)?;
    let record = bytes.get(offset..offset.checked_add(12)?)?;
    Some(std::array::from_fn(|axis| {
        f64::from(f32::from_le_bytes(
            record[axis * 4..axis * 4 + 4]
                .try_into()
                .expect("four bytes"),
        ))
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedStar {
    pub bright_index: usize,
    pub name: String,
    pub constellation: String,
    pub classification: String,
    pub distance_ly: f64,
}

#[derive(Debug, Deserialize)]
pub struct NamedCatalog {
    pub version: u8,
    pub sources: Vec<String>,
    pub stars: Vec<NamedStar>,
}

impl NamedCatalog {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIGHT: &[u8] = include_bytes!("../../../static/stars/bright.bin");
    const NAMED: &str = include_str!("../../../static/stars/named.json");

    fn one_record(x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut bytes = b"STR1\x01\0\0\0".to_vec();
        for value in [x, y, z, 2.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[255, 255, 255, 255]);
        bytes
    }

    #[test]
    fn the_shipped_catalog_validates_and_has_its_published_count() {
        assert_eq!(validate(BRIGHT).expect("the shipped catalog"), 12_191);
    }

    #[test]
    fn the_catalog_is_sorted_brightest_first_so_a_partial_stream_is_still_the_sky() {
        let magnitudes: Vec<f32> = BRIGHT[HEADER_LEN..]
            .chunks_exact(STRIDE)
            .map(|record| f32::from_le_bytes(record[12..16].try_into().unwrap()))
            .collect();
        assert!(magnitudes.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_truncated_or_absurd_header_is_refused() {
        assert_eq!(
            validate(b"STR1\0\0\0\0").err(),
            Some("star count out of bounds")
        );
        assert_eq!(
            validate(b"STR1\xff\xff\xff\xff").err(),
            Some("star count out of bounds")
        );
        assert_eq!(
            validate(b"NOPE\x01\0\0\0").err(),
            Some("bad star asset header")
        );
        let mut truncated = one_record(1.0, 0.0, 0.0);
        truncated.pop();
        assert_eq!(
            validate(&truncated).err(),
            Some("star asset length mismatch")
        );
    }

    #[test]
    fn a_record_that_is_not_a_finite_unit_vector_is_refused() {
        for bad in [
            one_record(0.0, 0.0, 0.0),
            one_record(f32::NAN, 0.0, 1.0),
            one_record(2.0, 0.0, 0.0),
        ] {
            assert_eq!(validate(&bad).err(), Some("invalid star record"));
        }
    }

    #[test]
    fn a_star_position_is_readable_by_index_and_bounded_by_the_catalog() {
        let vector = position(BRIGHT, 0).expect("the brightest star");
        let norm: f64 = vector.iter().map(|c| c * c).sum();
        assert!((norm - 1.0).abs() < 0.02, "{norm}");
        assert!(position(BRIGHT, 12_191).is_none());
    }

    #[test]
    fn the_named_catalog_is_structured_and_attributed_to_its_sources() {
        let catalog = NamedCatalog::parse(NAMED).expect("named.json");
        assert_eq!(catalog.version, 1);
        assert!(catalog.stars.len() >= 50);
        assert!(catalog.sources.iter().any(|source| source.contains("IAU")));
        assert!(
            catalog
                .sources
                .iter()
                .any(|source| source.contains("SIMBAD"))
        );
        for star in &catalog.stars {
            assert!(
                position(BRIGHT, star.bright_index).is_some(),
                "{}",
                star.name
            );
            assert!(!star.name.is_empty() && !star.constellation.is_empty());
            assert!(!star.classification.is_empty());
            assert!(star.distance_ly.is_finite() && star.distance_ly > 0.0);
        }
    }
}
