//! `arcade-tech` — tech trees for deterministic lockstep games.
//!
//! A game declares its tree once, as a static list of [`Node`]s, and
//! [`Tree::new`] checks it: unique, well-formed ids, prerequisites that exist,
//! and no cycles. Then the tree answers, for one player at one moment
//! ([`View`]), what each node is to them ([`Status`]) and what building it
//! would do ([`Tree::plan`]). The game applies the plan to its own state.
//!
//! Three kinds of node ([`Scope`]):
//!
//! - **Recipe**: personal, built again and again, making something each time
//!   (a gadget, a delivery). Unlocked, for what needs it, by building it once.
//! - **Research**: personal, built once, making nothing but the unlock.
//! - **Goal**: the whole server's. Its cost is a target everyone contributes
//!   towards from what they carry; once filled, it is unlocked for everyone,
//!   for good. The big communal builds.
//!
//! A node needs ([`Need`]) other nodes (a recipe built, research done, a goal
//! filled) and a score (a rank, a level). So trees branch as far as a game
//! likes, and a server's goals gate what each player can reach.
//!
//! Costs are per material, in a game's own fixed order (`N` of them): plain
//! arrays, so any game's material kinds fit without this crate knowing them.
//! Everything here is deterministic and allocation-free on the paths a sim
//! steps ([`Tree::plan`], [`Tree::needs_met`]); [`Tree::status`] allocates
//! only the list of unmet needs a panel shows.

use std::fmt;

/// The most nodes a tree holds: what an [`Unlocks`] can mark.
pub const MAX_NODES: usize = 128;

/// What kind of node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Personal, built again and again, making something each time.
    Recipe,
    /// Personal, built once: an unlock.
    Research,
    /// The server's, filled by everyone's contributions: an unlock for all.
    Goal,
}

/// Something a node needs before it can be built or contributed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Another node: a recipe built at least once, research done, or a goal
    /// filled.
    Node(&'static str),
    /// A score of at least this much: a rank, a level, whatever the game
    /// counts.
    Score(u32),
}

/// One node of a tree. `O` is what the game makes of it (a gadget, a
/// delivery's pay); this crate never looks inside.
#[derive(Clone, Debug, PartialEq)]
pub struct Node<O, const N: usize> {
    /// Stable for good: lower-case letters, digits and `_`. A game keeps
    /// what is unlocked under names made from it.
    pub id: &'static str,
    pub name: &'static str,
    /// Where it sits: a panel shows a tree a tier at a time.
    pub tier: u8,
    pub scope: Scope,
    /// Materials per build, or, for a goal, the server's whole target.
    pub cost: [u32; N],
    pub needs: &'static [Need],
    pub makes: O,
}

/// A set of a tree's nodes: those a player has unlocked, or a server has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Unlocks(u128);

impl Unlocks {
    pub const NONE: Self = Self(0);

    pub fn has(self, node: usize) -> bool {
        node < MAX_NODES && self.0 >> node & 1 == 1
    }

    pub fn with(self, node: usize) -> Self {
        assert!(node < MAX_NODES, "node {node} past MAX_NODES");
        Self(self.0 | 1 << node)
    }

    pub fn set(&mut self, node: usize) {
        *self = self.with(node);
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// The nodes in it, in index order.
    pub fn iter(self) -> impl Iterator<Item = usize> {
        (0..MAX_NODES).filter(move |n| self.has(*n))
    }

    /// As bits: node `n` is bit `n`. For a snapshot or a profile.
    pub fn bits(self) -> u128 {
        self.0
    }

    pub fn from_bits(bits: u128) -> Self {
        Self(bits)
    }
}

/// One player's side of a tree, at one moment.
#[derive(Clone, Copy, Debug)]
pub struct View<'a, const N: usize> {
    /// What they have unlocked themselves.
    pub mine: Unlocks,
    /// The goals their server has filled.
    pub server: Unlocks,
    /// Their score, for [`Need::Score`].
    pub score: u32,
    /// What they carry, per material.
    pub have: [u32; N],
    /// Each node's progress towards its target, by node index (zero for
    /// anything not a goal). Shorter than the tree reads as zero.
    pub toward: &'a [[u32; N]],
}

impl<const N: usize> View<'_, N> {
    fn toward(&self, node: usize) -> [u32; N] {
        self.toward.get(node).copied().unwrap_or([0; N])
    }
}

/// What a node is to one player now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status<const N: usize> {
    /// Not reachable yet: these needs are unmet.
    Locked { needs: Vec<Need> },
    /// Reachable, but they carry too little: short by this much. For a goal,
    /// they carry none of what it still wants.
    Short { short: [u32; N] },
    /// They can build it, or contribute to it.
    Ready,
    /// Research done, or a goal filled: nothing more to do here.
    Done,
}

/// What building a node would do, for the game to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan<const N: usize> {
    /// Materials taken from the player.
    pub take: [u32; N],
    /// The node becomes one of theirs (a first build, research).
    pub unlocks_mine: bool,
    /// A goal: this contribution fills it, unlocking it for the server.
    pub fills_goal: bool,
}

