//! `PartyAnnotator` — decide what a wallet IS to the project, on the record.
//!
//! Chain analysis produces keys; investigation produces meaning. The meaning
//! that matters here is one call: **is this address core team, an associate,
//! or a customer** — because the whole point of the surrounding tool is to
//! find the addresses sitting out in the unexamined crowd that actually belong
//! to the people running the project.
//!
//! That classification is the first control on the form and the one that moves
//! a wallet between rings. Entity, tags and source are detail hung off it.
//!
//! **Unclassified is not "customer".** A wallet nobody has looked at and a
//! wallet judged to be a customer are different claims; the form keeps them
//! apart, and clicking the current class clears it back to unexamined, because
//! concluding you were wrong is a legitimate move.
//!
//! ## The source rule, made visible rather than enforced
//!
//! An [`PartyBasis::Asserted`] note without a source is a guess. The widget
//! does not block it: blocking would push people to write the guess into the
//! label field instead, where nothing can flag it. Instead the source box is
//! promoted to the front when the basis is `Asserted`, an empty one is marked
//! **UNSOURCED** in place, and the response reports it so the host can count
//! them the way an export does.
//!
//! ## Entity vs label
//!
//! - **entity** — the person or organisation *behind* the wallet. Several
//!   wallets share one, which is what lets a view roll them into a single node.
//! - **label** — what to call this one wallet. A treasury's operational wallet
//!   may have a label and no entity, or both.
//!
//! Keeping them apart is what makes roll-up possible at all; a single "name"
//! field would conflate "this wallet" with "whoever owns it".

use egui::{Color32, Ui};

use crate::party_badge::PartyBasis;
use crate::{Chip, ChipVariant};

/// Where a wallet sits relative to the project. Mirrors the app's stored
/// `Class`; kept here so the widget has no dependency on a storage crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyClass {
    Core,
    Associate,
    Customer,
}

impl PartyClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "core team",
            Self::Associate => "associate",
            Self::Customer => "customer",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Core => "the project itself — treasury, mint wallets, the people running it",
            Self::Associate => "paid BY the project to do something — artist, dev, marketing",
            Self::Customer => "bought from the project",
        }
    }

    pub const ALL: [Self; 3] = [Self::Core, Self::Associate, Self::Customer];
}

/// The editable form. The host owns it, loads it from its own store, and
/// writes it back when [`PartyAnnotatorResponse::save`] comes back true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationDraft {
    /// `None` = not yet examined. NOT the same as `Customer`, and the form
    /// must never quietly turn one into the other.
    pub class: Option<PartyClass>,
    pub entity: String,
    pub label: String,
    pub tags: Vec<String>,
    pub basis: PartyBasis,
    pub source: String,
    /// Free-text tag being typed.
    pub tag_input: String,
}

impl Default for AnnotationDraft {
    fn default() -> Self {
        Self {
            class: None,
            entity: String::new(),
            label: String::new(),
            tags: Vec::new(),
            // A human filling this form IS asserting, so that is the honest
            // default. Defaulting to `Observed` would quietly launder every
            // note into a chain fact.
            basis: PartyBasis::Asserted,
            source: String::new(),
            tag_input: String::new(),
        }
    }
}

impl AnnotationDraft {
    /// Something was actually written down.
    pub fn has_content(&self) -> bool {
        self.class.is_some()
            || !self.entity.trim().is_empty()
            || !self.label.trim().is_empty()
            || !self.tags.is_empty()
    }

    /// A claim with nobody standing behind it.
    pub fn is_unsourced_assertion(&self) -> bool {
        self.basis == PartyBasis::Asserted && self.source.trim().is_empty() && self.has_content()
    }

    /// Add a tag, normalised and deduped. Returns whether it was new.
    pub fn add_tag(&mut self, tag: &str) -> bool {
        let t = tag.trim().to_lowercase();
        if t.is_empty() || self.tags.iter().any(|x| x == &t) {
            return false;
        }
        self.tags.push(t);
        self.tags.sort();
        true
    }

