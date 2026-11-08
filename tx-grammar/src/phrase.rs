//! The sequence and its forward projection into cells.
//!
//! A [`Step`] is one transaction's primitives. A [`Sequence`] is one or more
//! steps, linked when two transactions are one economic event. A [`Phrase`] is
//! the projection a renderer draws: readings, cells, facets and reconciliation.

use chains::ChainRef;
use serde::{Deserialize, Serialize};

use crate::model::{
    Absence, Actor, Asset, BalanceDelta, Basis, Claim, ClaimKind, Custody, Direction, Facet,
    Figure, FigureKind, Mechanism, Object, Outcome, PartyKey, Per, Visibility,
};
use crate::primitive::{Action, Primitive};
use crate::reading::Reading;

/// Whose side the sequence is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Viewpoint {
    /// One wallet's own side: the only viewpoint in which `Role::Own` appears.
    Wallet { who: PartyKey },
    /// Two parties and no owner: a policy watching a unit move.
    Pair,
    /// A chain-level reading with no party.
    Chain,
}

impl Viewpoint {
    /// The party a cell is told from.
    ///
    /// From a wallet's side that is always the other party, because the cell is
    /// about what the wallet did. From a pair it is the party the movement
    /// started at, so the cell can draw `actor → other`.
    pub fn actor_for(&self, from: &Actor, to: &Actor) -> Actor {
        match self {
            Viewpoint::Pair | Viewpoint::Chain => from.clone(),
            Viewpoint::Wallet { .. } => {
                if to.is_own() {
                    from.clone()
                } else {
                    to.clone()
                }
            }
        }
    }

    /// The second side, present only when the phrase has no owner.
    ///
    /// A policy row is a pair, so both ends of a movement are facts the reader
    /// needs. A wallet row has an own side, and naming it twice says nothing.
    pub fn other_for(&self, from: &Actor, to: &Actor) -> Option<Actor> {
        match self {
            Viewpoint::Pair | Viewpoint::Chain if from != to => Some(to.clone()),
            _ => None,
        }
    }
}

/// One transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub chain: ChainRef,
    pub tx: String,
    pub when: i64,
    #[serde(default)]
    pub outcome: Outcome,
    pub primitives: Vec<Primitive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<Facet>,
}

impl Step {
    pub fn new(chain: ChainRef, tx: impl Into<String>, when: i64) -> Self {
        Self {
            chain,
            tx: tx.into(),
            when,
            outcome: Outcome::Succeeded,
            primitives: Vec::new(),
            facets: Vec::new(),
        }
    }

    pub fn primitive(mut self, primitive: Primitive) -> Self {
        self.primitives.push(primitive);
        self
    }

    pub fn facet(mut self, facet: Facet) -> Self {
        self.facets.push(facet);
        self
    }

    pub fn outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn has_mechanism(&self, mechanism: Mechanism) -> bool {
        self.facets.iter().any(|facet| {
            matches!(
                facet,
                Facet::Mechanism {
                    mechanism: found
                } if *found == mechanism
            )
        })
    }

    /// The most private visibility any primitive in the step has.
    pub fn visibility(&self) -> Visibility {
        if self.primitives.is_empty() {
            return Visibility::Unknown;
        }
        self.primitives
            .iter()
            .fold(Visibility::Public, |found, primitive| {
                found.combine(primitive.visibility)
            })
    }

