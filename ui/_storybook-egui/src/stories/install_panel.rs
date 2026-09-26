//! `InstallPanel` story — an OAuth install link beside the table it was built
//! from, the settings the link cannot carry, and the unresolved-link state.
//!
//! The permissions are the set the Black Flag rewards bot asks for, reasons
//! included, at the lengths those reasons actually run. One of them ("View Audit
//! Log") is a full clause and a half, which is the case that decides whether the
//! row wraps as a unit or shoves the name off the left edge.

use egui_widgets::InstallPanel;
use egui_widgets::install_panel::InstallFact;

use egui_widgets::theme::Token;

use crate::{accent, muted, tok};

/// The invite link, built from a placeholder application id. The panel renders
/// it as text and never parses it, so what the story is checking is that it
/// truncates with the whole URL on hover rather than wrapping the layout.
const INVITE: &str = "https://discord.com/oauth2/authorize?client_id=1234567890\
                      &scope=bot%20applications.commands&permissions=274945133760";

/// The permissions the link requests — name, and the call that breaks without
/// it.
fn permissions() -> Vec<InstallFact<'static>> {
    vec![
        InstallFact::new(
            "View Channels",
            "The channels the guild registry routes notifications to.",
        ),
        InstallFact::new(
            "Send Messages",
            "The distribution rollup and every game notification.",
        ),
        InstallFact::new(
            "Send Messages in Threads",
            "Notifications are routed to threads, and the dispatcher joins them.",
        ),
        InstallFact::new(
            "Embed Links",
            "Rollups, battle reports and epoch recaps arrive as embeds.",
        ),
        InstallFact::new("Attach Files", "Rendered graphics are posted as attachments."),
        InstallFact::new(
            "Read Message History",
            "Reading back the message a board edits in place.",
        ),
        InstallFact::new(
            "Add Reactions",
            "The reaction affordances augie adds to its own messages.",
        ),
        // The long one. A reason that runs to two clauses is the row that has to
        // wrap rather than clip.
        InstallFact::new(
            "View Audit Log",
            "The role snapshot's incremental path reads role changes from the audit \
             log. Without it the snapshot stops after its first pass.",
        ),
        InstallFact::new(
            "Change Nickname",
            "augie sets its own nickname per guild to show version and environment.",
        ),
    ]
}

/// The two things an install link cannot arrange.
fn requirements() -> Vec<InstallFact<'static>> {
    vec![
        InstallFact::new(
            "Server Members Intent",
            "A privileged toggle on the application, not a permission. Without it the \
             snapshot cannot list members and every tier count stays at zero.",
        ),
        InstallFact::new(
            "Command registration",
            "The applications.commands scope is in the link; that is what lets augie \
             register its slash commands in the server.",
        ),
    ]
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Install Panel")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "The install link, the permissions it asks for with the reason each one is \
             there, and the settings the link cannot carry.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    ui.label(
        egui::RichText::new("A prepared install")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(6.0);
    let permissions = permissions();
    let requirements = requirements();
    let response = InstallPanel::new(INVITE, &permissions)
        .requirements(&requirements)
        .blurb(
            "Send this to a server's administrators. Discord's own guild picker decides \
             which server it lands in.",
        )
        .note(
            "Already invited? Following the link for the same server updates the \
             permissions there rather than adding a second install.",
        )
        .show(ui);
    // The panel copies to the clipboard and says so; the story confirms the
    // response reached the caller, which is the half a widget cannot do.
    if response.copied {
        ui.label(
            egui::RichText::new("link copied")
                .small()
                .color(tok(ui, Token::Success)),
        );
    }
    ui.add_space(16.0);

    // Permissions with no requirement list: a bot whose install is entirely
    // carried by the link should not grow an empty "Not permissions" section.
    ui.label(
        egui::RichText::new("No hand-steps")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(6.0);
    let short = &permissions[..2];
    let _ = InstallPanel::new(INVITE, short).show(ui);
    ui.add_space(16.0);

    // The state the console is in before the worker answers with an application
    // id. Saying so beats a copy button that copies nothing.
    ui.label(
        egui::RichText::new("Link not resolved yet")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(6.0);
    let _ = InstallPanel::new("", &[])
        .empty_note("The invite link is built by the worker and arrives with the snapshot.")
        .show(ui);
    ui.add_space(10.0);
    // With nothing to say about it, the panel falls back to its own line rather
    // than to nothing at all.
    let _ = InstallPanel::new("", &[]).show(ui);
}
