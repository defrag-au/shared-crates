//! Stories for the page pieces both apps draw: `Message` (a screen with
//! nothing else to show yet), `surface` (the panel every card sits on) and
//! `Painter::wrap` (prose broken into lines, a long word cut where it must
//! be), each at a phone's width.

use macroquad::prelude::*;
use macroquad_widgets::{Align, Message, Painter, TextSize, surface};

/// A phone's width: where each of these is read first.
const PHONE_W: f32 = 390.0;

/// Which piece a story shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageView {
    /// A heading and a sentence, as a page shows while it loads.
    Message,
    /// The same with an action under it, as an empty state offers one.
    MessageAction,
    /// Surfaces of several sizes, one clipped at its foot by a scroll.
    Surface,
    /// A paragraph wrapped at three widths, with a stake address too long
    /// for any line.
    Wrap,
}

pub struct PageStory {
    view: PageView,
    taps: u32,
}

impl PageStory {
    pub fn new(view: PageView) -> Self {
        Self { view, taps: 0 }
    }

    pub fn draw(&mut self, p: &Painter, area: Rect) {
        let c = p.theme.color;
        let phone = Rect::new(area.x, area.y, PHONE_W.min(area.w), area.h.min(700.0));
        let label = |text: &str, y: f32| {
            let r = Rect::new(area.x, y, area.w, 18.0);
            p.text_in(text, r, p.size(TextSize::Sm), c.text_muted, Align::Start);
        };
        match self.view {
            PageView::Message | PageView::MessageAction => {
                draw_rectangle(phone.x, phone.y, phone.w, phone.h, c.bg_primary);
                let m = Message::new(
                    "Your wallets",
                    "Reading what your wallets hold in this collection. A wallet that \
                     cannot be read says why under Wallets.",
                );
                let m = match self.view {
                    PageView::MessageAction => m.action("Add a wallet"),
                    _ => m,
                };
                if m.show_in(p, phone) {
                    self.taps += 1;
                }
                if self.view == PageView::MessageAction {
                    label(&format!("action taps: {}", self.taps), phone.bottom() + 8.0);
                }
            }
            PageView::Surface => {
                let mut y = area.y;
                for (note, h) in [("a stat card", 84.0), ("a list's panel", 180.0)] {
                    label(note, y);
                    surface(p, Rect::new(area.x, y + 22.0, PHONE_W - 24.0, h));
                    y += h + 40.0;
                }
                label("clipped at its foot, as a scrolled list cuts it", y);
                let clip = Rect::new(area.x, y + 22.0, PHONE_W - 24.0, 60.0);
                macroquad_gamekit::with_clip(clip, || {
                    surface(p, Rect::new(clip.x, clip.y, clip.w, 120.0));
                });
            }
            PageView::Wrap => {
                // A real holder's stake address (Toolhead #0001's owner, from
                // collection-ownership's public bundle): the word too long for a line.
                let text = "Paste the stake address \
                            stake1uylq65sr745njk58sm7lkgwdgve26ls8vjfwss5wnm6555cf9mmrh or a \
                            $handle; the line breaks at spaces and cuts a word only when one is \
                            wider than the whole line.";
                let size = p.size(TextSize::Md);
                let mut y = area.y;
                for w in [PHONE_W - 24.0, 240.0, 150.0] {
                    label(&format!("{w:.0} points wide"), y);
                    y += 22.0;
                    let lines = p.wrap(text, size, w);
                    let line_h = size * 1.45;
                    draw_rectangle_lines(
                        area.x - 1.0,
                        y,
                        w + 2.0,
                        lines.len() as f32 * line_h,
                        1.0,
                        c.border,
                    );
                    for (i, l) in lines.iter().enumerate() {
                        let r = Rect::new(area.x, y + i as f32 * line_h, w, line_h);
                        p.text_in(l, r, size, c.text_primary, Align::Start);
                    }
                    y += lines.len() as f32 * line_h + 20.0;
                }
            }
        }
    }
}
