//! `arcade-admin-quad` — the operator panel an arcade game shows itself through.
//!
//! Four things on one surface: what the client costs, what the peer plane is
//! doing, what the world looks like, and the game's own commands. The panel
//! draws what it is given and reports the id of a command that was pressed. It
//! reads no room, no peer plane and no clock, so the same panel serves a game
//! that has all three and a story that has none.
//!
//! That is the split `arcade-peer`'s machine already makes — report, then draw —
//! applied to a screen: the game answers "what is true", this crate answers "how
//! is it shown", and a press comes back as an id for the game to act on.
//!
//! ## The surface
//!
//! A modal, on `macroquad_widgets::sheet_card` — the same scrim, card and motion
//! the fastbuy breakdown uses. The host owns the open/shut state and gets
//! [`SheetReply::close`] when the reader taps Close or the scrim.
//!
//! The facts are a label and a value each, in two columns when the card can seat
//! them and one when it cannot. A pixel game's canvas is often only a few hundred
//! pixels tall, so the rows are the scarce resource; a card too short for all of
//! them says `+N more` rather than drawing half of itself and reading as healthy.
//!
//! ## The extensions a game fills in
//!
//! * [`Snapshot`] — the numbers. Every field is something the game knows; where
//!   a platform cannot measure one (browser memory outside Chromium), it is an
//!   `Option` and the panel draws a dash rather than a guess.
//! * [`Command`] — the actions. The game registers a static slice; the panel has
//!   never heard of the game and does not know what any of them do.
//!
//! ## What it is not
//!
//! Not a HUD: it is opened, read and closed. Not a rules surface: nothing here
//! may reach a sim, a hash or a replay. Not the place to compute anything the
//! game can compute — the panel is handed the answer.

use macroquad::prelude::*;
use macroquad_widgets::{Align, Painter, TextSize};

/// What one frame costs the client.
///
/// `worst_ms` is the point of the set: a mean of 16 ms that hides a 90 ms frame
/// is how a game feels bad while reading well.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClientCost {
    pub fps: f32,
    pub frame_ms: f32,
    pub worst_ms: f32,
    /// `None` where the platform cannot say — a browser exposes this only on
    /// Chromium, so a dash is the honest answer.
    pub memory_bytes: Option<u64>,
}

/// What the peer plane is doing.
///
/// `links_expected` comes from the room's interest sets, not from the links
/// themselves: a count of open links says nothing on its own, and "3 of 4" is
/// the sentence that says whether the mesh is whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetHealth {
    pub links_open: u16,
    pub links_expected: u16,
    pub links_relayed: u16,
    /// Tenths of a millisecond, as the negotiation machine's `Path` carries it.
    pub rtt_tenths: Option<u16>,
    /// Links torn down in the last minute.
    pub churn: u16,
    pub offered: u32,
    pub answered: u32,
    pub failed: u32,
}

/// The world, as the room last described it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct World {
    pub players: u16,
    /// Players with a connection right now; the rest are held through their
    /// grace.
    pub connected: u16,
    pub tick: u64,
    pub running: bool,
    pub colo: Option<String>,
    /// How long ago this was fetched. A stale reading must not look live.
    pub age_ms: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub client: ClientCost,
    pub net: NetHealth,
    /// `None` until a world has been heard from, which is ordinary on the first
    /// seconds of a session.
    pub world: Option<World>,
}

/// Something the game can be asked to do.
///
/// `id` is the whole contract: the panel returns it and the game matches on it,
/// so the panel never learns what a command means. `destructive` is drawn in the
/// warning colour and is the game's cue to ask twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    pub enabled: bool,
    pub destructive: bool,
}

/// What the game did about the last command. Shown under the buttons, because a
/// press that failed silently is worse than no button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    pub ok: bool,
}

pub struct Panel<'a> {
    pub snapshot: &'a Snapshot,
    pub commands: &'a [Command],
    pub outcome: Option<&'a Outcome>,
}

/// What the sheet asked for this frame.
///
/// Two answers rather than an `Option`: a command press and a close are
/// different facts, and a host that had to distinguish them by comparing ids to
/// a magic string would eventually get it wrong.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheetReply {
    /// The id of a command that was pressed, for the game to act on.
    pub pressed: Option<&'static str>,
    /// Close was pressed, or the scrim was tapped.
    pub close: bool,
}

