//! Wrapping and shrink-to-fit.
//!
//! The one genuinely fiddly part of the crate, and the reason it uses real
//! glyph metrics rather than an estimate: "make it as big as fits" is only
//! correct if the measurement is what the renderer will actually draw. An
//! approximation is wrong in the direction nobody checks — text that overflows
//! its panel in the posted picture but looked fine in the preview.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};

use crate::{Fit, PixelRect};

/// Text laid out to fit a box.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedText {
    /// The chosen size, in pixels.
    pub font_size: f32,
    /// Distance between baselines.
    pub line_height: f32,
    /// The text, wrapped.
    pub lines: Vec<String>,
}

/// Advance width of one line, including tracking.
///
/// `h_advance` rather than the outline bounds: advance is what the draw loop
/// steps by, so measuring anything else would put centred text off-centre.
pub fn measure(font: &FontArc, scale: PxScale, text: &str, spacing: f32) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0;
    for ch in text.chars() {
        width += scaled.h_advance(scaled.glyph_id(ch)) + spacing;
    }
    // The trailing gap after the last glyph is not part of the line.
    (width - spacing).max(0.0)
}

/// Largest size at which `text` fits `rect`, with the wrapping that achieves it.
///
/// Returns `None` when it does not fit even at [`Fit::min`] — the caller draws
/// nothing rather than something illegible, and the box stays visibly empty,
/// which is a failure someone can see and fix.
///
/// Searches downward in steps rather than by bisection: the space is small (a
/// few dozen candidate sizes), wrapping changes discontinuously with size, and
/// bisection over a non-monotonic predicate finds local answers. Stepping is
/// slower in a way nothing here can measure and correct in a way that shows.
pub fn fit_text(
    font: &FontArc,
    text: &str,
    rect: PixelRect,
    fit: &Fit,
    letter_spacing: f32,
    image_height: f32,
) -> Option<FittedText> {
    let max_px = (fit.max * image_height).max(1.0);
    let min_px = (fit.min * image_height).max(1.0).min(max_px);

    // ~4% steps: finer than anyone can see at meme sizes, and it bounds the
    // loop at about eighty iterations for the widest plausible range.
    let mut size = max_px;
    while size >= min_px {
        if let Some(fitted) = try_size(font, text, rect, fit, letter_spacing, size) {
            return Some(fitted);
        }
        size *= 0.96;
    }

    None
}

fn try_size(
    font: &FontArc,
    text: &str,
    rect: PixelRect,
    fit: &Fit,
    letter_spacing: f32,
    size: f32,
) -> Option<FittedText> {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let spacing = letter_spacing * size;

    let lines = wrap(font, scale, text, rect.w, spacing);

    if fit.max_lines > 0 && lines.len() > fit.max_lines as usize {
        return None;
    }

    // A word longer than the box cannot be wrapped any further — the only
    // remedy is a smaller size, so report failure and let the search continue.
    if lines
        .iter()
        .any(|line| measure(font, scale, line, spacing) > rect.w)
    {
        return None;
    }

    let line_height = scaled.height() + scaled.line_gap();
    if line_height * lines.len() as f32 > rect.h {
        return None;
    }

    Some(FittedText {
        font_size: size,
        line_height,
        lines,
    })
}

