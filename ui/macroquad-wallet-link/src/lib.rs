//! `macroquad-wallet-link` — the Discord wallet-link manager, on macroquad.
//!
//! The wallets linked to the reader's Discord account are moons round their
//! home planet; tapping one opens its details and a way to unlink it, and
//! "Link a wallet" opens the shared wallet modal, connects, and proves the
//! wallet through auth-service's signed challenge. A List view shows the same
//! wallets as rows. See `SPEC.md` for the numbers.
//!
//! ```ignore
//! // A gamekit screen that lets its players manage their links in place:
//! let mut linker = WalletLinker::live(DEFAULT_AUTH_BASE, token);
//! // each frame, in `Game::draw`:
//! if let Some(LinkerEvent::Linked(w)) = linker.show(frame, &painter, rect) {
//!     // refresh whatever this screen derives from the reader's wallets
//! }
//! ```
//!
//! ## Who owns what
//!
//! - **The flow** (`wallet-link-flow`, pure) decides: what to ask auth-service,
//!   what to ask the wallet, what to tell the reader.
//! - **[`LinkIo`]** runs those effects: [`LiveIo`] over miniquad's HTTP and
//!   wallet plugins, [`demo::DemoIo`] over nothing at all.
//! - **The connector** (`macroquad-wallet-flow`'s [`WalletConnector`], the
//!   gamekit's wallet modal) connects the wallet, and shows the proof step
//!   in its own words ([`ProofWords::LINK`]).
//! - **This crate** draws, and carries outcomes between the three.
//!
//! The page needs `wallet.js` and `evm.js` (wallet-miniquad), `platform.js`
//! (miniquad-platform) and `app.js` (macroquad-app); this crate's `build.rs`
//! stamps all four for its own app.

pub mod demo;
mod io;
mod list;
mod orbit;
mod planet;

use chains::ChainFamily;
use macroquad::prelude::*;
use macroquad_app::{Insets, Toasts, Tone};
use macroquad_gamekit::wallet_modal::{
    Account, ProofWords, SignInPhase, WalletModalAction, short_address,
};
use macroquad_gamekit::{Animated, Ease, Frame, Starfield};
use macroquad_wallet_flow::{CardanoHost, EvmHost, WalletConnector, WalletFlow, copy_text};
use macroquad_widgets::{
    Align, COPIED_SECS, Gradient, Line, Message, Painter, Pill, Sheet, SheetForm, SheetState,
    TextSize, Trailing, WellRow, fill_gradient, pill_row, segmented, sheet_card, sheet_chrome,
    well, well_height, well_value_width,
};
use wallet_link_flow::wallet_link::LinkedWallet;
use wallet_link_flow::{Claim, Effect, Event, LINKABLE, LinkStep, Linker, Phase, TokenClaims};

pub use io::{LinkIo, LiveIo, signature_event};
pub use list::{ListLook, ListView};
pub use orbit::{Identity, OrbitTap, OrbitView, short_tail, wallet_label};
pub use wallet_link_flow;
pub use wallet_link_flow::wallet_link::link::DEFAULT_AUTH_BASE;

/// The label `cardano_account` writes a Cardano account's reward address
/// under: what a Cardano link claims.
pub const STAKE_DETAIL: &str = "Stake address";

/// Seconds the modal shows a finished link before it closes and the moon
/// launches.
const CELEBRATE_SECS: f32 = 1.4;
const VIEW_FADE_SECS: f32 = 0.35;
const GUTTER: f32 = 20.0;
const HEADER_H: f32 = 56.0;
const CTA_H: f32 = 52.0;
const CTA_MAX_W: f32 = 360.0;
const TOGGLE_W: f32 = 132.0;
const TOGGLE_H: f32 = 32.0;

/// What a host may want to know about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkerEvent {
    Linked(LinkedWallet),
    Unlinked(LinkedWallet),
}

/// Orbit or list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Orbit,
    List,
}

/// See the [crate docs](crate).
pub struct WalletLinker {
    flow: Linker,
    io: Box<dyn LinkIo>,
    connector: WalletConnector,
    claims: TokenClaims,
    expired_hint: String,
    insets: Insets,
    started: bool,
    clock: f32,
    sky: Starfield,

    view: View,
    /// 0 orbit, 1 list.
    view_mix: Animated<f32>,
    orbit: OrbitView,
    list: ListView,