    pub fn toggle_tag(&mut self, tag: &str) {
        let t = tag.trim().to_lowercase();
        match self.tags.iter().position(|x| x == &t) {
            Some(i) => {
                self.tags.remove(i);
            }
            None => {
                self.add_tag(&t);
            }
        }
    }
}

pub struct PartyAnnotatorResponse {
    /// The reader asked to persist the draft.
    pub save: bool,
    /// The reader asked to discard their edits.
    pub revert: bool,
    /// The draft changed this frame.
    pub changed: bool,
    /// Would be saved as a claim with no source.
    pub unsourced: bool,
}

pub struct PartyAnnotator<'a> {
    id_salt: &'a str,
    /// The party being annotated, already display-formatted by the host.
    subject: &'a str,
    draft: &'a mut AnnotationDraft,
    /// Tags already in use, most-used first — the palette.
    palette: &'a [(String, usize)],
    /// Entities already known, for the datalist-style hints.
    entities: &'a [String],
    dirty: bool,
}

impl<'a> PartyAnnotator<'a> {
    pub fn new(id_salt: &'a str, subject: &'a str, draft: &'a mut AnnotationDraft) -> Self {
        Self {
            id_salt,
            subject,
            draft,
            palette: &[],
            entities: &[],
            dirty: false,
        }
    }

    pub fn palette(mut self, tags: &'a [(String, usize)]) -> Self {
        self.palette = tags;
        self
    }

    pub fn entities(mut self, e: &'a [String]) -> Self {
        self.entities = e;
        self
    }

