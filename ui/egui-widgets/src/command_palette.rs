//! `CommandPalette` — a keyboard-first launcher over a stack of contexts the caller resolves each frame.
//!
//! ⌘K opens a centred overlay. At the root it lists what the app can do right
//! now. Choosing a [`RowKind::Branch`] row descends into a context — pick a
//! collection, pick a room, pick a trait value — and choosing a
//! [`RowKind::Leaf`] dispatches and closes. Escape climbs one level and only
//! closes at the root.
//!
//! ## A stack, not a tree
//!
//! The palette holds only the path you are on, root to here. The tree is never
//! materialised: it is implied, frame by frame, by the rows the caller supplies
//! for the current path. Registering the shape up front is exactly what cannot
//! work for 27,000 collections, or for a pane that has not answered yet.
//!
//! ## The caller answers one question per frame
//!
//! Given [`PaletteState::path`] and [`PaletteState::query`], what are the rows?
//!
//! ```ignore
//! let rows = match palette.path().as_slice() {
//!     [] => rank_rows(&root_rows, palette.query(), 12),
//!     ["goto.collection"] => catalogue.search(palette.query(), 12),
//!     ["nostr.rooms", room] => members_of(room, palette.query()),
//!     _ => Vec::new(),
//! };
//! match CommandPalette::new("app", &rows).show(ui, &mut palette) {
//!     PaletteAction::Invoke { id, path } => { /* act on id, in path */ }
//!     PaletteAction::None => {}
//! }
//! ```
//!
//! ⚠️ **Match the WHOLE path, never its last element.** A level means nothing
//! without the levels above it — a member is a member OF a room. A descent is
//! keyed by the id of the row that was chosen, so the path is the chain of
//! selections and carries the room without a separate payload. A single
//! "current context" would force the caller to keep a shadow copy of the path
//! just to interpret what it was asked.
//!
//! ## The palette does not rank
//!
//! Rows are shown in the order given. [`rank_rows`] is the default for a small
//! in-memory set; a caller with its own ordering (market activity, recency)
//! passes its result through untouched. An earlier version re-filtered
//! whatever it was handed, which silently discarded any ranking the caller
//! had already done.
//!
//! Design: `cnft.dev-workers/docs/design/COMMAND_PALETTE_CONTEXT_STACK.md`.

use egui::{Align2, Event, Key, KeyboardShortcut, Modifiers, RichText, Ui, Vec2};

use crate::machine::Machine;
use crate::theme::{Space, SpaceExt, ThemeExt};
use crate::typeahead_search::{TypeaheadOption, TypeaheadSearch, rank_indices};
use crate::{Chip, PhosphorIcon};

/// Overlay width. Pinned rather than proportional so the palette lands in the
/// same place at every viewport size — a launcher people reach for by muscle
/// memory should not move.
const PALETTE_WIDTH: f32 = 460.0;

/// Where a descent went: the id of the row chosen to enter it.
///
/// Same namespace as row ids by construction — which is what lets the path
/// double as the chain of selections.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContextId(String);

impl ContextId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ContextId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ContextId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl std::fmt::Display for ContextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Open or closed — and nothing about depth.
///
/// A descent needs fresh entry framing just as opening does (so the Enter that
/// descended cannot also act in the level it opened), but
/// [`Machine::transition`] is unconditional: re-entering `Open` resets the
/// framing by itself. Carrying the depth here would be a second copy of
/// `stack.len()` for nothing to read.
enum PalettePhase {
    Closed,
    Open,
}

/// One level of the stack.
struct Frame {
    /// What was chosen to enter this level. `None` only at the root.
    entered_by: Option<ContextId>,
    /// Breadcrumb label.
    title: Option<String>,
    /// ⚠️ PER LEVEL, not per palette: climbing back restores what was typed
    /// up there. A single query string cannot.
    query: String,
    highlight: usize,
}

impl Frame {
    fn root() -> Self {
        Self {
            entered_by: None,
            title: None,
            query: String::new(),
            highlight: 0,
        }
    }

    fn descent(context: ContextId, title: String) -> Self {
        Self {
            entered_by: Some(context),
            title: Some(title),
            query: String::new(),
            highlight: 0,
        }
    }