    /// The wallet whose sheet is open, kept while the sheet slides away.
    selected: Option<String>,
    sheet: SheetState,
    copied: Option<(String, f32)>,

    /// The account claimed in this link, so it is claimed once.
    claimed: Option<String>,
    /// Seconds the finished link has shown.
    celebrated: f32,
    /// A link just made, held back from the orbit until the modal has gone.
    held: Option<LinkedWallet>,
    /// The primary button's fade: out while the modal is up.
    cta_alpha: Animated<f32>,
    /// Let go of the connected wallets once the modal has finished closing.
    release_after_close: bool,
    /// Launch this address's moon on the next sync.
    launch: Option<String>,
    modal_was_open: bool,
    toasts: Toasts,
    events: Vec<LinkerEvent>,
    /// The reader's Discord avatar on its way, for the sun's face.
    avatar: Option<macroquad_app::Pending>,
}

impl WalletLinker {
    /// Over `io`, connecting through `connector`, greeting the holder of
    /// `claims`. The connector is given the modal's link words.
    pub fn new(io: impl LinkIo + 'static, connector: WalletConnector, claims: TokenClaims) -> Self {
        Self {
            flow: Linker::new(),
            io: Box::new(io),
            connector: connector.proof_words(ProofWords::LINK),
            claims,
            expired_hint: wallet_link_flow::words::EXPIRED.to_string(),
            insets: Insets::default(),
            started: false,
            clock: 0.0,
            sky: Starfield::new(7, 140).drift(vec2(-4.0, 1.0)).size(2.0),
            view: View::Orbit,
            view_mix: Animated::new(0.0),
            orbit: OrbitView::new(),
            list: ListView::new(),
            selected: None,
            sheet: SheetState::new(),
            copied: None,
            claimed: None,
            celebrated: 0.0,
            held: None,
            cta_alpha: Animated::new(1.0),
            release_after_close: false,
            launch: None,
            modal_was_open: false,
            toasts: Toasts::new(),
            events: Vec::new(),
            avatar: None,
        }
    }

    /// The reader's Discord identity on the sun: their name from the token
    /// (or their id, once `/auth/me` has answered), and their avatar once it
    /// has loaded.
    fn refresh_identity(&mut self) {
        if !self.started {
            // The first frame: ask for the avatar the token names.
            if let Some(url) = self.claims.avatar_url() {
                self.avatar = Some(macroquad_app::Request::get(&url).send());
            }
        }
        let name = self.claims.display_name.clone().or_else(|| {
            self.flow
                .identity()
                .map(|i| format!("Discord {}", short_tail(&i.discord_user_id)))
        });
        let mut identity = self.orbit.identity().clone();
        let mut changed = identity.name != name;
        identity.name = name;
        if let Some(pending) = &mut self.avatar
            && let std::task::Poll::Ready(reply) = pending.poll()
        {
            self.avatar = None;
            // A missing or unreadable avatar leaves the sun its initial.
            if let Ok(r) = reply
                && r.ok()
                && let Ok(image) = Image::from_file_with_format(&r.body, None)
            {
                let texture = Texture2D::from_image(&image);
                texture.set_filter(FilterMode::Linear);
                identity.avatar = Some(texture);
                changed = true;
            }
        }
        if changed {
            self.orbit.set_identity(identity);
        }
    }

    /// auth-service at `base` with `token`, and the browser's Cardano and EVM
    /// wallets.
    pub fn live(base: &str, token: &str) -> Self {
        let claims = TokenClaims::peek(token).unwrap_or_default();
        Self::new(LiveIo::new(base, token), live_connector(), claims)
    }

    /// The stand-in service and wallets of [`demo`]: nothing real.
    pub fn demo() -> Self {
        Self::demo_with(demo::demo_linked())
    }

    /// The demo, starting with `wallets` linked.
    pub fn demo_with(wallets: Vec<LinkedWallet>) -> Self {
        let connector = WalletConnector::new()
            .host(demo::DemoChain::cardano())
            .host(demo::DemoChain::evm());
        Self::new(
            demo::DemoIo::new(wallets),
            link_copy(connector),
            demo::demo_claims(),
        )
    }

