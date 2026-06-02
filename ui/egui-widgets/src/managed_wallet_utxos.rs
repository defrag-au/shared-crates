//! Managed-wallet UTxO breakdown — a structured, role-aware view of a
//! custodial wallet's on-chain UTxOs.
//!
//! The unified mint+payments wallet (settle-as-you-mint) should hold **only
//! ADA**: buyer payments waiting to mint + a small operating float, plus the
//! change those txs return. It must never hold native assets — an NFT sitting
//! here means the wallet minted to *itself* (a self-payment bug) or a token
//! was sent in by mistake. The plain "tx#idx · N ADA · +2 assets" list buried
//! that distinction (and coloured the assets a cheerful green), so this widget
//! groups UTxOs by role and flags asset-bearing ones loudly.
//!
//! ```ignore
//! ManagedWalletUtxos::new(&snapshot.utxos)
//!     .assets_unexpected(true) // a custodial mint+payments wallet
//!     .show(ui);
//! ```

use cardano_assets::utxo::UtxoApi;
use egui::{RichText, Ui};

use crate::theme;

/// Role-aware UTxO breakdown for a managed wallet.
pub struct ManagedWalletUtxos<'a> {
    utxos: &'a [UtxoApi],
    assets_unexpected: bool,
}

/// An ADA-only UTxO below this reads as "small" — close enough to min-UTxO
/// that a pile of them just bloats mint-tx inputs without adding useful
/// spendable balance.
const SMALL_UTXO_LOVELACE: u64 = 3_000_000;
/// Spendable-UTxO count at/above which the wallet reads as fragmented.
const FRAGMENT_COUNT: usize = 12;
/// Small-UTxO count at/above which the wallet reads as fragmented even with a
/// modest total count.
const FRAGMENT_SMALL_COUNT: usize = 5;

/// How consolidated the spendable side of the wallet is. A fragmented mint
/// wallet matters under settle-as-you-mint: a mint tx must spend several
/// inputs, and with large on-chain-art metadata (up to ~15 KB) the extra
/// input bytes can crowd the 16 KB tx-size limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletShape {
    /// A handful of UTxOs / ADA concentrated — comfortable to build txs from.
    Healthy,
    /// Many UTxOs and/or lots of small ones — consolidation candidate.
    Fragmented,
}

/// The pure-vs-asset split + totals — the testable core behind the widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtxoBreakdown {
    /// UTxOs holding only ADA (the spendable mint-funding pool).
    pub ada_only_count: usize,
    /// ADA-only UTxOs below [`SMALL_UTXO_LOVELACE`] (dust-ish fragments).
    pub ada_only_small_count: usize,
    /// Σ lovelace across the ADA-only UTxOs.
    pub ada_only_lovelace: u64,
    /// UTxOs that carry one or more native assets.
    pub asset_bearing_count: usize,
    /// Σ lovelace locked in asset-bearing UTxOs (min-UTxO riding the tokens).
    pub asset_bearing_lovelace: u64,
    /// Total native tokens across all asset-bearing UTxOs.
    pub token_count: usize,
    /// Σ lovelace across every UTxO.
    pub total_lovelace: u64,
}

impl UtxoBreakdown {
    /// Split a wallet's UTxOs into the ADA-only vs asset-bearing buckets.
    pub fn of(utxos: &[UtxoApi]) -> Self {
        let mut b = UtxoBreakdown {
            ada_only_count: 0,
            ada_only_small_count: 0,
            ada_only_lovelace: 0,
            asset_bearing_count: 0,
            asset_bearing_lovelace: 0,
            token_count: 0,
            total_lovelace: 0,
        };
        for u in utxos {
            b.total_lovelace = b.total_lovelace.saturating_add(u.lovelace);
            if u.assets.is_empty() {
                b.ada_only_count += 1;
                if u.lovelace < SMALL_UTXO_LOVELACE {
                    b.ada_only_small_count += 1;
                }
                b.ada_only_lovelace = b.ada_only_lovelace.saturating_add(u.lovelace);
            } else {
                b.asset_bearing_count += 1;
                b.asset_bearing_lovelace = b.asset_bearing_lovelace.saturating_add(u.lovelace);
                b.token_count += u.assets.len();
            }
        }
        b
    }

