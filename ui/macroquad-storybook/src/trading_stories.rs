//! Stories for the trading views: a price chart and its life strip, stat
//! tiles, a change chip, a buy/sell split bar, an activity feed, a holders
//! board and map, a token header, and a raw treemap.
//!
//! The market data here is a **layout fixture**: a deterministic path with
//! plausible magnitudes, not a token's history. The token arena's captures are
//! the real-data check. Every story is driven by the storybook's clock, so P
//! pauses, `.` steps a frame and `--at` captures an exact moment of the
//! arrival, the pulse or the scrub.

use macroquad::prelude::*;
use macroquad_widgets::theme::with_alpha;
use macroquad_widgets::{
    BoardRow, ChangeChipVm, ChartLabels, ChartPoint, FEED_ROW_STEP, FeedRow, FeedVm, Gesture,
    HolderBoardVm, HolderMapLayout, HolderMapState, HolderMapVm, Holding, HoldingKind, LifeBucket,
    LifeStripVm, MapGroup, MapTile, MarketInk, Painter, PriceChartVm, SplitBarVm, StatMark,
    StatTile, StatTilesVm, TokenHeaderVm, TokenLogo, TradeMark, TradeSide, TradeSize, TreemapGroup,
    TreemapState, TreemapTile, TreemapVm, change_chip, draw_rounded_rect, feed, format,
    holder_board, holder_map, life_strip, price_chart, split_bar, stat_tiles, time_at_x,
    token_header, treemap,
};

/// Where the fixture's history starts: 13 Oct 2026, 10:13 UTC.
const T0: u64 = 1_792_000_000;
/// One fixture point an hour.
const STEP: u64 = 3_600;

/// Which trading story this is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TradingView {
    Chart,
    ChartPhone,
    ChartScrub,
    ChartHigh,
    Strip,
    StripPhone,
    StripPending,
    Stats,
    Split,
    Change,
    Feed,
    FeedScrolled,
    Board,
    BoardPartial,
    Map,
    Header,
    HeaderLive,
    Treemap,
}

/// A holding as the story owns it, before it is borrowed for a widget.
struct OwnedHolding {
    key: String,
    label: String,
    kind: HoldingKind,
    amount: f64,
}

/// A feed row as the story owns it.
struct OwnedRow {
    chip: String,
    tone: Color,
    title: String,
    title_is_handle: bool,
    sub: String,
    right: String,
    right_tone: Color,
    time: String,
}

pub struct TradingStory {
    view: TradingView,
    points: Vec<ChartPoint>,
    buckets: Vec<LifeBucket>,
    holdings: Vec<OwnedHolding>,
    rows: Vec<OwnedRow>,
    map: HolderMapState,
    treemap: TreemapState<String>,
    logo: Option<Texture2D>,
}

impl TradingStory {
    pub fn new(view: TradingView) -> Self {
        let points = points();
        let buckets = buckets(&points);
        let holdings = holdings();
        let rows = rows();
        let logo = (view == TradingView::HeaderLive).then(logo_texture);
        Self {
            view,
            points,
            buckets,
            holdings,
            rows,
            map: HolderMapState::new(),
            treemap: TreemapState::default(),
            logo,
        }
    }