    /// The cells a renderer draws, derived from the primitives.
    pub fn cells(&self, viewpoint: &Viewpoint) -> Vec<Cell> {
        let mut used = vec![false; self.primitives.len()];
        let mut cells = Vec::new();

        // 1. Permissions. A grant is neither a movement nor a claim.
        for (i, primitive) in self.primitives.iter().enumerate() {
            if used[i] {
                continue;
            }
            match &primitive.action {
                Action::Grant {
                    object, spender, ..
                } => {
                    let cell = cell_for(
                        self,
                        primitive,
                        Reading::Approve,
                        object.clone(),
                        spender.clone(),
                        Direction::Unchanged,
                        Custody::InHand,
                        None,
                        None,
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                Action::RevokeGrant {
                    object, spender, ..
                } => {
                    let cell = cell_for(
                        self,
                        primitive,
                        Reading::RevokeApproval,
                        object.clone(),
                        spender.clone(),
                        Direction::Unchanged,
                        Custody::InHand,
                        None,
                        None,
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                _ => {}
            }
        }

        // 2. Creation and destruction.
        for (i, primitive) in self.primitives.iter().enumerate() {
            if used[i] {
                continue;
            }
            match &primitive.action {
                Action::Create { object, to } => {
                    let direction = if to.is_own() {
                        Direction::In
                    } else {
                        Direction::Both
                    };
                    let cell = cell_for(
                        self,
                        primitive,
                        Reading::Mint,
                        object.clone(),
                        to.clone(),
                        direction,
                        Custody::InHand,
                        None,
                        None,
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                Action::Destroy { object, from } => {
                    let direction = if from.is_own() {
                        Direction::Out
                    } else {
                        Direction::Both
                    };
                    let cell = cell_for(
                        self,
                        primitive,
                        Reading::Burn,
                        object.clone(),
                        from.clone(),
                        direction,
                        Custody::InHand,
                        None,
                        None,
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                _ => {}
            }
        }

        // 3. Movements. A pair in opposite directions is a settlement or a
        // swap; a lone movement is a transfer.
        for i in 0..self.primitives.len() {
            if used[i] {
                continue;
            }
            let primitive = &self.primitives[i];
            let Action::Move {
                object: first_object,
                from: first_from,
                to: first_to,
                custody: first_custody,
            } = &primitive.action
            else {
                continue;
            };

            let Some(j) = find_move_pair(self, &used, i) else {
                let reading = if first_from.is_own() && first_to.is_own() {
                    Reading::SelfTransfer
                } else {
                    Reading::Transfer
                };
                let direction = if first_to.is_own() {
                    Direction::In
                } else if first_from.is_own() {
                    Direction::Out
                } else {
                    Direction::Both
                };
                let actor = viewpoint.actor_for(first_from, first_to);
                let other = viewpoint.other_for(first_from, first_to);
                let cell = cell_for(
                    self,
                    primitive,
                    reading,
                    first_object.clone(),
                    actor,
                    direction,
                    *first_custody,
                    None,
                    None,
                )
                .with_other(other);
                push(&mut cells, cell);
                used[i] = true;
                continue;
            };

            let second = &self.primitives[j];
            let Action::Move {
                object: second_object,
                from: second_from,
                to: second_to,
                custody: second_custody,
            } = &second.action
            else {
                continue;
            };

            let (item_object, item_from, item_to, item_custody, item_primitive, money_object) =
                match (first_object.is_currency(), second_object.is_currency()) {
                    (false, true) => (
                        first_object,
                        first_from,
                        first_to,
                        *first_custody,
                        primitive,
                        second_object,
                    ),
                    (true, false) => (
                        second_object,
                        second_from,
                        second_to,
                        *second_custody,
                        second,
                        first_object,
                    ),
                    _ => (
                        first_object,
                        first_from,
                        first_to,
                        *first_custody,
                        primitive,
                        second_object,
                    ),
                };

            let both_currency = first_object.is_currency() == second_object.is_currency();
            if both_currency {
                // A swap, a wrap, or an exchange of two currencies. The object
                // shown is the side that arrived; the other side is its value.
                let reading = if self.has_mechanism(Mechanism::Wrap) {
                    Reading::Wrap
                } else {
                    Reading::Swap
                };
                let direction = if item_to.is_own() {
                    Direction::In
                } else if item_from.is_own() {
                    Direction::Out
                } else {
                    Direction::Both
                };
                let actor = viewpoint.actor_for(item_from, item_to);
                let other = viewpoint.other_for(item_from, item_to);
                let cell = cell_for(
                    self,
                    item_primitive,
                    reading,
                    item_object.clone(),
                    actor,
                    direction,
                    item_custody,
                    None,
                    None,
                )
                .with_other(other)
                .with_visibility(primitive.visibility.combine(second.visibility));
                push(&mut cells, cell);
                used[i] = true;
                used[j] = true;
                continue;
            }

            let Some(money) = money_asset(money_object) else {
                // A hidden or collection object cannot carry a price; leave the
                // pair for the leftover pass rather than invent one.
                continue;
            };

            let accepted = find_claim(self, &used, item_object, true);
            let reading = if accepted.is_some() {
                Reading::Accept
            } else {
                Reading::Sale
            };
            let direction = if item_to.is_own() {
                Direction::In
            } else if item_from.is_own() {
                Direction::Out
            } else {
                Direction::Both
            };
            let actor = viewpoint.actor_for(item_from, item_to);
            let other = viewpoint.other_for(item_from, item_to);
            let cell = cell_for(
                self,
                item_primitive,
                reading,
                item_object.clone(),
                actor,
                direction,
                item_custody,
                Some(Figure::paid(money).per(if item_object.distinct() > 1 {
                    Per::Lot
                } else {
                    Per::Item
                })),
                None,
            )
            .with_other(other)
            .with_visibility(primitive.visibility.combine(second.visibility));
            push(&mut cells, cell);
            used[i] = true;
            used[j] = true;
            if let Some(k) = accepted {
                used[k] = true;
            }
        }

        // 4. Custody plus an intention: a listing, an offer, a delist.
        for (i, primitive) in self.primitives.iter().enumerate() {
            if used[i] {
                continue;
            }
            let Action::Hold {
                object,
                holder,
                from,
                to,
            } = &primitive.action
            else {
                continue;
            };

            if let Some(j) = find_claim(self, &used, object, false)
                && let Action::Declare { claim, figure } = &self.primitives[j].action
            {
                let reading = match (to, claim.kind) {
                    (Custody::Escrow, ClaimKind::Ask | ClaimKind::Auction) => Reading::Listing,
                    (Custody::Escrow, ClaimKind::Bid | ClaimKind::CollectionBid) => Reading::Offer,
                    (Custody::Script, _) if self.has_mechanism(Mechanism::Vesting) => Reading::Vest,
                    (Custody::Script, _) => Reading::Lock,
                    (Custody::Staked, _) => Reading::Stake,
                    _ => Reading::Unknown,
                };
                let cell = cell_for(
                    self,
                    primitive,
                    reading,
                    claim.object.clone(),
                    claim.actor.clone(),
                    Direction::InPlace,
                    *to,
                    Some(figure.clone()),
                    absence_for_claim(claim),
                )
                .with_visibility(primitive.visibility.combine(self.primitives[j].visibility));
                push(&mut cells, cell);
                used[i] = true;
                used[j] = true;
                continue;
            }

            if let Some(j) = find_claim(self, &used, object, true)
                && let Action::Retract { claim, figure } = &self.primitives[j].action
            {
                let reading = match claim.kind {
                    ClaimKind::Ask => Reading::Delist,
                    ClaimKind::Bid | ClaimKind::CollectionBid => Reading::CancelOffer,
                    ClaimKind::Auction => Reading::Cancel,
                    ClaimKind::Unknown => Reading::Cancel,
                };
                let cell = cell_for(
                    self,
                    primitive,
                    reading,
                    claim.object.clone(),
                    claim.actor.clone(),
                    Direction::InPlace,
                    *to,
                    figure.clone(),
                    absence_for_claim(claim),
                )
                .with_visibility(primitive.visibility.combine(self.primitives[j].visibility));
                push(&mut cells, cell);
                used[i] = true;
                used[j] = true;
                continue;
            }

            let reading = match (from, to) {
                (Custody::InHand, Custody::Script) if self.has_mechanism(Mechanism::Vesting) => {
                    Reading::Vest
                }
                (Custody::InHand, Custody::Script) => Reading::Lock,
                (Custody::Script, Custody::InHand) => Reading::Release,
                (Custody::InHand, Custody::Staked) => Reading::Stake,
                (Custody::Staked, Custody::InHand) => Reading::Unstake,
                (Custody::InHand, Custody::Escrow) if !object.is_currency() => Reading::Listing,
                (Custody::InHand, Custody::Escrow) => Reading::Offer,
                (Custody::Escrow, Custody::InHand) => Reading::Release,
                _ => Reading::Unknown,
            };
            let cell = cell_for(
                self,
                primitive,
                reading,
                object.clone(),
                holder.clone(),
                Direction::InPlace,
                *to,
                None,
                None,
            );
            push(&mut cells, cell);
            used[i] = true;
        }

        // 5. Intentions with no custody change of their own.
        for (i, primitive) in self.primitives.iter().enumerate() {
            if used[i] {
                continue;
            }
            match &primitive.action {
                Action::Declare { claim, figure } => {
                    let reading = match claim.kind {
                        ClaimKind::Ask | ClaimKind::Auction => Reading::Listing,
                        ClaimKind::Bid | ClaimKind::CollectionBid => Reading::Offer,
                        ClaimKind::Unknown => Reading::Unknown,
                    };
                    let cell = cell_for(
                        self,
                        primitive,
                        reading,
                        claim.object.clone(),
                        claim.actor.clone(),
                        Direction::InPlace,
                        Custody::InHand,
                        Some(figure.clone()),
                        absence_for_claim(claim),
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                Action::Retract { claim, figure } => {
                    let reading = match claim.kind {
                        ClaimKind::Ask => Reading::Delist,
                        ClaimKind::Bid | ClaimKind::CollectionBid => Reading::CancelOffer,
                        ClaimKind::Auction | ClaimKind::Unknown => Reading::Cancel,
                    };
                    let cell = cell_for(
                        self,
                        primitive,
                        reading,
                        claim.object.clone(),
                        claim.actor.clone(),
                        Direction::Unchanged,
                        Custody::InHand,
                        figure.clone(),
                        absence_for_claim(claim),
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                Action::Revise { claim, to, .. } => {
                    let cell = cell_for(
                        self,
                        primitive,
                        Reading::PriceChange,
                        claim.object.clone(),
                        claim.actor.clone(),
                        Direction::Unchanged,
                        Custody::InHand,
                        Some(to.clone()),
                        None,
                    );
                    push(&mut cells, cell);
                    used[i] = true;
                }
                _ => {}
            }
        }

        // 6. Anything left is said to be unknown rather than given a shape it
        // does not have.
        for (i, primitive) in self.primitives.iter().enumerate() {
            if used[i] {
                continue;
            }
            let object = primitive
                .object()
                .cloned()
                .unwrap_or_else(|| Object::hidden(Visibility::Unknown));
            let actor = primitive.actor().cloned().unwrap_or_else(Actor::unknown);
            let cell = cell_for(
                self,
                primitive,
                Reading::Unknown,
                object,
                actor,
                Direction::Unchanged,
                Custody::InHand,
                None,
                Some(Absence::Unknown),
            );
            push(&mut cells, cell);
            used[i] = true;
        }

        cells
    }
}

#[allow(clippy::too_many_arguments)]
fn cell_for(
    step: &Step,
    primitive: &Primitive,
    reading: Reading,
    object: Object,
    actor: Actor,
    direction: Direction,
    custody: Custody,
    figure: Option<Figure>,
    absence: Option<Absence>,
) -> Cell {
    Cell::new(reading, step.chain, object)
        .with_figure(figure)
        .with_actor(actor)
        .with_direction(direction)
        .with_custody(custody)
        .with_basis(primitive.basis)
        .with_outcome(step.outcome)
        .with_visibility(primitive.visibility)
        .with_absence(absence)
}

fn find_move_pair(step: &Step, used: &[bool], i: usize) -> Option<usize> {
    let first = &step.primitives[i].action;
    (i + 1..step.primitives.len())
        .find(|&j| !used[j] && opposite_moves(first, &step.primitives[j].action))
}

fn opposite_moves(first: &Action, second: &Action) -> bool {
    match (first, second) {
        (
            Action::Move {
                object: first_object,
                from: first_from,
                to: first_to,
                ..
            },
            Action::Move {
                object: second_object,
                from: second_from,
                to: second_to,
                ..
            },
        ) => first_from == second_to && first_to == second_from && first_object != second_object,
        _ => false,
    }
}

fn find_claim(step: &Step, used: &[bool], object: &Object, retract: bool) -> Option<usize> {
    step.primitives
        .iter()
        .enumerate()
        .find_map(|(j, primitive)| {
            if used[j] || primitive.object() != Some(object) {
                return None;
            }
            match (&primitive.action, retract) {
                (Action::Declare { .. }, false) | (Action::Retract { .. }, true) => Some(j),
                _ => None,
            }
        })
}

fn money_asset(object: &Object) -> Option<Asset> {
    match object {
        Object::Asset(asset) => Some(asset.clone()),
        Object::Lot { items, total, .. } => {
            let mut first = items.first()?.clone();
            first.quantity = *total;
            Some(first)
        }
        Object::Collection(_) | Object::Hidden { .. } => None,
    }
}

fn absence_for_claim(claim: &Claim) -> Option<Absence> {
    (claim.kind == ClaimKind::Unknown).then_some(Absence::Unknown)
}

/// Consecutive identical cells merge into one count. This is what lets a batch
/// of three identical listings read as `list ×3` without the producer inventing
/// a synthetic lot.
fn push(cells: &mut Vec<Cell>, cell: Cell) {
    if let Some(last) = cells.last_mut()
        && last.reading == cell.reading
        && last.object == cell.object
        && last.actor == cell.actor
        && last.other == cell.other
        && last.direction == cell.direction
        && last.custody == cell.custody
        && last.figure == cell.figure
    {
        last.count = last.count.saturating_add(cell.count);
        last.more = last.more.saturating_add(cell.more);
        return;
    }
    cells.push(cell);
}

/// What a layer of the grammar is about: a whole transaction, an economic event
/// across transactions, or a bridge across chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Composition {
    WithinTransaction,
    AcrossTransactions,
    AcrossChains,
}

/// Why two steps are one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    SharedParty { key: PartyKey },
    TimeGap { secs: i64 },
    Carrier { asset: Asset },
    Decoded { venue: String, kind: String },
    Trace { source: String },
    Proof { circuit: String },
    Receipt { outcome: Outcome },
}

/// A link between two steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub kind: LinkKind,
    pub from: usize,
    pub to: usize,
    pub basis: Basis,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkKind {
    EconomicEvent { gap_secs: i64 },
    CrossChain { mechanism: String },
}

impl Link {
    pub fn economic_event(
        from: usize,
        to: usize,
        gap_secs: i64,
        basis: Basis,
        evidence: Vec<Evidence>,
    ) -> Self {
        Self {
            kind: LinkKind::EconomicEvent { gap_secs },
            from,
            to,
            basis,
            evidence,
        }
    }

    pub fn cross_chain(from: usize, to: usize, mechanism: impl Into<String>, basis: Basis) -> Self {
        Self {
            kind: LinkKind::CrossChain {
                mechanism: mechanism.into(),
            },
            from,
            to,
            basis,
            evidence: Vec::new(),
        }
    }
}

/// The semantic model: what a producer builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub viewpoint: Viewpoint,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reconciliation: Vec<BalanceDelta>,
}

impl Sequence {
    pub fn new(viewpoint: Viewpoint) -> Self {
        Self {
            viewpoint,
            steps: Vec::new(),
            links: Vec::new(),
            reconciliation: Vec::new(),
        }
    }

    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    pub fn link(mut self, link: Link) -> Self {
        self.links.push(link);
        self
    }

    pub fn reconcile(mut self, delta: BalanceDelta) -> Self {
        self.reconciliation.push(delta);
        self
    }

    pub fn phrase(&self) -> Phrase {
        let composition = if self
            .links
            .iter()
            .any(|link| matches!(link.kind, LinkKind::CrossChain { .. }))
        {
            Composition::AcrossChains
        } else if self.steps.len() > 1 {
            Composition::AcrossTransactions
        } else {
            Composition::WithinTransaction
        };

        let mut facets = Vec::new();
        for step in &self.steps {
            for facet in &step.facets {
                if !facets.contains(facet) {
                    facets.push(facet.clone());
                }
            }
            let chain_facet = Facet::chain(step.chain);
            if !facets.contains(&chain_facet) {
                facets.push(chain_facet);
            }
        }

        let steps = self
            .steps
            .iter()
            .map(|step| PhraseStep {
                chain: step.chain,
                tx: step.tx.clone(),
                when: step.when,
                outcome: step.outcome,
                visibility: step.visibility(),
                cells: step.cells(&self.viewpoint),
            })
            .collect();

        Phrase {
            viewpoint: self.viewpoint.clone(),
            steps,
            composition,
            facets,
            reconciliation: self.reconciliation.clone(),
        }
    }
}

/// One step of a [`Phrase`], already projected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhraseStep {
    pub chain: ChainRef,
    pub tx: String,
    pub when: i64,
    pub outcome: Outcome,
    pub visibility: Visibility,
    pub cells: Vec<Cell>,
}

/// The forward projection a renderer draws. No strings to parse, no policy
/// decisions, no colours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phrase {
    pub viewpoint: Viewpoint,
    pub steps: Vec<PhraseStep>,
    pub composition: Composition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<Facet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reconciliation: Vec<BalanceDelta>,
}

impl Phrase {
    /// The readings in draw order.
    pub fn readings(&self) -> Vec<Reading> {
        self.steps
            .iter()
            .flat_map(|step| step.cells.iter().map(|cell| cell.reading))
            .collect()
    }

    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.steps.iter().flat_map(|step| step.cells.iter())
    }

    /// Every chain the phrase touches, in first-seen order.
    pub fn chains(&self) -> Vec<ChainRef> {
        let mut chains = Vec::new();
        for step in &self.steps {
            if !chains.contains(&step.chain) {
                chains.push(step.chain);
            }
        }
        chains
    }

    pub fn is_multi_chain(&self) -> bool {
        self.chains().len() > 1
    }

    /// The canonical compact token line.
    pub fn tokens(&self) -> String {
        crate::text::tokens(self)
    }

    /// The canonical sentence — the text-only interface, and the test oracle
    /// for the visual renderers.
    pub fn sentence(&self) -> String {
        crate::text::sentence(self)
    }
}

/// One action as a renderer sees it.
///
/// The fields are semantic. A renderer maps `reading` to its glyph, `direction`
/// to an arrow shape, `custody` to a ring, `basis` to a fill, `outcome` to a
/// failed mark and `visibility` to a shield. None of that is here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub reading: Reading,
    pub chain: ChainRef,
    pub object: Object,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<Figure>,
    pub actor: Actor,
    /// The far end of a movement, when the phrase has no owner to stand on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<Actor>,
    pub direction: Direction,
    pub custody: Custody,
    pub basis: Basis,
    pub outcome: Outcome,
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absence: Option<Absence>,
    pub count: u32,
    #[serde(default)]
    pub more: u32,
}

impl Cell {
    pub fn new(reading: Reading, chain: ChainRef, object: Object) -> Self {
        Self {
            reading,
            chain,
            object,
            figure: None,
            actor: Actor::unknown(),
            other: None,
            direction: Direction::Unchanged,
            custody: Custody::InHand,
            basis: Basis::Observed,
            outcome: Outcome::Unknown,
            visibility: Visibility::Public,
            absence: None,
            count: 1,
            more: 0,
        }
    }

    /// The one word the cell draws beside its glyph.
    pub fn label(&self) -> &'static str {
        self.reading.label()
    }

    pub fn with_figure(mut self, figure: Option<Figure>) -> Self {
        self.figure = figure;
        self
    }

    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = actor;
        self
    }

    pub fn with_other(mut self, other: Option<Actor>) -> Self {
        self.other = other;
        self
    }

    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    pub fn with_custody(mut self, custody: Custody) -> Self {
        self.custody = custody;
        self
    }

    pub fn with_basis(mut self, basis: Basis) -> Self {
        self.basis = basis;
        self
    }

    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_absence(mut self, absence: Option<Absence>) -> Self {
        self.absence = absence;
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    pub fn with_more(mut self, more: u32) -> Self {
        self.more = more;
        self
    }

    /// A figure is an asking price when the cell is a claim, not a settlement.
    pub fn asked(&self) -> Option<&Figure> {
        self.figure
            .as_ref()
            .filter(|figure| figure.kind == FigureKind::Ask)
    }
}
