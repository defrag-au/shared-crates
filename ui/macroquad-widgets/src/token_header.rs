//! `TokenHeader` — a token's identity and replay controls on one line: a disc
//! with its picture or its initial, the ticker, the venues it trades on, the
//! mode chip, and the settings / speed / pause buttons.
//!
//! The disc's art is the host's: macroquad has no clipping here, and the arena
//! rounds it with gamekit's `RoundedArt`, which this crate cannot depend on.
//! [`TokenLogo::paint`] is the seam — the host draws the texture into the disc
//! with the alpha the fade-in asks for.
//!
//! [`header_rects`] is the layout the buttons sit at, so a host can hit-test
//! them without restating the arithmetic.

use macroquad::prelude::*;
use ui_theme::{TextSize, mix};

use crate::market_ink::MarketInk;
use crate::marks::glow;
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;

/// The token's picture, once it has arrived.
#[derive(Clone, Copy)]
pub struct TokenLogo<'a> {
    pub texture: &'a Texture2D,
    /// Seconds since it arrived: the disc fades it in over the initial.
    pub age: f32,
    /// Draw the texture clipped into `rect` with `alpha` — the host's rounding.
    pub paint: &'a dyn Fn(&Texture2D, Rect, f32),
}

/// What the header draws.
#[derive(Clone, Copy)]
pub struct TokenHeaderVm<'a> {
    /// The token's ticker; a `$` is added when it has no space.
    pub ticker: &'a str,
    pub venues: &'a [&'a str],
    pub pools: usize,
    /// A stable colour for the token's disc.
    pub hue: Color,
    pub logo: Option<TokenLogo<'a>>,
    /// `REPLAY · 24H`, `LIVE`, `END OF TAPE`, `PAUSED`…
    pub mode: &'a str,
    /// The rate's label, `1M/S` … `1D/S`.
    pub speed: &'a str,
    /// The rate is one of the two fastest: the button is lit.
    pub speed_lit: bool,
    pub paused: bool,
    pub settings_open: bool,
    /// The fullscreen button, and whether the page is fullscreen now; `None`
    /// where there is no fullscreen to go to (an iPhone's Safari, an embed
    /// that does not allow it), and the button is not drawn.
    pub fullscreen: Option<bool>,
    pub ink: MarketInk,
}

/// Where the header's buttons are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderRects {
    /// Left of settings: only drawn when the host has fullscreen to offer.
    pub fullscreen: Rect,
    pub settings: Rect,
    pub speed: Rect,
    pub pause: Rect,
}

/// The header's button layout in `col`.
pub fn header_rects(col: Rect) -> HeaderRects {
    // Wide enough for the widest rate, `10M/S`.
    let speed = Rect::new(col.right() - 46.0, col.y + 40.0, 46.0, 20.0);
    let pause = Rect::new(speed.x - 34.0 - 6.0, speed.y, 34.0, speed.h);
    let settings = Rect::new(pause.x - pause.w - 6.0, pause.y, pause.w, pause.h);
    let fullscreen = Rect::new(settings.x - settings.w - 6.0, pause.y, pause.w, pause.h);
    HeaderRects {
        fullscreen,
        settings,
        speed,
        pause,
    }
}