/// One line of the panel, before it is drawn. Separated from [`show_sheet`] so
/// the wording can be read in a test without a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
}

/// The panel's lines, in the order they are drawn.
///
/// One fact per line rather than a sentence per section: the panel is read at a
/// glance, and two columns of label-and-value is what a glance can take. A world
/// that has not been heard from is one line saying so, rather than a block of
/// zeroes that reads like an empty world.
pub fn rows(snapshot: &Snapshot) -> Vec<Row> {
    let pair = |label: &'static str, value: String| Row { label, value };
    let mut rows = vec![
        pair("fps", format!("{:.0}", snapshot.client.fps)),
        pair("frame", format!("{:.1} ms", snapshot.client.frame_ms)),
        pair("worst", format!("{:.1} ms", snapshot.client.worst_ms)),
        pair("memory", memory(snapshot.client.memory_bytes)),
    ];

    let net = snapshot.net;
    rows.push(pair(
        "links",
        if net.links_expected > 0 {
            format!("{} of {}", net.links_open, net.links_expected)
        } else {
            format!("{}", net.links_open)
        },
    ));
    rows.push(pair("relayed", format!("{}", net.links_relayed)));
    rows.push(pair("rtt", tenths(net.rtt_tenths)));
    rows.push(pair(
        "answered",
        // A ratio nobody has started is not 0 of 0 — that reads as a failure.
        if net.offered > 0 {
            format!("{} of {}", net.answered, net.offered)
        } else {
            "—".into()
        },
    ));
    rows.push(pair("failed", format!("{}", net.failed)));
    rows.push(pair("churn", format!("{}", net.churn)));

    match &snapshot.world {
        // Short enough to sit beside its label in a narrow column; a value that
        // runs past its own label is what the reader sees first.
        None => rows.push(pair("world", "unheard".into())),
        Some(world) => {
            rows.push(pair("players", format!("{}", world.players)));
            // Only when it differs: `12 (11 live)` does not fit a narrow column,
            // and on a healthy world the two numbers are the same one.
            if world.connected != world.players {
                rows.push(pair("live", format!("{}", world.connected)));
            }
            rows.push(pair("tick", format!("{}", world.tick)));
            rows.push(pair(
                "state",
                if world.running { "running" } else { "paused" }.into(),
            ));
            rows.push(pair(
                "colo",
                world.colo.clone().unwrap_or_else(|| "—".into()),
            ));
            rows.push(pair(
                "echo",
                format!("{:.1} s old", f64::from(world.age_ms) / 1000.0),
            ));
        }
    }
    rows
}

fn memory(bytes: Option<u64>) -> String {
    match bytes {
        Some(bytes) => format!("{} MB", bytes / (1024 * 1024)),
        None => "—".into(),
    }
}

fn tenths(tenths: Option<u16>) -> String {
    match tenths {
        Some(tenths) => format!("{:.1} ms", f32::from(tenths) / 10.0),
        None => "—".into(),
    }
}

/// Draw the panel as a modal, over whatever the game drew.
///
/// The chrome is [`macroquad_widgets::sheet_card`] — the same scrim, card, shape
/// and motion the fastbuy breakdown uses — so this function owns only what is
/// inside it. A phone gets a bottom sheet and a desk a centred modal, because
/// the widget decides that, not this crate.
///
/// Closing is the host's: the sheet is a state machine the caller drives, and a
/// panel that closed itself would be a second owner of the same fact. A press on
/// Close or on the scrim comes back as [`SheetReply::close`].
///
/// A card too short for every row keeps the last cell for `+N more`.
pub fn show_sheet(
    painter: &Painter,
    panel: &Panel<'_>,
    state: &macroquad_widgets::SheetState,
    area: Rect,
) -> SheetReply {
    let form = macroquad_widgets::SheetForm::for_area(area);
    let content_w = macroquad_widgets::sheet_content_width(form, area);
    let columns = columns_for(content_w);
    let Some(card) = macroquad_widgets::sheet_card(
        painter,
        state,
        area,
        height(painter, panel, form, area, columns),
    ) else {
        return SheetReply::default();
    };
    let title = painter.size(TextSize::Lg);
    painter.text_top(
        "ADMIN",
        card.content.x,
        card.content.y,
        title,
        fade(painter.theme.color.text_primary, card.alpha),
    );
    let top = card.content.y + title + 10.0;
    let body_area = Rect::new(
        card.content.x,
        top,
        card.content.w,
        (card.content.bottom() - top).max(0.0),
    );
    let pressed = body(painter, panel, body_area, columns, card.alpha);

    // Only an open sheet answers taps: one closing is already on its way.
    let live = state.is_open() && state.progress() > 0.6;
    let dismissed = macroquad_widgets::Button::new("Close")
        .variant(macroquad_widgets::ButtonVariant::Tonal)
        .enabled(live)
        .show(painter, card.close);
    SheetReply {
        pressed,
        close: dismissed || (live && painter.tap.is_some_and(|t| !card.rect.contains(t))),
    }
}

