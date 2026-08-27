//! Nearest-city lookup over the compact GeoNames catalog.
//!
//! Source data: the GeoNames `cities15000` export (CC BY 4.0) filtered by
//! `tools/citycat.py` into `static/cities/cities.bin`. The browser fetches it
//! separately rather than having 100 KB embedded in every binary, so the
//! grounding label is honestly empty until the fetch lands rather than showing
//! a place the observer is not over.
//!
//! Distance is a spherical great-circle distance, so the antimeridian and the
//! poles are ordinary inputs rather than special cases.

const MAGIC: &[u8; 4] = b"CTY1";
const HEADER_LEN: usize = 8;
const RECORD_LEN: usize = 16;
const MAX_CITIES: usize = 100_000;
pub const EARTH_MEAN_RADIUS_KM: f64 = 6_371.008_8;

/// A nearest-city answer, borrowing its text from the validated catalog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct City<'a> {
    pub name: &'a str,
    /// ISO-3166 alpha-2 country code, as GeoNames records it.
    pub country: &'a str,
    pub distance_km: f64,
}

/// A structurally validated catalog. Every string range is checked once, on
/// construction, so a scan cannot fail halfway through and leave the label
/// showing a city from a corrupt record.
pub struct CityCatalog {
    bytes: Vec<u8>,
    count: usize,
}

#[derive(Clone, Copy)]
struct Record<'a> {
    name: &'a str,
    country: &'a str,
    lat_deg: f64,
    lon_deg: f64,
}

impl CityCatalog {
    pub fn parse(bytes: Vec<u8>) -> Option<Self> {
        let count = count(&bytes)?;
        for index in 0..count {
            record(&bytes, index, count)?;
        }
        Some(Self { bytes, count })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The nearest catalog city and its great-circle distance, or `None` for a
    /// coordinate that is not a point on Earth.
    #[must_use]
    pub fn nearest(&self, lat_deg: f64, lon_deg: f64) -> Option<City<'_>> {
        if !lat_deg.is_finite() || !lon_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
            return None;
        }
        let mut best: Option<(Record<'_>, f64)> = None;
        for index in 0..self.count {
            let candidate = record(&self.bytes, index, self.count)?;
            let distance = distance_km(lat_deg, lon_deg, candidate.lat_deg, candidate.lon_deg);
            if best.is_none_or(|(_, best)| distance < best) {
                best = Some((candidate, distance));
            }
        }
        best.map(|(record, distance_km)| City {
            name: record.name,
            country: record.country,
            distance_km,
        })
    }
}

fn count(bytes: &[u8]) -> Option<usize> {
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

fn record(bytes: &[u8], index: usize, total: usize) -> Option<Record<'_>> {
    let start = HEADER_LEN.checked_add(index.checked_mul(RECORD_LEN)?)?;
    let entry = bytes.get(start..start.checked_add(RECORD_LEN)?)?;
    let names_start = HEADER_LEN.checked_add(total.checked_mul(RECORD_LEN)?)?;

    let lat_micro = i32::from_le_bytes(entry[0..4].try_into().ok()?);
    let lon_micro = i32::from_le_bytes(entry[4..8].try_into().ok()?);
    let name_offset = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
    let name_len = u16::from_le_bytes(entry[12..14].try_into().ok()?) as usize;
    let country = std::str::from_utf8(&entry[14..16]).ok()?;
    let name_start = names_start.checked_add(name_offset)?;
    let name =
        std::str::from_utf8(bytes.get(name_start..name_start.checked_add(name_len)?)?).ok()?;

    Some(Record {
        name,
        country,
        lat_deg: f64::from(lat_micro) / 1_000_000.0,
        lon_deg: f64::from(lon_micro) / 1_000_000.0,
    })
}

#[must_use]
pub fn distance_km(lat1_deg: f64, lon1_deg: f64, lat2_deg: f64, lon2_deg: f64) -> f64 {
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
        CityCatalog::parse(include_bytes!("../../../static/cities/cities.bin").to_vec())
            .expect("the shipped city catalog is valid")
    }

    #[test]
    fn the_shipped_catalog_parses_and_has_its_filtered_size() {
        assert_eq!(catalog().len(), 4_063);
    }

    #[test]
    fn hand_checked_land_points_resolve_to_the_expected_city() {
        let catalog = catalog();
        for (lat, lon, name, country, within_km) in [
            (40.73, -73.99, "New York City", "US", 5.0),
            (35.6895, 139.6917, "Tokyo", "JP", 1.0),
            (-33.87, 151.21, "Sydney", "AU", 10.0),
        ] {
            let city = catalog.nearest(lat, lon).expect("a nearest city");
            assert_eq!((city.name, city.country), (name, country));
            assert!(
                city.distance_km < within_km,
                "{name} was {} km",
                city.distance_km
            );
        }
    }

    #[test]
    fn a_point_just_west_of_the_antimeridian_finds_the_city_just_east_of_it() {
        let catalog = catalog();
        let city = catalog.nearest(64.7, -179.0).expect("a nearest city");
        assert_eq!((city.name, city.country), ("Anadyr", "RU"));
        assert!(city.distance_km < 200.0);
    }

    #[test]
    fn the_open_ocean_reports_an_honest_distance_rather_than_nothing() {
        let catalog = catalog();
        let city = catalog.nearest(0.0, -140.0).expect("a nearest city");
        assert_eq!((city.name, city.country), ("Honolulu", "US"));
        assert!(city.distance_km > 2_000.0);
    }

    #[test]
    fn a_truncated_or_tampered_asset_is_refused_whole() {
        assert!(CityCatalog::parse(Vec::new()).is_none());
        assert!(CityCatalog::parse(b"CTY1\0\0\0\0".to_vec()).is_none());
        assert!(CityCatalog::parse(b"CTY1\x01\0\0\0".to_vec()).is_none());
        let mut tampered = include_bytes!("../../../static/cities/cities.bin").to_vec();
        tampered[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CityCatalog::parse(tampered).is_none());
    }

    #[test]
    fn a_coordinate_that_is_not_a_point_on_earth_has_no_nearest_city() {
        let catalog = catalog();
        assert!(catalog.nearest(91.0, 0.0).is_none());
        assert!(catalog.nearest(f64::NAN, 0.0).is_none());
        assert!(catalog.nearest(0.0, f64::INFINITY).is_none());
    }

    #[test]
    fn the_distance_function_has_its_exact_quarter_and_antipodal_values() {
        let quarter = distance_km(0.0, 0.0, 0.0, 90.0);
        let antipodal = distance_km(0.0, 0.0, 0.0, 180.0);
        assert!((quarter - std::f64::consts::FRAC_PI_2 * EARTH_MEAN_RADIUS_KM).abs() < 1e-9);
        assert!((antipodal - std::f64::consts::PI * EARTH_MEAN_RADIUS_KM).abs() < 1e-9);
    }
}