    /// `true` when the wallet holds any native asset.
    pub fn has_assets(&self) -> bool {
        self.asset_bearing_count > 0
    }

    /// Consolidation read on the spendable (ADA-only) side. Heuristic: many
    /// spendable UTxOs OR several small ones ⇒ [`WalletShape::Fragmented`].
    pub fn shape(&self) -> WalletShape {
        if self.ada_only_count >= FRAGMENT_COUNT
            || self.ada_only_small_count >= FRAGMENT_SMALL_COUNT
        {
            WalletShape::Fragmented
        } else {
            WalletShape::Healthy
        }
    }
}

impl<'a> ManagedWalletUtxos<'a> {
    /// New breakdown view. Defaults to treating assets as **unexpected** (the
    /// custodial mint+payments wallet case); call [`assets_unexpected`] with
    /// `false` for a generic wallet where holding tokens is normal.
    ///
    /// [`assets_unexpected`]: Self::assets_unexpected
    pub fn new(utxos: &'a [UtxoApi]) -> Self {
        Self {
            utxos,
            assets_unexpected: true,
        }
    }

    /// Whether asset-bearing UTxOs should be flagged as an anomaly.
    pub fn assets_unexpected(mut self, unexpected: bool) -> Self {
        self.assets_unexpected = unexpected;
        self
    }

    /// Render the breakdown into `ui`.
    pub fn show(self, ui: &mut Ui) {
        if self.utxos.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, "No UTxOs at this address.");
            return;
        }
        let b = UtxoBreakdown::of(self.utxos);

        // ── Summary line ──────────────────────────────────────────
        ui.label(
            RichText::new(format!(
                "{} UTxO{} · {} ADA total",
                self.utxos.len(),
                plural(self.utxos.len()),
                ada(b.total_lovelace),
            ))
            .color(theme::TEXT_SECONDARY)
            .small(),
        );
        ui.add_space(6.0);

        // ── Visual UTxO strip ─────────────────────────────────────
        // One block per UTxO, width ∝ lovelace (clamped so tiny ones stay
        // visible): a glance tells you whether the wallet is a couple of fat
        // blocks (healthy) or a scatter of thin ones (fragmented). Spendable
        // = green, asset-bearing = flagged.
        render_block_strip(ui, self.utxos, self.assets_unexpected);

        // Fragmentation read — only framed as a problem for a custodial mint
        // wallet, where input bloat fights the on-chain-art tx-size budget.
        if self.assets_unexpected && b.shape() == WalletShape::Fragmented {
            let small = if b.ada_only_small_count > 0 {
                format!(
                    " ({} under {} ADA)",
                    b.ada_only_small_count,
                    SMALL_UTXO_LOVELACE / 1_000_000
                )
            } else {
                String::new()
            };
            ui.horizontal(|ui| {
                crate::icons::install_phosphor_font(ui.ctx());
                ui.label(crate::PhosphorIcon::Warning.rich_text(12.0, theme::ACCENT_YELLOW));
                ui.label(
                    RichText::new(format!(
                        "Fragmented — {} spendable UTxOs{small}",
                        b.ada_only_count
                    ))
                    .color(theme::ACCENT_YELLOW)
                    .small()
                    .strong(),
                );
            });
            ui.label(
                RichText::new(
                    "A mint tx must spend several as inputs; with large on-chain-art metadata \
                     that can crowd the 16 KB tx limit. Consider consolidating (a Fund/no-op \
                     self-send merges them).",
                )
                .color(theme::TEXT_SECONDARY)
                .small(),
            );
        }
        ui.add_space(6.0);

        // ── Spendable ADA (the expected contents) ─────────────────
        ui.label(
            RichText::new(format!(
                "Spendable ADA — {} across {} UTxO{}",
                ada(b.ada_only_lovelace),
                b.ada_only_count,
                plural(b.ada_only_count),
            ))
            .color(theme::ACCENT_GREEN)
            .small()
            .strong(),
        );
        if self.assets_unexpected {
            ui.label(
                RichText::new(
                    "Buyer payments + operating float — what funds minting and the inline payouts.",
                )
                .color(theme::TEXT_MUTED)
                .small(),
            );
        }
        ui.add_space(2.0);
        for u in self.utxos.iter().filter(|u| u.assets.is_empty()) {
            utxo_ref_row(ui, u);
        }

        // ── Asset-bearing UTxOs (flagged when unexpected) ─────────
        if b.has_assets() {
            ui.add_space(8.0);
            if self.assets_unexpected {
                ui.horizontal(|ui| {
                    crate::icons::install_phosphor_font(ui.ctx());
                    ui.label(crate::PhosphorIcon::Warning.rich_text(12.0, theme::ACCENT_RED));
                    ui.label(
                        RichText::new(format!(
                            "Holds assets — {} UTxO{} carrying {} token{}",
                            b.asset_bearing_count,
                            plural(b.asset_bearing_count),
                            b.token_count,
                            plural(b.token_count),
                        ))
                        .color(theme::ACCENT_RED)
                        .small()
                        .strong(),
                    );
                });
                ui.label(
                    RichText::new(
                        "A mint + payments wallet should hold only ADA. Tokens here were most \
                         likely minted to the wallet itself or sent in by mistake — move or burn \
                         them so they don't get spent as fee/change.",
                    )
                    .color(theme::ACCENT_YELLOW)
                    .small(),
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "Assets — {} UTxO{}",
                        b.asset_bearing_count,
                        plural(b.asset_bearing_count),
                    ))
                    .color(theme::TEXT_SECONDARY)
                    .small()
                    .strong(),
                );
            }
            ui.add_space(2.0);
            for u in self.utxos.iter().filter(|u| !u.assets.is_empty()) {
                utxo_ref_row(ui, u);
                for aq in &u.assets {
                    let name = aq.asset_id.asset_name();
                    let name = if name.trim().is_empty() {
                        "(no name)".to_string()
                    } else {
                        name
                    };
                    let qty = if aq.quantity > 1 {
                        format!(" ×{}", aq.quantity)
                    } else {
                        String::new()
                    };
                    let color = if self.assets_unexpected {
                        theme::ACCENT_YELLOW
                    } else {
                        theme::TEXT_SECONDARY
                    };
                    ui.label(
                        RichText::new(format!(
                            "        {name}{qty} · {}",
                            truncate(&aq.asset_id.policy_id, 10)
                        ))
                        .small()
                        .color(color),
                    );
                }
            }
        }
    }
}

