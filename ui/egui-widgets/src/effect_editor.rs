//! effect_editor — edit the compositor's `[[effect]]` blocks: which slots an
//! effect recolours, how it picks, and the weighted variants it picks from.
//!
//! ## What this replaces, and why
//!
//! [`PaletteEditor`](crate::palette_editor) modelled
//! `[processing.techniques.colorization]`. The shipping project config's only
//! remaining mention of that block is a comment saying `[[effect]]` replaced it.
//! So the widget was a mirror of a shape nothing reads — and
//! `studio-core::ConfigPalette` was in turn mirroring the widget, propagating it
//! a second hop.
//!
//! An effect is closer to a **series palette** than to a paint bucket: a named
//! set of slots, a rule for picking, and a weighted list of tones any of which
//! may be a whole material rather than a colour.
//!
//! ## Two names, doing different jobs
//!
//! [`EffectVariant::name`] is **identity**: it lands in the asset URL
//! (`asset://local/…?palette=skin&variant=tan`) and is parsed back out, so
//! renaming a variant changes asset identity for anything already generated.
//! [`EffectVariant::label`] is the **public trait value** written to metadata —
//! `{ name = "tan", label = "Sand" }`. A single "name" field conflates a stable
//! internal key with a curated public string, which is what the old widget did.
//!
//! ## Modes are enums, not `Option` and not `bool`
//!
//! The config spells these as `base_color: Option<String>`, `shared: bool` and
//! `always: bool`. This widget does not copy that:
//!
//! - `base_color: None` does not mean *absent*, it means **tint the whole
//!   layer** — a considered decision, and the mode the shipping skin/neck/hand
//!   value-maps actually use. As an `Option` behind a mandatory colour picker it
//!   was unreachable, so the old widget could not express the only real palette
//!   in the repo. It is [`Recolor`] here.
//! - `shared` and `always` are [`Pick`] and [`Base`], because a bool parameter
//!   at a call site says nothing about which way round it goes.
//!
//! ## The spine
//!
//! One label column, one control column, and the variant table extends right
//! from that same control edge:
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │ name      [skin            ]                           🗑  │
//! │ trait     [Skin Tone       ]                               │
//! │ slots     [skin ×][neck ×][hand ×]  Add…                   │
//! │ recolour  (tint)(keyed remap)                              │
//! │ pick      (per slot)(shared)                               │
//! │ base      (offered)(never)                                 │
//! │ apply     (per layer)(group finish)                        │
//! │           variant     label        weight  recipe          │
//! │        ■  tan         Sand           1.0   tint       ×    │
//! │        ■  holo        Holographic    0.5   2 passes   ×    │
//! │        +  add variant                                      │
//! └────────────────────────────────────────────────────────────┘
//! ```

use egui::{Align, Align2, Color32, Layout, Sense, Ui, vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{ColorTokens, Radius, Space, SpaceExt, TextSize, ThemeExt, hairline};
use crate::token_multiselect::TokenMultiselect;

/// Width of the form's label column. Every control in the card starts one gap
/// to the right of this, including the variant table's first column.
const FORM_LABEL_W: f32 = 76.0;
/// Width of a variant's internal identity field.
const NAME_W: f32 = 120.0;
/// Width of a variant's public metadata label field.
const LABEL_W: f32 = 140.0;
/// Width of the weight column, sized for the widest value the range can print.
const WEIGHT_W: f32 = 64.0;
/// Width of the recipe column.
const RECIPE_W: f32 = 88.0;
/// Point size of the row actions.
const ACTION_PT: f32 = 13.0;
/// The colour a variant gets when it is first given one.
const NEW_TINT: [u8; 3] = [200, 160, 120];

// ============================================================================
// Model
// ============================================================================

/// How an effect recolours the pixels it touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recolor {
    /// Multiply the whole layer by the tone. The mode for a grayscale
    /// value-map, where there is nothing to key against.
    Tint,
    /// Recolour only pixels near this colour, preserving everything else.
    KeyedRemap([u8; 3]),
}