    /// What to say when the token is refused: how this host's reader gets a
    /// new one. The `/link-wallet` funnel's words by default.
    pub fn expired_hint(mut self, hint: impl Into<String>) -> Self {
        self.expired_hint = hint.into();
        self
    }

    /// The phone's safe area, for a component drawn edge to edge.
    pub fn set_insets(&mut self, insets: Insets) {
        self.insets = insets;
    }

    /// Show the orbit or the list.
    pub fn set_view(&mut self, view: View) {
        if view != self.view {
            self.view = view;
            let to = if view == View::List { 1.0 } else { 0.0 };
            self.view_mix.to(to, VIEW_FADE_SECS, Ease::CubicOut);
        }
    }

    /// Where the flow is.
    pub fn phase(&self) -> Phase {
        self.flow.phase()
    }

    /// The linked wallets, once loaded.
    pub fn wallets(&self) -> Option<&[LinkedWallet]> {
        self.flow.wallets()
    }

    /// The connector, for a host or a script driving it without a pointer.
    pub fn connector_mut(&mut self) -> &mut WalletConnector {
        &mut self.connector
    }

    /// Tell the flow something, as a tap would: for a script or a capture
    /// plan.
    pub fn send(&mut self, event: Event) {
        let effects = self.flow.handle(event);
        for e in effects {
            self.apply(e);
        }
    }

    /// Open a wallet's sheet, as a tap on its moon does.
    pub fn select(&mut self, address: &str) {
        self.selected = Some(address.to_string());
        self.sheet.open();
        self.orbit.pause(true);
    }

    /// Whether the screen is at rest: loaded, nothing moving, nothing open.
    /// What a capture waits for.
    pub fn settled(&self) -> bool {
        !matches!(self.flow.phase(), Phase::Loading) && self.orbit.settled()
    }

    /// Advance by `frame.dt`: the replies and signatures that arrived, the
    /// connector's wallets, the motion. Call every frame, from `Game::update`
    /// (the kit's capture loop runs updates without draws on its way to a
    /// moment). Returns what happened, for a host that cares.
    pub fn update(&mut self, frame: &Frame) -> Option<LinkerEvent> {
        let dt = frame.dt;
        self.clock += dt;
        self.refresh_identity();
        if !self.started {
            self.started = true;
            self.send(Event::Start);
        }
        for event in self.io.poll() {
            self.send(event);
        }
        self.connector.poll();
        self.view_mix.update(dt);
        self.sheet.update(dt);
        self.toasts.update(dt);
        if let Some((_, age)) = &mut self.copied {
            *age += dt;
            if *age > COPIED_SECS {
                self.copied = None;
            }
        }
        self.drive_connector(dt);

        // The orbit shows the list as it is, less a link the modal is still
        // celebrating: that moon launches once the modal has gone.
        if self.flow.wallets().is_some() {
            let shown = self.shown_wallets();
            let launch = self.launch.take();
            self.orbit.sync(&shown, launch.as_deref());
        }
        self.orbit.update(dt);
        let cta_to = if self.connector.is_visible() {
            0.0
        } else {
            1.0
        };
        if self.cta_alpha.target() != cta_to {
            self.cta_alpha.to(cta_to, 0.25, Ease::CubicOut);
        }
        self.cta_alpha.update(dt);
        if self.events.is_empty() {
            None
        } else {
            Some(self.events.remove(0))
        }
    }

