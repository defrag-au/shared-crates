//! `WalletModal` — a wallet connector a game opens from one button: which chain, which wallet, then what the wallet answered, on one card that glides between the steps.
//!
//! ## The flow
//!
//! [`wallet_button`] is the one control a screen carries. It says whether a
//! wallet is connected, connecting or neither, and a tap on it is the host's cue
//! to call [`WalletModal::open`]. The modal then asks at most three questions,
//! each skipped when its answer is already known:
//!
//! 1. **Chain** — a tile per family the host lists, in the host's order. A
//!    family on the roadmap with no bridge yet is listed as [`Support::Soon`]:
//!    shown, dimmed, and not tappable, so the roadmap is visible without
//!    offering a dead end. Skipped when the host lists one chain, or opens the
//!    modal with [`WalletModal::open_on`].
//! 2. **Wallet** — a row per detected wallet on that chain, with its own icon
//!    when the host decoded one and a monogram tile when it did not.
//! 3. **Answer** — the wallet has been asked: waiting, connected (with the
//!    address it answered with), declined, or failed.
//!
//! ## Who owns what
//!
//! The host owns the connection. The modal reports [`WalletModalAction`]s and
//! the host applies them to whatever bridge that chain has, then writes the
//! outcome back as the chain's [`ChainPhase`] in the [`WalletModalVm`]. The
//! modal owns only what it takes to *show* the flow: which step it is on, and
//! the motion between steps. That split is what lets one modal front Cardano's
//! CIP-30, an EVM provider and whatever comes next, and what lets every story
//! drive it with a fake host.
//!
//! A host that sets a chain's phase back to [`ChainPhase::Idle`] while the
//! answer step is showing (a cancel it applied, a disconnect) returns the modal
//! to the wallet list. A declined or failed phase is kept as the host left it:
//! the answer step offers to try again or to choose another wallet.
//!
//! ## Motion
//!
//! The card rises and fades in over a scrim; steps slide in the direction of
//! travel and cross-fade, with the rows of a new step arriving one after
//! another; the card's height follows its content; a success ring draws itself
//! and a check pops; a failure shakes once. Time is the `dt` passed to
//! [`WalletModal::show`] — nothing reads the clock — so every frame of every
//! transition can be captured exactly.
//!
//! Narrower than [`macroquad_widgets::BOTTOM_SHEET_BELOW`], the card is a sheet rising from the
//! bottom edge, as `DetailSheet` does.
//!
//! While the modal is visible the host should draw the screen behind it with
//! no tap: a tap meant for the scrim must not also press what lies under it.

use std::f32::consts::TAU;

use macroquad::models::{Mesh, Vertex, draw_mesh};
use macroquad::prelude::*;
use macroquad_widgets::{Painter, SheetForm, TextSize, Theme};

/// The chain vocabulary the modal is expressed in, re-exported so a host can
/// name a chain without depending on `chains` itself.
pub use chains::ChainFamily;

use crate::ease::Ease;
use crate::paint::{Gradient, fill_gradient};
use crate::shape::{fill_rounded, stroke_rounded};
use crate::tween::{Animated, Lerp};

/// Seconds the card takes to rise in.
const OPEN_SECS: f32 = 0.34;
/// Seconds it takes to leave: quicker than arriving, so dismissal feels obeyed.
const CLOSE_SECS: f32 = 0.2;
/// Seconds a step takes to slide in.
const SLIDE_SECS: f32 = 0.34;
/// Seconds the card's height takes to follow a new step.
const HEIGHT_SECS: f32 = 0.3;
/// How far a step travels as it slides.
const SLIDE_PX: f32 = 40.0;
/// Between one row's arrival and the next.
const STAGGER: f32 = 0.04;
/// How long one row takes to arrive.
const ROW_IN_SECS: f32 = 0.22;

const CARD_MAX_W: f32 = 420.0;
const SHEET_MAX_W: f32 = 560.0;
/// Room kept between a modal card and the edge of its area.
const MARGIN: f32 = 16.0;
const PAD: f32 = 20.0;
const RADIUS: f32 = 20.0;
/// Title, breadcrumb and step bar.
const HEADER_H: f32 = 104.0;
/// The grab handle a bottom sheet adds above its header.
const HANDLE_SPACE: f32 = 12.0;
const TILE_H: f32 = 96.0;
const TILE_GAP: f32 = 10.0;
const ROW_H: f32 = 58.0;
const ROW_GAP: f32 = 8.0;
const BUTTON_H: f32 = 44.0;
const BUTTON_GAP: f32 = 8.0;
const ICON_BUTTON: f32 = 32.0;
/// A body at least this wide lays the chain tiles out three across.
const THREE_ACROSS: f32 = 340.0;
const LINE_H: f32 = 17.0;
/// The answer step's picture: the wallet's icon in its ring.
const HERO_H: f32 = 108.0;
const ADDRESS_H: f32 = 36.0;

// ============================================================================
// The view model
// ============================================================================

/// One wallet the modal can offer on a chain.
#[derive(Clone)]
pub struct WalletOption {
    /// What the host connects by: a CIP-30 key, an EIP-6963 id.
    pub key: String,
    pub name: String,
    /// The wallet's own icon, decoded by the host. `None` draws a monogram tile.
    pub icon: Option<Texture2D>,
}

impl WalletOption {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: Texture2D) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// Where a chain's connection is, as the host knows it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ChainPhase {
    /// Nothing connected and nothing asked.
    #[default]
    Idle,
    /// `wallet` (a [`WalletOption::key`]) has been asked and has not answered.
    Connecting { wallet: String },
    /// `wallet` answered with `address`, already in the form a reader is shown.
    Connected { wallet: String, address: String },
    /// The reader refused in the wallet's own dialog. Not an error.
    Declined { wallet: String },
    /// A wallet or bridge fault, in the host's words.
    Failed { wallet: String, message: String },
}

impl ChainPhase {
    /// The wallet this phase is about, if any.
    pub fn wallet(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Connecting { wallet }
            | Self::Connected { wallet, .. }
            | Self::Declined { wallet }
            | Self::Failed { wallet, .. } => Some(wallet),
        }
    }

    /// Whether the chain is mid-conversation or connected — the phases the
    /// answer step opens straight onto.
    fn is_live(&self) -> bool {
        matches!(self, Self::Connecting { .. } | Self::Connected { .. })
    }
}

/// Whether a chain can be connected yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// There is a bridge: its wallets are offered.
    Ready,
    /// On the roadmap, with no bridge yet: listed, dimmed, not tappable.
    Soon,
}

/// One chain the modal lists.
#[derive(Clone)]
pub struct ChainOption {
    pub family: ChainFamily,
    pub support: Support,
    /// The wallets detected on it, in the order to offer them.
    pub wallets: Vec<WalletOption>,
    pub phase: ChainPhase,
}

impl ChainOption {
    /// A chain with a bridge, and the wallets detected on it.
    pub fn ready(family: ChainFamily, wallets: Vec<WalletOption>) -> Self {
        Self {
            family,
            support: Support::Ready,
            wallets,
            phase: ChainPhase::Idle,
        }
    }

    /// A chain on the roadmap, listed so the reader can see it is coming.
    pub fn soon(family: ChainFamily) -> Self {
        Self {
            family,
            support: Support::Soon,
            wallets: Vec::new(),
            phase: ChainPhase::Idle,
        }
    }

    pub fn with_phase(mut self, phase: ChainPhase) -> Self {
        self.phase = phase;
        self
    }

    /// The display name of `key` on this chain, falling back to the key.
    pub fn wallet_name<'a>(&'a self, key: &'a str) -> &'a str {
        self.wallets
            .iter()
            .find(|w| w.key == key)
            .map_or(key, |w| w.name.as_str())
    }

    fn wallet(&self, key: &str) -> Option<&WalletOption> {
        self.wallets.iter().find(|w| w.key == key)
    }
}

/// Everything the modal and its button draw.
#[derive(Clone)]
pub struct WalletModalVm {
    pub title: String,
    /// Up to two lines above the chain tiles: the place to say what connecting
    /// does and does not do.
    pub intro: Option<String>,
    /// The chains offered, in the order to offer them.
    pub chains: Vec<ChainOption>,
}

impl WalletModalVm {
    pub fn new(chains: Vec<ChainOption>) -> Self {
        Self {
            title: "Connect a wallet".to_string(),
            intro: None,
            chains,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_intro(mut self, intro: impl Into<String>) -> Self {
        self.intro = Some(intro.into());
        self
    }

    pub fn chain(&self, family: ChainFamily) -> Option<&ChainOption> {
        self.chains.iter().find(|c| c.family == family)
    }

    pub fn chain_mut(&mut self, family: ChainFamily) -> Option<&mut ChainOption> {
        self.chains.iter_mut().find(|c| c.family == family)
    }

    /// Set a chain's phase, if the chain is listed. The host's one write.
    pub fn set_phase(&mut self, family: ChainFamily, phase: ChainPhase) {
        if let Some(chain) = self.chain_mut(family) {
            chain.phase = phase;
        }
    }

    fn phase(&self, family: ChainFamily) -> ChainPhase {
        self.chain(family)
            .map(|c| c.phase.clone())
            .unwrap_or_default()
    }
}

/// What the reader asked for. The host applies it to the chain's bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletModalAction {
    /// Ask `wallet` on `family` to connect. Also a retry after a refusal.
    Connect { family: ChainFamily, wallet: String },
    /// Stop waiting on the wallet asked on `family`.
    Cancel(ChainFamily),
    /// Let go of the wallet connected on `family`.
    Disconnect(ChainFamily),
}

/// Which step the modal is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Chains,
    Wallets,
    Answer,
}

