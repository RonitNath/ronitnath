//! Compact GeoNames nearest-city lookup for the starscape.
//!
//! Source data: GeoNames `cities15000` export, CC BY 4.0, filtered by
//! `tools/citycat.py` into `public/cities/cities.bin`. The browser fetches the
//! catalog independently of the wasm bundle. Distance is a spherical
//! great-circle distance, so antimeridian and polar inputs do not suffer
//! longitude-wrap discontinuities.

const MAGIC: &[u8; 4] = b"CTY1";
const HEADER_LEN: usize = 8;
const RECORD_LEN: usize = 16;
const MAX_CITIES: usize = 100_000;
pub const EARTH_MEAN_RADIUS_KM: f64 = 6_371.008_8;

/// Nearest city result borrowing its text from a validated catalog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct City<'a> {
    pub name: &'a str,
    /// ISO-3166 alpha-2 country code from GeoNames.
    pub country: &'a str,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub distance_km: f64,
}

#[derive(Clone, Copy)]
struct CityRecord<'a> {
    name: &'a str,
    country: &'a str,
    lat_deg: f64,
    lon_deg: f64,
}

/// Owned, structurally validated city asset.
pub struct CityCatalog {
    bytes: Vec<u8>,
    count: usize,
}

impl CityCatalog {
    /// Validate and retain a fetched CTY1 asset. Every string range is checked
    /// up front, so later nearest-city scans cannot fail halfway through.
    pub fn parse(bytes: Vec<u8>) -> Option<Self> {
        let count = parsed_count(&bytes)?;
        for idx in 0..count {
            record_at(&bytes, idx, count)?;
        }
        Some(Self { bytes, count })
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.count
    }

    /// Return the nearest catalog city and its great-circle distance in km.
    pub fn nearest(&self, lat_deg: f64, lon_deg: f64) -> Option<City<'_>> {
        if !lat_deg.is_finite() || !lon_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
            return None;
        }

        let mut best: Option<(CityRecord<'_>, f64)> = None;
        for idx in 0..self.count {
            let record = record_at(&self.bytes, idx, self.count)?;
            let distance_km =
                great_circle_distance_km(lat_deg, lon_deg, record.lat_deg, record.lon_deg);
            if best.map(|(_, d)| distance_km < d).unwrap_or(true) {
                best = Some((record, distance_km));
            }
        }

        best.map(|(record, distance_km)| City {
            name: record.name,
            country: record.country,
            lat_deg: record.lat_deg,
            lon_deg: record.lon_deg,
            distance_km,
        })
    }
}

fn parsed_count(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < HEADER_LEN || &bytes[0..4] != MAGIC {
        return None;
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    if count == 0 || count > MAX_CITIES {
        return None;
    }
    let names_start = HEADER_LEN.checked_add(count.checked_mul(RECORD_LEN)?)?;
    (names_start <= bytes.len()).then_some(count)
}

fn record_at(bytes: &[u8], idx: usize, count: usize) -> Option<CityRecord<'_>> {
    let start = HEADER_LEN.checked_add(idx.checked_mul(RECORD_LEN)?)?;
    let rec = bytes.get(start..start.checked_add(RECORD_LEN)?)?;
    let names_start = HEADER_LEN.checked_add(count.checked_mul(RECORD_LEN)?)?;

    let lat_micro = i32::from_le_bytes(rec[0..4].try_into().ok()?);
    let lon_micro = i32::from_le_bytes(rec[4..8].try_into().ok()?);
    let name_offset = u32::from_le_bytes(rec[8..12].try_into().ok()?) as usize;
    let name_len = u16::from_le_bytes(rec[12..14].try_into().ok()?) as usize;
    let country = std::str::from_utf8(&rec[14..16]).ok()?;
    let name_start = names_start.checked_add(name_offset)?;
    let name_end = name_start.checked_add(name_len)?;
    let name = std::str::from_utf8(bytes.get(name_start..name_end)?).ok()?;

    Some(CityRecord {
        name,
        country,
        lat_deg: f64::from(lat_micro) / 1_000_000.0,
        lon_deg: f64::from(lon_micro) / 1_000_000.0,
    })
}

