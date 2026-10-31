//! `wallet-link` — the wallet-link manager as an app of its own: the
//! [`WalletLinker`] full screen, on the fluid stage.
//!
//! ```sh
//! # on the web, opened from /link-wallet (or any page holding a token):
//! #   https://<host>/?token=<jwt>
//! # the stand-in service and wallets, with nothing real behind them:
//! cargo run -p macroquad-wallet-link --bin wallet-link -- --demo
//! cargo run -p macroquad-wallet-link --bin wallet-link -- --demo \
//!     --shot .tmp/link.png --size 390x844 --at 3 --insets 47,0,34,0
//! # the crowded case: twelve wallets linked
//! cargo run -p macroquad-wallet-link --bin wallet-link -- --demo --demo-wallets 12
//! # natively against the real service, with a token from the environment:
//! LINK_TOKEN=<jwt> cargo run -p macroquad-wallet-link --bin wallet-link
//! ```
//!
//! A capture plan's `set` lines (`--plan`) wait until the list has loaded,
//! then apply one a frame: `view list`, `view orbit`, `select N` (open the
//! Nth wallet's sheet), `unlink N` (and ask to confirm it), `confirm`, `link` (open the
//! connector), `chain cardano|evm` and `wallet KEY` (pick in the connector,
//! as a tap would). Each section starts the demo afresh.
//!
//! On the web `?demo` runs the demo. With no token the screen says how to
//! open it.

use std::collections::VecDeque;

use macroquad::prelude::*;
use macroquad_app::Insets;
use macroquad_gamekit::harness;
use macroquad_gamekit::{Backdrop, Frame, Game, Stage};
use macroquad_wallet_link::wallet_link_flow::Event;
use macroquad_wallet_link::{DEFAULT_AUTH_BASE, View, WalletLinker, demo};
use macroquad_widgets::{Align, Message, Painter, TextSize, Theme, fill_rounded};

/// Where the app was opened from, and with what.
#[derive(Clone)]
enum Launch {
    Live(String),
    /// The demo, with this many wallets linked (`None`: its usual two).
    Demo(Option<usize>),
    /// No token: nothing to show but how to get one.
    NoToken,
}

impl Launch {
    fn read() -> Self {
        if harness::switch("--demo") || miniquad_platform::launch_query("demo").is_some() {
            return Self::Demo(harness::flag_num("--demo-wallets"));
        }
        let token = miniquad_platform::launch_query("token")
            .or_else(|| harness::flag("--token"))
            .or_else(|| std::env::var("LINK_TOKEN").ok())
            .filter(|t| !t.trim().is_empty());
        token.map_or(Self::NoToken, Self::Live)
    }

    fn linker(&self) -> Option<WalletLinker> {
        match self {
            Self::Live(token) => Some(WalletLinker::live(DEFAULT_AUTH_BASE, token)),
            Self::Demo(None) => Some(WalletLinker::demo()),
            Self::Demo(Some(n)) => Some(WalletLinker::demo_with(demo::demo_many(*n))),
            Self::NoToken => None,
        }
    }
}

/// A plan's `set` line, checked when it is read and applied once the list
/// has loaded.
#[derive(Clone, Debug)]
enum Command {
    View(View),
    Select(usize),
    Unlink(usize),
    /// Confirm the unlink asked.
    Confirm,
    Link,
    Chain(chains::ChainFamily),
    Wallet(String),
}

impl Command {
    fn parse(line: &str) -> Result<Self, String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let n = |s: &str| {
            s.parse::<usize>()
                .map_err(|_| format!("`{line}`: `{s}` is not a number"))
        };
        Ok(match words[..] {
            ["view", "list"] => Self::View(View::List),
            ["view", "orbit"] => Self::View(View::Orbit),
            ["select", i] => Self::Select(n(i)?),
            ["unlink", i] => Self::Unlink(n(i)?),
            ["confirm"] => Self::Confirm,
            ["link"] => Self::Link,
            ["chain", "cardano"] => Self::Chain(chains::ChainFamily::Cardano),
            ["chain", "evm"] => Self::Chain(chains::ChainFamily::Evm),
            ["wallet", key] => Self::Wallet(key.to_string()),
            _ => {
                return Err(format!(
                    "unknown set line `{line}`: try `view list`, `select 0`, `unlink 0`, `link`, `chain cardano`, `wallet eternl`"
                ));
            }
        })
    }

    fn apply(self, linker: &mut WalletLinker) -> Result<(), String> {
        let nth = |linker: &WalletLinker, i: usize| {
            linker
                .wallets()
                .and_then(|ws| ws.get(i))
                .map(|w| w.address.clone())
                .ok_or_else(|| format!("no wallet {i}"))
        };
        match self {
            Self::View(v) => linker.set_view(v),
            Self::Select(i) => {
                let address = nth(linker, i)?;
                linker.select(&address);
            }
            Self::Unlink(i) => {
                let address = nth(linker, i)?;
                linker.select(&address);
                linker.send(Event::AskUnlink(address));
            }
            Self::Confirm => linker.send(Event::ConfirmUnlink),
            Self::Link => linker.send(Event::Link(None)),
            Self::Chain(f) => linker.connector_mut().choose_chain(f),
            Self::Wallet(key) => linker.connector_mut().choose_wallet(key),
        }
        Ok(())
    }
}