impl Page {
    fn index(self) -> f32 {
        match self {
            Self::Chains => 0.0,
            Self::Wallets => 1.0,
            Self::Answer => 2.0,
        }
    }
}

/// What a tap inside the card asked for, before it is applied.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Act {
    Back,
    Close,
    Chain(ChainFamily),
    Wallet(String),
    Retry,
    Another,
    Done,
    Cancel,
    Disconnect,
}

// ============================================================================
// The modal
// ============================================================================

/// See the [module docs](self).
pub struct WalletModal {
    /// Whether the host wants it open. The card may still be leaving after this
    /// turns false; [`Self::is_visible`] is the question for input routing.
    open: bool,
    /// 0 closed, 1 open, eased.
    shown: Animated<f32>,
    page: Page,
    /// The page sliding out, while the current one slides in.
    leaving: Option<Page>,
    /// 0 as a step starts arriving, 1 when it has.
    slide: Animated<f32>,
    /// +1 forward, −1 back: which way a step travels.
    dir: f32,
    /// The card's height, following its content.
    height: Animated<f32>,
    /// The step bar's fill, in steps.
    progress: Animated<f32>,
    /// The back button's visibility.
    back_vis: Animated<f32>,
    /// The chain the flow is on, once settled.
    family: Option<ChainFamily>,
    /// The chain this opening is restricted to.
    only: Option<ChainFamily>,
    /// The wallet this flow asked, until the host answers.
    asked: Option<String>,
    /// Whether the chain step is part of this opening.
    chain_step: bool,
    /// The chain's phase as last drawn, to tell when it changed.
    seen: Option<ChainPhase>,
    /// Seconds the current step has shown, for the rows' stagger.
    page_age: f32,
    /// Seconds the chain's phase has held, for the answer step's motion.
    phase_age: f32,
    /// Smoothed hover per row of the current step.
    hover: Vec<f32>,
    /// The first sync after an open: settle the starting step without motion.
    fresh: bool,
    /// Jump the step bar and back button rather than easing them, once.
    snap: bool,
    /// Take the next height as given rather than easing to it.
    height_fresh: bool,
    /// Set by [`Self::settled`]: the first phase seen has already played.
    still: bool,
    dt: f32,
    /// Seconds since this modal was made, for the spinner.
    clock: f32,
}

impl Default for WalletModal {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletModal {
    pub fn new() -> Self {
        Self {
            open: false,
            shown: Animated::new(0.0),
            page: Page::Chains,
            leaving: None,
            slide: Animated::new(1.0),
            dir: 1.0,
            height: Animated::new(0.0),
            progress: Animated::new(0.0),
            back_vis: Animated::new(0.0),
            family: None,
            only: None,
            asked: None,
            chain_step: true,
            seen: None,
            page_age: 0.0,
            phase_age: 0.0,
            hover: Vec::new(),
            fresh: false,
            snap: true,
            height_fresh: true,
            still: false,
            dt: 0.0,
            clock: 0.0,
        }
    }

    /// Open on the chain step (or the one chain, when only one is listed).
    /// A no-op while already open, so a host may call it every frame.
    pub fn open(&mut self) {
        self.open_with(None);
    }

    /// Open on `family`'s wallets, with no chain step: for an action that only
    /// that chain can satisfy.
    pub fn open_on(&mut self, family: ChainFamily) {
        self.open_with(Some(family));
    }

    /// Skip the rise-in and the rows' arrival: the settled card, at once. For a
    /// capture of a step, or a host restoring a modal it had open.
    pub fn settled(mut self) -> Self {
        let to = if self.open { 1.0 } else { 0.0 };
        self.shown.set(to);
        self.slide.set(1.0);
        self.page_age = 10.0;
        self.phase_age = 10.0;
        self.snap = true;
        self.height_fresh = true;
        self.still = true;
        self
    }

    fn open_with(&mut self, only: Option<ChainFamily>) {
        if self.open && self.only == only {
            return;
        }
        let from_closed = !self.is_visible();
        self.open = true;
        self.only = only;
        self.family = only;
        self.asked = None;
        self.leaving = None;
        self.slide.set(1.0);
        self.seen = None;
        self.page_age = 0.0;
        self.phase_age = 0.0;
        self.hover.clear();
        self.fresh = true;
        if from_closed {
            self.snap = true;
            self.height_fresh = true;
        }
        self.shown.to(1.0, OPEN_SECS, Ease::ExpoOut);
    }

    /// Dismiss. A connect in flight is left to the host: closing the card is
    /// not cancelling the wallet's dialog, which may still be up.
    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.shown.to(0.0, CLOSE_SECS, Ease::CubicIn);
    }

    /// One step back, closing from the first step. For a host's Escape key or
    /// a gamepad's B.
    pub fn back(&mut self) {
        match self.page {
            Page::Answer => {
                self.asked = None;
                self.go(Page::Wallets, -1.0);
            }
            Page::Wallets if self.chain_step => self.go(Page::Chains, -1.0),
            Page::Wallets | Page::Chains => self.close(),
        }
    }

    /// Whether the host wants it open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether any of it is on screen: the question for "should the screen
    /// behind take input".
    pub fn is_visible(&self) -> bool {
        self.open || self.shown.get() > 0.0
    }

    pub fn page(&self) -> Page {
        self.page
    }

    /// The chain the flow has settled on, if it has.
    pub fn family(&self) -> Option<ChainFamily> {
        self.family
    }

    /// Advance, draw and report. Call every frame with the frame's `dt`; a
    /// closed modal draws nothing and costs nothing.
    pub fn show(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        area: Rect,
        dt: f32,
    ) -> Option<WalletModalAction> {
        self.step(vm, dt);
        if !self.is_visible() {
            return None;
        }
        let c = p.theme.color;
        let e = self.shown.get().clamp(0.0, 1.0);
        let form = SheetForm::for_area(area);
        let card_w = match form {
            SheetForm::Modal => CARD_MAX_W.min(area.w - 2.0 * MARGIN),
            SheetForm::Bottom => SHEET_MAX_W.min(area.w),
        };
        let handle = match form {
            SheetForm::Modal => 0.0,
            SheetForm::Bottom => HANDLE_SPACE,
        };
        let body_w = card_w - 2.0 * PAD;
        let room = match form {
            SheetForm::Modal => area.h - 2.0 * MARGIN,
            SheetForm::Bottom => area.h - MARGIN,
        };
        let target =
            (handle + HEADER_H + self.page_height(p, vm, self.page, body_w) + PAD).min(room);
        if self.height_fresh {
            self.height.set(target);
            self.height_fresh = false;
        } else if (self.height.target() - target).abs() > 0.5 {
            self.height.to(target, HEIGHT_SECS, Ease::ExpoOut);
        }
        let h = self.height.get();
        let card = match form {
            SheetForm::Modal => Rect::new(
                area.center().x - card_w * 0.5,
                area.center().y - h * 0.5 + (1.0 - e) * 28.0,
                card_w,
                h,
            ),
            SheetForm::Bottom => Rect::new(
                area.center().x - card_w * 0.5,
                area.bottom() - h + (1.0 - e) * (h + 24.0),
                card_w,
                h,
            ),
        };
        let tint = self.family.map_or(c.accent, |f| chain_tint(&p.theme, f));

        // Scrim and card.
        draw_rectangle(
            area.x,
            area.y,
            area.w,
            area.h,
            alpha(c.bg_primary, 0.74 * e),
        );
        // The sheet's bottom corners are pushed off the bottom edge, so it reads
        // as rising out of it rather than floating above it.
        let shape = match form {
            SheetForm::Modal => card,
            SheetForm::Bottom => Rect::new(card.x, card.y, card.w, card.h + RADIUS),
        };
        // A shadow falling below the card: one plate, since overlapping
        // translucent plates stack into a visible halo.
        fill_rounded(
            Rect::new(shape.x + 2.0, shape.y + 10.0, shape.w - 4.0, shape.h),
            RADIUS,
            alpha(c.bg_primary, 0.8 * e),
        );
        fill_gradient(
            shape,
            RADIUS,
            Gradient::Vertical {
                top: alpha(Color::lerp(c.bg_secondary, c.bg_highlight, 0.3), e),
                bottom: alpha(c.bg_secondary, e),
            },
        );
        // A glow in the chain's colour across the top: the card takes on the
        // chain the reader is connecting.
        fill_gradient(
            Rect::new(shape.x, shape.y, shape.w, 140.0f32.min(shape.h)),
            RADIUS,
            Gradient::Vertical {
                top: alpha(tint, 0.16 * e),
                bottom: alpha(tint, 0.0),
            },
        );
        stroke_rounded(shape, RADIUS, 1.0, alpha(c.border, e));
        if form == SheetForm::Bottom {
            fill_rounded(
                Rect::new(card.center().x - 20.0, card.y + 8.0, 40.0, 4.0),
                2.0,
                alpha(c.text_muted, 0.6 * e),
            );
        }

        let live = self.open && e > 0.85;
        let mut act = self.draw_header(p, vm, card, handle, tint, e, live);

        // The steps: the one leaving, then the one arriving over it.
        let body = Rect::new(
            card.x + PAD,
            card.y + handle + HEADER_H,
            body_w,
            (h - handle - HEADER_H - PAD).max(0.0),
        );
        let s = self.slide.get();
        let limit = card.bottom() - 6.0;
        if let Some(prev) = self.leaving {
            if s < 1.0 {
                let fade = (1.0 - s) * (1.0 - s);
                // Half the arriving step's travel, so it stays inside the
                // card's padding as it goes.
                let layer = Layer {
                    alpha: e * fade,
                    dx: -self.dir * SLIDE_PX * 0.5 * s,
                    live: false,
                    age: 10.0,
                    limit,
                };
                let _ = self.draw_page(p, vm, prev, body, layer);
            } else {
                self.leaving = None;
            }
        }
        let layer = Layer {
            alpha: e * s,
            dx: self.dir * SLIDE_PX * (1.0 - s),
            live: live && s > 0.6,
            age: self.page_age,
            limit,
        };
        if let Some(a) = self.draw_page(p, vm, self.page, body, layer) {
            act = Some(a);
        }

        if live && p.tap.is_some_and(|t| area.contains(t) && !card.contains(t)) {
            act = Some(Act::Close);
        }
        act.and_then(|a| self.apply(vm, a))
    }