/// Line height of the panel's own text. One constant, so the height a card is
/// sized for and the rows that go in it cannot disagree.
const ROW_STEP_FACTOR: f32 = 1.55;
/// Least width a column can be, whatever the figures measure.
///
/// Not measured: `Painter::measure` falls back to an estimate until a glyph is
/// in the atlas, so a measured threshold flips the panel between one column and
/// two on its first frames. The width is a layout constant; the row fits its own
/// text into whatever it is given.
const MIN_COLUMN_W: f32 = 122.0;
/// Room between a row's label and its value.
const COLUMN_GAP: f32 = 8.0;
/// Room between the two columns.
const GUTTER: f32 = 12.0;
/// Padding inside the card, once the sheet's own has been paid.
const BODY_PAD: f32 = 12.0;

/// Two columns when the card can seat two of [`MIN_COLUMN_W`], one otherwise.
///
/// A phone in portrait gets one; a 320-pixel-wide canvas, where height is the
/// scarce dimension, gets two — sixteen facts as sixteen lines would not fit.
fn columns_for(content_w: f32) -> usize {
    if content_w - 2.0 * BODY_PAD >= 2.0 * MIN_COLUMN_W + GUTTER {
        2
    } else {
        1
    }
}

fn column_w(body_w: f32, columns: usize) -> f32 {
    (body_w - GUTTER * (columns - 1) as f32) / columns as f32
}

/// How tall the card is when open: the chrome, the title, the rows and the
/// action band.
fn height(
    painter: &Painter,
    panel: &Panel<'_>,
    form: macroquad_widgets::SheetForm,
    area: Rect,
    columns: usize,
) -> f32 {
    let step = painter.size(TextSize::Sm) * ROW_STEP_FACTOR;
    let title = painter.size(TextSize::Lg) + 10.0;
    let lines = rows(panel.snapshot).len().div_ceil(columns);
    macroquad_widgets::sheet_chrome(form, area)
        + title
        + lines as f32 * step
        + action_band(painter, panel)
        // The room [`body`] keeps above its first row and below its last; a
        // card sized without it loses the final line to its own padding.
        + 2.0 * BODY_PAD
}

/// `colour` at `alpha` of its own opacity, for content inside a sheet that is
/// still sliding in.
fn fade(colour: Color, alpha: f32) -> Color {
    Color::new(colour.r, colour.g, colour.b, colour.a * alpha)
}

