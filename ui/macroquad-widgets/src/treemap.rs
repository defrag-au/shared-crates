//! `Treemap` — weights laid out as rectangles whose areas are proportional,
//! by a kept tree of cuts that is built once and only re-weighted, then drawn
//! with labels where they fit.
//!
//! # Why a kept tree
//!
//! The first holders map was squarified (Bruls et al.), which sorts by size and
//! packs rows: the squarest tiles, and the least stable. Two holders trading
//! places re-sorted, the rows re-packed, and tiles across the map jumped on
//! every trade. An ordered split recomputed each frame was steadier (MEASURED
//! on $PERP a beat at a time: the median tile moved 1 unit) but its discrete
//! choices (which item halves a block, which way to cut a near-square one)
//! flipped as weights wobbled and holders came and went: a tenth of beats still
//! moved a tile 70–250 units. Remembering choices by position only moved the
//! flips around.
//!
//! So the choices are made once, into a tree. A weight changing moves the cuts
//! through it, continuously; a holder leaving takes its leaf out (its sibling
//! takes the space); a holder arriving splits the last leaf. Nothing else
//! re-divides. Only when weights have drifted so far that a tile is a sliver is
//! the tree rebuilt ([`Tree::worst_aspect`]).
//!
//! Pure arithmetic over `Rect`s — the layout half has no drawing, so it is
//! tested natively. [`treemap`] draws a layout; the host decides what the
//! weights mean.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use macroquad::prelude::*;
use ui_theme::{TextSize, mix};

use crate::painter::Painter;
use crate::theme::with_alpha;

/// The smallest tile a reader sees as a tile, in square units: 20 by 20.
pub const SEEN_AREA: f32 = 400.0;

/// The aspect past which a kept tree is rebuilt, when that helps enough.
pub const DEFAULT_MAX_ASPECT: f32 = 10.0;

/// The depth past which a kept tree is rebuilt: its walks recurse, and a
/// browser's stack is small.
pub const MAX_DEPTH: usize = 400;

/// A node: one item, or a cut across (side by side) or down (one above the
/// other) into two.
#[derive(Debug, Clone)]
enum Node<K> {
    Leaf(K),
    Cut {
        across: bool,
        first: Box<Node<K>>,
        second: Box<Node<K>>,
    },
}

/// The cuts a layout is made of, kept between layouts.
#[derive(Debug, Clone)]
pub struct Tree<K> {
    root: Option<Node<K>>,
}

impl<K> Default for Tree<K> {
    fn default() -> Self {
        Self { root: None }
    }
}

