//! Byte-range serving for the one asset tree too big to embed.
//!
//! `static/stars/lod/**` is 49 MB of regional Gaia catalog. The deep-zoom
//! explorer never wants the file — it wants a few kilobytes per sky tile, at
//! offsets the shipped manifest names — so the only useful way to serve it is
//! `Range: bytes=…`. Everything here is that one HTTP behaviour: parse the
//! header, read the slice, answer 206; answer 416 when the client asks for
//! bytes the file does not have, so a stale manifest fails loudly instead of
//! quietly painting the wrong stars.
//!
//! A `Range`-less request is still answered in full (200 with
//! `Accept-Ranges: bytes`), because a client that cannot do ranges should get
//! the file rather than an error — but nothing on the site takes that path.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The path prefix, relative to the `static/` root, served this way.
pub const PREFIX: &str = "stars/lod/";

/// What a request asked for, already resolved against the file's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// No usable `Range` header: serve the whole file.
    Whole,
    /// An inclusive byte span that the file actually contains.
    Slice { start: u64, end: u64 },
    /// A byte range the file cannot satisfy — 416.
    Unsatisfiable,
}

/// Resolve a `Range` header against a known file length.
///
/// RFC 9110 §14.2: a range unit we do not understand, and a syntactically
/// invalid range, are *ignored* rather than rejected — hence [`Asked::Whole`]
/// for junk. Only a well-formed byte range that lies outside the file is a
/// 416, which is the case a wrong manifest produces.
#[must_use]
pub fn parse(value: Option<&str>, len: u64) -> Asked {
    let Some(spec) = value.and_then(|value| value.trim().strip_prefix("bytes=")) else {
        return Asked::Whole;
    };
    // One range only: a multipart/byteranges body is a whole response format
    // this site has no reason to grow.
    if spec.contains(',') {
        return Asked::Whole;
    }
    let Some((first, last)) = spec.trim().split_once('-') else {
        return Asked::Whole;
    };
    let (first, last) = (first.trim(), last.trim());

    // `bytes=-n`: the last n bytes.
    if first.is_empty() {
        let Ok(suffix) = last.parse::<u64>() else {
            return Asked::Whole;
        };
        if suffix == 0 || len == 0 {
            return Asked::Unsatisfiable;
        }
        return Asked::Slice {
            start: len.saturating_sub(suffix),
            end: len - 1,
        };
    }

    let Ok(start) = first.parse::<u64>() else {
        return Asked::Whole;
    };
    let end = if last.is_empty() {
        match len.checked_sub(1) {
            Some(end) => end,
            None => return Asked::Unsatisfiable,
        }
    } else {
        match last.parse::<u64>() {
            // A last-byte-pos past the end is clamped, not refused: that is how
            // a client asks for "this much, or whatever is left".
            Ok(last) => last.min(len.saturating_sub(1)),
            Err(_) => return Asked::Whole,
        }
    };
    if start >= len {
        return Asked::Unsatisfiable;
    }
    if end < start {
        // An invalid spec (last < first) is ignored, per the same clause.
        return Asked::Whole;
    }
    Asked::Slice { start, end }
}

/// Serve one on-disk file, honouring `Range`. Absent file → 404.
pub async fn respond(path: PathBuf, content_type: &'static str, headers: &HeaderMap) -> Response {
    let asked = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let read = tokio::task::spawn_blocking(move || read_slice(&path, asked.as_deref())).await;
    let Ok(Ok((len, asked, bytes))) = read else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match asked {
        Asked::Unsatisfiable => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::RANGE_NOT_SATISFIABLE.into_response()),
        Asked::Whole => (
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::ACCEPT_RANGES, "bytes".to_string()),
            ],
            Body::from(bytes),
        )
            .into_response(),
        Asked::Slice { start, end } => (
            StatusCode::PARTIAL_CONTENT,
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}")),
            ],
            Body::from(bytes),
        )
            .into_response(),
    }
}

