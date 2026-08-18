//! Squad picker — choose N assets from a roster to deploy on a mission.
//!
//! Two bands: the slots being filled, and a page of the roster to fill them
//! from. Tapping a candidate assigns it to the next free slot; tapping a filled
//! slot empties it.
//!
//! # Paginated, deliberately
//!
//! A player may hold hundreds of assets, which is exactly the "browse all N"
//! shape the charter names as *the* trap (README, "The four stop signs"). So
//! this pages through a fixed grid rather than scrolling a long one: no clip
//! rect, no virtualisation, constant draw cost per frame, and the page control
//! doubles as an honest signal of how much roster there is. If a surface ever
//! genuinely needs the scrolling version, it belongs in HTML, not here.
//!
//! # Stateless
//!
//! Selection, page and in-flight status all live in the host's `Vm`. This
//! widget only draws and reports taps — the host decides what a tap means,
//! which is what lets the same picker back a solo squad and a shared one.

use macroquad::prelude::*;

use crate::button::{Button, ButtonVariant};
use crate::painter::{draw_rounded_rect, Painter};
use crate::theme::with_alpha;

/// One asset that could be deployed.
#[derive(Clone)]
pub struct SquadCandidate {
    /// Stable handle — what actions carry, and what the host commits.
    pub id: String,
    /// Display name, e.g. `Toolhead #0220`.
    pub name: String,
    /// Primary role label, or `None` when the asset matched none.
    pub role: Option<String>,
    /// Contribution to squad power.
    pub power: i32,
    /// Unavailable to pick — already deployed elsewhere, injured, leased out.
    /// Drawn dimmed with the reason, rather than hidden: a player looking for
    /// a specific asset should find out *why* they can't use it.
    pub unavailable: Option<String>,
}

impl SquadCandidate {
    pub fn new(id: impl Into<String>, name: impl Into<String>, power: i32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role: None,
            power,
            unavailable: None,
        }
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    pub fn unavailable(mut self, why: impl Into<String>) -> Self {
        self.unavailable = Some(why.into());
        self
    }
}

/// Where the commit has got to, so the bar can say so.
#[derive(Clone, PartialEq)]
pub enum SquadCommit {
    /// Nothing in flight; the button is live.
    Idle,
    /// A commit is in flight — everything is disabled so a second tap can't
    /// race the first.
    Sending,
    /// Committed. Carries the line to show, e.g. "deployed: A, B, C".
    Done(String),
    Failed(String),
}

pub struct SquadPickerVm {
    pub candidates: Vec<SquadCandidate>,
    /// Chosen ids **in slot order** — slot order is player-visible, because a
    /// story addresses members by slot (`member_1`, `member_2`, …).
    pub chosen: Vec<String>,
    pub max_slots: usize,
    /// Zero-based page of the roster grid.
    pub page: usize,
    /// False once the squad is locked for the run; everything goes read-only.
    pub editable: bool,
    pub commit: SquadCommit,
    /// Label for the commit button — the verb differs by surface ("Deploy
    /// squad", "Send crew"), and the widget shouldn't presume.
    pub commit_label: String,
}

impl SquadPickerVm {
    pub fn new(candidates: Vec<SquadCandidate>, max_slots: usize) -> Self {
        Self {
            candidates,
            chosen: Vec::new(),
            max_slots,
            page: 0,
            editable: true,
            commit: SquadCommit::Idle,
            commit_label: "Deploy squad".to_string(),
        }
    }

    pub fn chosen(mut self, ids: Vec<String>) -> Self {
        self.chosen = ids;
        self
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    pub fn commit(mut self, commit: SquadCommit) -> Self {
        self.commit = commit;
        self
    }

    pub fn commit_label(mut self, label: impl Into<String>) -> Self {
        self.commit_label = label.into();
        self
    }

    fn is_chosen(&self, id: &str) -> bool {
        self.chosen.iter().any(|c| c == id)
    }

    fn candidate(&self, id: &str) -> Option<&SquadCandidate> {
        self.candidates.iter().find(|c| c.id == id)
    }

    pub fn power(&self) -> i32 {
        self.chosen
            .iter()
            .filter_map(|id| self.candidate(id))
            .map(|c| c.power)
            .sum()
    }

    /// Distinct roles across the chosen squad, first-seen order.
    ///
    /// Worth surfacing because role coverage is what missions branch on — four
    /// high-power assets sharing one role fail checks a weaker, broader squad
    /// passes, and nothing else on screen would say so.
    pub fn roles(&self) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for id in &self.chosen {
            if let Some(role) = self.candidate(id).and_then(|c| c.role.as_deref()) {
                if !seen.contains(&role) {
                    seen.push(role);
                }
            }
        }
        seen
    }