    /// Draw the story in `area`.
    pub fn draw(&mut self, p: &Painter, area: Rect, gesture: &Gesture, t: f64) {
        let ink = MarketInk::from_theme(&p.theme);
        let w = area.w.min(760.0);
        let x = area.x;
        let y = area.y + 8.0;
        match self.view {
            TradingView::Chart
            | TradingView::ChartPhone
            | TradingView::ChartHigh
            | TradingView::ChartScrub => {
                let width = if self.view == TradingView::ChartPhone {
                    w.min(350.0)
                } else {
                    w
                };
                let plot = Rect::new(x, y + 34.0, width, (area.h - 60.0).clamp(140.0, 280.0));
                let panel = Rect::new(plot.x - 10.0, plot.y - 30.0, plot.w + 20.0, plot.h + 62.0);
                draw_rounded_rect(
                    panel.x,
                    panel.y,
                    panel.w,
                    panel.h,
                    10.0,
                    with_alpha(p.theme.color.bg_secondary, 0.7),
                );
                let t0 = self.points[0].unix;
                let t1 = self.points.last().map_or(t0 + 1, |p| p.unix);
                let head = match self.view {
                    TradingView::ChartHigh => t1,
                    _ => t0 + (t1 - t0) * 2 / 3,
                };
                // The points the replay has SEEN, not the tape's future: the
                // arena's `series()` grows as beats land, and the chart's dim
                // band is what is still to come. Passing the whole fixture
                // would draw the future as history.
                let seen = self
                    .points
                    .iter()
                    .rposition(|p| p.unix <= head)
                    .map_or(1, |i| i + 1);
                let points = &self.points[..seen];
                let high = points.iter().map(|p| p.price).fold(f64::MIN, f64::max);
                let scrub = match self.view {
                    TradingView::ChartScrub => gesture
                        .drag
                        .or(gesture.tap)
                        .filter(|at| plot.contains(*at))
                        .map(|at| time_at_x(plot, t0, t1, at.x))
                        .or_else(|| Some(t0 + (t1 - t0) * 45 / 100)),
                    _ => None,
                };
                let scrub_label = scrub.map(|u| format!("{} {}", format::day(u), format::clock(u)));
                let start = format!(
                    "{} {}",
                    format::day(self.points[0].unix),
                    format::clock(self.points[0].unix)
                );
                let end = match self.points.last() {
                    Some(last) => {
                        format!("{} {}", format::day(last.unix), format::clock(last.unix))
                    }
                    None => "NOW".to_string(),
                };
                let high_label = format!("HIGH {}", format::price(high));
                let vm = PriceChartVm {
                    points,
                    lead: None,
                    t0,
                    t1,
                    range: None,
                    head,
                    scrub,
                    scrub_label: scrub_label.as_deref(),
                    new_high: if self.view == TradingView::ChartHigh {
                        0.8
                    } else {
                        0.0
                    },
                    up: points.last().is_none_or(|p| p.price >= points[0].price),
                    ink,
                    labels: ChartLabels {
                        high: Some(&high_label),
                        start: Some(&start),
                        end: Some(&end),
                    },
                };
                price_chart(p, &vm, plot);
            }
            TradingView::Strip | TradingView::StripPhone | TradingView::StripPending => {
                let width = if self.view == TradingView::StripPhone {
                    w.min(350.0)
                } else {
                    w
                };
                let rect = Rect::new(x, y + 54.0, width, 46.0);
                draw_rounded_rect(
                    rect.x - 8.0,
                    rect.y - 10.0,
                    rect.w + 16.0,
                    rect.h + 34.0,
                    10.0,
                    with_alpha(p.theme.color.bg_secondary, 0.7),
                );
                let t0 = self.points[0].unix;
                let t1 = self.points.last().map_or(t0 + 1, |p| p.unix);
                let span = t1 - t0;
                let head = t0 + span * 72 / 100;
                let brush = (t0 + span * 55 / 100, t1);
                let view = (head.saturating_sub(7 * 86_400), head);
                let drag = (self.view == TradingView::StripPending).then(|| t0 + span * 20 / 100);
                let drag_label =
                    drag.map(|u| format!("FROM {} {}", format::day(u), format::clock(u)));
                let first = format!("{} {}", format::day(t0), format::year(t0));
                let vm = LifeStripVm {
                    buckets: &self.buckets,
                    t0,
                    t1,
                    brush,
                    head,
                    view: Some(view),
                    loaded_from: t0 + span * 35 / 100,
                    at_start: false,
                    pending: self.view == TradingView::StripPending,
                    drag,
                    drag_label: drag_label.as_deref(),
                    first_label: Some(&first),
                    now_label: Some("NOW"),
                    ink,
                };
                life_strip(p, &vm, rect);
            }
            TradingView::Stats => {
                let tiles = [
                    StatTile {
                        label: "MCAP",
                        value: "18.4M",
                        mark: Some(StatMark::Ada),
                    },
                    StatTile {
                        label: "LIQUIDITY",
                        value: "2.1M",
                        mark: Some(StatMark::Ada),
                    },
                    StatTile {
                        label: "VOLUME",
                        value: "842K",
                        mark: Some(StatMark::Ada),
                    },
                    StatTile {
                        label: "HOLDERS",
                        value: "1,284",
                        mark: None,
                    },
                ];
                stat_tiles(
                    p,
                    &StatTilesVm { tiles: &tiles },
                    Rect::new(x, y + 20.0, w.min(350.0), 44.0),
                );
            }
            TradingView::Split => {
                for (i, (share, start, end)) in [
                    (0.62, "BUYS 62%", "38% SELLS"),
                    (0.5, "BUYS 50%", "50% SELLS"),
                    (0.08, "BUYS 8%", "92% SELLS"),
                ]
                .into_iter()
                .enumerate()
                {
                    split_bar(
                        p,
                        &SplitBarVm {
                            share,
                            start,
                            end,
                            start_ink: ink.buy,
                            end_ink: ink.sell,
                            marker_ink: p.theme.color.text_primary,
                        },
                        Rect::new(x, y + 20.0 + i as f32 * 60.0, w.min(350.0), 8.0),
                    );
                }
            }
            TradingView::Change => {
                for (i, (change, since)) in [
                    (0.03, "in the last 24h"),
                    (-0.124, "in the last 7 days"),
                    (0.0, "over all history"),
                ]
                .into_iter()
                .enumerate()
                {
                    change_chip(
                        p,
                        &ChangeChipVm { change, since, ink },
                        Rect::new(x, y + 20.0 + i as f32 * 52.0, w.min(350.0), 26.0),
                    );
                }
            }
            TradingView::Feed | TradingView::FeedScrolled => {
                let rows: Vec<FeedRow> = self
                    .rows
                    .iter()
                    .map(|r| FeedRow {
                        chip: &r.chip,
                        tone: r.tone,
                        title: &r.title,
                        title_is_handle: r.title_is_handle,
                        sub: &r.sub,
                        right: &r.right,
                        right_tone: r.right_tone,
                        time: &r.time,
                    })
                    .collect();
                let scrolled = (self.view == TradingView::FeedScrolled)
                    .then_some("6 BACK · REPLAYS FROM HERE");
                feed(
                    p,
                    &FeedVm {
                        rows: &rows,
                        age: (t as f32) % 2.0,
                        scrolled,
                    },
                    Rect::new(x, y + 20.0, w.min(350.0), 5.0 * FEED_ROW_STEP),
                );
            }
            TradingView::Board | TradingView::BoardPartial => {
                let holdings = borrowed(&self.holdings);
                let fits = 8;
                let total: f64 = holdings.iter().map(|h| h.amount).sum();
                let largest = holdings.first().map_or(1.0, |h| h.amount);
                let rows: Vec<BoardRow> = holdings
                    .iter()
                    .take(fits)
                    .enumerate()
                    .map(|(i, holding)| BoardRow {
                        holding,
                        rank: i + 1,
                        at: if i == fits - 1 { 7.45 } else { i as f32 },
                        flash: (i == 0).then_some((ink.buy, 0.6)),
                    })
                    .collect();
                let vm = HolderBoardVm {
                    rows: &rows,
                    top: 0,
                    fits,
                    count: holdings.len(),
                    largest,
                    total,
                    complete: self.view != TradingView::BoardPartial,
                    ink,
                };
                holder_board(p, &vm, Rect::new(x, y + 20.0, w.min(390.0), 220.0));
            }
            TradingView::Map => {
                let holdings = borrowed(&self.holdings);
                let body = Rect::new(
                    x,
                    y + 54.0,
                    w.min(390.0),
                    (area.h - 90.0).clamp(180.0, 330.0),
                );
                let total: f64 = holdings.iter().map(|h| h.amount).sum();
                // A title over the map, as the arena draws one. The widget
                // draws the body only; the host names the panel.
                p.text(
                    "THE SUPPLY, BY HOLDER",
                    body.x,
                    body.y - 18.0,
                    p.size(ui_theme::TextSize::Xs),
                    p.theme.color.text_primary,
                );
                let count = holdings
                    .iter()
                    .filter(|h| matches!(h.kind, HoldingKind::Person(_)))
                    .count();
                p.text(
                    &format!("{count} HOLDERS"),
                    body.right() - 76.0,
                    body.y - 18.0,
                    p.size(ui_theme::TextSize::Xs),
                    p.theme.color.text_muted,
                );
                let HolderMapLayout { tiles, groups } = self.map.layout(&holdings, body);
                let placed: Vec<MapTile> = holdings
                    .iter()
                    .filter_map(|h| {
                        tiles.get(h.key).map(|rect| MapTile {
                            holding: h,
                            rect: *rect,
                            flash: (h.key == "whale.ada").then_some((ink.buy, 0.7)),
                        })
                    })
                    .collect();
                let named: Vec<MapGroup> = groups
                    .iter()
                    .map(|(label, rect)| MapGroup { label, rect: *rect })
                    .collect();
                holder_map(
                    p,
                    &HolderMapVm {
                        tiles: &placed,
                        groups: &named,
                        total,
                        ink,
                    },
                    body,
                );
            }
            TradingView::Header | TradingView::HeaderLive => {
                let venues = ["cswap", "splash", "minswap-v2"];
                let col = Rect::new(x, y + 20.0, w.min(350.0), 160.0);
                let logo = self.logo.as_ref().map(|texture| TokenLogo {
                    texture,
                    age: 0.6,
                    paint: &plain_logo,
                });
                let live = self.view == TradingView::HeaderLive;
                let vm = TokenHeaderVm {
                    ticker: "PERP COIN",
                    venues: &venues,
                    pools: 3,
                    hue: p.theme.color.accent_cyan,
                    logo,
                    mode: if live { "LIVE" } else { "REPLAY · 24H" },
                    speed: if live { "1D/S" } else { "1H/S" },
                    speed_lit: live,
                    paused: false,
                    settings_open: false,
                    ink,
                };
                token_header(p, p, &vm, col);
            }
            TradingView::Treemap => {
                let area = Rect::new(
                    x,
                    y + 20.0,
                    w.min(620.0),
                    (area.h - 40.0).clamp(180.0, 330.0),
                );
                let items: Vec<(String, f64)> = (0..12)
                    .map(|i| (format!("tile {i}"), 120.0 - i as f64 * 8.0))
                    .collect();
                let laid = self.treemap.layout(&items, area);
                let tiles: Vec<TreemapTile> = laid
                    .iter()
                    .enumerate()
                    .map(|(i, (key, rect))| TreemapTile {
                        rect: *rect,
                        label: key,
                        share: Some("8.2%"),
                        fill: p.theme.series.nth(i),
                        flash: None,
                    })
                    .collect();
                let groups = [TreemapGroup {
                    rect: area,
                    label: "ALL WEIGHTS",
                }];
                treemap(
                    p,
                    &TreemapVm {
                        tiles: &tiles,
                        groups: &groups,
                    },
                    area,
                );
            }
        }
    }
}