    // ------------------------------------------------------------------------
    // The flow, without drawing
    // ------------------------------------------------------------------------

    /// Advance the motion and settle the step against the host's phases.
    fn step(&mut self, vm: &WalletModalVm, dt: f32) {
        self.dt = dt;
        self.clock += dt;
        self.shown.update(dt);
        self.slide.update(dt);
        self.height.update(dt);
        self.progress.update(dt);
        self.back_vis.update(dt);
        self.page_age += dt;
        self.phase_age += dt;
        if !self.is_visible() {
            self.leaving = None;
            return;
        }
        if self.open {
            self.sync(vm);
        }
    }

    fn sync(&mut self, vm: &WalletModalVm) {
        self.chain_step = self.only.is_none() && vm.chains.len() > 1;
        if self.family.is_none() && vm.chains.len() == 1 {
            self.family = Some(vm.chains[0].family);
        }
        if std::mem::take(&mut self.fresh) {
            self.page = match self.family {
                Some(f) if vm.phase(f).is_live() => Page::Answer,
                Some(_) => Page::Wallets,
                None => Page::Chains,
            };
        }
        let phase = self.family.map(|f| vm.phase(f)).unwrap_or_default();
        // The host let go of the chain while its answer was showing: there is
        // no answer left to show.
        if self.page == Page::Answer
            && phase == ChainPhase::Idle
            && self.seen.as_ref().is_some_and(|s| *s != ChainPhase::Idle)
        {
            self.asked = None;
            self.go(Page::Wallets, -1.0);
        }
        if self.seen.as_ref() != Some(&phase) {
            // A settled modal shows the phase it opened on as already arrived.
            if !std::mem::take(&mut self.still) {
                self.phase_age = 0.0;
            }
            self.seen = Some(phase);
        }

        let offset = if self.chain_step { 0.0 } else { 1.0 };
        let progress = self.page.index() - offset;
        let back = if self.can_back() { 1.0 } else { 0.0 };
        if std::mem::take(&mut self.snap) {
            self.progress.set(progress);
            self.back_vis.set(back);
        } else {
            if self.progress.target() != progress {
                self.progress.to(progress, SLIDE_SECS, Ease::ExpoOut);
            }
            if self.back_vis.target() != back {
                self.back_vis.to(back, 0.2, Ease::CubicOut);
            }
        }
    }

    fn can_back(&self) -> bool {
        match self.page {
            Page::Chains => false,
            Page::Wallets => self.chain_step,
            Page::Answer => true,
        }
    }

    fn go(&mut self, page: Page, dir: f32) {
        if page == self.page {
            return;
        }
        self.leaving = Some(self.page);
        self.page = page;
        self.dir = dir;
        self.slide.set(0.0);
        self.slide.to(1.0, SLIDE_SECS, Ease::ExpoOut);
        self.page_age = 0.0;
        self.hover.clear();
    }

    fn apply(&mut self, vm: &WalletModalVm, act: Act) -> Option<WalletModalAction> {
        match act {
            Act::Back => {
                self.back();
                None
            }
            Act::Close | Act::Done => {
                self.close();
                None
            }
            Act::Chain(family) => {
                self.choose_chain(vm, family);
                None
            }
            Act::Wallet(key) => self.choose_wallet(key),
            Act::Retry => {
                let family = self.family?;
                let wallet = vm
                    .phase(family)
                    .wallet()
                    .map(str::to_string)
                    .or_else(|| self.asked.clone())?;
                self.asked = Some(wallet.clone());
                Some(WalletModalAction::Connect { family, wallet })
            }
            Act::Another => {
                self.asked = None;
                self.go(Page::Wallets, -1.0);
                None
            }
            Act::Cancel => self.family.map(WalletModalAction::Cancel),
            Act::Disconnect => self.family.map(WalletModalAction::Disconnect),
        }
    }

    /// Pick `family` on the chain step, as a tap on its tile does: to its
    /// wallets, or to its answer when it is connecting or connected. A chain
    /// that is not listed, or is [`Support::Soon`], is ignored. For a host
    /// driving the flow without a pointer — a pad, a script.
    pub fn choose_chain(&mut self, vm: &WalletModalVm, family: ChainFamily) {
        let Some(chain) = vm.chain(family) else {
            return;
        };
        if chain.support == Support::Soon {
            return;
        }
        // An explicit choice supersedes the step the opening would have
        // guessed, even when it comes before the first frame.
        self.fresh = false;
        self.family = Some(family);
        self.asked = None;
        self.seen = None;
        let page = if chain.phase.is_live() {
            Page::Answer
        } else {
            Page::Wallets
        };
        self.go(page, 1.0);
    }

    /// Pick the wallet `key` on the settled chain, as a tap on its row does:
    /// the connect to apply, and the answer step. `None` before a chain is
    /// settled.
    pub fn choose_wallet(&mut self, key: impl Into<String>) -> Option<WalletModalAction> {
        let key = key.into();
        let family = self.family?;
        self.fresh = false;
        self.asked = Some(key.clone());
        self.go(Page::Answer, 1.0);
        Some(WalletModalAction::Connect {
            family,
            wallet: key,
        })
    }

    /// The phase the answer step draws: the host's, or — in the frame between
    /// a tap and the host applying it — the wallet just asked, as waiting.
    fn answer_phase(&self, vm: &WalletModalVm) -> ChainPhase {
        let phase = self.family.map(|f| vm.phase(f)).unwrap_or_default();
        match (&phase, &self.asked) {
            (ChainPhase::Idle, Some(wallet)) => ChainPhase::Connecting {
                wallet: wallet.clone(),
            },
            _ => phase,
        }
    }

    fn hover_at(&mut self, i: usize, on: bool) -> f32 {
        if self.hover.len() <= i {
            self.hover.resize(i + 1, 0.0);
        }
        let target = if on { 1.0 } else { 0.0 };
        let k = 1.0 - (-self.dt * 18.0).exp();
        self.hover[i] += (target - self.hover[i]) * k;
        self.hover[i]
    }

    // ------------------------------------------------------------------------
    // Layout
    // ------------------------------------------------------------------------

    fn page_height(&self, p: &Painter, vm: &WalletModalVm, page: Page, w: f32) -> f32 {
        match page {
            Page::Chains => {
                let cols = columns(w);
                let rows = vm.chains.len().div_ceil(cols).max(1) as f32;
                intro_height(p, vm, w) + rows * TILE_H + (rows - 1.0) * TILE_GAP
            }
            Page::Wallets => {
                let n = self
                    .family
                    .and_then(|f| vm.chain(f))
                    .map_or(0, |c| c.wallets.len());
                if n == 0 {
                    let lines = wrap(p, &self.empty_message(), p.size(TextSize::Md), w, 3);
                    104.0 + lines.len() as f32 * LINE_H
                } else {
                    n as f32 * ROW_H + (n as f32 - 1.0) * ROW_GAP
                }
            }
            Page::Answer => {
                let phase = self.answer_phase(vm);
                let copy = answer_copy(vm, self.family, &phase);
                let lines = wrap(p, &copy.message, p.size(TextSize::Md), w, 3);
                let mut h = HERO_H + 30.0 + lines.len() as f32 * LINE_H + 18.0;
                if matches!(phase, ChainPhase::Connected { .. }) {
                    h += ADDRESS_H + 16.0;
                }
                let buttons = copy.buttons.len() as f32;
                h + buttons * BUTTON_H + (buttons - 1.0).max(0.0) * BUTTON_GAP
            }
        }
    }

    fn empty_message(&self) -> String {
        let name = self.family.map_or("a", |f| f.display_name());
        format!(
            "Install a {name} wallet extension and reload, or open this game in your wallet's own browser."
        )
    }