    /// Draw into `area` and act on this frame's taps. Call every frame after
    /// [`Self::update`], from `Game::draw`.
    pub fn show(&mut self, frame: &Frame, p: &Painter, area: Rect) {
        let dt = frame.dt;
        let c = p.theme.color;
        let safe = self.insets.safe(area);
        // The sky, edge to edge.
        fill_gradient(
            area,
            0.0,
            Gradient::Vertical {
                top: c.bg_primary,
                bottom: planet::mix(c.bg_primary, c.accent, 0.07),
            },
        );
        self.sky.tint = Color::new(c.text_primary.r, c.text_primary.g, c.text_primary.b, 0.8);
        self.sky.draw(area, self.clock);

        let header = Rect::new(
            safe.x + GUTTER,
            safe.y + 14.0,
            safe.w - 2.0 * GUTTER,
            HEADER_H,
        );
        let cta_w = (safe.w - 2.0 * GUTTER).min(CTA_MAX_W);
        let cta = Rect::new(
            safe.center().x - cta_w * 0.5,
            safe.bottom() - 16.0 - CTA_H,
            cta_w,
            CTA_H,
        );
        let field = Rect::new(
            safe.x,
            header.bottom() + 8.0,
            safe.w,
            (cta.y - 12.0 - header.bottom() - 8.0).max(0.0),
        );

        let covered = self.connector.is_visible() || self.sheet.visible();
        let untapped = p.untapped();
        let body: &Painter = if covered { &untapped } else { p };
        let phase = self.flow.phase();

        self.draw_header(body, header, &phase);

        match &phase {
            Phase::Unreachable(reason) => {
                self.orbit_only(body, field, 0.35);
                if Message::new("Couldn't load your wallets", reason)
                    .action("Try again")
                    .show_in(body, field)
                {
                    self.send(Event::Retry);
                }
            }
            Phase::Expired => {
                self.orbit_only(body, field, 0.35);
                let hint = self.expired_hint.clone();
                Message::new("Session expired", &hint).show_in(body, field);
            }
            Phase::Loading => {
                let _ = self.orbit.show(body, field, None, 1.0, true);
                let note = Rect::new(field.x, field.bottom() - 40.0, field.w, 24.0);
                body.text_in(
                    "Loading your wallets",
                    note,
                    body.size(TextSize::Md),
                    c.text_muted,
                    Align::Center,
                );
            }
            _ => self.draw_body(frame, body, field, covered),
        }

        // The one primary action.
        let idle = matches!(phase, Phase::Idle);
        let cta_a = self.cta_alpha.get();
        if cta_a > 0.01
            && Pill::filled("Link a wallet")
                .ink(c.accent)
                .enabled(idle)
                .alpha(cta_a)
                .show(body, cta)
            && idle
        {
            self.close_sheet();
            self.send(Event::Link(None));
        }

        // The sheet, under the modal.
        let sheet_p: &Painter = if self.connector.is_visible() {
            &untapped
        } else {
            p
        };
        self.draw_sheet(sheet_p, area, &phase);

        self.toasts.show(p, cta.x, cta.w, cta.y - 10.0);

        // The connector's modal, on top of everything. Polled in `update`.
        if let Some(action) = self.connector.draw(p, area, dt) {
            self.on_modal(action);
        }
    }