/// Borrow the story's owned holdings for a widget.
fn borrowed(holdings: &[OwnedHolding]) -> Vec<Holding<'_>> {
    holdings
        .iter()
        .map(|h| Holding {
            key: &h.key,
            label: &h.label,
            kind: h.kind,
            amount: h.amount,
        })
        .collect()
}

/// A plain square logo for the header story. The arena rounds its own with
/// gamekit; a story only has to show the fade-in over the initial.
fn plain_logo(texture: &Texture2D, rect: Rect, alpha: f32) {
    // Inset so a square texture sits inside the 36-unit disc rather than over
    // its rim. The arena's `RoundedArt` clips to the circle instead.
    let inner = Rect::new(rect.x + 6.0, rect.y + 6.0, rect.w - 12.0, rect.h - 12.0);
    draw_texture_ex(
        texture,
        inner.x,
        inner.y,
        Color::new(1.0, 1.0, 1.0, alpha),
        DrawTextureParams {
            dest_size: Some(vec2(inner.w, inner.h)),
            ..Default::default()
        },
    );
}

fn logo_texture() -> Texture2D {
    const N: u32 = 32;
    let mut image = Image::gen_image_color(N as u16, N as u16, Color::new(0.2, 0.4, 0.8, 1.0));
    for y in 0..N {
        for x in 0..N {
            let on = ((x / 4) * 7 + (y / 4) * 13) % 5 < 2;
            let c = if on {
                Color::new(0.55, 0.78, 0.98, 1.0)
            } else {
                Color::new(0.18, 0.28, 0.52, 1.0)
            };
            image.set_pixel(x, y, c);
        }
    }
    Texture2D::from_image(&image)
}