impl<K: Clone + Eq + Hash> Tree<K> {
    /// The tree for `items` (`(key, weight)`, in the order to keep) in
    /// `area`: each block cut where its weight is nearest halved, along its
    /// longer side.
    pub fn build(items: &[(K, f64)], area: Rect) -> Self {
        let kept: Vec<(K, f64)> = items
            .iter()
            .filter(|(_, w)| *w > 0.0 && w.is_finite())
            .cloned()
            .collect();
        Self {
            root: build_node(&kept, area),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Every key the tree holds.
    pub fn keys(&self) -> HashSet<K> {
        let mut out = HashSet::new();
        if let Some(n) = &self.root {
            collect(n, &mut out);
        }
        out
    }

    /// Take `key`'s leaf out: its sibling takes the whole of their block.
    pub fn remove(&mut self, key: &K) {
        if let Some(root) = self.root.take() {
            self.root = without(root, key);
        }
    }

    /// Add `key` by cutting the last leaf in two, along its longer side in
    /// `area` laid out with `weights`. Newcomers gather at the end.
    pub fn append(&mut self, key: K, weights: &HashMap<K, f64>, area: Rect) {
        let Some(root) = self.root.take() else {
            self.root = Some(Node::Leaf(key));
            return;
        };
        // The last leaf's place now, for which way to cut it.
        let last = last_key(&root);
        let laid = layout_node(&root, weights, area);
        let across = laid
            .iter()
            .find(|(k, _)| *k == last)
            .is_none_or(|(_, r)| r.w >= r.h);
        self.root = Some(with_last(root, key, across));
    }

    /// Add `items` (in order) as one block cut out of the last leaf: the
    /// last leaf and the newcomers built into a tree of their own in the
    /// last leaf's place. One layout of the whole, however many arrive.
    fn append_all(&mut self, items: Vec<(K, f64)>, weights: &HashMap<K, f64>, area: Rect) {
        if items.is_empty() {
            return;
        }
        let Some(root) = self.root.take() else {
            self.root = build_node(&items, area);
            return;
        };
        let last = last_key(&root);
        let laid = layout_node(&root, weights, area);
        let place = laid
            .iter()
            .find(|(k, _)| *k == last)
            .map_or(area, |(_, r)| *r);
        let mut block = Vec::with_capacity(items.len() + 1);
        block.push((last.clone(), weights.get(&last).copied().unwrap_or(0.0)));
        block.extend(items);
        // A last leaf of no weight cannot be cut at a share: the newcomers
        // take its place by themselves.
        let built = match block[0].1 > 0.0 {
            true => build_node(&block, place),
            false => build_node(&block[1..], place),
        };
        self.root = match built {
            Some(node) => Some(with_last_node(root, node)),
            None => Some(root),
        };
    }

    /// Lay the tree out in `area` by `weights`: every cut where the weights
    /// put it, every leaf's area its share. A key of no weight gets no
    /// rectangle.
    pub fn layout(&self, weights: &HashMap<K, f64>, area: Rect) -> Vec<(K, Rect)> {
        match &self.root {
            Some(n) if area.w > 0.0 && area.h > 0.0 => layout_node(n, weights, area),
            _ => Vec::new(),
        }
    }

    /// The worst SEEN tile's long side over its short, in a layout: past a
    /// limit the tree has drifted and is worth rebuilding.
    ///
    /// Tiles under [`SEEN_AREA`] do not count. A dust holding is a sliver
    /// however the map is cut (MEASURED on $PERP: the worst tile in a fresh
    /// layout was 2,600 times longer than wide, and 200,000 in one kept for
    /// half the tape, while the worst tile big enough to see was 2.5 in
    /// both). Counted, they rebuilt the map on three beats in 190, and
    /// those three were every move over 20 units.
    pub fn worst_aspect(laid: &[(K, Rect)]) -> f32 {
        laid.iter()
            .filter(|(_, r)| r.w * r.h >= SEEN_AREA)
            .map(|(_, r)| r.w.max(r.h) / r.w.min(r.h))
            .fold(1.0, f32::max)
    }
}

/// Keep `tree` in step with `items` (`(key, weight)`, largest first) in
/// `area`: leavers out, newcomers appended, and the whole rebuilt only when
/// it is empty or a tile has become a sliver (`max_aspect`). Returns the
/// layout.
pub fn keep<K: Clone + Eq + Hash>(
    tree: &mut Tree<K>,
    items: &[(K, f64)],
    area: Rect,
    max_aspect: f32,
) -> Vec<(K, Rect)> {
    let weights: HashMap<K, f64> = items
        .iter()
        .filter(|(_, w)| *w > 0.0 && w.is_finite())
        .cloned()
        .collect();
    if tree.is_empty() {
        *tree = Tree::build(items, area);
    } else {
        let have = tree.keys();
        // Leavers out in one pass over the tree, not one pass each.
        let gone: HashSet<&K> = have.iter().filter(|k| !weights.contains_key(*k)).collect();
        if !gone.is_empty()
            && let Some(root) = tree.root.take()
        {
            tree.root = without_all(root, &gone);
        }
        // This layout's newcomers, in order, as one block cut out of the
        // last tile: one layout to find it, and a balanced block rather
        // than a chain a newcomer deep (which a launch's thousands of
        // arrivals made thousands deep).
        let fresh: Vec<(K, f64)> = items
            .iter()
            .filter(|(k, w)| *w > 0.0 && w.is_finite() && !have.contains(k))
            .cloned()
            .collect();
        tree.append_all(fresh, &weights, area);
    }
    let laid = tree.layout(&weights, area);
    let worst = Tree::worst_aspect(&laid);
    // Every frame's newcomers deepen the tree's last corner by a level or
    // so. A layout walks it in one pass however deep, but by recursion, and
    // a browser's stack is small: past [`MAX_DEPTH`] it is rebuilt, which
    // re-packs the map once in a long while.
    let deep = tree.root.as_ref().is_some_and(|n| depth(n) > MAX_DEPTH);
    if worst <= max_aspect && !deep {
        return laid;
    }
    if deep {
        *tree = Tree::build(items, area);
        return tree.layout(&weights, area);
    }
    // Rebuilt only if that is much better: a [large, tiny] pair is a
    // sliver however it is cut, and rebuilding for it every frame would be
    // the re-packing this exists to stop.
    let fresh = Tree::build(items, area);
    let redone = fresh.layout(&weights, area);
    match Tree::worst_aspect(&redone) * 2.0 < worst {
        true => {
            *tree = fresh;
            redone
        }
        false => laid,
    }
}

/// A kept tree and its aspect limit, held by the host across frames.
#[derive(Debug, Clone)]
pub struct TreemapState<K> {
    tree: Tree<K>,
    max_aspect: f32,
}

impl<K> Default for TreemapState<K> {
    fn default() -> Self {
        Self {
            tree: Tree::default(),
            max_aspect: DEFAULT_MAX_ASPECT,
        }
    }
}

impl<K: Clone + Eq + Hash> TreemapState<K> {
    pub fn new(max_aspect: f32) -> Self {
        Self {
            tree: Tree::default(),
            max_aspect,
        }
    }

    /// The layout for `items` in `area`, keeping the cuts where they are unless
    /// the tree has drifted.
    pub fn layout(&mut self, items: &[(K, f64)], area: Rect) -> Vec<(K, Rect)> {
        keep(&mut self.tree, items, area, self.max_aspect)
    }

    /// Forget every cut: a different dataset, or a jump.
    pub fn reset(&mut self) {
        self.tree = Tree::default();
    }
}

/// One tile to draw. `rect` is absolute; a tile outside `area` is clipped.
#[derive(Debug, Clone, Copy)]
pub struct TreemapTile<'a> {
    pub rect: Rect,
    pub label: &'a str,
    /// The share under the label, where the tile has a second line of room.
    pub share: Option<&'a str>,
    pub fill: Color,
    /// A trade's colour and strength, mixed over `fill`.
    pub flash: Option<(Color, f32)>,
}

/// A group's block, named where it has room.
#[derive(Debug, Clone, Copy)]
pub struct TreemapGroup<'a> {
    pub rect: Rect,
    pub label: &'a str,
}