/// Open the file, resolve the range against its real length, read only what
/// was asked for. Blocking, and called from a blocking task for that reason.
fn read_slice(path: &PathBuf, asked: Option<&str>) -> std::io::Result<(u64, Asked, Vec<u8>)> {
    use std::io::{Read, Seek};

    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    match parse(asked, len) {
        Asked::Unsatisfiable => Ok((len, Asked::Unsatisfiable, Vec::new())),
        // Nothing on the site takes this path — the explorer always sends a
        // range — but a whole-file read of the 49 MB catalog is still bounded
        // and rare enough not to warrant a streaming body.
        Asked::Whole => {
            let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
            file.read_to_end(&mut bytes)?;
            Ok((len, Asked::Whole, bytes))
        }
        Asked::Slice { start, end } => {
            file.seek(std::io::SeekFrom::Start(start))?;
            let size = usize::try_from(end - start + 1).unwrap_or(usize::MAX);
            let mut bytes = vec![0; size];
            file.read_exact(&mut bytes)?;
            Ok((len, Asked::Slice { start, end }, bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_span_resolves_to_the_bytes_the_manifest_named() {
        assert_eq!(
            parse(Some("bytes=12-4459"), 49_405_260),
            Asked::Slice {
                start: 12,
                end: 4_459
            }
        );
        assert_eq!(
            parse(Some("bytes=0-0"), 10),
            Asked::Slice { start: 0, end: 0 }
        );
    }

    #[test]
    fn an_open_ended_or_suffix_span_is_resolved_against_the_real_length() {
        assert_eq!(
            parse(Some("bytes=8-"), 10),
            Asked::Slice { start: 8, end: 9 }
        );
        assert_eq!(
            parse(Some("bytes=-3"), 10),
            Asked::Slice { start: 7, end: 9 }
        );
        // Asking for more tail than the file has is the whole file, not an error.
        assert_eq!(
            parse(Some("bytes=-50"), 10),
            Asked::Slice { start: 0, end: 9 }
        );
        // A last-byte-pos past the end clamps: "this much, or what is left".
        assert_eq!(
            parse(Some("bytes=5-99"), 10),
            Asked::Slice { start: 5, end: 9 }
        );
    }

    #[test]
    fn a_range_the_file_cannot_satisfy_is_the_only_thing_that_is_refused() {
        for outside in ["bytes=10-12", "bytes=99-", "bytes=-0"] {
            assert_eq!(parse(Some(outside), 10), Asked::Unsatisfiable, "{outside}");
        }
        assert_eq!(parse(Some("bytes=0-0"), 0), Asked::Unsatisfiable);
    }

    #[test]
    fn anything_we_do_not_understand_is_ignored_rather_than_refused() {
        for ignored in [
            "items=0-1",
            "bytes=0-1,4-5",
            "bytes=abc-def",
            "bytes=9-2",
            "bytes",
            "",
        ] {
            assert_eq!(parse(Some(ignored), 10), Asked::Whole, "{ignored}");
        }
        assert_eq!(parse(None, 10), Asked::Whole);
    }

    #[test]
    fn the_shipped_catalog_is_readable_by_the_offsets_its_manifest_publishes() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../static/stars/lod/g12.bin");
        let (len, asked, bytes) = read_slice(&path, Some("bytes=12-4459")).expect("the catalog");
        assert_eq!(
            asked,
            Asked::Slice {
                start: 12,
                end: 4_459
            }
        );
        assert_eq!(bytes.len(), 4_448);
        // The first tile begins at the 12-byte header, so the header is not in it.
        assert_ne!(&bytes[..8], b"GDR3LOD1");

        let (_, asked, empty) = read_slice(&path, Some(&format!("bytes={len}-"))).expect("a 416");
        assert_eq!(asked, Asked::Unsatisfiable);
        assert!(empty.is_empty());
    }
}