/// A deterministic price path: an early climb, a plateau, a drawdown and a
/// recovery. A trade every fourth point, so the marks and volume show at the
/// width the story draws.
fn points() -> Vec<ChartPoint> {
    (0..96)
        .map(|i| {
            let i_f = i as f64;
            let wave = 0.000_12 * (i_f / 9.0).sin() + 0.000_04 * (i_f / 3.7).sin();
            let price = (0.000_42 + i_f * 0.000_004 + wave).max(0.000_01);
            let trade = (i % 4 == 0).then(|| {
                let ada = 30.0 + ((i * 37) % 480) as f64;
                TradeMark {
                    side: if (i / 4) % 3 == 0 {
                        TradeSide::Sell
                    } else {
                        TradeSide::Buy
                    },
                    size: if ada > 400.0 {
                        TradeSize::Whale
                    } else if ada > 160.0 {
                        TradeSize::Big
                    } else {
                        TradeSize::Small
                    },
                    ada: Some(ada),
                }
            });
            ChartPoint {
                unix: T0 + i as u64 * STEP,
                price,
                trade,
            }
        })
        .collect()
}

fn buckets(points: &[ChartPoint]) -> Vec<LifeBucket> {
    points
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| {
            let lo = pair.iter().map(|p| p.price).fold(f64::MAX, f64::min);
            let hi = pair.iter().map(|p| p.price).fold(f64::MIN, f64::max);
            let close = pair.last().map_or(lo, |p| p.price);
            let txs = pair.iter().filter(|p| p.trade.is_some()).count() as u64 * 17 + 3;
            LifeBucket {
                unix: pair[0].unix,
                bucket_secs: STEP * 2,
                lo,
                hi,
                close,
                txs,
                seen: i % 17 != 0,
            }
        })
        .collect()
}

