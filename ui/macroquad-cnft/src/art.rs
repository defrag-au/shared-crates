//! `ArtLoader` — NFT art as textures: fetched a few at a time, retried, cached, evicted.

use std::collections::{HashMap, HashSet};
use std::task::Poll;

use cnft_holdings::OwnedAsset;
use image_core::{Format, ImageCache, ImageSize, LoadQueue, Outcome, Slot};
use macroquad::prelude::{FilterMode, Texture2D};

use crate::fetch::{ImageRequest, get_image};

/// The URL to draw an asset's art from, on the default IIIF host.
///
/// On the web the service picks the format (`auto`): the browser decodes
/// whatever comes back, and the service knows whether the collection is
/// pixel art. Natively macroquad decodes PNG and TGA only, so it asks for
/// PNG — several times the bytes, which only a developer's machine pays.
pub fn art_url(asset: &OwnedAsset, size: ImageSize) -> String {
    asset.art().size(size).format(platform_format()).build()
}

/// As [`art_url`], against another IIIF base: a same-origin proxy for a
/// surface under a strict content security policy, such as a Discord
/// Activity. `base` is everything up to and including `/iiif/3`.
pub fn art_url_on(base: &str, asset: &OwnedAsset, size: ImageSize) -> String {
    asset
        .art()
        .on(base)
        .size(size)
        .format(platform_format())
        .build()
}

fn platform_format() -> Format {
    if cfg!(target_arch = "wasm32") {
        Format::Auto
    } else {
        Format::Png
    }
}

/// What a URL's art is doing, for drawing it.
pub enum ArtView<'a> {
    /// Asked for and not here yet: queued or in flight.
    Loading,
    /// Failed for good. Retrying will not help.
    Failed,
    /// Here. `age` is seconds since it arrived, on the clock
    /// [`ArtLoader::update`] is given, for fading it in.
    Ready { texture: &'a Texture2D, age: f32 },
}

/// Counts for a debug overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArtStats {
    pub queued: usize,
    pub in_flight: usize,
    pub cached: usize,
    pub cached_bytes: usize,
    pub failed: usize,
}

/// Textures by URL. Call [`Self::update`] once per frame, after the frame's
/// requests.
///
/// Built from `image-core`'s queue and cache, so the de-duplication, the
/// priorities, the retries and the eviction are the tested ones rather than
/// another copy.
///
/// # Ask every frame
///
/// [`Self::request`] what is on screen, every frame. A URL still waiting in
/// the queue that nobody asked for this frame is dropped before it is
/// fetched: art the player scrolled past does not hold up art they are
/// looking at. A fetch already in flight is left to finish and is cached.
pub struct ArtLoader {
    queue: LoadQueue,
    cache: ImageCache<Texture2D>,
    in_flight: Vec<(String, ImageRequest)>,
    /// Every URL the queue holds an entry for, so queued ones can be found.
    known: HashSet<String>,
    /// URLs requested since the last update.
    asked: HashSet<String>,
    /// When each cached texture arrived, on the update clock.
    arrived: HashMap<String, f32>,
    now: f32,
}

impl Default for ArtLoader {
    /// Four fetches at a time, three attempts each, and a 64 MB texture
    /// budget: about a hundred 400 px thumbnails.
    fn default() -> Self {
        Self::new(4, 3, 64 * 1024 * 1024)
    }
}

impl ArtLoader {
    pub fn new(concurrency: usize, attempts: u32, budget_bytes: usize) -> Self {
        Self {
            queue: LoadQueue::new(concurrency, attempts),
            cache: ImageCache::new(budget_bytes),
            in_flight: Vec::new(),
            known: HashSet::new(),
            asked: HashSet::new(),
            arrived: HashMap::new(),
            now: 0.0,
        }
    }

    /// Ask for a URL this frame. Lower priority is sooner. Asking every frame
    /// fetches it once.
    ///
    /// A texture the cache evicted is fetched again when asked for, and a
    /// URL that failed for good stays failed until [`Self::forget`].
    pub fn request(&mut self, url: &str, priority: i32) {
        if self.queue.slot(url) == Some(Slot::Done) && !self.cache.contains(url) {
            self.queue.forget(url);
        }
        self.queue.request(url, priority);
        if !self.known.contains(url) {
            self.known.insert(url.to_string());
        }
        if !self.asked.contains(url) {
            self.asked.insert(url.to_string());
        }
    }

    /// The texture, if it has arrived. Counts as a use for eviction.
    pub fn get(&mut self, url: &str) -> Option<&Texture2D> {
        self.cache.get(url)
    }

