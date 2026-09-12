//! Phosphor icon font integration for egui
//!
//! Uses the Phosphor Icons Regular weight TTF font (MIT License).
//! <https://phosphoricons.com>
//!
//! Call [`install_phosphor_font`] once during app setup, then use [`PhosphorIcon`]
//! to render icons with arbitrary size and color.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, FontFamily, FontId, Pos2, RichText, Ui};

/// The font family name registered for Phosphor icons.
pub const PHOSPHOR_FAMILY_NAME: &str = "phosphor-icons";

/// Tracks whether the font has been installed in ANY context in this process.
/// Kept only to back the deprecated [`phosphor_font_installed`] query — the
/// install itself is guarded per-context.
static FONT_INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Whether the Phosphor family is bound in the context currently being laid
    /// out, as last observed by [`ensure_fonts`].
    ///
    /// # Why this is not simply a bool on the context
    ///
    /// [`phosphor_family`] is the funnel every icon passes through — all ~57
    /// `rich_text` call sites in this crate and every one downstream — and it
    /// takes no `Context`, so it cannot ask. A thread-local can be consulted
    /// from there, and egui lays a context out on one thread at a time.
    ///
    /// It starts **true**, deliberately. False would mean any caller that never
    /// reaches [`ensure_fonts`] silently loses its icons; true means such a
    /// caller behaves exactly as it did before this existed. The value only
    /// becomes false where we have positively observed the family missing.
    static PHOSPHOR_READY: Cell<bool> = const { Cell::new(true) };
}

/// Font family for Phosphor icons.
///
/// Falls back to [`FontFamily::Proportional`] for the single pass between
/// [`install_phosphor_font`] queueing the font and egui binding it — see
/// [`ensure_fonts`]. Laying text out in an unbound family is not a soft failure
/// in epaint, it is a panic (`"… is not bound to any fonts"`), and in a wasm
/// host that is a white screen.
pub fn phosphor_family() -> FontFamily {
    if PHOSPHOR_READY.with(Cell::get) {
        FontFamily::Name(PHOSPHOR_FAMILY_NAME.into())
    } else {
        FontFamily::Proportional
    }
}

/// Make this crate's icons safe to lay out in the pass that is running now, and
/// report whether they will actually render.
///
/// Every widget in this crate calls this at the top of its `show`. Takes a
/// [`Ui`] rather than a [`egui::Context`] on purpose: a `Ui` only exists inside
/// a pass, which is what makes the readiness probe below safe — `Context::fonts`
/// panics before the first pass, which is exactly where a host installs fonts.
///
/// # The one-pass gap this closes
///
/// `Context::set_fonts` does not install anything; it stores the definitions on
/// [`egui::Memory`] and egui binds them at the *start of the next pass*. So the
/// `install_phosphor_font` call at the top of a widget's `show` cannot serve the
/// pass that triggered it — the very next line asking for a Phosphor glyph
/// panicked. Every current host installs fonts at startup and so never met it,
/// which is precisely why it sat here unnoticed: the self-install advertised as
/// being "for safety" was load-bearing for nobody and protected no one.
pub fn ensure_fonts(ui: &Ui) -> bool {
    ensure_fonts_in_pass(ui.ctx())
}

/// [`ensure_fonts`] for a caller that is rendering but holds only a
/// [`egui::Context`] — a toast queue, a modal that opens its own `Window`.
///
/// # Panics
///
/// Named for its one precondition: `Context::fonts` panics before the first
/// pass, so the caller must already be inside one. A `&Ui` or a `&Painter`
/// proves that on its own, which is why [`ensure_fonts`] is the one to reach
/// for. Do **not** call this from `App::new` — that is
/// [`install_phosphor_font`]'s job.
pub fn ensure_fonts_in_pass(ctx: &egui::Context) -> bool {
    install_phosphor_font(ctx);
    // What egui is actually holding, not what we asked it for.
    let ready = ctx.fonts(|f| {
        f.families()
            .iter()
            .any(|family| matches!(family, FontFamily::Name(n) if &**n == PHOSPHOR_FAMILY_NAME))
    });
    PHOSPHOR_READY.with(|c| c.set(ready));
    ready
}

