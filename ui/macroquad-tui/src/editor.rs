//! Single-line editor with history. Drives the prompt area; the grid renderer
//! reads its current state each frame and paints prompt + buffer + cursor
//! position into the bottom-most editable row.

#[derive(Default)]
pub struct LineEditor {
    pub prompt: String,
    pub buffer: String,
    pub cursor: usize,
    history: Vec<String>,
    /// Index into history when up-arrow is browsing past entries. `None`
    /// means the current `buffer` is live (not yet submitted).
    history_idx: Option<usize>,
    /// Snapshot of the live buffer when history-browsing starts, so down-
    /// arrow back past the latest entry restores what the user was typing.
    pending: String,
}

impl LineEditor {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            ..Default::default()
        }
    }

    pub fn insert(&mut self, ch: char) {
        self.buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_idx = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut new_cursor = self.cursor - 1;
        while !self.buffer.is_char_boundary(new_cursor) && new_cursor > 0 {
            new_cursor -= 1;
        }
        self.buffer.replace_range(new_cursor..self.cursor, "");
        self.cursor = new_cursor;
        self.history_idx = None;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }
        let mut end = self.cursor + 1;
        while end < self.buffer.len() && !self.buffer.is_char_boundary(end) {
            end += 1;
        }
        self.buffer.replace_range(self.cursor..end, "");
        self.history_idx = None;
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut nc = self.cursor - 1;
        while !self.buffer.is_char_boundary(nc) && nc > 0 {
            nc -= 1;
        }
        self.cursor = nc;
    }

    pub fn move_right(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }
        let mut nc = self.cursor + 1;
        while nc < self.buffer.len() && !self.buffer.is_char_boundary(nc) {
            nc += 1;
        }
        self.cursor = nc;
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_idx {
            None => {
                self.pending = self.buffer.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_idx = Some(next);
        self.buffer = self.history[next].clone();
        self.cursor = self.buffer.len();
    }

    pub fn history_next(&mut self) {
        let Some(i) = self.history_idx else { return };
        if i + 1 >= self.history.len() {
            self.history_idx = None;
            self.buffer = std::mem::take(&mut self.pending);
            self.cursor = self.buffer.len();
        } else {
            let n = i + 1;
            self.history_idx = Some(n);
            self.buffer = self.history[n].clone();
            self.cursor = self.buffer.len();
        }
    }

    /// Commit the current buffer, push to history (unless empty or duplicate),
    /// reset the editor for the next input. Returns the submitted line.
    pub fn submit(&mut self) -> String {
        let line = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        self.history_idx = None;
        self.pending.clear();
        if !line.trim().is_empty() && self.history.last().map(|s| s.as_str()) != Some(line.as_str())
        {
            self.history.push(line.clone());
        }
        line
    }

    /// Current full display line (prompt + buffer), with the cursor column
    /// expressed as the grid offset where the next char will appear.
    pub fn render(&self) -> (String, usize) {
        let line = format!("{}{}", self.prompt, self.buffer);
        let cursor_col = self.prompt.chars().count() + self.buffer[..self.cursor].chars().count();
        (line, cursor_col)
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Byte range of the "current word" — the run of non-whitespace
    /// characters ending at the cursor. Used by tab completion to
    /// decide what to replace.
    pub fn current_word_range(&self) -> std::ops::Range<usize> {
        let bytes = self.buffer.as_bytes();
        let end = self.cursor;
        let mut start = end;
        while start > 0 {
            let prev = start - 1;
            if (bytes[prev] as char).is_whitespace() {
                break;
            }
            start = prev;
        }
        start..end
    }

    pub fn current_word(&self) -> &str {
        &self.buffer[self.current_word_range()]
    }

    /// Replace the current word with `replacement`, moving the cursor to
    /// the end of the inserted text.
    pub fn replace_current_word(&mut self, replacement: &str) {
        let range = self.current_word_range();
        let new_cursor = range.start + replacement.len();
        self.buffer.replace_range(range, replacement);
        self.cursor = new_cursor;
        self.history_idx = None;
    }

    /// Apply a tab completion. Returns the result so the caller can
    /// decide whether to print the candidate list (when ambiguous).
    pub fn complete<S: CompletionSource>(&mut self, source: &S) -> CompletionResult {
        let candidates = source.candidates(self.buffer(), self.cursor());
        match candidates.len() {
            0 => CompletionResult::NoMatch,
            1 => {
                let mut c = candidates.into_iter().next().unwrap();
                if source.complete_with_trailing_space(&c) {
                    c.push(' ');
                }
                self.replace_current_word(&c);
                CompletionResult::Unique
            }
            _ => {
                let prefix = common_prefix(&candidates);
                let word = self.current_word().to_string();
                if prefix.len() > word.len() {
                    self.replace_current_word(&prefix);
                    CompletionResult::Prefix { candidates }
                } else {
                    CompletionResult::Ambiguous { candidates }
                }
            }
        }
    }
}

/// Plug into [`LineEditor::complete`]. Implementors inspect the current
/// buffer + cursor and return the candidate words that could come next.
pub trait CompletionSource {
    /// Return candidate strings for the word at `cursor` in `buffer`.
    fn candidates(&self, buffer: &str, cursor: usize) -> Vec<String>;

    /// Whether to append a trailing space when completing a unique match.
    /// Default `true` — fits commands, field names, and most identifiers.
    fn complete_with_trailing_space(&self, _completion: &str) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub enum CompletionResult {
    NoMatch,
    Unique,
    /// Buffer was extended to the common prefix of all candidates; caller
    /// can optionally show the list of remaining candidates.
    Prefix {
        candidates: Vec<String>,
    },
    /// All candidates already share the entire current word as prefix.
    /// Caller should display the list (e.g. on double-tab).
    Ambiguous {
        candidates: Vec<String>,
    },
}

fn common_prefix(candidates: &[String]) -> String {
    if candidates.is_empty() {
        return String::new();
    }
    let first = candidates[0].as_bytes();
    let mut end = first.len();
    for cand in &candidates[1..] {
        let cb = cand.as_bytes();
        end = end.min(cb.len());
        for i in 0..end {
            if first[i] != cb[i] {
                end = i;
                break;
            }
        }
    }
    // Round down to UTF-8 char boundary
    while end > 0 && !candidates[0].is_char_boundary(end) {
        end -= 1;
    }
    candidates[0][..end].to_string()
}

#[cfg(test)]
mod tab_tests {
    use super::*;

    struct Static<'a>(&'a [&'a str]);
    impl CompletionSource for Static<'_> {
        fn candidates(&self, buffer: &str, cursor: usize) -> Vec<String> {
            let word: String = buffer[..cursor]
                .rsplit(char::is_whitespace)
                .next()
                .unwrap_or("")
                .to_string();
            self.0
                .iter()
                .filter(|c| c.starts_with(&word))
                .map(|c| c.to_string())
                .collect()
        }
        fn complete_with_trailing_space(&self, _: &str) -> bool {
            false
        }
    }

    #[test]
    fn extends_to_common_prefix_when_multi() {
        let mut e = LineEditor::new("> ");
        e.insert('q');
        let r = e.complete(&Static(&["query", "quit"]));
        assert!(matches!(r, CompletionResult::Prefix { .. }));
        assert_eq!(e.buffer(), "qu");
    }

    #[test]
    fn unique_match_completes_fully() {
        let mut e = LineEditor::new("> ");
        e.insert('a');
        let r = e.complete(&Static(&["about"]));
        assert!(matches!(r, CompletionResult::Unique));
        assert_eq!(e.buffer(), "about");
    }

    #[test]
    fn no_match_leaves_buffer_alone() {
        let mut e = LineEditor::new("> ");
        e.insert('z');
        let r = e.complete(&Static(&["alpha", "beta"]));
        assert!(matches!(r, CompletionResult::NoMatch));
        assert_eq!(e.buffer(), "z");
    }

    #[test]
    fn current_word_is_token_at_cursor() {
        let mut e = LineEditor::new("> ");
        e.insert('q');
        e.insert('u');
        e.insert('e');
        e.insert('r');
        e.insert('y');
        e.insert(' ');
        e.insert('s');
        e.insert('h');
        assert_eq!(e.current_word(), "sh");
    }
}
