//! Selection — one shared "what is the reader pointing at" for a whole surface.
//!
//! Brushing & linking's other half. The spine links faces in *time*; this
//! links them in *identity*: hover a holder's pile and that holder lights up in
//! every face; pin it (click) and it stays lit while you scrub. Faces READ this
//! to decide emphasis and WRITE it from their own hover/click. It is a plain
//! struct on purpose — the app owns one and hands `&mut` to each face in turn.
//!
//! Emphasis follows the dataviz rule "one series is the point, the rest are
//! context": when something is selected, everything else drops to the
//! de-emphasis level rather than the selected thing getting louder.
//!
//! ## Several pins, one focus
//!
//! **Pins are a SET.** Comparing two wallets is the normal case in an
//! investigation — "did these two ever pay the same person" is the question
//! that cracks a case — and a single-slot selection forces you to keep
//! re-pinning and holding the other one in your head.
//!
//! Some things still need exactly one subject, though: a detail panel, an
//! annotation form. That is the **focus** — the most recently pinned by
//! default, changeable by picking one. So:
//!
//! - [`Selection::is_selected`] — in the pinned set (or hovered). Drives
//!   emphasis, rings, labels: several things can be lit at once.
//! - [`Selection::active`] — the ONE subject: the focus, else the hover.

/// A shared selection over entity keys (holder, destination, party — any `String`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Under the pointer right now. Cleared by whoever set it when the pointer
    /// leaves; faces should call [`Selection::clear_hover_if`] on their key.
    pub hovered: Option<String>,
    /// Clicked to stick, in the order they were pinned so a chip row is
    /// stable. Survives hover changes.
    pub pinned: Vec<String>,
    /// Which pin a single-subject view (detail panel, annotator) is showing.
    /// Always one of `pinned`, or `None` when nothing is pinned.
    pub focus: Option<String>,
}

impl Selection {
    /// The ONE subject for a detail view: the focused pin, else whatever is
    /// hovered. Use [`Self::is_selected`] for emphasis — several things can be
    /// lit at once, but only one can be *inspected*.
    pub fn active(&self) -> Option<&str> {
        self.focus
            .as_deref()
            .or_else(|| self.pinned.last().map(String::as_str))
            .or(self.hovered.as_deref())
    }

    /// Is anything selected at all? If not, faces render at full weight.
    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty() && self.hovered.is_none()
    }

    /// Part of the selection — pinned, or under the pointer.
    pub fn is_selected(&self, key: &str) -> bool {
        self.hovered.as_deref() == Some(key) || self.is_pinned(key)
    }

    pub fn is_pinned(&self, key: &str) -> bool {
        self.pinned.iter().any(|k| k == key)
    }

    /// Emphasis level for `key` in `0.0..=1.0`: 1 when nothing is selected or
    /// this key is part of the selection; the de-emphasis floor otherwise.
    pub fn emphasis(&self, key: &str) -> f32 {
        if self.is_empty() || self.is_selected(key) {
            1.0
        } else {
            DIM
        }
    }

    /// Kept as the "is this the thing" predicate faces already call. Now true
    /// for ANY pinned key, not just one.
    pub fn is_active(&self, key: &str) -> bool {
        self.is_selected(key)
    }

    pub fn hover(&mut self, key: impl Into<String>) {
        self.hovered = Some(key.into());
    }

    /// Clear the hover only if it is `key` — so a face leaving doesn't stomp
    /// on a hover another face just set this same frame.
    pub fn clear_hover_if(&mut self, key: &str) {
        if self.hovered.as_deref() == Some(key) {
            self.hovered = None;
        }
    }

    /// Add to the pinned set and focus it. Pinning something already pinned
    /// just moves the focus there.
    pub fn pin(&mut self, key: impl Into<String>) {
        let key = key.into();
        if !self.is_pinned(&key) {
            self.pinned.push(key.clone());
        }
        self.focus = Some(key);
    }

    pub fn unpin(&mut self, key: &str) {
        self.pinned.retain(|k| k != key);
        // The focus must always name a live pin, or a detail panel would keep
        // showing something the reader has just dismissed.
        if self.focus.as_deref() == Some(key) {
            self.focus = self.pinned.last().cloned();
        }
    }

    /// Move the focus without changing what is pinned — clicking a chip.
    pub fn focus_on(&mut self, key: &str) {
        if self.is_pinned(key) {
            self.focus = Some(key.to_string());
        }
    }

    /// Click semantics: pin, or unpin if already pinned.
    pub fn toggle_pin(&mut self, key: impl Into<String>) {
        let key = key.into();
        if self.is_pinned(&key) {
            self.unpin(&key);
        } else {
            self.pin(key);
        }
    }

    /// Clear every pin — clicking empty space.
    pub fn clear_pin(&mut self) {
        self.pinned.clear();
        self.focus = None;
    }
}

