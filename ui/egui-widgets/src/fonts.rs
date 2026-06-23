//! Remote font loading — fetch TTF/OTF fonts from the published font bucket
//! (Cloudflare R2, `https://assets.augmint.bot/fonts/`) at runtime and register them
//! with egui. Cross-platform via `ehttp`: a real HTTP request on native, `fetch` in the
//! browser.
//!
//! egui does its own glyph rasterisation and can't use the host's installed fonts, so a
//! remote font has to be downloaded as **bytes** and handed to egui. egui's rasteriser
//! needs **TTF/OTF** — NOT WOFF2 — so the bucket serves raw `.ttf` files.
//!
//! The embedded DejaVu fallback (see [`crate::icons::install_phosphor_font`]) stays in
//! place *behind* any remote font, so there's no flash-of-tofu while a download is in
//! flight and the UI degrades gracefully if a fetch fails or the bucket is unreachable.
//!
//! ```no_run
//! # use egui_widgets::{fonts, icons};
//! # let ctx = egui::Context::default();
//! icons::install_phosphor_font(&ctx); // bundled fonts first (incl. DejaVu fallback)
//! // Make JetBrains Mono the primary monospace face, pulled from R2:
//! fonts::load_remote_font(
//!     &ctx,
//!     fonts::r2::JETBRAINS_MONO,
//!     egui::FontFamily::Monospace,
//!     egui::epaint::text::FontPriority::Highest,
//! );
//! ```

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{Context, FontData, FontFamily};

/// Base URL of the published font bucket. Append a filename (see [`r2`]).
pub const REMOTE_FONT_BASE: &str = "https://assets.augmint.bot/fonts/";

/// Known filenames currently published in the bucket. Use with [`load_remote_font`].
/// (Pass any other filename directly — these are just the curated set.)
pub mod r2 {
    pub const NOTO_SANS_BOLD: &str = "NotoSans-Bold.ttf";
    pub const NOTO_SANS_MONO: &str = "NotoSansMono-Regular.ttf";
    pub const JETBRAINS_MONO: &str = "JetBrainsMono-Regular.ttf";
    /// Display / heading face.
    pub const ANTON: &str = "Anton-Regular.ttf";
    // Inter (static TTFs from the rsms/inter release `extras/ttf/`) — upload pending.
    pub const INTER_REGULAR: &str = "Inter-Regular.ttf";
    pub const INTER_SEMIBOLD: &str = "Inter-SemiBold.ttf";
    pub const INTER_BOLD: &str = "Inter-Bold.ttf";
}

/// Asynchronously fetch `file` (e.g. [`r2::JETBRAINS_MONO`]) from [`REMOTE_FONT_BASE`]
/// and install it into `family` at `priority`:
/// - [`FontPriority::Highest`] — make it the **primary** font for that family (used
///   whenever it has the glyph), keeping existing fonts (and the DejaVu fallback) behind.
/// - [`FontPriority::Lowest`] — add it as an extra fallback after everything else.
///
/// Non-blocking: the request runs in the background and the font appears at the next
/// repaint once it lands. A failed/non-200 fetch is a no-op (the bundled fonts remain),
/// logged at warn level. Idempotent per `file` — egui skips a font name already loaded.
pub fn load_remote_font(ctx: &Context, file: &str, family: FontFamily, priority: FontPriority) {
    let url = format!("{REMOTE_FONT_BASE}{file}");
    let ctx = ctx.clone();
    let name = file.to_owned();
    ehttp::fetch(ehttp::Request::get(&url), move |result| match result {
        Ok(resp) if resp.ok => {
            ctx.add_font(FontInsert::new(
                &name,
                FontData::from_owned(resp.bytes),
                vec![InsertFontFamily { family, priority }],
            ));
            ctx.request_repaint();
        }
        Ok(resp) => log::warn!("remote font '{name}': HTTP {}", resp.status),
        Err(e) => log::warn!("remote font '{name}': {e}"),
    });
}