    fn label(&self) -> &str {
        self.title
            .as_deref()
            .or_else(|| self.entered_by.as_ref().map(ContextId::as_str))
            .unwrap_or_default()
    }
}

/// Caller-owned palette state, persisted across frames.
///
/// Open/closed rides a [`Machine`], whose entry framing suppresses dismissal on
/// the frame the palette opened — the click that OPENED it is still in that
/// frame's input and lands outside the not-yet-rendered window, which
/// otherwise read as an instant self-dismiss ("the palette never appeared").
pub struct PaletteState {
    phase: Machine<PalettePhase>,
    /// Root plus one frame per descent. NEVER EMPTY — `stack[0]` is the root.
    stack: Vec<Frame>,
}

impl Default for PaletteState {
    fn default() -> Self {
        Self {
            phase: Machine::new(PalettePhase::Closed),
            stack: vec![Frame::root()],
        }
    }
}

impl PaletteState {
    /// Open at the root with an empty query. To open straight into a context
    /// ("+ action" opening the action picker), `open()` then [`Self::push`].
    pub fn open(&mut self) {
        self.stack.truncate(1);
        self.stack[0] = Frame::root();
        self.phase.transition(PalettePhase::Open);
    }

    pub fn close(&mut self) {
        self.phase.transition(PalettePhase::Closed);
    }

    pub fn is_open(&self) -> bool {
        matches!(self.phase.get(), PalettePhase::Open)
    }

    /// Every selection made to get here, root first. Empty at the root.
    ///
    /// `&str` rather than [`ContextId`] so the caller can match slice patterns
    /// against literals — `["nostr.rooms", room] => …` — which is the shape
    /// resolving rows should take. ⚠️ Match the whole path, never its last
    /// element; see the module header.
    pub fn path(&self) -> Vec<&str> {
        self.stack
            .iter()
            .filter_map(|f| f.entered_by.as_ref())
            .map(ContextId::as_str)
            .collect()
    }

    /// The query typed at the current level.
    pub fn query(&self) -> &str {
        &self.top().query
    }

    /// Descents from the root. 0 at the root.
    pub fn depth(&self) -> usize {
        self.stack.len() - 1
    }

    /// Descend into `context`, labelled `title` in the breadcrumb.
    pub fn push(&mut self, context: impl Into<ContextId>, title: impl Into<String>) {
        self.stack
            .push(Frame::descent(context.into(), title.into()));
        self.reenter();
    }

    /// Climb one level. `false` at the root, which cannot be popped.
    pub fn pop(&mut self) -> bool {
        if self.stack.len() == 1 {
            return false;
        }
        self.stack.pop();
        self.reenter();
        true
    }

    /// Jump back to the ancestor at `depth` — a breadcrumb click. A no-op for a
    /// depth at or below the current one.
    pub fn truncate_to(&mut self, depth: usize) {
        if depth + 1 < self.stack.len() {
            self.stack.truncate(depth + 1);
            self.reenter();
        }
    }

    fn path_owned(&self) -> Vec<ContextId> {
        self.stack
            .iter()
            .filter_map(|f| f.entered_by.clone())
            .collect()
    }

    fn top(&self) -> &Frame {
        self.stack.last().expect("the stack always holds the root")
    }

    fn top_mut(&mut self) -> &mut Frame {
        self.stack
            .last_mut()
            .expect("the stack always holds the root")
    }

    /// A level change is an entry, the same as opening. Only while open —
    /// preparing the stack of a closed palette must not open it.
    fn reenter(&mut self) {
        if self.is_open() {
            self.phase.transition(PalettePhase::Open);
        }
    }
}

/// One row: what to show, and what choosing it does.
#[derive(Clone)]
pub struct PaletteRow {
    pub option: TypeaheadOption,
    pub kind: RowKind,
}

impl PaletteRow {
    pub fn leaf(option: TypeaheadOption) -> Self {
        Self {
            option,
            kind: RowKind::Leaf,
        }
    }

