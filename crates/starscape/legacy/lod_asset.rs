//! Validation for the generated Gaia DR3 regional star assets.

use serde::Deserialize;

pub const MAGIC: &[u8; 8] = b"GDR3LOD1";
pub const HEADER_LEN: usize = 12;
pub const RECORD_LEN: usize = 16;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub record_bytes: usize,
    pub ra_bins: usize,
    pub dec_bins: usize,
    pub tiles: Vec<Tile>,
}

#[derive(Deserialize)]
pub struct Tile {
    pub id: usize,
    pub offset: usize,
    pub bytes: usize,
    pub count: usize,
}

pub fn validate_binary(bytes: &[u8]) -> Result<usize, &'static str> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err("bad Gaia LOD header");
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| "bad Gaia count")?) as usize;
    if bytes.len()
        != HEADER_LEN
            + count
                .checked_mul(RECORD_LEN)
                .ok_or("Gaia length overflow")?
    {
        return Err("Gaia LOD length mismatch");
    }
    Ok(count)
}

pub fn validate_manifest(json: &[u8], binary_len: usize) -> Result<usize, &'static str> {
    let manifest: Manifest = serde_json::from_slice(json).map_err(|_| "bad Gaia manifest JSON")?;
    if manifest.version != 1
        || manifest.record_bytes != RECORD_LEN
        || manifest.ra_bins != 32
        || manifest.dec_bins != 24
        || manifest.tiles.len() != 768
    {
        return Err("bad Gaia manifest shape");
    }
    let mut offset = HEADER_LEN;
    let mut count = 0usize;
    for (expected_id, tile) in manifest.tiles.iter().enumerate() {
        if tile.id != expected_id
            || tile.offset != offset
            || tile.bytes
                != tile
                    .count
                    .checked_mul(RECORD_LEN)
                    .ok_or("Gaia tile overflow")?
        {
            return Err("bad Gaia tile index");
        }
        offset = offset.checked_add(tile.bytes).ok_or("Gaia tile overflow")?;
        count = count.checked_add(tile.count).ok_or("Gaia tile overflow")?;
    }
    if offset != binary_len {
        return Err("Gaia manifest length mismatch");
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_gaia_assets_are_complete_when_present() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("public/stars/lod");
        let binary_path = root.join("g12.bin");
        if !binary_path.exists() {
            return;
        }
        let binary = std::fs::read(binary_path).unwrap();
        let manifest = std::fs::read(root.join("manifest.json")).unwrap();
        let binary_count = validate_binary(&binary).unwrap();
        let manifest_count = validate_manifest(&manifest, binary.len()).unwrap();
        assert_eq!(binary_count, manifest_count);
        assert!(binary_count > 3_000_000);

        let mid = std::fs::read(root.join("g9.bin")).unwrap();
        assert!(validate_binary(&mid).unwrap() > 170_000);
    }
}