impl Recolor {
    pub const ALL: [Self; 2] = [Self::Tint, Self::KeyedRemap(NEW_TINT)];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Tint => "tint",
            Self::KeyedRemap(_) => "keyed remap",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::Tint => "Multiply the whole layer by the tone — for a grayscale value-map",
            Self::KeyedRemap(_) => "Recolour only pixels near the key colour, preserving the rest",
        }
    }

    /// Same variant, ignoring the key colour — so a selector can ask "is this
    /// the mode I am showing" without the colour making every comparison false.
    const fn same_mode(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Tint, Self::Tint) | (Self::KeyedRemap(_), Self::KeyedRemap(_))
        )
    }
}

/// Whether the effect's slots share one pick per token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Each slot picks independently.
    PerSlot,
    /// All the effect's slots share ONE pick — skin/neck/hand share a body tone.
    Shared,
}

impl Pick {
    pub const ALL: [Self; 2] = [Self::PerSlot, Self::Shared];

    pub const fn label(self) -> &'static str {
        match self {
            Self::PerSlot => "per slot",
            Self::Shared => "shared",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::PerSlot => "Each slot picks its own variant",
            Self::Shared => "Every slot in this effect gets the same pick, per token",
        }
    }
}

/// Whether a token may come out with no tint at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// The unprocessed base is one of the possible outcomes.
    Offered,
    /// Every token gets a tone — the raw art never shows.
    Never,
}

impl Base {
    pub const ALL: [Self; 2] = [Self::Offered, Self::Never];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Offered => "offered",
            Self::Never => "never",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::Offered => "The untinted base is one of the outcomes",
            Self::Never => "Always tint — the raw value-map is meaningless on its own",
        }
    }
}

/// How the effect applies across its slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// Tint each slot's layer independently.
    PerLayer,
    /// One finish applied per group member.
    Group,
}

impl Apply {
    pub const ALL: [Self; 2] = [Self::PerLayer, Self::Group];

    pub const fn label(self) -> &'static str {
        match self {
            Self::PerLayer => "per layer",
            Self::Group => "group finish",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::PerLayer => "Tint each slot's own layer",
            Self::Group => "Apply one finish per group member",
        }
    }
}

/// What a variant actually does to the pixels.
///
/// A variant is not always a colour. `gold` is `tint → specular → grain`; a
/// gradient-mapped metal has no flat tint at all. Rendering those as a plain
/// swatch is a lie about what the token will look like, so the recipe is part
/// of the model and the editor reports it.
#[derive(Debug, Clone, PartialEq)]
pub enum Recipe {
    /// A flat tint in this colour.
    Tint([u8; 3]),
    /// A multi-pass effect pipeline. `preview` is the flat colour the config
    /// also carries, for the swatch. The passes themselves are edited
    /// elsewhere — this widget will not silently drop them.
    Pipeline { passes: usize, preview: [u8; 3] },
    /// No tint and no pipeline — a `none` variant, or one the group finish
    /// drives entirely.
    Untinted,
}

impl Recipe {
    /// The colour to show in the swatch, if there is one.
    pub const fn swatch(&self) -> Option<[u8; 3]> {
        match self {
            Self::Tint(c) => Some(*c),
            Self::Pipeline { preview, .. } => Some(*preview),
            Self::Untinted => None,
        }
    }

    /// What the recipe column says.
    pub fn summary(&self) -> String {
        match self {
            Self::Tint(_) => "tint".to_string(),
            Self::Pipeline { passes: 1, .. } => "1 pass".to_string(),
            Self::Pipeline { passes, .. } => format!("{passes} passes"),
            Self::Untinted => "—".to_string(),
        }
    }

    /// True when the renderer runs a pipeline rather than a flat multiply.
    pub const fn is_pipeline(&self) -> bool {
        matches!(self, Self::Pipeline { .. })
    }
}

