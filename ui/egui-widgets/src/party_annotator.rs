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

use crate::id_pill::IdPill;
use crate::party_badge::PartyBasis;
use crate::select::{MultiSelect, Select, SelectOption};
use crate::{Chip, ChipVariant};

/// Where a wallet sits relative to the project. Mirrors the app's stored
/// `Class`; kept here so the widget has no dependency on a storage crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyClass {
    /// A founder's PERSONAL wallet — distinct from `Core` because the
    /// distinction is the investigation: value moving core→core is the
    /// project operating, value moving core→founder is the project's money
    /// becoming a person's. A tool that files the founder under "core team"
    /// cannot ask whether the project paid its founder, and one that watches
    /// the founder receive payroll-shaped transfers will forever propose
    /// "associate" — both wallets are insiders, but one of them is the
    /// question.
    Founder,
    Core,
    Associate,
    Customer,
    /// Examined and judged irrelevant to this investigation. A VERDICT, not a
    /// place: unlike the other classes it takes no ring seat and hides from
    /// the lists — but it is a dismissal, not an erasure. The evidence keeps
    /// running against it, and a dismissed wallet that later scores strongly
    /// comes back as a disagreement. Distinct from unexamined (`None`):
    /// "nobody has looked" and "somebody looked and waved it off" must never
    /// be confusable, because only one of them is finished work.
    Ignored,
}