/// The de-emphasis level for non-selected marks. Low enough to recede, high
/// enough that the shape of the whole is still legible behind the selection.
pub const DIM: f32 = 0.22;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_and_a_single_pin_behave_as_before() {
        let mut s = Selection::default();
        assert!(s.is_empty());
        assert_eq!(s.emphasis("a"), 1.0);
        s.hover("a");
        assert_eq!(s.active(), Some("a"));
        assert_eq!(s.emphasis("a"), 1.0);
        assert_eq!(s.emphasis("b"), DIM);
        s.toggle_pin("b");
        assert_eq!(s.active(), Some("b"), "a pin outranks a hover");
        s.clear_hover_if("zzz");
        assert_eq!(s.hovered.as_deref(), Some("a"), "wrong key doesn't clear");
        s.clear_hover_if("a");
        assert!(s.hovered.is_none());
        s.toggle_pin("b");
        assert!(s.is_empty(), "toggling the pinned key unpins");
    }

    /// The point of the change: several wallets lit at once.
    #[test]
    fn several_pins_are_all_selected() {
        let mut s = Selection::default();
        s.pin("a");
        s.pin("b");
        s.pin("c");
        for k in ["a", "b", "c"] {
            assert!(s.is_selected(k), "{k} should be lit");
            assert_eq!(s.emphasis(k), 1.0);
        }
        assert_eq!(s.emphasis("d"), DIM, "everything else recedes");
        assert_eq!(s.pinned, vec!["a", "b", "c"], "pin order is stable");
        assert_eq!(s.active(), Some("c"), "focus follows the newest pin");
    }

    /// The focus must always name a LIVE pin — otherwise a detail panel keeps
    /// showing a wallet the reader just dismissed.
    #[test]
    fn unpinning_the_focus_moves_it_to_a_survivor() {
        let mut s = Selection::default();
        s.pin("a");
        s.pin("b");
        assert_eq!(s.active(), Some("b"));

        s.unpin("b");
        assert_eq!(s.active(), Some("a"), "focus fell back to the remaining pin");
        assert!(!s.is_selected("b"));

        s.unpin("a");
        assert_eq!(s.active(), None);
        assert!(s.is_empty());
    }

    /// Focusing changes what is inspected without changing what is lit.
    #[test]
    fn focusing_a_chip_does_not_change_the_pinned_set() {
        let mut s = Selection::default();
        s.pin("a");
        s.pin("b");
        s.focus_on("a");
        assert_eq!(s.active(), Some("a"));
        assert_eq!(s.pinned, vec!["a", "b"], "both still pinned");
        assert!(s.is_selected("b"));

        // Focusing something that is not pinned is a no-op, not a silent pin.
        s.focus_on("zzz");
        assert_eq!(s.active(), Some("a"));
        assert!(!s.is_pinned("zzz"));
    }

    /// Re-pinning an existing pin refocuses rather than duplicating.
    #[test]
    fn re_pinning_refocuses_without_duplicating() {
        let mut s = Selection::default();
        s.pin("a");
        s.pin("b");
        s.pin("a");
        assert_eq!(s.pinned, vec!["a", "b"], "no duplicate");
        assert_eq!(s.active(), Some("a"), "focus moved back to a");
    }

    #[test]
    fn clearing_drops_every_pin_and_the_focus() {
        let mut s = Selection::default();
        s.pin("a");
        s.pin("b");
        s.clear_pin();
        assert!(s.pinned.is_empty());
        assert_eq!(s.focus, None);
        assert!(s.is_empty());
    }
}