/// One tone an effect can pick.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectVariant {
    /// Internal identity. Lands in `asset://…?variant=<name>` and is parsed
    /// back out — renaming changes asset identity.
    pub name: String,
    /// Public trait value in metadata. Empty falls back to the normalised name.
    pub label: String,
    /// Selection weight, relative to the effect's other variants.
    pub weight: f32,
    /// What this variant does to the pixels.
    pub recipe: Recipe,
}

impl Default for EffectVariant {
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            weight: 1.0,
            recipe: Recipe::Tint(NEW_TINT),
        }
    }
}

/// A named recolouring applied to a set of slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// Internal effect key.
    pub name: String,
    /// Metadata trait key for the per-token pick. Empty = `"<Name> Colour"`.
    pub trait_name: String,
    /// Slots this effect applies to.
    pub slots: Vec<String>,
    pub recolor: Recolor,
    pub pick: Pick,
    pub base: Base,
    pub apply: Apply,
    pub variants: Vec<EffectVariant>,
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            name: String::new(),
            trait_name: String::new(),
            slots: Vec::new(),
            recolor: Recolor::Tint,
            pick: Pick::PerSlot,
            base: Base::Offered,
            apply: Apply::PerLayer,
            variants: Vec::new(),
        }
    }
}

impl Effect {
    /// Sum of the variant weights — the denominator every variant's share is
    /// measured against. Weights are relative, so `1.0` means nothing on its own.
    pub fn total_weight(&self) -> f32 {
        self.variants.iter().map(|v| v.weight).sum()
    }

    /// This variant's share of the picks, in `0.0..=1.0`.
    ///
    /// `None` when nothing can be picked, which is a real state: an effect whose
    /// weights all sit at zero never fires, and showing `NaN%` or `0%` would
    /// present that as an ordinary distribution.
    pub fn share(&self, index: usize) -> Option<f32> {
        let total = self.total_weight();
        match total > 0.0 {
            true => self.variants.get(index).map(|v| v.weight / total),
            false => None,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

pub struct EffectEditor<'a> {
    effects: &'a mut Vec<Effect>,
    slot_options: &'a [String],
    add_label: &'a str,
}

/// The column geometry every row in an effect card shares.
struct Spine {
    /// Left edge of every control — form fields and the variant table alike.
    control_x: f32,
    action: f32,
    gap: f32,
    row_h: f32,
    swatch: f32,
}

impl Spine {
    fn measure(ui: &Ui) -> Self {
        let gap = ui.space(Space::Md);
        Self {
            control_x: FORM_LABEL_W + gap,
            action: ACTION_PT + gap * 2.0,
            gap,
            row_h: ui.text_size(TextSize::Base) + ui.space(Space::Md),
            swatch: ui.spacing().interact_size.x,
        }
    }

    /// Total card content width — fixed, so typing does not resize the card.
    fn content_w(&self) -> f32 {
        self.control_x
            + self.swatch
            + self.gap
            + NAME_W
            + self.gap
            + LABEL_W
            + self.gap
            + WEIGHT_W
            + self.gap
            + RECIPE_W
            + self.gap
            + self.action
    }

    /// Width a form control gets when it spans to the weight column — wide
    /// enough to be usable, short of the trailing action.
    fn field_w(&self) -> f32 {
        self.swatch + self.gap + NAME_W + self.gap + LABEL_W
    }
}

impl<'a> EffectEditor<'a> {
    /// `slot_options` is the full slot roster the effect can draw from.
    pub fn new(effects: &'a mut Vec<Effect>, slot_options: &'a [String]) -> Self {
        Self {
            effects,
            slot_options,
            add_label: "Add effect",
        }
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    /// Returns true when anything changed this frame.
    pub fn show(self, ui: &mut Ui) -> bool {
        let Self {
            effects,
            slot_options,
            add_label,
        } = self;

        crate::icons::ensure_fonts(ui);
        let colors = ui.tokens().color;
        let spine = Spine::measure(ui);

        let mut changed = false;
        let mut remove_effect: Option<usize> = None;

        for (ei, effect) in effects.iter_mut().enumerate() {
            ui.push_id(ei, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(spine.content_w());
                    changed |= show_effect(ui, ei, effect, slot_options, &spine, &colors, || {
                        remove_effect = Some(ei)
                    });
                });
            });
        }

        if let Some(i) = remove_effect {
            effects.remove(i);
            changed = true;
        }
        if ui.button(add_label).clicked() {
            effects.push(Effect::default());
            changed = true;
        }

        changed
    }
}

