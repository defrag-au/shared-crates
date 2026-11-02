//! The nine things a transaction can physically do.
//!
//! Everything else — a sale, a listing, a swap, an approval — is a reading
//! derived from a combination of these. A new marketplace, venue or event kind
//! maps to existing primitives; it does not add a variant here.

use serde::{Deserialize, Serialize};

use crate::model::{Actor, Basis, Claim, Custody, Figure, Object, Visibility};

/// One primitive action, with the two facts that belong to every action rather
/// than to its shape: how firmly it is known, and whether it is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Primitive {
    #[serde(default)]
    pub basis: Basis,
    #[serde(default)]
    pub visibility: Visibility,
    pub action: Action,
}

impl Primitive {
    pub fn transfer(object: Object, from: Actor, to: Actor) -> Self {
        Self::new(Action::Move {
            object,
            from,
            to,
            custody: Custody::InHand,
        })
    }

    pub fn transfer_with(object: Object, from: Actor, to: Actor, custody: Custody) -> Self {
        Self::new(Action::Move {
            object,
            from,
            to,
            custody,
        })
    }

    /// Assets coming into existence.
    pub fn create(object: Object, to: Actor) -> Self {
        Self::new(Action::Create { object, to })
    }

    /// Assets leaving existence.
    pub fn destroy(object: Object, from: Actor) -> Self {
        Self::new(Action::Destroy { object, from })
    }

    /// A custody change in place: a listing, a delist, a lock, a stake.
    ///
    /// This is deliberately not a [`Primitive::transfer`]. A listing moves the
    /// UTxO to a marketplace script but does not move ownership, and rendering
    /// it as a transfer is how the marketplace became the largest holder.
    pub fn hold(object: Object, holder: Actor, from: Custody, to: Custody) -> Self {
        Self::new(Action::Hold {
            object,
            holder,
            from,
            to,
        })
    }

    /// An intention: an ask, a bid, an auction.
    pub fn declare(claim: Claim, figure: Figure) -> Self {
        Self::new(Action::Declare { claim, figure })
    }

    /// An intention withdrawn.
    ///
    /// `figure` is the last known price, where the source still carries it —
    /// a cancellation often does. It is not invented when absent.
    pub fn retract(claim: Claim, figure: Option<Figure>) -> Self {
        Self::new(Action::Retract { claim, figure })
    }

    /// An intention changed: a price change, an updated offer.
    pub fn revise(claim: Claim, from: Option<Figure>, to: Figure) -> Self {
        Self::new(Action::Revise { claim, from, to })
    }

    /// A permission granted: an ERC-20 allowance, an ERC-721 operator.
    pub fn grant(object: Object, owner: Actor, spender: Actor) -> Self {
        Self::new(Action::Grant {
            object,
            owner,
            spender,
        })
    }

    /// A permission withdrawn.
    pub fn revoke_grant(object: Object, owner: Actor, spender: Actor) -> Self {
        Self::new(Action::RevokeGrant {
            object,
            owner,
            spender,
        })
    }

    pub fn with_basis(mut self, basis: Basis) -> Self {
        self.basis = basis;
        self
    }

    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// The object this primitive is about, where it has one.
    pub fn object(&self) -> Option<&Object> {
        match &self.action {
            Action::Move { object, .. }
            | Action::Create { object, .. }
            | Action::Destroy { object, .. }
            | Action::Hold { object, .. }
            | Action::Grant { object, .. }
            | Action::RevokeGrant { object, .. } => Some(object),
            Action::Declare { claim, .. }
            | Action::Retract { claim, .. }
            | Action::Revise { claim, .. } => Some(&claim.object),
        }
    }

    /// The party this primitive is chiefly about, where it has one.
    pub fn actor(&self) -> Option<&Actor> {
        match &self.action {
            Action::Move { from, .. } => Some(from),
            Action::Create { to, .. } => Some(to),
            Action::Destroy { from, .. } => Some(from),
            Action::Hold { holder, .. } => Some(holder),
            Action::Declare { claim, .. }
            | Action::Retract { claim, .. }
            | Action::Revise { claim, .. } => Some(&claim.actor),
            Action::Grant { spender, .. } | Action::RevokeGrant { spender, .. } => Some(spender),
        }
    }

    fn new(action: Action) -> Self {
        Self {
            basis: Basis::Observed,
            visibility: Visibility::Public,
            action,
        }
    }
}

/// The physical shape of a primitive. Kept apart from [`Primitive`] so every
/// action carries a basis and a visibility without repeating those fields nine
/// times.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Move {
        object: Object,
        from: Actor,
        to: Actor,
        #[serde(default)]
        custody: Custody,
    },
    Create {
        object: Object,
        to: Actor,
    },
    Destroy {
        object: Object,
        from: Actor,
    },
    Hold {
        object: Object,
        holder: Actor,
        from: Custody,
        to: Custody,
    },
    Declare {
        claim: Claim,
        figure: Figure,
    },
    Retract {
        claim: Claim,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        figure: Option<Figure>,
    },
    Revise {
        claim: Claim,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Figure>,
        to: Figure,
    },
    Grant {
        object: Object,
        owner: Actor,
        spender: Actor,
    },
    RevokeGrant {
        object: Object,
        owner: Actor,
        spender: Actor,
    },
}
