//! `commands` — widgets declare what they can do; the palette lists it.
//!
//! ## The problem
//!
//! [`CommandPalette`](crate::command_palette) takes a command list the **app**
//! builds. So the app shell has to know every capability of every widget below
//! it, and restate each one: the wallet roster knows perfectly well that it can
//! add a wallet, but "Add wallet" has to be written out again in whatever page
//! happens to own the palette, wired to a flag that reaches back down. Every new
//! widget is a second edit somewhere else, and a widget that is not on screen
//! keeps its command in the list unless someone remembers to filter it.
//!
//! Here the widget says it, once, where it is:
//!
//! ```ignore
//! if Command::new("wallet.add", "Add wallet")
//!     .hint("stake address or $handle")
//!     .group("Wallets")
//!     .offer(ui)
//! {
//!     state.adding = true;
//! }
//! ```
//!
//! and the app hands the palette [`offered`] instead of a literal list.
//!
//! ## Declare, then ask — because this is immediate mode
//!
//! A command cannot be a callback. A closure that could actually *do* the thing
//! would have to borrow the host's state, and it would have to outlive the frame
//! to sit in a registry — those two are not compatible. So [`Command::offer`]
//! does what every other control here does: it declares itself and returns
//! whether it fired, exactly like `Button::clicked()`. The widget that offers a
//! command is the widget that performs it, and it finds out in the same place
//! it would have handled a click.
//!
//! That also means the `+` button and the palette entry are not two paths to one
//! behaviour — they are one path. The button calls [`invoke`]; so does the app
//! when the palette returns an id. Neither knows about the other.
//!
//! ## On screen or not in the list
//!
//! Each offer is stamped with the pass it happened in, and [`offered`] returns
//! only what was offered in this pass or the last one. A widget that stops being
//! drawn drops out of the palette by itself, with no bookkeeping and nothing to
//! forget — which matters because a palette entry that cannot reach its widget
//! is worse than a missing one. It appears to work and silently does nothing.
//!
//! One pass of grace, not zero, because the palette is usually drawn *before*
//! the widgets it lists: at that moment the current pass has no offers in it yet.
//!
//! ## Ordering, and why it does not matter
//!
//! If the palette is drawn before the offering widget, the widget sees the
//! invocation later the same pass. If after, it sees it on the next one. Either
//! way it fires exactly once, because claiming it clears it. An invocation
//! nobody claims expires rather than lurking — otherwise navigating away and
//! back would run a command the reader asked for minutes ago.

use egui::{Context, Ui};

use crate::icons::PhosphorIcon;
use crate::typeahead_search::TypeaheadOption;

/// Passes an offer or an invocation stays live. See the module header.
const GRACE: u64 = 1;

fn registry_id() -> egui::Id {
    egui::Id::new("egui-widgets/commands")
}

/// Something a widget on screen can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// Stable across frames and unique within the app — it is what [`invoke`]
    /// names. Dotted scopes read well: `wallet.add`, `collection.refresh`.
    pub id: String,
    /// What the palette lists it as. Imperative: "Add wallet", not "Wallets".
    pub title: String,
    /// A second line — what it will ask for, or what it will do.
    pub hint: Option<String>,
    /// A section heading, for a palette that groups.
    pub group: Option<String>,
    pub icon: Option<PhosphorIcon>,
    /// An offered-but-disabled command still lists, greyed. A command that
    /// vanishes when unavailable makes the reader wonder what they misremembered
    /// — the same reason [`crate::option_group`] keeps disabled choices.
    pub enabled: bool,
}

impl Command {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            hint: None,
            group: None,
            icon: None,
            enabled: true,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Declare this command and report whether it fired this pass.
    ///
    /// Call every pass the offering widget draws — it is how the command stays
    /// in the palette, and how the widget learns it was chosen.
    pub fn offer(self, ui: &Ui) -> bool {
        let ctx = ui.ctx();
        let now = pass(ctx);
        let id = self.id.clone();
        ctx.data_mut(|d| {
            let reg: &mut Registry = d.get_temp_mut_or_default(registry_id());
            reg.record(self, now);
            reg.claim(&id, now)
        })
    }
}

/// Fire a command by id — from a button, a shortcut, or the palette.
///
/// Naming one that nothing offers is harmless: it expires unclaimed.
pub fn invoke(ctx: &Context, id: impl Into<String>) {
    let id = id.into();
    let now = pass(ctx);
    ctx.data_mut(|d| {
        let reg: &mut Registry = d.get_temp_mut_or_default(registry_id());
        reg.pending = Some((id, now));
    });
    // The claiming widget may already have drawn this pass, so the pass that
    // acts on this still has to happen.
    ctx.request_repaint();
}