    /// Whether the draft differs from what is stored — drives the save button.
    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    pub fn show(self, ui: &mut Ui) -> PartyAnnotatorResponse {
        let Self {
            id_salt,
            subject,
            draft,
            palette,
            entities,
            dirty,
        } = self;
        let mut changed = false;
        let mut save = false;
        let mut revert = false;
        let muted = ui.visuals().weak_text_color();

        // THREE LINES: who, what kind, how do you know. Everything else is
        // rarer than that and lives behind the disclosure — a form that asks
        // nine questions to record one fact does not get filled in.
        ui.push_id(id_salt, |ui| {
            // Subject + basis on one line. Basis is a menu, not three buttons:
            // it is `asserted` for almost every note ever written here.
            // WRAPPED, not right-aligned: in a narrow column a right-to-left
            // layout squeezes the subject down to "stake1uy" and the one thing
            // the form must never be ambiguous about is WHICH wallet you are
            // annotating. Let the basis fall to the next line instead.
            ui.horizontal_wrapped(|ui| {
                // The subject is only repeated when the host has not already
                // named it above — in the shell it has, and the name appearing
                // twice in one panel is pure noise.
                if !subject.is_empty() {
                    ui.label(egui::RichText::new(subject).strong());
                }
                {
                    egui::ComboBox::from_id_salt("basis")
                        .selected_text(
                            egui::RichText::new(draft.basis.word())
                                .small()
                                .color(basis_color(draft.basis)),
                        )
                        .width(92.0)
                        .show_ui(ui, |ui| {
                            for b in [
                                PartyBasis::Asserted,
                                PartyBasis::Derived,
                                PartyBasis::Observed,
                            ] {
                                if ui.selectable_label(draft.basis == b, b.word()).clicked() {
                                    draft.basis = b;
                                    changed = true;
                                }
                            }
                        });
                }
            });

            // 1 · WHAT IS THIS WALLET TO THE PROJECT.
            //
            // First, because it is the decision the tool exists to support:
            // spotting an address out in the unexamined ring that actually
            // belongs to the team, and pulling it in. Everything else on this
            // form is detail hung off that call.
            ui.horizontal_wrapped(|ui| {
                for c in PartyClass::ALL {
                    let on = draft.class == Some(c);
                    if ui
                        .selectable_label(on, c.label())
                        .on_hover_text(c.hint())
                        .clicked()
                    {
                        // Clicking the current class clears it — back to
                        // unexamined, which is a legitimate thing to conclude
                        // you were wrong about.
                        draft.class = if on { None } else { Some(c) };
                        changed = true;
                    }
                }
                if draft.class.is_none() {
                    ui.label(egui::RichText::new("not yet examined").small().color(muted));
                }
            });

            // 2 · who is behind it.
            let name = ui.add(
                egui::TextEdit::singleline(&mut draft.entity)
                    .hint_text("who? — Dwess, MEKKALABS…")
                    .desired_width(f32::INFINITY),
            );
            changed |= name.changed();
            // Known entities appear only WHILE TYPING, filtered — a standing
            // row of every entity ever named is noise nine frames in ten.
            if name.has_focus() && !draft.entity.trim().is_empty() {
                let q = draft.entity.to_lowercase();
                let hits: Vec<&String> = entities
                    .iter()
                    .filter(|e| e.to_lowercase().contains(&q) && **e != draft.entity)
                    .take(4)
                    .collect();
                if !hits.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for e in hits {
                            if ui.small_button(e).clicked() {
                                draft.entity = e.clone();
                                changed = true;
                            }
                        }
                    });
                }
            }
            // 3 · what kind of thing it is. Chips on the same line as the
            // input, and the palette appears only as FILTERED suggestions
            // while typing — a standing wall of every tag ever used is the
            // single biggest source of clutter in a form like this.
            ui.horizontal_wrapped(|ui| {
                let mut remove: Option<usize> = None;
                for (i, t) in draft.tags.iter().enumerate() {
                    if ui
                        .small_button(format!("{t} x"))
                        .on_hover_text("remove")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove {
                    draft.tags.remove(i);
                    changed = true;
                }
                let te = ui.add(
                    egui::TextEdit::singleline(&mut draft.tag_input)
                        .hint_text(if draft.tags.is_empty() { "tags" } else { "+" })
                        .desired_width(if draft.tags.is_empty() { 200.0 } else { 90.0 }),
                );
                // A singleline TextEdit surrenders focus on Enter, so
                // `lost_focus` is the reliable commit signal.
                if te.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && draft.add_tag(&draft.tag_input.clone())
                {
                    draft.tag_input.clear();
                    changed = true;
                }
            });
            if !draft.tag_input.trim().is_empty() {
                let q = draft.tag_input.trim().to_lowercase();
                let hits: Vec<&(String, usize)> = palette
                    .iter()
                    .filter(|(t, _)| t.contains(&q) && !draft.tags.iter().any(|x| x == t))
                    .take(5)
                    .collect();
                if !hits.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (t, n) in hits {
                            if ui.small_button(format!("{t} ({n})")).clicked() {
                                draft.add_tag(t);
                                draft.tag_input.clear();
                                changed = true;
                            }
                        }
                    });
                }
            }

            // 4 · how do you know. This IS the basis question for almost every
            // note, so it is asked in plain words rather than as a taxonomy.
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut draft.source)
                        .hint_text(if draft.basis == PartyBasis::Asserted {
                            "how do you know?"
                        } else {
                            "source"
                        })
                        .desired_width(f32::INFINITY),
                )
                .changed();
            if draft.is_unsourced_assertion() {
                // Marked in place, at the moment of writing — not in an export
                // nobody reads until later.
                Chip::new("unsourced")
                    .variant(ChipVariant::Warning)
                    .on_hover_text("this will save as a claim nobody stands behind")
                    .show(ui);
            }

            // Everything rarer than the three above.
            ui.horizontal(|ui| {
                egui::CollapsingHeader::new(egui::RichText::new("more").small().color(muted))
                    .id_salt("more")
                    .show_unindented(ui, |ui| {
                        // Only the label lives here now. Free-text reasoning
                        // moved to the COMMENT thread the host renders below
                        // this form — timestamped and attributable, which a
                        // single note field could never be.
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut draft.label)
                                    .hint_text("label just this wallet")
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                    });
                let can_save = dirty || changed;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(can_save, egui::Button::new("Save"))
                        .clicked()
                    {
                        save = true;
                    }
                    if can_save && ui.small_button("revert").clicked() {
                        revert = true;
                    }
                });
            });
        });

        PartyAnnotatorResponse {
            save,
            revert,
            changed,
            unsourced: draft.is_unsourced_assertion(),
        }
    }
}