    pub fn branch(option: TypeaheadOption) -> Self {
        Self {
            option,
            kind: RowKind::Branch,
        }
    }
}

/// What choosing a row does. A named decision, not a flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// [`PaletteAction::Invoke`], and the palette closes.
    Leaf,
    /// Descend into a context keyed by this row's own id.
    Branch,
}

/// The default ranking for a small in-memory row set: an empty query browses
/// the first `limit` rows in the order given, otherwise
/// [`crate::filter_options`]' tiers. Kinds travel with their rows.
pub fn rank_rows(rows: &[PaletteRow], query: &str, limit: usize) -> Vec<PaletteRow> {
    if query.trim().is_empty() {
        return rows.iter().take(limit).cloned().collect();
    }
    rank_indices(rows.iter().map(|r| &r.option), query, limit)
        .into_iter()
        .map(|i| rows[i].clone())
        .collect()
}

/// What the palette did this frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PaletteAction {
    None,
    /// A leaf was chosen. `path` is where the reader was; `id` is what they
    /// chose there. The palette has already closed.
    Invoke {
        id: String,
        path: Vec<ContextId>,
    },
}

/// The command palette overlay.
pub struct CommandPalette<'a> {
    id_salt: &'a str,
    rows: &'a [PaletteRow],
    placeholder: Option<&'a str>,
    root_title: &'a str,
    /// Rows visible before the list scrolls.
    limit: usize,
    /// Listen for ⌘K / Ctrl-K to open. On by default; disable when an app
    /// binds the chord itself.
    keybinding: bool,
}

impl<'a> CommandPalette<'a> {
    /// `rows` are for the path as it stands THIS frame — resolve them from
    /// [`PaletteState::path`] and [`PaletteState::query`] just before calling.
    pub fn new(id_salt: &'a str, rows: &'a [PaletteRow]) -> Self {
        Self {
            id_salt,
            rows,
            placeholder: None,
            root_title: "All",
            limit: 12,
            keybinding: true,
        }
    }

