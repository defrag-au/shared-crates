//! `agent_config` — who answers, on whose key, for whom, and how much
//! (feature `gateway`).
//!
//! The server-owner's view of the @mention agent, in three parts that are
//! deliberately separate widgets because they have different consequences:
//!
//! 1. [`provider_picker`] — **which model, at whose endpoint.** Presets for
//!    the common providers, and a Custom option, because the platform accepts
//!    any OpenAI-compatible endpoint and an allow-list would be a list to
//!    maintain against a market that adds a provider a month.
//! 2. [`credential_field`] — **the key.** Write-only. See below.
//! 3. [`budget_editor`] — **which roles may ask, and for how much a day.**
//!
//! ## The credential is write-only, and that shapes the widget
//!
//! A key goes in through an action and never comes back — not to the admin
//! who typed it, not in a state dump, not in a delta. So this widget can
//! never render a stored key, only [`ProviderStatus`] metadata: a masked
//! tail, when it was set, when it last answered. Everything about the layout
//! follows from that — there is no "edit" affordance, only *replace*, because
//! there is nothing to edit into.
//!
//! Masking happens at the server boundary ([`gateway_wiring::mask_key`]),
//! never here. A widget that received a whole key in order to truncate it for
//! display would already have lost.
//!
//! ## Why budgets sit beside the key
//!
//! Under bring-your-own-credential the daily allowance stops being our cost
//! control and becomes the owner's. Putting the budget next to the key it
//! spends is the honest arrangement: one screen answers "whose money, and how
//! much of it per person per day".

use egui::Ui;
// No `mask_key` here on purpose — masking happens at the server boundary and
// this widget only ever renders the already-masked `ProviderStatus`.
use gateway_wiring::{
    AgentEntitlement, AgentMode, AgentTier, GuildRole, ProviderStatus, preset_for_base_url,
    provider_presets, provider_problems,
};

use crate::icons::{install_phosphor_font, phosphor_label};
use crate::relative_time::relative_label;
use crate::utils::{format_number, section_heading};
use crate::select::{Select, SelectOption};
use crate::{PhosphorIcon, theme};

/// A token-count spinner that reads as a number rather than a digit run.
///
/// `500000` and `100000` are near-indistinguishable at a glance, which is
/// exactly the comparison this control exists for — the whole point of tiers
/// is that one is bigger than another.
fn token_drag(value: &mut u32, min: u32) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .speed(1_000)
        .range(min..=10_000_000)
        .custom_formatter(|n, _| format_number(n as i64))
        // Accept what we print: a pasted "500,000" must not parse as 500.
        .custom_parser(|s| s.replace([',', ' ', '_'], "").parse::<f64>().ok())
}

/// A labelled row inside one of the config grids, so labels share a column
/// edge instead of each field starting wherever its label happened to end.
fn field_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> bool) -> bool {
    ui.label(egui::RichText::new(label).color(theme::TEXT_SECONDARY));
    let changed = add(ui);
    ui.end_row();
    changed
}

/// The editable half of a provider configuration. The key is NOT here — it
/// lives in [`CredentialDraft`], which is cleared the moment it is submitted.
#[derive(Default, Clone, PartialEq)]
pub struct ProviderDraft {
    pub base_url: String,
    pub model: String,
}

impl ProviderDraft {
    /// Seed from what the server reports, so opening the editor shows the
    /// live configuration rather than an empty form.
    pub fn from_status(status: Option<&ProviderStatus>) -> Self {
        match status {
            Some(s) => Self {
                base_url: s.base_url.clone(),
                model: s.model.clone(),
            },
            None => {
                let first = &provider_presets()[0];
                Self {
                    base_url: first.base_url.to_string(),
                    model: first.default_model.to_string(),
                }
            }
        }
    }
}

/// The key being typed, held apart from everything else so it is easy to
/// prove it never reaches a save payload it does not belong in.
#[derive(Default)]
pub struct CredentialDraft {
    /// Never persisted, never logged, cleared on submit.
    pub secret: String,
    /// The reader asked to replace a key that is already set.
    pub replacing: bool,
}

impl CredentialDraft {
    pub fn clear(&mut self) {
        self.secret.clear();
        self.replacing = false;
    }
}