/// Name of the bundled broad-coverage fallback font.
pub const FALLBACK_FAMILY_NAME: &str = "dejavu-fallback";

/// Register the bundled fonts with the egui context: the **Phosphor** icon font (its
/// own family, referenced via [`PhosphorIcon`]) and **DejaVu Sans** as a broad-coverage
/// *fallback* appended to both standard families. egui has no automatic font fallback,
/// so without this any glyph the primary font lacks (accented/international text, maths,
/// symbols) renders as a `□` tofu box — appending DejaVu (which covers Latin-ext, Greek,
/// Cyrillic, arrows, maths and many symbols) catches them. Intentional icons should
/// still use [`PhosphorIcon`]; this is the safety net for arbitrary *data* strings.
///
/// Safe to call multiple times — only queues once per context.
///
/// **Call this at startup**, from `App::new` with `&cc.egui_ctx`, before
/// [`crate::theme::configure_style`]. It only *queues* the definitions: egui
/// binds them at the start of the next pass, so calling it from inside a pass
/// leaves that pass without icons. Widgets in this crate call [`ensure_fonts`]
/// instead, which handles that gap; this is the host's entry point, and takes a
/// [`egui::Context`] because at startup there is no [`Ui`] yet.
pub fn install_phosphor_font(ctx: &egui::Context) {
    // Guarded PER CONTEXT, not per process. Fonts live in the context, so a
    // process-wide flag means the SECOND context in a process (a second
    // viewport, a test running beside another) never gets them and either
    // renders tofu or panics with "not bound to any fonts".
    //
    // "queued", not "installed": all this does is hand egui the definitions.
    let queued = egui::Id::new("egui-widgets/fonts-installed");
    if ctx.data(|d| d.get_temp::<bool>(queued)).unwrap_or(false) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(queued, true));
    FONT_INSTALLED.store(true, Ordering::Relaxed);

    let mut fonts = egui::FontDefinitions::default();

    // Phosphor icons — its own family, selected explicitly by `PhosphorIcon::*`.
    fonts.font_data.insert(
        PHOSPHOR_FAMILY_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../fonts/Phosphor.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Name(PHOSPHOR_FAMILY_NAME.into()))
        .or_default()
        .push(PHOSPHOR_FAMILY_NAME.to_owned());

    // DejaVu Sans — broad-coverage fallback APPENDED (lowest priority) to both standard
    // families, so the primary font wins for normal text and DejaVu only fills its gaps.
    fonts.font_data.insert(
        FALLBACK_FAMILY_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../fonts/DejaVuSans.ttf"
        ))),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(FALLBACK_FAMILY_NAME.to_owned());
    }

    ctx.set_fonts(fonts);
    // The icons are missing until the pass after this one, and an egui host
    // only repaints when something asks it to. Without this the first frame
    // that can draw icons waits on the next mouse move.
    ctx.request_repaint();
}

/// Alias for [`install_phosphor_font`] — installs all bundled fonts (Phosphor icons +
/// the DejaVu coverage fallback). Prefer this name in new code.
pub fn install_fonts(ctx: &egui::Context) {
    install_phosphor_font(ctx);
}

/// Whether [`install_phosphor_font`] has ever been called in this process.
///
/// Note what this does **not** tell you: whether the font is bound in any
/// particular context, or usable in the pass you are in. Fonts live on a
/// context and arrive a pass later than the call that queued them. Use
/// [`ensure_fonts`] for the question you almost certainly mean.
pub fn phosphor_font_installed() -> bool {
    FONT_INSTALLED.load(Ordering::Relaxed)
}