/// Colour for a basis, so a claim never renders like a chain fact.
pub fn basis_color(b: PartyBasis) -> Color32 {
    match b {
        PartyBasis::Observed => Color32::from_rgb(0x4c, 0xaf, 0x50),
        PartyBasis::Derived => Color32::from_rgb(0x39, 0x87, 0xe5),
        PartyBasis::Asserted => Color32::from_rgb(0xe0, 0x8a, 0x2e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_normalised_and_deduped() {
        let mut d = AnnotationDraft::default();
        assert!(d.add_tag("Artist"));
        assert!(!d.add_tag("artist"), "same tag, different case");
        assert!(!d.add_tag("   "), "blank is not a tag");
        assert!(d.add_tag("team"));
        assert_eq!(d.tags, vec!["artist".to_string(), "team".to_string()]);

        d.toggle_tag("ARTIST");
        assert_eq!(d.tags, vec!["team".to_string()], "toggle removes");
        d.toggle_tag("exchange");
        assert_eq!(d.tags, vec!["exchange".to_string(), "team".to_string()]);
    }

    /// The default must be the cautious one: a person filling in a form is
    /// asserting, and defaulting to `Observed` would launder every note into a
    /// chain fact.
    #[test]
    fn the_default_basis_is_asserted() {
        assert_eq!(AnnotationDraft::default().basis, PartyBasis::Asserted);
    }

    /// Classification is the primary act, and clearing it is a legitimate
    /// conclusion — not a way of saying "customer".
    #[test]
    fn class_toggles_back_to_unexamined_and_counts_as_content() {
        let mut d = AnnotationDraft::default();
        assert_eq!(d.class, None, "a fresh wallet is unexamined");
        assert!(!d.has_content());

        d.class = Some(PartyClass::Core);
        assert!(d.has_content(), "classifying IS recording something");
        assert!(
            d.is_unsourced_assertion(),
            "calling something core team with no source is an unsourced claim"
        );

        d.source = "named in the launch thread".into();
        assert!(!d.is_unsourced_assertion());

        // Unexamined is reachable again, and is distinct from Customer.
        d.class = None;
        assert_ne!(d.class, Some(PartyClass::Customer));
    }

    #[test]
    fn an_unsourced_assertion_is_detected_only_when_something_was_claimed() {
        let mut d = AnnotationDraft::default();
        assert!(!d.is_unsourced_assertion(), "an empty form claims nothing");

        d.entity = "Dwess".into();
        assert!(d.is_unsourced_assertion(), "a claim with no source");

        d.source = "   ".into();
        assert!(d.is_unsourced_assertion(), "whitespace is not a source");

        d.source = "named in the launch thread".into();
        assert!(!d.is_unsourced_assertion());

        // Observed and derived carry no such obligation.
        let mut o = AnnotationDraft {
            basis: PartyBasis::Observed,
            ..Default::default()
        };
        o.label = "royalty".into();
        assert!(!o.is_unsourced_assertion());
    }

    #[test]
    fn content_detection_ignores_whitespace() {
        let mut d = AnnotationDraft {
            label: "   ".into(),
            ..Default::default()
        };
        assert!(!d.has_content());
        d.tags.push("artist".into());
        assert!(d.has_content());
    }
}
