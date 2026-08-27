//! The regional catalog, held in memory a tile at a time.
//!
//! Three levels of star, and the whole design is about never fetching more of
//! them than the current view can show: the bright catalog is already in the
//! page, the mid catalog is one 2.8 MB file worth having once the view is
//! narrow, and the deep catalog is fetched as byte ranges out of a 49 MB file
//! that is never downloaded. Tiles are cached with a byte budget and evicted
//! oldest-first, so a long session panning across the sky settles at a fixed
//! footprint instead of accumulating one.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen_futures::spawn_local;

use crate::dom;
use crate::explorer::camera::Camera;
use crate::explorer::lod::{self, Manifest, Star};
use crate::telemetry;

const MANIFEST_URL: &str = "/static/stars/lod/manifest.json";
const MID_URL: &str = "/static/stars/lod/g9.bin";
const DEEP_URL: &str = "/static/stars/lod/g12.bin";

/// How many tile requests may be in flight. Enough to fill a view in one pass,
/// few enough that a fast zoom does not queue a hundred stale ones.
const MAX_INFLIGHT: usize = 6;
/// Cache ceilings. Whichever is hit first evicts the oldest resident tile.
const MAX_TILES: usize = 128;
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// How long a failed tile is left alone before it is asked for again.
const RETRY_AFTER_MS: f64 = 30_000.0;

struct Resident {
    id: usize,
    stars: Rc<Vec<Star>>,
    bytes: usize,
    loaded_at: f64,
}

#[derive(Default)]
pub struct Tiles {
    manifest: RefCell<Option<Rc<Manifest>>>,
    mid: RefCell<Option<Rc<Vec<Star>>>>,
    mid_loaded_at: Cell<f64>,
    mid_loading: Cell<bool>,
    /// Oldest first: eviction takes from the front, a fresh load pushes on the
    /// back, and a tile that is drawn is moved to the back.
    cache: RefCell<Vec<Resident>>,
    inflight: RefCell<Vec<usize>>,
    failed: RefCell<Vec<(usize, f64)>>,
}

impl Tiles {
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// Fetch the tile index. Until it lands the atlas simply has no deep
    /// catalog, which is a thinner sky and not a broken one.
    pub async fn load_manifest(self: &Rc<Self>) {
        match dom::fetch_text(MANIFEST_URL).await {
            Ok(text) => match Manifest::parse(&text) {
                Ok(manifest) => {
                    telemetry::number("explorer", "tileCount", manifest.tiles.len() as f64);
                    self.manifest.replace(Some(Rc::new(manifest)));
                }
                Err(error) => dom::warn("regional catalog index unusable:", &error.into()),
            },
            Err(error) => dom::warn("regional catalog index unavailable:", &error),
        }
    }

    /// The tiles this view wants, in the order it wants them.
    #[must_use]
    pub fn wanted(&self, camera: &Camera, aspect: f64) -> Vec<usize> {
        self.manifest
            .borrow()
            .as_ref()
            .map(|manifest| manifest.tiles_for(camera.ra, camera.dec, camera.fov, aspect))
            .unwrap_or_default()
    }

    /// Ask for whatever this view needs and does not have.
    pub fn request(self: &Rc<Self>, camera: &Camera, aspect: f64) {
        if camera.fov <= lod::MID_FOV_DEG {
            self.request_mid();
        }
        let Some(manifest) = self.manifest.borrow().clone() else {
            return;
        };
        let now = dom::now_ms();
        for id in self.wanted(camera, aspect) {
            if self.inflight.borrow().len() >= MAX_INFLIGHT {
                return;
            }
            let known = self.cache.borrow().iter().any(|tile| tile.id == id)
                || self.inflight.borrow().contains(&id)
                || self
                    .failed
                    .borrow()
                    .iter()
                    .any(|(failed, at)| *failed == id && now - at < RETRY_AFTER_MS);
            if known {
                continue;
            }
            let Some(tile) = manifest.tiles.get(id).copied() else {
                continue;
            };
            if tile.bytes == 0 {
                continue;
            }
            self.inflight.borrow_mut().push(id);
            let tiles = Rc::clone(self);
            spawn_local(async move {
                let end = tile.offset + tile.bytes - 1;
                let fetched = dom::fetch_range(DEEP_URL, tile.offset, end).await;
                tiles.inflight.borrow_mut().retain(|queued| *queued != id);
                match fetched {
                    Ok(bytes) => tiles.admit(id, &bytes),
                    Err(error) => {
                        dom::warn("sky tile unavailable:", &error);
                        tiles.failed.borrow_mut().push((id, dom::now_ms()));
                    }
                }
            });
        }
    }

    /// Decode a fetched tile into the cache and evict down to the budget.
    fn admit(&self, id: usize, bytes: &[u8]) {
        let stars = lod::decode(bytes);
        let mut cache = self.cache.borrow_mut();
        cache.retain(|tile| tile.id != id);
        cache.push(Resident {
            id,
            stars: Rc::new(stars),
            bytes: bytes.len(),
            loaded_at: dom::now_ms(),
        });
        let mut total: usize = cache.iter().map(|tile| tile.bytes).sum();
        while cache.len() > MAX_TILES || total > MAX_BYTES {
            let evicted = cache.remove(0);
            total -= evicted.bytes;
        }
        telemetry::number("explorer", "residentTiles", cache.len() as f64);
        telemetry::number("explorer", "residentBytes", total as f64);
    }

    /// A resident tile's stars and the instant they arrived, marking it as
    /// recently used so eviction takes something else first.
    #[must_use]
    pub fn resident(&self, id: usize) -> Option<(Rc<Vec<Star>>, f64)> {
        let mut cache = self.cache.borrow_mut();
        let at = cache.iter().position(|tile| tile.id == id)?;
        let tile = cache.remove(at);
        let found = (Rc::clone(&tile.stars), tile.loaded_at);
        cache.push(tile);
        Some(found)
    }

    #[must_use]
    pub fn mid(&self) -> Option<(Rc<Vec<Star>>, f64)> {
        self.mid
            .borrow()
            .as_ref()
            .map(|stars| (Rc::clone(stars), self.mid_loaded_at.get()))
    }

    fn request_mid(self: &Rc<Self>) {
        if self.mid.borrow().is_some() || self.mid_loading.replace(true) {
            return;
        }
        let tiles = Rc::clone(self);
        spawn_local(async move {
            match dom::fetch_bytes(MID_URL).await {
                Ok(bytes) => match lod::decode_file(&bytes) {
                    Ok(stars) => {
                        telemetry::number("explorer", "midStars", stars.len() as f64);
                        tiles.mid_loaded_at.set(dom::now_ms());
                        tiles.mid.replace(Some(Rc::new(stars)));
                    }
                    Err(error) => dom::warn("mid catalog unusable:", &error.into()),
                },
                Err(error) => dom::warn("mid catalog unavailable:", &error),
            }
            tiles.mid_loading.set(false);
        });
    }

    /// Drop everything. The atlas is closed; nothing on the page can draw a
    /// star from here again, and 64 MB is not a footprint to leave behind.
    pub fn clear(&self) {
        self.cache.borrow_mut().clear();
        self.inflight.borrow_mut().clear();
        self.failed.borrow_mut().clear();
        self.mid.replace(None);
        self.manifest.replace(None);
        telemetry::number("explorer", "residentTiles", 0.0);
        telemetry::number("explorer", "residentBytes", 0.0);
    }
}