    /// Placeholder for the search field. Set it per path, in the same place
    /// the rows are chosen; unset, it is generic.
    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = Some(text);
        self
    }

    /// The breadcrumb's root chip.
    pub fn root_title(mut self, text: &'a str) -> Self {
        self.root_title = text;
        self
    }

    /// Rows visible before the list scrolls. The palette never truncates —
    /// how many rows to supply is the caller's ranking decision.
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    pub fn keybinding(mut self, enabled: bool) -> Self {
        self.keybinding = enabled;
        self
    }

    /// Handle the keys and render the overlay when open. Call once per frame
    /// from the app root — the overlay floats above everything via an egui
    /// `Window`.
    ///
    /// | key | at the root | deeper |
    /// |---|---|---|
    /// | ⌘K | toggle | close entirely |
    /// | Escape | close | climb one level |
    /// | Backspace, empty query | nothing | climb one level |
    /// | Enter on a branch | descend | descend |
    /// | Enter on a leaf | invoke, close | invoke, close |
    /// | click a breadcrumb | — | jump there |
    pub fn show(self, ui: &mut Ui, state: &mut PaletteState) -> PaletteAction {
        let ctx = ui.ctx().clone();

        if self.keybinding {
            let chord = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
            if ctx.input_mut(|i| i.consume_shortcut(&chord)) {
                if state.is_open() {
                    state.close();
                } else {
                    state.open();
                }
            }
        }
        if !state.is_open() {
            state.phase.tick();
            return PaletteAction::None;
        }
        // Entry frame = the frame the palette opened or changed level (show
        // ticks at exit, so this holds however early in the frame it happened).
        let just_entered = state.phase.entered();

        // The rows were resolved for the path as it stood when the caller built
        // them. Climbing before render invalidates them, and drawing them under
        // the new breadcrumb for a frame would show a level's candidates beneath
        // its parent's name — so the frame that climbs draws no rows, and the
        // repaint brings the right ones.
        let mut rows = self.rows;
        if !just_entered {
            if fresh_press(&ctx, Key::Escape) {
                if !state.pop() {
                    state.close();
                    state.phase.tick();
                    return PaletteAction::None;
                }
                rows = &[];
                ctx.request_repaint();
            } else if state.depth() > 0
                && state.query().is_empty()
                && fresh_press(&ctx, Key::Backspace)
            {
                // Checked BEFORE the field sees the key: a Backspace that
                // deletes the last character leaves an empty query, and must
                // not also climb.
                state.pop();
                rows = &[];
                ctx.request_repaint();
            }
        }

        let depth = state.depth();
        let placeholder = self.placeholder.unwrap_or(match depth {
            0 => "Type a command…",
            _ => "Search…",
        });
        let crumbs: Vec<String> = state
            .stack
            .iter()
            .skip(1)
            .map(|f| f.label().to_owned())
            .collect();
        // One field id per path. Autofocus fires on a field's APPEARANCE, so a
        // single stable id would leave the caret on a field whose level was just
        // replaced — every descent and climb would end with the reader clicking
        // back into the box.
        let edit_salt = std::iter::once(self.id_salt)
            .chain(state.path())
            .collect::<Vec<_>>()
            .join("/");
        let shown: Vec<TypeaheadOption> = rows.iter().map(displayed).collect();

        let accent = ui.tokens().color.accent;
        let muted = ui.tokens().color.text_muted;
        let root_title = self.root_title;
        let limit = self.limit;

        let mut chosen: Option<String> = None;
        let mut jump: Option<usize> = None;
        let mut dismissed = false;
        let top = state.top_mut();

        egui::Window::new("command_palette")
            .id(egui::Id::new((self.id_salt, "palette_window")))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 80.0))
            // ⚠️ WIDTH ONLY. `fixed_size` sets `min_size` AND `max_size` on the
            // inner `Resize`, so `fixed_size(460, 0)` — intended as "460 wide,
            // height follows content" — pinned `max_size.y` to ZERO and clamped
            // the dropdown to a sliver about one and a half rows tall,
            // whatever `limit` said. Pin the two width bounds instead and
            // leave height free to grow up to the typeahead's own
            // `max_visible_rows` cap.
            .min_width(PALETTE_WIDTH)
            .max_width(PALETTE_WIDTH)
            .frame(
                egui::Frame::window(&ctx.global_style())
                    .fill(ui.tokens().color.bg_secondary)
                    .stroke(egui::Stroke::new(1.0_f32, accent)),
            )
            .show(&ctx, |ui| {
                if !crumbs.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        if Chip::new(root_title).clickable(true).show(ui).clicked {
                            jump = Some(0);
                        }
                        for (i, crumb) in crumbs.iter().enumerate() {
                            ui.label(PhosphorIcon::CaretRight.rich_text(12.0, muted));
                            let level = i + 1;
                            if level == crumbs.len() {
                                // Where you are is not a control.
                                ui.label(RichText::new(crumb).color(accent).strong());
                            } else if Chip::new(crumb).clickable(true).show(ui).clicked {
                                jump = Some(level);
                            }
                        }
                    });
                    ui.gap(Space::Sm);
                }

                let resp =
                    TypeaheadSearch::new(&edit_salt, &mut top.query, &shown, &mut top.highlight)
                        .placeholder(placeholder)
                        .empty_text("Nothing matches")
                        .max_visible_rows(limit)
                        .autofocus(true)
                        .show(ui);
                chosen = resp.chosen;

                // Click-away dismissal: the window consumes its own clicks, so
                // a primary click anywhere else while open closes the palette.
                // Suppressed on the entry frame — see `just_entered`.
                if !just_entered
                    && ui.input(|i| i.pointer.any_click())
                    && !ui.rect_contains_pointer(ui.min_rect().expand(8.0))
                {
                    dismissed = true;
                }
            });

        let mut action = PaletteAction::None;
        if let Some(level) = jump {
            state.truncate_to(level);
            ctx.request_repaint();
        } else if let Some(id) = chosen {
            match rows.iter().find(|r| r.option.id == id) {
                Some(PaletteRow {
                    kind: RowKind::Branch,
                    option,
                }) => {
                    let title = bare_title(&option.title).to_owned();
                    state.push(id, title);
                    ctx.request_repaint();
                }
                _ => {
                    action = PaletteAction::Invoke {
                        id,
                        path: state.path_owned(),
                    };
                }
            }
        }

        if dismissed || matches!(action, PaletteAction::Invoke { .. }) {
            state.close();
        }
        state.phase.tick();
        action
    }
}

