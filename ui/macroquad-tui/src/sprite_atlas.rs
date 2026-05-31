//! Lazy-loaded sharded sprite atlas — multiple JPEG/PNG tiles that
//! together form one logical key-addressable atlas, fetched on demand.
//!
//! ## Why sharded
//!
//! A single packed atlas trades boot weight for fast access. Once it
//! grows past a few MB the boot wait gets noticeable, and any change
//! to a single tile invalidates the whole file in the browser cache.
//!
//! Sharding cuts each tile's blast radius: a stable hash maps each
//! key to a shard id, so editing one tile only changes the file it
//! lives in. Users only download the shards they touch, and HTTP/2
//! multiplexing handles parallel fetches across shards. Boot stays
//! cheap (just an index file).
//!
//! ## Wire shape
//!
//! Built by the build script — see the schema in [`AtlasIndex`]
//! below. The runtime just reads `[shard_id, col, row]` per key; the
//! shard-assignment hash function only matters at build time.
//!
//! ## Usage shape
//!
//! ```ignore
//! let atlas = ShardedAtlas::from_index_json(
//!     &index_json,
//!     |n| format!("sketches-atlas-{n}.jpg"),
//!     FilterMode::Linear,
//! );
//!
//! // Each frame in the scene that draws atlas tiles:
//! atlas.tick();                                      // poll in-flight loads
//! match atlas.get(key) {
//!     AtlasSlot::Ready { texture, source } => draw_texture_ex(texture, .., source: Some(source), ..),
//!     AtlasSlot::Pending                   => draw_loading_placeholder(),
//!     AtlasSlot::Missing                   => { /* unknown key */ },
//! }
//! ```
//!
//! `get` kicks off the shard's async load on the first call that hits
//! an unloaded shard; subsequent frames return `Pending` until the
//! load resolves, then `Ready` thereafter. Loaded shard textures stay
//! cached for the program's lifetime.

use std::collections::HashMap;

use macroquad::experimental::coroutines::{start_coroutine, Coroutine};
use macroquad::prelude::Rect;
use macroquad::texture::{load_image, FilterMode, Image, Texture2D};
use serde::Deserialize;

/// Wire shape of the atlas index JSON.
///
/// ```text
/// {
///   "cell_width": 256,
///   "cell_height": 256,
///   "shard_count": 8,
///   "entries": {
///     "doc-a/inc-1": [0, 3, 2],   // [shard_id, col_in_shard, row_in_shard]
///     "doc-a/inc-2": [3, 0, 0],
///     ...
///   }
/// }
/// ```
///
/// `shard_count` lets callers iterate shards (e.g. to prefetch all of
/// them up-front if they want) without scanning entries.
#[derive(Deserialize)]
struct AtlasIndex {
    cell_width: u32,
    cell_height: u32,
    shard_count: u32,
    entries: HashMap<String, [u32; 3]>,
}

/// State of a key's tile lookup. The borrow on `Ready` lets the
/// renderer reach the texture without cloning — see module example.
#[derive(Debug)]
pub enum AtlasSlot<'a> {
    /// Key isn't in the index — caller can fall back to a default
    /// sprite or skip the draw.
    Missing,
    /// Slot exists but its shard hasn't finished loading yet. The
    /// load is in flight; another `tick` + `get` next frame may
    /// upgrade this to `Ready`.
    Pending,
    /// Slot is ready to draw.
    Ready {
        texture: &'a Texture2D,
        /// Sub-rectangle of `texture` (in atlas pixel coords) that
        /// holds this key's tile. Pass through `draw_texture_ex`'s
        /// `source` parameter.
        source: Rect,
    },
}

type ShardUrlFn = Box<dyn Fn(u32) -> String + Send + Sync>;

pub struct ShardedAtlas {
    cell_width: u32,
    cell_height: u32,
    shard_count: u32,
    /// Key → (shard_id, col, row).
    index: HashMap<String, (u32, u32, u32)>,
    /// Loaded shard textures keyed by `shard_id`.
    textures: HashMap<u32, Texture2D>,
    /// Shard loads currently in flight keyed by `shard_id`. Polled
    /// each frame from `tick`.
    pending: HashMap<u32, Coroutine<Result<Image, macroquad::Error>>>,
    shard_url: ShardUrlFn,
    /// Filter applied to each newly-installed shard texture.
    filter: FilterMode,
}

impl ShardedAtlas {
    /// Parse the index JSON and build an atlas in its initial empty
    /// state — no shards loaded yet, all `get`s will return `Pending`
    /// until the first frame after a shard arrives.
    ///
    /// `shard_url` formats a shard id into the URL/path that
    /// `macroquad::texture::load_image` will fetch. Same path rules
    /// as `load_image`: relative to the page on WASM, relative to
    /// `pc_assets_folder` on native.
    pub fn from_index_json<F>(index_json: &str, shard_url: F, filter: FilterMode) -> Self
    where
        F: Fn(u32) -> String + Send + Sync + 'static,
    {
        let parsed: AtlasIndex = serde_json::from_str(index_json)
            .expect("sharded atlas index failed to deserialize — schema drift?");
        let index = parsed
            .entries
            .into_iter()
            .map(|(k, v)| (k, (v[0], v[1], v[2])))
            .collect();
        Self {
            cell_width: parsed.cell_width,
            cell_height: parsed.cell_height,
            shard_count: parsed.shard_count,
            index,
            textures: HashMap::new(),
            pending: HashMap::new(),
            shard_url: Box::new(shard_url),
            filter,
        }
    }