impl PartyClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Founder => "founder",
            Self::Core => "core team",
            Self::Associate => "associate",
            Self::Customer => "customer",
            Self::Ignored => "ignore",
        }
    }

    /// A few words, for a menu row under the label. [`hint`](Self::hint) is
    /// the full argument and is too long to sit in a list.
    pub fn blurb(self) -> &'static str {
        match self {
            Self::Founder => "a founder's personal wallet",
            Self::Core => "the project itself — treasury, mint, the people running it",
            Self::Associate => "paid BY the project — artist, dev, marketing",
            Self::Customer => "bought from the project",
            Self::Ignored => "examined, judged irrelevant — not erased",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Founder => {
                "a founder's PERSONAL wallet — value arriving here is the project's money \
                 becoming a person's, which is the question this tool exists to ask"
            }
            Self::Core => "the project itself — treasury, mint wallets, the people running it",
            Self::Associate => "paid BY the project to do something — artist, dev, marketing",
            Self::Customer => "bought from the project",
            Self::Ignored => {
                "examined, judged irrelevant — hidden from the list and ring. Not erased: \
                 strong evidence against a dismissal comes back as a disagreement"
            }
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Founder,
        Self::Core,
        Self::Associate,
        Self::Customer,
        Self::Ignored,
    ];
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
    // (`tag_input` lived here — the half-typed tag. The tag control is now a
    // creatable `MultiSelect`, which keeps its filter in egui temp memory like
    // every other select, so the draft no longer carries UI scratch state.)
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
    /// The party being annotated — the **whole** identifier, not a shortened
    /// one: it renders as an [`IdPill`], which middle-elides for display and
    /// copies the full value. Empty when the host already names the subject
    /// above the form, which is the usual case.
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
                // `Align::Center`: the address is text and the basis select is
                // a 30pt framed box, so a baseline-aligned row sat the address
                // against the top of the control.
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // The subject is only repeated when the host has not
                    // already named it above — in the shell it has, and
                    // the name appearing twice in one panel is pure noise.
                    if !subject.is_empty() {
                        // `IdPill` owns the truncation, which is why the
                        // caller passes the WHOLE address: it middle-elides
                        // for display, shows the full value on hover, and
                        // copies the real thing. A pre-truncated string
                        // would put an unusable address on the clipboard.
                        IdPill::new("wallet", subject).show(ui);
                    }
                    {
                        // The basis is never empty — a claim always rests on
                        // something — so this select is deliberately NOT
                        // clearable, and its swatch carries the same colour
                        // coding the rest of the form uses for basis.
                        let bases = [
                            PartyBasis::Asserted,
                            PartyBasis::Derived,
                            PartyBasis::Observed,
                        ];
                        let options: Vec<SelectOption> = bases
                            .iter()
                            .map(|b| {
                                SelectOption::new(b.word(), b.word()).swatch(Some(basis_color(*b)))
                            })
                            .collect();
                        // Salted with the widget's OWN id — `Select` builds its id
                        // from the salt alone (deliberately, so `Grid` siblings
                        // cannot collide), which means the enclosing `push_id`
                        // does not reach it. A constant here put three annotators
                        // on one id.
                        let resp = Select::new((id_salt, "basis"), &options)
                            .value_from_id(draft.basis.word(), "unknown basis")
                            .clearable(false)
                            .width(140.0)
                            .show(ui);
                        if let Some(word) = resp.chosen
                            && let Some(b) = bases.iter().find(|b| b.word() == word)
                            && draft.basis != *b
                        {
                            draft.basis = *b;
                            changed = true;
                        }
                    }
                });
            });

            // 1 · WHAT IS THIS WALLET TO THE PROJECT.
            //
            // First, because it is the decision the tool exists to support:
            // spotting an address out in the unexamined ring that actually
            // belongs to the team, and pulling it in. Everything else on this
            // form is detail hung off that call.
            ui.add_space(10.0);
            field_label(ui, "What is this wallet to the project?", muted);
            {
                // A select, not a row of toggles. Five mutually-exclusive
                // options wrapping across two lines read as loose words; and
                // **clearing is now a visible affordance** rather than the
                // undiscoverable "click the one that is already on".
                //
                // Empty is not a sixth class: `None` is "not yet examined",
                // which the placeholder says outright.
                let options: Vec<SelectOption> = PartyClass::ALL
                    .iter()
                    .map(|c| SelectOption::new(c.label(), c.label()).subtitle(c.blurb()))
                    .collect();
                let resp = Select::new((id_salt, "class"), &options)
                    .value_from_id(
                        draft.class.map(|c| c.label()).unwrap_or_default(),
                        "unknown class",
                    )
                    .placeholder("not yet examined")
                    .width(240.0)
                    .show(ui);
                if let Some(word) = resp.chosen {
                    draft.class = PartyClass::ALL.iter().copied().find(|c| c.label() == word);
                    changed = true;
                }
                if resp.cleared {
                    draft.class = None;
                    changed = true;
                }
            }

            // 2 · who is behind it.
            ui.add_space(10.0);
            field_label(ui, "Entity — who is behind it", muted);
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
            // 3 · what kind of thing it is.
            //
            // A CREATABLE multiselect: the palette is a record of tags already
            // in use, not a closed vocabulary, so typing a new one offers
            // "Create …". That replaced a hand-rolled arrangement of chips, a
            // text field, and a second row of filtered suggestion buttons —
            // three controls doing what one does, each with its own idea of
            // what a tag looks like.
            ui.add_space(10.0);
            field_label(ui, "Tags — what kind of thing it is", muted);
            {
                // Most-used first (the palette's own order), with the count as
                // the subtitle — which tag is the established one is exactly
                // the thing a tagger wants to know before inventing a synonym.
                let options: Vec<SelectOption> = palette
                    .iter()
                    .map(|(t, n)| {
                        SelectOption::new(t.clone(), t.clone()).subtitle(format!("used {n}×"))
                    })
                    .collect();
                let resp = MultiSelect::new((id_salt, "tags"), &draft.tags, &options)
                    .placeholder("tags")
                    .empty_text("type to add a new tag")
                    .creatable(true)
                    .clearable(false)
                    .width(320.0)
                    .show(ui);
                if let Some(tag) = resp.added
                    && draft.add_tag(&tag)
                {
                    changed = true;
                }
                if let Some(i) = resp.removed
                    && i < draft.tags.len()
                {
                    draft.tags.remove(i);
                    changed = true;
                }
            }

            // 4 · how do you know. This IS the basis question for almost every
            // note, so it is asked in plain words rather than as a taxonomy.
            ui.add_space(10.0);
            field_label(ui, "Source — how do you know", muted);
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

            // Everything rarer than the three above. On its OWN row: sharing
            // a line with the save controls made the disclosure's height
            // decide where the buttons sat, so they landed differently on
            // every card depending on whether it was expanded.
            ui.add_space(10.0);
            egui::CollapsingHeader::new(egui::RichText::new("more").small().color(muted))
                .id_salt("more")
                .show_unindented(ui, |ui| {
                    // Only the label lives here now. Free-text reasoning moved
                    // to the COMMENT thread the host renders below this form —
                    // timestamped and attributable, which a single note field
                    // could never be.
                    field_label(ui, "Label — this wallet alone", muted);
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut draft.label)
                                .hint_text("label just this wallet")
                                .desired_width(f32::INFINITY),
                        )
                        .changed();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            let can_save = dirty || changed;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_save, egui::Button::new("Save"))
                    .clicked()
                {
                    save = true;
                }
                if can_save && ui.button("Revert").clicked() {
                    revert = true;
                }
                // The unsourced warning belongs BESIDE the save button, not
                // orphaned up by the source field: it describes what saving
                // will produce, and it is read at the moment of deciding.
                if draft.is_unsourced_assertion() {
                    Chip::new("unsourced")
                        .variant(ChipVariant::Warning)
                        .on_hover_text("this will save as a claim nobody stands behind")
                        .show(ui);
                }
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

/// A small caption above a field.
///
/// Hint text disappears the moment a field has content, so a filled-in form
/// was four unlabelled boxes — you could read what someone wrote and not what
/// they were answering. These stay.
fn field_label(ui: &mut Ui, text: &str, muted: Color32) {
    ui.label(egui::RichText::new(text).small().color(muted));
    ui.add_space(2.0);
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
