//! `macroquad-gamekit` — the game-feel layer for macroquad clients.
//!
//! The pieces a card or board game needs between "draw a rectangle" and "the
//! card glides to the hand, turning, and its text stays sharp": easing and
//! tweens, a sequencer that plays animation cues in order, retained sprites with
//! rotated hit tests, baked faces, rich text with inline icons, rounded and
//! cover-cropped art, pointer handling for cards, and effects.
//!
//! The companion to `macroquad-widgets`: those widgets are stateless by
//! charter, and everything here is state that lives across frames. Every
//! module takes time as an argument and never reads the clock, so animations
//! are unit-tested, paused, stepped and captured at exact moments.
//!
//! Stories for each module are in `macroquad-storybook`, under "gamekit".
//! Read `CATALOG.md` before adding a module, and `CARD-FACES.md` before
//! building a card face. A game client's review loop is [`harness`].
//!
//! Two opt-in features, off by default because each needs a system library
//! natively on Linux: `audio` (sound, music, mute — [`audio`]) and
//! `gamepads` (pads by player slot — [`pads`]). In the browser both run
//! through miniquad plugins the page must load; [`web`] ships them.

#[cfg(feature = "audio")]
pub mod audio;
pub mod bake;
pub mod ease;
pub mod fx;
pub mod harness;
pub mod light;
#[cfg(feature = "gamepads")]
pub mod pads;
pub mod paint;
pub mod pointer;
pub mod sequence;
pub mod shape;
pub mod sprite;
pub mod text;
pub mod tween;
pub mod web;

#[cfg(feature = "audio")]
pub use audio::{Audio, load_pcm, wav};
pub use bake::{Baked, Baker};
pub use ease::Ease;
pub use fx::{Particles, Popups, Shake};
pub use harness::{Args, Capture, Script, Scripted, Viewport, plain_target, window_camera};
pub use light::Lighting;
#[cfg(feature = "gamepads")]
pub use pads::{Pad, Pads};
pub use paint::{
    Gradient, bevel, fill_circle_gradient, fill_gradient, grain, inner_shadow, noise_pixels,
    noise_texture,
};
pub use pointer::{HoverGrace, Pointer, PointerConfig, PointerEvent};
pub use sequence::{Sequencer, stagger};
pub use shape::{RoundedArt, cover_source, fill_rounded, stroke_rounded};
pub use sprite::{Pose, Sprite, draw_order, topmost};
pub use text::{
    Align, MacroquadMeasure, Measure, Outline, Placed, RichText, Shadow, StyleId, TextLayout,
    TextStyle, warm_font_cache,
};
pub use tween::{Animated, Lerp};