fn body(
    painter: &Painter,
    panel: &Panel<'_>,
    rect: Rect,
    columns: usize,
    alpha: f32,
) -> Option<&'static str> {
    let small = painter.size(TextSize::Sm);
    let step = small * ROW_STEP_FACTOR;
    let left = rect.x + BODY_PAD;
    let width = rect.w - BODY_PAD * 2.0;
    let col_w = column_w(width, columns);

    let actions = action_band(painter, panel);
    let floor = rect.y + rect.h - BODY_PAD - actions;
    let lines = (((floor - (rect.y + BODY_PAD)) / step).floor()).max(0.0) as usize;

    let all = rows(panel.snapshot);
    // One cell is kept back for "+N more" when the rows do not fit.
    let capacity = lines * columns;
    let (shown, hidden) = if all.len() > capacity && capacity > 0 {
        (capacity - 1, all.len() - (capacity - 1))
    } else {
        (all.len().min(capacity), 0)
    };

    // Column-major, one column's worth of lines at a time. The split is the
    // card's line count, not the number of rows shown: `+N more` is drawn in the
    // cell after the last row, and with a single column that cell is the line
    // below it — a split of `shown` puts it back at the top, over the first row.
    let per_column = lines.max(1);
    let cell = |i: usize| {
        let col = (i / per_column).min(columns - 1);
        let line = i % per_column;
        Rect::new(
            left + col as f32 * (col_w + GUTTER),
            rect.y + BODY_PAD + line as f32 * step,
            col_w,
            step,
        )
    };
    for (i, row) in all[..shown].iter().enumerate() {
        draw_row(painter, row, cell(i), small, alpha);
    }
    if hidden > 0 {
        painter.text_in(
            &format!("+{hidden} more"),
            cell(shown),
            small,
            fade(painter.theme.color.text_muted, alpha),
            Align::Start,
        );
    }

    let mut pressed = None;
    let mut at = floor + 8.0;
    let row_h = small * 2.2;
    for command in panel.commands {
        let button = Rect::new(left, at, width, row_h);
        let hit = painter.interact(button, command.enabled);
        let (bg, fg) = if !command.enabled {
            (
                fade(painter.theme.color.bg_highlight, alpha),
                fade(painter.theme.color.text_muted, alpha),
            )
        } else if command.destructive {
            (
                fade(painter.theme.color.warning, alpha),
                fade(painter.theme.color.bg_primary, alpha),
            )
        } else if hit.hover || hit.pressed {
            (
                fade(painter.theme.color.accent, alpha),
                fade(painter.theme.color.bg_primary, alpha),
            )
        } else {
            (
                fade(painter.theme.color.bg_highlight, alpha),
                fade(painter.theme.color.text_primary, alpha),
            )
        };
        draw_rectangle(button.x, button.y, button.w, button.h, bg);
        painter.text_in(
            command.label,
            Rect::new(button.x + 8.0, button.y, button.w - 16.0, button.h),
            small,
            fg,
            Align::Start,
        );
        if hit.clicked && command.enabled && pressed.is_none() {
            pressed = Some(command.id);
        }
        at += row_h + 6.0;
    }

    if let Some(outcome) = panel.outcome {
        let colour = if outcome.ok {
            painter.theme.color.success
        } else {
            painter.theme.color.error
        };
        painter.text_in(
            &outcome.text,
            Rect::new(left, at, width, step),
            small,
            fade(colour, alpha),
            Align::Start,
        );
    }

    pressed
}

/// A label on the left and its figure on the right, both inside one column.
///
/// A value more than three fifths of the cell is fitted: nothing in the panel is
/// that long, so it means something longer than intended got in, and a number
/// drawn over its own label is unreadable. The label is not measured — every one
/// is a short word this crate wrote, and measuring it would cost an allocation
/// on every row of every frame to catch a case that cannot happen.
fn draw_row(painter: &Painter, row: &Row, cell: Rect, size: f32, alpha: f32) {
    let room = (cell.w - COLUMN_GAP) * 0.6;
    let value = if painter.measure_mono(&row.value, size).width > room {
        painter.fit_mono(&row.value, size, room)
    } else {
        row.value.clone()
    };
    painter.text_in(
        row.label,
        cell,
        size,
        fade(painter.theme.color.text_muted, alpha),
        Align::Start,
    );
    painter.mono_in(
        &value,
        cell,
        size,
        fade(painter.theme.color.text_primary, alpha),
        Align::End,
    );
}

