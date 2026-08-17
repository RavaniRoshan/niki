//! Universal list cursor shared by every selectable panel/overlay.
//!
//! The command palette, the slash command menu and the permission modal all
//! present a vertical list of rows the user moves through with `Up`/`Down`
//! (or `k`/`j`), activates with `Enter`, and — since Phase 4 — hovers and
//! clicks with the mouse. [`ListCursor`] is the single piece of arithmetic
//! behind all of them so the panels behave identically.
//!
//! Deliberately dependency-light: plain `usize` math, no ratatui types.

/// A wrapping cursor over `count` rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListCursor {
    /// Currently highlighted row (always `< count`, or `0` when empty).
    pub selected: usize,
    /// Number of selectable rows.
    pub count: usize,
}

/// Which panel currently owns list navigation (`Up`/`Down`/`Enter`/`Esc`)
/// and mouse hover/click routing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocusState {
    /// The chat view / input box owns keys and mouse (copy-mode).
    #[default]
    Chat,
    /// The slash command menu overlay is active.
    CommandMenu,
    /// The Ctrl+P command palette overlay is active.
    CommandPalette,
    /// The permission request modal is active.
    Permission,
}

impl FocusState {
    /// `true` when an overlay (not the chat view) owns list navigation.
    pub fn is_overlay(self) -> bool {
        !matches!(self, FocusState::Chat)
    }
}

impl ListCursor {
    /// A cursor over `count` rows, starting at row 0.
    pub fn new(count: usize) -> Self {
        Self { selected: 0, count }
    }

    /// A cursor over `count` rows starting at `selected` (clamped into range).
    pub fn with_selected(count: usize, selected: usize) -> Self {
        let mut c = Self { selected: 0, count };
        c.set_selected(selected);
        c
    }

    /// No rows to select.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Update the row count (e.g. after the filter changed), clamping the
    /// selection so it never points past the end of the list.
    pub fn set_count(&mut self, count: usize) {
        self.count = count;
        if self.selected >= count {
            self.selected = count.saturating_sub(1);
        }
    }

    /// Move the selection to `idx`, clamped into range.
    pub fn set_selected(&mut self, idx: usize) {
        self.selected = if self.count == 0 {
            0
        } else {
            idx.min(self.count - 1)
        };
    }

    /// Move up one row; wraps to the last row from the top.
    pub fn prev(&mut self) {
        if self.count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = if self.selected == 0 {
            self.count - 1
        } else {
            self.selected - 1
        };
    }

    /// Move down one row; wraps to the first row from the bottom.
    pub fn next(&mut self) {
        if self.count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % self.count;
    }

    /// Activate the current row (`Enter`). `None` when the list is empty.
    pub fn submit(&self) -> Option<usize> {
        if self.count == 0 {
            None
        } else {
            Some(self.selected.min(self.count - 1))
        }
    }

    /// Move the highlight to a hovered row. Returns `true` when the highlight
    /// actually moved (so callers only repaint on real changes). Out-of-range
    /// rows are ignored — hovering the border must not move the selection.
    pub fn hover(&mut self, idx: usize) -> bool {
        if idx >= self.count {
            return false;
        }
        let changed = self.selected != idx;
        self.selected = idx;
        changed
    }

    /// Select and activate a clicked row. `None` when the row does not exist.
    pub fn click(&mut self, idx: usize) -> Option<usize> {
        if idx >= self.count {
            return None;
        }
        self.selected = idx;
        Some(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_wraps_and_clamps() {
        let mut c = ListCursor::new(3);
        c.prev();
        assert_eq!(c.selected, 2, "prev at top wraps to last row");
        c.prev();
        assert_eq!(c.selected, 1);

        // Empty list: prev is a no-op pinned at 0.
        let mut empty = ListCursor::new(0);
        empty.prev();
        assert_eq!(empty.selected, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn next_wraps_and_clamps() {
        let mut c = ListCursor::new(2);
        c.next();
        assert_eq!(c.selected, 1);
        c.next();
        assert_eq!(c.selected, 0, "next at bottom wraps to first row");

        // Empty list: next is a no-op pinned at 0.
        let mut empty = ListCursor::new(0);
        empty.next();
        assert_eq!(empty.selected, 0);
    }

    #[test]
    fn submit_returns_selected_or_none() {
        let mut c = ListCursor::new(3);
        c.next();
        assert_eq!(c.submit(), Some(1));
        assert_eq!(ListCursor::new(0).submit(), None);
    }

    #[test]
    fn hover_and_click_ignore_out_of_range() {
        let mut c = ListCursor::new(3);
        assert!(c.hover(2));
        assert!(!c.hover(2), "hovering the same row reports no change");
        assert!(!c.hover(9), "out-of-range hover keeps the selection");
        assert_eq!(c.selected, 2);

        assert_eq!(c.click(0), Some(0));
        assert_eq!(c.selected, 0);
        assert_eq!(c.click(7), None);
        assert_eq!(c.selected, 0);
    }

    #[test]
    fn set_count_clamps_selection() {
        let mut c = ListCursor::with_selected(5, 4);
        assert_eq!(c.selected, 4);
        c.set_count(2);
        assert_eq!(c.selected, 1);
        c.set_count(0);
        assert_eq!(c.selected, 0);
        assert_eq!(c.submit(), None);
    }

    #[test]
    fn focus_state_overlay_predicate() {
        assert!(!FocusState::default().is_overlay());
        assert!(FocusState::CommandMenu.is_overlay());
        assert!(FocusState::CommandPalette.is_overlay());
        assert!(FocusState::Permission.is_overlay());
    }
}
