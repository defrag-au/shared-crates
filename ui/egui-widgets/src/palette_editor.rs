//! palette_editor — edit colorization palettes: each palette has a name, a base
//! color, and a list of variants (name + color + weight). Backs the
//! `[processing.techniques.colorization]` palettes in the config editor.
//!
//! Mutates the palette list in place (composite editor with nested rows); returns
//! `true` when anything changed so the host can re-serialise / re-validate.

use egui::Ui;

#[derive(Debug, Clone)]
pub struct PaletteVariant {
    pub name: String,
    pub color: [u8; 3],
    pub weight: f32,
}

impl Default for PaletteVariant {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: [200, 160, 120],
            weight: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Palette {
    pub name: String,
    pub base_color: [u8; 3],
    pub variants: Vec<PaletteVariant>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [212, 165, 116],
            variants: Vec::new(),
        }
    }
}

pub struct PaletteEditor<'a> {
    palettes: &'a mut Vec<Palette>,
    add_label: &'a str,
}

impl<'a> PaletteEditor<'a> {
    pub fn new(palettes: &'a mut Vec<Palette>) -> Self {
        Self {
            palettes,
            add_label: "Add palette",
        }
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let Self {
            palettes,
            add_label,
        } = self;

        let mut changed = false;
        let mut remove_palette: Option<usize> = None;

        for (pi, palette) in palettes.iter_mut().enumerate() {
            ui.push_id(pi, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Base");
                        if ui.color_edit_button_srgb(&mut palette.base_color).changed() {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut palette.name)
                                    .hint_text("palette name")
                                    .desired_width(140.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        if ui.button("Remove palette").clicked() {
                            remove_palette = Some(pi);
                        }
                    });

                    let mut remove_variant: Option<usize> = None;
                    for (vi, variant) in palette.variants.iter_mut().enumerate() {
                        ui.push_id(vi, |ui| {
                            ui.horizontal(|ui| {
                                if ui.color_edit_button_srgb(&mut variant.color).changed() {
                                    changed = true;
                                }
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut variant.name)
                                            .hint_text("variant")
                                            .desired_width(100.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.label("w");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut variant.weight)
                                            .speed(0.1)
                                            .range(0.0..=1000.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                if ui.button("×").on_hover_text("Remove variant").clicked() {
                                    remove_variant = Some(vi);
                                }
                            });
                        });
                    }
                    if let Some(i) = remove_variant {
                        palette.variants.remove(i);
                        changed = true;
                    }
                    if ui.button("+ variant").clicked() {
                        palette.variants.push(PaletteVariant::default());
                        changed = true;
                    }
                });
            });
        }

        if let Some(i) = remove_palette {
            palettes.remove(i);
            changed = true;
        }
        if ui.button(add_label).clicked() {
            palettes.push(Palette::default());
            changed = true;
        }

        changed
    }
}