/// What the reader asked for this frame.
#[derive(Default)]
pub struct AgentConfigResponse {
    /// Provider base URL or model edited — the draft changed.
    pub provider_changed: bool,
    /// Save the typed key. The caller sends it write-only and then calls
    /// [`CredentialDraft::clear`].
    pub submit_credential: bool,
    /// Run one cheap completion against the saved config.
    pub test_clicked: bool,
    /// Remove the stored credential entirely.
    pub clear_credential: bool,
    /// The entitlement (roles/budgets) changed.
    pub budget_changed: bool,
    /// Refetch the guild's role roster.
    pub refresh_roles: bool,
}

/// Provider selection: preset buttons, then the two fields they fill.
pub fn provider_picker(ui: &mut Ui, draft: &mut ProviderDraft) -> bool {
    install_phosphor_font(ui.ctx());
    let mut changed = false;
    let current = preset_for_base_url(&draft.base_url);

    section_heading(ui, "Provider");

    ui.horizontal_wrapped(|ui| {
        for preset in provider_presets() {
            let selected = current.as_ref().is_some_and(|c| c.label == preset.label);
            if ui.selectable_label(selected, preset.label).clicked() && !selected {
                // Switching provider replaces the model too — a model name is
                // provider-specific, and carrying `grok-4.3` to OpenAI would
                // produce a save that validates and then fails on first use.
                draft.base_url = preset.base_url.to_string();
                draft.model = preset.default_model.to_string();
                changed = true;
            }
        }
        // Custom is a real choice, not a fallback: it is selected exactly
        // when no preset matches, and clicking it clears the URL so the
        // field is obviously waiting for one.
        let custom = current.is_none();
        if ui
            .selectable_label(custom, "Custom")
            .on_hover_text("any OpenAI-compatible endpoint")
            .clicked()
            && !custom
        {
            draft.base_url.clear();
            changed = true;
        }
    });

    ui.add_space(8.0);
    egui::Grid::new("agent_provider_fields")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            changed |= field_row(ui, "API base URL", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.base_url)
                        .hint_text("https://…")
                        .desired_width(300.0),
                )
                .changed()
            });
            changed |= field_row(ui, "Model", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.model)
                        .hint_text("model name")
                        .desired_width(300.0),
                )
                .changed()
            });
        });

    // Named before saving rather than after: both of these produce a config
    // that stores fine and fails on the first question a member asks.
    let problems = provider_problems(&draft.base_url, &draft.model);
    if !problems.is_empty() {
        ui.add_space(4.0);
        for problem in problems {
            ui.horizontal(|ui| {
                // `rich_text`, not `as_str` — the codepoint only resolves in
                // the Phosphor FAMILY, and handing the bare string to a
                // normal label renders tofu.
                ui.label(PhosphorIcon::Warning.rich_text(13.0, theme::WARNING));
                ui.colored_label(theme::WARNING, problem);
            });
        }
    }
    if let Some(preset) = preset_for_base_url(&draft.base_url) {
        ui.add_space(4.0);
        ui.hyperlink_to(format!("Get a {} API key", preset.label), preset.keys_url);
    }

    changed
}