/// A key's first press this frame — auto-repeat excluded, and consumed so it
/// acts once.
///
/// ⚠️ Repeat matters for Backspace: held down, it would climb one level per
/// repeat event, through every level to the root, the moment the query
/// emptied.
fn fresh_press(ctx: &egui::Context, key: Key) -> bool {
    ctx.input_mut(|i| {
        let fresh = i.events.iter().any(
            |e| matches!(e, Event::Key { key: k, pressed: true, repeat: false, .. } if *k == key),
        );
        if fresh {
            i.consume_key(Modifiers::NONE, key);
        }
        fresh
    })
}

/// A title without a trailing ellipsis — the breadcrumb names the level, it
/// does not promise more input.
fn bare_title(title: &str) -> &str {
    title
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim_end()
}

/// The option as listed. A branch reads "Go to collection…": the ellipsis is the
/// long-standing menu convention for "this asks for more before it acts",
/// which makes descent legible before Enter — and it renders in the default
/// font, where a chevron glyph might not.
fn displayed(row: &PaletteRow) -> TypeaheadOption {
    let mut option = row.option.clone();
    if row.kind == RowKind::Branch && bare_title(&option.title).len() == option.title.len() {
        option.title.push('…');
    }
    option
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pass::TestPass as _;
    use egui::{Pos2, RawInput, Rect, vec2};

    fn leaf(id: &str, title: &str) -> PaletteRow {
        PaletteRow::leaf(TypeaheadOption::new(id, title))
    }

    fn branch(id: &str, title: &str) -> PaletteRow {
        PaletteRow::branch(TypeaheadOption::new(id, title))
    }

    fn rows(n: usize) -> Vec<PaletteRow> {
        (0..n)
            .map(|i| leaf(&format!("cmd.{i}"), &format!("Command {i}")))
            .collect()
    }

    // ── the stack ────────────────────────────────────────────────────────

    #[test]
    fn a_fresh_palette_sits_at_the_root() {
        let s = PaletteState::default();
        assert_eq!(s.depth(), 0);
        assert!(s.path().is_empty());
        assert_eq!(s.query(), "");
    }

    #[test]
    fn the_path_is_every_selection_root_first() {
        let mut s = PaletteState::default();
        s.open();
        s.push("nostr.rooms", "Rooms");
        s.push("nostr.room:general", "#general");
        assert_eq!(s.path(), vec!["nostr.rooms", "nostr.room:general"]);
        assert_eq!(s.depth(), 2);
    }

    #[test]
    fn climbing_restores_what_was_typed_at_that_level() {
        let mut s = PaletteState::default();
        s.open();
        s.top_mut().query = "spa".into();
        s.push("goto.collection", "Go to collection");
        assert_eq!(s.query(), "", "a new level starts empty");
        s.top_mut().query = "budz".into();
        assert!(s.pop());
        assert_eq!(s.query(), "spa", "and the level above kept its query");
    }

    #[test]
    fn the_root_cannot_be_popped() {
        let mut s = PaletteState::default();
        s.open();
        assert!(!s.pop());
        assert_eq!(s.depth(), 0);
    }

    #[test]
    fn truncate_to_jumps_straight_to_an_ancestor() {
        let mut s = PaletteState::default();
        s.open();
        s.push("a", "A");
        s.push("b", "B");
        s.push("c", "C");
        s.truncate_to(1);
        assert_eq!(s.path(), vec!["a"]);
        s.truncate_to(5);
        assert_eq!(s.path(), vec!["a"], "a deeper target is a no-op");
    }

    #[test]
    fn opening_starts_again_from_the_root() {
        let mut s = PaletteState::default();
        s.open();
        s.top_mut().query = "x".into();
        s.push("a", "A");
        s.close();
        s.open();
        assert_eq!(s.depth(), 0);
        assert_eq!(s.query(), "");
    }

    #[test]
    fn rows_rank_like_options_and_keep_their_kind() {
        let set = vec![
            leaf("m", "Megasnek"),
            branch("s", "Snek"),
            leaf("o", "Other"),
        ];
        let got = rank_rows(&set, "snek", 10);
        let titles: Vec<&str> = got.iter().map(|r| r.option.title.as_str()).collect();
        assert_eq!(titles, vec!["Snek", "Megasnek"]);
        assert_eq!(
            got[0].kind,
            RowKind::Branch,
            "the kind travels with the row"
        );
        assert_eq!(
            rank_rows(&set, "  ", 2).len(),
            2,
            "an empty query browses the first `limit`, in order"
        );
    }

    #[test]
    fn a_branch_row_says_it_asks_for_more() {
        assert_eq!(
            displayed(&branch("g", "Go to collection")).title,
            "Go to collection…"
        );
        assert_eq!(
            displayed(&branch("g", "Go to collection…")).title,
            "Go to collection…",
            "not doubled when the caller already wrote it"
        );
        assert_eq!(displayed(&leaf("x", "Refresh")).title, "Refresh");
    }

    // ── interaction ──────────────────────────────────────────────────────
    // "The stack is right" and "the keys move the stack" are different claims;
    // the second is the one that breaks in an app.

    struct Harness {
        ctx: egui::Context,
        state: PaletteState,
    }

    impl Harness {
        fn open() -> Self {
            let ctx = egui::Context::default();
            // The search row draws a phosphor magnifier; without the font
            // bound, layout panics before anything happens.
            crate::icons::install_fonts(&ctx);
            let mut state = PaletteState::default();
            state.open();
            Self { ctx, state }
        }

        fn step(&mut self, rows: &[PaletteRow], events: Vec<Event>) -> PaletteAction {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = PaletteAction::None;
            let state = &mut self.state;
            let _ = self.ctx.test_pass(raw, |ui| {
                out = CommandPalette::new("h", rows)
                    .keybinding(false)
                    .show(ui, state);
            });
            out
        }

        /// The entry frame, then a frame for the autofocus request to land.
        fn settle(&mut self, rows: &[PaletteRow]) {
            self.step(rows, vec![]);
            self.step(rows, vec![]);
        }
    }

    /// A key going down. ⚠️ The `repeat` written here is IGNORED: egui
    /// overwrites it on every press from its own held-key set
    /// (`input_state/mod.rs:408-413`). A press that is never released leaves
    /// the key held, and the next press of it arrives as a repeat.
    fn press(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    fn release(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    /// One keystroke: down and up in the same pass.
    ///
    /// An earlier draft sent presses with no releases, so every second press
    /// of a key reached the palette as auto-repeat — and the palette, correctly
    /// ignoring repeats, "failed" to close at the root. The widget was right;
    /// the harness was holding the key down.
    fn tap(key: Key) -> Vec<Event> {
        vec![press(key), release(key)]
    }

    #[test]
    fn enter_on_a_branch_descends_instead_of_invoking() {
        let set = vec![branch("nostr.rooms", "Browse rooms"), leaf("x", "Other")];
        let mut h = Harness::open();
        h.settle(&set);
        let action = h.step(&set, tap(Key::Enter));
        assert_eq!(action, PaletteAction::None, "a branch does not invoke");
        assert_eq!(h.state.path(), vec!["nostr.rooms"]);
        assert!(h.state.is_open(), "and the palette stays open");
    }

    #[test]
    fn enter_on_a_leaf_invokes_with_the_path_it_was_chosen_in() {
        let mut h = Harness::open();
        h.state.push("nostr.rooms", "Rooms");
        h.state.push("nostr.room:general", "#general");
        let set = vec![leaf("member:$alice", "$alice")];
        h.settle(&set);
        let action = h.step(&set, tap(Key::Enter));
        assert_eq!(
            action,
            PaletteAction::Invoke {
                id: "member:$alice".into(),
                path: vec![
                    ContextId::new("nostr.rooms"),
                    ContextId::new("nostr.room:general")
                ],
            },
            "the same member under #art would be a different request"
        );
        assert!(!h.state.is_open());
    }

    #[test]
    fn escape_climbs_one_level_then_closes_at_the_root() {
        let mut h = Harness::open();
        h.state.push("a", "A");
        h.settle(&[]);
        h.step(&[], tap(Key::Escape));
        assert_eq!(h.state.depth(), 0, "climbed");
        assert!(h.state.is_open(), "without closing");
        h.step(&[], tap(Key::Escape));
        assert!(!h.state.is_open(), "and at the root it closes");
    }

    #[test]
    fn backspace_climbs_only_from_an_empty_query_and_never_on_repeat() {
        let mut h = Harness::open();
        h.state.push("a", "A");
        h.state.push("b", "B");
        h.settle(&[]);

        h.state.top_mut().query = "x".into();
        h.step(&[], tap(Key::Backspace));
        assert_eq!(h.state.depth(), 2, "a typed query is edited, not climbed");

        // A real hold, not a flag: down, and never released.
        h.state.top_mut().query.clear();
        h.step(&[], vec![press(Key::Backspace)]);
        assert_eq!(h.state.depth(), 1, "the first press of a hold climbs once");

        // Still held, so egui marks both of these repeats. Two, so it is the
        // repeat that holds the line and not just the entry-frame guard on the
        // pass right after the climb.
        h.step(&[], vec![press(Key::Backspace)]);
        h.step(&[], vec![press(Key::Backspace)]);
        assert_eq!(
            h.state.depth(),
            1,
            "a HELD Backspace must not cascade up the stack"
        );

        h.step(&[], vec![release(Key::Backspace)]);
        h.step(&[], tap(Key::Backspace));
        assert_eq!(h.state.depth(), 0, "a fresh keystroke climbs again");
    }

    // ── geometry ─────────────────────────────────────────────────────────

    /// Render the palette open and report the overlay's rect.
    fn open_palette_rect(count: usize, limit: usize) -> Rect {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let set = rows(count);
        let mut state = PaletteState::default();
        state.open();

        // Two passes: the first is the entry frame, and the window's size is
        // only known to `Memory` after it has been laid out.
        for _ in 0..2 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
                ..Default::default()
            };
            let _ = ctx.test_pass(raw, |ui| {
                CommandPalette::new("t", &set)
                    .limit(limit)
                    .keybinding(false)
                    .show(ui, &mut state);
            });
        }
        ctx.memory(|m| m.area_rect(egui::Id::new(("t", "palette_window"))))
            .expect("the palette window was laid out")
    }

    /// ⚠️ THE REGRESSION THIS EXISTS FOR.
    ///
    /// `Window::fixed_size` sets `min_size` AND `max_size` on the inner
    /// `Resize`. The palette passed `fixed_size(460, 0)` meaning "460 wide,
    /// height follows content", which pinned `max_size.y` to zero and clamped
    /// the dropdown to roughly one and a half rows — regardless of `limit`,
    /// and with no compile error and no panic to point at it.
    #[test]
    fn the_dropdown_is_tall_enough_for_the_rows_it_was_asked_to_show() {
        let rect = open_palette_rect(8, 8);
        // Eight 46px rows plus the search row; the assert is deliberately
        // loose on chrome and tight on the thing that broke.
        assert!(
            rect.height() > 8.0 * 46.0,
            "palette is {}px tall — not enough for 8 rows of 46px",
            rect.height()
        );
    }

    /// The other half: a short list must not reserve the full height.
    #[test]
    fn a_short_list_does_not_reserve_the_whole_limit() {
        let tall = open_palette_rect(8, 8);
        let short = open_palette_rect(2, 8);
        assert!(
            short.height() < tall.height(),
            "2 options ({}px) should be shorter than 8 ({}px)",
            short.height(),
            tall.height()
        );
    }

    /// Width stays pinned — the launcher should not move or resize under a
    /// longer command title.
    #[test]
    fn width_is_pinned_regardless_of_content() {
        let a = open_palette_rect(2, 8);
        let b = open_palette_rect(8, 8);
        assert_eq!(a.width(), b.width(), "width drifted with content");
    }
}
