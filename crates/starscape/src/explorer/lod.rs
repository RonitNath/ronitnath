//! The regional Gaia catalog: its record format, and which tiles a view needs.
//!
//! `g12.bin` is one 49 MB file of 16-byte records, grouped into 768 sky tiles
//! whose byte offsets `manifest.json` publishes. Nothing ever downloads the
//! file: the client reads the manifest, works out which tiles the current view
//! covers, and asks the server for those byte ranges. `g9.bin` is the same
//! record format at a coarser limit and is small enough to fetch whole.
//!
//! Everything here is arithmetic on bytes and angles, so it is all testable
//! without a browser — which is the point of keeping it out of the render loop.

use std::f64::consts::TAU;

use serde::Deserialize;

pub const MAGIC: &[u8; 8] = b"GDR3LOD1";
pub const HEADER_LEN: usize = 12;
pub const RECORD_LEN: usize = 16;

/// Below this field of view the mid catalog is worth its 2.8 MB.
pub const MID_FOV_DEG: f64 = 60.0;
/// Below this one, individual regional tiles are worth a request each.
pub const DEEP_FOV_DEG: f64 = 25.0;

/// One decoded star: a J2000 unit vector, a G magnitude, and a colour.
#[derive(Debug, Clone, Copy)]
pub struct Star {
    pub pos: [f32; 3],
    pub mag: f32,
    pub color: [u8; 3],
}

/// Octahedral unit-vector decoding, the encoding the builder writes.
#[must_use]
pub fn oct_decode(x: i16, y: i16) -> [f32; 3] {
    let (mut px, mut py) = (f64::from(x) / 32_767.0, f64::from(y) / 32_767.0);
    let pz = 1.0 - px.abs() - py.abs();
    if pz < 0.0 {
        let ox = px;
        px = (1.0 - py.abs()) * sign(ox);
        py = (1.0 - ox.abs()) * sign(py);
    }
    let norm = (px * px + py * py + pz * pz).sqrt();
    if norm == 0.0 {
        return [0.0, 0.0, 1.0];
    }
    [(px / norm) as f32, (py / norm) as f32, (pz / norm) as f32]
}

/// `signum`, but zero counts as positive — the encoder's convention.
fn sign(value: f64) -> f64 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

/// A compact perceptual read of Gaia's BP−RP colour index: blue below zero,
/// warm past two. Not a spectrum, and not claimed to be one — it is the same
/// ramp the shipped catalog was designed against.
#[must_use]
pub fn colour(bp_rp: f64) -> [u8; 3] {
    let t = ((bp_rp + 0.4) / 2.8).clamp(0.0, 1.0);
    [
        (190.0 + 65.0 * t).round() as u8,
        (216.0 + 26.0 * t).round() as u8,
        (255.0 - 115.0 * t).round() as u8,
    ]
}

/// Decode a run of records. A trailing partial record is dropped rather than
/// guessed at: a tile that does not divide by the record length is a tile the
/// client would otherwise decode one field out of phase.
#[must_use]
pub fn decode(bytes: &[u8]) -> Vec<Star> {
    bytes
        .chunks_exact(RECORD_LEN)
        .map(|record| {
            let at = |offset: usize| {
                i16::from_le_bytes(record[offset..offset + 2].try_into().expect("two bytes"))
            };
            Star {
                pos: oct_decode(at(8), at(10)),
                mag: f32::from(at(12)) / 1_000.0,
                color: colour(f64::from(at(14)) / 1_000.0),
            }
        })
        .collect()
}

/// Decode a whole file, header and all.
pub fn decode_file(bytes: &[u8]) -> Result<Vec<Star>, &'static str> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err("bad Gaia LOD header");
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| "bad Gaia count")?) as usize;
    let stars = decode(&bytes[HEADER_LEN..]);
    if stars.len() != count {
        return Err("Gaia LOD length mismatch");
    }
    Ok(stars)
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Tile {
    pub id: usize,
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub record_bytes: usize,
    pub ra_bins: usize,
    pub dec_bins: usize,
    pub tiles: Vec<Tile>,
}