    // ------------------------------------------------------------------------
    // The flow and the connector
    // ------------------------------------------------------------------------

    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::Call(call) => self.io.call(&call),
            Effect::Sign(request) => self.io.sign(&request),
            Effect::OpenConnector(family) => {
                self.release_after_close = false;
                self.release_wallets();
                self.claimed = None;
                match family {
                    Some(f) => self.connector.open_on(f),
                    None => self.connector.open(),
                }
                self.modal_was_open = true;
            }
            Effect::Linked(wallet) => {
                // Said once the modal has gone and the moon is on its way:
                // under the modal it would be read by no one.
                self.held = Some(wallet.clone());
                self.events.push(LinkerEvent::Linked(wallet));
            }
            Effect::Unlinked(wallet) => {
                self.close_sheet();
                self.toasts
                    .push(format!("Unlinked {}", wallet_label(&wallet)), Tone::Info);
                self.events.push(LinkerEvent::Unlinked(wallet));
            }
            Effect::Notice(text) => self.toasts.push(text, Tone::Warning),
        }
    }

    /// Let a link the modal was celebrating into the orbit: its moon
    /// launches, and the toast says so.
    fn release_held(&mut self) {
        if let Some(wallet) = self.held.take() {
            self.toasts
                .push(format!("Linked {}", wallet_label(&wallet)), Tone::Success);
            self.launch = Some(wallet.address);
        }
    }

    /// Let go of every connected wallet: each link starts from the chain step,
    /// not from the wallet linked last.
    fn release_wallets(&mut self) {
        let families: Vec<ChainFamily> = self.connector.connected().map(|(f, ..)| f).collect();
        for f in families {
            self.connector.apply(&WalletModalAction::Disconnect(f));
        }
    }

    /// Carry outcomes between the connector and the flow, once a frame.
    fn drive_connector(&mut self, dt: f32) {
        // A wallet connected while the flow waits for one: claim it.
        if matches!(
            self.flow.phase(),
            Phase::Link {
                step: LinkStep::Connect,
                ..
            }
        ) && let Some(claim) = self.connected_claim()
            && self.claimed.as_deref() != Some(claim.address())
        {
            self.claimed = Some(claim.address().to_string());
            self.send(Event::Claim(claim));
        }

        // A finished link: shown in the modal for a moment, then the moon.
        let finished = matches!(
            self.flow.phase(),
            Phase::Link {
                step: LinkStep::Linked { .. } | LinkStep::Already { .. },
                ..
            }
        );
        if finished {
            self.celebrated += dt;
            if self.celebrated >= CELEBRATE_SECS {
                self.celebrated = 0.0;
                self.connector.close();
                self.release_after_close = true;
                if let Phase::Link {
                    step: LinkStep::Already { address },
                    ..
                } = self.flow.phase()
                {
                    // Nothing new: the moon that already was this wallet
                    // flares, so the reader sees which it is.
                    self.orbit.flare(&address);
                }
                self.release_held();
                self.send(Event::Done);
            }
        } else {
            self.celebrated = 0.0;
        }

        // The reader closed the modal mid-link.
        let open = self.connector.modal_mut().is_open();
        if self.modal_was_open && !open && matches!(self.flow.phase(), Phase::Link { .. }) {
            self.release_after_close = true;
            self.release_held();
            self.send(Event::ConnectorClosed);
        }
        self.modal_was_open = open;
        // The wallet is let go once the card has left: a closing card still
        // draws its last step, and that step needs its account.
        if self.release_after_close && !self.connector.is_visible() {
            self.release_after_close = false;
            self.release_wallets();
        }

        // The modal's proof step follows the flow.
        let phase = self.flow.phase();
        for family in LINKABLE {
            let name = self.wallet_name(family);
            let connected = self.connector.connected().any(|(f, ..)| f == family);
            let sign_in = proof_phase(&phase, family, connected, &name);
            self.connector.set_sign_in(family, sign_in);
        }
    }

    /// The first connected account the flow can link, as a claim.
    fn connected_claim(&self) -> Option<Claim> {
        self.connector
            .connected()
            .find_map(|(family, _, account)| claim_of(family, account))
    }

    fn wallet_name(&self, family: ChainFamily) -> String {
        self.connector
            .connected()
            .find(|(f, ..)| *f == family)
            .and_then(|(f, key, _)| {
                self.connector
                    .vm()
                    .chain(f)
                    .map(|c| c.wallet_name(key).to_string())
            })
            .unwrap_or_else(|| "your wallet".to_string())
    }

    fn on_modal(&mut self, action: WalletModalAction) {
        if let WalletModalAction::SignIn(_) = action {
            match self.flow.phase() {
                Phase::Link {
                    step: LinkStep::Declined | LinkStep::Failed(_),
                    ..
                } => self.send(Event::Retry),
                Phase::Link {
                    step: LinkStep::Connect,
                    ..
                } => {
                    if let Some(claim) = self.connected_claim() {
                        self.claimed = Some(claim.address().to_string());
                        self.send(Event::Claim(claim));
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------------
    // Drawing
    // ------------------------------------------------------------------------

    fn draw_header(&mut self, p: &Painter, header: Rect, phase: &Phase) {
        let c = p.theme.color;
        let loaded = !matches!(
            phase,
            Phase::Loading | Phase::Unreachable(_) | Phase::Expired
        );
        let toggle = Rect::new(
            header.right() - TOGGLE_W,
            header.y + 4.0,
            TOGGLE_W,
            TOGGLE_H,
        );
        let text_w = if loaded {
            header.w - TOGGLE_W - 12.0
        } else {
            header.w
        };
        let title = "Linked wallets";
        let who = self
            .claims
            .display_name
            .clone()
            .or_else(|| {
                self.flow
                    .identity()
                    .map(|i| format!("Discord {}", short_address(&i.discord_user_id)))
            })
            .unwrap_or_else(|| "your Discord account".into());
        let sub = match (phase, self.flow.wallets()) {
            (Phase::Loading, _) => "Loading".to_string(),
            (_, Some(ws)) if ws.len() == 1 => format!("1 wallet linked to {who}"),
            (_, Some(ws)) => format!("{} wallets linked to {who}", ws.len()),
            _ => format!("Linked to {who}"),
        };
        let title_size = p.size(TextSize::Xl2);
        let sub_size = p.size(TextSize::Md);
        p.stack_in(
            &[
                Line::new(
                    &p.fit(title, title_size, text_w),
                    title_size,
                    c.text_primary,
                ),
                Line::new(&p.fit(&sub, sub_size, text_w), sub_size, c.text_muted),
            ],
            Rect::new(header.x, header.y, text_w, header.h),
            Align::Start,
            6.0,
        );
        if loaded {
            let picked = match self.view {
                View::Orbit => 0,
                View::List => 1,
            };
            if let Some(i) = segmented(p, toggle, &["Orbit", "List"], picked, true) {
                self.set_view(if i == 0 { View::Orbit } else { View::List });
            }
        }
    }

    /// The orbit with no list and no taps: behind a message.
    fn orbit_only(&mut self, p: &Painter, field: Rect, fade: f32) {
        let _ = self.orbit.show(&p.untapped(), field, None, fade, false);
    }

    /// The list as the screen shows it, less a link the modal is still
    /// celebrating: that moon launches once the modal has gone.
    fn shown_wallets(&self) -> Vec<LinkedWallet> {
        self.flow
            .wallets()
            .unwrap_or_default()
            .iter()
            .filter(|w| self.held.as_ref().is_none_or(|h| h.address != w.address))
            .cloned()
            .collect()
    }

    /// The orbit and the list, cross-fading. `covered` is a sheet or the modal
    /// over them: they draw, and take no input.
    fn draw_body(&mut self, frame: &Frame, p: &Painter, field: Rect, covered: bool) {
        let shown = self.shown_wallets();
        let mix = self.view_mix.get();
        let selected = if self.sheet.is_open() {
            self.selected.clone()
        } else {
            None
        };
        let untapped = p.untapped();
        let orbit_p: &Painter = if mix < 0.5 { p } else { &untapped };
        if mix < 0.999 {
            match self
                .orbit
                .show(orbit_p, field, selected.as_deref(), 1.0 - mix, false)
            {
                Some(OrbitTap::Moon(address)) => self.select(&address),
                // A ghost planet asks for its first wallet.
                Some(OrbitTap::Planet {
                    family,
                    empty: true,
                }) if matches!(self.flow.phase(), Phase::Idle) => {
                    self.close_sheet();
                    self.send(Event::Link(Some(family)));
                }
                Some(OrbitTap::Planet { .. } | OrbitTap::Sun | OrbitTap::Sky) => self.close_sheet(),
                None => {}
            }
        }
        if mix > 0.001 {
            let live = mix >= 0.5 && !covered;
            let list_p: &Painter = if live { p } else { &untapped };
            let look = ListLook {
                selected: selected.as_deref(),
                fade: mix,
                live,
            };
            if let Some(address) = self.list.show(frame, list_p, field, &shown, look) {
                self.select(&address);
            }
        }
    }

    fn close_sheet(&mut self) {
        if self.sheet.is_open() {
            self.sheet.close();
            self.orbit.pause(false);
            if matches!(self.flow.phase(), Phase::Confirming(_)) {
                self.send(Event::KeepWallet);
            }
        }
    }

    fn draw_sheet(&mut self, p: &Painter, area: Rect, phase: &Phase) {
        if !self.sheet.visible() {
            if !self.sheet.is_open() {
                self.selected = None;
            }
            return;
        }
        let Some(address) = self.selected.clone() else {
            return;
        };
        let Some(wallet) = self
            .flow
            .wallets()
            .and_then(|ws| ws.iter().find(|w| w.address == address))
            .cloned()
        else {
            // Gone from the list (unlinked): let the sheet go.
            self.close_sheet();
            return;
        };
        let c = p.theme.color;
        let family = wallet_link_flow::family_of(&wallet);
        let tint = orbit::moon_tint(p, family);
        let label = wallet_label(&wallet);
        let chain = family.map_or("Unknown chain", |f| f.display_name());
        let confirming =
            matches!(phase, Phase::Confirming(w) | Phase::Unlinking(w) if w.address == address);
        let unlinking = matches!(phase, Phase::Unlinking(w) if w.address == address);

        let form = SheetForm::for_area(area);
        let sheet = Sheet::new().title(&label).subtitle(chain).tint(tint);
        let content_w = macroquad_widgets::sheet_content_width(form, area);
        let copied = |value: &str| {
            self.copied
                .as_ref()
                .filter(|(v, _)| v == value)
                .map(|(_, age)| *age)
        };
        let mono = p.size(TextSize::Md);
        let addr_shown = fit_middle(
            p,
            &wallet.address,
            mono,
            well_value_width(content_w, Trailing::Copy { copied: None }),
        );
        let handle = wallet.handle.as_ref().map(|_| wallet_label(&wallet));
        // The chain is the sheet's subtitle; the well holds what is copied.
        let mut rows = Vec::new();
        if let Some(h) = &handle {
            rows.push(
                WellRow::stacked("ADA Handle", h).trailing(Trailing::Copy { copied: copied(h) }),
            );
        }
        rows.push(
            WellRow::stacked("Address", &addr_shown)
                .mono()
                .trailing(Trailing::Copy {
                    copied: copied(&wallet.address),
                }),
        );
        let well_h = well_height(p, &rows);
        let note = format!(
            "This removes the link between {label} and your Discord account. You can link it again any time."
        );
        let note_size = p.size(TextSize::Md);
        let note_lines = if confirming {
            p.wrap(&note, note_size, content_w)
        } else {
            Vec::new()
        };
        let line_h = note_size * 1.35;
        let note_h = if confirming {
            14.0 + note_lines.len() as f32 * line_h
        } else {
            0.0
        };
        let height = sheet_chrome(form, area, &sheet) + well_h + note_h;
        let Some(card) = sheet_card(p, &self.sheet, area, height, &sheet) else {
            return;
        };
        if card.dismissed {
            self.close_sheet();
        }
        let panel = Rect::new(card.content.x, card.content.y, card.content.w, well_h);
        let answer = well(p, panel, &rows, card.live, card.alpha);
        if let Some(i) = answer.copy {
            // The rows are the handle (when there is one), then the address.
            let value = match (&handle, i) {
                (Some(h), 0) => h.clone(),
                _ => wallet.address.clone(),
            };
            copy_text(&value);
            self.copied = Some((value, 0.0));
        }
        if confirming {
            let mut y = panel.bottom() + 14.0;
            for line in &note_lines {
                p.text_in(
                    line,
                    Rect::new(card.content.x, y, card.content.w, line_h),
                    note_size,
                    alpha(c.text_secondary, card.alpha),
                    Align::Start,
                );
                y += line_h;
            }
        }

        // The footer: Unlink, then Keep · Unlink to confirm.
        if confirming {
            let rects = pill_row(card.footer, 2, 10.0, Some(0.4));
            if Pill::ghost("Keep")
                .enabled(!unlinking)
                .live(card.live)
                .alpha(card.alpha)
                .show(p, rects[0])
            {
                self.send(Event::KeepWallet);
            }
            let label = if unlinking { "Unlinking" } else { "Unlink" };
            if Pill::danger(label)
                .enabled(!unlinking)
                .live(card.live)
                .alpha(card.alpha)
                .show(p, rects[1])
            {
                self.send(Event::ConfirmUnlink);
            }
        } else if Pill::danger("Unlink")
            .enabled(matches!(phase, Phase::Idle))
            .live(card.live)
            .alpha(card.alpha)
            .show(p, card.footer)
        {
            self.send(Event::AskUnlink(address));
        }
    }
}

/// The connector a live page links through: the browser's CIP-30 and
/// EIP-6963 wallets, in the link words.
pub fn live_connector() -> WalletConnector {
    link_copy(
        WalletConnector::new()
            .host(CardanoHost::new(WalletFlow::new()))
            .host(EvmHost::new()),
    )
}

/// The modal's title and intro for linking.
fn link_copy(connector: WalletConnector) -> WalletConnector {
    connector
        .title("Link a wallet")
        .intro("Connect the wallet to link. You'll sign a message to prove it's yours; nothing is sent or spent.")
        .proof_words(ProofWords::LINK)
}

/// What a connected `account` on `family` claims, when the flow can link it.
pub fn claim_of(family: ChainFamily, account: &Account) -> Option<Claim> {
    match family {
        ChainFamily::Cardano => account
            .details
            .iter()
            .find(|d| d.label == STAKE_DETAIL)
            .map(|d| Claim::Cardano {
                stake_address: d.value.clone(),
            }),
        ChainFamily::Evm => Some(Claim::Evm {
            address: account.address.clone(),
        }),
        _ => None,
    }
}

/// The modal's proof step for `family`, from where the flow is. `wallet` is
/// the connected wallet's name, for "Approve the message in Eternl".
pub fn proof_phase(
    phase: &Phase,
    family: ChainFamily,
    connected: bool,
    wallet: &str,
) -> SignInPhase {
    match phase {
        Phase::Link {
            step: LinkStep::Connect,
            ..
        } if connected => SignInPhase::Offered,
        Phase::Link {
            family: Some(f),
            step,
        } if *f == family => match step {
            LinkStep::Connect => SignInPhase::Offered,
            LinkStep::Prepare => SignInPhase::Working {
                step: "Preparing the message to sign.".into(),
                in_wallet: false,
            },
            LinkStep::Sign => SignInPhase::Working {
                step: format!(
                    "Approve the message in {wallet}. Signing proves the wallet is yours and costs nothing."
                ),
                in_wallet: true,
            },
            LinkStep::Check => SignInPhase::Working {
                step: "Checking the signature.".into(),
                in_wallet: false,
            },
            LinkStep::Linked { .. } => SignInPhase::SignedIn {
                detail: Some("Linked to Discord".into()),
            },
            LinkStep::Already { .. } => SignInPhase::SignedIn {
                detail: Some("Already linked".into()),
            },
            LinkStep::Declined => SignInPhase::Declined,
            LinkStep::Failed(message) => SignInPhase::Failed {
                message: message.clone(),
            },
        },
        _ => SignInPhase::NotNeeded,
    }
}

/// `s` shortened in the middle until it fits `w` in the monospace face.
fn fit_middle(p: &Painter, s: &str, size: f32, w: f32) -> String {
    if p.measure_mono(s, size).width <= w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut keep = chars.len().saturating_sub(1) / 2;
    while keep > 4 {
        let shown: String = chars[..keep]
            .iter()
            .chain(['…'].iter())
            .chain(chars[chars.len() - keep..].iter())
            .collect();
        if p.measure_mono(&shown, size).width <= w {
            return shown;
        }
        keep -= 1;
    }
    short_address(s)
}

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modal_follows_each_step_of_a_link() {
        let at = |step| Phase::Link {
            family: Some(ChainFamily::Cardano),
            step,
        };
        assert_eq!(
            proof_phase(
                &at(LinkStep::Declined),
                ChainFamily::Cardano,
                true,
                "Eternl"
            ),
            SignInPhase::Declined
        );
        let SignInPhase::Working { step, in_wallet } =
            proof_phase(&at(LinkStep::Sign), ChainFamily::Cardano, true, "Eternl")
        else {
            panic!("working");
        };
        assert!(in_wallet && step.contains("Eternl"));
        assert_eq!(
            proof_phase(&at(LinkStep::Sign), ChainFamily::Evm, true, "Rabby"),
            SignInPhase::NotNeeded,
            "another chain's step is not this chain's"
        );
        assert_eq!(
            proof_phase(&Phase::Idle, ChainFamily::Cardano, true, "Eternl"),
            SignInPhase::NotNeeded
        );
    }

    /// A Cardano account claims its reward address, read where the shared
    /// account builder writes it.
    #[test]
    fn a_cardano_account_claims_its_stake_address() {
        let hex = format!("01{}{}", "11".repeat(28), "22".repeat(28));
        let address = wallet_miniquad::hex_to_bech32(&hex);
        let stake = wallet_miniquad::stake_address_bech32(&hex);
        let account = macroquad_wallet_flow::cardano_account(&address, stake.as_deref(), 1, None);
        assert_eq!(
            claim_of(ChainFamily::Cardano, &account),
            stake.map(|s| Claim::Cardano { stake_address: s })
        );
        assert_eq!(claim_of(ChainFamily::Solana, &account), None);
    }

    /// The bech32 a link is stored in decodes to the hex `signData` takes:
    /// the flow's decoder agrees with pallas, both ways.
    #[test]
    fn the_flows_bech32_agrees_with_pallas() {
        let stake = demo::demo_stake("33");
        let hex = wallet_link_flow::bech32_to_hex(&stake).expect("decodes");
        assert_eq!(wallet_miniquad::hex_to_bech32(&hex), stake);
    }
}