/// Phosphor icon identifiers with their Unicode codepoints.
///
/// This is a curated subset relevant to game/NFT UIs. The full Phosphor set
/// has 4500+ icons — add more variants as needed from the codepoint mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhosphorIcon {
    // Navigation / Position
    MapPin,
    Anchor,
    Compass,
    Path,
    Boat,

    // Combat / Stats
    Sword,
    Shield,
    Lightning,
    Crosshair,
    Fire,
    Skull,

    // Resources
    Package,
    Coins,
    Star,
    Crown,
    Heart,

    // Queue / Time
    List,
    Clock,
    Hourglass,
    Play,

    // Status
    Flag,
    Warning,
    User,
    Wallet,
    Question,
    Eye,
    Gear,

    // Actions
    Copy,
    Trash,
    ArrowsOut,
    SignOut,
    Plus,
    Minus,
    X,
    PencilSimple,
    Check,
    CheckCircle,
    Lock,
    LockOpen,
    Handshake,

    // Arrows
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowsDownUp,
    ArrowsClockwise,

    // Misc
    Spiral,
    CaretRight,
    CaretDown,
    MagnifyingGlass,
}

impl PhosphorIcon {
    /// Unicode codepoint for this icon in the Phosphor Regular font.
    pub const fn codepoint(self) -> char {
        match self {
            Self::MapPin => '\u{e316}',
            Self::Anchor => '\u{e514}',
            Self::Compass => '\u{e1c8}',
            Self::Path => '\u{e39c}',
            Self::Boat => '\u{e786}',
            Self::Sword => '\u{e5ba}',
            Self::Shield => '\u{e40a}',
            Self::Lightning => '\u{e2de}',
            Self::Crosshair => '\u{e1d6}',
            Self::Fire => '\u{e242}',
            Self::Skull => '\u{e916}',
            Self::Package => '\u{e390}',
            Self::Coins => '\u{e78e}',
            Self::Star => '\u{e46a}',
            Self::Crown => '\u{e614}',
            Self::Heart => '\u{e2a8}',
            Self::List => '\u{e2f0}',
            Self::Clock => '\u{e19a}',
            Self::Hourglass => '\u{e2b2}',
            Self::Play => '\u{e3d0}',
            Self::Flag => '\u{e244}',
            Self::Warning => '\u{e4e0}',
            Self::User => '\u{e4c2}',
            Self::Wallet => '\u{e68a}',
            Self::Question => '\u{e3e8}',
            Self::Eye => '\u{e220}',
            Self::Gear => '\u{e270}',
            Self::Copy => '\u{e1ca}',
            Self::Trash => '\u{e4a6}',
            Self::ArrowsOut => '\u{e0a2}',
            Self::SignOut => '\u{e42a}',
            Self::Plus => '\u{e3d4}',
            Self::Minus => '\u{e32a}',
            Self::X => '\u{e4f6}',
            Self::PencilSimple => '\u{e3b4}',
            Self::Check => '\u{e182}',
            Self::CheckCircle => '\u{e184}',
            Self::Lock => '\u{e2fa}',
            Self::LockOpen => '\u{e306}',
            Self::Handshake => '\u{e582}',
            Self::ArrowUp => '\u{e08e}',
            Self::ArrowDown => '\u{e03e}',
            Self::ArrowLeft => '\u{e058}',
            Self::ArrowRight => '\u{e06c}',
            Self::ArrowsDownUp => '\u{e098}',
            Self::ArrowsClockwise => '\u{e094}',
            Self::Spiral => '\u{e9fa}',
            Self::CaretRight => '\u{e13a}',
            Self::CaretDown => '\u{e136}',
            Self::MagnifyingGlass => '\u{e30c}',
        }
    }

    /// Icon as a single-char string.
    pub fn as_str(self) -> String {
        self.codepoint().to_string()
    }

    /// Create [`RichText`] for this icon with the given size and color.
    ///
    /// Resolves its family through [`phosphor_family`], so it is safe to lay
    /// out in a pass where the font is not yet bound **provided** something has
    /// called [`ensure_fonts`] on this thread — which every widget in this crate
    /// does before it draws.
    pub fn rich_text(self, size: f32, color: Color32) -> RichText {
        RichText::new(self.as_str())
            .font(FontId::new(size, phosphor_family()))
            .color(color)
    }