    /// Pages of roster at this width.
    ///
    /// Takes the width because the column count does — an asset name is the
    /// one thing on a card that must stay readable, so a narrow surface gets
    /// fewer, wider cards rather than four cramped ones. The host needs this
    /// to clamp `page` when the roster changes under it.
    pub fn pages(&self, w: f32) -> usize {
        self.candidates.len().div_ceil(per_page(w)).max(1)
    }
}

/// Columns that fit at this width without squeezing names to ellipses.
///
/// One division, not a layout engine — the charter's stop sign is auto-sizing
/// and reflow, and this is neither: a fixed grid whose column count is chosen
/// once from the width it was handed.
fn columns_for(w: f32) -> usize {
    (((w + GAP) / (MIN_CARD_W + GAP)).floor() as usize).clamp(1, 4)
}

fn per_page(w: f32) -> usize {
    columns_for(w) * ROWS
}

pub enum SquadPickerAction {
    /// Add or remove this asset. Carries the id rather than an index because
    /// the roster can be re-sorted between render and dispatch.
    Toggle(String),
    /// Show this page of the roster.
    Page(usize),
    Commit,
}

pub struct SquadPickerResponse {
    pub bottom: f32,
    pub action: Option<SquadPickerAction>,
}

/// Narrowest a roster card may be. Below this an asset name — the one thing a
/// player scans for — collapses to an ellipsis, so the grid drops a column
/// instead.
const MIN_CARD_W: f32 = 150.0;
const ROWS: usize = 3;
const SLOT_H: f32 = 84.0;
const CARD_H: f32 = 68.0;
const GAP: f32 = 10.0;

