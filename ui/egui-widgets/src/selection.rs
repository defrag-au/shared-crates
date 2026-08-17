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

/// A shared selection over entity keys (holder, destination, party — any `String`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Under the pointer right now. Cleared by whoever set it when the pointer
    /// leaves; faces should call [`Selection::clear_hover_if`] on their key.
    pub hovered: Option<String>,
    /// Clicked to stick. Survives hover changes; cleared by clicking again or
    /// clicking empty space.
    pub pinned: Option<String>,
}

impl Selection {
    /// The key currently in force: pinned wins over hovered.
    pub fn active(&self) -> Option<&str> {
        self.pinned.as_deref().or(self.hovered.as_deref())
    }

    /// Is anything selected at all? If not, faces render at full weight.
    pub fn is_empty(&self) -> bool {
        self.active().is_none()
    }

    /// Emphasis level for `key` in `0.0..=1.0`: 1 when nothing is selected or
    /// this key is the selection; the de-emphasis floor otherwise.
    pub fn emphasis(&self, key: &str) -> f32 {
        match self.active() {
            None => 1.0,
            Some(k) if k == key => 1.0,
            Some(_) => DIM,
        }
    }

    pub fn is_active(&self, key: &str) -> bool {
        self.active() == Some(key)
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

    /// Click semantics: pin, or unpin if already pinned.
    pub fn toggle_pin(&mut self, key: impl Into<String>) {
        let key = key.into();
        if self.pinned.as_deref() == Some(key.as_str()) {
            self.pinned = None;
        } else {
            self.pinned = Some(key);
        }
    }

    pub fn clear_pin(&mut self) {
        self.pinned = None;
    }
}

/// The de-emphasis level for non-selected marks. Low enough to recede, high
/// enough that the shape of the whole is still legible behind the selection.
pub const DIM: f32 = 0.22;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_wins_and_emphasis_dims_others() {
        let mut s = Selection::default();
        assert!(s.is_empty());
        assert_eq!(s.emphasis("a"), 1.0);
        s.hover("a");
        assert_eq!(s.active(), Some("a"));
        assert_eq!(s.emphasis("a"), 1.0);
        assert_eq!(s.emphasis("b"), DIM);
        s.toggle_pin("b");
        assert_eq!(s.active(), Some("b"), "pinned wins over hovered");
        s.clear_hover_if("zzz");
        assert_eq!(s.hovered.as_deref(), Some("a"), "wrong key doesn't clear");
        s.clear_hover_if("a");
        assert!(s.hovered.is_none());
        s.toggle_pin("b");
        assert!(s.is_empty(), "toggling the pinned key unpins");
    }
}
