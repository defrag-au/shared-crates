//! Global image cache for preloaded images
//!
//! This module provides a shared cache that stores preloaded images as blob URLs.
//! Components can check this cache before loading images from the network.

use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    /// Global cache mapping image URLs to blob URLs
    static IMAGE_CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Check if an image URL is in the cache
pub fn get_cached_url(url: &str) -> Option<String> {
    IMAGE_CACHE.with(|cache| cache.borrow().get(url).cloned())
}

/// Store a blob URL in the cache for a given image URL
pub fn cache_blob_url(original_url: String, blob_url: String) {
    IMAGE_CACHE.with(|cache| {
        cache.borrow_mut().insert(original_url, blob_url);
    });
}

/// Get the number of cached images
pub fn cache_size() -> usize {
    IMAGE_CACHE.with(|cache| cache.borrow().len())
}

/// Preload an image and store it in the cache as a blob URL
/// Returns a Future that resolves when the image is cached
pub async fn preload_image(url: String) -> Result<String, JsValue> {
    // Check if already cached
    if let Some(blob_url) = get_cached_url(&url) {
        return Ok(blob_url);
    }

    // Fetch the image as a blob
    let window = web_sys::window().ok_or("no window")?;
    let resp_promise = window.fetch_with_str(&url);
    let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(resp_promise)
        .await?
        .dyn_into()?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "Failed to fetch image: {}",
            resp.status()
        )));
    }

    let blob_promise = resp.blob()?;
    let blob: web_sys::Blob = wasm_bindgen_futures::JsFuture::from(blob_promise)
        .await?
        .dyn_into()?;

    // Create a blob URL
    let blob_url = web_sys::Url::create_object_url_with_blob(&blob)?;

    // Store in cache
    cache_blob_url(url, blob_url.clone());

    Ok(blob_url)
}