/// What a treemap draws: placed tiles and the groups around them.
#[derive(Debug, Clone, Copy)]
pub struct TreemapVm<'a> {
    pub tiles: &'a [TreemapTile<'a>],
    pub groups: &'a [TreemapGroup<'a>],
}

/// Draw a layout: each tile's fill, its label and share where they fit, and
/// each group's name at its foot. A tile still easing in from elsewhere is
/// clipped to `area`, so it cannot draw over its neighbours.
pub fn treemap(p: &Painter, vm: &TreemapVm, area: Rect) {
    let s = p.size(TextSize::Xs);
    for tile in vm.tiles {
        let Some(r) = tile.rect.intersect(area) else {
            continue;
        };
        if r.w < 1.0 || r.h < 1.0 {
            continue;
        }
        let fill = match tile.flash {
            Some((c, a)) => mix(tile.fill, c, a.clamp(0.0, 1.0)),
            None => tile.fill,
        };
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
            p.text_in(tile.label, name, s, ink, crate::Align::Start);
            if let Some(share) = tile.share
                && r.h > 34.0
            {
                let line = Rect::new(r.x + 4.0, r.y + 15.0, r.w - 8.0, 11.0);
                p.text_in(share, line, s, with_alpha(ink, 0.75), crate::Align::Start);
            }
        }
    }
    for group in vm.groups {
        let Some(r) = group.rect.intersect(area) else {
            continue;
        };
        if r.w > 70.0 && r.h > 50.0 {
            let label = Rect::new(r.x + 4.0, r.bottom() - 14.0, r.w - 8.0, 11.0);
            p.text_in(
                group.label,
                label,
                s,
                with_alpha(p.theme.color.text_primary, 0.45),
                crate::Align::End,
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

fn build_node<K: Clone>(items: &[(K, f64)], area: Rect) -> Option<Node<K>> {
    match items {
        [] => None,
        [(k, _)] => Some(Node::Leaf(k.clone())),
        _ => {
            let total: f64 = items.iter().map(|(_, w)| w).sum();
            let at = nearest_even(items, total);
            let across = area.w >= area.h;
            let first: f64 = items[..at].iter().map(|(_, w)| w).sum();
            let (a, b) = cut(area, (first / total) as f32, across);
            Some(Node::Cut {
                across,
                first: Box::new(build_node(&items[..at], a)?),
                second: Box::new(build_node(&items[at..], b)?),
            })
        }
    }
}

fn collect<K: Clone + Eq + Hash>(n: &Node<K>, out: &mut HashSet<K>) {
    match n {
        Node::Leaf(k) => {
            out.insert(k.clone());
        }
        Node::Cut { first, second, .. } => {
            collect(first, out);
            collect(second, out);
        }
    }
}

fn without<K: Eq>(n: Node<K>, key: &K) -> Option<Node<K>> {
    match n {
        Node::Leaf(k) if k == *key => None,
        Node::Leaf(k) => Some(Node::Leaf(k)),
        Node::Cut {
            across,
            first,
            second,
        } => match (without(*first, key), without(*second, key)) {
            (Some(a), Some(b)) => Some(Node::Cut {
                across,
                first: Box::new(a),
                second: Box::new(b),
            }),
            (Some(only), None) | (None, Some(only)) => Some(only),
            (None, None) => None,
        },
    }
}

fn without_all<K: Eq + Hash>(n: Node<K>, gone: &HashSet<&K>) -> Option<Node<K>> {
    match n {
        Node::Leaf(k) if gone.contains(&k) => None,
        Node::Leaf(k) => Some(Node::Leaf(k)),
        Node::Cut {
            across,
            first,
            second,
        } => match (without_all(*first, gone), without_all(*second, gone)) {
            (Some(a), Some(b)) => Some(Node::Cut {
                across,
                first: Box::new(a),
                second: Box::new(b),
            }),
            (Some(only), None) | (None, Some(only)) => Some(only),
            (None, None) => None,
        },
    }
}

/// `n` with its last leaf replaced by `node`.
fn with_last_node<K>(n: Node<K>, node: Node<K>) -> Node<K> {
    match n {
        Node::Leaf(_) => node,
        Node::Cut {
            across,
            first,
            second,
        } => Node::Cut {
            across,
            first,
            second: Box::new(with_last_node(*second, node)),
        },
    }
}

fn last_key<K: Clone>(n: &Node<K>) -> K {
    match n {
        Node::Leaf(k) => k.clone(),
        Node::Cut { second, .. } => last_key(second),
    }
}

fn with_last<K>(n: Node<K>, key: K, across: bool) -> Node<K> {
    match n {
        Node::Leaf(k) => Node::Cut {
            across,
            first: Box::new(Node::Leaf(k)),
            second: Box::new(Node::Leaf(key)),
        },
        Node::Cut {
            across: a,
            first,
            second,
        } => Node::Cut {
            across: a,
            first,
            second: Box::new(with_last(*second, key, across)),
        },
    }
}

/// Every node's weight, in pre-order: one pass over the tree, so a layout
/// costs a visit per node. Summing a subtree at each cut instead cost a visit
/// per node per level — and a tree that newcomers have grown at its end is
/// deep, which made a layout quadratic in the holders.
fn weigh<K: Eq + Hash>(n: &Node<K>, weights: &HashMap<K, f64>, out: &mut Vec<f64>) -> f64 {
    let at = out.len();
    out.push(0.0);
    let w = match n {
        Node::Leaf(k) => weights.get(k).copied().unwrap_or(0.0),
        Node::Cut { first, second, .. } => weigh(first, weights, out) + weigh(second, weights, out),
    };
    out[at] = w;
    w
}

/// How many nodes `n` holds, itself included: how far past a cut's first
/// child its second starts, in pre-order.
fn size<K>(n: &Node<K>) -> usize {
    match n {
        Node::Leaf(_) => 1,
        Node::Cut { first, second, .. } => 1 + size(first) + size(second),
    }
}

fn depth<K>(n: &Node<K>) -> usize {
    match n {
        Node::Leaf(_) => 1,
        Node::Cut { first, second, .. } => 1 + depth(first).max(depth(second)),
    }
}

fn layout_node<K: Clone + Eq + Hash>(
    n: &Node<K>,
    weights: &HashMap<K, f64>,
    area: Rect,
) -> Vec<(K, Rect)> {
    let mut sums = Vec::new();
    weigh(n, weights, &mut sums);
    let mut out = Vec::new();
    let mut at = 0;
    place(n, &sums, &mut at, area, &mut out);
    out
}

/// Lay `n` out in `area` from its pre-order weights, `at` being its index
/// there; leaves `at` past its last node.
fn place<K: Clone>(
    n: &Node<K>,
    sums: &[f64],
    at: &mut usize,
    area: Rect,
    out: &mut Vec<(K, Rect)>,
) {
    let me = *at;
    *at += 1;
    match n {
        Node::Leaf(k) => {
            if sums[me] > 0.0 {
                out.push((k.clone(), area));
            }
        }
        Node::Cut {
            across,
            first,
            second,
        } => {
            // The first child is next in pre-order; its weight is there.
            let (whole, a) = (sums[me], sums[me + 1]);
            if whole <= 0.0 {
                *at += size(first) + size(second);
                return;
            }
            let (ra, rb) = cut(area, (a / whole) as f32, *across);
            place(first, sums, at, ra, out);
            place(second, sums, at, rb, out);
        }
    }
}

/// `area` cut at fraction `f`: `across` its width (side by side), or down
/// its height (one above the other).
fn cut(area: Rect, f: f32, across: bool) -> (Rect, Rect) {
    match across {
        true => {
            let w = area.w * f;
            (
                Rect::new(area.x, area.y, w, area.h),
                Rect::new(area.x + w, area.y, area.w - w, area.h),
            )
        }
        false => {
            let h = area.h * f;
            (
                Rect::new(area.x, area.y, area.w, h),
                Rect::new(area.x, area.y + h, area.w, area.h - h),
            )
        }
    }
}

/// The split that leaves each side nearest half the weight.
fn nearest_even<K>(items: &[(K, f64)], total: f64) -> usize {
    let (mut at, mut best, mut acc) = (1, f64::INFINITY, 0.0);
    for (i, (_, w)) in items.iter().enumerate().take(items.len() - 1) {
        acc += w;
        let off = (acc - total / 2.0).abs();
        if off < best {
            (at, best) = (i + 1, off);
        }
    }
    at
}

/// Lay `items` out once, in the order given: a tree built and laid out at
/// the same weights. The tests' way in; the map keeps its tree ([`keep`]).
#[cfg(test)]
fn split<K: Clone + Eq + Hash>(items: &[(K, f64)], area: Rect) -> Vec<(K, Rect)> {
    let weights: HashMap<K, f64> = items.iter().cloned().collect();
    Tree::build(items, area).layout(&weights, area)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(r: &Rect) -> f64 {
        r.w as f64 * r.h as f64
    }

    #[test]
    fn areas_are_proportional_and_fill_the_space() {
        let space = Rect::new(10.0, 20.0, 600.0, 400.0);
        let items = vec![
            ("a", 6.0),
            ("b", 6.0),
            ("c", 4.0),
            ("d", 3.0),
            ("e", 2.0),
            ("f", 2.0),
            ("g", 1.0),
        ];
        let laid = split(&items, space);
        assert_eq!(laid.len(), items.len());
        let total: f64 = items.iter().map(|(_, w)| w).sum();
        for (k, r) in &laid {
            let w = items.iter().find(|(x, _)| x == k).unwrap().1;
            let want = w / total * area(&space);
            assert!(
                (area(r) - want).abs() / want < 1e-3,
                "{k}: {} against {want}",
                area(r)
            );
            assert!(
                r.x >= space.x - 1e-3 && r.right() <= space.right() + 1e-3,
                "{k} {r:?}"
            );
            assert!(
                r.y >= space.y - 1e-3 && r.bottom() <= space.bottom() + 1e-3,
                "{k} {r:?}"
            );
        }
        let filled: f64 = laid.iter().map(|(_, r)| area(r)).sum();
        assert!((filled - area(&space)).abs() / area(&space) < 1e-3);
    }

    #[test]
    fn tiles_do_not_overlap() {
        let space = Rect::new(0.0, 0.0, 800.0, 300.0);
        let items: Vec<(usize, f64)> = (1..=40).map(|i| (i, 1000.0 / i as f64)).collect();
        let laid = split(&items, space);
        for (i, (_, a)) in laid.iter().enumerate() {
            for (_, b) in &laid[i + 1..] {
                let ox = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
                let oy = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
                assert!(ox * oy < 1e-2, "{a:?} overlaps {b:?}");
            }
        }
    }

    /// SMALL, SMOOTH DELTAS: with the tree kept, a weight moving moves the
    /// cuts through it a little and nothing else.
    #[test]
    fn a_kept_tree_moves_tiles_a_little() {
        let space = Rect::new(0.0, 0.0, 600.0, 300.0);
        let items: Vec<(usize, f64)> = (1..=30).map(|i| (i, 1000.0 / i as f64)).collect();
        let mut tree = Tree::default();
        let before = keep(&mut tree, &items, space, 10.0);
        let mut nudged = items.clone();
        nudged[3].1 *= 1.05;
        let after = keep(&mut tree, &nudged, space, 10.0);
        let at = |laid: &[(usize, Rect)], k: usize| laid.iter().find(|(x, _)| *x == k).unwrap().1;
        let most = items
            .iter()
            .map(|(k, _)| {
                let (a, b) = (at(&before, *k), at(&after, *k));
                (a.x - b.x)
                    .abs()
                    .max((a.y - b.y).abs())
                    .max((a.right() - b.right()).abs())
                    .max((a.bottom() - b.bottom()).abs())
            })
            .fold(0.0f32, f32::max);
        assert!(most < 6.0, "a 5% change moved a tile {most} units");
    }

    /// A leaver's sibling takes its space; a newcomer splits the last tile.
    /// Everyone else stays where they were, to within the weights' shift.
    #[test]
    fn joining_and_leaving_change_only_their_corner() {
        let space = Rect::new(0.0, 0.0, 600.0, 300.0);
        let items: Vec<(usize, f64)> = (1..=30).map(|i| (i, 1000.0 / i as f64)).collect();
        let mut tree = Tree::default();
        keep(&mut tree, &items, space, 10.0);
        let mut more = items.clone();
        more.push((99, 5.0));
        let laid = keep(&mut tree, &more, space, 10.0);
        assert!(
            laid.iter().any(|(k, _)| *k == 99),
            "the newcomer has a tile"
        );
        let fewer: Vec<(usize, f64)> = more.iter().copied().filter(|(k, _)| *k != 7).collect();
        let laid = keep(&mut tree, &fewer, space, 10.0);
        assert!(laid.iter().all(|(k, _)| *k != 7), "the leaver has none");
        assert_eq!(laid.len(), fewer.len());
        let filled: f64 = laid.iter().map(|(_, r)| area(r)).sum();
        assert!((filled - area(&space)).abs() / area(&space) < 1e-3);
    }

    /// A launch: holders arriving a few a layout for thousands of layouts.
    /// Each arrival once deepened the tree's last corner by a level and laid
    /// the whole tree out again, which made a frame quadratic in the
    /// holders; the tree stays within its depth, and every holder has a
    /// tile.
    #[test]
    fn thousands_of_arrivals_keep_the_tree_shallow_and_whole() {
        let space = Rect::new(0.0, 0.0, 900.0, 520.0);
        let mut state = TreemapState::default();
        let mut items: Vec<(usize, f64)> = Vec::new();
        // Past MAX_DEPTH arrivals, so the rebuild is exercised too.
        for frame in 0..MAX_DEPTH + 100 {
            for j in 0..3 {
                let k = frame * 3 + j;
                items.push((k, 1.0 + (k % 97) as f64));
            }
            let laid = state.layout(&items, space);
            assert_eq!(laid.len(), items.len(), "frame {frame}");
        }
        let root = state.tree.root.as_ref().expect("a tree");
        assert!(depth(root) <= MAX_DEPTH, "depth {}", depth(root));
        let laid = state.layout(&items, space);
        let filled: f64 = laid.iter().map(|(_, r)| area(r)).sum();
        assert!((filled - area(&space)).abs() / area(&space) < 1e-3);
    }

    #[test]
    fn nothing_to_lay_out_is_nothing() {
        let space = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(split::<u8>(&[], space).is_empty());
        assert!(split(&[(1u8, 0.0)], space).is_empty());
        assert_eq!(split(&[(1u8, 5.0)], space), vec![(1u8, space)]);
    }

    /// The state helper keeps the tree between layouts and forgets it on reset.
    #[test]
    fn the_state_keeps_its_cuts_and_forgets_them_on_reset() {
        let space = Rect::new(0.0, 0.0, 600.0, 300.0);
        let items: Vec<(usize, f64)> = (1..=12).map(|i| (i, 1000.0 / i as f64)).collect();
        let mut state = TreemapState::default();
        let first = state.layout(&items, space);
        let second = state.layout(&items, space);
        assert_eq!(first, second);
        state.reset();
        let rebuilt = state.layout(&items, space);
        assert_eq!(first, rebuilt);
    }
}
