//! `macroquad-app` — the shell an app (rather than a game) needs on macroquad:
//! the frame of the page, how you move through it, and how you type into it.
//!
//! Run the app on [`macroquad_gamekit::Stage::Fluid`], so the stage is the
//! window at one unit per point, and lay it out each frame from
//! [`shell_layout`], which puts an [`AppBar`] at the top and a bottom
//! navigation bar (a rail beside the content on a wider screen) inside the
//! safe area the phone reports.
//!
//! # What lives here, and why not in `macroquad-widgets`
//!
//! `macroquad-widgets` keeps four stop signs so that its widgets stay small
//! and stateless: no long clipped lists, no text input, no focus, no layout
//! engine. An app that browses thousands of NFTs, searches them and keeps its
//! place in the browser's history needs the first three. They live here, on
//! purpose and behind their own lines:
//!
//! - **Machinery an app shell needs, built once:** the route in the page's
//!   hash and the back button ([`Router`]), the safe area and the keyboard
//!   ([`Insets`]), a real text field with the phone's own keyboard
//!   ([`TextField`]), HTTP with any method and the reply's status ([`http`]),
//!   a virtualised grid ([`ArtGrid`]), toasts ([`Toasts`]), the app bar and
//!   the navigation bar.
//! - **Still no layout engine.** Rects are computed by the app, as they are
//!   everywhere else on macroquad. [`shell_layout`] answers the one layout
//!   every screen shares; a screen lays out its own body.
//! - **Still no focus management.** One field is edited at a time, and the
//!   phone's keyboard decides what Tab does.
//! - **Widgets that hold no state still belong in `macroquad-widgets`.** A
//!   chip or a stat that an app here grows goes there once a second consumer
//!   wants it, by the widgets crate's own rule.
//!
//! # The page
//!
//! `js/app.js` is a miniquad plugin ([`PLUGIN_JS`]): stamp it into the web
//! directory from `build.rs`, as every plugin is, and the bundler stages it.
//! Natively each service has a plain-Rust stand-in, so an app runs, is
//! captured and is tested off the web: an in-memory history (`--route` opens
//! a deep screen), insets from `--insets`, a text field that reads the
//! keyboard, and HTTP on a thread with `ureq`.

pub mod app_bar;
pub mod art_grid;
pub mod chips;
pub mod glyph;
pub mod history;
pub mod http;
pub mod input;
pub mod insets;
pub mod layout;
pub mod nav;
pub mod toast;

pub use app_bar::{AppBar, BarAction, BarHit, Leading, icon_button};
pub use art_grid::{ArtGrid, GridGeometry, GridSpec};
pub use chips::{CHIP_H, Chip, ChipRow, chip_layout, chip_row};
pub use glyph::{Glyph, draw_glyph};
pub use history::{Route, Router};
pub use http::{HttpError, Method, Pending, Request, Response};
pub use input::{FIELD_H, FieldKind, FieldReply, Keyboard, TextField};
pub use insets::Insets;
pub use layout::{BAR_H, NAV_H, NavPlace, RAIL_W, ShellLayout, shell_layout};
pub use nav::{NavItem, bottom_nav, nav_rail};
pub use toast::{Toast, Toasts, Tone};

/// JavaScript source for the miniquad plugin. Stamp it into the web
/// directory from `build.rs` so the page loads the plugin the wasm was linked
/// against:
///
/// ```ignore
/// fs::write(web.join("app.js"), macroquad_app::PLUGIN_JS)?;
/// ```
pub const PLUGIN_JS: &str = include_str!("../js/app.js");

/// Plugin handshake for `gl.js`: `version` in `js/app.js` must match, so a
/// page carrying an older copy fails at instantiate rather than calling a
/// function gl.js would stub out.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn app_shell_crate_version() -> u32 {
    1
}

/// A colour as CSS reads it, for the page's own input to match the canvas.
pub(crate) fn css_rgba(c: macroquad::color::Color) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "rgba({},{},{},{:.3})",
        byte(c.r),
        byte(c.g),
        byte(c.b),
        c.a.clamp(0.0, 1.0)
    )
}

/// A `--name value` flag from the command line, natively: how a capture
/// opens a deep route or pretends to have a notch.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn flag(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_reads_as_css() {
        let c = macroquad::color::Color::new(1.0, 0.5, 0.0, 0.25);
        assert_eq!(css_rgba(c), "rgba(255,128,0,0.250)");
    }

    #[test]
    fn the_plugin_names_itself_as_the_handshake_does() {
        assert!(PLUGIN_JS.contains("name: 'app_shell'"));
        assert!(PLUGIN_JS.contains("version: 1,"));
    }
}
