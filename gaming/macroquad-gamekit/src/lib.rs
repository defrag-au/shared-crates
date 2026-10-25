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
//! are unit-tested, paused, stepped and captured at exact moments. The wall
//! clock is read in one place, the [`harness`]: the frame time, and the timer
//! and frame cap the loop in [`app`] uses.
//!
//! A client's frame loop is [`app::run`]: implement [`Game`] and the loop
//! brings startup, the preloader, the glyph gate, captures and the frame log.
//! The gate matters for any screen with much text: without it, macroquad
//! re-uploads a font's whole atlas every time a run brings a new glyph, which
//! is a stall on the web and gigabytes of graphics memory on macOS.
//!
//! Stories for each module are in `macroquad-storybook`, under "gamekit".
//! Read `CATALOG.md` before adding a module, `CARD-FACES.md` before
//! building a card face, and `ANIMATION.md` before drawing or animating a
//! sprite: the rubric for motion, and the tools that check it. A game client's review loop is [`harness`].
//!
//! Two opt-in features, off by default because each needs a system library
//! natively on Linux: `audio` (sound, music, mute — [`audio`]) and
//! `gamepads` (pads by player slot — [`pads`]). In the browser both run
//! through miniquad plugins the page must load; [`web`] ships them.
//!
//! A third, `discord-auth`, is Discord login ([`auth`]): shared-crates
//! `discord-auth`'s miniquad edge, re-exported. The game links to its own
//! worker's login route, and [`auth::Session::load`] picks the session up
//! when the browser comes back. Its page plugin is
//! [`web::Plugin::DiscordAuth`].

pub mod app;
/// Discord login for a game: see the crate docs.
#[cfg(feature = "discord-auth")]
pub use discord_auth::miniquad as auth;
#[cfg(feature = "audio")]
pub mod audio;
pub mod bake;
pub mod dir;
pub mod ease;
pub mod face;
pub mod film;
pub mod fx;
pub mod gait;
pub mod harness;
pub mod icon;
pub mod input;
pub mod light;
pub mod motion;
#[cfg(feature = "gamepads")]
pub mod pads;
pub mod paint;
pub mod pixel;
pub mod plan;
pub mod pointer;
pub mod preload;
pub mod sequence;
pub mod shape;
pub mod sprite;
pub mod sprite_file;
pub mod starfield;
pub mod startup;
pub mod step;
pub mod tech;
pub mod text;
pub mod thumbs;
pub mod tween;
pub mod viewer;
pub mod web;

pub use app::{Backdrop, Frame, Game, Stage};
#[cfg(feature = "audio")]
pub use audio::{Audio, Panned, Voices, load_pcm, pan_gains, pan_mono, wav, wav_stereo};
pub use bake::{Baked, Baker};
pub use dir::Dir4;
pub use ease::Ease;
pub use face::{emblem, gem, line, plate, shrink_to_fit, tab, tab_width};
pub use fx::{Particles, Popups, Shake};
pub use gait::{Gait, Stride};
pub use harness::{
    Args, Capture, KeyScript, Script, Scripted, SheetSprite, Viewport, plain_target, sprite_sheet,
    switch, window_camera,
};
pub use icon::{Icon, draw_icon};
pub use input::{Actions, Binding, InputSnapshot, PadState};
pub use light::Lighting;
pub use motion::{Cycle, Shift};
#[cfg(feature = "gamepads")]
pub use pads::{Pad, Pads};
pub use paint::{
    Gradient, bevel, fill_circle_gradient, fill_gradient, grain, inner_shadow, noise_pixels,
    noise_texture, scanlines,
};
pub use pixel::{Grid, Onion, Palette, PixelError, PixelSprite, parse_frames};
pub use pointer::{HoverGrace, Pointer, PointerConfig, PointerEvent, Swipe, Swipes};
pub use preload::Preloader;
pub use sequence::{Sequencer, stagger};
pub use shape::{RoundedArt, cover_source, fill_rounded, stroke_rounded};
pub use sprite::{Pose, Sprite, draw_order, topmost};
pub use sprite_file::{SpriteFile, SpriteFileError};
pub use starfield::{Star, Starfield};
pub use startup::{Finished, Phase, Startup};
pub use step::{FixedStep, Tick};
pub use tech::{TechHit, TechLayout, TechPanel, TechStyle, TechTile, TileState};
pub use text::{
    Align, GlyphFlush, MacroquadMeasure, Measure, MeasureAt, Outline, Placed, RichText, Shadow,
    StyleId, TextLayout, TextStyle, draw_run, enable_glyph_gate, flush_glyphs, glyphs_pending,
    load_font, measure_run, register_font, ungated, warm_font_cache,
};
pub use thumbs::{Finger, StickShown, ThumbFrame, Thumbs};
pub use tween::{Animated, Lerp};