struct App {
    font: Option<Font>,
    mono: Option<Font>,
    theme: Theme,
    launch: Launch,
    linker: Option<WalletLinker>,
    queued: VecDeque<Command>,
}

impl App {
    fn new(font: Option<Font>, mono: Option<Font>) -> Self {
        let launch = Launch::read();
        Self {
            font,
            mono,
            theme: Theme::tokyo_night(),
            linker: launch.linker(),
            launch,
            queued: VecDeque::new(),
        }
    }
}

impl Game for App {
    fn update(&mut self, frame: &Frame) {
        let Some(linker) = &mut self.linker else {
            return;
        };
        let _ = linker.update(frame);
        if linker.wallets().is_some()
            && let Some(command) = self.queued.pop_front()
            && let Err(e) = command.apply(linker)
        {
            eprintln!("plan: {e}");
            std::process::exit(2);
        }
    }

    fn draw(&mut self, frame: &Frame) {
        let area = Rect::new(0.0, 0.0, frame.viewport.size.x, frame.viewport.size.y);
        let insets = Insets::read();
        let p = Painter::new(
            self.font.as_ref(),
            self.mono.as_ref(),
            self.theme,
            frame.input.gesture.tap,
        )
        .pixel_scale(frame.px)
        .at_time(frame.clock as f64);
        let c = p.theme.color;
        clear_background(c.bg_primary);
        let Some(linker) = &mut self.linker else {
            Message::new(
                "Open this from Discord",
                "Run /link-wallet in Discord, then follow its link here to manage the wallets linked to your account.",
            )
            .show_in(&p, insets.safe(area));
            return;
        };
        linker.set_insets(insets);
        linker.show(frame, &p, area);
        if matches!(self.launch, Launch::Demo(_)) {
            // Nothing on these screens is real; say so where it cannot be
            // missed or mistaken for content.
            let safe = insets.safe(area);
            let size = p.size(TextSize::Xs);
            let badge = Rect::new(safe.center().x - 26.0, safe.y + 2.0, 52.0, 16.0);
            fill_rounded(
                badge,
                8.0,
                Color::new(c.warning.r, c.warning.g, c.warning.b, 0.22),
            );
            p.text_in("DEMO", badge, size, c.warning, Align::Center);
        }
    }

    fn backdrop(&self) -> Backdrop {
        let c = self.theme.color;
        Backdrop {
            top: c.bg_primary,
            bottom: c.bg_primary,
            accent: c.accent,
        }
    }

    fn capture_note(&self, frame: &Frame) -> String {
        let phase = self
            .linker
            .as_ref()
            .map_or("no token".to_string(), |l| format!("{:?}", l.phase()));
        format!("t={:.2}s · {phase}", frame.clock)
    }

    fn reset(&mut self) {
        self.linker = self.launch.linker();
        self.queued.clear();
    }

    fn setup(&mut self, line: &str) -> Result<(), String> {
        self.queued.push_back(Command::parse(line)?);
        Ok(())
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Linked wallets".to_string(),
        window_width: 390,
        window_height: 844,
        high_dpi: true,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let font = macroquad_gamekit::load_font(include_bytes!(
        "../../../macroquad-storybook/assets/NotoSans-Bold.ttf"
    ))
    .ok();
    let mono = macroquad_gamekit::load_font(include_bytes!(
        "../../../macroquad-storybook/assets/JetBrainsMono-Regular.ttf"
    ))
    .ok();
    macroquad_gamekit::app::run_on(Stage::Fluid, App::new(font, mono)).await;
}
