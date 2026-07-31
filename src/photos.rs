//! Inline, bounded photo decoding and EXIF-free variant generation.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::store::Store;
use crate::store::photos::{NewPhoto, PhotoVariant};

/// Defense-in-depth defaults checked from the encoded header before decoding.
pub const DEFAULT_MAX_IMAGE_SIDE: u32 = 8_192;
pub const DEFAULT_MAX_IMAGE_PIXELS: u64 = 50_000_000;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct UploadAttribution {
    pub identity_id: Option<i64>,
    pub person_id: Option<i64>,
}

pub struct ProcessedPhoto {
    pub hash: String,
    pub width: u32,
    pub height: u32,
    pub taken_at: Option<String>,
    variants: Vec<EncodedVariant>,
}

struct EncodedVariant {
    kind: &'static str,
    filename: String,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::WebP)
    } else {
        None
    }
}

fn exif_taken_at(bytes: &[u8], format: ImageFormat) -> Option<String> {
    if format != ImageFormat::Jpeg {
        return None;
    }
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let field = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
    let raw = match &field.value {
        exif::Value::Ascii(values) => std::str::from_utf8(values.first()?)
            .ok()?
            .trim_end_matches('\0'),
        _ => return None,
    };
    let raw = raw.as_bytes();
    // EXIF uses an ASCII-only fixed form. Validate bytes before indexing so
    // malformed UTF-8/multibyte values can never panic at string boundaries.
    if raw.len() != 19
        || !raw.is_ascii()
        || !raw[..4].iter().all(u8::is_ascii_digit)
        || raw[4] != b':'
        || !raw[5..7].iter().all(u8::is_ascii_digit)
        || raw[7] != b':'
        || !raw[8..10].iter().all(u8::is_ascii_digit)
        || raw[10] != b' '
        || !raw[11..13].iter().all(u8::is_ascii_digit)
        || raw[13] != b':'
        || !raw[14..16].iter().all(u8::is_ascii_digit)
        || raw[16] != b':'
        || !raw[17..19].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{}:{}",
        std::str::from_utf8(&raw[..4]).ok()?,
        std::str::from_utf8(&raw[5..7]).ok()?,
        std::str::from_utf8(&raw[8..10]).ok()?,
        std::str::from_utf8(&raw[11..13]).ok()?,
        std::str::from_utf8(&raw[14..16]).ok()?,
        std::str::from_utf8(&raw[17..19]).ok()?,
    ))
}

fn webp(image: &DynamicImage) -> Result<Vec<u8>, AppError> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::WebP)
        .map_err(|e| AppError::Invalid(format!("could not encode photo: {e}")))?;
    Ok(out.into_inner())
}

fn bounded(image: &DynamicImage, max: u32) -> DynamicImage {
    if image.width() <= max && image.height() <= max {
        image.clone()
    } else {
        image.thumbnail(max, max)
    }
}

fn decode_limits(max_pixels: u64, max_side: u32) -> Limits {
    // Allocation limit counts output pixels plus decoder scratch. 16 bytes per
    // configured pixel gives enough headroom for RGBA output while still
    // preventing decompression bombs from allocating past the configured cap.
    let mut limits = Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(max_pixels.saturating_mul(16));
    limits
}

pub fn process_with_limits(
    bytes: &[u8],
    max_pixels: u64,
    max_side: u32,
) -> Result<ProcessedPhoto, AppError> {
    let format =
        sniff(bytes).ok_or_else(|| AppError::Invalid("photo must be JPEG, PNG, or WebP".into()))?;
    // Read only the encoded header first. Explicitly classify an oversized
    // declaration as 413 before constructing any decoder pixel buffer.
    let header_reader = ImageReader::with_format(Cursor::new(bytes), format);
    let (header_width, header_height) = header_reader
        .into_dimensions()
        .map_err(|_| AppError::Invalid("photo dimensions could not be read".into()))?;
    let pixels = u64::from(header_width)
        .checked_mul(u64::from(header_height))
        .ok_or_else(|| AppError::Invalid("photo dimensions are too large".into()))?;
    if header_width > max_side || header_height > max_side || pixels > max_pixels {
        return Err(AppError::PayloadTooLarge);
    }
    let taken_at = exif_taken_at(bytes, format);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits(max_pixels, max_side));
    let image = reader
        .decode()
        .map_err(|_| AppError::Invalid("photo could not be decoded".into()))?;
    let (width, height) = image.dimensions();
    if width > max_side || height > max_side || u64::from(width) * u64::from(height) > max_pixels {
        return Err(AppError::PayloadTooLarge);
    }

    // The stored "original" is normalized to lossless WebP. Re-encoding all
    // three outputs from decoded pixels guarantees no EXIF/GPS survives.
    let original = webp(&image)?;
    let hash = format!("{:x}", Sha256::digest(&original));
    let thumb_image = bounded(&image, 320);
    let medium_image = bounded(&image, 1280);
    let variants = vec![
        EncodedVariant {
            kind: "original",
            filename: format!("{hash}.webp"),
            width,
            height,
            bytes: original,
        },
        EncodedVariant {
            kind: "thumb",
            filename: format!("{hash}.thumb.webp"),
            width: thumb_image.width(),
            height: thumb_image.height(),
            bytes: webp(&thumb_image)?,
        },
        EncodedVariant {
            kind: "medium",
            filename: format!("{hash}.medium.webp"),
            width: medium_image.width(),
            height: medium_image.height(),
            bytes: webp(&medium_image)?,
        },
    ];
    Ok(ProcessedPhoto {
        hash,
        width,
        height,
        taken_at,
        variants,
    })
}

