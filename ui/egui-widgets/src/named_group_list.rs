//! named_group_list — a list of named groups, each a `name` + a member multiselect
//! (+ an optional boolean flag like "allow none"). Backs exclusive groups,
//! bundled sets, and linked traits in the config editor. Composes
//! [`TokenMultiselect`](crate::token_multiselect::TokenMultiselect) for membership.
//!
//! Mutates the group list in place (it's a composite editor with nested widgets,
//! so in-place editing is simpler than threading granular events); returns
//! `true` when anything changed so the host can re-validate.

use egui::Ui;

use crate::token_multiselect::TokenMultiselect;

#[derive(Default, Debug, Clone)]
pub struct NamedGroup {
    pub name: String,
    pub members: Vec<String>,
    /// Optional flag (e.g. exclusive group `allow_none`); shown only if the
    /// editor is configured with a flag label.
    pub flag: bool,
}

pub struct NamedGroupList<'a> {
    groups: &'a mut Vec<NamedGroup>,
    options: &'a [String],
    flag_label: Option<&'a str>,
    member_label: &'a str,
    add_label: &'a str,
}

impl<'a> NamedGroupList<'a> {
    pub fn new(groups: &'a mut Vec<NamedGroup>, options: &'a [String]) -> Self {
        Self {
            groups,
            options,
            flag_label: None,
            member_label: "+ member",
            add_label: "Add group",
        }
    }

    /// Show a boolean flag per group with this label (e.g. "allow none").
    pub fn flag(mut self, label: &'a str) -> Self {
        self.flag_label = Some(label);
        self
    }

    pub fn member_label(mut self, label: &'a str) -> Self {
        self.member_label = label;
        self
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    /// Returns true if any group/member/flag changed this frame.
    pub fn show(self, ui: &mut Ui) -> bool {
        let Self {
            groups,
            options,
            flag_label,
            member_label,
            add_label,
        } = self;

        let mut changed = false;
        let mut remove_group: Option<usize> = None;

        for (gi, group) in groups.iter_mut().enumerate() {
            ui.push_id(gi, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut group.name)
                                    .hint_text("group name")
                                    .desired_width(150.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        if let Some(label) = flag_label {
                            if ui.checkbox(&mut group.flag, label).changed() {
                                changed = true;
                            }
                        }
                        if ui.button("Remove group").clicked() {
                            remove_group = Some(gi);
                        }
                    });

                    let r = TokenMultiselect::new(&group.members, options)
                        .add_label(member_label)
                        .show(ui);
                    if let Some(opt) = r.added {
                        if !group.members.contains(&opt) {
                            group.members.push(opt);
                            changed = true;
                        }
                    }
                    if let Some(i) = r.removed {
                        if i < group.members.len() {
                            group.members.remove(i);
                            changed = true;
                        }
                    }
                    if r.cleared {
                        group.members.clear();
                        changed = true;
                    }
                });
            });
        }

        if let Some(i) = remove_group {
            groups.remove(i);
            changed = true;
        }
        if ui.button(add_label).clicked() {
            groups.push(NamedGroup::default());
            changed = true;
        }

        changed
    }
}
