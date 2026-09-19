//! `CommandPalette` story — a context stack: branch rows descend, leaves dispatch, Escape climbs.

use egui_widgets::command_palette::{
    CommandPalette, ContextId, PaletteAction, PaletteRow, PaletteState, rank_rows,
};
use egui_widgets::commands::{self, Command};
use egui_widgets::typeahead_search::TypeaheadOption;

use crate::{accent, muted};

/// Contexts this story resolves. Each is the id of the branch row that enters it.
const GOTO: &str = "story.goto";
const ROOMS: &str = "story.rooms";
/// Also the id of the stand-in filter widget's command — a registry command
/// that takes an argument descends into a context keyed by its own id.
const FILTER: &str = "story.filter";

/// Leaf and branch id prefixes.
const COLLECTION: &str = "story.collection:";
const ROOM: &str = "story.room:";
const MEMBER: &str = "story.member:";
const TRAIT: &str = "story.trait:";

const COLLECTIONS: &[&str] = &[
    "SpaceBudz",
    "Clay Nation",
    "Pavia",
    "ADA Handle",
    "HOSKY C(ash grab)NFT",
    "Chilled Kongs",
    "Deadpxlz",
    "Yummi Universe",
    "The Ape Society",
    "Cardano Kidz",
    "Boss Cat Rocket Club",
    "Aquafarmers",
    "Derp Birds",
    "Relics of Magic",
];

const TRAITS: &[&str] = &[
    "Background: Blue",
    "Background: Gold",
    "Eyes: Laser",
    "Eyes: Sleepy",
    "Hat: Crown",
    "Hat: None",
    "Mouth: Pipe",
];

/// `$alice` is in two rooms on purpose: her row alone cannot say which room
/// she was picked in. The path can.
const ROOM_MEMBERS: &[(&str, &[&str])] = &[
    ("general", &["$alice", "$bob", "$carol"]),
    ("trading", &["$dave", "$erin"]),
    ("art", &["$frank", "$alice"]),
];

pub struct CommandPaletteState {
    pub palette: PaletteState,
    pub invoked: Vec<String>,
    /// Stand-ins for widgets coming and going as the reader navigates.
    pub show_roster: bool,
    pub show_exporter: bool,
    /// The collection "open" in this pretend app. The filter widget only
    /// offers its command while one is.
    pub focused: Option<&'static str>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            palette: PaletteState::default(),
            invoked: Vec::new(),
            show_roster: true,
            show_exporter: true,
            focused: Some("SpaceBudz"),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut CommandPaletteState) {
    ui.label(
        egui::RichText::new("Command Palette")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A keyboard-first launcher over a STACK of contexts. Rows ending in \
             '…' descend; the rest dispatch. Escape climbs one level (and closes \
             only at the root), Backspace on an empty query climbs, breadcrumbs \
             jump. The palette never ranks — each frame this story resolves the \
             rows for the path it is on.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("Open palette").clicked() {
            state.palette.open();
        }
        if ui.button("Open straight into Go to").clicked() {
            state.palette.open();
            state.palette.push(GOTO, "Go to collection");
        }
    });
    crate::caption(
        ui,
        "⌘K here belongs to the storybook shell, so this story's palette opens \
         from the buttons — two palettes bound to one chord would fight over it.",
    );

    ui.add_space(10.0);
    crate::heading(ui, "Fed by the widgets on screen");
    crate::caption(
        ui,
        "The roster and exporter `offer` plain commands. The trait filter offers \
         one that takes an argument: it lists as a branch, the palette descends \
         into its context, this story supplies trait values, and the chosen \
         value comes back to the widget through `invoke_with`. Close the \
         collection and the filter leaves the palette.",
    );
    ui.add_space(6.0);
    ui.checkbox(&mut state.show_roster, "a roster widget is on screen");
    ui.checkbox(&mut state.show_exporter, "an exporter widget is on screen");
    ui.horizontal(|ui| {
        ui.label(format!(
            "open collection: {}",
            state.focused.unwrap_or("none")
        ));
        if state.focused.is_some() && ui.button("Close collection").clicked() {
            state.focused = None;
        }
    });
    ui.add_space(4.0);

    if state.show_roster
        && Command::new("story.roster.add", "Add wallet")
            .hint("stake address or $handle")
            .group("Wallets")
            .offer(ui)
    {
        state
            .invoked
            .push("story.roster.add (claimed by the roster)".into());
    }
    if state.show_exporter
        && Command::new("story.export.csv", "Export as CSV")
            .hint("the current selection")
            .group("Export")
            .offer(ui)
    {
        state
            .invoked
            .push("story.export.csv (claimed by the exporter)".into());
    }
    if let Some(collection) = state.focused
        && let Some(value) = Command::new(FILTER, "Filter by trait")
            .hint(format!("on {collection}"))
            .group("Collection")
            .argument("trait value")
            .offer_with_argument(ui)
    {
        state
            .invoked
            .push(format!("filter widget applied '{value}' to {collection}"));
    }

    // Resolve the rows for the path as it stands, then let go of the borrow
    // before the palette takes the state mutably.
    let offers = commands::offered_rows(ui.ctx());
    let (rows, placeholder) = {
        let path = state.palette.path();
        (
            rows_for(&path, state.palette.query(), offers),
            placeholder_for(&path),
        )
    };

    match CommandPalette::new("story_palette", &rows)
        .placeholder(placeholder)
        .keybinding(false)
        .show(ui, &mut state.palette)
    {
        PaletteAction::Invoke { id, path } => {
            let path: Vec<&str> = path.iter().map(ContextId::as_str).collect();
            dispatch(ui.ctx(), state, &path, id);
        }
        PaletteAction::None => {}
    }

    ui.add_space(12.0);
    let path = state.palette.path();
    crate::caption(
        ui,
        format!(
            "path: [{}]   query: \"{}\"   depth: {}",
            path.join(", "),
            state.palette.query(),
            state.palette.depth()
        ),
    );

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Invocation log")
            .color(crate::secondary(ui))
            .strong(),
    );
    if state.invoked.is_empty() {
        ui.label(
            egui::RichText::new("nothing invoked yet")
                .color(muted(ui))
                .small(),
        );
    }
    for line in state.invoked.iter().rev().take(8) {
        ui.label(egui::RichText::new(line).color(crate::ink(ui)).small());
    }
}