    pub fn shard_count(&self) -> u32 {
        self.shard_count
    }

    /// True when the key exists in the index, regardless of whether
    /// the shard has been loaded yet.
    pub fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Look up a key. If the key's shard isn't loaded and no load is
    /// in flight, kicks off the load. The returned state reflects the
    /// situation *as of this call*; `tick` + a subsequent `get` will
    /// upgrade `Pending` to `Ready` once the coroutine completes.
    pub fn get(&mut self, key: &str) -> AtlasSlot<'_> {
        let Some(&(shard, col, row)) = self.index.get(key) else {
            return AtlasSlot::Missing;
        };
        if !self.textures.contains_key(&shard) {
            self.kick_off_load(shard);
            return AtlasSlot::Pending;
        }
        let texture = self.textures.get(&shard).unwrap();
        let source = Rect::new(
            (col * self.cell_width) as f32,
            (row * self.cell_height) as f32,
            self.cell_width as f32,
            self.cell_height as f32,
        );
        AtlasSlot::Ready { texture, source }
    }

    /// Explicitly request a shard to be loaded ahead of time —
    /// e.g. a "browse" scene that knows the user is about to view a
    /// run of tiles from the same shard, or a prefetch sweep at idle.
    /// No-op if already loaded or in flight.
    pub fn prefetch(&mut self, shard_id: u32) {
        if shard_id >= self.shard_count {
            return;
        }
        if self.textures.contains_key(&shard_id) || self.pending.contains_key(&shard_id) {
            return;
        }
        self.kick_off_load(shard_id);
    }

    /// Poll all in-flight shard loads. Completed ones get installed
    /// into the texture cache and removed from `pending`. Call once
    /// per frame from the scene that owns the atlas.
    pub fn tick(&mut self) {
        // Two-pass — `coroutine.retrieve()` takes `&self` but we need
        // `&mut self` to install the new texture. Collect the
        // finished work first, then mutate.
        let mut to_install: Vec<(u32, Image)> = Vec::new();
        let mut to_remove: Vec<u32> = Vec::new();
        for (&shard_id, coroutine) in &self.pending {
            if let Some(result) = coroutine.retrieve() {
                to_remove.push(shard_id);
                if let Ok(img) = result {
                    to_install.push((shard_id, img));
                }
                // Err: silently drop the in-flight entry. The next
                // `get` for this shard will retry. Tests for transient
                // network failure can re-issue; permanent failure
                // (bad URL) will just keep returning `Pending`. We can
                // expose an error API once anything cares.
            }
        }
        for s in to_remove {
            self.pending.remove(&s);
        }
        for (id, img) in to_install {
            let tex = Texture2D::from_image(&img);
            tex.set_filter(self.filter);
            self.textures.insert(id, tex);
        }
    }

    fn kick_off_load(&mut self, shard_id: u32) {
        if self.pending.contains_key(&shard_id) {
            return;
        }
        let url = (self.shard_url)(shard_id);
        let coroutine = start_coroutine(load_image_owned(url));
        self.pending.insert(shard_id, coroutine);
    }
}

/// `load_image` takes `&str`; coroutines need an owned future, so wrap
/// it in an async block that owns the path string.
async fn load_image_owned(url: String) -> Result<Image, macroquad::Error> {
    load_image(&url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_parses_with_three_coord_entries() {
        let json = r#"{
            "cell_width": 256,
            "cell_height": 256,
            "shard_count": 4,
            "entries": {
                "a/1": [0, 3, 2],
                "a/2": [1, 0, 0],
                "b/1": [3, 7, 5]
            }
        }"#;
        let atlas = ShardedAtlas::from_index_json(json, |n| format!("shard-{n}.jpg"), FilterMode::Linear);
        assert_eq!(atlas.shard_count(), 4);
        assert!(atlas.contains("a/1"));
        assert!(atlas.contains("b/1"));
        assert!(!atlas.contains("c/1"));
    }

    #[test]
    fn missing_key_returns_missing() {
        let json = r#"{
            "cell_width": 100, "cell_height": 100, "shard_count": 1,
            "entries": { "a/1": [0, 0, 0] }
        }"#;
        let mut atlas = ShardedAtlas::from_index_json(json, |_| "x.jpg".into(), FilterMode::Linear);
        assert!(matches!(atlas.get("nope"), AtlasSlot::Missing));
    }

    // Note: we don't test the Pending → Ready transition here because
    // `start_coroutine` needs a running macroquad event loop. That
    // path is exercised by the integration runs.
}
