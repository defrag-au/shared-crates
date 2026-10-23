//! Stories for `route_graph` and `route_strip`: a buy split across venues, in
//! every shape the widget has to hold and every state a leg goes through.
//!
//! Figures are from a real moment — the $LUMP pools on 2026-10-23, a 34,500
//! ADA Splash pool beside a 4,400 ADA CSWAP one — so the shares, meters and
//! wording are the ones a reader actually meets, not round placeholders.

use macroquad::prelude::*;
use macroquad_widgets::{
    LegState, Painter, ROUTE_STRIP_H, RouteCard, RouteGraphVm, RouteMeter, RouteNode,
    draw_rounded_rect, route_graph, route_graph_height, route_strip,
};
use ui_theme::Token;

/// The phone sheet's content width: where the graph has to fit.
const PHONE_W: f32 = 350.0;
const PANEL_PAD: f32 = 20.0;

/// What a story does over time. The widget is still; a story may step its
/// data through states, as a host would.
#[derive(Clone, Copy)]
pub enum Script {
    /// As given.
    Still,
    /// The whole life of a split buy, looping: planned, sent, the exact part
    /// settled while the order waits for its batcher, then filled.
    Fill,
}

pub struct RouteStory {
    source: RouteNode,
    sink: RouteNode,
    cards: Vec<RouteCard>,
    trailer: String,
    script: Script,
}

/// The route's own price impact, as fastbuy shows it: 5% fills the bar, ticks
/// at 1% and at the 3% warning line, warned past it.
fn impact(pct: f32) -> Option<RouteMeter> {
    Some(RouteMeter {
        fill: (pct / 5.0).min(1.0),
        ticks: vec![0.2, 0.6],
        warn: pct > 3.0,
        label: format!("impact {pct:.1}%"),
    })
}

fn card(label: &str, share: f32, detail: &str, ink: Token, m: Option<RouteMeter>) -> RouteCard {
    RouteCard {
        label: label.to_string(),
        share,
        share_text: format!("{:.1}%", share * 100.0),
        detail: detail.to_string(),
        ink,
        state: LegState::Idle,
        meter: m,
        declined: false,
    }
}

fn ada(figure: &str) -> RouteNode {
    RouteNode {
        name: "ADA".into(),
        figure: figure.into(),
        ink: Token::TextPrimary,
    }
}

fn lump(figure: &str) -> RouteNode {
    RouteNode {
        name: "LUMP".into(),
        figure: figure.into(),
        ink: Token::Accent,
    }
}

impl RouteStory {
    fn new(source: RouteNode, sink: RouteNode, cards: Vec<RouteCard>, trailer: &str) -> Self {
        Self {
            source,
            sink,
            cards,
            trailer: trailer.into(),
            script: Script::Still,
        }
    }

    fn script(mut self, script: Script) -> Self {
        self.script = script;
        self
    }

    /// 5,000 ADA of $LUMP: Splash takes most, a CSWAP order the rest, and
    /// both run past their pools' 2%.
    pub fn split() -> Self {
        Self::new(
            ada("5,000"),
            lump("21.60M"),
            vec![
                card(
                    "Splash",
                    0.892,
                    "4,462 ADA · 19.29M exact",
                    Token::AccentBlue,
                    impact(11.9),
                ),
                card(
                    "CSWAP",
                    0.108,
                    "538 ADA · ~2.31M · min 2.26M",
                    Token::AccentGreen,
                    impact(12.2),
                ),
            ],
            "at least 21.56M LUMP",
        )
    }

    /// 1,000 ADA: CSWAP was priced and the split missed the bar, so it shows
    /// faded with the reason.
    pub fn declined() -> Self {
        let mut ghost = card(
            "CSWAP",
            0.0,
            "adds 0.12% · needs 0.25%",
            Token::AccentGreen,
            impact(2.0),
        );
        ghost.share_text = "0%".into();
        ghost.declined = true;
        Self::new(
            ada("1,000"),
            lump("4.74M"),
            vec![
                card(
                    "Splash",
                    1.0,
                    "1,000 ADA · 4.74M exact",
                    Token::AccentBlue,
                    impact(3.8),
                ),
                ghost,
            ],
            "",
        )
    }

    /// One venue, nothing weighed against it: the plainest graph.
    pub fn single() -> Self {
        Self::new(
            ada("100"),
            lump("488K"),
            vec![card(
                "Splash",
                1.0,
                "100 ADA · 488K exact",
                Token::AccentBlue,
                impact(0.6),
            )],
            "",
        )
    }