/// One block per UTxO, width ∝ lovelace (clamped so tiny ones stay visible),
/// wrapping across rows. Spendable (ADA-only) blocks first, big→small, in
/// green; asset-bearing blocks after, flagged. Hover shows the ref + amount.
fn render_block_strip(ui: &mut Ui, utxos: &[UtxoApi], assets_unexpected: bool) {
    let max = utxos.iter().map(|u| u.lovelace).max().unwrap_or(1).max(1);
    const H: f32 = 20.0;
    const MIN_W: f32 = 6.0;
    const MAX_W: f32 = 72.0;

    let mut order: Vec<&UtxoApi> = utxos.iter().filter(|u| u.assets.is_empty()).collect();
    order.sort_by(|a, b| b.lovelace.cmp(&a.lovelace));
    let mut with_assets: Vec<&UtxoApi> = utxos.iter().filter(|u| !u.assets.is_empty()).collect();
    with_assets.sort_by(|a, b| b.lovelace.cmp(&a.lovelace));
    order.extend(with_assets);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        for u in order {
            let is_asset = !u.assets.is_empty();
            let frac = u.lovelace as f32 / max as f32;
            let w = (MIN_W + frac * (MAX_W - MIN_W)).clamp(MIN_W, MAX_W);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::hover());
            let color = if is_asset && assets_unexpected {
                theme::ACCENT_RED
            } else if is_asset {
                theme::ACCENT_YELLOW
            } else {
                theme::ACCENT_GREEN
            };
            ui.painter().rect_filled(rect, 2.0, color);
            let head = &u.tx_hash[..u.tx_hash.len().min(8)];
            let mut tip = format!("{head}…#{} · {} ADA", u.output_index, ada(u.lovelace));
            if is_asset {
                tip.push_str(&format!(
                    " · {} asset{}",
                    u.assets.len(),
                    plural(u.assets.len())
                ));
            }
            resp.on_hover_text(tip);
        }
    });
}

