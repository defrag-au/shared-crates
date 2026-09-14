//! `TestPass` — drive one egui pass from a test without tripping epaint's
//! unapplied-texture guard.
//!
//! A real frontend hands `FullOutput::textures_delta` to its renderer, which
//! uploads the glyphs and images egui just rasterised. A test has no renderer,
//! so it drops the output — and since epaint 0.36 that is a `debug_assert!` in
//! `TexturesDelta::drop`:
//!
//! ```text
//! Dropped TexturesDelta with 1 unapplied deltas. Deltas need to be handled.
//! ```
//!
//! The guard is worth keeping: in an app it catches a renderer that quietly
//! stopped uploading, which otherwise shows up as text that never appears. But
//! in a test the deltas have nowhere to go, so they are discarded explicitly.
//! It fires only in debug builds, which is exactly where tests run — so a
//! suite that passes in release is not evidence the guard is satisfied.
//!
//! Use these in place of [`egui::Context::run_ui`] / [`egui::Context::end_pass`];
//! they are otherwise identical and return the same `FullOutput`, so a harness
//! that inspects `shapes` keeps working unchanged.
//!
//! ```
//! use egui_widgets::test_pass::TestPass as _;
//!
//! let ctx = egui::Context::default();
//! let output = ctx.test_pass(egui::RawInput::default(), |ui| {
//!     ui.label("hello");
//! });
//! assert!(!output.shapes.is_empty());
//! ```
//!
//! Public rather than `#[cfg(test)]` because every consumer's own tests hit the
//! same guard the moment they drive a widget from this crate.

use egui::{Context, FullOutput, RawInput, Ui};

/// One egui pass, with the texture deltas discarded rather than uploaded.
pub trait TestPass {
    /// [`egui::Context::run_ui`], minus the texture upload a test cannot perform.
    fn test_pass(&self, input: RawInput, run_ui: impl FnMut(&mut Ui)) -> FullOutput;

    /// [`egui::Context::end_pass`], for harnesses built on `begin_pass`.
    fn end_test_pass(&self) -> FullOutput;
}

impl TestPass for Context {
    fn test_pass(&self, input: RawInput, run_ui: impl FnMut(&mut Ui)) -> FullOutput {
        discard_textures(self.run_ui(input, run_ui))
    }

    fn end_test_pass(&self) -> FullOutput {
        discard_textures(self.end_pass())
    }
}

/// Drop the deltas a test has no renderer to upload.
fn discard_textures(mut output: FullOutput) -> FullOutput {
    output.textures_delta.clear();
    output
}