impl Manifest {
    /// Parse and check the shape. A manifest that disagrees with the binary
    /// about the record length would have every tile decode as noise, so the
    /// mismatch is refused here rather than rendered.
    pub fn parse(json: &str) -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        if manifest.version != 1 {
            return Err(format!("unknown manifest version {}", manifest.version));
        }
        if manifest.record_bytes != RECORD_LEN {
            return Err(format!("record length {}", manifest.record_bytes));
        }
        if manifest.ra_bins == 0 || manifest.dec_bins == 0 {
            return Err("a manifest with no bins".to_string());
        }
        if manifest.tiles.len() != manifest.ra_bins * manifest.dec_bins {
            return Err(format!("{} tiles for the bin grid", manifest.tiles.len()));
        }
        for (expected, tile) in manifest.tiles.iter().enumerate() {
            if tile.id != expected {
                return Err(format!("tile {} out of order", tile.id));
            }
            if tile.bytes % RECORD_LEN as u64 != 0 {
                return Err(format!("tile {} is not whole records", tile.id));
            }
        }
        Ok(manifest)
    }

    /// The tile ids covering a view centred on `(ra, dec)` radians with a
    /// vertical field of `fov` degrees on a viewport of the given aspect.
    ///
    /// Empty above [`DEEP_FOV_DEG`]: a wide view covers most of the sky, and
    /// downloading most of a 49 MB catalog to draw stars smaller than a pixel
    /// is the exact failure the tiling exists to prevent.
    #[must_use]
    pub fn tiles_for(&self, ra: f64, dec: f64, fov: f64, aspect: f64) -> Vec<usize> {
        if fov > DEEP_FOV_DEG {
            return Vec::new();
        }
        let ra_bin = ra.rem_euclid(TAU) / TAU * self.ra_bins as f64;
        let dec_bin =
            (dec + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI * self.dec_bins as f64;

        let half_v = fov / 2.0;
        let half_h = (half_v.to_radians().tan() * aspect).atan().to_degrees();
        // Meridians converge toward the poles, so a fixed angular width spans
        // more right-ascension bins the further north or south the view is.
        let cos_dec = dec.cos().max(0.2);
        let rx = (half_h / (360.0 / self.ra_bins as f64) / cos_dec)
            .ceil()
            .max(0.0) as i64
            + 1;
        let ry = (half_v / (180.0 / self.dec_bins as f64)).ceil().max(0.0) as i64 + 1;

        let ra_bins = self.ra_bins as i64;
        let mut ids = Vec::new();
        for y in (dec_bin.floor() as i64 - ry)..=(dec_bin.floor() as i64 + ry) {
            if y < 0 || y >= self.dec_bins as i64 {
                continue;
            }
            for x in (ra_bin.floor() as i64 - rx)..=(ra_bin.floor() as i64 + rx) {
                let id = (y * ra_bins + x.rem_euclid(ra_bins)) as usize;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = include_str!("../../../../static/stars/lod/manifest.json");

    #[test]
    fn octahedral_decoding_returns_unit_vectors_over_the_whole_encoding_range() {
        for x in [-32_767, -12_345, 0, 7, 32_767] {
            for y in [-32_767, -999, 0, 4_242, 32_767] {
                let v = oct_decode(x, y);
                let norm = f64::from(v[0])
                    .hypot(f64::from(v[1]))
                    .hypot(f64::from(v[2]));
                assert!((norm - 1.0).abs() < 1e-5, "({x},{y}) gave {norm}");
            }
        }
        // The +z pole is the origin of the encoding.
        assert!(oct_decode(0, 0)[2] > 0.999);
    }

    #[test]
    fn the_colour_ramp_runs_blue_to_warm_and_never_leaves_the_byte_range() {
        let blue = colour(-2.0);
        let warm = colour(4.0);
        assert!(blue[2] > warm[2], "blue end should carry more blue");
        assert!(warm[0] > blue[0], "warm end should carry more red");
        assert_eq!(colour(-0.4), [190, 216, 255]);
    }

    #[test]
    fn a_run_of_records_decodes_and_a_partial_tail_is_dropped_not_guessed() {
        let mut bytes = vec![0u8; RECORD_LEN * 2];
        bytes[8..10].copy_from_slice(&0i16.to_le_bytes());
        bytes[12..14].copy_from_slice(&9_500i16.to_le_bytes());
        let stars = decode(&bytes);
        assert_eq!(stars.len(), 2);
        assert!((stars[0].mag - 9.5).abs() < 1e-4);

        bytes.push(0);
        assert_eq!(decode(&bytes).len(), 2);
    }

    #[test]
    fn a_file_whose_header_disagrees_with_its_body_is_refused() {
        let mut file = MAGIC.to_vec();
        file.extend_from_slice(&2u32.to_le_bytes());
        file.extend_from_slice(&[0; RECORD_LEN]);
        assert_eq!(decode_file(&file).err(), Some("Gaia LOD length mismatch"));
        assert_eq!(decode_file(b"NOPE").err(), Some("bad Gaia LOD header"));

        file[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(decode_file(&file).expect("one record").len(), 1);
    }

    #[test]
    fn the_shipped_manifest_indexes_the_shipped_catalog_exactly() {
        let manifest = Manifest::parse(MANIFEST).expect("the shipped manifest");
        assert_eq!((manifest.ra_bins, manifest.dec_bins), (32, 24));
        assert_eq!(manifest.tiles.len(), 768);

        // Contiguous from the header to the last byte of the file.
        let mut offset = HEADER_LEN as u64;
        for tile in &manifest.tiles {
            assert_eq!(
                tile.offset, offset,
                "tile {} starts in the wrong place",
                tile.id
            );
            offset += tile.bytes;
        }
        let length = std::fs::metadata(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../static/stars/lod/g12.bin"
        ))
        .expect("the shipped catalog")
        .len();
        assert_eq!(offset, length, "the manifest does not cover the file");
    }

    #[test]
    fn a_wide_view_asks_for_no_tiles_at_all() {
        let manifest = Manifest::parse(MANIFEST).expect("the shipped manifest");
        assert!(manifest.tiles_for(1.0, 0.2, 115.0, 1.6).is_empty());
        assert!(
            manifest
                .tiles_for(1.0, 0.2, DEEP_FOV_DEG + 0.1, 1.6)
                .is_empty()
        );
        assert!(!manifest.tiles_for(1.0, 0.2, DEEP_FOV_DEG, 1.6).is_empty());
    }

    #[test]
    fn a_narrow_view_asks_for_a_handful_of_neighbouring_tiles_and_wraps_at_ra_zero() {
        let manifest = Manifest::parse(MANIFEST).expect("the shipped manifest");
        let ids = manifest.tiles_for(0.01, 0.0, 8.0, 1.6);
        assert!((6..=36).contains(&ids.len()), "{} tiles", ids.len());
        assert!(ids.iter().all(|id| *id < manifest.tiles.len()));
        // Just east of RA 0 the window straddles the wrap, so both the first
        // and the last right-ascension bin of that row are wanted.
        let row = ids[0] / manifest.ra_bins;
        assert!(ids.contains(&(row * manifest.ra_bins)));
        assert!(ids.contains(&(row * manifest.ra_bins + manifest.ra_bins - 1)));
    }

    #[test]
    fn the_polar_window_widens_in_right_ascension_rather_than_missing_stars() {
        let manifest = Manifest::parse(MANIFEST).expect("the shipped manifest");
        let equator = manifest.tiles_for(1.0, 0.0, 20.0, 1.6).len();
        let polar = manifest.tiles_for(1.0, 1.4, 20.0, 1.6).len();
        assert!(polar > equator, "{polar} is not more than {equator}");
    }

    #[test]
    fn every_wanted_tile_is_one_the_manifest_can_be_asked_for() {
        let manifest = Manifest::parse(MANIFEST).expect("the shipped manifest");
        for dec in [-1.5, -0.7, 0.0, 0.7, 1.5] {
            for ra in [0.0, 1.0, 3.0, 6.2] {
                for id in manifest.tiles_for(ra, dec, 12.0, 1.6) {
                    let tile = manifest.tiles.get(id).expect("a tile the manifest holds");
                    assert_eq!(tile.id, id);
                }
            }
        }
    }
}
