//! `arcade-admin-quad` — the operator panel an arcade game shows itself through.
//!
//! Four things on one surface: what the client costs, what the peer plane is
//! doing, what the world looks like, and the game's own commands. Under the
//! facts, three sparklines show how frame time, rtt and the player count have
//! moved over the window the host hands in. The panel draws what it is given
//! and reports the id of a command that was pressed. It reads no room, no peer
//! plane and no clock, so the same panel serves a game that has all three and a
//! story that has none.
//!
//! That is the split `arcade-peer`'s machine already makes — report, then draw —
//! applied to a screen: the game answers "what is true", this crate answers "how
//! is it shown", and a press comes back as an id for the game to act on.
//!
//! ## The surface
//!
//! An operator console, not the game's modal. A rectangular panel with a title
//! bar, facts grouped under `CLIENT` / `NETWORK` / `WORLD`, and commands as a
//! menu of compact rows. Everything is drawn in the host's **monospace face**,
//! so figures line up in a column and the panel reads like a terminal rather
//! than like the island behind it.
//!
//! The chrome is deliberately this crate's own. Reusing the shared game sheet
//! gave the panel rounded corners, a scrim, a bottom-sheet form and a large
//! `Close` button in the game's face — which is exactly the thing an operator
//! surface should not look like. The host still owns the open/shut state (a
//! [`SheetState`], so the motion is the same one the rest of a game uses) and
//! gets [`SheetReply::close`] when `CLOSE` or the scrim is pressed.
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
use macroquad_widgets::{Align, Painter, SheetState, TextSize};

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

/// What the game did about the last command. Shown under the commands, because
/// a press that failed silently is worse than no button.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    pub ok: bool,
}

/// Recent samples for the panel's sparklines, oldest first.
///
/// Each series is the host's own history: the panel has no clock and keeps no
/// state, so what a sparkline shows is what the game measured. `NaN` is a gap —
/// the host had no sample at that moment — and the line breaks rather than
/// dropping to zero, so a network that stopped answering does not read like a
/// round trip of 0 ms.
#[derive(Clone, Copy, Debug, Default)]
pub struct Trends<'a> {
    pub frame_ms: &'a [f32],
    pub rtt_ms: &'a [f32],
    pub players: &'a [f32],
    /// How much wall time the window covers, drawn in the heading. The host
    /// knows how many samples it actually has, so an early panel says `5s`
    /// rather than claiming a full window it has not filled.
    pub span_secs: u32,
}

pub struct Panel<'a> {
    pub snapshot: &'a Snapshot,
    pub commands: &'a [Command],
    pub outcome: Option<&'a Outcome>,
    pub trends: Option<&'a Trends<'a>>,
}

/// What the panel asked for this frame.
///
/// Two answers rather than an `Option`: a command press and a close are
/// different facts, and a host that had to distinguish them by comparing ids to
/// a magic string would eventually get it wrong.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheetReply {
    /// The id of a command that was pressed, for the game to act on.
    pub pressed: Option<&'static str>,
    /// `CLOSE` was pressed, or the scrim was tapped.
    pub close: bool,
}

/// Which part of the running game a fact describes.
///
/// The panel groups its rows under these, because sixteen unlabelled lines read
/// as a wall. The order is the order a glance takes them in: what the client
/// costs, then whether the mesh is whole, then what the room says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Client,
    Network,
    World,
}

impl Section {
    /// The heading drawn over its rows.
    pub fn label(self) -> &'static str {
        match self {
            Section::Client => "CLIENT",
            Section::Network => "NETWORK",
            Section::World => "WORLD",
        }
    }
}

/// One line of the panel, before it is drawn. Separated from [`show_sheet`] so
/// the wording can be read in a test without a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub section: Section,
}

