//! The two text projections: the compact token line and the sentence.
//!
//! This is the ONLY module in the crate that writes prose. The visual renderers
//! never build a sentence; they draw [`Cell`](crate::phrase::Cell)s. Keeping
//! prose here is what stops the model turning back into `TxType::Display`.

use crate::model::{
    Absence, Actor, Asset, Custody, Direction, Figure, Hidden, Object, Outcome, Party, Quantity,
    Role, Visibility,
};
use crate::phrase::{Cell, Composition, Phrase, PhraseStep};

/// The canonical compact line: readings, objects, figures and actors, with no
/// sentence grammar. This is what a log, a capture note or a debug view shows.
pub fn tokens(phrase: &Phrase) -> String {
    if phrase.steps.is_empty() {
        return String::new();
    }
    phrase
        .steps
        .iter()
        .filter_map(|step| {
            let body = step
                .cells
                .iter()
                .map(token_cell)
                .collect::<Vec<_>>()
                .join(" ∥ ");
            if body.is_empty() {
                return None;
            }
            let body = match step.outcome {
                Outcome::Reverted => format!("reverted: {body}"),
                Outcome::Failed => format!("failed: {body}"),
                Outcome::Dropped => format!("dropped: {body}"),
                _ => body,
            };
            Some(if phrase.is_multi_chain() {
                format!("[{}] {body}", step.chain.as_caip2())
            } else {
                body
            })
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

/// The text-only interface: one or more sentences generated from the same
/// cells. This is the accessible rendering, the social-card body's source, and
/// the oracle the visual renderers are compared against.
pub fn sentence(phrase: &Phrase) -> String {
    let mut out = String::new();
    match phrase.composition {
        Composition::AcrossTransactions if phrase.steps.len() > 1 => {
            out.push_str(&format!(
                "One event across {} transactions. ",
                phrase.steps.len()
            ));
        }
        Composition::AcrossChains => {
            out.push_str(&format!(
                "One event across {} chains. ",
                phrase.chains().len()
            ));
        }
        _ => {}
    }
    for step in &phrase.steps {
        out.push_str(&sentence_step(step));
        out.push(' ');
    }
    out.trim_end().to_string()
}

fn sentence_step(step: &PhraseStep) -> String {
    let chain = step.chain.as_caip2();
    let attempted = step.outcome.is_failure();
    let mut sentences = step
        .cells
        .iter()
        .map(sentence_cell)
        .collect::<Vec<_>>()
        .join(" ");
    if sentences.is_empty() {
        sentences = "No action was recorded.".to_string();
    }
    let visibility = match step.visibility {
        Visibility::Shielded => " (shielded)",
        _ => "",
    };
    if attempted {
        let word = match step.outcome {
            Outcome::Reverted => "reverted",
            Outcome::Failed => "failed",
            Outcome::Dropped => "was dropped",
            _ => "did not take effect",
        };
        format!("On {chain}{visibility}: Attempted: {sentences} The transaction {word}.")
    } else {
        format!("On {chain}{visibility}: {sentences}")
    }
}

fn token_cell(cell: &Cell) -> String {
    let mut parts = Vec::new();
    let mut label = cell.label().to_string();
    if cell.count > 1 {
        label.push_str(&format!(" ×{}", cell.count));
    }
    parts.push(label);
    parts.push(format_object(&cell.object));
    if let Some(figure) = &cell.figure {
        parts.push(format_figure(figure));
    }
    let actor = format_actor(&cell.actor);
    match &cell.other {
        Some(other) => parts.push(format!("{actor} → {}", format_actor(other))),
        None if !actor.is_empty() && actor != "you" && actor != "unknown" => parts.push(actor),
        None => {}
    }
    if cell.custody != Custody::InHand {
        parts.push(format_custody(cell.custody).to_string());
    }
    if let Some(absence) = &cell.absence {
        let note = format_absence(absence);
        if !note.is_empty() && !parts.iter().any(|part| part == note) {
            parts.push(note.to_string());
        }
    }
    if cell.visibility == Visibility::Shielded && !parts.iter().any(|part| part == "shielded") {
        parts.push("shielded".to_string());
    }
    if cell.outcome.is_failure() {
        parts.push(
            match cell.outcome {
                Outcome::Reverted => "reverted",
                Outcome::Failed => "failed",
                Outcome::Dropped => "dropped",
                _ => "ended",
            }
            .to_string(),
        );
    }
    parts.join(" · ")
}

fn sentence_cell(cell: &Cell) -> String {
    let object = sentence_object(&cell.object);
    let actor = format_actor(&cell.actor);
    let figure = cell
        .figure
        .as_ref()
        .map(|figure| format_asset(&figure.amount));
    let venue = (cell.actor.role == Role::Venue && actor != "venue").then_some(actor.as_str());
    let other = cell.other.as_ref().map(format_actor);
    let body = match cell.reading {
        crate::reading::Reading::Sale => match (other.as_deref(), cell.direction, figure) {
            (Some(other), _, Some(amount)) => {
                format!("{object} moved from {actor} to {other} for {amount}.")
            }
            (Some(other), _, None) => {
                format!("{object} moved from {actor} to {other} at an unknown price.")
            }
            (None, Direction::In, Some(amount)) => {
                format!("Bought {object} from {actor} for {amount}.")
            }
            (None, Direction::Out, Some(amount)) => {
                format!("Sold {object} to {actor} for {amount}.")
            }
            (None, _, Some(amount)) => format!("Traded {object} with {actor} for {amount}."),
            (None, _, None) => format!("Traded {object} with {actor} at an unknown price."),
        },
        crate::reading::Reading::Accept => match figure {
            Some(amount) => format!("Accepted an offer on {object} for {amount}."),
            None => format!("Accepted an offer on {object}."),
        },
        crate::reading::Reading::Listing => match (venue, figure) {
            (Some(venue), Some(amount)) => format!("Listed {object} on {venue} at {amount}."),
            (None, Some(amount)) => format!("Listed {object} at {amount}."),
            (Some(venue), None) => format!("Listed {object} on {venue} at an unknown price."),
            (None, None) => format!("Listed {object}."),
        },
        crate::reading::Reading::Offer => match figure {
            Some(amount) => format!("Offered {amount} for {object}."),
            None => format!("Made an offer on {object}."),
        },
        crate::reading::Reading::Transfer | crate::reading::Reading::Shield => {
            match (&other, cell.direction) {
                (Some(other), _) => format!("{object} moved from {actor} to {other}."),
                (None, Direction::In) => format!("Received {object} from {actor}."),
                (None, Direction::Out) => format!("Sent {object} to {actor}."),
                (None, _) => format!("{object} moved between parties."),
            }
        }
        crate::reading::Reading::Unshield => match cell.direction {
            Direction::In => format!("Unshielded {object} from {actor}."),
            _ => format!("Unshielded {object}."),
        },
        crate::reading::Reading::SelfTransfer => {
            format!("Moved {object} between own addresses.")
        }
        crate::reading::Reading::Mint => format!("Minted {object}."),
        crate::reading::Reading::Burn => format!("Burned {object}."),
        crate::reading::Reading::Swap => match other {
            Some(other) => format!("Swapped {object} between {actor} and {other}."),
            None => format!("Swapped {object} with {actor}."),
        },
        crate::reading::Reading::Wrap => format!("Wrapped {object}."),
        crate::reading::Reading::Unwrap => format!("Unwrapped {object}."),
        crate::reading::Reading::Lock => format!("Locked {object} in a script."),
        crate::reading::Reading::Release => format!("Released {object} from a script."),
        crate::reading::Reading::Stake => format!("Staked {object}."),
        crate::reading::Reading::Unstake => format!("Unstaked {object}."),
        crate::reading::Reading::Vest => format!("Locked {object} in a vesting contract."),
        crate::reading::Reading::Borrow => format!("Borrowed {object}."),
        crate::reading::Reading::Repay => format!("Repaid {object}."),
        crate::reading::Reading::Liquidate => format!("Liquidated {object}."),
        crate::reading::Reading::Approve => {
            format!("Approved {actor} to spend {object}.")
        }
        crate::reading::Reading::RevokeApproval => {
            format!("Revoked {actor}'s permission to spend {object}.")
        }
        crate::reading::Reading::PriceChange => match figure {
            Some(amount) => format!("Changed the price of {object} to {amount}."),
            None => format!("Changed the price of {object}."),
        },
        crate::reading::Reading::Delist => format!("Delisted {object}."),
        crate::reading::Reading::CancelOffer => format!("Cancelled an offer on {object}."),
        crate::reading::Reading::Cancel => {
            let mut text = format!("Cancelled a claim on {object}.");
            if matches!(cell.absence, Some(Absence::Unknown)) {
                text.push_str(" The side is not known.");
            }
            text
        }
        crate::reading::Reading::Unknown => {
            format!("An unrecognised action on {object}.")
        }
    };
    let note = match &cell.absence {
        Some(Absence::Hidden {
            reason: Hidden::Shielded,
        }) => " The amount is shielded.",
        Some(Absence::Hidden {
            reason: Hidden::Private,
        }) => " It is private.",
        Some(Absence::Hidden {
            reason: Hidden::NotIndexed,
        }) => " It was not indexed.",
        Some(Absence::Unknown) if cell.reading != crate::reading::Reading::Cancel => {
            " The details are unknown."
        }
        Some(Absence::Ambiguous { count }) => {
            return format!("{body} {count} possibilities were recorded.");
        }
        _ => "",
    };
    format!("{body}{note}")
}

fn sentence_object(object: &Object) -> String {
    match object {
        Object::Asset(asset) => format_asset(asset),
        Object::Lot {
            distinct, items, ..
        } => {
            if *distinct == 1
                && let Some(asset) = items.first()
            {
                return format_asset(asset);
            }
            format!("{distinct} items")
        }
        Object::Collection(_) => "a collection".to_string(),
        Object::Hidden {
            visibility: Visibility::Shielded,
            ..
        } => "a shielded asset".to_string(),
        Object::Hidden { .. } => "a hidden asset".to_string(),
    }
}

fn format_object(object: &Object) -> String {
    match object {
        Object::Asset(asset) => format_asset(asset),
        Object::Lot {
            distinct, items, ..
        } => {
            if *distinct == 1
                && let Some(asset) = items.first()
            {
                return format_asset(asset);
            }
            format!("{distinct} items")
        }
        Object::Collection(asset_type) => {
            format!("collection {}", short_reference(asset_type.reference()))
        }
        Object::Hidden {
            visibility: Visibility::Shielded,
            ..
        } => "shielded asset".to_string(),
        Object::Hidden { .. } => "hidden asset".to_string(),
    }
}

fn format_figure(figure: &Figure) -> String {
    format!("{} {}", figure.kind.label(), format_asset(&figure.amount))
}

fn format_asset(asset: &Asset) -> String {
    let amount = format_quantity(asset.quantity, asset.decimals);
    let label = asset.label.clone().unwrap_or_else(|| short_token(asset));
    if asset.is_currency() {
        format!("{amount} {label}")
    } else if asset.quantity == Quantity::ONE {
        label
    } else {
        format!("{amount} × {label}")
    }
}

fn format_quantity(quantity: Quantity, decimals: Option<u8>) -> String {
    let Some(decimals) = decimals else {
        return quantity.to_decimal();
    };
    if decimals == 0 {
        return quantity.to_decimal();
    }
    let mut digits = quantity.to_decimal();
    if digits.len() <= decimals as usize {
        let zeros = "0".repeat(decimals as usize - digits.len() + 1);
        digits = format!("{zeros}{digits}");
    }
    let split = digits.len() - decimals as usize;
    let (whole, fraction) = digits.split_at(split);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fraction}")
    }
}

fn short_token(asset: &Asset) -> String {
    if let Some(token_id) = asset.token.token_id() {
        return format!("#{token_id}");
    }
    short_reference(asset.token.asset_type().reference())
}

fn short_reference(reference: &str) -> String {
    if reference.len() > 12 {
        format!("{}…{}", &reference[..6], &reference[reference.len() - 4..])
    } else {
        reference.to_string()
    }
}

fn format_actor(actor: &Actor) -> String {
    if actor.role == Role::Own {
        return "you".to_string();
    }
    match &actor.party {
        Party::Known { label, .. } => label.clone(),
        Party::BelowFloor => "below the scan floor".to_string(),
        Party::Untraced => "untraced".to_string(),
        Party::Shielded { .. } => "a shielded party".to_string(),
        Party::Ambiguous { count } => format!("{count} parties"),
        Party::Unknown => match actor.role {
            Role::Peer => "peer".to_string(),
            Role::Venue => "venue".to_string(),
            Role::Contract => "contract".to_string(),
            Role::Pool => "pool".to_string(),
            Role::Network => "network".to_string(),
            _ => "unknown".to_string(),
        },
    }
}

fn format_custody(custody: Custody) -> &'static str {
    match custody {
        Custody::InHand => "",
        Custody::Escrow => "escrow",
        Custody::Script => "script",
        Custody::Staked => "staked",
    }
}

fn format_absence(absence: &Absence) -> &'static str {
    match absence {
        Absence::Hidden {
            reason: Hidden::Shielded,
        } => "shielded",
        Absence::Hidden {
            reason: Hidden::Private,
        } => "private",
        Absence::Hidden {
            reason: Hidden::NotIndexed,
        } => "not indexed",
        Absence::Unknown => "unknown",
        Absence::Ambiguous { .. } => "ambiguous",
    }
}