fn holdings() -> Vec<OwnedHolding> {
    let raw = [
        ("$perpstaking", HoldingKind::Person(6), 215_000.0),
        ("burn address", HoldingKind::Burn, 135_000.0),
        ("$nextquest", HoldingKind::Person(5), 39_000.0),
        ("q9el..erfa", HoldingKind::Person(4), 28_000.0),
        ("vault.ada", HoldingKind::Lock, 24_000.0),
        ("[cswap]", HoldingKind::Venue, 18_000.0),
        ("$shield.tools", HoldingKind::Lock, 15_000.0),
        ("stake.reef", HoldingKind::Staking, 12_000.0),
        ("whale.ada", HoldingKind::Person(5), 10_000.0),
        ("shark.ada", HoldingKind::Person(4), 7_500.0),
        ("script:44f1", HoldingKind::Contract, 6_000.0),
        ("dolphin.ada", HoldingKind::Person(3), 5_000.0),
        ("fish.ada", HoldingKind::Person(2), 3_000.0),
        ("in the pools", HoldingKind::Pool, 121_500.0),
    ];
    let mut out: Vec<OwnedHolding> = raw
        .into_iter()
        .map(|(label, kind, amount)| OwnedHolding {
            key: label.to_string(),
            label: label.to_string(),
            kind,
            amount,
        })
        .collect();
    out.sort_by(|a, b| b.amount.total_cmp(&a.amount).then(a.key.cmp(&b.key)));
    out
}

fn rows() -> Vec<OwnedRow> {
    let raw = [
        (
            "BUY",
            "q9el..erfa",
            true,
            "bought 1,696,645 PERP",
            "+1.70M",
            "12:40",
        ),
        (
            "SELL",
            "shark.ada",
            true,
            "sold 82,100 PERP",
            "-82.1K",
            "12:38",
        ),
        (
            "MINT",
            "$perpstaking",
            true,
            "supply minted",
            "+2.0M",
            "12:31",
        ),
        ("LIQ", "[cswap]", false, "liquidity added", "+2.0K", "12:20"),
        (
            "MOVE",
            "vault.ada to whale.ada",
            true,
            "wallet to wallet",
            "18.0K",
            "12:04",
        ),
    ];
    raw.into_iter()
        .map(|(chip, title, handle, sub, right, time)| {
            let tone = match chip {
                "BUY" | "MINT" => Color::new(0.29, 0.87, 0.50, 1.0),
                "SELL" => Color::new(0.98, 0.36, 0.48, 1.0),
                "LIQ" => Color::new(0.18, 0.83, 0.75, 1.0),
                _ => Color::new(0.65, 0.55, 0.98, 1.0),
            };
            OwnedRow {
                chip: chip.to_string(),
                tone,
                title: title.to_string(),
                title_is_handle: handle,
                sub: sub.to_string(),
                right: right.to_string(),
                right_tone: tone,
                time: time.to_string(),
            }
        })
        .collect()
}