/// The panel's lines, in the order they are drawn.
///
/// **Ordered by what a glance needs, because a short panel truncates from the
/// end.** The nine rows a cramped surface keeps are the ones that say whether
/// the world is healthy, and the counters that only matter when it is not are
/// what `+N more` stands for.
///
/// One fact per line rather than a sentence per section: the panel is read at a
/// glance, and a label and its figure is what a glance can take. A world that
/// has not been heard from is one line saying so, rather than a block of zeroes
/// that reads like an empty world.
pub fn rows(snapshot: &Snapshot) -> Vec<Row> {
    let pair = |section: Section, label: &'static str, value: String| Row {
        label,
        value,
        section,
    };
    let net = snapshot.net;
    let mut rows = vec![
        pair(
            Section::Client,
            "fps",
            format!("{:.0}", snapshot.client.fps),
        ),
        pair(
            Section::Client,
            "frame",
            format!("{:.1} ms", snapshot.client.frame_ms),
        ),
        pair(
            Section::Client,
            "worst",
            format!("{:.1} ms", snapshot.client.worst_ms),
        ),
        pair(
            Section::Client,
            "memory",
            memory(snapshot.client.memory_bytes),
        ),
        pair(
            Section::Network,
            "links",
            if net.links_expected > 0 {
                format!("{} of {}", net.links_open, net.links_expected)
            } else {
                format!("{}", net.links_open)
            },
        ),
        pair(Section::Network, "rtt", tenths(net.rtt_tenths)),
        pair(Section::Network, "failed", format!("{}", net.failed)),
        pair(
            Section::Network,
            "relayed",
            format!("{}", net.links_relayed),
        ),
        pair(Section::Network, "churn", format!("{}", net.churn)),
        pair(
            Section::Network,
            "answered",
            // A ratio nobody has started is not 0 of 0 — that reads as a failure.
            if net.offered > 0 {
                format!("{} of {}", net.answered, net.offered)
            } else {
                "—".into()
            },
        ),
    ];

    match &snapshot.world {
        // Short enough to sit beside its label in a narrow column; a value that
        // runs past its own label is what the reader sees first.
        None => rows.push(pair(Section::World, "world", "unheard".into())),
        Some(world) => {
            rows.push(pair(
                Section::World,
                "players",
                format!("{}", world.players),
            ));
            // Only when it differs: `12 (11 live)` does not fit a narrow column,
            // and on a healthy world the two numbers are the same one.
            if world.connected != world.players {
                rows.push(pair(Section::World, "live", format!("{}", world.connected)));
            }
            rows.push(pair(Section::World, "tick", format!("{}", world.tick)));
            rows.push(pair(
                Section::World,
                "colo",
                world.colo.clone().unwrap_or_else(|| "—".into()),
            ));
            rows.push(pair(
                Section::World,
                "state",
                if world.running { "running" } else { "paused" }.into(),
            ));
            rows.push(pair(
                Section::World,
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

/// The panel's own measurements, all resolved from the type ramp once a frame.
struct Metrics {
    title: f32,
    heading: f32,
    row: f32,
    row_step: f32,
    heading_step: f32,
    command_step: f32,
    header_h: f32,
}

impl Metrics {
    fn new(painter: &Painter) -> Self {
        // A console is read at arm's length on a large window: the body is a
        // comfortable reading size, the headings one rung below it so they
        // recede, and the title one rung above.
        let title = painter.size(TextSize::Xl);
        let heading = painter.size(TextSize::Base);
        let row = painter.size(TextSize::Lg);
        Self {
            title,
            heading,
            row,
            row_step: row * ROW_STEP_FACTOR,
            heading_step: heading * HEADING_STEP_FACTOR,
            command_step: row * COMMAND_STEP_FACTOR,
            header_h: title * HEADER_FACTOR,
        }
    }
}

/// The panel's widest form. Wider than this and the two columns drift apart;
/// narrower is what a phone's margin leaves.
const MAX_PANEL_W: f32 = 600.0;
/// Room between the panel and the screen's edge, in points.
const MIN_MARGIN: f32 = 12.0;
const MAX_MARGIN: f32 = 24.0;
/// Padding inside the panel.
const PAD: f32 = 16.0;
/// Room between the title bar and the first heading.
const BODY_TOP: f32 = 10.0;
/// Room between a row's label and its figure.
const LABEL_GAP: f32 = 18.0;
/// Room between the two columns of facts.
const COLUMN_GUTTER: f32 = 22.0;
/// Least width a column can be. A phone in portrait takes one column.
///
/// Not measured: `Painter::measure_mono` falls back to an estimate until a
/// glyph is in the atlas, so a measured threshold flips the panel between one
/// column and two on its first frames. The width is a layout constant; the row
/// fits its own text into whatever it is given.
const MIN_COLUMN_W: f32 = 190.0;
/// Room between one section and the next.
const SECTION_GAP: f32 = 12.0;
/// Room between the last fact and the commands.
const COMMAND_GAP: f32 = 14.0;
/// A sparkline's label column, its figure column, and the room between the
/// figure and the chart.
const TREND_LABEL_W: f32 = 86.0;
const TREND_VALUE_W: f32 = 92.0;
const TREND_CHART_GAP: f32 = 12.0;
/// Height of one trend row, as a multiple of its point size. Enough for the
/// chart to have shape at the body's size.
const TREND_ROW_FACTOR: f32 = 1.8;
/// Below this much height the sparklines are left out rather than crowding out
/// the facts they are describing.
const TRENDS_MIN_H: f32 = 300.0;
/// Line height of a fact, as a multiple of its point size.
const ROW_STEP_FACTOR: f32 = 1.55;
/// Line height of a section heading.
const HEADING_STEP_FACTOR: f32 = 1.7;
/// Height of a command row.
const COMMAND_STEP_FACTOR: f32 = 1.9;
/// Height of the title bar, as a multiple of the title's point size.
const HEADER_FACTOR: f32 = 2.4;
/// How far the panel rises while it is opening.
const RISE: f32 = 6.0;

/// Draw the panel over whatever the game drew, in window points.
///
/// `area` is the window, so the console is laid out as it would be on any
/// screen rather than in the game's canvas. The host owns the open/shut state:
/// the panel fades and rises with [`SheetState`]'s motion, and a press on
/// `CLOSE` or on the scrim comes back as [`SheetReply::close`].
///
/// A panel too short for every fact keeps a line for `+N more` rather than
/// drawing half of itself and reading as healthy.
pub fn show_sheet(
    painter: &Painter,
    panel: &Panel<'_>,
    state: &SheetState,
    area: Rect,
) -> SheetReply {
    if !state.visible() {
        return SheetReply::default();
    }
    let m = Metrics::new(painter);
    let alpha = state.progress();
    let live = state.is_open() && alpha > 0.6;

    let margin_x = (area.w * 0.03).clamp(MIN_MARGIN, MAX_MARGIN);
    // Vertically the panel keeps only the minimum: a desktop has height to
    // spare, and a 640-point story should seat the whole console.
    let margin_y = MIN_MARGIN;
    let width = (area.w - 2.0 * margin_x).clamp(0.0, MAX_PANEL_W);
    let content_w = (width - 2.0 * PAD).max(0.0);
    let columns = columns_for(content_w);

    let blocks = blocks(rows(panel.snapshot));
    let commands_h = commands_height(panel, &m);
    let max_h = (area.h - 2.0 * margin_y).max(0.0);
    let trends_h = if max_h >= TRENDS_MIN_H {
        trends_height(panel, &m)
    } else {
        0.0
    };
    let body_capacity =
        (max_h - m.header_h - BODY_TOP - trends_h - commands_h - PAD).max(m.row_step);
    let fit = fit(&blocks, columns, &m, body_capacity);
    let height = (m.header_h + BODY_TOP + fit.height + trends_h + commands_h + PAD).min(max_h);
    let rect = Rect::new(
        area.x + (area.w - width) * 0.5,
        area.y + (area.h - height) * 0.5 + (1.0 - alpha) * RISE,
        width,
        height,
    );

    draw_rectangle(
        area.x,
        area.y,
        area.w,
        area.h,
        Color::new(0.0, 0.0, 0.0, 0.45 * alpha),
    );
    draw_panel(
        painter,
        panel,
        rect,
        content_w,
        columns,
        &blocks,
        &fit,
        &m,
        alpha,
        live,
        trends_h > 0.0,
    )
}

/// Two columns when there is room for two readable ones, one otherwise.
fn columns_for(content_w: f32) -> usize {
    if content_w >= 2.0 * MIN_COLUMN_W + COLUMN_GUTTER {
        2
    } else {
        1
    }
}

fn column_w(content_w: f32, columns: usize) -> f32 {
    (content_w - COLUMN_GUTTER * (columns - 1) as f32) / columns as f32
}

/// Consecutive rows sharing a section, in the order `rows` returned them.
#[derive(Debug)]
struct Block {
    section: Section,
    rows: Vec<Row>,
}

fn blocks(rows: Vec<Row>) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for row in rows {
        match blocks.last_mut() {
            Some(block) if block.section == row.section => block.rows.push(row),
            _ => blocks.push(Block {
                section: row.section,
                rows: vec![row],
            }),
        }
    }
    blocks
}

/// How many lines a run of `rows` takes in `columns` columns.
fn lines_for(rows: usize, columns: usize) -> usize {
    rows.div_ceil(columns.max(1))
}

/// What fits, and what the `+N more` line stands for.
struct Fit {
    /// `(block index, rows shown)`, in draw order.
    shown: Vec<(usize, usize)>,
    hidden: usize,
    height: f32,
}

fn blocks_height(blocks: &[Block], columns: usize, m: &Metrics) -> f32 {
    let mut height = 0.0;
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            height += SECTION_GAP;
        }
        height += m.heading_step;
        height += lines_for(block.rows.len(), columns) as f32 * m.row_step;
    }
    height
}

/// Take whole rows, in section order, until `capacity` is spent.
///
/// A section that cannot seat even one row stops the walk: the rows after it
/// are all less important, and drawing them under a heading of their own would
/// put the important facts below the drill-down.
fn fit(blocks: &[Block], columns: usize, m: &Metrics, capacity: f32) -> Fit {
    let full = blocks_height(blocks, columns, m);
    if full <= capacity {
        return Fit {
            shown: blocks
                .iter()
                .enumerate()
                .map(|(index, block)| (index, block.rows.len()))
                .collect(),
            hidden: 0,
            height: full,
        };
    }

    // The last line is the `+N more` cell, so the rows may only spend what is
    // left after it.
    let budget = (capacity - m.row_step).max(0.0);
    let mut fit = Fit {
        shown: Vec::new(),
        hidden: 0,
        height: 0.0,
    };
    let mut used = 0.0;
    for (index, block) in blocks.iter().enumerate() {
        let gap = if fit.shown.is_empty() {
            0.0
        } else {
            SECTION_GAP
        };
        let need = |take: usize| {
            used + gap + m.heading_step + lines_for(take, columns) as f32 * m.row_step
        };
        if need(1) > budget {
            fit.hidden += block.rows.len();
            fit.hidden += blocks[index + 1..]
                .iter()
                .map(|block| block.rows.len())
                .sum::<usize>();
            break;
        }
        let mut take = block.rows.len();
        while take > 1 && need(take) > budget {
            take -= 1;
        }
        fit.hidden += block.rows.len() - take;
        used = need(take);
        fit.shown.push((index, take));
        if take < block.rows.len() {
            fit.hidden += blocks[index + 1..]
                .iter()
                .map(|block| block.rows.len())
                .sum::<usize>();
            break;
        }
    }
    fit.height = used + if fit.hidden > 0 { m.row_step } else { 0.0 };
    fit
}

/// Height the sparklines need, or 0 when the host gave no history at all.
///
/// The block is reserved from the first frame even before the first sample:
/// growing the panel a quarter second after it opens would move every fact
/// under the reader's eye.
fn trends_height(panel: &Panel<'_>, m: &Metrics) -> f32 {
    if panel.trends.is_none() {
        return 0.0;
    }
    COMMAND_GAP + m.heading_step + 3.0 * m.row * TREND_ROW_FACTOR
}

/// The three things a console plots over time. Fixed rather than generic: each
/// has its own figure format and its own sensible floor for the chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrendKind {
    Frame,
    Rtt,
    Players,
}