pub fn squad_picker(
    p: &Painter,
    vm: &SquadPickerVm,
    x: f32,
    mut y: f32,
    w: f32,
) -> SquadPickerResponse {
    let t = p.theme;
    let mut action = None;
    // Everything is inert while a commit is in flight, and after it lands.
    let live = vm.editable && vm.commit == SquadCommit::Idle;

    // ── slots ───────────────────────────────────────────────────────────
    p.text_top("Squad", x, y, 16.0, t.fg);
    let count = format!("{} / {}", vm.chosen.len(), vm.max_slots);
    let cw = p.measure(&count, 16.0).width;
    p.mono(
        &count,
        x + w - cw,
        p.top_baseline(y, 16.0),
        16.0,
        if vm.chosen.len() == vm.max_slots {
            t.accent
        } else {
            t.muted
        },
    );
    y += 24.0;

    let slot_w = (w - GAP * (vm.max_slots as f32 - 1.0)) / vm.max_slots as f32;
    for slot in 0..vm.max_slots {
        let r = Rect::new(x + (slot_w + GAP) * slot as f32, y, slot_w, SLOT_H);
        match vm.chosen.get(slot).and_then(|id| vm.candidate(id)) {
            Some(c) => {
                let hit = p.interact(r, live);
                let fill = if hit.hover {
                    with_alpha(t.danger, 0.14)
                } else {
                    with_alpha(t.accent, 0.12)
                };
                draw_rounded_rect(r.x, r.y, r.w, r.h, 8.0, fill);
                draw_rounded_rect(r.x, r.y, r.w, 2.0, 1.0, t.accent);

                p.mono(
                    &format!("{}", slot + 1),
                    r.x + 10.0,
                    p.top_baseline(r.y + 10.0, 12.0),
                    12.0,
                    t.accent,
                );
                let pw = p.measure(&format!("{}", c.power), 13.0).width;
                p.mono(
                    &format!("{}", c.power),
                    r.x + r.w - 10.0 - pw,
                    p.top_baseline(r.y + 10.0, 13.0),
                    13.0,
                    t.muted,
                );
                p.text_top(
                    &fit_name(p, &c.name, r.w - 20.0, 15.0),
                    r.x + 10.0,
                    r.y + 30.0,
                    15.0,
                    t.fg,
                );
                p.text_top(
                    // On hover the sub-line becomes the affordance — it says
                    // what a tap will do, only while a tap would do it.
                    &if hit.hover {
                        "tap to remove".to_string()
                    } else {
                        clip(p, c.role.as_deref().unwrap_or("no role"), r.w - 20.0, 13.0)
                    },
                    r.x + 10.0,
                    r.y + 50.0,
                    13.0,
                    if hit.hover { t.danger } else { t.muted },
                );

                if hit.clicked {
                    action = Some(SquadPickerAction::Toggle(c.id.clone()));
                }
            }
            None => {
                draw_rounded_rect(r.x, r.y, r.w, r.h, 8.0, with_alpha(t.panel, 0.6));
                p.mono(
                    &format!("{}", slot + 1),
                    r.x + 10.0,
                    p.top_baseline(r.y + 10.0, 12.0),
                    12.0,
                    with_alpha(t.muted, 0.5),
                );
                let label = "empty";
                let lw = p.measure(label, 14.0).width;
                p.text(
                    label,
                    r.x + (r.w - lw) / 2.0,
                    p.centre_baseline(r.y, r.h, 14.0),
                    14.0,
                    with_alpha(t.muted, 0.7),
                );
            }
        }
    }
    y += SLOT_H + 10.0;

    // Power and role coverage.
    let roles = vm.roles();
    let summary = if roles.is_empty() {
        format!("power {}", vm.power())
    } else {
        format!("power {} · {}", vm.power(), roles.join(", "))
    };
    p.text_top(&summary, x, y, 13.0, t.muted);
    y += 26.0;

    // ── roster ──────────────────────────────────────────────────────────
    p.text_top(&format!("Roster · {}", vm.candidates.len()), x, y, 16.0, t.fg);

    let pages = vm.pages(w);
    if pages > 1 {
        // Pager sits on the roster heading line: it belongs to the grid, and a
        // separate row would push the grid down for no gain.
        let bw = 30.0;
        let label = format!("{} / {}", vm.page + 1, pages);
        let lw = p.measure(&label, 14.0).width;
        let next_x = x + w - bw;
        let label_x = next_x - GAP - lw;
        let prev_x = label_x - GAP - bw;

        if Button::new("‹")
            .variant(ButtonVariant::Ghost)
            .enabled(live && vm.page > 0)
            .show(p, Rect::new(prev_x, y - 6.0, bw, 28.0))
        {
            action = Some(SquadPickerAction::Page(vm.page.saturating_sub(1)));
        }
        p.mono(&label, label_x, p.top_baseline(y, 14.0), 14.0, t.muted);
        if Button::new("›")
            .variant(ButtonVariant::Ghost)
            .enabled(live && vm.page + 1 < pages)
            .show(p, Rect::new(next_x, y - 6.0, bw, 28.0))
        {
            action = Some(SquadPickerAction::Page(vm.page + 1));
        }
    }
    y += 24.0;

    if vm.candidates.is_empty() {
        p.text_top(
            "Nothing eligible — link a wallet holding this collection.",
            x,
            y,
            14.0,
            t.muted,
        );
        y += 30.0;
    } else {
        let cols = columns_for(w);
        let per = cols * ROWS;
        let card_w = (w - GAP * (cols as f32 - 1.0)) / cols as f32;
        let start = vm.page * per;
        let full = vm.chosen.len() >= vm.max_slots;

        for cell in 0..per {
            let Some(c) = vm.candidates.get(start + cell) else {
                break;
            };
            let col = cell % cols;
            let row = cell / cols;
            let r = Rect::new(
                x + col as f32 * (card_w + GAP),
                y + row as f32 * (CARD_H + GAP),
                card_w,
                CARD_H,
            );

            let picked = vm.is_chosen(&c.id);
            let blocked = c.unavailable.is_some();
            // A picked card stays tappable when full — that's how you swap.
            let enabled = live && !blocked && (!full || picked);
            let hit = p.interact(r, enabled);

            let fill = if picked {
                with_alpha(t.accent, 0.16)
            } else if hit.hover {
                with_alpha(t.fg, 0.08)
            } else {
                with_alpha(t.panel, 0.6)
            };
            draw_rounded_rect(r.x, r.y, r.w, r.h, 8.0, fill);
            if picked {
                draw_rounded_rect(r.x, r.y, 3.0, r.h, 1.5, t.accent);
            }

            let dim = blocked || (full && !picked);
            let ink = if dim { with_alpha(t.fg, 0.45) } else { t.fg };

            let pw = p.measure(&format!("{}", c.power), 12.0).width;
            p.text_top(
                &fit_name(p, &c.name, r.w - 20.0 - pw - 6.0, 14.0),
                r.x + 10.0,
                r.y + 10.0,
                14.0,
                ink,
            );
            p.mono(
                &format!("{}", c.power),
                r.x + r.w - 10.0 - pw,
                p.top_baseline(r.y + 10.0, 12.0),
                12.0,
                if picked { t.accent } else { with_alpha(t.muted, 0.9) },
            );

            let sub = match (&c.unavailable, picked) {
                (Some(why), _) => why.clone(),
                (None, true) => "deployed".to_string(),
                (None, false) => c.role.clone().unwrap_or_else(|| "no role".to_string()),
            };
            p.text_top(
                &clip(p, &sub, r.w - 20.0, 12.0),
                r.x + 10.0,
                r.y + 32.0,
                12.0,
                match (&c.unavailable, picked) {
                    (Some(_), _) => t.warn,
                    (None, true) => t.accent,
                    (None, false) => with_alpha(t.muted, if dim { 0.6 } else { 1.0 }),
                },
            );

            if hit.clicked {
                action = Some(SquadPickerAction::Toggle(c.id.clone()));
            }
        }

        // Reserve the full grid height regardless of how many cards this page
        // holds, so a short last page doesn't make the commit bar jump.
        y += ROWS as f32 * (CARD_H + GAP);
    }

    y += 6.0;

    // ── commit ──────────────────────────────────────────────────────────
    let (status, colour) = match &vm.commit {
        _ if !vm.editable => ("Squad is locked for this run.".to_string(), t.muted),
        SquadCommit::Idle if vm.chosen.is_empty() => {
            ("Pick at least one.".to_string(), t.muted)
        }
        SquadCommit::Idle => (String::new(), t.muted),
        SquadCommit::Sending => ("Deploying…".to_string(), t.muted),
        SquadCommit::Done(msg) => (msg.clone(), t.accent),
        SquadCommit::Failed(why) => (format!("Failed: {why}"), t.danger),
    };

    let btn_w = 150.0;
    let btn = Rect::new(x + w - btn_w, y, btn_w, 40.0);
    if !status.is_empty() {
        p.text_top(
            &clip(p, &status, w - btn_w - GAP * 2.0, 13.0),
            x,
            y + 13.0,
            13.0,
            colour,
        );
    }
    if Button::new(&vm.commit_label)
        .enabled(live && !vm.chosen.is_empty())
        .show(p, btn)
    {
        action = Some(SquadPickerAction::Commit);
    }
    y += 40.0;

    SquadPickerResponse { bottom: y, action }
}

