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
//! and the app hands the palette [`offered_rows`] instead of a literal list.
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
//! ## Commands that take an argument
//!
//! "Filter by trait" cannot fire on its own — it needs a trait. Declaring the
//! argument makes its palette row a [branch](crate::command_palette::RowKind):
//! choosing it descends into a context keyed by the command's id, the APP
//! supplies the candidates for that path (only the app knows the catalogue, the
//! vocabulary, the pane), and the chosen value comes back through
//! [`invoke_with`] to the widget that offered it — in the same call:
//!
//! ```ignore
//! if let Some(value) = Command::new("collection.filter", "Filter by trait")
//!     .group("Collection")
//!     .argument("trait value")
//!     .offer_with_argument(ui)
//! {
//!     state.apply_trait(value);
//! }
//! ```
//!
//! ⚠️ [`Command::argument`] returns an [`ArgumentCommand`], which has no
//! `offer`. On a command that needs a value, a `bool`-returning `offer` would
//! compile, report `true`, and throw the value away — a bug with no symptom at
//! the call site. Call `.argument()` last, after the other builders.
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

use crate::command_palette::{PaletteRow, RowKind};
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
    /// The value this command needs before it can fire, described. Private so
    /// the only way to set it is [`Command::argument`], which takes `offer`
    /// away — see [`ArgumentCommand`].
    argument: Option<String>,
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
            argument: None,
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

    /// This command needs a value — `describes` says what kind ("trait value").
    /// Its palette row becomes a branch. Call last; see the module header.
    pub fn argument(mut self, describes: impl Into<String>) -> ArgumentCommand {
        self.argument = Some(describes.into());
        ArgumentCommand(self)
    }

    /// The value this command asks for, if it asks for one.
    pub fn takes_argument(&self) -> Option<&str> {
        self.argument.as_deref()
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

/// A [`Command`] that needs a value before it can fire.
///
/// A distinct type so `offer` is not available on it: the only way to declare
/// one is the call that hands the value back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentCommand(Command);

impl ArgumentCommand {
    /// Declare this command and return the value it was invoked with this pass.
    ///
    /// `None` both when nothing invoked it and when something invoked it
    /// WITHOUT a value (a bare [`invoke`] from a toolbar button) — a command that
    /// needs a value cannot act on one it was not given, so that invocation
    /// expires unclaimed.
    pub fn offer_with_argument(self, ui: &Ui) -> Option<String> {
        let ctx = ui.ctx();
        let now = pass(ctx);
        let id = self.0.id.clone();
        ctx.data_mut(|d| {
            let reg: &mut Registry = d.get_temp_mut_or_default(registry_id());
            reg.record(self.0, now);
            reg.claim_argument(&id, now)
        })
    }
}

/// Fire a command by id — from a button, a shortcut, or the palette.
///
/// Naming one that nothing offers is harmless: it expires unclaimed.
pub fn invoke(ctx: &Context, id: impl Into<String>) {
    set_pending(ctx, id.into(), None);
}

/// Fire a command that takes an argument, with its value — what the app calls
/// when the palette returns a leaf chosen inside that command's context.
pub fn invoke_with(ctx: &Context, id: impl Into<String>, argument: impl Into<String>) {
    set_pending(ctx, id.into(), Some(argument.into()));
}

fn set_pending(ctx: &Context, id: String, argument: Option<String>) {
    let at = pass(ctx);
    ctx.data_mut(|d| {
        let reg: &mut Registry = d.get_temp_mut_or_default(registry_id());
        reg.pending = Some(Pending { id, argument, at });
    });
    // The claiming widget may already have drawn this pass, so the pass that
    // acts on this still has to happen.
    ctx.request_repaint();
}

/// Everything on screen can do, right now.
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

/// [`offered`], as palette rows: a command that takes an argument is a branch.
pub fn offered_rows(ctx: &Context) -> Vec<PaletteRow> {
    offered(ctx).into_iter().map(row_for).collect()
}

fn row_for(c: Command) -> PaletteRow {
    let kind = match c.argument {
        Some(_) => RowKind::Branch,
        None => RowKind::Leaf,
    };
    let mut option = TypeaheadOption::new(c.id, c.title);
    // The group rides in the subtitle rather than being dropped: the palette
    // is a flat list, and "Wallets · stake address or $handle" is what tells
    // two similarly-named commands apart.
    option.subtitle = match (c.group, c.hint) {
        (Some(g), Some(h)) => Some(format!("{g} · {h}")),
        (Some(g), None) => Some(g),
        (None, Some(h)) => Some(h),
        (None, None) => None,
    };
    PaletteRow { option, kind }
}

/// Whether anything currently offers this id — for an affordance that should
/// hide when the thing it triggers is not on screen.
pub fn is_offered(ctx: &Context, id: &str) -> bool {
    offered(ctx).iter().any(|c| c.id == id)
}

fn pass(ctx: &Context) -> u64 {
    ctx.cumulative_pass_nr()
}

/// An invocation waiting to be claimed.
#[derive(Clone)]
struct Pending {
    id: String,
    argument: Option<String>,
    /// The pass it was made in.
    at: u64,
}

#[derive(Clone, Default)]
struct Registry {
    /// Each command and the pass it was last offered in.
    offers: Vec<(Command, u64)>,
    pending: Option<Pending>,
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
            Some(p) if p.id == id => {
                self.pending = None;
                true
            }
            _ => {
                self.expire_pending(now);
                false
            }
        }
    }

    /// Claim an invocation of `id` that CARRIES a value. One without a value is
    /// left to expire: the command cannot act on it, and consuming it would
    /// hide that something asked.
    fn claim_argument(&mut self, id: &str, now: u64) -> Option<String> {
        match &self.pending {
            Some(Pending {
                id: pending,
                argument: Some(_),
                ..
            }) if pending == id => self.pending.take().and_then(|p| p.argument),
            _ => {
                self.expire_pending(now);
                None
            }
        }
    }

    fn expire_pending(&mut self, now: u64) {
        if let Some(p) = &self.pending
            && now.saturating_sub(p.at) > GRACE
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

    fn pending(id: &str, argument: Option<&str>, at: u64) -> Option<Pending> {
        Some(Pending {
            id: id.into(),
            argument: argument.map(Into::into),
            at,
        })
    }

    #[test]
    fn a_command_fires_once_and_only_for_its_own_id() {
        let mut reg = Registry::default();
        reg.record(cmd("wallet.add"), 0);
        reg.record(cmd("wallet.clear"), 0);
        reg.pending = pending("wallet.add", None, 0);

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
            pending: pending("wallet.add", None, 3),
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
        let row = row_for(
            Command::new("wallet.add", "Add wallet")
                .group("Wallets")
                .hint("stake address or $handle"),
        );
        assert_eq!(
            row.option.subtitle.as_deref(),
            Some("Wallets · stake address or $handle")
        );
    }

    #[test]
    fn a_command_that_takes_an_argument_lists_as_a_branch() {
        let plain = row_for(cmd("wallet.add"));
        let needs_value = row_for(
            Command::new("collection.filter", "Filter by trait")
                .argument("trait value")
                .0,
        );
        assert_eq!(plain.kind, RowKind::Leaf);
        assert_eq!(
            needs_value.kind,
            RowKind::Branch,
            "choosing it must ask for the value, not fire without one"
        );
    }

    #[test]
    fn an_argument_command_claims_only_an_invocation_that_carries_a_value() {
        let mut reg = Registry {
            pending: pending("collection.filter", None, 0),
            ..Default::default()
        };
        assert_eq!(
            reg.claim_argument("collection.filter", 0),
            None,
            "a bare invoke has no value to hand over"
        );
        assert!(
            reg.pending.is_some(),
            "and is left to expire, not silently consumed"
        );

        reg.pending = pending("collection.filter", Some("Eyes: Laser"), 0);
        assert_eq!(
            reg.claim_argument("collection.filter", 0).as_deref(),
            Some("Eyes: Laser")
        );
        assert!(reg.pending.is_none(), "claiming consumes it");
    }
}