impl TrendKind {
    fn label(self) -> &'static str {
        match self {
            TrendKind::Frame => "FRAME",
            TrendKind::Rtt => "RTT",
            TrendKind::Players => "PLAYERS",
        }
    }

    /// The latest sample, not the latest finite one: a series that has stopped
    /// answering says `—` even while its older line is still visible.
    fn value(self, values: &[f32]) -> String {
        match values.last().copied().filter(|value| value.is_finite()) {
            None => "—".into(),
            Some(value) => match self {
                TrendKind::Frame | TrendKind::Rtt => format!("{value:.1} ms"),
                TrendKind::Players => format!("{value:.0}"),
            },
        }
    }

    /// The floor of the chart's scale, so a steady 16 ms line sits mid-height
    /// rather than pinned to the top or the bottom.
    fn ceiling(self) -> f32 {
        match self {
            TrendKind::Frame => 33.3,
            TrendKind::Rtt => 20.0,
            TrendKind::Players => 2.0,
        }
    }
}

/// Draw the trends block, returning the height it used including the gap above
/// it and its heading.
fn draw_trends(
    painter: &Painter,
    trends: &Trends<'_>,
    left: f32,
    right: f32,
    y: f32,
    m: &Metrics,
    alpha: f32,
) -> f32 {
    let y = y + COMMAND_GAP;
    heading(
        painter,
        &format!("TRENDS / {}s", trends.span_secs),
        left,
        right,
        y,
        m,
        alpha,
    );
    let row_h = m.row * TREND_ROW_FACTOR;
    let mut y = y + m.heading_step;
    for (kind, values) in [
        (TrendKind::Frame, trends.frame_ms),
        (TrendKind::Rtt, trends.rtt_ms),
        (TrendKind::Players, trends.players),
    ] {
        draw_trend_row(
            painter,
            kind,
            values,
            Rect::new(left, y, right - left, row_h),
            m,
            alpha,
        );
        y += row_h;
    }
    COMMAND_GAP + m.heading_step + 3.0 * row_h
}

