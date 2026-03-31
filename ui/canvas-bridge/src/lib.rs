//! FBO-based bridge for compositing external canvas rendering into egui.
//!
//! Provides [`CanvasBridge`] which manages an offscreen framebuffer (FBO) that
//! captures rendering from an external renderer (e.g., femtovg) and blits it
//! into an egui [`PaintCallback`] region.
//!
//! # Architecture
//!
//! External renderers like femtovg insist on rendering to the default framebuffer.
//! This crate captures that output via `blitFramebuffer`, stores it in an FBO texture,
//! then draws that texture into the correct egui viewport position using a passthrough
//! shader and fullscreen quad.
//!
//! Uses raw `web_sys::WebGl2RenderingContext` (not glow) to avoid slotmap isolation
//! issues when multiple glow versions coexist in the same binary.
//!
//! # Usage
//!
//! ```rust,ignore
//! use canvas_bridge::{CanvasBridge, WasmSync};
//! use std::sync::Arc;
//! use std::cell::RefCell;
//!
//! // In App::new(), create the bridge from the eframe canvas
//! let canvas_el = get_canvas_element();
//! let bridge = CanvasBridge::new(&canvas_el, 800, 600).unwrap();
//!
//! // Wrap for PaintCallback's Send+Sync requirement
//! let shared = Arc::new(WasmSync::new(RefCell::new(bridge)));
//!
//! // In the PaintCallback:
//! let shared = shared.clone();
//! let callback = egui::PaintCallback {
//!     rect,
//!     callback: Arc::new(egui_glow::CallbackFn::new(move |info, _painter| {
//!         let mut bridge = shared.borrow_mut();
//!         // 1. Your renderer draws to the default framebuffer
//!         my_renderer.render(bridge.fbo_width(), bridge.fbo_height());
//!         // 2. Capture and blit to egui's viewport
//!         bridge.capture_and_blit(&info);
//!     })),
//! };
//! ```

mod blit;
mod shaders;

pub use blit::CanvasBridge;

use std::cell::{Ref, RefCell, RefMut};

/// WASM Send+Sync wrapper for use with egui's `CallbackFn`.
///
/// egui's `CallbackFn` requires `Send + Sync` bounds even in WASM
/// where there is only one thread. This wrapper provides the unsafe
/// impls needed — safe because WASM is single-threaded.
pub struct WasmSync<T>(T);

unsafe impl<T> Send for WasmSync<T> {}
unsafe impl<T> Sync for WasmSync<T> {}

impl<T> WasmSync<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> WasmSync<RefCell<T>> {
    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.borrow_mut()
    }
}
