//! One call to set a frontend up before its first frame.
//!
//! Every egui app in the estate opened with the same three-or-four-line
//! incantation, copied from whichever neighbour was handy:
//!
//! ```ignore
//! egui_extras::install_image_loaders(&cc.egui_ctx);
//! cc.egui_ctx.add_image_loader(std::sync::Arc::new(
//!     egui_widgets::image_loader::browser::BrowserImageLoader::default(),
//! ));
//! egui_widgets::install_phosphor_font(&cc.egui_ctx);
//! ```
//!
//! Sixteen copies, and it only worked because people copied it. A new app that
//! reached for the obvious `egui_extras::install_image_loaders` alone got an
//! app whose images decode on the main thread, and whose icons render as tofu —
//! neither of which announces itself.
//!
//! [`install_defaults`] is that block, once.

/// Install the image loaders and the icon font.
///
/// Call **before** [`crate::theme::configure_style`]: the icon family has to be
/// registered before the style names it. Idempotent, so calling it twice is
/// harmless.
///
/// # What it installs
///
/// - `egui_extras`' loaders, which provide the **bytes** half — notably the
///   http loader. Note that one only accepts absolute `http://` / `https://`
///   URIs; a relative path is refused by every bytes loader and the image then
///   silently never arrives, whatever the decoder is doing.
/// - [`crate::image_loader::browser::BrowserImageLoader`] on wasm, which
///   decodes with `createImageBitmap()` off the main thread. `egui_extras`'
///   own `ImageCrateLoader` decodes synchronously *on* the main thread with the
///   `image` crate, which stutters the UI on anything large — the browser
///   loader is registered after it and wins.
/// - The Phosphor icon family, without which [`crate::PhosphorIcon`] renders
///   as tofu.
///
/// On native there is no browser to decode with, so `ImageCrateLoader` stands —
/// which is why this is the right call on both targets rather than something a
/// native app has to opt out of.
// The one place the raw call belongs — this function IS the wrapper the lint
// points everyone else at.
#[allow(clippy::disallowed_methods)]
pub fn install_defaults(ctx: &egui::Context) {
    // Icons first: `configure_style` names the family, and the caller is told
    // to run that after this.
    crate::icons::install_phosphor_font(ctx);

    egui_extras::install_image_loaders(ctx);

    #[cfg(target_arch = "wasm32")]
    ctx.add_image_loader(std::sync::Arc::new(
        crate::image_loader::browser::BrowserImageLoader::default(),
    ));
}