fn draw_trend_row(
    painter: &Painter,
    kind: TrendKind,
    values: &[f32],
    row: Rect,
    m: &Metrics,
    alpha: f32,
) {
    let c = painter.theme.color;
    let label_w = TREND_LABEL_W.min(row.w);
    let value_w = TREND_VALUE_W.min((row.w - label_w).max(0.0));
    painter.mono_in(
        kind.label(),
        Rect::new(row.x, row.y, label_w, row.h),
        m.heading,
        fade(c.text_muted, alpha),
        Align::Start,
    );
    painter.mono_in(
        &kind.value(values),
        Rect::new(row.x + label_w, row.y, value_w, row.h),
        m.row,
        fade(c.text_primary, alpha),
        Align::End,
    );
    let chart_x = row.x + label_w + value_w + TREND_CHART_GAP;
    let chart_w = (row.right() - chart_x).max(0.0);
    if chart_w <= 0.0 {
        return;
    }
    let chart_h = m.row * 1.4;
    let chart = Rect::new(chart_x, row.y + (row.h - chart_h) * 0.5, chart_w, chart_h);
    sparkline(painter, kind, values, chart, alpha);
}

/// The y-range a series is drawn in.
///
/// Millisecond series are read from zero, so a spike has a height relative to a
/// frame budget rather than to whatever the window happened to contain. A count
/// is different: ten players and twelve are two steps worth seeing, so a player
/// series gets its own range with half a player of headroom.
fn trend_range(kind: TrendKind, values: &[f32]) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| value.is_finite()) {
        low = low.min(value);
        high = high.max(value);
    }
    match kind {
        TrendKind::Players if low.is_finite() => (low - 0.5, high + 0.5),
        TrendKind::Players => (0.0, 1.0),
        TrendKind::Frame | TrendKind::Rtt => (0.0, kind.ceiling().max(high.max(0.0))),
    }
}