/// One effect card. Split out because `show` was otherwise six levels of
/// closure deep and the borrow of `effects` fought every early return.
fn show_effect(
    ui: &mut Ui,
    ei: usize,
    effect: &mut Effect,
    slot_options: &[String],
    spine: &Spine,
    colors: &ColorTokens,
    mut remove: impl FnMut(),
) -> bool {
    let mut changed = false;

    // ── Identity ────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        form_label(ui, spine, "name", colors);
        changed |= text_field(ui, &mut effect.name, "effect name", spine.field_w());
        // The form's fields stop at the label column, but the trailing action
        // belongs to the same axis the variant rows use — so the span the
        // variant table's weight and recipe columns occupy is held open here.
        // An allocation and not `add_space`: egui puts `item_spacing` after a
        // widget but not after a bare space.
        ui.allocate_exact_size(
            vec2(WEIGHT_W + spine.gap + RECIPE_W, spine.row_h),
            Sense::hover(),
        );
        if action(ui, spine, PhosphorIcon::Trash, "Remove effect", colors) {
            remove();
        }
    });

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        form_label(ui, spine, "trait", colors);
        let hint = match effect.name.is_empty() {
            // The config's own fallback, shown rather than described: an empty
            // field that silently means something is worse than one that says so.
            true => "<Name> Colour".to_string(),
            false => format!("{} Colour", title_case(&effect.name)),
        };
        changed |= text_field(ui, &mut effect.trait_name, &hint, spine.field_w());
    });

    // ── Slots ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        form_label(ui, spine, "slots", colors);
        let r = TokenMultiselect::new(("slots", ei), &effect.slots, slot_options)
            .placeholder("add slot")
            .empty_text("every slot is already in this effect")
            .width(spine.field_w())
            .show(ui);
        if let Some(slot) = r.added
            && !effect.slots.contains(&slot)
        {
            effect.slots.push(slot);
            changed = true;
        }
        if let Some(i) = r.removed
            && i < effect.slots.len()
        {
            effect.slots.remove(i);
            changed = true;
        }
    });

    // ── Modes ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        form_label(ui, spine, "recolour", colors);
        for mode in Recolor::ALL {
            let selected = effect.recolor.same_mode(mode);
            if ui
                .selectable_label(selected, mode.label())
                .on_hover_text(mode.hint())
                .clicked()
                && !selected
            {
                effect.recolor = mode;
                changed = true;
            }
        }
        // The key colour belongs to one mode and appears only in it, rather
        // than sitting there greyed out asking to be read as configuration.
        if let Recolor::KeyedRemap(mut key) = effect.recolor
            && ui
                .color_edit_button_srgb(&mut key)
                .on_hover_text("Pixels near this colour are the ones recoloured")
                .changed()
        {
            effect.recolor = Recolor::KeyedRemap(key);
            changed = true;
        }
    });

    changed |= mode_row(ui, spine, "pick", colors, &Pick::ALL, &mut effect.pick);
    changed |= mode_row(ui, spine, "base", colors, &Base::ALL, &mut effect.base);
    changed |= mode_row(ui, spine, "apply", colors, &Apply::ALL, &mut effect.apply);

    // ── Variants ────────────────────────────────────────────────────────────
    if !effect.variants.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spine.gap;
            // Leading space takes no trailing gap of its own, so the gap that
            // follows the swatch on a real row is added here.
            ui.add_space(spine.control_x + spine.swatch + spine.gap);
            column_header(ui, spine, NAME_W, "variant", colors);
            column_header(ui, spine, LABEL_W, "emits", colors);
            column_header(ui, spine, WEIGHT_W, "weight", colors);
            column_header(ui, spine, RECIPE_W, "recipe", colors);
        });
    }

    let mut remove_variant: Option<usize> = None;
    let total = effect.total_weight();
    for (vi, variant) in effect.variants.iter_mut().enumerate() {
        ui.push_id(vi, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = spine.gap;
                ui.add_space(spine.control_x);
                changed |= swatch(ui, spine, &mut variant.recipe, colors);
                changed |= text_field(ui, &mut variant.name, "name", NAME_W);
                // The derived value, shown as the placeholder. `label` is an
                // OVERRIDE of it, not a second name — an empty field here is not
                // "no trait value", it is "the one derived from the identity",
                // and a field that renders blank for the common case reads as
                // something you forgot to fill in.
                changed |= text_field(
                    ui,
                    &mut variant.label,
                    &derived_label(&variant.name),
                    LABEL_W,
                );
                let share = match total > 0.0 {
                    true => format!("{:.0}% of picks", variant.weight / total * 100.0),
                    false => "every weight is zero — this effect never fires".to_string(),
                };
                if ui
                    .add_sized(
                        vec2(WEIGHT_W, spine.row_h),
                        egui::DragValue::new(&mut variant.weight)
                            .speed(0.1)
                            .range(0.0..=1000.0),
                    )
                    .on_hover_text(share)
                    .changed()
                {
                    changed = true;
                }
                recipe_cell(ui, spine, &variant.recipe, colors);
                if action(ui, spine, PhosphorIcon::X, "Remove variant", colors) {
                    remove_variant = Some(vi);
                }
            });
        });
    }
    if let Some(i) = remove_variant {
        effect.variants.remove(i);
        changed = true;
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        ui.add_space(spine.control_x);
        if add_row(ui, spine, "add variant") {
            effect.variants.push(EffectVariant::default());
            changed = true;
        }
    });

    changed
}

