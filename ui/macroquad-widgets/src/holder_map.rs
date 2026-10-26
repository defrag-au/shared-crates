//! `HolderMap` — holdings by area, grouped by what they are, on a treemap that
//! holds still as shares move.
//!
//! The treemap cuts are made once and only re-weighted ([`crate::treemap`]), so
//! a trade moves tiles by slivers rather than re-packing them. The host eases
//! each tile and lights the trader's; this draws the placed tiles and the
//! groups' names, and clips a tile still easing in from elsewhere.

use std::collections::HashMap;

use macroquad::prelude::*;
use ui_theme::{TextSize, mix};

use crate::holder_board::{Holding, HoldingKind, holding_colour};
use crate::market_ink::MarketInk;
use crate::painter::{Painter, draw_rounded_rect};
use crate::theme::with_alpha;
use crate::treemap::{DEFAULT_MAX_ASPECT, TreemapState};

/// The groups, in the fixed order the map lays them out: a group's block stays
/// where it is as the shares shift, and only its edges move.
pub const GROUPS: [&str; 7] = [
    "PEOPLE",
    "POOLS",
    "LOCKED",
    "STAKED",
    "CONTRACTS",
    "AT VENUES",
    "BURNED",
];

/// The map's group a holding sits in.
pub fn group_of(kind: HoldingKind) -> &'static str {
    match kind {
        HoldingKind::Person(_) => "PEOPLE",
        HoldingKind::Pool => "POOLS",
        HoldingKind::Lock => "LOCKED",
        HoldingKind::Staking => "STAKED",
        HoldingKind::Venue => "AT VENUES",
        HoldingKind::Contract => "CONTRACTS",
        HoldingKind::Burn => "BURNED",
    }
}

/// A map's placed tiles and its groups' blocks.
#[derive(Debug, Clone, Default)]
pub struct HolderMapLayout {
    pub tiles: HashMap<String, Rect>,
    pub groups: Vec<(&'static str, Rect)>,
}

/// The cuts a map is made of, kept between frames: one tree for the groups'
/// blocks, and one per group for its holdings. A share changing moves cuts; a
/// holder leaving or joining changes one corner.
#[derive(Debug, Clone)]
pub struct HolderMapState {
    groups: TreemapState<&'static str>,
    members: HashMap<&'static str, TreemapState<String>>,
}

impl Default for HolderMapState {
    fn default() -> Self {
        Self {
            groups: TreemapState::new(DEFAULT_MAX_ASPECT),
            members: HashMap::new(),
        }
    }
}

impl HolderMapState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every cut: a different token, or a jump.
    pub fn reset(&mut self) {
        self.groups.reset();
        self.members.clear();
    }

    /// Each holding's place on the map, and each group's block: the groups'
    /// blocks first, then the holdings inside each, both laid out by the kept
    /// trees.
    pub fn layout(&mut self, holdings: &[Holding], area: Rect) -> HolderMapLayout {
        let weights: Vec<(&'static str, f64)> = GROUPS
            .iter()
            .map(|g| {
                let w: f64 = holdings
                    .iter()
                    .filter(|h| group_of(h.kind) == *g)
                    .map(|h| h.amount.max(0.0))
                    .sum();
                (*g, w)
            })
            .collect();
        let blocks = self.groups.layout(&weights, area);
        self.members
            .retain(|g, _| blocks.iter().any(|(b, _)| b == g));
        let mut tiles = HashMap::new();
        for (group, rect) in &blocks {
            // `holdings` is largest first: the order a fresh tree is built in,
            // and newcomers are appended in.
            let members: Vec<(String, f64)> = holdings
                .iter()
                .filter(|h| group_of(h.kind) == *group)
                .map(|h| (h.key.to_string(), h.amount.max(0.0)))
                .collect();
            let tree = self.members.entry(group).or_default();
            tiles.extend(tree.layout(&members, *rect));
        }
        HolderMapLayout {
            tiles,
            groups: blocks,
        }
    }
}

/// One placed tile.
#[derive(Clone, Copy, Debug)]
pub struct MapTile<'a> {
    pub holding: &'a Holding<'a>,
    pub rect: Rect,
    /// A trade's colour and strength, mixed over the holding's colour.
    pub flash: Option<(Color, f32)>,
}

/// A group's block, named where it has room.
#[derive(Clone, Copy, Debug)]
pub struct MapGroup<'a> {
    pub label: &'a str,
    pub rect: Rect,
}

/// What the map draws.
#[derive(Clone, Copy, Debug)]
pub struct HolderMapVm<'a> {
    pub tiles: &'a [MapTile<'a>],
    pub groups: &'a [MapGroup<'a>],
    /// Every holding's amount together, for the shares.
    pub total: f64,
    pub ink: MarketInk,
}

