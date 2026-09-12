//! `EffectEditor` story — the compositor's `[[effect]]` blocks.
//!
//! The two seeded effects are chosen to be the two things the widget this
//! replaced could not do: a **global tint** over a grayscale value-map (its
//! `base_color` was a mandatory colour, so tint mode was unreachable), and a
//! variant that is a **material** rather than a colour.

use egui_widgets::effect_editor::{
    Apply, Base, Effect, EffectEditor, EffectVariant, Pick, Recipe, Recolor,
};

pub struct EffectEditorState {
    pub slots: Vec<String>,
    pub effects: Vec<Effect>,
}

impl Default for EffectEditorState {
    fn default() -> Self {
        Self {
            slots: ["skin", "neck", "hand", "hair", "clothes", "backgrounds"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            effects: vec![
                // The shipping hodlcroft skin effect. The art is one grayscale
                // value-map shared by three slots, so: tint (nothing to key
                // against), shared (one body tone per token), never (the raw
                // value-map is meaningless untinted).
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
                        // No label — the emitted trait value is derived from the
                        // identity. Most variants want this; the column is only
                        // filled when the public value should differ.
                        EffectVariant {
                            name: "brown".into(),
                            label: String::new(),
                            weight: 1.0,
                            recipe: Recipe::Tint([128, 88, 72]),
                        },
                        // Derived too, and showing the normalisation: the
                        // identity is a slug because it goes in a URL, the
                        // emitted value is not.
                        EffectVariant {
                            name: "deep_sea".into(),
                            label: String::new(),
                            weight: 0.3,
                            recipe: Recipe::Tint([128, 208, 248]),
                        },
                        // A MATERIAL, not a colour: tint → holo_shimmer. The
                        // recipe column is the only thing that says so.
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
                },
                // The other mode: art that is full-colour except for one keyed
                // region, so only pixels near the key get recoloured.
                Effect {
                    name: "trim".into(),
                    trait_name: String::new(),
                    slots: vec!["clothes".into()],
                    recolor: Recolor::KeyedRemap([212, 165, 116]),
                    pick: Pick::PerSlot,
                    base: Base::Offered,
                    apply: Apply::PerLayer,
                    variants: vec![
                        EffectVariant {
                            name: "gold".into(),
                            label: String::new(),
                            weight: 1.0,
                            recipe: Recipe::Tint([230, 184, 92]),
                        },
                        // No tint and no pipeline — a real state the swatch has
                        // to render as an empty well rather than inventing a
                        // colour for.
                        EffectVariant {
                            name: "none".into(),
                            label: "Plain".into(),
                            weight: 3.0,
                            recipe: Recipe::Untinted,
                        },
                    ],
                },
            ],
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut EffectEditorState) {
    crate::heading(ui, "Effect Editor");
    crate::caption(
        ui,
        "A named recolouring over a set of slots, and the weighted tones it \
         picks from — the compositor's [[effect]] block. Closer to a series \
         palette than to a paint bucket.",
    );
    crate::caption(
        ui,
        "`name` is identity — it lands in asset://…?variant=tan and is parsed \
         back out, so renaming changes asset identity. `emits` is what the \
         metadata trait actually says, and it is an OVERRIDE: leave it blank and \
         it derives from the identity, which is what `brown` and `deep_sea` do \
         here. Fill it in only where the public value should differ, as `tan` → \
         Sand does.",
    );
    crate::caption(
        ui,
        "The modes are enums, not an Option and not bools. `recolour` decides \
         whether the whole layer is multiplied or only pixels near a key colour \
         are remapped — and the key colour appears only in the mode that has \
         one. Watch the recipe column: `holo` is a two-pass material, not a \
         swatch.",
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("Reset").clicked() {
            *state = EffectEditorState::default();
        }
        if ui.button("Clear all").clicked() {
            state.effects.clear();
        }
    });
    ui.add_space(8.0);

    EffectEditor::new(&mut state.effects, &state.slots).show(ui);

    ui.add_space(8.0);
    let variants: usize = state.effects.iter().map(|e| e.variants.len()).sum();
    ui.label(
        egui::RichText::new(format!(
            "{} effect(s), {variants} variant(s)",
            state.effects.len()
        ))
        .color(crate::secondary(ui))
        .small(),
    );
}