/// One `tx_hash#index [copy] · N ADA` row. The ref is a copyable `IdPill`
/// (operators paste it into an explorer / `cardano-cli`).
fn utxo_ref_row(ui: &mut Ui, u: &UtxoApi) {
    ui.horizontal(|ui| {
        crate::IdPill::utxo_ref(&u.tx_hash, u.output_index).show(ui);
        ui.label(
            RichText::new(format!("{} ADA", ada(u.lovelace)))
                .monospace()
                .small()
                .color(theme::TEXT_PRIMARY),
        );
    });
}

/// Lovelace → ADA with 6 decimals.
fn ada(lovelace: u64) -> String {
    format!("{:.6}", lovelace as f64 / 1_000_000.0)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Head-truncate a long id (policy_id) to `keep` chars + ellipsis.
fn truncate(s: &str, keep: usize) -> String {
    if s.len() <= keep {
        return s.to_string();
    }
    format!("{}…", &s[..keep])
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::utxo::AssetQuantity;
    use cardano_assets::AssetId;

    fn pure(lovelace: u64) -> UtxoApi {
        UtxoApi {
            tx_hash: format!("tx{lovelace}"),
            output_index: 0,
            lovelace,
            assets: vec![],
            tags: vec![],
        }
    }

    fn with_assets(lovelace: u64, n: usize) -> UtxoApi {
        let assets = (0..n)
            .map(|i| AssetQuantity {
                asset_id: AssetId::new_unchecked("policy".to_string(), format!("{i:02x}")),
                quantity: 1,
            })
            .collect();
        UtxoApi {
            tx_hash: format!("txa{lovelace}"),
            output_index: 1,
            lovelace,
            assets,
            tags: vec![],
        }
    }

    #[test]
    fn breakdown_splits_pure_and_asset_bearing() {
        let utxos = vec![
            pure(117_667_750),
            with_assets(1_206_800, 2),
            pure(68_125_678),
        ];
        let b = UtxoBreakdown::of(&utxos);
        assert_eq!(b.ada_only_count, 2);
        assert_eq!(b.ada_only_lovelace, 117_667_750 + 68_125_678);
        assert_eq!(b.asset_bearing_count, 1);
        assert_eq!(b.asset_bearing_lovelace, 1_206_800);
        assert_eq!(b.token_count, 2);
        assert_eq!(b.total_lovelace, 117_667_750 + 1_206_800 + 68_125_678);
        assert!(b.has_assets());
    }

    #[test]
    fn breakdown_pure_wallet_has_no_assets() {
        let b = UtxoBreakdown::of(&[pure(10_000_000), pure(5_000_000)]);
        assert!(!b.has_assets());
        assert_eq!(b.asset_bearing_count, 0);
        assert_eq!(b.token_count, 0);
    }

    #[test]
    fn shape_healthy_for_a_few_fat_utxos() {
        let b = UtxoBreakdown::of(&[pure(140_000_000), pure(117_667_750), pure(68_000_000)]);
        assert_eq!(b.ada_only_small_count, 0);
        assert_eq!(b.shape(), WalletShape::Healthy);
    }

    #[test]
    fn shape_fragmented_on_high_count() {
        // 12 spendable UTxOs trips the count threshold even though none are small.
        let utxos: Vec<UtxoApi> = (0..12).map(|_| pure(10_000_000)).collect();
        let b = UtxoBreakdown::of(&utxos);
        assert_eq!(b.shape(), WalletShape::Fragmented);
    }

    #[test]
    fn shape_fragmented_on_small_pile() {
        // A couple of big UTxOs but five dust-ish ones → fragmented.
        let mut utxos = vec![pure(120_000_000), pure(40_000_000)];
        utxos.extend((0..5).map(|_| pure(1_200_000)));
        let b = UtxoBreakdown::of(&utxos);
        assert_eq!(b.ada_only_small_count, 5);
        assert!(b.ada_only_count < FRAGMENT_COUNT);
        assert_eq!(b.shape(), WalletShape::Fragmented);
    }
}
