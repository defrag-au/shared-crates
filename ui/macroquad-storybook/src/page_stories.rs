//! Stories for the page pieces both apps draw: `Message` (a screen with
//! nothing else to show yet), `surface` (the panel every card sits on),
//! `Painter::wrap` (prose broken into lines, a long word cut where it must
//! be), and the holder profile a collection's achievements are read on, each at
//! a phone's width.

use macroquad::prelude::*;
use macroquad_widgets::theme::with_alpha;
use macroquad_widgets::{
    Align, Message, Painter, StatTile, StatTilesVm, TextSize, draw_rounded_rect, stat_tiles,
    surface,
};

/// A phone's width: where each of these is read first.
const PHONE_W: f32 = 390.0;

/// Which rung of the allocation ladder a holder has reached.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rung {
    Gtd,
    Fcfs,
}

/// One real holder's profile, as the 2026-10-31 replay scored it.
struct Profile {
    /// The ADA handle, where the holder has one; most do not.
    handle: Option<&'static str>,
    stake: &'static str,
    rung: Rung,
    core: u32,
    core_total: u32,
    flavour: u32,
    earned: u32,
    open: u32,
    missed: u32,
    unread: u32,
    rarest: &'static str,
    /// `(axis, earned, open, missed, not measured)`.
    axes: &'static [(&'static str, u32, u32, u32, u32)],
    /// The open achievements nearest to being earned: `(title, what is left, how far)`.
    closest: &'static [(&'static str, &'static str, f32)],
}

/// The catalogue's proposed GTD bar: two core achievements.
const GTD_CORE: u32 = 2;

/// Achievements this build can evaluate at all; the other five need the token.
const HOLDER_EVALUABLE: u32 = 58;

/// A holder with room to spare — `$djo`, the largest holding in the replay
/// (470 assets, never sold).
const HOLDS_EVERYTHING: Profile = Profile {
    handle: Some("$djo"),
    stake: "4331030…6a6",
    rung: Rung::Gtd,
    core: 5,
    core_total: 10,
    flavour: 27,
    earned: 32,
    open: 19,
    missed: 7,
    unread: 5,
    rarest: "2.0%",
    axes: &[
        ("PROVENANCE", 3, 1, 7, 0),
        ("CONVICTION", 9, 4, 0, 0),
        ("MARKET", 0, 13, 0, 0),
        ("CURATION", 20, 1, 0, 0),
        ("STAKE", 0, 0, 0, 5),
    ],
    closest: &[
        ("Year One", "356/365 days · 11 days left", 356.0 / 365.0),
        ("Ten Pack", "5/10 minted", 0.5),
        ("Three Crowns", "1/3 values", 1.0 / 3.0),
        ("Churner", "1.0 movements per asset", 0.1),
    ],
};

/// A holder one core away — a churner with three achievements and no handle,
/// which is the commoner case and the one the page has to make useful.
const ONE_CORE_SHORT: Profile = Profile {
    handle: None,
    stake: "ee28e0c…3cb",
    rung: Rung::Fcfs,
    core: 1,
    core_total: 10,
    flavour: 2,
    earned: 3,
    open: 53,
    missed: 2,
    unread: 5,
    rarest: "11.3%",
    axes: &[
        ("PROVENANCE", 0, 10, 1, 0),
        ("CONVICTION", 2, 10, 1, 0),
        ("MARKET", 0, 13, 0, 0),
        ("CURATION", 1, 20, 0, 0),
        ("STAKE", 0, 0, 0, 5),
    ],
    closest: &[
        ("Hundred Day", "72 of 100 days", 0.72),
        ("Churner", "6 movements per asset", 0.6),
        ("Round Trip", "3 of 5 round trips", 0.6),
    ],
};

/// One achievement's state, as a pip.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PipState {
    Earned,
    Open,
    Missed,
    Unread,
}