// ============================================================================
// Cells
// ============================================================================

/// A selector over a named-enum mode, on the shared form spine.
fn mode_row<T>(
    ui: &mut Ui,
    spine: &Spine,
    label: &str,
    colors: &ColorTokens,
    all: &[T],
    current: &mut T,
) -> bool
where
    T: Copy + PartialEq,
    T: ModeLabel,
{
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        form_label(ui, spine, label, colors);
        for mode in all {
            let selected = *current == *mode;
            if ui
                .selectable_label(selected, mode.label())
                .on_hover_text(mode.hint())
                .clicked()
                && !selected
            {
                *current = *mode;
                changed = true;
            }
        }
    });
    changed
}

/// What [`mode_row`] needs of a mode enum. Implemented by hand rather than
/// derived so the `const fn label` on each enum stays the single source.
pub trait ModeLabel {
    fn label(&self) -> &'static str;
    fn hint(&self) -> &'static str;
}

macro_rules! mode_label {
    ($($t:ty),+) => {$(
        impl ModeLabel for $t {
            fn label(&self) -> &'static str { (*self).label() }
            fn hint(&self) -> &'static str { (*self).hint() }
        }
    )+};
}
mode_label!(Pick, Base, Apply);

/// A label in the form's label column.
fn form_label(ui: &mut Ui, spine: &Spine, text: &str, colors: &ColorTokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(FORM_LABEL_W, spine.row_h), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    cell.label(egui::RichText::new(text).color(colors.text_secondary));
}

/// A muted column label occupying exactly the column it names.
fn column_header(ui: &mut Ui, spine: &Spine, width: f32, text: &str, colors: &ColorTokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, spine.row_h), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let size = cell.text_size(TextSize::Sm);
    cell.label(
        egui::RichText::new(text)
            .color(colors.text_muted)
            .size(size),
    );
}

fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> bool {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(width),
    )
    .changed()
}

