//! Phosphor icon font integration for egui
//!
//! Uses the Phosphor Icons Regular weight TTF font (MIT License).
//! <https://phosphoricons.com>
//!
//! Call [`install_phosphor_font`] once during app setup, then use [`PhosphorIcon`]
//! to render icons with arbitrary size and color.

use egui::{Color32, FontFamily, FontId, Pos2, RichText, Ui};

/// The font family name registered for Phosphor icons.
pub const PHOSPHOR_FAMILY_NAME: &str = "phosphor-icons";

/// Font family for Phosphor icons. Use after calling [`install_phosphor_font`].
pub fn phosphor_family() -> FontFamily {
    FontFamily::Name(PHOSPHOR_FAMILY_NAME.into())
}

/// Register the Phosphor icon font with the egui context.
/// Call once during app initialization (e.g. in `CreationContext` setup).
pub fn install_phosphor_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

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

    ctx.set_fonts(fonts);
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
    Question,
    Eye,
    Gear,

    // Actions
    Trash,
    ArrowsOut,
    SignOut,
    Plus,
    Minus,
    X,

    // Arrows
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Misc
    Spiral,
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
            Self::Heart => '\u{e2a8}',
            Self::List => '\u{e2f0}',
            Self::Clock => '\u{e19a}',
            Self::Hourglass => '\u{e2b2}',
            Self::Play => '\u{e3d0}',
            Self::Flag => '\u{e244}',
            Self::Warning => '\u{e4e0}',
            Self::User => '\u{e4c2}',
            Self::Question => '\u{e3e8}',
            Self::Eye => '\u{e220}',
            Self::Gear => '\u{e270}',
            Self::Trash => '\u{e4a6}',
            Self::ArrowsOut => '\u{e0a2}',
            Self::SignOut => '\u{e42a}',
            Self::Plus => '\u{e3d4}',
            Self::Minus => '\u{e32a}',
            Self::X => '\u{e4f6}',
            Self::ArrowUp => '\u{e08e}',
            Self::ArrowDown => '\u{e03e}',
            Self::ArrowLeft => '\u{e058}',
            Self::ArrowRight => '\u{e06c}',
            Self::Spiral => '\u{e9fa}',
        }
    }

    /// Icon as a single-char string.
    pub fn as_str(self) -> String {
        self.codepoint().to_string()
    }

    /// Create [`RichText`] for this icon with the given size and color.
    pub fn rich_text(self, size: f32, color: Color32) -> RichText {
        RichText::new(self.as_str())
            .font(FontId::new(size, phosphor_family()))
            .color(color)
    }

    /// Display this icon as an egui label.
    pub fn show(self, ui: &mut Ui, size: f32, color: Color32) -> egui::Response {
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
        Self::Heart,
        Self::List,
        Self::Clock,
        Self::Hourglass,
        Self::Play,
        Self::Flag,
        Self::Warning,
        Self::User,
        Self::Question,
        Self::Eye,
        Self::Gear,
        Self::Trash,
        Self::ArrowsOut,
        Self::SignOut,
        Self::Plus,
        Self::Minus,
        Self::X,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::Spiral,
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
            Self::Heart => "Heart",
            Self::List => "List",
            Self::Clock => "Clock",
            Self::Hourglass => "Hourglass",
            Self::Play => "Play",
            Self::Flag => "Flag",
            Self::Warning => "Warning",
            Self::User => "User",
            Self::Question => "Question",
            Self::Eye => "Eye",
            Self::Gear => "Gear",
            Self::Trash => "Trash",
            Self::ArrowsOut => "Arrows Out",
            Self::SignOut => "Sign Out",
            Self::Plus => "Plus",
            Self::Minus => "Minus",
            Self::X => "X",
            Self::ArrowUp => "Arrow Up",
            Self::ArrowDown => "Arrow Down",
            Self::ArrowLeft => "Arrow Left",
            Self::ArrowRight => "Arrow Right",
            Self::Spiral => "Spiral",
        }
    }
}
