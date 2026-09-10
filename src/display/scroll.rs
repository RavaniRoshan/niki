//! Scroll state with follow semantics and chained scrolling (TUI-010).
//!
//! Every scrollable view owns a [`ScrollState`]: a manual `offset` plus a
//! `follow` pin. `follow` means "stay glued to the end" (chat auto-scroll);
//! any manual upward scroll clears it, scrolling past the end re-arms it.
//! [`ScrollState::scroll_by`] returns the unconsumed remainder so nested
//! views chain innermost-first (Pi remainder protocol): a tool-detail wheel
//! that hits the top scrolls the chat behind it instead of dying.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScrollState {
    /// Manual offset from the top, in lines. Ignored while `follow` is set.
    pub offset: usize,
    /// Pinned to the end of the content.
    pub follow: bool,
}

impl ScrollState {
    /// Manual scroller starting at the top (modals, detail views).
    pub fn new() -> Self {
        Self {
            offset: 0,
            follow: false,
        }
    }

    /// End-pinned scroller (chat transcript).
    pub fn follow_end() -> Self {
        Self {
            offset: 0,
            follow: true,
        }
    }

    /// Effective top-line offset for `content` lines in a `viewport`-tall view.
    pub fn view_offset(&self, content: usize, viewport: usize) -> usize {
        let max = content.saturating_sub(viewport);
        if self.follow {
            max
        } else {
            self.offset.min(max)
        }
    }

    /// Whether the view currently shows the end of the content.
    pub fn at_end(&self, content: usize, viewport: usize) -> bool {
        self.view_offset(content, viewport) >= content.saturating_sub(viewport)
    }

    /// Scroll by `delta` lines (negative = up). Returns the unconsumed
    /// remainder for chaining into the parent view: nonzero only when the
    /// edge was hit (top overshoot, or bottom overshoot past the end pin).
    pub fn scroll_by(&mut self, delta: isize, content: usize, viewport: usize) -> isize {
        let max = content.saturating_sub(viewport) as isize;
        let cur = if self.follow {
            max
        } else {
            (self.offset.min(max.max(0) as usize)) as isize
        };
        let next = cur + delta;
        if next < 0 {
            self.follow = false;
            self.offset = 0;
            return next;
        }
        if next > max {
            self.follow = true;
            self.offset = max.max(0) as usize;
            return next - max;
        }
        self.follow = false;
        self.offset = next as usize;
        0
    }

    /// Nudge without geometry (keyboard scrolling where no viewport is at
    /// hand). The next render clamps via [`ScrollState::view_offset`].
    /// Up clears the end pin; down preserves it (matches historical keys).
    pub fn nudge(&mut self, delta: isize) {
        if delta < 0 {
            self.follow = false;
            self.offset = self.offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta as usize);
        }
    }

    /// Jump to an absolute offset (scrollbar drag). Landing on the end
    /// re-arms the pin.
    pub fn jump_to(&mut self, offset: usize, content: usize, viewport: usize) {
        let max = content.saturating_sub(viewport);
        self.offset = offset.min(max);
        self.follow = self.offset >= max;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_pins_to_end() {
        let s = ScrollState::follow_end();
        assert_eq!(s.view_offset(100, 20), 80);
        assert!(s.at_end(100, 20));
        // Short content: offset 0, still "at end".
        assert_eq!(s.view_offset(10, 20), 0);
    }

    #[test]
    fn manual_offset_clamps() {
        let mut s = ScrollState::new();
        s.offset = 40;
        assert_eq!(s.view_offset(100, 20), 40);
        assert!(!s.at_end(100, 20));
        s.offset = 999; // clamps to max, which is the end
        assert_eq!(s.view_offset(100, 20), 80);
        assert!(s.at_end(100, 20));
    }

    #[test]
    fn scroll_by_moves_and_reports_remainder() {
        let mut s = ScrollState::follow_end();
        // Up from the end: moves, no remainder.
        assert_eq!(s.scroll_by(-3, 100, 20), 0);
        assert_eq!(s.view_offset(100, 20), 77);
        assert!(!s.follow);
        // Down past the end: pins, remainder carries.
        assert_eq!(s.scroll_by(10, 100, 20), 7);
        assert!(s.follow);
        assert_eq!(s.view_offset(100, 20), 80);
        // Up past the top: clamps to 0, negative remainder carries.
        let mut top = ScrollState::new();
        assert_eq!(top.scroll_by(-5, 100, 20), -5);
        assert_eq!(top.view_offset(100, 20), 0);
    }

    #[test]
    fn nudge_preserves_keyboard_semantics() {
        let mut s = ScrollState::follow_end();
        s.nudge(-1);
        assert!(!s.follow);
        assert_eq!(s.offset, 0); // was pinned (offset 0); saturates, pin cleared
        let mut d = ScrollState::follow_end();
        d.nudge(1); // down keeps the pin (historical: down never clears)
        assert!(d.follow);
    }

    #[test]
    fn jump_to_rearms_at_end() {
        let mut s = ScrollState::new();
        s.jump_to(40, 100, 20);
        assert!(!s.follow);
        assert_eq!(s.view_offset(100, 20), 40);
        s.jump_to(80, 100, 20);
        assert!(s.follow);
        s.jump_to(999, 100, 20);
        assert_eq!(s.view_offset(100, 20), 80);
    }
}