/// One series as a line, oldest on the left. A non-finite sample breaks the
/// line: a gap is information, and joining across it would invent a reading.
fn sparkline(painter: &Painter, kind: TrendKind, values: &[f32], rect: Rect, alpha: f32) {
    let c = painter.theme.color;
    draw_rectangle(
        rect.x,
        rect.bottom() - 1.0,
        rect.w,
        1.0,
        fade(c.border, alpha * 0.55),
    );
    let (low, high) = trend_range(kind, values);
    let span = (high - low).max(f32::EPSILON);
    let n = values.len();
    let x_at = |i: usize| {
        if n <= 1 {
            rect.x + rect.w * 0.5
        } else {
            rect.x + rect.w * (i as f32 / (n - 1) as f32)
        }
    };
    let y_at =
        |value: f32| rect.bottom() - 2.0 - ((value - low) / span).clamp(0.0, 1.0) * (rect.h - 4.0);
    let colour = fade(c.accent, alpha);
    let mut previous: Option<(f32, f32)> = None;
    let mut finite = 0usize;
    for (i, value) in values.iter().enumerate() {
        if !value.is_finite() {
            previous = None;
            continue;
        }
        finite += 1;
        let point = (x_at(i), y_at(*value));
        if let Some(before) = previous {
            if kind == TrendKind::Players {
                // A count changes between samples; a step keeps the reading
                // true for the interval it was measured in.
                draw_line(before.0, before.1, point.0, before.1, 1.5, colour);
                draw_line(point.0, before.1, point.0, point.1, 1.5, colour);
            } else {
                draw_line(before.0, before.1, point.0, point.1, 1.5, colour);
            }
        }
        previous = Some(point);
    }
    if finite == 1
        && let Some((i, value)) = values
            .iter()
            .enumerate()
            .find(|(_, value)| value.is_finite())
    {
        draw_circle(x_at(i), y_at(*value), 1.5, colour);
    }
}

/// Height the commands and their outcome need, so the facts above can stop
/// short of them.
fn commands_height(panel: &Panel<'_>, m: &Metrics) -> f32 {
    let mut height = 0.0;
    if !panel.commands.is_empty() {
        height += COMMAND_GAP + m.heading_step;
        height += panel.commands.len() as f32 * m.command_step;
    }
    if panel.outcome.is_some() {
        if panel.commands.is_empty() {
            height += COMMAND_GAP;
        }
        height += m.row_step;
    }
    height
}