/// The key: metadata when one is set, an input when one is being supplied.
pub fn credential_field(
    ui: &mut Ui,
    status: Option<&ProviderStatus>,
    draft: &mut CredentialDraft,
    now_ms: f64,
    response: &mut AgentConfigResponse,
) {
    install_phosphor_font(ui.ctx());
    section_heading(ui, "API key");

    if let Some(status) = status {
        ui.horizontal(|ui| {
            // The masked tail, not the key. Enough to tell two keys apart
            // when someone is checking they rotated the right one.
            ui.label(
                egui::RichText::new(&status.masked_key)
                    .monospace()
                    .color(theme::TEXT_PRIMARY),
            );
            let set = relative_label(((now_ms - status.set_at_ms) / 1000.0) as i64);
            // For a fresh key "never used" is ordinary; for an old one it is
            // the symptom that nobody is reaching the agent at all.
            let used = match status.last_used_ms {
                Some(used) => {
                    format!(
                        "last used {}",
                        relative_label(((now_ms - used) / 1000.0) as i64)
                    )
                }
                None => "never used".to_string(),
            };
            ui.colored_label(theme::TEXT_MUTED, format!("set {set} · {used}"));
        });
        ui.add_space(4.0);
    }

    if let Some(error) = status.and_then(|s| s.last_error.as_deref()) {
        // The provider's real words, to the admin only. Members get a canned
        // line that says nothing about anyone's billing.
        ui.horizontal(|ui| {
            ui.label(PhosphorIcon::Warning.rich_text(13.0, theme::ERROR));
            ui.colored_label(theme::ERROR, format!("last attempt failed: {error}"));
        });
    }

    let entering = status.is_none() || draft.replacing;
    if entering {
        ui.horizontal(|ui| {
            let field = egui::TextEdit::singleline(&mut draft.secret)
                .password(true)
                .hint_text("paste key — stored encrypted, never shown again")
                .desired_width(280.0);
            ui.add(field);
            let ready = !draft.secret.trim().is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("Save key"))
                .clicked()
            {
                response.submit_credential = true;
            }
            if draft.replacing && ui.button("Cancel").clicked() {
                draft.clear();
            }
        });
        ui.colored_label(
            theme::TEXT_MUTED,
            "Encrypted before it is stored. It is never displayed again — \
             replacing it is the only way to change it.",
        );
    } else {
        ui.horizontal(|ui| {
            if ui.button("Replace key").clicked() {
                draft.replacing = true;
                draft.secret.clear();
            }
            if ui
                .button("Test")
                .on_hover_text("one cheap completion against this configuration")
                .clicked()
            {
                response.test_clicked = true;
            }
            // Removal turns the agent off for the whole server, so it is not
            // a third identical button in a row of harmless ones — it sits
            // apart and says what it costs.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(phosphor_label(ui, PhosphorIcon::Trash, "Remove key"))
                    .on_hover_text("the agent stops answering in this server")
                    .clicked()
                {
                    response.clear_credential = true;
                }
            });
        });
    }
}