    // ------------------------------------------------------------------------
    // Drawing
    // ------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn draw_header(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        card: Rect,
        handle: f32,
        tint: Color,
        e: f32,
        live: bool,
    ) -> Option<Act> {
        let c = p.theme.color;
        let mut act = None;
        let top = card.y + handle + PAD;
        let x = card.x + PAD;
        let w = card.w - 2.0 * PAD;
        let mid = top + ICON_BUTTON * 0.5;

        // Back: slides in from the left as the title makes room for it.
        let bv = self.back_vis.get();
        if bv > 0.01 {
            let r = Rect::new(x - 8.0 * (1.0 - bv), top, ICON_BUTTON, ICON_BUTTON);
            let hit = p.interact(r, live && bv > 0.9);
            if hit.hover || hit.pressed {
                draw_circle(r.center().x, mid, 16.0, alpha(c.bg_highlight, 0.8 * e));
            }
            chevron(r.center(), 6.0, -1.0, alpha(c.text_primary, e * bv));
            if hit.clicked {
                act = Some(Act::Back);
            }
        }
        let title_x = x + (ICON_BUTTON + 8.0) * bv;
        let title_size = p.size(TextSize::Xl2);
        let title = p.fit(&vm.title, title_size, w - (title_x - x) - ICON_BUTTON - 8.0);
        p.text_mid(&title, title_x, mid, title_size, alpha(c.text_primary, e));

        let close = Rect::new(x + w - ICON_BUTTON, top, ICON_BUTTON, ICON_BUTTON);
        let hit = p.interact(close, live);
        if hit.hover || hit.pressed {
            draw_circle(close.center().x, mid, 16.0, alpha(c.bg_highlight, 0.8 * e));
        }
        cross(close.center(), 5.5, alpha(c.text_muted, e));
        if hit.clicked {
            act = Some(Act::Close);
        }

        // Where the flow is: the chain, and the wallet once one is asked.
        let crumb_mid = top + 50.0;
        let crumb_size = p.size(TextSize::Md);
        match (self.page, self.family) {
            (Page::Chains, _) | (_, None) => {
                let line = p.fit("Choose a network to connect on.", crumb_size, w);
                p.text_mid(&line, x, crumb_mid, crumb_size, alpha(c.text_muted, e));
            }
            (page, Some(family)) => {
                chain_mark(family, vec2(x + 8.0, crumb_mid), 15.0, alpha(tint, e));
                let mut line = family.display_name().to_string();
                if page == Page::Answer
                    && let Some(chain) = vm.chain(family)
                    && let Some(key) = self.answer_phase(vm).wallet()
                {
                    line = format!("{line}  \u{203a}  {}", chain.wallet_name(key));
                }
                let line = p.fit(&line, crumb_size, w - 24.0);
                p.text_mid(
                    &line,
                    x + 22.0,
                    crumb_mid,
                    crumb_size,
                    alpha(c.text_secondary, e),
                );
            }
        }

        // The step bar: one segment per question this opening asks.
        let steps = if self.chain_step { 3 } else { 2 };
        let gap = 6.0;
        let seg_w = (w - gap * (steps - 1) as f32) / steps as f32;
        let bar_y = top + 70.0;
        let progress = self.progress.get();
        for k in 0..steps {
            let seg = Rect::new(x + k as f32 * (seg_w + gap), bar_y, seg_w, 3.0);
            fill_rounded(seg, 1.5, alpha(c.bg_highlight, e));
            let fill = (progress - k as f32 + 1.0).clamp(0.0, 1.0);
            if fill > 0.0 {
                fill_rounded(
                    Rect::new(seg.x, seg.y, seg.w * fill, seg.h),
                    1.5,
                    alpha(tint, e),
                );
            }
        }
        act
    }

    fn draw_page(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        page: Page,
        body: Rect,
        layer: Layer,
    ) -> Option<Act> {
        if layer.alpha <= 0.001 {
            return None;
        }
        match page {
            Page::Chains => self.draw_chains(p, vm, body, layer),
            Page::Wallets => self.draw_wallets(p, vm, body, layer),
            Page::Answer => self.draw_answer(p, vm, body, layer),
        }
    }

    fn draw_chains(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        body: Rect,
        layer: Layer,
    ) -> Option<Act> {
        let c = p.theme.color;
        let cols = columns(body.w);
        let tile_w = (body.w - TILE_GAP * (cols - 1) as f32) / cols as f32;
        if let Some(intro) = &vm.intro {
            let size = p.size(TextSize::Md);
            for (i, line) in wrap(p, intro, size, body.w, 2).iter().enumerate() {
                p.text_mid(
                    line,
                    body.x + layer.dx,
                    body.y + 6.0 + i as f32 * LINE_H,
                    size,
                    alpha(c.text_secondary, layer.alpha),
                );
            }
        }
        let top = body.y + intro_height(p, vm, body.w);
        let mut act = None;
        for (i, chain) in vm.chains.iter().enumerate() {
            let s = layer.stagger(i);
            let a = layer.alpha * s;
            let (col, row) = ((i % cols) as f32, (i / cols) as f32);
            let mut rect = Rect::new(
                body.x + layer.dx + col * (tile_w + TILE_GAP),
                top + row * (TILE_H + TILE_GAP) + (1.0 - s) * 12.0,
                tile_w,
                TILE_H,
            );
            let room = layer.room(rect.bottom());
            let a = a * room;
            let ready = chain.support == Support::Ready;
            let hit = p.interact(rect, layer.live && ready && room > 0.9);
            let hv = self.hover_at(i, hit.hover || hit.pressed);
            rect.y -= 2.0 * hv;
            let tint = chain_tint(&p.theme, chain.family);
            let dim = if ready { 1.0 } else { 0.38 };
            let press = if hit.pressed { 0.08 } else { 0.0 };

            fill_rounded(
                rect,
                14.0,
                alpha(
                    Color::lerp(c.bg_highlight, tint, 0.06 + 0.12 * hv + press),
                    a * dim * (0.55 + 0.25 * hv),
                ),
            );
            stroke_rounded(rect, 14.0, 1.0, alpha(tint, a * dim * (0.2 + 0.6 * hv)));

            let mark = vec2(rect.center().x, rect.y + 33.0);
            draw_circle(
                mark.x,
                mark.y,
                21.0,
                alpha(tint, a * dim * (0.16 + 0.08 * hv)),
            );
            chain_mark(chain.family, mark, 24.0, alpha(tint, a * dim));

            let name_size = p.size(TextSize::Lg);
            p.text_centred(
                &p.fit(chain.family.display_name(), name_size, rect.w - 12.0),
                Rect::new(rect.x, rect.y + 58.0, rect.w, 14.0),
                name_size,
                alpha(c.text_primary, a * dim),
            );
            let (status, ink) = match (&chain.support, &chain.phase) {
                (Support::Soon, _) => ("Coming soon".to_string(), c.text_muted),
                (_, ChainPhase::Connected { wallet, .. }) => {
                    (chain.wallet_name(wallet).to_string(), c.success)
                }
                (_, ChainPhase::Connecting { .. }) => ("Connecting…".to_string(), tint),
                _ => (
                    match chain.wallets.len() {
                        0 => "No wallet found".to_string(),
                        1 => "1 wallet".to_string(),
                        n => format!("{n} wallets"),
                    },
                    c.text_muted,
                ),
            };
            let status_size = p.size(TextSize::Sm);
            p.text_centred(
                &p.fit(&status, status_size, rect.w - 12.0),
                Rect::new(rect.x, rect.y + 76.0, rect.w, 10.0),
                status_size,
                alpha(ink, a),
            );
            if matches!(chain.phase, ChainPhase::Connected { .. }) {
                check_badge(
                    vec2(mark.x + 16.0, mark.y - 15.0),
                    7.0,
                    c.success,
                    c.bg_primary,
                    a,
                );
            }
            if hit.clicked {
                act = Some(Act::Chain(chain.family));
            }
        }
        act
    }

    fn draw_wallets(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        body: Rect,
        layer: Layer,
    ) -> Option<Act> {
        let c = p.theme.color;
        let family = self.family?;
        let chain = vm.chain(family)?;
        let tint = chain_tint(&p.theme, family);

        if chain.wallets.is_empty() {
            let mid = vec2(body.center().x + layer.dx, body.y + 34.0);
            draw_circle(mid.x, mid.y, 30.0, alpha(tint, 0.14 * layer.alpha));
            chain_mark(family, mid, 32.0, alpha(tint, 0.7 * layer.alpha));
            let title_size = p.size(TextSize::Xl);
            p.text_centred(
                &format!("No {} wallet found", family.display_name()),
                Rect::new(body.x + layer.dx, body.y + 74.0, body.w, 16.0),
                title_size,
                alpha(c.text_primary, layer.alpha * layer.room(body.y + 90.0)),
            );
            let size = p.size(TextSize::Md);
            for (i, line) in wrap(p, &self.empty_message(), size, body.w, 3)
                .iter()
                .enumerate()
            {
                let top = body.y + 100.0 + i as f32 * LINE_H;
                p.text_centred(
                    line,
                    Rect::new(body.x + layer.dx, top, body.w, 12.0),
                    size,
                    alpha(c.text_muted, layer.alpha * layer.room(top + 12.0)),
                );
            }
            return None;
        }

        let mut act = None;
        for (i, wallet) in chain.wallets.iter().enumerate() {
            let s = layer.stagger(i);
            let a = layer.alpha * s;
            let rect = Rect::new(
                body.x + layer.dx + (1.0 - s) * 16.0 * self.dir,
                body.y + i as f32 * (ROW_H + ROW_GAP),
                body.w,
                ROW_H,
            );
            let room = layer.room(rect.bottom());
            let a = a * room;
            let hit = p.interact(rect, layer.live && room > 0.9);
            let hv = self.hover_at(i, hit.hover || hit.pressed);
            let press = if hit.pressed { 0.08 } else { 0.0 };
            fill_rounded(
                rect,
                14.0,
                alpha(
                    Color::lerp(c.bg_highlight, tint, 0.05 + 0.12 * hv + press),
                    a * (0.55 + 0.25 * hv),
                ),
            );
            stroke_rounded(rect, 14.0, 1.0, alpha(tint, a * (0.14 + 0.55 * hv)));
            // An accent edge that grows in from the left as the row is hovered.
            if hv > 0.02 {
                fill_rounded(
                    Rect::new(rect.x + 4.0, rect.center().y - 12.0 * hv, 3.0, 24.0 * hv),
                    1.5,
                    alpha(tint, a * hv),
                );
            }
            let icon = Rect::new(rect.x + 12.0, rect.y + 11.0, 36.0, 36.0);
            wallet_icon(p, Some(wallet), &wallet.name, icon, a);

            let connected = matches!(
                &chain.phase,
                ChainPhase::Connected { wallet: w, .. } if *w == wallet.key
            );
            let name_size = p.size(TextSize::Xl);
            let right_room = if connected { 96.0 } else { 36.0 };
            let name = p.fit(&wallet.name, name_size, rect.w - 64.0 - right_room);
            p.text_mid(
                &name,
                rect.x + 62.0 + 3.0 * hv,
                rect.center().y,
                name_size,
                alpha(c.text_primary, a),
            );
            if connected {
                let chip = Rect::new(rect.right() - 92.0, rect.center().y - 11.0, 80.0, 22.0);
                fill_rounded(chip, 11.0, alpha(c.success, 0.16 * a));
                p.text_centred("Connected", chip, p.size(TextSize::Sm), alpha(c.success, a));
            } else {
                chevron(
                    vec2(rect.right() - 20.0 + 3.0 * hv, rect.center().y),
                    5.0,
                    1.0,
                    alpha(Color::lerp(c.text_muted, tint, hv), a),
                );
            }
            if hit.clicked {
                act = Some(Act::Wallet(wallet.key.clone()));
            }
        }
        act
    }

    fn draw_answer(
        &mut self,
        p: &Painter,
        vm: &WalletModalVm,
        body: Rect,
        layer: Layer,
    ) -> Option<Act> {
        let c = p.theme.color;
        let family = self.family?;
        let chain = vm.chain(family)?;
        let tint = chain_tint(&p.theme, family);
        let phase = self.answer_phase(vm);
        let copy = answer_copy(vm, Some(family), &phase);
        let key = phase.wallet().unwrap_or("");
        let wallet = chain.wallet(key);
        let name = chain.wallet_name(key);
        let age = self.phase_age;
        let a = layer.alpha;

        // A failure shakes once, the way a wrong answer does in a game.
        let shake = if matches!(phase, ChainPhase::Failed { .. }) {
            let k = (1.0 - age / 0.42).max(0.0);
            (age * 46.0).sin() * 7.0 * k * k
        } else {
            0.0
        };
        let dx = layer.dx + shake;
        let hero = vec2(body.center().x + dx, body.y + 50.0);
        let ring_r = 42.0;

        match &phase {
            ChainPhase::Connecting { .. } | ChainPhase::Idle => {
                let pulse = (self.clock * 2.4).sin() * 0.5 + 0.5;
                draw_circle(
                    hero.x,
                    hero.y,
                    ring_r + 6.0 + 4.0 * pulse,
                    alpha(tint, 0.08 * a),
                );
                draw_circle_lines(hero.x, hero.y, ring_r, 3.0, alpha(c.bg_highlight, a));
                let spin = self.clock * 320.0;
                let sweep = 90.0 + 60.0 * (self.clock * 1.7).sin();
                draw_arc(
                    hero.x,
                    hero.y,
                    64,
                    ring_r - 1.5,
                    spin,
                    3.0,
                    sweep,
                    alpha(tint, a),
                );
            }
            ChainPhase::Connected { .. } => {
                let k = Ease::CubicOut.apply(age / 0.5);
                draw_circle(
                    hero.x,
                    hero.y,
                    ring_r + 8.0 * k,
                    alpha(c.success, 0.10 * a * k),
                );
                draw_circle_lines(hero.x, hero.y, ring_r, 3.0, alpha(c.bg_highlight, a));
                draw_arc(
                    hero.x,
                    hero.y,
                    64,
                    ring_r - 1.5,
                    -90.0,
                    3.0,
                    360.0 * k,
                    alpha(c.success, a),
                );
            }
            ChainPhase::Declined { .. } => {
                draw_circle_lines(hero.x, hero.y, ring_r, 3.0, alpha(c.text_muted, 0.7 * a));
            }
            ChainPhase::Failed { .. } => {
                draw_circle(hero.x, hero.y, ring_r + 6.0, alpha(c.error, 0.08 * a));
                draw_circle_lines(hero.x, hero.y, ring_r, 3.0, alpha(c.error, a));
            }
        }
        let icon = Rect::new(hero.x - 28.0, hero.y - 28.0, 56.0, 56.0);
        wallet_icon(p, wallet, name, icon, a);
        let badge = vec2(hero.x + 30.0, hero.y + 28.0);
        match &phase {
            ChainPhase::Connected { .. } => {
                let pop = Ease::BackOut.apply((age - 0.35) / 0.3);
                if pop > 0.0 {
                    check_badge(badge, 11.0 * pop, c.success, c.bg_primary, a);
                }
            }
            ChainPhase::Declined { .. } => {
                draw_circle(badge.x, badge.y, 11.0, alpha(c.warning, a));
                draw_line(
                    badge.x - 4.5,
                    badge.y,
                    badge.x + 4.5,
                    badge.y,
                    2.2,
                    alpha(c.bg_primary, a),
                );
            }
            ChainPhase::Failed { .. } => {
                draw_circle(badge.x, badge.y, 11.0, alpha(c.error, a));
                draw_line(
                    badge.x,
                    badge.y - 5.0,
                    badge.x,
                    badge.y + 1.5,
                    2.2,
                    alpha(c.bg_primary, a),
                );
                draw_circle(badge.x, badge.y + 4.5, 1.3, alpha(c.bg_primary, a));
            }
            _ => {}
        }

        let mut y = body.y + HERO_H;
        let title_size = p.size(TextSize::Xl2);
        p.text_centred(
            &p.fit(&copy.title, title_size, body.w),
            Rect::new(body.x + dx, y, body.w, 18.0),
            title_size,
            alpha(c.text_primary, a * layer.room(y + 18.0)),
        );
        y += 30.0;
        let size = p.size(TextSize::Md);
        for line in wrap(p, &copy.message, size, body.w, 3) {
            p.text_centred(
                &line,
                Rect::new(body.x + dx, y, body.w, 12.0),
                size,
                alpha(c.text_muted, a * layer.room(y + 12.0)),
            );
            y += LINE_H;
        }
        y += 18.0;

        if let ChainPhase::Connected { address, .. } = &phase {
            let a = a * layer.room(y + ADDRESS_H);
            let pill = Rect::new(body.x + dx, y, body.w, ADDRESS_H);
            fill_rounded(pill, 12.0, alpha(c.bg_primary, 0.7 * a));
            stroke_rounded(pill, 12.0, 1.0, alpha(c.border, a));
            chain_mark(
                family,
                vec2(pill.x + 20.0, pill.center().y),
                14.0,
                alpha(tint, a),
            );
            let mono = p.size(TextSize::Md);
            let shown = fit_address(p, address, mono, pill.w - 48.0);
            p.mono_mid(
                &shown,
                pill.x + 36.0,
                pill.center().y,
                mono,
                alpha(c.text_primary, a),
            );
            y += ADDRESS_H + 16.0;
        }

        let mut act = None;
        for (label, style, which) in &copy.buttons {
            let rect = Rect::new(body.x + dx, y, body.w, BUTTON_H);
            let fill = match which {
                Act::Done => c.success,
                _ => tint,
            };
            let room = layer.room(rect.bottom());
            if pill_button(
                p,
                rect,
                label,
                *style,
                fill,
                layer.live && room > 0.9,
                a * room,
            ) {
                act = Some(which.clone());
            }
            y += BUTTON_H + BUTTON_GAP;
        }
        act
    }
}