    /// Display this icon as an egui label.
    pub fn show(self, ui: &mut Ui, size: f32, color: Color32) -> egui::Response {
        ensure_fonts(ui);
        ui.label(self.rich_text(size, color))
    }

    /// Paint this icon at a specific position using the painter.
    pub fn paint(
        self,
        painter: &egui::Painter,
        pos: Pos2,
        align: egui::Align2,
        size: f32,
        color: Color32,
    ) {
        // A `Painter` exists only inside a pass, same as a `Ui`.
        ensure_fonts_in_pass(painter.ctx());
        painter.text(
            pos,
            align,
            self.as_str(),
            FontId::new(size, phosphor_family()),
            color,
        );
    }

    /// All icons in this enum, useful for galleries/demos.
    pub const ALL: &'static [PhosphorIcon] = &[
        Self::MapPin,
        Self::Anchor,
        Self::Compass,
        Self::Path,
        Self::Boat,
        Self::Sword,
        Self::Shield,
        Self::Lightning,
        Self::Crosshair,
        Self::Fire,
        Self::Skull,
        Self::Package,
        Self::Coins,
        Self::Star,
        Self::Crown,
        Self::Heart,
        Self::List,
        Self::Clock,
        Self::Hourglass,
        Self::Play,
        Self::Flag,
        Self::Warning,
        Self::User,
        Self::Wallet,
        Self::Question,
        Self::Eye,
        Self::Gear,
        Self::Copy,
        Self::Trash,
        Self::ArrowsOut,
        Self::SignOut,
        Self::Plus,
        Self::Minus,
        Self::X,
        Self::PencilSimple,
        Self::Check,
        Self::CheckCircle,
        Self::Lock,
        Self::LockOpen,
        Self::Handshake,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::ArrowsDownUp,
        Self::ArrowsClockwise,
        Self::Spiral,
        Self::CaretRight,
        Self::CaretDown,
        Self::MagnifyingGlass,
    ];

    /// Human-readable name for display.
    pub const fn name(self) -> &'static str {
        match self {
            Self::MapPin => "Map Pin",
            Self::Anchor => "Anchor",
            Self::Compass => "Compass",
            Self::Path => "Path",
            Self::Boat => "Boat",
            Self::Sword => "Sword",
            Self::Shield => "Shield",
            Self::Lightning => "Lightning",
            Self::Crosshair => "Crosshair",
            Self::Fire => "Fire",
            Self::Skull => "Skull",
            Self::Package => "Package",
            Self::Coins => "Coins",
            Self::Star => "Star",
            Self::Crown => "Crown",
            Self::Heart => "Heart",
            Self::List => "List",
            Self::Clock => "Clock",
            Self::Hourglass => "Hourglass",
            Self::Play => "Play",
            Self::Flag => "Flag",
            Self::Warning => "Warning",
            Self::User => "User",
            Self::Wallet => "Wallet",
            Self::Question => "Question",
            Self::Eye => "Eye",
            Self::Gear => "Gear",
            Self::Copy => "Copy",
            Self::Trash => "Trash",
            Self::ArrowsOut => "Arrows Out",
            Self::SignOut => "Sign Out",
            Self::Plus => "Plus",
            Self::Minus => "Minus",
            Self::X => "X",
            Self::PencilSimple => "Pencil Simple",
            Self::Check => "Check",
            Self::CheckCircle => "Check Circle",
            Self::Lock => "Lock",
            Self::LockOpen => "Lock Open",
            Self::Handshake => "Handshake",
            Self::ArrowUp => "Arrow Up",
            Self::ArrowDown => "Arrow Down",
            Self::ArrowLeft => "Arrow Left",
            Self::ArrowRight => "Arrow Right",
            Self::ArrowsDownUp => "Arrows Down Up",
            Self::ArrowsClockwise => "Arrows Clockwise",
            Self::Spiral => "Spiral",
            Self::CaretRight => "Caret Right",
            Self::CaretDown => "Caret Down",
            Self::MagnifyingGlass => "Magnifying Glass",
        }
    }
}