/// One pip: filled for earned, a ring for open, a bar for a one-way miss, a
/// faint ring for a source that could not answer. Four states, four shapes — a
/// colour alone would be unreadable to some readers and in a screenshot.
fn pip(cx: f32, cy: f32, step: f32, state: PipState, theme: &macroquad_widgets::Theme) {
    let c = theme.color;
    let r = (step * 0.28).clamp(2.5, 4.0);
    match state {
        PipState::Earned => draw_circle(cx, cy, r, c.accent),
        PipState::Open => draw_circle_lines(cx, cy, r, 1.2, c.text_secondary),
        PipState::Missed => draw_rectangle(cx - r, cy - 1.0, r * 2.0, 2.0, c.text_muted),
        PipState::Unread => draw_circle_lines(cx, cy, r, 1.0, with_alpha(c.text_muted, 0.45)),
    }
}

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
    /// A holder's achievements page: where they stand, the grid by axis, and
    /// what is closest to being earned.
    HolderProfile,
    /// The same page for a holder one core short of the guaranteed phase.
    HolderNearMiss,
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
            PageView::HolderProfile => self.holder_profile(p, phone, &HOLDS_EVERYTHING),
            PageView::HolderNearMiss => self.holder_profile(p, phone, &ONE_CORE_SHORT),
        }
    }

    /// The page, drawn at a phone's width. Every figure is an outcome of the
    /// 2026-10-31 replay for that holder, not a plausible-looking number.
    fn holder_profile(&self, p: &Painter, phone: Rect, holder: &Profile) {
        let c = p.theme.color;
        draw_rectangle(phone.x, phone.y, phone.w, phone.h, c.bg_primary);
        let x = phone.x + 14.0;
        let w = phone.w - 28.0;
        let xs = p.size(TextSize::Xs);
        let base = p.size(TextSize::Base);
        let lg = p.size(TextSize::Lg);
        let mut y = phone.y + 14.0;

        // Who this is. A handle where the holder has one; the credential is the
        // key the bundle is searched by, so it is shown either way.
        p.text_in(
            holder.handle.unwrap_or(holder.stake),
            Rect::new(x, y, w, 20.0),
            lg,
            c.text_primary,
            Align::Start,
        );
        p.text_in(
            "connected",
            Rect::new(x, y, w, 20.0),
            xs,
            c.success,
            Align::End,
        );
        if holder.handle.is_some() {
            p.text_in(
                holder.stake,
                Rect::new(x, y + 20.0, w, 14.0),
                xs,
                c.text_muted,
                Align::Start,
            );
        }
        y += 44.0;

        // The answer the page exists for: which phase this holder would get,
        // and the rule that decided it.
        let (phase, rule, rung) = match holder.rung {
            Rung::Gtd => (
                "GUARANTEED",
                format!(
                    "GTD needs {} core · you have {} core, {} flavour",
                    GTD_CORE, holder.core, holder.flavour
                ),
                0usize,
            ),
            Rung::Fcfs => (
                "FIRST COME",
                format!(
                    "one more core for GTD · you have {} core, {} flavour",
                    holder.core, holder.flavour
                ),
                1usize,
            ),
        };
        let band = Rect::new(x, y, w, 86.0);
        surface(p, band);
        p.text_in(
            phase,
            Rect::new(x + 12.0, y + 10.0, w - 24.0, 18.0),
            lg,
            c.accent,
            Align::Start,
        );
        p.text_in(
            &rule,
            Rect::new(x + 12.0, y + 29.0, w - 24.0, 14.0),
            xs,
            c.text_muted,
            Align::Start,
        );
        let ladder = Rect::new(x + 12.0, y + 54.0, w - 24.0, 6.0);
        let stop = (ladder.w - 8.0) / 3.0;
        for (i, label) in ["GTD", "FCFS", "PUBLIC"].into_iter().enumerate() {
            let seg = Rect::new(ladder.x + i as f32 * (stop + 4.0), ladder.y, stop, 6.0);
            draw_rounded_rect(seg.x, seg.y, seg.w, seg.h, 3.0, c.bg_primary);
            // The rungs are exclusive: a holder gets the phase they reached, so the
            // ladder marks the rung occupied rather than every rung below it.
            let reached = i == rung;
            if reached {
                draw_rounded_rect(seg.x, seg.y, seg.w, seg.h, 3.0, c.accent);
            }
            p.text_in(
                label,
                Rect::new(seg.x, seg.y + 10.0, seg.w, 12.0),
                xs,
                match reached {
                    true => c.text_primary,
                    false => c.text_muted,
                },
                Align::Start,
            );
        }
        y += 86.0 + 12.0;

        // Four figures, then the grid: one row per axis, one pip per
        // achievement, so every state reads in five lines rather than five screens.
        stat_tiles(
            p,
            &StatTilesVm {
                tiles: &[
                    StatTile {
                        label: "EARNED",
                        value: &format!("{}/{HOLDER_EVALUABLE}", holder.earned),
                        mark: None,
                    },
                    StatTile {
                        label: "CORE",
                        value: &format!("{}/{}", holder.core, holder.core_total),
                        mark: None,
                    },
                    StatTile {
                        label: "RAREST",
                        value: holder.rarest,
                        mark: None,
                    },
                    StatTile {
                        label: "OPEN",
                        value: &holder.open.to_string(),
                        mark: None,
                    },
                ],
            },
            Rect::new(x, y, w, 54.0),
        );
        y += 54.0 + 16.0;

        p.text_in(
            "BY AXIS",
            Rect::new(x, y, w, 12.0),
            xs,
            c.text_muted,
            Align::Start,
        );
        y += 16.0;
        for (axis, earned, open, missed, unread) in holder.axes {
            let row = Rect::new(x, y, w, 18.0);
            p.text_in(axis, row, xs, c.text_secondary, Align::Start);
            let pips_x = x + 86.0;
            let pip_w = w - 86.0 - 44.0;
            let step = pip_w / 21.0;
            let mut i = 0usize;
            for (count, state) in [
                (earned, PipState::Earned),
                (open, PipState::Open),
                (missed, PipState::Missed),
                (unread, PipState::Unread),
            ] {
                for _ in 0..*count {
                    let cx = pips_x + i as f32 * step + step * 0.5;
                    pip(cx, row.y + row.h * 0.5, step, state, &p.theme);
                    i += 1;
                }
            }
            let total = earned + open + missed + unread;
            p.text_in(
                &match unread == &total {
                    true => "not read".to_string(),
                    false => format!("{earned}/{total}"),
                },
                row,
                xs,
                c.text_muted,
                Align::End,
            );
            y += 18.0;
        }
        y += 12.0;

        // What one more achievement would take. The evaluator already says
        // `have/need` and, for a tenure, the date it lands — so this section is
        // a presentation of what it computed, not a second reading.
        p.text_in(
            "CLOSEST",
            Rect::new(x, y, w, 12.0),
            xs,
            c.text_muted,
            Align::Start,
        );
        y += 16.0;
        for (title, detail, fraction) in holder.closest {
            p.text_in(
                title,
                Rect::new(x, y, w * 0.55, 14.0),
                base,
                c.text_primary,
                Align::Start,
            );
            p.text_in(
                detail,
                Rect::new(x, y, w, 14.0),
                xs,
                c.text_muted,
                Align::End,
            );
            let bar = Rect::new(x, y + 17.0, w, 3.0);
            draw_rounded_rect(bar.x, bar.y, bar.w, bar.h, 1.5, c.bg_secondary);
            draw_rounded_rect(
                bar.x,
                bar.y,
                bar.w * fraction.clamp(0.0, 1.0),
                bar.h,
                1.5,
                c.accent,
            );
            y += 30.0;
        }
        y += 2.0;
        p.text_in(
            &format!(
                "{} open · {} missed · {} not measured on this source",
                holder.open, holder.missed, holder.unread
            ),
            Rect::new(x, y, w, 14.0),
            xs,
            c.text_muted,
            Align::Start,
        );
    }
}