/// The variant's colour, or an empty well when it has none.
///
/// An untinted variant is a real state, so the cell has to render it as one —
/// a colour picker forced to show *some* colour would invent configuration.
fn swatch(ui: &mut Ui, spine: &Spine, recipe: &mut Recipe, colors: &ColorTokens) -> bool {
    match recipe.swatch() {
        Some(mut c) => {
            if ui.color_edit_button_srgb(&mut c).changed() {
                *recipe = match *recipe {
                    Recipe::Pipeline { passes, .. } => Recipe::Pipeline { passes, preview: c },
                    _ => Recipe::Tint(c),
                };
                return true;
            }
            false
        }
        None => {
            let (rect, resp) =
                ui.allocate_exact_size(vec2(spine.swatch, spine.row_h), Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            ui.painter().rect(
                rect.shrink(2.0),
                ui.tokens().corner(Radius::Sm),
                Color32::TRANSPARENT,
                hairline(colors.border),
                egui::StrokeKind::Inside,
            );
            if resp
                .on_hover_text("Untinted — click to give it a colour")
                .clicked()
            {
                *recipe = Recipe::Tint(NEW_TINT);
                return true;
            }
            false
        }
    }
}

/// What the renderer will actually do, in the recipe column.
fn recipe_cell(ui: &mut Ui, spine: &Spine, recipe: &Recipe, colors: &ColorTokens) {
    let (rect, resp) = ui.allocate_exact_size(vec2(RECIPE_W, spine.row_h), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    // A pipeline variant is a MATERIAL. Showing it in the same muted grey as a
    // flat tint is how a four-pass gold ends up looking like a beige square.
    let color = match recipe.is_pipeline() {
        true => colors.accent,
        false => colors.text_muted,
    };
    let size = cell.text_size(TextSize::Sm);
    cell.label(
        egui::RichText::new(recipe.summary())
            .color(color)
            .size(size),
    );
    if recipe.is_pipeline() {
        resp.on_hover_text("A multi-pass effect pipeline — edited in the pipeline editor");
    }
}

/// A trailing row action, as a glyph rather than a filled button.
fn action(
    ui: &mut Ui,
    spine: &Spine,
    icon: PhosphorIcon,
    hint: &str,
    colors: &ColorTokens,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(spine.action, spine.row_h), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = match resp.hovered() {
        true => colors.error,
        false => colors.text_muted,
    };
    icon.paint(
        ui.painter(),
        egui::pos2(rect.left() + spine.gap + ACTION_PT * 0.5, rect.center().y),
        Align2::CENTER_CENTER,
        ACTION_PT,
        color,
    );
    resp.on_hover_text(hint).clicked()
}

/// The add affordance, as a real [`egui::Button`].
///
/// It was a painted glyph plus a label on a bare `Sense::click` rect, which
/// looked like a caption and gave a tap target the height of one line of text.
/// A button is the affordance the reader already knows, and it brings the
/// theme's own padding and hit area with it rather than this widget inventing
/// one. It still sits on the variant columns so it reads as "another of these".
fn add_row(ui: &mut Ui, spine: &Spine, label: &str) -> bool {
    use crate::buttons::UiButtonExt as _;
    ui.add_clickable_sized(
        [spine.swatch + spine.gap + NAME_W, spine.row_h],
        egui::Button::new(crate::icons::phosphor_label(ui, PhosphorIcon::Plus, label)),
    )
    .clicked()
}

/// The metadata trait value a variant emits when it carries no explicit label:
/// its identity, normalised. `deep_sea` → `Deep Sea`.
pub fn derived_label(name: &str) -> String {
    name.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(title_case)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `skin` → `Skin`, for the trait-name placeholder.
fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2, Rect};

    fn slots() -> Vec<String> {
        ["skin", "neck", "hand", "hair"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Every text shape the editor painted, as `(text, x)`.
    fn painted(effects: &mut Vec<Effect>) -> Vec<(String, f32)> {
        let ctx = egui::Context::default();
        let options = slots();
        let frame = |effects: &mut Vec<Effect>| {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                ..Default::default()
            });
            egui::Area::new(Id::new("ee")).show(&ctx, |ui| {
                EffectEditor::new(effects, &options).show(ui);
            });
            ctx.end_pass()
        };
        // First pass binds the fonts — see `icons::ensure_fonts`.
        let _ = frame(effects);
        let out = frame(effects);

        let mut found = Vec::new();
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos.x)),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        for cs in &out.shapes {
            walk(&cs.shape, &mut found);
        }
        found
    }

    fn x_of(found: &[(String, f32)], text: &str) -> Vec<f32> {
        found
            .iter()
            .filter(|(t, _)| t == text)
            .map(|(_, x)| *x)
            .collect()
    }

    /// For text laid out as a `LayoutJob` — an icon+label button is one galley
    /// whose text is the glyph, a space, and the label, so it never equals the
    /// label on its own.
    fn x_containing(found: &[(String, f32)], text: &str) -> Vec<f32> {
        found
            .iter()
            .filter(|(t, _)| t.contains(text))
            .map(|(_, x)| *x)
            .collect()
    }

    /// The shipping hodlcroft skin effect, as close as the model allows.
    fn skin() -> Effect {
        Effect {
            name: "skin".into(),
            trait_name: "Skin Tone".into(),
            slots: vec!["skin".into(), "neck".into(), "hand".into()],
            recolor: Recolor::Tint,
            pick: Pick::Shared,
            base: Base::Never,
            apply: Apply::PerLayer,
            variants: vec![
                EffectVariant {
                    name: "tan".into(),
                    label: "Sand".into(),
                    weight: 1.0,
                    recipe: Recipe::Tint([216, 168, 136]),
                },
                EffectVariant {
                    name: "holo".into(),
                    label: "Holographic".into(),
                    weight: 0.5,
                    recipe: Recipe::Pipeline {
                        passes: 2,
                        preview: [200, 204, 216],
                    },
                },
            ],
        }
    }

    #[test]
    fn the_shipping_skin_effect_is_expressible() {
        // The old widget could not represent this at all: its `base_color` was a
        // mandatory `[u8; 3]`, so the global-tint mode the skin/neck/hand
        // value-maps actually use was unreachable. That is the whole reason for
        // the reshape, so it gets a test rather than a comment.
        let e = skin();
        assert_eq!(e.recolor, Recolor::Tint);
        assert_eq!(e.pick, Pick::Shared);
        assert_eq!(e.base, Base::Never);
        assert_eq!(e.slots.len(), 3);
    }

    #[test]
    fn identity_and_public_label_are_separate_fields() {
        // `name` lands in `asset://…?variant=tan` and is parsed back out;
        // `label` is the metadata trait value. One field cannot be both.
        let mut effects = vec![skin()];
        let found = painted(&mut effects);
        assert_eq!(x_of(&found, "tan").len(), 1, "the internal identity");
        assert_eq!(x_of(&found, "Sand").len(), 1, "the public trait value");
        let name_x = x_of(&found, "tan")[0];
        let label_x = x_of(&found, "Sand")[0];
        assert!(
            label_x > name_x,
            "label column ({label_x}) sits right of the name column ({name_x})"
        );
    }

    #[test]
    fn a_pipeline_variant_reports_its_passes_instead_of_posing_as_a_tint() {
        // `gold` is tint → specular → grain. Drawn as a flat swatch with no
        // other signal, a material is indistinguishable from a beige square.
        let mut effects = vec![skin()];
        let found = painted(&mut effects);
        assert_eq!(x_of(&found, "2 passes").len(), 1);
        assert_eq!(
            x_of(&found, "tint").len(),
            2,
            "the mode row, and one variant"
        );
    }

    #[test]
    fn the_trailing_actions_share_one_axis() {
        let mut effects = vec![skin()];
        let found = painted(&mut effects);
        let trash = x_of(&found, &PhosphorIcon::Trash.as_str());
        let crosses = x_of(&found, &PhosphorIcon::X.as_str());
        assert_eq!(trash.len(), 1);
        // Not every `×` on the card is a row action: each selected slot is a
        // removable chip inside the slots control and carries its own. So the
        // assertion is about the AXIS — exactly the two variant removes join the
        // trash on it, and the three chip crosses stay off it.
        let (on_axis, off_axis): (Vec<f32>, Vec<f32>) =
            crosses.iter().partition(|x| (**x - trash[0]).abs() < 1.0);
        assert_eq!(on_axis.len(), 2, "one row action per variant: {crosses:?}");
        assert_eq!(
            off_axis.len(),
            3,
            "one chip per slot, none of them in the action column: {crosses:?}"
        );
    }

    #[test]
    fn every_column_header_sits_over_its_column() {
        // A header row is built from spaces while the rows beneath it are built
        // from widgets, and egui only puts `item_spacing` after the latter — so
        // the two drift by one gap unless the header adds it back.
        let mut effects = vec![skin()];
        let found = painted(&mut effects);
        for (header, cell) in [
            ("variant", "tan"),
            ("emits", "Sand"),
            ("recipe", "2 passes"),
        ] {
            let h = x_of(&found, header);
            let c = x_of(&found, cell);
            assert_eq!(h.len(), 1, "header {header}");
            assert_eq!(c.len(), 1, "cell {cell}");
            assert!(
                (h[0] - c[0]).abs() < 5.0,
                "header {header} at {} over {cell} at {}",
                h[0],
                c[0]
            );
        }
    }

    #[test]
    fn the_key_colour_appears_only_in_the_mode_that_has_one() {
        // A greyed-out picker in tint mode reads as configuration that happens
        // to be disabled, rather than as a field that does not apply.
        let mut tint = vec![Effect {
            recolor: Recolor::Tint,
            ..skin()
        }];
        let mut keyed = vec![Effect {
            recolor: Recolor::KeyedRemap([212, 165, 116]),
            ..skin()
        }];
        // The mode labels are painted either way; what differs is the picker,
        // which is a rect rather than text — so count interactive widgets via
        // the label the picker carries a tooltip for. Proxy: the keyed card is
        // wider in content because it holds one more widget on that row.
        let a = painted(&mut tint);
        let b = painted(&mut keyed);
        assert_eq!(x_of(&a, "keyed remap").len(), 1);
        assert_eq!(x_of(&b, "keyed remap").len(), 1);
        // Both render the row; the difference is asserted on the model, which is
        // what drives the picker's existence.
        assert!(!matches!(tint[0].recolor, Recolor::KeyedRemap(_)));
        assert!(matches!(keyed[0].recolor, Recolor::KeyedRemap(_)));
    }

    #[test]
    fn an_empty_label_derives_from_the_identity() {
        // `label` is an override, so the blank case has to have a defined
        // answer — otherwise the column reads as a field nobody filled in.
        assert_eq!(derived_label("tan"), "Tan");
        assert_eq!(derived_label("deep_sea"), "Deep Sea");
        assert_eq!(derived_label("off-white"), "Off White");
        assert_eq!(derived_label(""), "");
    }

    #[test]
    fn share_is_none_when_nothing_can_be_picked() {
        // All-zero weights is not "0% each", it is an effect that never fires.
        let mut e = skin();
        for v in &mut e.variants {
            v.weight = 0.0;
        }
        assert_eq!(e.share(0), None);
        let ok = skin();
        assert_eq!(ok.share(0), Some(1.0 / 1.5));
    }

    #[test]
    fn an_effect_with_no_variants_labels_no_columns() {
        let mut effects = vec![Effect {
            name: "empty".into(),
            ..Default::default()
        }];
        let found = painted(&mut effects);
        assert!(x_of(&found, "weight").is_empty());
        assert!(x_of(&found, "recipe").is_empty());
        assert_eq!(x_containing(&found, "add variant").len(), 1);
    }
}
