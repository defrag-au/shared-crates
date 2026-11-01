//! `Insets` — the parts of the screen the app must not put anything under:
//! the notch and status bar, the home indicator, a rounded corner's cut, and
//! the soft keyboard while it is open.
//!
//! In points, which on [`macroquad_gamekit::Stage::Fluid`] are the stage's own
//! units. On the web they come from the page (`env(safe-area-inset-*)` and the
//! visual viewport); natively they are zero unless a capture asks for a phone's
//! with `--insets top,right,bottom,left[,keyboard]`, so a layout is reviewed
//! with a notch without a phone to hand.

use macroquad::prelude::Rect;

/// See the [module docs](self).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// How much of the bottom the soft keyboard covers while it is open. It
    /// covers the bottom inset too, so the two are not added.
    pub keyboard: f32,
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn app_inset(side: u32) -> f32;
}

impl Insets {
    /// This frame's insets. Cheap enough to read every frame, which is how a
    /// phone turned on its side (its notch moves to a side) is followed.
    pub fn read() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let side = |i| unsafe { app_inset(i) }.max(0.0);
            Self {
                top: side(0),
                right: side(1),
                bottom: side(2),
                left: side(3),
                keyboard: side(4),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::flag("--insets")
                .and_then(|v| Self::parse(&v))
                .unwrap_or_default()
        }
    }

    /// `top,right,bottom,left[,keyboard]` in points, as `--insets` takes them.
    pub fn parse(s: &str) -> Option<Self> {
        let n: Vec<f32> = s
            .split(',')
            .map(|p| p.trim().parse::<f32>())
            .collect::<Result<_, _>>()
            .ok()?;
        match n[..] {
            [top, right, bottom, left] => Some(Self {
                top,
                right,
                bottom,
                left,
                keyboard: 0.0,
            }),
            [top, right, bottom, left, keyboard] => Some(Self {
                top,
                right,
                bottom,
                left,
                keyboard,
            }),
            _ => None,
        }
    }

    /// What the bottom loses: the keyboard while it is open, else the home
    /// indicator's inset.
    pub fn bottom_cover(&self) -> f32 {
        self.bottom.max(self.keyboard)
    }

    /// `area` less the insets on every side, keyboard included.
    pub fn safe(&self, area: Rect) -> Rect {
        let bottom = self.bottom_cover();
        Rect::new(
            area.x + self.left,
            area.y + self.top,
            (area.w - self.left - self.right).max(0.0),
            (area.h - self.top - bottom).max(0.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phones_insets_parse() {
        let i = Insets::parse("47, 0, 34, 0").unwrap();
        assert_eq!((i.top, i.bottom, i.keyboard), (47.0, 34.0, 0.0));
        let k = Insets::parse("47,0,34,0,336").unwrap();
        assert_eq!(k.keyboard, 336.0);
        assert_eq!(Insets::parse("47,0"), None);
        assert_eq!(Insets::parse("a,b,c,d"), None);
    }

    #[test]
    fn the_keyboard_covers_the_home_indicator_rather_than_adding_to_it() {
        let i = Insets {
            top: 47.0,
            bottom: 34.0,
            keyboard: 300.0,
            ..Insets::default()
        };
        let safe = i.safe(Rect::new(0.0, 0.0, 390.0, 844.0));
        assert_eq!(safe, Rect::new(0.0, 47.0, 390.0, 844.0 - 47.0 - 300.0));
    }
}