/// The next step towards a target ([`Tree::next_toward`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next<const N: usize> {
    /// Build (or give to) this node now: it is on the way, and ready.
    Build(usize),
    /// This node is next on the way, and reachable, but they carry too
    /// little: save up this much more.
    Save { node: usize, short: [u32; N] },
    /// Nothing they can build moves them closer: these needs (a score) stand
    /// in the way.
    Blocked { needs: Vec<Need> },
    /// There already: research done, a goal filled.
    Reached,
}

/// Why a tree is not a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    TooMany(usize),
    BadId(&'static str),
    Duplicate(&'static str),
    /// A need naming no node: (node, the missing id).
    Unknown(&'static str, &'static str),
    /// A node needing itself, directly or round a loop.
    Cycle(&'static str),
    /// A goal with nothing to fill.
    EmptyGoal(&'static str),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany(n) => write!(f, "{n} nodes, more than {MAX_NODES}"),
            Self::BadId(id) => write!(f, "{id:?} is not a node id"),
            Self::Duplicate(id) => write!(f, "{id:?} appears twice"),
            Self::Unknown(node, need) => write!(f, "{node:?} needs {need:?}, which is no node"),
            Self::Cycle(id) => write!(f, "{id:?} needs itself"),
            Self::EmptyGoal(id) => write!(f, "goal {id:?} has nothing to fill"),
        }
    }
}

/// A checked tree.
#[derive(Clone, Debug)]
pub struct Tree<O: 'static, const N: usize> {
    nodes: &'static [Node<O, N>],
    /// Each node's [`Need::Node`]s, resolved to indices.
    needs: Vec<Vec<usize>>,
}

impl<O, const N: usize> Tree<O, N> {
    /// Check `nodes` and resolve their needs.
    pub fn new(nodes: &'static [Node<O, N>]) -> Result<Self, TreeError> {
        if nodes.len() > MAX_NODES {
            return Err(TreeError::TooMany(nodes.len()));
        }
        for (i, node) in nodes.iter().enumerate() {
            let well_formed = !node.id.is_empty()
                && node.id.len() <= 32
                && node
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            if !well_formed {
                return Err(TreeError::BadId(node.id));
            }
            if nodes[..i].iter().any(|n| n.id == node.id) {
                return Err(TreeError::Duplicate(node.id));
            }
            if node.scope == Scope::Goal && node.cost.iter().all(|c| *c == 0) {
                return Err(TreeError::EmptyGoal(node.id));
            }
        }
        let mut needs = Vec::with_capacity(nodes.len());
        for node in nodes {
            let mut resolved = Vec::new();
            for need in node.needs {
                if let Need::Node(id) = need {
                    let at = nodes
                        .iter()
                        .position(|n| n.id == *id)
                        .ok_or(TreeError::Unknown(node.id, id))?;
                    resolved.push(at);
                }
            }
            needs.push(resolved);
        }
        // No cycles: every node reachable from its needs is not itself.
        for start in 0..nodes.len() {
            let mut seen = vec![false; nodes.len()];
            let mut stack = needs[start].clone();
            while let Some(n) = stack.pop() {
                if n == start {
                    return Err(TreeError::Cycle(nodes[start].id));
                }
                if !std::mem::replace(&mut seen[n], true) {
                    stack.extend(&needs[n]);
                }
            }
        }
        Ok(Self { nodes, needs })
    }