/// Greedy word wrap to a pixel width.
///
/// Breaks on whitespace only. A single word wider than the box is left long
/// rather than split mid-word — [`try_size`] sees it does not fit and shrinks,
/// which is what a person would do. Hyphenating "SUPERCALIFRAGILISTIC" is not
/// an improvement.
///
/// Explicit newlines in the input are honoured as hard breaks, because someone
/// typing them meant them.
fn wrap(font: &FontArc, scale: PxScale, text: &str, max_width: f32, spacing: f32) -> Vec<String> {
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let mut current = String::new();

        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };

            if measure(font, scale, &candidate, spacing) <= max_width || current.is_empty() {
                current = candidate;
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }

        lines.push(current);
    }

    // A trailing hard newline produces an empty last line that occupies height
    // for nothing.
    while lines.last().is_some_and(|line| line.is_empty()) && lines.len() > 1 {
        lines.pop();
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real face, because the whole point of this module is that the metrics
    /// are the ones that will be drawn with. `ab_glyph` ships one for tests.
    fn font() -> FontArc {
        FontArc::try_from_slice(include_bytes!("../fonts/DejaVuSans.ttf") as &[u8])
            .expect("test font parses")
    }

    fn rect(w: f32, h: f32) -> PixelRect {
        PixelRect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        }
    }

    #[test]
    fn short_text_takes_the_largest_size_offered() {
        let fitted = fit_text(
            &font(),
            "OK",
            rect(400.0, 200.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .expect("fits");

        assert_eq!(fitted.lines, vec!["OK"]);
        // Fit::default().max is 0.16 of the 1000px image height.
        assert!(
            (fitted.font_size - 160.0).abs() < 1.0,
            "{}",
            fitted.font_size
        );
    }

    /// The behaviour the old point-based overlay could not do at all: text too
    /// long for one line becomes several, inside the box.
    #[test]
    fn long_text_wraps_rather_than_running_off_the_edge() {
        let fitted = fit_text(
            &font(),
            "a yield bearing NFT with no yield whatsoever",
            rect(300.0, 400.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .expect("fits");

        assert!(fitted.lines.len() > 1, "{:?}", fitted.lines);
        let scale = PxScale::from(fitted.font_size);
        for line in &fitted.lines {
            assert!(
                measure(&font(), scale, line, 0.0) <= 300.0,
                "line overflows: {line:?}"
            );
        }
    }

    /// The shrink half: the same text in a smaller box comes out smaller.
    #[test]
    fn a_tighter_box_gets_a_smaller_size() {
        let text = "a yield bearing NFT with no yield";
        let roomy = fit_text(
            &font(),
            text,
            rect(600.0, 400.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .unwrap();
        let cramped = fit_text(
            &font(),
            text,
            rect(200.0, 120.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .unwrap();

        assert!(
            cramped.font_size < roomy.font_size,
            "{} vs {}",
            cramped.font_size,
            roomy.font_size
        );
    }

    /// Every wrapped block has to fit the box's *height* too, or the composite
    /// spills over the panel edge.
    #[test]
    fn the_wrapped_block_fits_the_box_height() {
        let fitted = fit_text(
            &font(),
            "one two three four five six seven eight nine ten eleven twelve",
            rect(240.0, 150.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .expect("fits");

        assert!(
            fitted.line_height * fitted.lines.len() as f32 <= 150.0,
            "{} lines at {} = {}",
            fitted.lines.len(),
            fitted.line_height,
            fitted.line_height * fitted.lines.len() as f32
        );
    }

    /// A cap forces a smaller size rather than more lines. Compared against the
    /// uncapped fit of the same text, which uses more lines at a larger size —
    /// asserting `<= 2` alone would pass on text that never wrapped at all.
    #[test]
    fn a_line_cap_trades_size_for_lines() {
        let text = "one two three four five six";
        let uncapped = fit_text(
            &font(),
            text,
            rect(400.0, 400.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .expect("fits");

        let capped = fit_text(
            &font(),
            text,
            rect(400.0, 400.0),
            &Fit {
                max_lines: 2,
                ..Fit::default()
            },
            0.0,
            1000.0,
        )
        .expect("fits");

        assert!(uncapped.lines.len() > 2, "{:?}", uncapped.lines);
        assert!(capped.lines.len() <= 2, "{:?}", capped.lines);
        assert!(
            capped.font_size < uncapped.font_size,
            "{} vs {}",
            capped.font_size,
            uncapped.font_size
        );
    }

    /// Nothing legible fits, so nothing is drawn. The box stays visibly empty,
    /// which someone can see and fix — unlike text scaled to two pixels.
    #[test]
    fn text_that_cannot_fit_legibly_returns_nothing() {
        let strict = Fit {
            max: 0.16,
            min: 0.14,
            max_lines: 1,
        };
        let fitted = fit_text(
            &font(),
            "an entire sentence that will never fit on one line at this size",
            rect(100.0, 400.0),
            &strict,
            0.0,
            1000.0,
        );

        assert!(fitted.is_none());
    }

    /// A word wider than the box is left whole and the size drops instead —
    /// hyphenating mid-word looks like a bug, not a layout.
    /// A word wider than the box is left whole and the size drops instead —
    /// hyphenating mid-word looks like a bug, not a layout. The floor is lowered
    /// here because thirty-four characters across 300px genuinely needs a small
    /// size; the default `min` would (correctly) give up instead.
    #[test]
    fn an_unbreakable_word_shrinks_rather_than_splitting() {
        let fitted = fit_text(
            &font(),
            "SUPERCALIFRAGILISTICEXPIALIDOCIOUS",
            rect(300.0, 400.0),
            &Fit {
                min: 0.005,
                ..Fit::default()
            },
            0.0,
            1000.0,
        )
        .expect("fits");

        assert_eq!(fitted.lines, vec!["SUPERCALIFRAGILISTICEXPIALIDOCIOUS"]);
        assert!(fitted.font_size < 160.0, "{}", fitted.font_size);
    }

    /// Someone who typed a newline meant it.
    #[test]
    fn explicit_newlines_are_hard_breaks() {
        let fitted = fit_text(
            &font(),
            "top\nbottom",
            rect(600.0, 400.0),
            &Fit::default(),
            0.0,
            1000.0,
        )
        .expect("fits");

        assert_eq!(fitted.lines, vec!["top", "bottom"]);
    }

    /// Measurement steps by advance, so a centred line is actually centred —
    /// measuring outline bounds instead would drift by the side bearings.
    #[test]
    fn measuring_is_additive_over_advances() {
        let font = font();
        let scale = PxScale::from(64.0);

        let one = measure(&font, scale, "A", 0.0);
        let two = measure(&font, scale, "AA", 0.0);
        assert!((two - one * 2.0).abs() < 0.01, "{one} {two}");
    }

    #[test]
    fn tracking_widens_a_line_by_one_gap_less_than_its_glyphs() {
        let font = font();
        let scale = PxScale::from(64.0);

        let tight = measure(&font, scale, "ABCD", 0.0);
        let loose = measure(&font, scale, "ABCD", 10.0);
        assert!((loose - tight - 30.0).abs() < 0.01, "{tight} {loose}");
    }
}
