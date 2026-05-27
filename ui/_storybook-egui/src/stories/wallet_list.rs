//! Story: `WalletList` — the per-client wallet roster on the admin portal
//! dashboard. Covers the realistic layouts: fresh client (1 wallet), client
//! with collections, mixed roles, and archived rows.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::wallet_list::{
    WalletList, WalletListAction, WalletListLayout, WalletListRole, WalletListRow,
};

#[derive(Default)]
pub struct WalletListState {
    /// Last archive action received, for demoing the action channel.
    pub last_archived: Option<u32>,
}

/// Build a sample row. `full_addr` is the canonical bech32 string the copy
/// button writes to the clipboard; the inline display is a middle-elided
/// truncation of it.
fn row(
    account_index: u32,
    label: &str,
    role: WalletListRole,
    full_addr: &str,
    auto_created: bool,
    archived: bool,
) -> WalletListRow {
    WalletListRow {
        account_index,
        label: label.to_string(),
        address_short: truncate_middle(full_addr, 14, 8),
        address_full: Some(full_addr.to_string()),
        role,
        auto_created,
        archived_at: if archived { Some(1_700_000_000) } else { None },
    }
}

/// Middle-elision used by the portal — same prefix/suffix idea as
/// `frontend::app::truncate_middle` but local to the story so the widget
/// crate doesn't take on a truncation utility.
fn truncate_middle(s: &str, prefix: usize, suffix: usize) -> String {
    if s.len() <= prefix + suffix + 1 {
        return s.to_string();
    }
    let p: String = s.chars().take(prefix).collect();
    let q: String = s
        .chars()
        .rev()
        .take(suffix)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{p}…{q}")
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletListState) {
    ui.label(
        egui::RichText::new(
            "Per-client wallet roster — Primary at the top, Collections grouped, \
             Custom folded below. Section headers appear when more than one bucket \
             is populated. Each row shows the bech32 enterprise address (truncated) \
             with a copy-to-clipboard icon.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(16.0);

    // ── Variant 1: fresh client (single Primary) ───────────────────────
    ui.label(
        egui::RichText::new("Fresh client — single Primary wallet")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Right after self-provision. No collections yet. No section header — \
             the widget collapses to a clean one-row layout.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![row(
        0,
        "Primary",
        WalletListRole::Primary,
        "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
        false,
        false,
    )];
    let _ = WalletList::new(&rows).show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 2: client with a couple of collections ─────────────────
    ui.label(
        egui::RichText::new("With collections — section headers appear")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Once a second bucket is populated, the widget surfaces \"Primary\" \
             and \"Collections (N)\" headers so users can scan.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            0,
            "Primary",
            WalletListRole::Primary,
            "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
            false,
            false,
        ),
        row(
            1,
            "Aliens",
            WalletListRole::Collection,
            "addr_test1vqnvts8en6q3qj9xkz2p3ahurv7lqkw0p3aexq8ad9a17e22",
            true,
            false,
        ),
        row(
            2,
            "Nikepig",
            WalletListRole::Collection,
            "addr_test1vp9k8tcs0g7d7yze0v9rryf5p3afy3w4cy3lhsq6m8bc91ff",
            true,
            false,
        ),
    ];
    let resp = WalletList::new(&rows).show(ui);
    for a in resp.actions {
        match a {
            WalletListAction::Archive { account_index } => {
                state.last_archived = Some(account_index);
            }
        }
    }
    if let Some(idx) = state.last_archived {
        ui.add_space(4.0);
        ui.colored_label(
            egui::Color32::LIGHT_GREEN,
            format!("✓ archive requested for #{idx}"),
        );
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 3: many collections + an archived row ──────────────────
    ui.label(
        egui::RichText::new("Many collections, with one archived")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Archived rows render dimmed with an \"archived\" chip and no Archive \
             button (the action is idempotent server-side but the affordance would \
             be confusing).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            0,
            "Primary",
            WalletListRole::Primary,
            "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
            false,
            false,
        ),
        row(1, "Aliens", WalletListRole::Collection, "addr_test1vqnvts8en6q3qj9xkz2p3ahurv7lqkw0p3aexq8ad9a17e22", true, false),
        row(2, "Nikepig", WalletListRole::Collection, "addr_test1vp9k8tcs0g7d7yze0v9rryf5p3afy3w4cy3lhsq6m8bc91ff", true, false),
        row(3, "Toolheads", WalletListRole::Collection, "addr_test1vqahjlw2qspx9chgxctf48k0wq2g9j8a6fl74xg7q34fab10", true, false),
        row(4, "JRYNers", WalletListRole::Collection, "addr_test1vrkxs3l5dt8jjlxhykqwa45dz8gj7nl2q6m3wt4kx07a8e02", true, true),
        row(5, "IslaNOVA", WalletListRole::Collection, "addr_test1vp7ckag5jdtwfse5fy0adyfeesn5tep4qz0u9rmskg2bc491", true, false),
    ];
    let _ = WalletList::new(&rows).show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 4: mixed roles including Custom ────────────────────────
    ui.label(
        egui::RichText::new("Mixed — including a Custom wallet")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Advanced users may add operator-named wallets outside the collection \
             lifecycle. They get their own bucket below Collections.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(0, "Primary", WalletListRole::Primary, "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465", false, false),
        row(1, "Aliens", WalletListRole::Collection, "addr_test1vqnvts8en6q3qj9xkz2p3ahurv7lqkw0p3aexq8ad9a17e22", true, false),
        row(
            7,
            "Cold storage",
            WalletListRole::Custom,
            "addr_test1vzv87fxqt9jrlxh2v9rxe6f55l9a5anq5tkwgmf2pl0099aa",
            false,
            false,
        ),
        row(
            8,
            "Treasury",
            WalletListRole::Custom,
            "addr_test1vqgs5gzlr8hxpwsxnq6kfdsr2vd9s7y3wlxc6n83sjaa44cc",
            false,
            false,
        ),
    ];
    let _ = WalletList::new(&rows).show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 5: card layout — single Primary ────────────────────────
    ui.label(
        egui::RichText::new("Card layout — single Primary")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Taller tile with a filled role pill, heading-sized label, and the \
             address on its own footer line. Use when wallet identity is the \
             focal element of the surface (an \"Identities\" sub-page, say) \
             rather than one row in a long list.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![row(
        0,
        "Primary",
        WalletListRole::Primary,
        "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
        false,
        false,
    )];
    let _ = WalletList::new(&rows)
        .with_layout(WalletListLayout::Card)
        .show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 6: card layout — multi-bucket with archived ────────────
    ui.label(
        egui::RichText::new("Card layout — multiple wallets, with archived")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Section headers still apply; archived tiles dim and lose the Archive \
             button. The same row VMs feed both layouts — only the per-card geometry \
             differs.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            0,
            "Primary",
            WalletListRole::Primary,
            "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
            false,
            false,
        ),
        row(
            1,
            "Aliens",
            WalletListRole::Collection,
            "addr_test1vqnvts8en6q3qj9xkz2p3ahurv7lqkw0p3aexq8ad9a17e22",
            true,
            false,
        ),
        row(
            2,
            "Nikepig",
            WalletListRole::Collection,
            "addr_test1vp9k8tcs0g7d7yze0v9rryf5p3afy3w4cy3lhsq6m8bc91ff",
            true,
            false,
        ),
        row(
            3,
            "JRYNers",
            WalletListRole::Collection,
            "addr_test1vrkxs3l5dt8jjlxhykqwa45dz8gj7nl2q6m3wt4kx07a8e02",
            true,
            true,
        ),
        row(
            7,
            "Cold storage",
            WalletListRole::Custom,
            "addr_test1vzv87fxqt9jrlxh2v9rxe6f55l9a5anq5tkwgmf2pl0099aa",
            false,
            false,
        ),
    ];
    let _ = WalletList::new(&rows)
        .with_layout(WalletListLayout::Card)
        .show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 7: card layout, 2-column grid ──────────────────────────
    ui.label(
        egui::RichText::new("Card layout — 2-column grid")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "`.with_columns(2)` packs cards into a grid. Per-bucket clamped so the \
             single Primary card still fills the surface width; Collections wraps \
             at 2-wide. Last row of an odd-count bucket leaves a trailing empty \
             cell to keep the grid aligned.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let rows = vec![
        row(
            0,
            "Primary",
            WalletListRole::Primary,
            "addr_test1vpedt5kty0v59fk2y4q44sxgs2my3aqlhxw7r5fzm9fc465",
            false,
            false,
        ),
        row(
            1,
            "Aliens",
            WalletListRole::Collection,
            "addr_test1vqnvts8en6q3qj9xkz2p3ahurv7lqkw0p3aexq8ad9a17e22",
            true,
            false,
        ),
        row(
            2,
            "Nikepig",
            WalletListRole::Collection,
            "addr_test1vp9k8tcs0g7d7yze0v9rryf5p3afy3w4cy3lhsq6m8bc91ff",
            true,
            false,
        ),
        row(
            3,
            "Toolheads",
            WalletListRole::Collection,
            "addr_test1vqahjlw2qspx9chgxctf48k0wq2g9j8a6fl74xg7q34fab10",
            true,
            false,
        ),
        row(
            4,
            "JRYNers",
            WalletListRole::Collection,
            "addr_test1vrkxs3l5dt8jjlxhykqwa45dz8gj7nl2q6m3wt4kx07a8e02",
            true,
            false,
        ),
        row(
            5,
            "IslaNOVA",
            WalletListRole::Collection,
            "addr_test1vp7ckag5jdtwfse5fy0adyfeesn5tep4qz0u9rmskg2bc491",
            true,
            false,
        ),
    ];
    let _ = WalletList::new(&rows)
        .with_layout(WalletListLayout::Card)
        .with_columns(2)
        .show(ui);

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(16.0);

    // ── Variant 8: list layout, 2-column grid ──────────────────────────
    ui.label(
        egui::RichText::new("List layout — 2-column grid")
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "The same `with_columns(2)` applies to the compact list mode for dense \
             surfaces. List rows are tighter, so 2 columns is the usable max — \
             3+ crowds the truncated address.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(8.0);

    let _ = WalletList::new(&rows)
        .with_layout(WalletListLayout::List)
        .with_columns(2)
        .show(ui);
}