#[allow(clippy::too_many_arguments)]
fn draw_panel(
    painter: &Painter,
    panel: &Panel<'_>,
    rect: Rect,
    content_w: f32,
    columns: usize,
    blocks: &[Block],
    fit: &Fit,
    m: &Metrics,
    alpha: f32,
    live: bool,
    trends_on: bool,
) -> SheetReply {
    let c = painter.theme.color;
    let left = rect.x + PAD;
    let right = rect.x + rect.w - PAD;

    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fade(c.bg_primary, alpha));
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        m.header_h,
        fade(c.bg_secondary, alpha),
    );
    draw_rectangle(
        rect.x,
        rect.y + m.header_h,
        rect.w,
        1.0,
        fade(c.border, alpha),
    );
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, fade(c.border, alpha));

    // Title bar: the name on the left, the close affordance on the right. The
    // affordance is words rather than a glyph so it reads in any mono face and
    // needs no icon table.
    let head_mid = rect.y + m.header_h * 0.5;
    painter.mono_mid(
        "ADMIN",
        left,
        head_mid,
        m.title,
        fade(c.text_primary, alpha),
    );
    let close_size = m.heading;
    let close_w = painter.measure_mono("CLOSE", close_size).width + 12.0;
    let close = Rect::new(right - close_w, rect.y, close_w, m.header_h);
    let close_hit = painter.interact(close, live);
    let close_colour = if close_hit.hover || close_hit.pressed {
        c.text_primary
    } else {
        c.text_muted
    };
    painter.mono_in(
        "CLOSE",
        close,
        close_size,
        fade(close_colour, alpha),
        Align::End,
    );

    let body_top = rect.y + m.header_h + BODY_TOP;
    let mut y = body_top;
    for (i, (index, take)) in fit.shown.iter().enumerate() {
        let block = &blocks[*index];
        if i > 0 {
            y += SECTION_GAP;
        }
        heading(painter, block.section.label(), left, right, y, m, alpha);
        y += m.heading_step;

        let w = column_w(content_w, columns);
        for (i, row) in block.rows.iter().take(*take).enumerate() {
            let column = i % columns;
            let line = i / columns;
            let cell = Rect::new(
                left + column as f32 * (w + COLUMN_GUTTER),
                y + line as f32 * m.row_step,
                w,
                m.row_step,
            );
            draw_row(painter, row, cell, m.row, alpha);
        }
        y += lines_for(*take, columns) as f32 * m.row_step;
    }
    if fit.hidden > 0 {
        painter.mono_in(
            &format!("+{} more", fit.hidden),
            Rect::new(left, y, content_w, m.row_step),
            m.row,
            fade(c.text_muted, alpha),
            Align::Start,
        );
        y += m.row_step;
    }

    if trends_on && let Some(trends) = panel.trends {
        y += draw_trends(painter, trends, left, right, y, m, alpha);
    }

    let mut pressed = None;
    if !panel.commands.is_empty() {
        y += COMMAND_GAP;
        heading(painter, "COMMANDS", left, right, y, m, alpha);
        y += m.heading_step;
        for command in panel.commands {
            let row = Rect::new(left, y, content_w, m.command_step);
            if let Some(id) = draw_command(painter, command, row, m.row, alpha, live)
                && pressed.is_none()
            {
                pressed = Some(id);
            }
            y += m.command_step;
        }
    }

    if let Some(outcome) = panel.outcome {
        let colour = if outcome.ok {
            painter.theme.color.success
        } else {
            painter.theme.color.error
        };
        let text = painter.fit_mono(&outcome.text, m.row, content_w);
        painter.mono_in(
            &text,
            Rect::new(left, y, content_w, m.row_step),
            m.row,
            fade(colour, alpha),
            Align::Start,
        );
    }

    let dismissed =
        (close_hit.clicked && live) || (live && painter.tap.is_some_and(|tap| !rect.contains(tap)));
    SheetReply {
        pressed,
        close: dismissed,
    }
}

/// A section heading with a hairline running to the panel's edge.
fn heading(painter: &Painter, label: &str, left: f32, right: f32, y: f32, m: &Metrics, alpha: f32) {
    let c = painter.theme.color;
    let rect = Rect::new(left, y, right - left, m.heading_step);
    painter.mono_in(
        label,
        rect,
        m.heading,
        fade(c.text_muted, alpha),
        Align::Start,
    );
    let text_w = painter.measure_mono(label, m.heading).width;
    let start = left + text_w + 10.0;
    if right > start {
        draw_rectangle(
            start,
            y + m.heading_step * 0.5,
            right - start,
            1.0,
            fade(c.border, alpha * 0.7),
        );
    }
}