fn great_circle_distance_km(lat1_deg: f64, lon1_deg: f64, lat2_deg: f64, lon2_deg: f64) -> f64 {
    let (lat1, lon1) = (lat1_deg.to_radians(), lon1_deg.to_radians());
    let (lat2, lon2) = (lat2_deg.to_radians(), lon2_deg.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (lon2 - lon1 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_MEAN_RADIUS_KM * h.sqrt().min(1.0).asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CityCatalog {
        CityCatalog::parse(include_bytes!("../../public/cities/cities.bin").to_vec())
            .expect("valid city fixture")
    }

    fn assert_nearest(
        catalog: &CityCatalog,
        lat: f64,
        lon: f64,
        name: &str,
        country: &str,
        max_km: f64,
    ) {
        let city = catalog.nearest(lat, lon).expect("nearest city");
        assert_eq!(city.name, name);
        assert_eq!(city.country, country);
        assert!(
            city.distance_km < max_km,
            "{} was {} km away",
            city.name,
            city.distance_km
        );
    }

    #[test]
    fn city_asset_has_expected_filter_size() {
        assert_eq!(catalog().len(), 4_063);
    }

    #[test]
    fn parser_rejects_empty_truncated_and_bad_string_assets() {
        assert!(CityCatalog::parse(Vec::new()).is_none());
        assert!(CityCatalog::parse(b"CTY1\0\0\0\0".to_vec()).is_none());
        assert!(CityCatalog::parse(b"CTY1\x01\0\0\0".to_vec()).is_none());
        let mut bad = include_bytes!("../../public/cities/cities.bin").to_vec();
        bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CityCatalog::parse(bad).is_none());
    }

    #[test]
    fn nearest_city_finds_hand_checked_land_points() {
        let catalog = catalog();
        assert_nearest(&catalog, 40.73, -73.99, "New York City", "US", 5.0);
        assert_nearest(&catalog, 35.6895, 139.6917, "Tokyo", "JP", 1.0);
        assert_nearest(&catalog, -33.87, 151.21, "Sydney", "AU", 10.0);
    }

    #[test]
    fn nearest_city_handles_antimeridian_wraparound() {
        let catalog = catalog();
        let city = catalog.nearest(64.7, -179.0).expect("nearest city");
        assert_eq!((city.name, city.country), ("Anadyr", "RU"));
        assert!(city.distance_km < 200.0 && city.lon_deg > 170.0);
    }

    #[test]
    fn nearest_city_reports_deep_ocean_distance() {
        let catalog = catalog();
        let city = catalog.nearest(0.0, -140.0).expect("nearest city");
        assert_eq!((city.name, city.country), ("Honolulu", "US"));
        assert!(city.distance_km > 2_000.0);
    }

    #[test]
    fn great_circle_distance_has_exact_quarter_and_antipodal_values() {
        let quarter = great_circle_distance_km(0.0, 0.0, 0.0, 90.0);
        let antipodal = great_circle_distance_km(0.0, 0.0, 0.0, 180.0);
        assert!((quarter - std::f64::consts::FRAC_PI_2 * EARTH_MEAN_RADIUS_KM).abs() < 1e-9);
        assert!((antipodal - std::f64::consts::PI * EARTH_MEAN_RADIUS_KM).abs() < 1e-9);
    }

    #[test]
    fn nearest_city_rejects_invalid_coordinates() {
        let catalog = catalog();
        assert!(catalog.nearest(91.0, 0.0).is_none());
        assert!(catalog.nearest(f64::NAN, 0.0).is_none());
        assert!(catalog.nearest(0.0, f64::INFINITY).is_none());
    }
}
