//! Transcript search panel (TUI-011).
//!
//! Plain-text, case-insensitive substring search over the built chat lines.
//! Deliberately v1-scoped (per the Pi extraction plan): no grapheme-span
//! mapping, no regex. Matches are line indices into `state.chat_lines`;
//! navigation reveals the match by clearing the end pin and jumping the
//! chat scroll to ~1/3 viewport above the hit.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::display::state::AppState;

/// Open search state: query + match cursor over transcript line indices.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<usize>,
    pub selected: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recompute matches for `query` over line texts. Resets the cursor.
    pub fn run<'a>(&mut self, query: String, texts: impl Iterator<Item = &'a str>) {
        self.query = query;
        self.matches.clear();
        self.selected = 0;
        let needle = self.query.to_lowercase();
        if needle.is_empty() {
            return;
        }
        for (i, text) in texts.enumerate() {
            if text.to_lowercase().contains(&needle) {
                self.matches.push(i);
            }
        }
    }

    /// Currently selected match as a transcript line index.
    pub fn current(&self) -> Option<usize> {
        self.matches.get(self.selected).copied()
    }

    /// 1-based (position, total) for the status marker. `None` when empty.
    pub fn position(&self) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            None
        } else {
            Some((self.selected + 1, self.matches.len()))
        }
    }

    /// Next match, wrapping. No-op when empty.
    pub fn next(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    /// Previous match, wrapping. No-op when empty.
    pub fn prev(&mut self) {
        if !self.matches.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.matches.len() - 1);
        }
    }
}

/// Handle a key while search is open. Returns true when consumed.
/// Printable characters edit the query; Enter/Down/Up cycle matches;
/// Esc closes. Anything else falls through to the composer.
pub fn handle_search_key(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        KeyCode::Esc => {
            state.search = None;
            true
        }
        KeyCode::Enter | KeyCode::Down => {
            if let Some(s) = state.search.as_mut() {
                s.next();
            }
            reveal_current(state);
            true
        }
        KeyCode::Up => {
            if let Some(s) = state.search.as_mut() {
                s.prev();
            }
            reveal_current(state);
            true
        }
        KeyCode::Backspace => {
            let mut query = String::new();
            if let Some(s) = state.search.as_mut() {
                s.query.pop();
                query = s.query.clone();
            }
            rerun(state, query);
            true
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            let mut query = String::new();
            if let Some(s) = state.search.as_mut() {
                s.query.push(c);
                query = s.query.clone();
            }
            rerun(state, query);
            true
        }
        _ => false,
    }
}

/// Recompute matches for `query` over the current transcript and reveal.
fn rerun(state: &mut AppState, query: String) {
    let texts: Vec<String> = state.chat_lines.iter().map(|l| l.text.clone()).collect();
    if let Some(s) = state.search.as_mut() {
        s.run(query, texts.iter().map(|t| t.as_str()));
    }
    reveal_current(state);
}

/// Jump the chat scroll so the current match sits ~1/3 viewport from the top.
/// Clears the end pin (searching implies leaving the tail).
fn reveal_current(state: &mut AppState) {
    let target = state.search.as_ref().and_then(|s| s.current());
    if let Some(line) = target {
        let viewport = state.chat_viewport_h.get().max(1);
        state.chat_scroll.follow = false;
        state.chat_scroll.offset = line.saturating_sub(viewport / 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn searched(query: &str) -> SearchState {
        let mut s = SearchState::new();
        let lines = ["hello world", "nothing here", "HELLO again", "last"];
        s.run(query.to_string(), lines.into_iter());
        s
    }

    #[test]
    fn run_is_case_insensitive() {
        let s = searched("hello");
        assert_eq!(s.matches, vec![0, 2]);
        assert_eq!(s.position(), Some((1, 2)));
    }

    #[test]
    fn empty_query_matches_nothing() {
        let s = searched("");
        assert!(s.matches.is_empty());
        assert_eq!(s.current(), None);
        assert_eq!(s.position(), None);
    }

    #[test]
    fn next_prev_wrap() {
        let mut s = searched("hello");
        assert_eq!(s.current(), Some(0));
        s.next();
        assert_eq!(s.current(), Some(2));
        s.next();
        assert_eq!(s.current(), Some(0));
        s.prev();
        assert_eq!(s.current(), Some(2));
    }

    #[test]
    fn nav_on_empty_is_noop() {
        let mut s = searched("zzz");
        s.next();
        s.prev();
        assert_eq!(s.current(), None);
    }
}