    pub fn nodes(&self) -> &'static [Node<O, N>] {
        self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, at: usize) -> Option<&'static Node<O, N>> {
        self.nodes.get(at)
    }

    /// A node's index by id.
    pub fn index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// The tiers in use, lowest first.
    pub fn tiers(&self) -> Vec<u8> {
        let mut tiers: Vec<u8> = self.nodes.iter().map(|n| n.tier).collect();
        tiers.sort_unstable();
        tiers.dedup();
        tiers
    }

    /// The nodes in `tier`, in the order declared.
    pub fn in_tier(&self, tier: u8) -> impl Iterator<Item = usize> + '_ {
        (0..self.nodes.len()).filter(move |i| self.nodes[*i].tier == tier)
    }

    /// The nodes `at` needs, by index.
    pub fn needs_of(&self, at: usize) -> &[usize] {
        &self.needs[at]
    }

    /// Whether `at` is unlocked for this player: theirs, or a filled goal.
    fn unlocked(&self, at: usize, view: &View<N>) -> bool {
        match self.nodes[at].scope {
            Scope::Goal => view.server.has(at),
            Scope::Recipe | Scope::Research => view.mine.has(at),
        }
    }

    /// Whether every need of `at` is met.
    pub fn needs_met(&self, at: usize, view: &View<N>) -> bool {
        let node = &self.nodes[at];
        let score_ok = node.needs.iter().all(|need| match need {
            Need::Score(s) => view.score >= *s,
            Need::Node(_) => true,
        });
        score_ok && self.needs[at].iter().all(|n| self.unlocked(*n, view))
    }

    /// What `at` is to this player now.
    pub fn status(&self, at: usize, view: &View<N>) -> Status<N> {
        let node = &self.nodes[at];
        let done = match node.scope {
            Scope::Research => view.mine.has(at),
            Scope::Goal => view.server.has(at),
            Scope::Recipe => false,
        };
        if done {
            return Status::Done;
        }
        if !self.needs_met(at, view) {
            let needs = node
                .needs
                .iter()
                .filter(|need| match need {
                    Need::Score(s) => view.score < *s,
                    Need::Node(id) => self.index(id).is_some_and(|n| !self.unlocked(n, view)),
                })
                .copied()
                .collect();
            return Status::Locked { needs };
        }
        match node.scope {
            Scope::Goal => {
                let wanted = remaining(node.cost, view.toward(at));
                if (0..N).any(|m| wanted[m] > 0 && view.have[m] > 0) {
                    Status::Ready
                } else {
                    Status::Short { short: wanted }
                }
            }
            Scope::Recipe | Scope::Research => {
                let short: [u32; N] =
                    std::array::from_fn(|m| node.cost[m].saturating_sub(view.have[m]));
                if short.iter().all(|s| *s == 0) {
                    Status::Ready
                } else {
                    Status::Short { short }
                }
            }
        }
    }

    /// What building (or contributing to) `at` would do now, or `None` if it
    /// cannot: locked, too little carried, done.
    pub fn plan(&self, at: usize, view: &View<N>) -> Option<Plan<N>> {
        let node = self.nodes.get(at)?;
        if self.status(at, view) != Status::Ready {
            return None;
        }
        Some(match node.scope {
            Scope::Recipe => Plan {
                take: node.cost,
                unlocks_mine: !view.mine.has(at),
                fills_goal: false,
            },
            Scope::Research => Plan {
                take: node.cost,
                unlocks_mine: true,
                fills_goal: false,
            },
            Scope::Goal => {
                let wanted = remaining(node.cost, view.toward(at));
                let take: [u32; N] = std::array::from_fn(|m| wanted[m].min(view.have[m]));
                Plan {
                    take,
                    unlocks_mine: false,
                    fills_goal: (0..N).all(|m| take[m] == wanted[m]),
                }
            }
        })
    }

    /// The next step towards `target` for this player: what to build (or
    /// give to) now, what to save up for, or what stands in the way that no
    /// building can move (a score). The *target* is the player's own choice
    /// (a bot's ambition, a person's wish); the tree only says how to get
    /// there.
    ///
    /// Unmet prerequisites are worked through cheapest first (by what they
    /// still cost, then by index), depth first, so the step is always one a
    /// player can act on: a leaf whose own needs are met. A recipe target is
    /// never [`Next::Reached`], since it is built again and again; research
    /// done and a goal filled are.
    pub fn next_toward(&self, target: usize, view: &View<N>) -> Next<N> {
        if target >= self.nodes.len() {
            return Next::Blocked { needs: Vec::new() };
        }
        let done = match self.nodes[target].scope {
            Scope::Research => view.mine.has(target),
            Scope::Goal => view.server.has(target),
            Scope::Recipe => false,
        };
        if done {
            return Next::Reached;
        }
        self.step_toward(target, view)
    }

    fn step_toward(&self, at: usize, view: &View<N>) -> Next<N> {
        let node = &self.nodes[at];
        let short_score: Vec<Need> = node
            .needs
            .iter()
            .filter(|n| matches!(n, Need::Score(s) if view.score < *s))
            .copied()
            .collect();
        if !short_score.is_empty() {
            return Next::Blocked { needs: short_score };
        }
        let mut unmet: Vec<usize> = self.needs[at]
            .iter()
            .copied()
            .filter(|n| !self.unlocked(*n, view))
            .collect();
        if !unmet.is_empty() {
            unmet.sort_by_key(|n| (self.still_costs(*n, view), *n));
            let mut blocked = Vec::new();
            for n in unmet {
                match self.step_toward(n, view) {
                    Next::Blocked { needs } => blocked.extend(needs),
                    next => return next,
                }
            }
            return Next::Blocked { needs: blocked };
        }
        match self.status(at, view) {
            Status::Ready => Next::Build(at),
            Status::Short { short } => Next::Save { node: at, short },
            Status::Done => Next::Reached,
            // Needs met above, so never locked; say so rather than panic.
            Status::Locked { needs } => Next::Blocked { needs },
        }
    }

    /// What a node still costs: its cost, or what a goal still wants.
    fn still_costs(&self, at: usize, view: &View<N>) -> u64 {
        let node = &self.nodes[at];
        let left = match node.scope {
            Scope::Goal => remaining(node.cost, view.toward(at)),
            Scope::Recipe | Scope::Research => node.cost,
        };
        left.iter().map(|c| u64::from(*c)).sum()
    }

    /// Every need between nodes, as (needed, needer): the lines a panel draws.
    pub fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.needs
            .iter()
            .enumerate()
            .flat_map(|(to, from)| from.iter().map(move |f| (*f, to)))
    }
}

/// What a goal still wants: its target less what it has.
pub fn remaining<const N: usize>(cost: [u32; N], toward: [u32; N]) -> [u32; N] {
    std::array::from_fn(|m| cost[m].saturating_sub(toward[m]))
}

#[cfg(test)]
mod tests;