/// Fit a name into `max_w`, keeping the part that identifies it.
///
/// Collection names are overwhelmingly `<common prefix> #<number>` — "Toolhead
/// #0220". Tail-truncating that yields "Toolhe…", which throws away the only
/// part that distinguishes one asset from another and leaves four identical
/// slots. So when the full name won't fit, fall back to a trailing *serial*
/// ("#0220").
///
/// Only a serial, though: the tail of "Toolhead #0777 With A Very Long Name"
/// is "Name", which identifies nothing. When the last token doesn't look like
/// a number, an ellipsis at least preserves the leading characters someone can
/// scan against.
fn fit_name(p: &Painter, s: &str, max_w: f32, size: f32) -> String {
    if p.measure(s, size).width <= max_w {
        return s.to_string();
    }
    if let Some((_, tail)) = s.rsplit_once(char::is_whitespace) {
        if is_serial(tail) && p.measure(tail, size).width <= max_w {
            return tail.to_string();
        }
    }
    clip(p, s, max_w, size)
}

/// Does this token look like an asset's serial — `#0220`, `0220`, `1104`?
fn is_serial(token: &str) -> bool {
    let digits = token.strip_prefix('#').unwrap_or(token);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// Truncate with an ellipsis to fit `max_w`.
///
/// Asset names are user-supplied and unbounded; one that overflows would paint
/// over its neighbour, and a mid-glyph clip reads as a rendering bug rather
/// than a long name. Character-wise so a multi-byte name is never cut
/// mid-codepoint.
fn clip(p: &Painter, s: &str, max_w: f32, size: f32) -> String {
    if p.measure(s, size).width <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if p.measure(&format!("{probe}…"), size).width > max_w {
            break;
        }
        out = probe;
    }
    format!("{out}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> SquadPickerVm {
        SquadPickerVm::new(
            (0..10)
                .map(|i| {
                    SquadCandidate::new(format!("a{i}"), format!("Asset #{i}"), 100 - i as i32)
                        .with_role(if i % 2 == 0 { "mechanic" } else { "medic" })
                })
                .collect(),
            4,
        )
    }

    /// Wide enough for the full four columns (4 × 150 + 3 × 10).
    const WIDE: f32 = 660.0;
    /// Two columns' worth.
    const NARROW: f32 = 320.0;

    #[test]
    fn columns_shrink_with_width_but_never_to_zero() {
        assert_eq!(columns_for(WIDE), 4);
        assert_eq!(columns_for(NARROW), 2);
        // A surface narrower than one card still gets one column rather than a
        // division by zero.
        assert_eq!(columns_for(40.0), 1);
        // And never more than four, however wide — a fifth column would make
        // cards wider than they need to be and the grid harder to scan.
        assert_eq!(columns_for(4000.0), 4);
    }

    #[test]
    fn pages_cover_every_candidate() {
        // 10 candidates, 12 per page at four columns — one page.
        assert_eq!(vm().pages(WIDE), 1);
        // The same roster on a narrow surface pages, because fewer fit.
        assert_eq!(vm().pages(NARROW), 2);
        // An empty roster still has one (empty) page, so the pager has
        // something coherent to render.
        assert_eq!(SquadPickerVm::new(Vec::new(), 4).pages(WIDE), 1);
    }

    #[test]
    fn a_full_last_page_does_not_add_an_empty_one() {
        // Exactly 12 candidates at 12 per page is one page. An off-by-one here
        // shows the player an empty page and reads as a loading failure.
        let exact = SquadPickerVm::new(
            (0..12)
                .map(|i| SquadCandidate::new(format!("a{i}"), format!("Asset #{i}"), 1))
                .collect(),
            4,
        );
        assert_eq!(exact.pages(WIDE), 1);
    }

    #[test]
    fn power_and_roles_come_from_the_chosen_only() {
        let vm = vm().chosen(vec!["a0".into(), "a1".into()]);
        assert_eq!(vm.power(), 100 + 99);
        assert_eq!(vm.roles(), vec!["mechanic", "medic"]);
    }

    #[test]
    fn duplicate_roles_are_reported_once() {
        // Coverage is a set — two mechanics is one role covered, and a squad
        // summary claiming "mechanic, mechanic" would misread as breadth.
        let vm = vm().chosen(vec!["a0".into(), "a2".into()]);
        assert_eq!(vm.roles(), vec!["mechanic"]);
    }

    #[test]
    fn only_a_trailing_serial_counts_as_the_identifying_part() {
        // The cases that make a narrow slot readable...
        assert!(is_serial("#0220"));
        assert!(is_serial("1104"));
        // ...and the ones that would throw away the identity instead. "Name"
        // as a slot label is indistinguishable from any other long name.
        assert!(!is_serial("Name"));
        assert!(!is_serial("#"));
        assert!(!is_serial(""));
        assert!(!is_serial("#12a"));
    }

    #[test]
    fn an_unknown_chosen_id_is_ignored_not_counted() {
        // The host may hold a stale selection after the roster refreshes; that
        // must not inflate power or panic.
        let vm = vm().chosen(vec!["a0".into(), "gone".into()]);
        assert_eq!(vm.power(), 100);
        assert_eq!(vm.roles(), vec!["mechanic"]);
    }
}
