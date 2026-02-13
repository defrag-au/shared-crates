//! Color utilities for widget styling
//!
//! Provides helpers for working with colors, including contrast detection
//! for determining optimal text colors on colored backgrounds.

/// Determines the optimal text color (black or white) for a given background color.
///
/// Uses the WCAG relative luminance formula to calculate perceived brightness,
/// then returns black for light backgrounds and white for dark backgrounds.
///
/// # Arguments
/// * `hex_color` - A hex color string (with or without leading `#`)
///
/// # Returns
/// `"#000000"` for light backgrounds, `"#ffffff"` for dark backgrounds.
/// Returns `"#ffffff"` if the color cannot be parsed.
///
/// # Example
/// ```
/// use ui_core::color::optimal_text_color;
///
/// assert_eq!(optimal_text_color("#ffffff"), "#000000"); // white -> black text
/// assert_eq!(optimal_text_color("#000000"), "#ffffff"); // black -> white text
/// assert_eq!(optimal_text_color("#0072b5"), "#ffffff"); // Pantone 641 -> white text
/// assert_eq!(optimal_text_color("#7dc45c"), "#000000"); // light green -> black text
/// ```
pub fn optimal_text_color(hex_color: &str) -> &'static str {
    match parse_hex_color(hex_color) {
        Some((r, g, b)) => {
            let luminance = relative_luminance(r, g, b);
            // Threshold of 0.35 balances readability:
            // - Light colors (yellow, light green, white) get black text
            // - Most saturated colors get white text
            if luminance > 0.35 {
                "#000000" // Light background -> black text
            } else {
                "#ffffff" // Dark background -> white text
            }
        }
        None => "#ffffff", // Default to white text on parse failure
    }
}

/// Calculates the relative luminance of an RGB color.
///
/// Uses the WCAG 2.0 formula for relative luminance:
/// L = 0.2126 * R + 0.7152 * G + 0.0722 * B
///
/// Where R, G, B are linearized (gamma-corrected) values.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let r = linearize(r as f64 / 255.0);
    let g = linearize(g as f64 / 255.0);
    let b = linearize(b as f64 / 255.0);

    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Linearizes an sRGB color component (gamma correction).
fn linearize(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Parses a hex color string into RGB components.
///
/// Supports formats: `#RRGGBB`, `RRGGBB`, `#RGB`, `RGB`
pub fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');

    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 => {
            // Short form: #RGB -> #RRGGBB
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some((r, g, b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimal_text_color() {
        // Light colors -> black text
        assert_eq!(optimal_text_color("#ffffff"), "#000000"); // White
        assert_eq!(optimal_text_color("#ffff00"), "#000000"); // Yellow
        assert_eq!(optimal_text_color("#7dc45c"), "#000000"); // Light green

        // Dark/saturated colors -> white text
        assert_eq!(optimal_text_color("#000000"), "#ffffff"); // Black
        assert_eq!(optimal_text_color("#0072b5"), "#ffffff"); // Pantone 641
        assert_eq!(optimal_text_color("#c45c5c"), "#ffffff"); // Muted red
        assert_eq!(optimal_text_color("#5c8dc4"), "#ffffff"); // Blue
        assert_eq!(optimal_text_color("#c45cc4"), "#ffffff"); // Purple

        // Without hash prefix
        assert_eq!(optimal_text_color("ffffff"), "#000000");
        assert_eq!(optimal_text_color("000000"), "#ffffff");

        // Short form
        assert_eq!(optimal_text_color("#fff"), "#000000");
        assert_eq!(optimal_text_color("#000"), "#ffffff");
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex_color("ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_hex_color("#0072b5"), Some((0, 114, 181)));
        assert_eq!(parse_hex_color("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("abc"), Some((170, 187, 204)));
        assert_eq!(parse_hex_color("invalid"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
    }
}
