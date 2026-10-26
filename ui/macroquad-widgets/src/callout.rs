//! `Callout` — one sentence about what just happened, in the colour that says
//! how it went, with an optional action at its end.
//!
//! # Why this is a widget
//!
//! Five macroquad surfaces in this workspace each wrote their own version of
//! "a status line": `frontend-fastbuy`'s `SENT · {hash}` / `LANDED in block
//! {h}`, `frontend-pump`'s failure reasons, `frontend-mq`'s error and "Try
//! again", `frontend-blocks`' `NO SIGNAL · {why}` and `cardano-hub`'s prefixed
//! status. Each re-derived the tone→ink map, which is the same drift
//! `tx-cart-view` was created to stop one layer up. All five now draw this
//! widget, which is what the tone vocabulary is sized for.
//!
//! The vocabulary is [`CalloutTone`]. Five of its six variants are
//! `tx-cart-view`'s `Tone` exactly (`Normal`, `Muted`, `Busy`, `Positive`,
//! `Negative`); `Warn` is the amber a replay's pending state and a paused
//! feed both reach for, which that enum has no name for.

use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::Painter;

/// How the thing being reported went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalloutTone {
    /// Nothing went wrong and nothing needs saying: the default.
    Normal,
    /// A hint, a placeholder, a line that is not the point.
    Muted,
    /// In flight, or waiting: the accent.
    Busy,
    /// It worked.
    Positive,
    /// It worked, but a reader should look at something.
    Warn,
    /// It failed.
    Negative,
}

/// What the callout says.
#[derive(Clone, Copy, Debug)]
pub struct CalloutVm<'a> {
    pub tone: CalloutTone,
    /// The sentence. It is fitted to the room the action leaves, so a host
    /// never has to shorten it itself.
    pub text: &'a str,
    /// A label at the end of the line — `Try again`, `Dismiss` — whose rect is
    /// returned for the host to hit-test. Text, not a button: a host that
    /// wants a button has [`Button`](crate::Button).
    pub action: Option<&'a str>,
    /// The ramp step. A host passes the step its own screen already uses, so
    /// adopting the widget is not a restyle: the arena draws a note under a
    /// panel at [`TextSize::Base`], a station's telemetry line at
    /// [`TextSize::Xl`].
    pub size: TextSize,
}

/// The ink a tone is drawn in.
pub fn tone_ink(theme: &crate::Theme, tone: CalloutTone) -> Color {
    match tone {
        CalloutTone::Normal => theme.color.text_primary,
        CalloutTone::Muted => theme.color.text_muted,
        CalloutTone::Busy => theme.color.accent,
        CalloutTone::Positive => theme.color.success,
        CalloutTone::Warn => theme.color.warning,
        CalloutTone::Negative => theme.color.error,
    }
}

/// Draw `vm` in `rect`, on one line, and return the action's rect.
///
/// The sentence and the action share a midline, so a row of fixed height reads
/// the same whether or not there is an action. The sentence is fitted to the
/// room the action leaves, and nothing is drawn when the rect has no room for a
/// word.
pub fn callout(p: &Painter, vm: &CalloutVm, rect: Rect) -> Option<Rect> {
    let ink = tone_ink(&p.theme, vm.tone);
    let size = p.size(vm.size);
    let mid = rect.y + rect.h * 0.5;
    let gap = 12.0;
    let action_w = vm.action.map_or(0.0, |a| p.measure(a, size).width + gap);
    let text = p.fit(vm.text, size, (rect.w - action_w).max(0.0));
    p.text_mid(&text, rect.x, mid, size, ink);
    vm.action.map(|action| {
        let w = p.measure(action, size).width;
        let r = Rect::new(rect.right() - w, rect.y, w, rect.h);
        p.text_in(action, r, size, ink, crate::Align::End);
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six tones are six distinct inks on the shipped theme: a callout in
    /// the wrong colour is worse than none, because it reads as the right one.
    #[test]
    fn every_tone_resolves_to_a_different_ink() -> Result<(), String> {
        let theme = crate::Theme::tokyo_night();
        let mut seen: Vec<(CalloutTone, Color)> = Vec::new();
        for tone in [
            CalloutTone::Normal,
            CalloutTone::Muted,
            CalloutTone::Busy,
            CalloutTone::Positive,
            CalloutTone::Warn,
            CalloutTone::Negative,
        ] {
            let ink = tone_ink(&theme, tone);
            if let Some((other, _)) = seen.iter().find(|(_, c)| *c == ink) {
                return Err(format!("{tone:?} has the same ink as {other:?}"));
            }
            seen.push((tone, ink));
        }
        Ok(())
    }
}
