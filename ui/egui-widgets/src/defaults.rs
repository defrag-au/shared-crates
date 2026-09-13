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
//! [`install`] is that block, once.
//!
//! ## Why the theme is an argument and not a second call
//!
//! [`install_defaults`] — what this was — installed the fonts and the loaders
//! and stopped, leaving the theme to a separate `configure_style`. Four of the
//! twenty-two apps that called it never made that second call.
//!
//! Those four look correct today, but only by luck: `Theme::default()` happens
//! to *be* `tokyo_night`, so the widgets match the hand-rolled Tokyo Night
//! palettes the apps draw their own chrome from. The day that default moves —
//! and there are four presets in the crate already — those four apps repaint
//! and nothing in them says why. An app should not be depending on which
//! preset the library thinks is its favourite.
//!
//! So the theme is a parameter. You cannot set an app up without saying what it
//! looks like, and the compiler is what says so.

/// Everything an app must install before its first frame: the loaders, the icon
/// font, and the theme.
///
/// ```ignore
/// use egui_widgets::theme::{FontStrategy, Theme};
/// egui_widgets::install(
///     &cc.egui_ctx,
///     Theme::tokyo_night().with_fonts(FontStrategy::monospace()),
/// );
/// ```
///
/// Idempotent, so calling it twice is harmless. The theme goes in last because
/// the icon family has to be registered before the style names it — an ordering
/// that used to be the caller's to remember.
pub fn install(ctx: &egui::Context, theme: crate::theme::Theme) {
    install_assets(ctx);
    crate::theme::install_theme(ctx, theme);
}

/// Install the image loaders and the icon font.
///
/// Prefer [`install`], which also binds the theme. This is for a host that
/// genuinely wants the assets without one — a widget gallery rendering several
/// themes at once, say.
///
/// Idempotent, so calling it twice is harmless.
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
pub fn install_assets(ctx: &egui::Context) {
    // Icons first: the style names the family, so it has to exist by the time
    // the theme is applied.
    crate::icons::install_phosphor_font(ctx);

    egui_extras::install_image_loaders(ctx);

    #[cfg(target_arch = "wasm32")]
    ctx.add_image_loader(std::sync::Arc::new(
        crate::image_loader::browser::BrowserImageLoader::default(),
    ));
}

/// Install the loaders and the icon font, but no theme.
///
/// Renamed and deprecated rather than quietly given a theme: an app calling
/// this was getting whatever `Theme::default()` happens to be for every widget,
/// and picking one on its behalf would repaint it without asking. Move to
/// [`install`] and name the theme — `Theme::tokyo_night()` preserves what an app
/// with a hand-rolled Tokyo Night palette already looks like,
/// `Theme::default()` preserves what a widget in it already looks like, and
/// which of those you want is a question only the app can answer.
#[deprecated(
    since = "0.1.0",
    note = "use `install(ctx, theme)`, which also binds the theme — see the module docs for the two-palette bug this shape caused"
)]
pub fn install_defaults(ctx: &egui::Context) {
    install_assets(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{FontStrategy, Theme, ThemeExt};

    #[test]
    fn install_binds_the_theme_the_caller_named() {
        // Named against a preset that is NOT the default, on purpose.
        // `Theme::default()` IS `tokyo_night`, so asserting the two differ
        // proves nothing — an earlier version of this test did exactly that and
        // failed, which is how the assumption got checked at all.
        let ctx = egui::Context::default();
        install(&ctx, Theme::opensea());
        assert_eq!(
            ThemeExt::tokens(&ctx).color.bg_primary,
            Theme::opensea().color.bg_primary,
        );
        assert_ne!(
            Theme::opensea().color.bg_primary,
            Theme::default().color.bg_primary,
            "the preset must differ from the default, or this proves nothing"
        );
    }

    #[test]
    fn a_font_strategy_survives_being_folded_into_the_theme() {
        // `with_fonts` replaced `configure_style`'s inline pairing. If it
        // dropped the strategy, every migrated app would silently revert to the
        // proportional scale.
        let ctx = egui::Context::default();
        let mono = Theme::tokyo_night().with_fonts(FontStrategy::monospace());
        install(&ctx, mono.clone());
        assert_eq!(ThemeExt::tokens(&ctx).text, mono.text);
        assert_ne!(
            mono.text,
            Theme::tokyo_night().text,
            "monospace must actually change the scale, or this proves nothing"
        );
    }

    #[test]
    fn installing_assets_alone_leaves_the_theme_untouched() {
        // The escape hatch has to actually escape — a gallery rendering several
        // themes needs the assets without one being chosen for it.
        let ctx = egui::Context::default();
        install_assets(&ctx);
        assert_eq!(
            ThemeExt::tokens(&ctx).color.bg_primary,
            Theme::default().color.bg_primary,
        );
    }
}