/// Height the action band needs, so the rows above can stop short of it.
fn action_band(painter: &Painter, panel: &Panel<'_>) -> f32 {
    let small = painter.size(TextSize::Sm);
    let row_h = small * 2.2;
    let buttons = panel.commands.len() as f32 * (row_h + 6.0);
    let outcome = if panel.outcome.is_some() {
        small * 1.55
    } else {
        0.0
    };
    buttons + outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            client: ClientCost {
                fps: 59.8,
                frame_ms: 16.7,
                worst_ms: 31.2,
                memory_bytes: Some(412 * 1024 * 1024),
            },
            net: NetHealth {
                links_open: 3,
                links_expected: 4,
                links_relayed: 1,
                rtt_tenths: Some(124),
                churn: 2,
                offered: 7,
                answered: 7,
                failed: 1,
            },
            world: Some(World {
                players: 12,
                connected: 11,
                tick: 41_120,
                running: true,
                colo: Some("BNE".into()),
                age_ms: 1_200,
            }),
        }
    }

    fn value(rows: &[Row], label: &str) -> String {
        rows.iter()
            .find(|row| row.label == label)
            .map(|row| row.value.clone())
            .unwrap_or_else(|| panic!("no `{label}` row in {rows:?}"))
    }

    #[test]
    fn the_rows_say_what_the_numbers_mean() {
        let rows = rows(&snapshot());
        assert_eq!(value(&rows, "fps"), "60");
        assert_eq!(value(&rows, "frame"), "16.7 ms");
        assert_eq!(value(&rows, "worst"), "31.2 ms");
        assert_eq!(value(&rows, "links"), "3 of 4");
        assert_eq!(value(&rows, "rtt"), "12.4 ms");
        assert_eq!(value(&rows, "answered"), "7 of 7");
        assert_eq!(value(&rows, "players"), "12");
        assert_eq!(value(&rows, "live"), "11");
        assert_eq!(value(&rows, "colo"), "BNE");
        assert_eq!(value(&rows, "echo"), "1.2 s old");
    }

    /// The second number is a fact only when it is a different one; a healthy
    /// world does not spend a line saying 12 players are 12 players.
    #[test]
    fn a_world_where_everyone_is_connected_does_not_repeat_the_count() {
        let mut snapshot = snapshot();
        snapshot.world.as_mut().unwrap().connected = 12;
        let rows = rows(&snapshot);
        assert!(
            rows.iter().all(|row| row.label != "live"),
            "a fully connected world drew a live count: {rows:?}"
        );
    }

    /// A link count with nothing to compare it against is just a number, so the
    /// "of" is only there when the room has said what to expect.
    #[test]
    fn an_unknown_expected_count_does_not_invent_one() {
        let mut snapshot = snapshot();
        snapshot.net.links_expected = 0;
        assert_eq!(value(&rows(&snapshot), "links"), "3");
    }

    /// Two dashes, for two different reasons: a platform that cannot measure
    /// memory, and a pair that has not reported a round trip.
    #[test]
    fn what_the_platform_cannot_measure_is_a_dash_not_a_zero() {
        let mut snapshot = snapshot();
        snapshot.client.memory_bytes = None;
        snapshot.net.rtt_tenths = None;
        let rows = rows(&snapshot);
        assert_eq!(value(&rows, "memory"), "—");
        assert_eq!(value(&rows, "rtt"), "—");
    }

    /// `0 of 0` reads as a failed negotiation rather than one nobody started.
    #[test]
    fn a_negotiation_nobody_started_is_not_a_ratio() {
        let mut snapshot = snapshot();
        snapshot.net.offered = 0;
        snapshot.net.answered = 0;
        assert_eq!(value(&rows(&snapshot), "answered"), "—");
    }

    #[test]
    fn a_world_not_heard_from_says_so_rather_than_reading_as_empty() {
        let mut snapshot = snapshot();
        snapshot.world = None;
        let rows = rows(&snapshot);
        assert_eq!(value(&rows, "world"), "unheard");
        assert!(
            rows.iter().all(|row| row.label != "players"),
            "an unheard world drew a player count: {rows:?}"
        );
    }

    /// The panel is read on a 320x224 canvas as well as a phone, so its two
    /// columns have to be possible there, and must not be attempted on a narrow
    /// portrait screen.
    #[test]
    fn two_columns_only_when_the_card_has_the_width() {
        assert_eq!(columns_for(280.0), 2, "a 320-wide canvas seats two");
        assert_eq!(columns_for(252.0), 1, "a 292-wide phone does not");
        assert_eq!(columns_for(160.0), 1, "a 240-wide canvas certainly not");
    }
}