/// Draw the placed tiles in `area`, and each group's name where it has room.
pub fn holder_map(p: &Painter, vm: &HolderMapVm, area: Rect) {
    let s = p.size(TextSize::Xs);
    let total = vm.total.max(1.0);
    for tile in vm.tiles {
        // Clipped to the map: a tile still easing in from elsewhere must not
        // draw outside it.
        let Some(r) = tile.rect.intersect(area) else {
            continue;
        };
        if r.w < 1.0 || r.h < 1.0 {
            continue;
        }
        let mut fill = holding_colour(vm.ink, &p.theme, tile.holding.kind);
        if let Some((c, a)) = tile.flash {
            fill = mix(fill, c, a.clamp(0.0, 1.0));
        }
        // A hairline between tiles: the background showing through.
        let inset = if r.w > 3.0 && r.h > 3.0 { 0.5 } else { 0.0 };
        draw_rectangle(
            r.x + inset,
            r.y + inset,
            r.w - inset * 2.0,
            r.h - inset * 2.0,
            with_alpha(fill, 0.85),
        );
        if r.w > 52.0 && r.h > 22.0 {
            let ink = ink_on(fill, p.theme.color.bg_primary, p.theme.color.text_primary);
            let name = Rect::new(r.x + 4.0, r.y + 3.0, r.w - 8.0, 11.0);
            p.text_in(
                &p.fit(tile.holding.label, s, name.w),
                name,
                s,
                ink,
                crate::Align::Start,
            );
            if r.h > 34.0 {
                let line = Rect::new(r.x + 4.0, r.y + 15.0, r.w - 8.0, 11.0);
                p.text_in(
                    &crate::format::share(tile.holding.amount / total),
                    line,
                    s,
                    with_alpha(ink, 0.75),
                    crate::Align::Start,
                );
            }
        }
    }
    // Each group's name over its block, where it has room: on its own dark
    // plate, since the tile under it may be any lightness.
    for group in vm.groups {
        let Some(r) = group.rect.intersect(area) else {
            continue;
        };
        if r.w > 70.0 && r.h > 50.0 {
            let w = p.measure(group.label, s).width;
            let plate = Rect::new(r.right() - w - 8.0, r.bottom() - 15.0, w + 8.0, 13.0);
            draw_rounded_rect(
                plate.x,
                plate.y,
                plate.w,
                plate.h,
                3.0,
                with_alpha(p.theme.color.bg_primary, 0.6),
            );
            p.text_in(
                group.label,
                plate,
                s,
                with_alpha(p.theme.color.text_primary, 0.8),
                crate::Align::Center,
            );
        }
    }
}

/// Dark ink on a light tile, light on a dark one.
fn ink_on(c: Color, dark: Color, light: Color) -> Color {
    match 0.299 * c.r + 0.587 * c.g + 0.114 * c.b > 0.6 {
        true => dark,
        false => light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding<'a>(key: &'a str, kind: HoldingKind, amount: f64) -> Holding<'a> {
        Holding {
            key,
            label: key,
            kind,
            amount,
        }
    }

    /// Every group with a holding has a block, and every holding a tile; the
    /// tiles fill their group's block rather than each other.
    #[test]
    fn every_holding_gets_a_tile_inside_its_group() {
        let holdings = vec![
            holding("a", HoldingKind::Person(4), 500.0),
            holding("b", HoldingKind::Person(1), 200.0),
            holding("pool", HoldingKind::Pool, 250.0),
            holding("burn", HoldingKind::Burn, 50.0),
        ];
        let mut state = HolderMapState::new();
        let area = Rect::new(0.0, 0.0, 400.0, 300.0);
        let laid = state.layout(&holdings, area);
        assert_eq!(laid.tiles.len(), holdings.len());
        for h in &holdings {
            assert!(laid.tiles.contains_key(h.key), "{} has no tile", h.key);
        }
        assert!(
            laid.groups
                .iter()
                .any(|(g, _)| *g == "PEOPLE" && laid.tiles.contains_key("a"))
        );
        let filled: f64 = laid.tiles.values().map(|r| r.w as f64 * r.h as f64).sum();
        let whole = (area.w * area.h) as f64;
        assert!((filled - whole).abs() / whole < 1e-3);
    }

    #[test]
    fn a_group_with_nothing_has_no_block() {
        let holdings = vec![holding("a", HoldingKind::Person(0), 10.0)];
        let mut state = HolderMapState::new();
        let laid = state.layout(&holdings, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(laid.groups.len(), 1);
        assert_eq!(laid.groups[0].0, "PEOPLE");
    }
}