pub fn process(bytes: &[u8]) -> Result<ProcessedPhoto, AppError> {
    process_with_limits(bytes, DEFAULT_MAX_IMAGE_PIXELS, DEFAULT_MAX_IMAGE_SIDE)
}

pub fn event_dir(root: &Path, account_id: i64, event_id: i64) -> PathBuf {
    root.join(account_id.to_string()).join(event_id.to_string())
}

pub async fn persist(
    store: &Store,
    root: &Path,
    account_id: i64,
    event_id: i64,
    filename: &str,
    caption: &str,
    attribution: UploadAttribution,
    processed: ProcessedPhoto,
) -> Result<i64, AppError> {
    let dir = event_dir(root, account_id, event_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(anyhow::Error::from)?;
    let mut rows = Vec::with_capacity(3);
    let mut staged = Vec::with_capacity(3);
    for variant in &processed.variants {
        let path = dir.join(&variant.filename);
        let temp = dir.join(format!(
            ".{}.{}.{}.tmp",
            variant.filename,
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        tokio::fs::write(&temp, &variant.bytes)
            .await
            .map_err(anyhow::Error::from)?;
        staged.push((temp, path));
        rows.push(PhotoVariant {
            kind: variant.kind.into(),
            storage_key: variant.filename.clone(),
            width: Some(i64::from(variant.width)),
            height: Some(i64::from(variant.height)),
            byte_size: variant.bytes.len() as i64,
        });
    }
    let original_size = processed.variants[0].bytes.len() as i64;
    // Cross-process protocol: SQLite's writer lock spans the invisible row
    // insert, atomic file publication, and commit. GC takes that same lock.
    let ingest = store
        .begin_photo_ingest(
            &NewPhoto {
                account_id,
                event_id,
                uploaded_by_identity_id: attribution.identity_id,
                uploaded_by_person_id: attribution.person_id,
                storage_key: &processed.hash,
                original_filename: filename,
                mime_type: "image/webp",
                byte_size: original_size,
                width: i64::from(processed.width),
                height: i64::from(processed.height),
                caption,
                taken_at: processed.taken_at.as_deref(),
            },
            &rows,
        )
        .await?;
    for (temp, path) in &staged {
        if tokio::fs::metadata(path).await.is_ok() {
            tokio::fs::remove_file(temp)
                .await
                .map_err(anyhow::Error::from)?;
        } else {
            tokio::fs::rename(temp, path)
                .await
                .map_err(anyhow::Error::from)?;
        }
    }
    let id = ingest.id;
    ingest.commit().await?;
    Ok(id)
}

pub async fn gc(
    store: &Store,
    root: &Path,
    account_id: i64,
    older_than_days: i64,
) -> anyhow::Result<usize> {
    let candidates = store
        .gc_photo_candidates(account_id, older_than_days)
        .await?;
    let mut purged = 0;
    for photo in candidates {
        // Hold SQLite's writer lock across the final reference check, unlink,
        // and purge. Ingest holds the same lock from row insert through atomic
        // file publication, so neither operation can invalidate the other.
        let guard = store
            .begin_photo_gc(account_id, photo.event_id, photo.id, &photo.storage_key)
            .await?;
        if !guard.has_live_reference {
            let dir = event_dir(root, account_id, photo.event_id);
            for suffix in [".webp", ".thumb.webp", ".medium.webp"] {
                let _ =
                    tokio::fs::remove_file(dir.join(format!("{}{}", photo.storage_key, suffix)))
                        .await;
            }
        }
        purged += guard.purge_and_commit().await? as usize;
    }
    Ok(purged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_with_exif() -> Vec<u8> {
        let image = DynamicImage::new_rgb8(2, 2);
        let mut jpeg = Cursor::new(Vec::new());
        image.write_to(&mut jpeg, ImageFormat::Jpeg).unwrap();
        let jpeg = jpeg.into_inner();

        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        for (tag, offset) in [(0x8769u16, 38u32), (0x8825u16, 56u32)] {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&4u16.to_le_bytes());
            tiff.extend_from_slice(&1u32.to_le_bytes());
            tiff.extend_from_slice(&offset.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&74u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&2u32.to_le_bytes());
        tiff.extend_from_slice(b"N\0\0\0");
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(b"2026:07:04 13:14:15\0");

        let mut payload = b"Exif\0\0".to_vec();
        payload.extend(tiff);
        payload.extend_from_slice(b"GPS_MARKER");
        let length = (payload.len() + 2) as u16;
        let mut result = vec![0xff, 0xd8, 0xff, 0xe1];
        result.extend_from_slice(&length.to_be_bytes());
        result.extend(payload);
        result.extend_from_slice(&jpeg[2..]);
        result
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        !crc
    }

    fn huge_header_png(width: u32, height: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr);
        let mut crc_input = b"IHDR".to_vec();
        crc_input.extend_from_slice(&ihdr);
        png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        for kind in [b"IDAT".as_slice(), b"IEND".as_slice()] {
            png.extend_from_slice(&0u32.to_be_bytes());
            png.extend_from_slice(kind);
            png.extend_from_slice(&crc32(kind).to_be_bytes());
        }
        png
    }

    #[test]
    fn huge_dimension_high_compression_image_is_rejected_before_decode() {
        let bytes = huge_header_png(50_000, 50_000);
        assert!(bytes.len() < 64);
        assert!(matches!(process(&bytes), Err(AppError::PayloadTooLarge)));
    }

    #[test]
    fn captures_datetime_and_strips_gps_from_every_variant() {
        let processed = process(&jpeg_with_exif()).unwrap();
        assert_eq!(processed.taken_at.as_deref(), Some("2026-07-04 13:14:15"));
        assert_eq!(processed.variants.len(), 3);
        for variant in processed.variants {
            assert!(!variant.bytes.windows(10).any(|w| w == b"GPS_MARKER"));
            assert!(variant.bytes.len() >= 12 && &variant.bytes[8..12] == b"WEBP");
        }
    }

    async fn file_count(path: PathBuf) -> usize {
        let mut entries = tokio::fs::read_dir(path).await.unwrap();
        let mut count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        count
    }

    #[tokio::test]
    async fn concurrent_upload_gc_invariant_keeps_files_for_every_live_row() {
        let store = Store::connect_in_memory().await;
        let (_, account_id) = store
            .signup_with_password("Host", "host@example.com", "hash")
            .await
            .unwrap();
        let event = store
            .create_event(account_id, "photos", "Photos", "2026-07-04 13:00")
            .await
            .unwrap();
        let root = std::env::temp_dir().join(format!("ronitnath-photo-unit-{}", event.id));
        let _ = tokio::fs::remove_dir_all(&root).await;
        let mut png = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(4, 3)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let bytes = png.into_inner();

        let first = persist(
            &store,
            &root,
            account_id,
            event.id,
            "a.png",
            "",
            UploadAttribution {
                identity_id: Some(1),
                person_id: None,
            },
            process(&bytes).unwrap(),
        )
        .await
        .unwrap();
        let second = persist(
            &store,
            &root,
            account_id,
            event.id,
            "b.png",
            "",
            UploadAttribution {
                identity_id: Some(1),
                person_id: None,
            },
            process(&bytes).unwrap(),
        )
        .await
        .unwrap();
        let photos = store.list_photos_admin(account_id, event.id).await.unwrap();
        assert_eq!(photos.len(), 2);
        assert_eq!(photos[0].storage_key, photos[1].storage_key);
        assert_eq!(file_count(event_dir(&root, account_id, event.id)).await, 3);
        assert_eq!(
            store
                .list_photo_variants(account_id, first)
                .await
                .unwrap()
                .len(),
            3
        );

        store
            .soft_delete_photo(account_id, event.id, first, Some(1), None, false)
            .await
            .unwrap();
        assert_eq!(gc(&store, &root, account_id, 0).await.unwrap(), 1);
        assert_eq!(file_count(event_dir(&root, account_id, event.id)).await, 3);
        store
            .soft_delete_photo(account_id, event.id, second, Some(1), None, false)
            .await
            .unwrap();
        assert_eq!(gc(&store, &root, account_id, 0).await.unwrap(), 1);
        assert_eq!(file_count(event_dir(&root, account_id, event.id)).await, 0);
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