/// The one question the palette asks: given this path and query, what rows?
///
/// Matched on the WHOLE path. `[ROOMS, room]` is a member list only because of
/// the `ROOMS` in front of it.
fn rows_for(path: &[&str], query: &str, offers: Vec<PaletteRow>) -> Vec<PaletteRow> {
    match path {
        [] => {
            let mut root = vec![
                PaletteRow::branch(
                    TypeaheadOption::new(GOTO, "Go to collection").subtitle("any collection"),
                ),
                PaletteRow::branch(
                    TypeaheadOption::new(ROOMS, "Browse rooms")
                        .subtitle("two levels: a room, then a member"),
                ),
                PaletteRow::leaf(TypeaheadOption::new(
                    "story.gateway.status",
                    "Show gateway status",
                )),
            ];
            root.extend(offers);
            let mut rows = rank_rows(&root, query, 9);
            // Inline: a typed query also surfaces a few collections at the
            // root, capped hard so they cannot drown the commands. The full
            // list stays behind "Go to collection".
            if !query.trim().is_empty() {
                rows.extend(rank_rows(&collection_rows(), query, 3));
            }
            rows
        }
        [GOTO] => rank_rows(&collection_rows(), query, 12),
        [ROOMS] => rank_rows(&room_rows(), query, 12),
        [ROOMS, room] => rank_rows(&member_rows(room), query, 12),
        [FILTER] => rank_rows(&trait_rows(), query, 12),
        _ => Vec::new(),
    }
}

fn placeholder_for(path: &[&str]) -> &'static str {
    match path {
        [] => "Go somewhere, or run something…",
        [GOTO] => "Collection name…",
        [ROOMS] => "Room…",
        [ROOMS, _] => "Member…",
        [FILTER] => "Trait value…",
        _ => "Search…",
    }
}

fn collection_rows() -> Vec<PaletteRow> {
    COLLECTIONS
        .iter()
        .map(|c| {
            PaletteRow::leaf(
                TypeaheadOption::new(format!("{COLLECTION}{c}"), *c).subtitle("Collection"),
            )
        })
        .collect()
}

fn room_rows() -> Vec<PaletteRow> {
    ROOM_MEMBERS
        .iter()
        .map(|(room, members)| {
            PaletteRow::branch(
                TypeaheadOption::new(format!("{ROOM}{room}"), format!("#{room}"))
                    .subtitle(format!("{} members", members.len())),
            )
        })
        .collect()
}

fn member_rows(room_id: &str) -> Vec<PaletteRow> {
    let room = room_id.strip_prefix(ROOM).unwrap_or(room_id);
    ROOM_MEMBERS
        .iter()
        .find(|(r, _)| *r == room)
        .map(|(_, members)| {
            members
                .iter()
                .map(|m| {
                    PaletteRow::leaf(
                        TypeaheadOption::new(format!("{MEMBER}{m}"), *m)
                            .subtitle(format!("in #{room}")),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn trait_rows() -> Vec<PaletteRow> {
    TRAITS
        .iter()
        .map(|t| PaletteRow::leaf(TypeaheadOption::new(format!("{TRAIT}{t}"), *t)))
        .collect()
}

/// Act on a chosen leaf, knowing where it was chosen.
fn dispatch(ctx: &egui::Context, state: &mut CommandPaletteState, path: &[&str], id: String) {
    let at = format!("[{}]", path.join(", "));
    if let Some(name) = id.strip_prefix(COLLECTION) {
        state.focused = COLLECTIONS.iter().copied().find(|c| *c == name);
        state
            .invoked
            .push(format!("opened {name} (chosen at {at})"));
    } else if let (Some(member), [ROOMS, room]) = (id.strip_prefix(MEMBER), path) {
        let room = room.strip_prefix(ROOM).unwrap_or(room);
        state.invoked.push(format!(
            "picked {member} in #{room} — the room came from the path"
        ));
    } else if let (Some(value), [FILTER]) = (id.strip_prefix(TRAIT), path) {
        // The story does not apply the filter. It hands the value back to the
        // command's owner, which claims it on its next offer.
        commands::invoke_with(ctx, FILTER, value);
        state
            .invoked
            .push(format!("palette handed '{value}' to the filter widget"));
    } else {
        // Not ours — whichever widget offered it will claim it.
        commands::invoke(ctx, id.clone());
        state.invoked.push(format!("invoked: {id}"));
    }
}
