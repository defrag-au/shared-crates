//! `ui-theme` — the design vocabulary both renderers speak.
//!
//! Renderer-free by construction: no egui, no macroquad, no wasm-bindgen, so a
//! miniquad binary, a Worker or a TUI can all hold it. Each front end supplies a
//! [`Paint`] impl for its own colour type and gets the whole model.
//!
//! # Why this exists
//!
//! `egui-widgets` grew a genuinely good colour model — named [`Token`]s, an
//! override enum that replaced `Option<Color32>`, encoding palettes validated
//! against colour-vision deficiency — and **only its return type was
//! egui-bound**. `macroquad-widgets` had a flat struct of eleven fields and no
//! override mechanism at all.
//!
//! So the two drifted, and it was measured (2026-09-16): six colours
//! byte-identical across the crates, three already diverged, and `success`
//! resolving to a different hue on each side. Nobody decided that; it is what
//! happens when one palette is maintained twice.
//!
//! The colour *science* was in worse shape — relative luminance was implemented
//! **four times** across three crates, in two precisions, because an
//! integration test cannot reach a `#[cfg(test)]` module. Two of those copies
//! had even drifted apart on the sRGB threshold itself (`0.039_28` against
//! `0.040_45`), so the contrast suite was not quite measuring what
//! `ColorTokens::on` computed. Here the maths is public and singular, and tests
//! call it instead of restating it.
//!
//! # Layers
//!
//! - **[`color`]** — [`Srgb`], the [`Paint`] bridge, and the colour science:
//!   WCAG luminance and contrast, CIELAB ΔE76, Machado dichromacy simulation,
//!   Oklab mixing.
//! - **[`tokens`]** — the chrome palette ([`ColorTokens`]) and a name for each
//!   entry ([`Token`]).
//! - **[`encoding`]** — what charts encode *data* with: categorical, diverging,
//!   sequential and identity kinds, each with the invariant a theme must not
//!   break.
//! - **[`ink`]** — [`Ink`], the override model, and the [`Palette`] trait a
//!   renderer's theme implements so `ink.resolve(&theme)` works unchanged.
//! - **[`type_scale`]** — the typography axis: [`TextRole`] (what text *is*),
//!   [`TextSize`] (how big), the [`TextScale`] ramp they resolve through, and
//!   [`TypeScale`]. Renderer-free by the same split as colour: it answers "what
//!   point size, in which family", and each front end builds its own font
//!   handle from that.
//! - **`egui` / `macroquad` features** — [`Paint`] and `From` impls for
//!   `egui::Color32` and `macroquad::color::Color`. They live here because the
//!   orphan rule forbids them anywhere else: to a widget crate, both the trait
//!   and the colour type are foreign. Enabling one never pulls the other.
//!
//! # Generic, so call sites do not move
//!
//! Each front end aliases the vocabulary to its own colour type:
//!
//! ```ignore
//! pub type ColorTokens = ui_theme::ColorTokens<egui::Color32>;
//! pub type Ink = ui_theme::Ink<egui::Color32>;
//! pub type SeriesPalette = ui_theme::SeriesPalette<egui::Color32>;
//! ```
//!
//! so `c.text_primary` still yields that renderer's colour and the ~48 files
//! naming `Ink::` / `Token::` never change.
//!
//! # Rules this crate keeps
//!
//! - **A colour decision is written down, not left absent.** That is what
//!   [`Token`] and [`Ink`] are for; `Option<Color32>` could not say which token
//!   a `None` deferred to, so two resolve sites for one field could silently
//!   disagree.
//! - **Legibility is computed, never hand-picked.** [`ColorTokens::on`] derives
//!   a foreground from the contrast ratio, so a new palette cannot quietly ship
//!   unreadable text.
//! - **Encoding colour is separate from chrome colour.** Charts encode data;
//!   chrome describes structure. Conflating them is how a palette change becomes
//!   a wrong chart that still looks fine.

pub mod color;
pub mod encoding;
pub mod ink;
pub mod tokens;
pub mod type_scale;

pub use color::{
    Dichromacy, Paint, Srgb, contrast_ratio, delta_e, from_oklab, lab, legible_on, mix, oklab,
    relative_luminance, simulate, to_linear, to_srgb, with_alpha,
};
pub use encoding::{Diverging, IdentityEnvelope, Sequential, Series, SeriesPalette};
pub use ink::{Ink, Palette};
pub use tokens::{ColorTokens, Token};
pub use type_scale::{Family, TextRole, TextScale, TextSize, TypeScale};
