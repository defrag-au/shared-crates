//! Marking a registry label so it can't be mistaken for an ADA Handle.
//!
//! Names for a Cardano party arrive from two places and mean different things.
//! A HANDLE is an NFT someone bought: `$alice`, self-chosen, transferable, and
//! rendered with its `$`. A LABEL comes from this registry: `JPG.store`,
//! attested by whoever added the entry, and rendered plain.
//!
//! Plenty of code carries a party's name as one `String` — a map from stake
//! key to name, a column, a wire field — and once the two are in the same slot
//! nothing downstream can tell them apart. The visible symptom is `$JPG.store`
//! on screen; the quieter one is a venue label written to a column called
//! `handle`, where it will later be read back as a person.
//!
//! So a label travels marked: `[JPG.store]`. ADA Handles are drawn from
//! `a-z 0-9 . _ -`, which excludes `[` and `]`, so the marker cannot collide
//! with a real name and the test is a cheap one on the string itself.
//!
//! This lives here, next to the labels, so that a display layer can unmark a
//! name without depending on whatever service produced it.

/// Mark a registry label as an entity name rather than a handle.
pub fn mark(label: &str) -> String {
    format!("[{label}]")
}

/// The label inside a marked name, or `None` if it is an ordinary handle.
///
/// Both brackets are required — a half-marked string is treated as a handle,
/// so a truncated name can never be promoted into an entity.
pub fn unmark(name: &str) -> Option<&str> {
    name.strip_prefix('[')?.strip_suffix(']')
}

/// Whether `name` is a marked registry label rather than an ADA Handle.
pub fn is_marked(name: &str) -> bool {
    unmark(name).is_some()
}

/// Render a name the way its kind requires: a label plain, a handle with `$`.
///
/// The `$` is not doubled if the handle already carries one, because callers
/// disagree about whether they store it.
pub fn display(name: &str) -> String {
    match unmark(name) {
        Some(label) => label.to_string(),
        None => format!("${}", name.trim_start_matches('$')),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marked_label_round_trips() {
        assert_eq!(mark("JPG.store"), "[JPG.store]");
        assert_eq!(unmark("[JPG.store]"), Some("JPG.store"));
        assert!(is_marked("[JPG.store]"));
    }

    /// The whole point: a handle can never look like a label. `.`, `_` and `-`
    /// are all legal in a handle, so the test has to be the brackets.
    #[test]
    fn a_handle_is_never_mistaken_for_a_label() {
        for handle in ["alice", "jpg.store", "dr.death", "alice_bob", "alice-123"] {
            assert!(!is_marked(handle), "{handle} is a handle");
            assert_eq!(unmark(handle), None);
        }
    }

    /// Half a marker is not a marker.
    #[test]
    fn an_unbalanced_bracket_is_not_a_marker() {
        assert!(!is_marked("[JPG.store"));
        assert!(!is_marked("JPG.store]"));
    }

    #[test]
    fn display_punctuates_each_kind() {
        assert_eq!(display("[JPG.store]"), "JPG.store");
        assert_eq!(display("alice"), "$alice");
    }

    /// Callers disagree about whether a stored handle keeps its `$`.
    #[test]
    fn display_does_not_double_an_existing_dollar() {
        assert_eq!(display("$alice"), "$alice");
    }
}