/// A label on the left and its figure on the right, both inside one column.
///
/// A value more than three fifths of the cell is fitted: nothing in the panel
/// is that long, so it means something longer than intended got in, and a
/// number drawn over its own label is unreadable. The label is not measured —
/// every one is a short word this crate wrote, and measuring it would cost an
/// allocation on every row of every frame to catch a case that cannot happen.
fn draw_row(painter: &Painter, row: &Row, cell: Rect, size: f32, alpha: f32) {
    let c = painter.theme.color;
    let room = (cell.w - LABEL_GAP) * 0.6;
    let value = if painter.measure_mono(&row.value, size).width > room {
        painter.fit_mono(&row.value, size, room)
    } else {
        row.value.clone()
    };
    painter.mono_in(
        row.label,
        cell,
        size,
        fade(c.text_secondary, alpha),
        Align::Start,
    );
    painter.mono_in(&value, cell, size, fade(c.text_primary, alpha), Align::End);
}

/// One command as a menu row: a marker, the label, and a highlight under the
/// pointer. Returns the id when it was pressed.
fn draw_command(
    painter: &Painter,
    command: &Command,
    row: Rect,
    size: f32,
    alpha: f32,
    live: bool,
) -> Option<&'static str> {
    let c = painter.theme.color;
    let hit = painter.interact(row, command.enabled && live);
    let hot = hit.hover || hit.pressed;
    if hot {
        draw_rectangle(
            row.x - 4.0,
            row.y,
            row.w + 8.0,
            row.h,
            fade(c.bg_highlight, alpha),
        );
    }
    let marker_colour = if !command.enabled {
        c.text_muted
    } else if command.destructive {
        c.warning
    } else if hot {
        c.accent
    } else {
        c.text_muted
    };
    let label_colour = if !command.enabled {
        c.text_muted
    } else if command.destructive {
        c.warning
    } else {
        c.text_primary
    };
    painter.mono_in(
        ">",
        Rect::new(row.x, row.y, size, row.h),
        size,
        fade(marker_colour, alpha),
        Align::Start,
    );
    painter.mono_in(
        command.label,
        Rect::new(row.x + size + 6.0, row.y, row.w - size - 6.0, row.h),
        size,
        fade(label_colour, alpha),
        Align::Start,
    );
    (hit.clicked && command.enabled && live).then_some(command.id)
}