    /// Four routes: the compact cards, and a long name that has to give way.
    pub fn four() -> Self {
        Self::new(
            ada("12,500"),
            lump("49.80M"),
            vec![
                card(
                    "Splash",
                    0.71,
                    "8,875 ADA · 35.52M exact",
                    Token::AccentBlue,
                    None,
                ),
                card(
                    "CSWAP",
                    0.16,
                    "2,000 ADA · ~8.11M · min 7.95M",
                    Token::AccentGreen,
                    None,
                ),
                card(
                    "CSWAP (second LUMP/ADA pool)",
                    0.09,
                    "1,125 ADA · ~4.37M · min 4.28M",
                    Token::AccentGreen,
                    None,
                ),
                card(
                    "Minswap",
                    0.04,
                    "500 ADA · ~1.80M · min 1.76M",
                    Token::AccentMagenta,
                    None,
                ),
            ],
            "at least 49.31M LUMP",
        )
    }

    /// The order closed without a fill, and the transaction that failed.
    pub fn ended(failed: bool) -> Self {
        let mut s = Self::split();
        if failed {
            for k in &mut s.cards {
                k.state = LegState::Failed;
                k.detail = format!(
                    "{} · nothing taken",
                    k.detail.split(" · ").next().unwrap_or("")
                );
            }
        } else {
            s.cards[0].state = LegState::Done;
            s.cards[0].detail = "4,462 ADA · 19.29M settled".into();
            s.cards[1].state = LegState::Warn;
            s.cards[1].detail = "538 ADA · closed unfilled".into();
        }
        s
    }

    pub fn filling() -> Self {
        Self::split().script(Script::Fill)
    }

    /// The cards at story time `t`.
    fn at(&self, t: f32) -> Vec<RouteCard> {
        let mut cards = self.cards.clone();
        if let Script::Fill = self.script {
            let phase = t % 9.0;
            let (exact, order, exact_line, order_line) = match phase {
                p if p < 1.5 => (LegState::Idle, LegState::Idle, None, None),
                p if p < 3.0 => (LegState::Pending, LegState::Pending, None, None),
                p if p < 6.0 => (
                    LegState::Done,
                    LegState::Waiting,
                    Some("4,462 ADA · 19.29M settled"),
                    Some("538 ADA · ~2.31M · filling"),
                ),
                _ => (
                    LegState::Done,
                    LegState::Done,
                    Some("4,462 ADA · 19.29M settled"),
                    Some("538 ADA · filled 2.30M"),
                ),
            };
            cards[0].state = exact;
            cards[1].state = order;
            if let Some(l) = exact_line {
                cards[0].detail = l.into();
            }
            if let Some(l) = order_line {
                cards[1].detail = l.into();
            }
        }
        cards
    }

    /// The graph in a sheet-like panel at phone width, the strip in a panel
    /// under it, and a line saying what the script is doing.
    pub fn draw(&self, p: &Painter, x: f32, y: f32, t: f32) {
        let c = &p.theme.color;
        let cards = self.at(t);
        let graph_h = route_graph_height(cards.len());
        let panel = Rect::new(x, y, PHONE_W + PANEL_PAD * 2.0, graph_h + PANEL_PAD * 2.0);
        draw_rounded_rect(panel.x, panel.y, panel.w, panel.h, 18.0, c.bg_secondary);
        let vm = RouteGraphVm {
            source: &self.source,
            sink: &self.sink,
            cards: &cards,
        };
        route_graph(
            p,
            &vm,
            Rect::new(x + PANEL_PAD, y + PANEL_PAD, PHONE_W, graph_h),
        );

        let sy = panel.bottom() + 16.0;
        let strip_panel = Rect::new(x, sy, panel.w, ROUTE_STRIP_H + 32.0);
        draw_rounded_rect(
            strip_panel.x,
            strip_panel.y,
            strip_panel.w,
            strip_panel.h,
            14.0,
            c.bg_secondary,
        );
        route_strip(
            p,
            &cards,
            &self.trailer,
            Rect::new(x + 16.0, sy + 16.0, panel.w - 32.0, ROUTE_STRIP_H),
        );

        let note = match self.script {
            Script::Still => "pipe thickness is share; its strength and dot are state",
            Script::Fill => {
                "planned · sent · exact part settled, order waiting · filled (9 s loop)"
            }
        };
        p.text_top(note, x, strip_panel.bottom() + 14.0, 12.0, c.text_muted);
    }
}