/// Draw the header in `col`: `p` is the body face, `d` the display face the
/// ticker uses. Returns the buttons' rects.
pub fn token_header(p: &Painter, d: &Painter, vm: &TokenHeaderVm, col: Rect) -> HeaderRects {
    let rects = header_rects(col);

    // The token disc: its picture, faded in over the ticker's initial once it
    // arrives; the initial alone when it has none.
    let disc = vec2(col.x + 18.0, col.y + 30.0);
    glow(disc, 18.0, vm.hue, 0.8);
    draw_circle(
        disc.x,
        disc.y,
        18.0,
        mix(vm.hue, p.theme.color.bg_primary, 0.35),
    );
    let disc_box = Rect::new(disc.x - 18.0, disc.y - 18.0, 36.0, 36.0);
    let shown = match vm.logo {
        Some(logo) => {
            let a = (logo.age / 0.35).clamp(0.0, 1.0);
            (logo.paint)(logo.texture, disc_box, a);
            a
        }
        None => 0.0,
    };
    if shown < 1.0 {
        let initial: String = vm.ticker.chars().take(1).collect();
        d.text_in(
            &initial,
            disc_box,
            d.size(TextSize::Xl2),
            with_alpha(p.theme.color.text_primary, 1.0 - shown),
            crate::Align::Center,
        );
    }
    draw_circle_lines(
        disc.x,
        disc.y,
        18.0,
        1.5,
        mix(vm.hue, p.theme.color.text_primary, 0.4),
    );

    // The name over the venues, on two named midlines either side of the
    // disc's centre: two faces, so not one `stack_in`.
    let name = if vm.ticker.contains(' ') {
        vm.ticker.to_string()
    } else {
        format!("${}", vm.ticker)
    };
    let (name_mid, venue_mid) = (disc.y - 7.0, disc.y + 13.0);
    d.text_mid(
        &name,
        col.x + 46.0,
        name_mid,
        d.size(TextSize::Xl3),
        p.theme.color.text_primary,
    );
    // Up to the buttons on its right, never under them: venues drop off the
    // end (`+2`) until it fits, and as a last resort it is a count. Never cut
    // with an ellipsis, which reads as a mistake.
    let first_button = match vm.fullscreen {
        Some(_) => rects.fullscreen,
        None => rects.settings,
    };
    let room = first_button.x - 8.0 - (col.x + 46.0);
    let venue = venue_line(p, vm.venues, vm.pools, p.size(TextSize::Sm), room);
    p.text_mid(
        &venue,
        col.x + 46.0,
        venue_mid,
        p.size(TextSize::Sm),
        p.theme.color.text_muted,
    );

    // The mode chip.
    let cs = p.size(TextSize::Xs);
    let cw = p.measure(vm.mode, cs).width + 14.0;
    let chip = Rect::new(col.right() - cw, col.y + 12.0, cw, 18.0);
    let live = vm.mode.starts_with("LIVE");
    let tone = if live { vm.ink.sell } else { vm.ink.accent };
    draw_rounded_rect(chip.x, chip.y, chip.w, chip.h, 9.0, with_alpha(tone, 0.16));
    if live {
        let blink = (p.time() as f32 * 2.0).fract() < 0.6;
        draw_circle(
            chip.x - 7.0,
            chip.center().y,
            3.0,
            with_alpha(tone, if blink { 1.0 } else { 0.3 }),
        );
    }
    p.text_in(vm.mode, chip, cs, tone, crate::Align::Center);

    // The rate: chain time a second; a tap steps it. Lit at the two fastest,
    // where a busy hour lands in a blink.
    let speed = rects.speed;
    draw_rounded_rect(
        speed.x,
        speed.y,
        speed.w,
        speed.h,
        6.0,
        with_alpha(vm.ink.accent, if vm.speed_lit { 0.35 } else { 0.1 }),
    );
    p.text_in(
        vm.speed,
        speed,
        p.size(TextSize::Xs),
        if vm.speed_lit {
            p.theme.color.text_primary
        } else {
            p.theme.color.text_muted
        },
        crate::Align::Center,
    );

    // Pause: two bars, or a play mark while paused.
    let pause = rects.pause;
    draw_rounded_rect(
        pause.x,
        pause.y,
        pause.w,
        pause.h,
        6.0,
        with_alpha(vm.ink.accent, if vm.paused { 0.35 } else { 0.1 }),
    );
    let ink = if vm.paused {
        p.theme.color.text_primary
    } else {
        p.theme.color.text_muted
    };
    let c = pause.center();
    if vm.paused {
        draw_triangle(
            vec2(c.x - 4.0, c.y - 5.5),
            vec2(c.x - 4.0, c.y + 5.5),
            vec2(c.x + 5.5, c.y),
            ink,
        );
    } else {
        draw_rectangle(c.x - 4.5, c.y - 5.0, 3.0, 10.0, ink);
        draw_rectangle(c.x + 1.5, c.y - 5.0, 3.0, 10.0, ink);
    }

    // Settings: three sliders, lit while the panel is open.
    let settings = rects.settings;
    draw_rounded_rect(
        settings.x,
        settings.y,
        settings.w,
        settings.h,
        6.0,
        with_alpha(vm.ink.accent, if vm.settings_open { 0.35 } else { 0.1 }),
    );
    let settings_ink = if vm.settings_open {
        p.theme.color.text_primary
    } else {
        p.theme.color.text_muted
    };
    let c = settings.center();
    for (i, knob) in [(-1.0f32, 3.0f32), (0.0, -3.0), (1.0, 1.0)] {
        let y = c.y + i * 4.0;
        draw_line(c.x - 7.0, y, c.x + 7.0, y, 1.0, settings_ink);
        draw_circle(c.x + knob, y, 1.8, settings_ink);
    }

    // Fullscreen: four corners pointing out, or in while fullscreen.
    if let Some(on) = vm.fullscreen {
        let r = rects.fullscreen;
        draw_rounded_rect(r.x, r.y, r.w, r.h, 6.0, with_alpha(vm.ink.accent, 0.1));
        let c = r.center();
        let (h, arm) = (5.5f32, 3.5f32);
        for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let corner = vec2(c.x + sx * h, c.y + sy * h);
            // Out: the arms run back from the corner. In: from a corner
            // pulled toward the centre, outward.
            let (at, dir) = match on {
                false => (corner, -1.0),
                true => (vec2(c.x + sx * (h - arm), c.y + sy * (h - arm)), 1.0),
            };
            let ink = p.theme.color.text_muted;
            draw_line(at.x, at.y, at.x + dir * sx * arm, at.y, 1.5, ink);
            draw_line(at.x, at.y, at.x, at.y + dir * sy * arm, 1.5, ink);
        }
    }
    rects
}

/// The header's venue line, as long as `room` allows: every venue, then the
/// first few and `+N`, then only counts.
fn venue_line(p: &Painter, venues: &[&str], pools: usize, size: f32, room: f32) -> String {
    let pools = format!("{pools} pool{}", if pools == 1 { "" } else { "s" });
    for keep in (1..=venues.len()).rev() {
        let mut names = venues[..keep].join(" · ");
        if keep < venues.len() {
            names = format!("{names} +{}", venues.len() - keep);
        }
        let line = format!("{names} · {pools}");
        if p.measure(&line, size).width <= room {
            return line;
        }
    }
    match venues.len() {
        0 => pools,
        n => format!("{n} venue{} · {pools}", if n == 1 { "" } else { "s" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buttons_sit_left_of_each_other_without_overlapping() {
        let col = Rect::new(20.0, 0.0, 350.0, 844.0);
        let r = header_rects(col);
        assert_eq!(r.speed.right(), col.right());
        assert!(r.pause.right() < r.speed.x);
        assert!(r.settings.right() < r.pause.x);
        assert!(r.fullscreen.right() < r.settings.x);
        assert_eq!(r.settings.w, r.pause.w);
        assert_eq!(r.fullscreen.w, r.pause.w);
    }
}