    /// Where a URL's art is, for drawing. Counts as a use for eviction.
    ///
    /// A URL never asked for reads as [`ArtView::Loading`]; ask for it.
    pub fn view(&mut self, url: &str) -> ArtView<'_> {
        if self.queue.slot(url) == Some(Slot::Failed) {
            return ArtView::Failed;
        }
        let age = self.arrived.get(url).map(|t| (self.now - t).max(0.0));
        match self.cache.get(url) {
            Some(texture) => ArtView::Ready {
                texture,
                age: age.unwrap_or(f32::INFINITY),
            },
            None => ArtView::Loading,
        }
    }

    /// Is the art here? Does not count as a use, so a check cannot keep a
    /// texture alive that is never drawn.
    pub fn ready(&self, url: &str) -> bool {
        self.cache.contains(url)
    }

    /// The clock the last [`Self::update`] was given.
    pub fn now(&self) -> f32 {
        self.now
    }

    /// Did this URL fail for good? A screen can stop showing a spinner.
    pub fn failed(&self, url: &str) -> bool {
        self.queue.slot(url) == Some(Slot::Failed)
    }

    /// Anything queued or in flight.
    pub fn loading(&self) -> bool {
        !self.queue.is_idle()
    }

    /// Nothing loading, and nothing arrived within the last `fade` seconds:
    /// every tile has finished fading in. What a review capture waits for,
    /// so it shows the art rather than the first frame of its fade.
    pub fn settled(&self, fade: f32) -> bool {
        !self.loading() && self.arrived.values().all(|t| self.now - t >= fade)
    }

    pub fn stats(&self) -> ArtStats {
        ArtStats {
            queued: self.queue.count(Slot::Queued),
            in_flight: self.in_flight.len(),
            cached: self.cache.len(),
            cached_bytes: self.cache.used_bytes(),
            failed: self.queue.count(Slot::Failed),
        }
    }

    /// Drop a URL entirely, so the next request starts fresh.
    pub fn forget(&mut self, url: &str) {
        self.queue.forget(url);
        self.cache.remove(url);
        self.arrived.remove(url);
        self.known.remove(url);
        self.in_flight.retain(|(u, _)| u != url);
    }

    /// Drop what was not asked for, start what the queue allows, collect what
    /// finished and upload it. `now` is the game's clock, in seconds; it
    /// dates arrivals for [`ArtView::Ready`]'s `age`.
    ///
    /// True when a texture arrived this frame, for a caller that bakes art
    /// into something and must bake again.
    pub fn update(&mut self, now: f32) -> bool {
        self.now = now;

        let stale: Vec<String> = self
            .known
            .iter()
            .filter(|u| !self.asked.contains(*u) && self.queue.slot(u) == Some(Slot::Queued))
            .cloned()
            .collect();
        for url in stale {
            self.queue.forget(&url);
            self.known.remove(&url);
        }
        self.asked.clear();

        for url in self.queue.next_batch() {
            let req = get_image(&url);
            self.in_flight.push((url, req));
        }

        let mut arrived = false;
        let mut still = Vec::with_capacity(self.in_flight.len());
        for (url, mut req) in self.in_flight.drain(..) {
            match req.poll() {
                Poll::Pending => still.push((url, req)),
                Poll::Ready(Ok(img)) => {
                    let (Ok(w), Ok(h)) = (u16::try_from(img.width), u16::try_from(img.height))
                    else {
                        macroquad::logging::warn!(
                            "art {}: {}x{} is larger than a texture can be",
                            url,
                            img.width,
                            img.height
                        );
                        self.queue.finish(&url, Outcome::Permanent);
                        continue;
                    };
                    let tex = Texture2D::from_rgba8(w, h, &img.rgba);
                    tex.set_filter(FilterMode::Linear);
                    // Evicted textures are dropped here, which frees them.
                    for gone in self.cache.insert(url.clone(), tex, img.byte_size()) {
                        if gone.key != url {
                            self.arrived.remove(&gone.key);
                        }
                    }
                    self.arrived.insert(url.clone(), now);
                    self.queue.finish(&url, Outcome::Ok);
                    arrived = true;
                }
                Poll::Ready(Err(e)) => {
                    // Explicit arguments: miniquad's log macros print a lone
                    // format string literally, so inline `{e}` would log "{e}".
                    macroquad::logging::warn!("art {}: {}", url, e);
                    let outcome = if e.is_permanent() {
                        Outcome::Permanent
                    } else {
                        Outcome::Retryable
                    };
                    self.queue.finish(&url, outcome);
                }
            }
        }
        self.in_flight = still;
        arrived
    }
}

/// NFT art on a gamekit wall: `ArtLoader` is an image source for
/// `macroquad_gamekit::wall::ImageTile`, the same queue, retries and cache.
impl macroquad_gamekit::wall::ImageSource for ArtLoader {
    fn request(&mut self, url: &str, priority: i32) {
        ArtLoader::request(self, url, priority);
    }

    fn view(&mut self, url: &str) -> macroquad_gamekit::wall::ImageView<'_> {
        use macroquad_gamekit::wall::ImageView;
        match ArtLoader::view(self, url) {
            ArtView::Loading => ImageView::Loading,
            ArtView::Failed => ImageView::Failed,
            ArtView::Ready { texture, age } => ImageView::Ready { texture, age },
        }
    }
}