/// Who may ask, and for how much a day.
///
/// Lifted from the standalone admin, where it was operator-only because the
/// platform paid. Under bring-your-own-key the owner pays, so this is their
/// control and it is shared rather than private to one app.
pub fn budget_editor(
    ui: &mut Ui,
    agent: &mut Option<AgentEntitlement>,
    roles: Option<&[GuildRole]>,
    id_salt: &str,
    response: &mut AgentConfigResponse,
) {
    let mode = AgentMode::of(agent.as_ref());

    install_phosphor_font(ui.ctx());
    section_heading(ui, "Who may ask");

    ui.horizontal_wrapped(|ui| {
        for option in [AgentMode::Off, AgentMode::Everyone, AgentMode::ByRole] {
            let mut current = mode;
            if ui
                .selectable_value(&mut current, option, option.label())
                .on_hover_text(option.hint())
                .clicked()
                && option != mode
            {
                // Switching mode REBUILDS rather than preserving what the
                // other mode held. Tiers kept across a trip through
                // "everyone" would silently come back, which is the ambiguity
                // this control exists to remove.
                *agent = option.entitlement();
                response.budget_changed = true;
            }
        }
    });
    ui.colored_label(theme::TEXT_MUTED, mode.hint());

    let Some(entitlement) = agent.as_mut() else {
        return;
    };
    ui.add_space(4.0);

    if mode == AgentMode::Everyone {
        ui.horizontal(|ui| {
            ui.label("Every member gets");
            response.budget_changed |= ui
                .add(token_drag(&mut entitlement.default_daily_tokens, 1))
                .changed();
            ui.colored_label(theme::TEXT_MUTED, "tokens/day");
        });
        return;
    }

    // Whatever the roster last returned. Absent means never fetched, which
    // the select renders differently from "fetched, and this role is gone".
    let options: Vec<SelectOption> = roles
        .unwrap_or(&[])
        .iter()
        .map(|r| {
            SelectOption::new(r.id.clone(), format!("@{}", r.name))
                .swatch((r.color != 0).then(|| {
                    egui::Color32::from_rgb(
                        ((r.color >> 16) & 0xff) as u8,
                        ((r.color >> 8) & 0xff) as u8,
                        (r.color & 0xff) as u8,
                    )
                }))
        })
        .collect();

    ui.horizontal(|ui| {
        if ui
            .button(phosphor_label(
                ui,
                PhosphorIcon::ArrowsClockwise,
                "Refresh roles",
            ))
            .clicked()
        {
            response.refresh_roles = true;
        }
        ui.colored_label(
            theme::TEXT_MUTED,
            match (roles, options.len()) {
                (None, _) => "roles not loaded".to_string(),
                (Some(_), n) => format!("{n} roles"),
            },
        );
    });

    ui.add_space(8.0);
    let mut remove_tier: Option<usize> = None;
    for (index, tier) in entitlement.tiers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            // Salted per row: two tiers sharing a salt share one open menu.
            let resp = Select::new((id_salt, "tier", index), &options)
                .value_from_id(
                    &tier.role,
                    "no such role in this server — it may have been deleted",
                )
                .placeholder("Select role…")
                .width(200.0)
                .show(ui);
            if let Some(id) = resp.chosen {
                tier.role = id;
                response.budget_changed = true;
            }
            if resp.cleared {
                tier.role.clear();
                response.budget_changed = true;
            }

            response.budget_changed |= ui.add(token_drag(&mut tier.daily_tokens, 0)).changed();
            ui.colored_label(theme::TEXT_MUTED, "tokens/day");

            // Removing the TIER is pushed to the far right, away from the
            // picker's own clear-the-role `×`. Adjacent, similarly-sized
            // controls that discard different amounts of work is how someone
            // deletes a row when they meant to change a name — and a trash
            // can says "this row goes" in a way another × does not.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // PLACEHOLDER colour so the button's own hover/disabled state
                // tints the glyph, as it would any other label.
                if ui
                    .button(PhosphorIcon::Trash.rich_text(14.0, egui::Color32::PLACEHOLDER))
                    .on_hover_text("remove this tier")
                    .clicked()
                {
                    remove_tier = Some(index);
                }
            });
        });
    }

    if let Some(index) = remove_tier {
        entitlement.tiers.remove(index);
        response.budget_changed = true;
    }
    ui.add_space(4.0);
    if ui
        .button(phosphor_label(ui, PhosphorIcon::Plus, "Add role tier"))
        .clicked()
    {
        entitlement.tiers.push(AgentTier::default());
        response.budget_changed = true;
    }

    // A role with no id matches nobody, which reads as the tier silently not
    // working. Named here rather than rejected on save: a half-typed row is
    // an ordinary state to be in, not an error.
    if entitlement
        .tiers
        .iter()
        .any(|tier| tier.role.trim().is_empty())
    {
        ui.colored_label(theme::ACCENT_YELLOW, "a tier with no role matches nobody");
    }
    // Zero is a real setting (suspend a tier without deleting it), but it is
    // worth saying out loud, since "configured" and "able to ask" look the
    // same in a list otherwise.
    let muted = entitlement
        .tiers
        .iter()
        .filter(|t| t.daily_tokens == 0 && !t.role.trim().is_empty())
        .count();
    if muted > 0 {
        ui.colored_label(
            theme::TEXT_MUTED,
            format!("{muted} tier(s) set to 0 — those members cannot ask"),
        );
    }
}

/// Everything above, in the order a server owner meets it: pick a provider,
/// give it a key, then decide who may spend it.
#[allow(clippy::too_many_arguments)]
pub fn agent_config_section(
    ui: &mut Ui,
    status: Option<&ProviderStatus>,
    provider: &mut ProviderDraft,
    credential: &mut CredentialDraft,
    agent: &mut Option<AgentEntitlement>,
    roles: Option<&[GuildRole]>,
    id_salt: &str,
    now_ms: f64,
) -> AgentConfigResponse {
    let mut response = AgentConfigResponse::default();
    install_phosphor_font(ui.ctx());

    // The screen's own title, a size above the three section headings under
    // it — without that step the sections read as siblings of the whole
    // thing rather than parts of it.
    ui.label(
        egui::RichText::new("Agentic responses")
            .color(theme::TEXT_PRIMARY)
            .size(19.0)
            .strong(),
    );
    ui.colored_label(
        theme::TEXT_MUTED,
        "The bot answers when mentioned, using your own provider account. \
         You are billed by them directly.",
    );
    ui.add_space(14.0);

    // One separated block per concern. They are read in this order once, on
    // setup — pick a provider, give it a key, decide who may spend it — and
    // returned to individually afterwards, which is what the rules are for.
    response.provider_changed = provider_picker(ui, provider);

    ui.add_space(14.0);
    ui.separator();
    ui.add_space(10.0);
    credential_field(ui, status, credential, now_ms, &mut response);

    ui.add_space(14.0);
    ui.separator();
    ui.add_space(10.0);
    budget_editor(ui, agent, roles, id_salt, &mut response);

    response
}