/// Everything on screen can do, right now — for feeding the palette.
///
/// Sorted by group then title, so a list assembled from widgets scattered
/// across the tree does not come out in draw order.
pub fn offered(ctx: &Context) -> Vec<Command> {
    let now = pass(ctx);
    ctx.data_mut(|d| {
        let reg: &mut Registry = d.get_temp_mut_or_default(registry_id());
        reg.prune(now);
        let mut live: Vec<Command> = reg.offers.iter().map(|(c, _)| c.clone()).collect();
        live.sort_by(|a, b| {
            a.group
                .cmp(&b.group)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        live
    })
}

/// [`offered`], as rows the palette can render.
pub fn offered_options(ctx: &Context) -> Vec<TypeaheadOption> {
    offered(ctx)
        .into_iter()
        .map(|c| {
            let mut o = TypeaheadOption::new(c.id, c.title);
            // The group rides in the subtitle rather than being dropped: the
            // palette is a flat list, and "Wallets · stake address or $handle"
            // is what tells two similarly-named commands apart.
            o.subtitle = match (c.group, c.hint) {
                (Some(g), Some(h)) => Some(format!("{g} · {h}")),
                (Some(g), None) => Some(g),
                (None, Some(h)) => Some(h),
                (None, None) => None,
            };
            o
        })
        .collect()
}

/// Whether anything currently offers this id — for an affordance that should
/// hide when the thing it triggers is not on screen.
pub fn is_offered(ctx: &Context, id: &str) -> bool {
    offered(ctx).iter().any(|c| c.id == id)
}

fn pass(ctx: &Context) -> u64 {
    ctx.cumulative_pass_nr()
}

#[derive(Clone, Default)]
struct Registry {
    /// Each command and the pass it was last offered in.
    offers: Vec<(Command, u64)>,
    /// An invocation waiting to be claimed, and the pass it was made in.
    pending: Option<(String, u64)>,
}

impl Registry {
    fn record(&mut self, command: Command, now: u64) {
        match self.offers.iter_mut().find(|(c, _)| c.id == command.id) {
            // Replaced, not skipped: a command's title or enabled state can
            // change between passes and the palette must show the current one.
            Some(slot) => *slot = (command, now),
            None => self.offers.push((command, now)),
        }
    }

    fn claim(&mut self, id: &str, now: u64) -> bool {
        match &self.pending {
            Some((pending, _)) if pending == id => {
                self.pending = None;
                true
            }
            _ => {
                self.expire_pending(now);
                false
            }
        }
    }

    fn expire_pending(&mut self, now: u64) {
        if let Some((_, at)) = &self.pending
            && now.saturating_sub(*at) > GRACE
        {
            self.pending = None;
        }
    }

    fn prune(&mut self, now: u64) {
        self.offers
            .retain(|(_, at)| now.saturating_sub(*at) <= GRACE);
        self.expire_pending(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str) -> Command {
        Command::new(id, id)
    }

    #[test]
    fn a_command_fires_once_and_only_for_its_own_id() {
        let mut reg = Registry::default();
        reg.record(cmd("wallet.add"), 0);
        reg.record(cmd("wallet.clear"), 0);
        reg.pending = Some(("wallet.add".into(), 0));

        assert!(
            !reg.claim("wallet.clear", 0),
            "a different command must not"
        );
        assert!(reg.claim("wallet.add", 0), "the named one does");
        assert!(
            !reg.claim("wallet.add", 0),
            "and not twice — claiming consumes it"
        );
    }

    #[test]
    fn a_widget_that_stops_drawing_leaves_the_palette() {
        // The reason this is automatic rather than bookkept: a palette entry
        // that cannot reach its widget is worse than a missing one, because it
        // appears to work and silently does nothing.
        let mut reg = Registry::default();
        reg.record(cmd("wallet.add"), 10);
        reg.prune(10);
        assert_eq!(reg.offers.len(), 1, "the pass it was offered in");
        reg.prune(11);
        assert_eq!(reg.offers.len(), 1, "one pass of grace");
        reg.prune(12);
        assert!(reg.offers.is_empty(), "then it is gone");
    }

    #[test]
    fn the_grace_pass_exists_because_the_palette_draws_first() {
        // The palette is usually rendered above the widgets it lists, so at the
        // moment it asks, the current pass contains no offers yet. Zero grace
        // would make the list empty on every frame.
        let mut reg = Registry::default();
        reg.record(cmd("wallet.add"), 4);
        // Pass 5, palette drawn before the widget has re-offered:
        reg.prune(5);
        assert_eq!(reg.offers.len(), 1, "still listed while it re-offers");
    }

    #[test]
    fn an_unclaimed_invocation_expires_rather_than_lurking() {
        // Otherwise navigating away and back runs a command the reader asked
        // for minutes ago.
        let mut reg = Registry {
            pending: Some(("wallet.add".into(), 3)),
            ..Default::default()
        };
        reg.expire_pending(4);
        assert!(reg.pending.is_some(), "one pass of grace to be claimed");
        reg.expire_pending(5);
        assert!(reg.pending.is_none(), "then dropped");
    }

    #[test]
    fn re_offering_updates_rather_than_duplicates() {
        // A command's title and enabled state change between passes; the
        // palette has to show the current one, and exactly one of it.
        let mut reg = Registry::default();
        reg.record(Command::new("x", "Old title"), 0);
        reg.record(Command::new("x", "New title").enabled(false), 1);
        assert_eq!(reg.offers.len(), 1);
        assert_eq!(reg.offers[0].0.title, "New title");
        assert!(!reg.offers[0].0.enabled);
        assert_eq!(reg.offers[0].1, 1, "and the stamp moved forward");
    }

    #[test]
    fn a_group_and_a_hint_both_reach_the_palette_row() {
        // Two commands called "Refresh" are told apart by where they came from,
        // so the group cannot be dropped just because the palette is flat.
        let c = Command::new("wallet.add", "Add wallet")
            .group("Wallets")
            .hint("stake address or $handle");
        let mut reg = Registry::default();
        reg.record(c, 0);
        let listed = &reg.offers[0].0;
        assert_eq!(listed.group.as_deref(), Some("Wallets"));
        assert_eq!(listed.hint.as_deref(), Some("stake address or $handle"));
    }
}
