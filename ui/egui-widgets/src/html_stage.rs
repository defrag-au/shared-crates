//! `HtmlStage` — mount a live HTML document over an `egui::Rect`.
//!
//! Fully on-chain generative art is not a still: the piece *is* an HTML
//! document, and the browser is meant to **run** it. Nothing in the image
//! pipeline can carry one — IIIF and the IPFS mirror resolve one still per
//! asset, and the format detectors have no HTML arm — so a front end that
//! wants to show the piece has to hand it to the browser directly.
//! `cardano_assets::AssetEnvelope::live_art` is what tells you a piece is one
//! of these.
//!
//! egui draws into a canvas and cannot execute a document, so the piece is
//! rendered by an `<iframe>` that this widget keeps positioned over the
//! rectangle the UI reserved for it. egui draws the frame; the iframe draws
//! the art.
//!
//! ## Two placements, one primitive
//!
//! [`StageLayer`] decides the stacking order, and that is the only difference
//! between the two ways to show a piece:
//!
//! - [`StageLayer::AboveCanvas`] (default) — the iframe sits over the canvas.
//!   Nothing about the host app has to change, and egui keeps every pixel it
//!   would otherwise have drawn *outside* the stage. Nothing egui paints can
//!   appear over the art.
//! - [`StageLayer::BelowCanvas`] — the "cutout": the iframe sits *under* the
//!   canvas, so egui chrome composites over the art. This is the sentience
//!   hub-guardian technique, and it is the nicer look — but it is a
//!   host-wide commitment, not a widget setting: the app must render with a
//!   transparent clear colour, its canvas must be transparent, positioned and
//!   `z-index: 1`, and **every panel frame that covers the rect must stop
//!   filling** (egui has no way to punch a hole in a painted rect, so the
//!   background has to be drawn around it).
//!
//! ## Traps this widget exists to keep you out of
//!
//! - **A hidden iframe is still running.** These pieces drive themselves from
//!   `requestAnimationFrame`, so leaving one mounted-but-invisible burns a
//!   core. Teardown means *removing the element*, which is why [`HtmlStage`]
//!   tears down on `Drop` — drop the stage and the piece stops. Hiding it, or
//!   letting a `src` swap stand in for teardown, does not.
//! - **One at a time.** A grid of stages is a grid of animation loops. Keep
//!   the still (the piece's `cover`) in the grid and mount a stage only in
//!   the viewer.
//! - **`sandbox` must not carry `allow-same-origin`.** On a same-origin
//!   document, `allow-scripts allow-same-origin` lets the piece reach up and
//!   remove its own sandbox attribute. `allow-scripts` alone gives an opaque
//!   origin, which is the isolation the whole arrangement rests on — so it is
//!   hardcoded here rather than exposed as an option.
//! - **Pointer events are off by default.** An iframe that takes input
//!   swallows the scroll wheel and egui's hover for its whole rect, which
//!   turns a stage inside a scrollable page into a dead zone. Display-only is
//!   the default; [`StageOptions::interactive`] opts in and means it.

use egui::Rect;
use wasm_bindgen::JsValue;

/// Where the stage sits in the page's stacking order. See the module docs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StageLayer {
    /// Over the canvas. Self-contained — no host changes required.
    #[default]
    AboveCanvas,
    /// Under the canvas — the cutout. Requires a transparent, `z-index: 1`
    /// canvas and non-filling panel frames; see the module docs.
    BelowCanvas,
}

/// How to mount a stage. [`StageOptions::default`] is the display-only
/// overlay, which is what most callers want.
#[derive(Clone, Debug)]
pub struct StageOptions {
    layer: StageLayer,
    interactive: bool,
    radius: f32,
    title: Option<String>,
}

impl Default for StageOptions {
    fn default() -> Self {
        Self {
            layer: StageLayer::default(),
            interactive: false,
            radius: 0.0,
            title: None,
        }
    }
}

impl StageOptions {
    /// Stack over or under the canvas. See [`StageLayer`].
    #[must_use]
    pub fn layer(mut self, layer: StageLayer) -> Self {
        self.layer = layer;
        self
    }