/// A Phosphor glyph followed by proportional label text, as one
/// `WidgetText`.
///
/// **Use this rather than formatting the glyph into a string.** A
/// `RichText` renders in ONE family, so `format!("{icon} {label}")` looks
/// up the Phosphor codepoint in the proportional font, does not find it,
/// and draws tofu — with no error anywhere. Only a `LayoutJob` can give
/// the glyph and the label different families.
///
/// Both runs resolve to `TextStyle::Small`, so an icon+label widget matches
/// a text-only sibling. Colour stays `PLACEHOLDER` so the host widget's
/// enabled / hovered / selected state carries through automatically.
///
/// Callers must still have called [`install_phosphor_font`] — the family
/// has to exist before a `FontId` can name it.
pub fn phosphor_label(ui: &Ui, icon: PhosphorIcon, label: &str) -> egui::WidgetText {
    use egui::text::LayoutJob;
    use egui::{TextFormat, TextStyle};

    let small = TextStyle::Small.resolve(ui.style());
    let mut job = LayoutJob::default();
    job.append(
        &icon.as_str(),
        0.0,
        TextFormat {
            font_id: FontId::new(small.size, phosphor_family()),
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    job.append(
        &format!(" {label}"),
        0.0,
        TextFormat {
            font_id: FontId::new(small.size, FontFamily::Proportional),
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    job.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2, Rect};

    /// Draw an icon on a context that has never had fonts installed, `passes`
    /// times, and report whether the icon was ever laid out in its real family.
    fn draw_icon_on_a_fresh_context(passes: usize) -> Vec<bool> {
        let ctx = egui::Context::default();
        let mut observed = Vec::new();
        for _ in 0..passes {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 200.0))),
                ..Default::default()
            });
            egui::Area::new(Id::new("icon")).show(&ctx, |ui| {
                // The call every widget in this crate makes, followed
                // immediately by laying a glyph out — the sequence that panicked.
                let ready = ensure_fonts(ui);
                PhosphorIcon::X.show(ui, 12.0, Color32::WHITE);
                observed.push(ready);
            });
            let _ = ctx.end_pass();
        }
        observed
    }

    #[test]
    fn a_host_that_never_installed_the_fonts_does_not_panic() {
        // `Context::set_fonts` hands egui the definitions and egui binds them at
        // the START OF THE NEXT PASS. So the self-install at the top of a
        // widget's `show` could never serve the pass that triggered it, and the
        // next line asking for a Phosphor glyph took the whole app down —
        // a white screen under wasm. This test is the regression: reaching the
        // assert at all is the thing being asserted.
        let observed = draw_icon_on_a_fresh_context(1);
        assert_eq!(observed, vec![false], "pass 1 cannot have the font yet");
    }

    #[test]
    fn the_icons_arrive_on_the_very_next_pass() {
        // The fallback is for exactly one pass. If it were sticky, every host
        // that forgot the startup call would silently render no icons forever,
        // which is a worse failure than the panic because nothing reports it.
        let observed = draw_icon_on_a_fresh_context(3);
        assert_eq!(observed, vec![false, true, true]);
    }

    #[test]
    fn the_family_reported_follows_readiness() {
        // `phosphor_family` is the funnel: it is what makes ~57 `rich_text` call
        // sites safe without any of them being touched.
        PHOSPHOR_READY.with(|c| c.set(false));
        assert_eq!(phosphor_family(), FontFamily::Proportional);
        PHOSPHOR_READY.with(|c| c.set(true));
        assert_eq!(
            phosphor_family(),
            FontFamily::Name(PHOSPHOR_FAMILY_NAME.into())
        );
    }

    #[test]
    fn readiness_defaults_to_optimistic_on_a_fresh_thread() {
        // False would mean any caller that never reaches `ensure_fonts` — a
        // downstream crate building its own `FontId` from `phosphor_family` —
        // silently loses its icons. It must behave exactly as it did before.
        let on_its_own_thread = std::thread::spawn(|| PHOSPHOR_READY.with(Cell::get))
            .join()
            .unwrap();
        assert!(on_its_own_thread);
    }
}