/// How a step is being drawn: its fade, its slide, whether it takes input, and
/// how long it has shown (for the rows' arrival).
///
/// macroquad cannot clip, so a step taller than the card's current height —
/// the one leaving as the card shrinks, the one arriving as it grows — fades
/// out whatever runs past `limit` instead of drawing over the scrim.
#[derive(Clone, Copy)]
struct Layer {
    alpha: f32,
    dx: f32,
    live: bool,
    age: f32,
    /// The lowest y content may reach before it fades.
    limit: f32,
}

impl Layer {
    /// How far row `i` has arrived, 0..=1.
    fn stagger(&self, i: usize) -> f32 {
        Ease::CubicOut.apply((self.age - i as f32 * STAGGER) / ROW_IN_SECS)
    }

    /// How much of something ending at `bottom` may show, 0..=1.
    fn room(&self, bottom: f32) -> f32 {
        (1.0 - (bottom - self.limit) / 14.0).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PillStyle {
    Filled,
    Ghost,
}

/// What the answer step says, and what it offers.
struct AnswerCopy {
    title: String,
    message: String,
    buttons: Vec<(&'static str, PillStyle, Act)>,
}

fn answer_copy(vm: &WalletModalVm, family: Option<ChainFamily>, phase: &ChainPhase) -> AnswerCopy {
    let chain = family.and_then(|f| vm.chain(f));
    let name = |key: &str| chain.map_or(key.to_string(), |c| c.wallet_name(key).to_string());
    let chain_name = family.map_or("", |f| f.display_name());
    let again = vec![
        ("Try again", PillStyle::Filled, Act::Retry),
        ("Choose another wallet", PillStyle::Ghost, Act::Another),
    ];
    match phase {
        ChainPhase::Idle => AnswerCopy {
            title: String::new(),
            message: String::new(),
            buttons: Vec::new(),
        },
        ChainPhase::Connecting { wallet } => AnswerCopy {
            title: format!("Waiting for {}", name(wallet)),
            message: format!("Approve the request in {} to connect.", name(wallet)),
            buttons: vec![("Cancel", PillStyle::Ghost, Act::Cancel)],
        },
        ChainPhase::Connected { wallet, .. } => AnswerCopy {
            title: "Connected".to_string(),
            message: format!("{} on {chain_name}", name(wallet)),
            buttons: vec![
                ("Done", PillStyle::Filled, Act::Done),
                ("Disconnect", PillStyle::Ghost, Act::Disconnect),
            ],
        },
        ChainPhase::Declined { wallet } => AnswerCopy {
            title: "Request cancelled".to_string(),
            message: format!(
                "{} didn't share an account. Try again, or choose another wallet.",
                name(wallet)
            ),
            buttons: again,
        },
        ChainPhase::Failed { message, .. } => AnswerCopy {
            title: "Couldn't connect".to_string(),
            message: message.clone(),
            buttons: again,
        },
    }
}

/// The room the chain step's intro takes above its tiles.
fn intro_height(p: &Painter, vm: &WalletModalVm, w: f32) -> f32 {
    vm.intro.as_ref().map_or(0.0, |intro| {
        wrap(p, intro, p.size(TextSize::Md), w, 2).len() as f32 * LINE_H + 8.0
    })
}

fn columns(w: f32) -> usize {
    if w >= THREE_ACROSS { 3 } else { 2 }
}

// ============================================================================
// The button
// ============================================================================

/// The one wallet control a screen carries. `true` when tapped: the host's cue
/// to [`WalletModal::open`].
///
/// Disconnected, it is a filled call to action. Connecting, it spins. Connected,
/// it shows each connected chain's mark and the first connection's address,
/// with a live dot.
pub fn wallet_button(p: &Painter, vm: &WalletModalVm, rect: Rect) -> bool {
    let c = p.theme.color;
    let hit = p.interact(rect, true);
    let r = rect.h * 0.5;
    let connected: Vec<(ChainFamily, &str)> = vm
        .chains
        .iter()
        .filter_map(|ch| match &ch.phase {
            ChainPhase::Connected { address, .. } => Some((ch.family, address.as_str())),
            _ => None,
        })
        .collect();
    let connecting = vm
        .chains
        .iter()
        .find(|ch| matches!(ch.phase, ChainPhase::Connecting { .. }));
    let label_size = p.size(TextSize::Lg);
    let lift = if hit.pressed {
        -0.08
    } else if hit.hover {
        0.1
    } else {
        0.0
    };

    if let Some(&(_, address)) = connected.first() {
        fill_rounded(
            rect,
            r,
            Color::lerp(c.bg_secondary, c.bg_highlight, 0.4 + lift * 3.0),
        );
        stroke_rounded(rect, r, 1.0, c.border);
        let disc = r * 0.62;
        let mut x = rect.x + r;
        for (i, (family, _)) in connected.iter().take(3).enumerate() {
            let at = vec2(x + i as f32 * disc * 1.25, rect.center().y);
            let tint = chain_tint(&p.theme, *family);
            draw_circle(at.x, at.y, disc + 1.5, c.bg_secondary);
            draw_circle(at.x, at.y, disc, Color::lerp(c.bg_highlight, tint, 0.3));
            chain_mark(*family, at, disc * 1.15, tint);
        }
        x += (connected.len().min(3) as f32 - 1.0) * disc * 1.25 + disc + 10.0;
        let mono = p.size(TextSize::Md);
        let dot_room = 22.0;
        let shown = fit_address(p, address, mono, rect.right() - x - dot_room);
        p.mono_mid(&shown, x, rect.center().y, mono, c.text_primary);
        let dot = vec2(rect.right() - 16.0, rect.center().y);
        let glow = (p.time() as f32 * 2.0).sin() * 0.5 + 0.5;
        draw_circle(dot.x, dot.y, 5.0 + 2.0 * glow, alpha(c.success, 0.18));
        draw_circle(dot.x, dot.y, 3.5, c.success);
    } else if let Some(chain) = connecting {
        let tint = chain_tint(&p.theme, chain.family);
        fill_rounded(rect, r, Color::lerp(c.bg_secondary, tint, 0.16 + lift));
        stroke_rounded(rect, r, 1.0, alpha(tint, 0.5));
        let at = vec2(rect.x + r, rect.center().y);
        let t = p.time() as f32;
        draw_circle_lines(at.x, at.y, 8.0, 2.0, alpha(tint, 0.25));
        draw_arc(at.x, at.y, 32, 7.0, t * 360.0, 2.0, 100.0, tint);
        let label = p.fit("Connecting…", label_size, rect.right() - at.x - 16.0 - 12.0);
        p.text_mid(
            &label,
            at.x + 16.0,
            rect.center().y,
            label_size,
            c.text_primary,
        );
    } else {
        let base = Color::lerp(c.accent, c.text_primary, lift.max(0.0));
        let base = Color::lerp(base, c.bg_primary, (-lift).max(0.0) * 2.0);
        fill_gradient(
            rect,
            r,
            Gradient::Vertical {
                top: Color::lerp(base, c.text_primary, 0.18),
                bottom: base,
            },
        );
        let glyph = rect.h * 0.42;
        let label = "Connect wallet";
        let lw = p.measure(label, label_size).width;
        let total = glyph + 10.0 + lw;
        let gx = (rect.center().x - total * 0.5).max(rect.x + 12.0);
        wallet_glyph(
            Rect::new(gx, rect.center().y - glyph * 0.5, glyph, glyph),
            c.bg_primary,
            base,
        );
        let label = p.fit(label, label_size, rect.right() - gx - glyph - 10.0 - 8.0);
        p.text_mid(
            &label,
            gx + glyph + 10.0,
            rect.center().y,
            label_size,
            c.bg_primary,
        );
    }
    hit.clicked
}

// ============================================================================
// Marks and icons
// ============================================================================

/// The colour a chain is drawn in: one of the theme's accents, so a re-skin
/// moves the marks with everything else.
pub fn chain_tint(theme: &Theme, family: ChainFamily) -> Color {
    let c = &theme.color;
    match family {
        ChainFamily::Cardano => c.accent_blue,
        ChainFamily::Evm => c.accent_magenta,
        ChainFamily::Solana => c.accent_green,
        ChainFamily::Tezos => c.accent_cyan,
        ChainFamily::Xrpl => c.text_secondary,
        ChainFamily::Midnight => c.accent_yellow,
    }
}

/// A chain's mark, drawn with shapes and centred on `at`, `size` across.
///
/// Evocations, not the chains' logos: a ring of dots for Cardano, a faceted
/// diamond for EVM, three slanted bars for Solana, a hooked `t` for Tezos, two
/// chevrons for XRPL and a crescent for Midnight. Recognisable at 14 px and
/// clean at 48, with no asset to ship and nothing that overlaps itself, so a
/// mark fades evenly.
pub fn chain_mark(family: ChainFamily, at: Vec2, size: f32, color: Color) {
    let r = size * 0.5;
    let dim = Color::new(color.r, color.g, color.b, color.a * 0.68);
    match family {
        ChainFamily::Cardano => {
            for k in 0..6 {
                let a = (k as f32 * 60.0 + 30.0).to_radians();
                let p = at + vec2(a.cos(), a.sin()) * 0.4 * r;
                draw_circle(p.x, p.y, 0.15 * r, color);
            }
            for k in 0..12 {
                let a = (k as f32 * 30.0).to_radians();
                let p = at + vec2(a.cos(), a.sin()) * 0.82 * r;
                draw_circle(p.x, p.y, 0.09 * r, dim);
            }
        }
        ChainFamily::Evm => {
            let v = |x: f32, y: f32| at + vec2(x, y) * r;
            let (top, mid) = (v(0.0, -1.0), v(0.0, 0.34));
            draw_triangle(top, v(-0.62, 0.08), mid, color);
            draw_triangle(top, v(0.62, 0.08), mid, dim);
            let (low, bottom) = (v(0.0, 0.52), v(0.0, 1.0));
            draw_triangle(v(-0.62, 0.26), low, bottom, color);
            draw_triangle(v(0.62, 0.26), low, bottom, dim);
        }
        ChainFamily::Solana => {
            let (w, h, skew) = (1.6 * r, 0.32 * r, 0.32 * r);
            let left = at.x - w * 0.5;
            for (k, dy) in [-0.6f32, 0.0, 0.6].into_iter().enumerate() {
                let (top, bottom) = (at.y + dy * r - h * 0.5, at.y + dy * r + h * 0.5);
                let ink = Color::new(color.r, color.g, color.b, color.a * (1.0 - 0.16 * k as f32));
                let quad = if k == 1 {
                    [
                        vec2(left, top),
                        vec2(left + w - skew, top),
                        vec2(left + w, bottom),
                        vec2(left + skew, bottom),
                    ]
                } else {
                    [
                        vec2(left + skew, top),
                        vec2(left + w, top),
                        vec2(left + w - skew, bottom),
                        vec2(left, bottom),
                    ]
                };
                draw_triangle(quad[0], quad[1], quad[2], ink);
                draw_triangle(quad[0], quad[2], quad[3], ink);
            }
        }
        ChainFamily::Tezos => {
            // Laid out on a grid where every part meets the next edge to edge.
            let o = at + vec2(0.0, -0.06 * r);
            let v = |x: f32, y: f32| o + vec2(x, y) * r;
            let bar = (v(-0.52, -0.78), v(0.48, -0.6));
            draw_rectangle(
                bar.0.x,
                bar.0.y,
                bar.1.x - bar.0.x,
                bar.1.y - bar.0.y,
                color,
            );
            let stem = (v(-0.2, -0.6), v(-0.01, 0.25));
            draw_rectangle(
                stem.0.x,
                stem.0.y,
                stem.1.x - stem.0.x,
                stem.1.y - stem.0.y,
                color,
            );
            let arm = [
                v(-0.01, -0.14),
                v(0.46, -0.44),
                v(0.46, -0.27),
                v(-0.01, 0.03),
            ];
            draw_triangle(arm[0], arm[1], arm[2], dim);
            draw_triangle(arm[0], arm[2], arm[3], dim);
            let centre = v(0.17, 0.25);
            let n = 24;
            let (mut outer, mut inner) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
            for i in 0..=n {
                let a = (180.0 - 215.0 * i as f32 / n as f32).to_radians();
                let d = vec2(a.cos(), -a.sin());
                outer.push(centre + d * 0.37 * r);
                inner.push(centre + d * 0.18 * r);
            }
            fill_strip(&outer, &inner, color);
        }
        ChainFamily::Xrpl => {
            // Two chevrons pointing at each other, each a band `t` thick.
            let t = vec2(0.0, 0.2 * r);
            let v = |x: f32, y: f32| at + vec2(x, y) * r;
            let top = [v(-0.66, -0.7), v(0.0, -0.16), v(0.66, -0.7)];
            fill_strip(&top, &top.map(|q| q + t), color);
            let bottom = [v(-0.66, 0.7), v(0.0, 0.16), v(0.66, 0.7)];
            fill_strip(&bottom, &bottom.map(|q| q - t), color);
        }
        ChainFamily::Midnight => {
            crescent(at, 0.78 * r, 0.36 * r, 0.64 * r, -45f32.to_radians(), color);
            let star = at + vec2(0.5, -0.5) * r;
            draw_circle(star.x, star.y, 0.1 * r, dim);
        }
    }
}

/// A wallet's picture in a rounded tile: its own icon over a dark plate, or a
/// monogram on a gradient picked from its name, so a wallet without an icon
/// keeps the same colour wherever it is listed.
pub fn wallet_icon(p: &Painter, wallet: Option<&WalletOption>, name: &str, rect: Rect, a: f32) {
    let c = p.theme.color;
    let radius = rect.w * 0.28;
    if let Some(icon) = wallet.and_then(|w| w.icon.as_ref()) {
        fill_rounded(rect, radius, alpha(c.bg_primary, a));
        stroke_rounded(rect, radius, 1.0, alpha(c.border, a));
        let inset = rect.w * 0.14;
        draw_texture_ex(
            icon,
            rect.x + inset,
            rect.y + inset,
            Color::new(1.0, 1.0, 1.0, a),
            DrawTextureParams {
                dest_size: Some(vec2(rect.w - 2.0 * inset, rect.h - 2.0 * inset)),
                ..Default::default()
            },
        );
        return;
    }
    let hues = [
        c.accent_blue,
        c.accent_magenta,
        c.accent_cyan,
        c.accent_orange,
        c.accent_green,
        c.accent_yellow,
    ];
    let hue = hues[name_hash(name) as usize % hues.len()];
    fill_gradient(
        rect,
        radius,
        Gradient::Vertical {
            top: alpha(Color::lerp(hue, c.text_primary, 0.25), a),
            bottom: alpha(Color::lerp(hue, c.bg_primary, 0.2), a),
        },
    );
    let letter = name
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    // theme-exempt: a monogram fills the tile it sits in, so it is sized from
    // the tile rather than from the type ramp.
    p.text_centred(&letter, rect, rect.h * 0.46, alpha(c.bg_primary, a));
}

/// A stable hash of a name, so a wallet keeps its colour between runs.
fn name_hash(name: &str) -> u32 {
    name.bytes().fold(2_166_136_261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    })
}

/// A wallet, drawn: a body, a flap, and a clasp in `under`.
fn wallet_glyph(r: Rect, color: Color, under: Color) {
    let s = r.w;
    let body = Rect::new(r.x, r.y + 0.22 * s, s, 0.72 * s);
    stroke_rounded(body, 0.16 * s, 0.11 * s, color);
    draw_line(
        r.x + 0.1 * s,
        r.y + 0.24 * s,
        r.x + 0.72 * s,
        r.y + 0.04 * s,
        0.11 * s,
        color,
    );
    let pocket = Rect::new(r.x + 0.56 * s, r.y + 0.43 * s, 0.44 * s, 0.3 * s);
    fill_rounded(pocket, 0.1 * s, color);
    draw_circle(pocket.x + 0.15 * s, pocket.center().y, 0.06 * s, under);
}

fn check_badge(at: Vec2, r: f32, fill: Color, ink: Color, a: f32) {
    draw_circle(at.x, at.y, r, alpha(fill, a));
    let k = r / 7.0;
    let th = 1.8 * k;
    let p0 = at + vec2(-3.2, 0.2) * k;
    let p1 = at + vec2(-0.8, 2.6) * k;
    let p2 = at + vec2(3.4, -2.4) * k;
    draw_line(p0.x, p0.y, p1.x, p1.y, th, alpha(ink, a));
    draw_line(p1.x, p1.y, p2.x, p2.y, th, alpha(ink, a));
}

/// `>` (dir 1) or `<` (dir −1), `size` from point to tail.
fn chevron(at: Vec2, size: f32, dir: f32, color: Color) {
    let tip = at + vec2(size * 0.5 * dir, 0.0);
    let tail = at.x - size * 0.5 * dir;
    draw_line(tail, at.y - size, tip.x, tip.y, 2.0, color);
    draw_line(tail, at.y + size, tip.x, tip.y, 2.0, color);
}

fn cross(at: Vec2, half: f32, color: Color) {
    draw_line(
        at.x - half,
        at.y - half,
        at.x + half,
        at.y + half,
        2.0,
        color,
    );
    draw_line(
        at.x - half,
        at.y + half,
        at.x + half,
        at.y - half,
        2.0,
        color,
    );
}

/// A rounded button in the modal's own fade, so it leaves with its step.
fn pill_button(
    p: &Painter,
    rect: Rect,
    label: &str,
    style: PillStyle,
    fill: Color,
    live: bool,
    a: f32,
) -> bool {
    let c = p.theme.color;
    let hit = p.interact(rect, live);
    let r = rect.h * 0.5;
    let size = p.size(TextSize::Lg);
    let label = p.fit(label, size, rect.w - 24.0);
    match style {
        PillStyle::Filled => {
            let base = if hit.pressed {
                Color::lerp(fill, c.bg_primary, 0.15)
            } else if hit.hover {
                Color::lerp(fill, c.text_primary, 0.12)
            } else {
                fill
            };
            fill_gradient(
                rect,
                r,
                Gradient::Vertical {
                    top: alpha(Color::lerp(base, c.text_primary, 0.14), a),
                    bottom: alpha(base, a),
                },
            );
            p.text_centred(&label, rect, size, alpha(c.bg_primary, a));
        }
        PillStyle::Ghost => {
            let k = if hit.pressed {
                0.7
            } else if hit.hover {
                0.45
            } else {
                0.0
            };
            if k > 0.0 {
                fill_rounded(rect, r, alpha(c.bg_highlight, k * a));
            }
            stroke_rounded(rect, r, 1.0, alpha(c.border, a));
            p.text_centred(&label, rect, size, alpha(c.text_primary, a));
        }
    }
    hit.clicked
}

// ============================================================================
// Geometry and text
// ============================================================================

/// A band between two polylines walked in the same direction, as triangles.
fn fill_strip(outer: &[Vec2], inner: &[Vec2], color: Color) {
    let n = outer.len().min(inner.len());
    if n < 2 {
        return;
    }
    let mut vertices = Vec::with_capacity(n * 2);
    for i in 0..n {
        vertices.push(Vertex::new(outer[i].x, outer[i].y, 0.0, 0.0, 0.0, color));
        vertices.push(Vertex::new(inner[i].x, inner[i].y, 0.0, 0.0, 0.0, color));
    }
    let mut indices = Vec::with_capacity((n - 1) * 6);
    for i in 0..n - 1 {
        let k = (2 * i) as u16;
        indices.extend([k, k + 1, k + 2, k + 1, k + 3, k + 2]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// The part of a circle (radius `outer`, centre `at`) outside a second circle
/// of radius `bite`, `offset` away from it in direction `angle`: a crescent,
/// filled as one strip so it fades evenly.
fn crescent(at: Vec2, outer: f32, offset: f32, bite: f32, angle: f32, color: Color) {
    let d = offset;
    let x = (d * d + outer * outer - bite * bite) / (2.0 * d);
    let y = (outer * outer - x * x).max(0.0).sqrt();
    let phi = y.atan2(x);
    let psi = y.atan2(x - d);
    let (sin, cos) = angle.sin_cos();
    let turn = |v: Vec2| at + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    let n = 32;
    let (mut a, mut b) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let ao = phi + t * (TAU - 2.0 * phi);
        let ai = psi + t * (TAU - 2.0 * psi);
        a.push(turn(vec2(ao.cos(), ao.sin()) * outer));
        b.push(turn(vec2(d + bite * ai.cos(), bite * ai.sin())));
    }
    fill_strip(&a, &b, color);
}

/// Both ends of an address, with an ellipsis between: the head says what kind
/// of address it is, the tail is what a reader checks.
pub fn short_address(address: &str) -> String {
    middle_cut(address, 10, 6)
}

fn middle_cut(address: &str, head: usize, tail: usize) -> String {
    let chars: Vec<char> = address.chars().collect();
    if chars.len() <= head + tail + 4 {
        return address.to_string();
    }
    let h: String = chars[..head].iter().collect();
    let t: String = chars[chars.len() - tail..].iter().collect();
    format!("{h}…{t}")
}

/// The longest middle cut of `address` that fits `w` in the monospace face,
/// so a narrow button keeps both ends rather than losing the tail to a second
/// ellipsis.
fn fit_address(p: &Painter, address: &str, size: f32, w: f32) -> String {
    for (head, tail) in [(10, 6), (8, 5), (6, 4), (5, 3)] {
        let cut = middle_cut(address, head, tail);
        if p.measure_mono(&cut, size).width <= w {
            return cut;
        }
    }
    p.fit_mono(&middle_cut(address, 5, 3), size, w)
}

/// `s` broken into at most `max_lines` lines that fit `w`, at word boundaries;
/// a message that still does not fit is cut at its last line.
fn wrap(p: &Painter, s: &str, size: f32, w: f32, max_lines: usize) -> Vec<String> {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if out.len() == max_lines {
            if let Some(last) = out.last_mut() {
                *last = p.fit(&format!("{last} {}", words[i..].join(" ")), size, w);
            }
            break;
        }
        let mut line = words[i].to_string();
        i += 1;
        while i < words.len() {
            let candidate = format!("{line} {}", words[i]);
            if p.measure(&candidate, size).width > w {
                break;
            }
            line = candidate;
            i += 1;
        }
        out.push(p.fit(&line, size, w));
    }
    out
}

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn cardano() -> ChainOption {
        ChainOption::ready(
            ChainFamily::Cardano,
            vec![
                WalletOption::new("eternl", "Eternl"),
                WalletOption::new("lace", "Lace"),
            ],
        )
    }

    fn evm() -> ChainOption {
        ChainOption::ready(
            ChainFamily::Evm,
            vec![WalletOption::new("io.rabby", "Rabby")],
        )
    }

    fn roadmap() -> WalletModalVm {
        WalletModalVm::new(vec![
            cardano(),
            evm(),
            ChainOption::soon(ChainFamily::Solana),
            ChainOption::soon(ChainFamily::Midnight),
        ])
    }

    /// Run the flow for `secs` without drawing.
    fn run(modal: &mut WalletModal, vm: &WalletModalVm, secs: f32) {
        let frames = (secs / DT).ceil() as usize;
        for _ in 0..frames {
            modal.step(vm, DT);
        }
    }

    #[test]
    fn a_closed_modal_is_not_visible_and_opening_rises_in() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        assert!(!modal.is_visible());
        modal.open();
        modal.step(&vm, DT);
        assert!(modal.is_visible());
        assert!(modal.shown.get() < 1.0, "it rises rather than appearing");
        run(&mut modal, &vm, OPEN_SECS);
        assert_eq!(modal.shown.get(), 1.0);
    }

    #[test]
    fn several_chains_open_on_the_chain_step() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Chains);
        assert!(modal.chain_step);
    }

    #[test]
    fn one_chain_skips_the_chain_step() {
        let vm = WalletModalVm::new(vec![cardano()]);
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Wallets);
        assert_eq!(modal.family(), Some(ChainFamily::Cardano));
        assert!(!modal.chain_step);
    }

    #[test]
    fn open_on_a_chain_skips_the_chain_step_even_when_others_are_listed() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open_on(ChainFamily::Evm);
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Wallets);
        assert_eq!(modal.family(), Some(ChainFamily::Evm));
        assert!(!modal.chain_step);
    }

    #[test]
    fn a_chain_on_the_roadmap_cannot_be_picked() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Chain(ChainFamily::Solana));
        assert_eq!(modal.page(), Page::Chains);
        assert_eq!(modal.family(), None);
    }

    #[test]
    fn picking_a_wallet_asks_the_host_and_moves_to_the_answer() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        assert_eq!(modal.apply(&vm, Act::Chain(ChainFamily::Cardano)), None);
        assert_eq!(modal.page(), Page::Wallets);
        let action = modal.apply(&vm, Act::Wallet("eternl".into()));
        assert_eq!(
            action,
            Some(WalletModalAction::Connect {
                family: ChainFamily::Cardano,
                wallet: "eternl".into()
            })
        );
        assert_eq!(modal.page(), Page::Answer);
    }

    /// The host has not applied the connect yet: the answer step must not
    /// bounce back to the list in that frame.
    #[test]
    fn the_answer_waits_for_the_host_before_it_has_a_phase() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open_on(ChainFamily::Cardano);
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Wallet("eternl".into()));
        run(&mut modal, &vm, 0.5);
        assert_eq!(modal.page(), Page::Answer);
        assert_eq!(
            modal.answer_phase(&vm),
            ChainPhase::Connecting {
                wallet: "eternl".into()
            }
        );
    }

    #[test]
    fn a_host_cancel_returns_to_the_wallets() {
        let mut vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open_on(ChainFamily::Cardano);
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Wallet("eternl".into()));
        vm.set_phase(
            ChainFamily::Cardano,
            ChainPhase::Connecting {
                wallet: "eternl".into(),
            },
        );
        modal.step(&vm, DT);
        assert_eq!(
            modal.apply(&vm, Act::Cancel),
            Some(WalletModalAction::Cancel(ChainFamily::Cardano))
        );
        vm.set_phase(ChainFamily::Cardano, ChainPhase::Idle);
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Wallets);
    }

    #[test]
    fn a_refusal_stays_on_the_answer_and_retries_the_same_wallet() {
        let mut vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open_on(ChainFamily::Cardano);
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Wallet("lace".into()));
        vm.set_phase(
            ChainFamily::Cardano,
            ChainPhase::Declined {
                wallet: "lace".into(),
            },
        );
        run(&mut modal, &vm, 1.0);
        assert_eq!(modal.page(), Page::Answer);
        assert_eq!(
            modal.apply(&vm, Act::Retry),
            Some(WalletModalAction::Connect {
                family: ChainFamily::Cardano,
                wallet: "lace".into()
            })
        );
        modal.apply(&vm, Act::Another);
        assert_eq!(modal.page(), Page::Wallets);
    }

    #[test]
    fn a_connected_chain_opens_on_its_receipt() {
        let mut vm = roadmap();
        vm.set_phase(
            ChainFamily::Cardano,
            ChainPhase::Connected {
                wallet: "eternl".into(),
                address: "addr1".into(),
            },
        );
        let mut modal = WalletModal::new();
        modal.open_on(ChainFamily::Cardano);
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Answer);
        assert_eq!(
            modal.apply(&vm, Act::Disconnect),
            Some(WalletModalAction::Disconnect(ChainFamily::Cardano))
        );
    }

    #[test]
    fn tapping_a_connected_chain_goes_to_its_receipt() {
        let mut vm = roadmap();
        vm.set_phase(
            ChainFamily::Evm,
            ChainPhase::Connected {
                wallet: "io.rabby".into(),
                address: "0x1f".into(),
            },
        );
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Chain(ChainFamily::Evm));
        assert_eq!(modal.page(), Page::Answer);
    }

    #[test]
    fn back_walks_the_steps_and_closes_from_the_first() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Chain(ChainFamily::Cardano));
        modal.apply(&vm, Act::Wallet("eternl".into()));
        modal.back();
        assert_eq!(modal.page(), Page::Wallets);
        modal.back();
        assert_eq!(modal.page(), Page::Chains);
        modal.back();
        assert!(!modal.is_open());
        assert!(modal.is_visible(), "still leaving");
        run(&mut modal, &vm, CLOSE_SECS + DT);
        assert!(!modal.is_visible());
    }

    #[test]
    fn back_from_the_only_chains_wallets_closes() {
        let vm = WalletModalVm::new(vec![cardano()]);
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        modal.back();
        assert!(!modal.is_open());
    }

    #[test]
    fn a_step_slides_in_and_the_old_one_leaves() {
        let vm = roadmap();
        let mut modal = WalletModal::new().settled();
        modal.open();
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Chain(ChainFamily::Cardano));
        assert_eq!(modal.leaving, Some(Page::Chains));
        assert_eq!(modal.slide.get(), 0.0);
        run(&mut modal, &vm, SLIDE_SECS);
        assert_eq!(modal.slide.get(), 1.0);
    }

    #[test]
    fn reopening_starts_fresh() {
        let vm = roadmap();
        let mut modal = WalletModal::new();
        modal.open();
        modal.step(&vm, DT);
        modal.apply(&vm, Act::Chain(ChainFamily::Cardano));
        modal.close();
        run(&mut modal, &vm, 1.0);
        modal.open();
        modal.step(&vm, DT);
        assert_eq!(modal.page(), Page::Chains);
        assert_eq!(modal.family(), None);
    }

    #[test]
    fn an_address_keeps_both_ends() {
        let a = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";
        let s = short_address(a);
        assert!(s.starts_with("stake1u896"));
        assert!(s.ends_with("c5rhq9"));
        assert_eq!(short_address("0x1f"), "0x1f");
    }

    #[test]
    fn a_name_keeps_its_colour() {
        assert_eq!(name_hash("Eternl"), name_hash("Eternl"));
        assert_ne!(name_hash("Eternl"), name_hash("Lace"));
    }
}
