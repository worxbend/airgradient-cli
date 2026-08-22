//! A selection index into a list, kept inside that list's bounds.
//!
//! Three views drive a vertical list with the same keys — the theme picker, the
//! config editor, and the chart's metric list — and each had written out the
//! same rule: an index moved by a signed delta, clamped to the ends, with a
//! half-page step and jumps to first and last.
//!
//! The list each one indexes is not owned here, and its length can change
//! between frames (the metric list follows what the device reports), so every
//! method takes the current `len`. That is what makes "the cursor is always in
//! range" a property of the type rather than something each caller has to
//! remember at both the write and the read.

/// A clamped index into a list of `len` items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListCursor {
    index: usize,
}

impl ListCursor {
    /// The selected index, clamped to a list of `len` items.
    ///
    /// Clamped on the way out as well as on the way in, because the list may
    /// have shrunk since the cursor was last moved.
    pub fn index(self, len: usize) -> usize {
        self.index.min(len.saturating_sub(1))
    }

    /// Moves the cursor by `delta`, stopping at either end.
    ///
    /// Stops rather than wraps: on a settings list, running off the bottom onto
    /// the top would move the selection the opposite way to the key that was
    /// pressed.
    pub fn move_by(&mut self, delta: i32, len: usize) {
        let last = len.saturating_sub(1);
        // `last` bounds the cast: these lists are a handful of rows, never
        // anywhere near `i32::MAX`.
        let next = (self.index.min(last) as i32).saturating_add(delta);
        self.index = next.clamp(0, last as i32) as usize;
    }

    /// Selects `index`, clamped to the list — e.g. from a click on a row.
    pub fn set(&mut self, index: usize, len: usize) {
        self.index = index.min(len.saturating_sub(1));
    }

    /// The signed step `ctrl-d`/`ctrl-u` take: half a screen's worth of rows,
    /// in `direction`.
    ///
    /// Half a page is at least one row, so the key still moves on a list too
    /// short to halve. The three views that scroll this way each apply the
    /// step through their own cursor-moving method -- moving the theme cursor
    /// also previews the theme under it -- so this returns the delta rather
    /// than moving anything itself.
    pub fn half_page_delta(direction: i32, len: usize) -> i32 {
        direction * (len / 2).max(1) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_stops_at_both_ends_rather_than_wrapping() {
        let mut cursor = ListCursor::default();

        cursor.move_by(-1, 4);
        assert_eq!(cursor.index(4), 0, "already at the top");

        cursor.move_by(99, 4);
        assert_eq!(cursor.index(4), 3, "clamped to the last row");

        cursor.move_by(1, 4);
        assert_eq!(cursor.index(4), 3, "already at the bottom");
    }

    #[test]
    fn an_empty_list_has_no_row_to_select() {
        let mut cursor = ListCursor::default();

        cursor.move_by(5, 0);
        assert_eq!(cursor.index(0), 0, "no panic, and nothing selected");
        cursor.set(7, 0);
        assert_eq!(cursor.index(0), 0);
    }

    #[test]
    fn a_cursor_past_the_end_of_a_shrunken_list_reads_as_its_last_row() {
        // The metric list follows what the device reports, so it can shrink
        // between the frame that moved the cursor and the frame that reads it.
        let mut cursor = ListCursor::default();
        cursor.set(9, 10);

        assert_eq!(cursor.index(10), 9);
        assert_eq!(cursor.index(3), 2, "clamped on the way out too");
    }

    #[test]
    fn moving_after_the_list_shrinks_starts_from_the_clamped_position() {
        let mut cursor = ListCursor::default();
        cursor.set(9, 10);

        // Not 8: the cursor was effectively on row 2 of a 3-row list.
        cursor.move_by(-1, 3);
        assert_eq!(cursor.index(3), 1);
    }

    #[test]
    fn a_half_page_always_moves_at_least_one_row() {
        assert_eq!(ListCursor::half_page_delta(1, 10), 5);
        assert_eq!(ListCursor::half_page_delta(1, 1), 1);
        assert_eq!(ListCursor::half_page_delta(1, 0), 1);
    }

    #[test]
    fn a_half_page_upwards_is_the_same_step_negated() {
        assert_eq!(ListCursor::half_page_delta(-1, 10), -5);
        assert_eq!(ListCursor::half_page_delta(-1, 0), -1);
    }
}