    /// Let the piece receive pointer input. Off by default, and worth
    /// leaving off outside a modal that owns the screen — see the module
    /// docs.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Round the stage's corners to match a frame drawn around it, in CSS px.
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// The iframe's accessible name. Worth setting to the piece's name: an
    /// unlabelled iframe is announced as "frame" and nothing else.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// A mounted live document, positioned by [`present`](Self::present).
///
/// Dropping the stage removes the iframe from the DOM and stops the piece.
pub struct HtmlStage {
    iframe: web_sys::HtmlIFrameElement,
}

impl HtmlStage {
    /// Mount `src` in an iframe appended to `<body>`.
    ///
    /// `src` is whatever the metadata's `files[].src` resolved to: a
    /// `data:text/html;utf8,…` URI, handed to the browser as-is so it does its
    /// own decoding, or a URL to a document.
    ///
    /// Appends to `<body>` because placement does not depend on the parent —
    /// the stage is `position: fixed`, so it is positioned against the
    /// viewport regardless — and `<body>` is the one element guaranteed to be
    /// there.
    pub fn mount(src: &str, options: StageOptions) -> Result<Self, JsValue> {
        use wasm_bindgen::JsCast as _;

        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| JsValue::from_str("no document to mount a stage in"))?;
        let body = document
            .body()
            .ok_or_else(|| JsValue::from_str("no document body to mount a stage in"))?;

        let iframe = document
            .create_element("iframe")?
            .dyn_into::<web_sys::HtmlIFrameElement>()?;

        iframe.set_attribute("src", src)?;
        // `allow-scripts` and NOTHING else. See the module docs: adding
        // `allow-same-origin` to a same-origin document would let the piece
        // remove its own sandbox.
        iframe.set_attribute("sandbox", "allow-scripts")?;
        iframe.set_attribute("scrolling", "no")?;
        if let Some(title) = &options.title {
            let _ = iframe.set_attribute("title", title);
        }

        let style = iframe.style();
        let _ = style.set_property("position", "fixed");
        let _ = style.set_property("border", "0");
        let _ = style.set_property("background", "transparent");
        let _ = style.set_property(
            "z-index",
            match options.layer {
                StageLayer::AboveCanvas => "2",
                StageLayer::BelowCanvas => "0",
            },
        );
        let _ = style.set_property(
            "pointer-events",
            if options.interactive { "auto" } else { "none" },
        );
        if options.radius > 0.0 {
            let radius = options.radius;
            let _ = style.set_property("border-radius", &format!("{radius}px"));
            let _ = style.set_property("overflow", "hidden");
        }

        body.append_child(&iframe)?;
        Ok(Self { iframe })
    }

    /// Move the stage to `rect` — call it every frame the stage should be
    /// visible.
    ///
    /// `rect` is an egui rect, and egui points *are* CSS px, so no scaling is
    /// applied: the canvas's backing store may be 2× on a retina display, but
    /// its CSS size is its size in points. The rect is read as
    /// viewport-relative, which is correct while the canvas starts at the
    /// viewport's origin — true of every front end here (a full-window canvas
    /// with `margin: 0`). A host that nests its canvas passes
    /// `rect.translate(canvas_origin)`.
    pub fn present(&self, rect: Rect) {
        let style = self.iframe.style();
        let (x, y) = (rect.min.x, rect.min.y);
        let (w, h) = (rect.width(), rect.height());
        let _ = style.set_property("left", &format!("{x}px"));
        let _ = style.set_property("top", &format!("{y}px"));
        let _ = style.set_property("width", &format!("{w}px"));
        let _ = style.set_property("height", &format!("{h}px"));
    }

    /// Remove the stage from the DOM, stopping the piece.
    ///
    /// Also runs on `Drop`; call it directly when the teardown must be
    /// immediate rather than whenever the owning state happens to be dropped.
    pub fn unmount(&self) {
        if let Some(parent) = self.iframe.parent_node() {
            let _ = parent.remove_child(&self.iframe);
        }
    }

    /// Post a message into the piece.
    ///
    /// The escape hatch for pieces that take directions — the sentience
    /// pieces drive their renderer this way. Generic on-chain art does not
    /// listen, so this is a no-op for most pieces; the host is responsible
    /// for knowing which it has.
    pub fn post_message(&self, value: &JsValue) {
        if let Some(window) = self.iframe.content_window() {
            let _ = window.post_message(value, "*");
        }
    }

    /// The iframe itself, for styling beyond what [`StageOptions`] covers.
    #[must_use]
    pub fn element(&self) -> &web_sys::HtmlIFrameElement {
        &self.iframe
    }
}

impl Drop for HtmlStage {
    /// Teardown is the whole safety story — see the module docs. Hiding a
    /// stage leaves its animation loop running, so the element goes.
    fn drop(&mut self) {
        self.unmount();
    }
}