/// `colour` at `alpha` of its own opacity, for content inside a panel that is
/// still sliding in.
fn fade(colour: Color, alpha: f32) -> Color {
    Color::new(colour.r, colour.g, colour.b, colour.a * alpha)
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

    /// The ramp at the sizes the console uses, so a layout test can measure
    /// without a window.
    fn metrics() -> Metrics {
        Metrics {
            title: 16.0,
            heading: 13.0,
            row: 15.0,
            row_step: 15.0 * ROW_STEP_FACTOR,
            heading_step: 13.0 * HEADING_STEP_FACTOR,
            command_step: 15.0 * COMMAND_STEP_FACTOR,
            header_h: 16.0 * HEADER_FACTOR,
        }
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

    /// The panel groups its facts, and every row belongs to exactly the part of
    /// the game it describes. A section that lost a row would draw it under the
    /// wrong heading.
    #[test]
    fn every_fact_belongs_to_the_part_of_the_game_it_describes() {
        let rows = rows(&snapshot());
        let section = |label: &str| {
            rows.iter()
                .find(|row| row.label == label)
                .map(|row| row.section)
                .unwrap_or_else(|| panic!("no `{label}` row in {rows:?}"))
        };
        assert_eq!(section("fps"), Section::Client);
        assert_eq!(section("memory"), Section::Client);
        assert_eq!(section("links"), Section::Network);
        assert_eq!(section("answered"), Section::Network);
        assert_eq!(section("players"), Section::World);
        assert_eq!(section("echo"), Section::World);
    }

    /// Grouping is by runs, so the draw order is the order `rows` returned.
    #[test]
    fn the_sections_are_drawn_in_the_order_the_rows_are_written() {
        let blocks = blocks(rows(&snapshot()));
        let labels: Vec<_> = blocks.iter().map(|block| block.section.label()).collect();
        assert_eq!(labels, ["CLIENT", "NETWORK", "WORLD"]);
        assert!(
            blocks.iter().all(|block| !block.rows.is_empty()),
            "an empty section reached the layout: {blocks:?}"
        );
    }

    /// A phone in portrait takes one column; a desktop panel takes two. Both
    /// are layout constants, because a measured text width is an estimate until
    /// the glyphs land.
    #[test]
    fn two_columns_only_when_the_panel_can_seat_two() {
        assert_eq!(columns_for(568.0), 2, "a 600-wide panel seats two");
        assert_eq!(columns_for(334.0), 1, "a 390-wide phone gives one");
        assert_eq!(columns_for(264.0), 1, "a 320-wide surface gives one");
    }

    /// A full set of rows in two columns plus its sparklines is short enough for
    /// a desktop, so the truncation path is only for a cramped story — not for
    /// the real surface.
    #[test]
    fn the_whole_panel_fits_a_desktop_height() {
        let metrics = metrics();
        let snapshot = snapshot();
        let commands = [Command {
            id: "readout",
            label: "Toggle net readout",
            enabled: true,
            destructive: false,
        }];
        let trends = Trends {
            frame_ms: &[16.0, 17.0, 16.5],
            rtt_ms: &[12.0, 12.4],
            players: &[11.0, 12.0],
            span_secs: 15,
        };
        let panel = Panel {
            snapshot: &snapshot,
            commands: &commands,
            outcome: None,
            trends: Some(&trends),
        };
        let blocks = blocks(rows(&snapshot));
        let full = blocks_height(&blocks, 2, &metrics);
        let height = metrics.header_h
            + BODY_TOP
            + full
            + trends_height(&panel, &metrics)
            + commands_height(&panel, &metrics)
            + PAD;
        assert!(
            height < 620.0,
            "the panel is {height} points tall, too tall for a desktop capture"
        );
    }

    /// The latest sample is the headline; a series that stopped answering says
    /// `—` even while its older line is still on screen.
    #[test]
    fn a_trend_reports_its_latest_sample_or_a_dash() {
        assert_eq!(TrendKind::Frame.value(&[15.0, 16.5]), "16.5 ms");
        assert_eq!(TrendKind::Rtt.value(&[12.0, f32::NAN]), "—");
        assert_eq!(TrendKind::Players.value(&[10.0, 12.0]), "12");
        let empty: [f32; 0] = [];
        assert_eq!(TrendKind::Rtt.value(&empty), "—");
    }

    /// A panel that will plot a history reserves the space on its first
    /// frame; a game that offers no history reserves none. The third case — a
    /// history with no samples yet — keeps the rows, so opening the panel does
    /// not resize it under the reader.
    #[test]
    fn a_trend_block_is_reserved_before_the_first_sample() {
        let snapshot = snapshot();
        let metrics = metrics();
        let none = Panel {
            snapshot: &snapshot,
            commands: &[],
            outcome: None,
            trends: None,
        };
        assert_eq!(trends_height(&none, &metrics), 0.0);

        let empty = Trends::default();
        let pending = Panel {
            trends: Some(&empty),
            ..none
        };
        assert!(trends_height(&pending, &metrics) > 0.0);
    }

    /// A count's line has to show a step between ten and twelve players, so it
    /// uses its own range; a frame time is read from zero so a spike has a
    /// meaningful height.
    #[test]
    fn a_player_count_gets_its_own_range_so_a_step_is_visible() {
        let (low, high) = trend_range(TrendKind::Players, &[10.0, 11.0, 12.0]);
        assert!(low < 10.0, "the bottom of the range was {low}");
        assert!(high > 12.0, "the top of the range was {high}");
        assert_eq!(trend_range(TrendKind::Frame, &[16.0, 17.0]), (0.0, 33.3));
    }

    /// A panel shorter than its facts keeps one line for the count of what it
    /// left out.
    #[test]
    fn a_cramped_panel_says_how_many_facts_it_left_out() {
        let metrics = metrics();
        let blocks = blocks(rows(&snapshot()));
        let all: usize = blocks.iter().map(|block| block.rows.len()).sum();
        let fit = fit(&blocks, 2, &metrics, 80.0);
        assert!(fit.hidden > 0, "nothing was hidden");
        let shown: usize = fit.shown.iter().map(|(_, take)| *take).sum();
        assert_eq!(shown + fit.hidden, all, "a fact went missing");
        assert!(
            fit.height <= 80.0,
            "the fitted panel is {} points in an 80-point hole",
            fit.height
        );
        // Not a heading with no rows under it.
        assert!(
            fit.shown.iter().all(|(_, take)| *take > 0),
            "an empty block was kept: {:?}",
            fit.shown
        );
    }
}
