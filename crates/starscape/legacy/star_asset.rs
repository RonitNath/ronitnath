//! Pure validation for the fetched GPU star catalog.

const MAGIC: &[u8; 4] = b"STR1";
pub const STRIDE: usize = 20;
pub const HEADER_LEN: usize = 8;
const MAX_STARS: usize = 1_000_000;

pub struct ValidatedStars<'a> {
    pub count: i32,
    pub records: &'a [u8],
}

/// Parse and check the 8-byte header. Returns `(count, header_len)`.
pub fn validate_header(bytes: &[u8]) -> Result<(usize, usize), &'static str> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
        return Err("bad star asset header");
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| "bad star count")?) as usize;
    if count == 0 || count > MAX_STARS || count > i32::MAX as usize {
        return Err("star count out of bounds");
    }
    Ok((count, HEADER_LEN))
}

pub fn validate(bytes: &[u8]) -> Result<ValidatedStars<'_>, &'static str> {
    let (count, _) = validate_header(bytes)?;
    let payload_len = count
        .checked_mul(STRIDE)
        .ok_or("star asset length overflow")?;
    let total_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or("star asset length overflow")?;
    if bytes.len() != total_len {
        return Err("star asset length mismatch");
    }

    let records = &bytes[HEADER_LEN..];
    for record in records.chunks_exact(STRIDE) {
        let component = |offset: usize| {
            f32::from_le_bytes(
                record[offset..offset + 4]
                    .try_into()
                    .expect("four-byte field"),
            )
        };
        let (x, y, z, magnitude) = (component(0), component(4), component(8), component(12));
        let norm_squared = x * x + y * y + z * z;
        if !x.is_finite()
            || !y.is_finite()
            || !z.is_finite()
            || !magnitude.is_finite()
            || !(0.98..=1.02).contains(&norm_squared)
        {
            return Err("invalid star record");
        }
    }

    Ok(ValidatedStars {
        count: count as i32,
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut bytes = b"STR1\x01\0\0\0".to_vec();
        for value in [x, y, z, 2.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[255, 255, 255, 255]);
        bytes
    }

    #[test]
    fn accepts_the_generated_catalog() {
        let bytes = include_bytes!("../../public/stars/bright.bin");
        let stars = validate(bytes).expect("generated catalog is valid");
        assert_eq!(stars.count, 12_191);
        assert_eq!(stars.records.len(), 12_191 * STRIDE);
    }

    #[test]
    fn rejects_zero_unbounded_and_length_mismatched_counts() {
        assert_eq!(
            validate(b"STR1\0\0\0\0").err(),
            Some("star count out of bounds")
        );
        assert_eq!(
            validate(b"STR1\xff\xff\xff\xff").err(),
            Some("star count out of bounds")
        );
        let mut truncated = record(1.0, 0.0, 0.0);
        truncated.pop();
        assert_eq!(
            validate(&truncated).err(),
            Some("star asset length mismatch")
        );
    }

    #[test]
    fn rejects_zero_nonfinite_and_nonunit_vectors() {
        assert_eq!(
            validate(&record(0.0, 0.0, 0.0)).err(),
            Some("invalid star record")
        );
        assert_eq!(
            validate(&record(f32::NAN, 0.0, 1.0)).err(),
            Some("invalid star record")
        );
        assert_eq!(
            validate(&record(2.0, 0.0, 0.0)).err(),
            Some("invalid star record")
        );
    }
}
