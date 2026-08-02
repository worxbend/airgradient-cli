//! Click targets recorded while drawing a frame.
//!
//! Mouse events arrive as bare terminal cell coordinates, so something has to
//! map a cell back to the thing drawn there. Recording the rectangles during
//! the draw — rather than recomputing the layout when a click arrives — means
//! the hit regions cannot disagree with what is actually on screen, even as
//! the layout reflows with the terminal size.

use ratatui::layout::Rect;

/// Something clickable the renderer drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    /// A row in the theme picker, indexed into `theme::ALL`.
    ThemeRow(usize),
    /// A row in the config editor, indexed into `ConfigField::ALL`.
    ConfigRow(usize),
}

/// The click targets from the most recent frame.
#[derive(Debug, Default, Clone)]
pub struct HitMap {
    zones: Vec<(Rect, HitTarget)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.zones.clear();
    }

    pub fn push(&mut self, area: Rect, target: HitTarget) {
        self.zones.push((area, target));
    }

    /// The target drawn at this cell, if any.
    ///
    /// Later zones win: the renderer draws overlays after the content beneath
    /// them, so the last match is the one actually visible at that cell.
    pub fn hit(&self, column: u16, row: u16) -> Option<HitTarget> {
        self.zones
            .iter()
            .rev()
            .find(|(area, _)| {
                column >= area.x
                    && column < area.x.saturating_add(area.width)
                    && row >= area.y
                    && row < area.y.saturating_add(area.height)
            })
            .map(|(_, target)| *target)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn hit_matches_only_inside_the_recorded_rect() {
        let mut hits = HitMap::default();
        hits.push(rect(2, 3, 10, 1), HitTarget::ThemeRow(4));

        assert_eq!(hits.hit(2, 3), Some(HitTarget::ThemeRow(4)));
        assert_eq!(hits.hit(11, 3), Some(HitTarget::ThemeRow(4)));

        // One cell past each edge is a miss.
        assert_eq!(hits.hit(12, 3), None);
        assert_eq!(hits.hit(1, 3), None);
        assert_eq!(hits.hit(2, 4), None);
        assert_eq!(hits.hit(2, 2), None);
    }

    #[test]
    fn later_zones_win_so_overlays_beat_the_content_beneath_them() {
        let mut hits = HitMap::default();
        hits.push(rect(0, 0, 10, 10), HitTarget::ConfigRow(0));
        hits.push(rect(2, 2, 2, 2), HitTarget::ThemeRow(7));

        assert_eq!(hits.hit(2, 2), Some(HitTarget::ThemeRow(7)));
        assert_eq!(hits.hit(9, 9), Some(HitTarget::ConfigRow(0)));
    }

    #[test]
    fn clear_drops_the_previous_frames_zones() {
        let mut hits = HitMap::default();
        hits.push(rect(0, 0, 4, 4), HitTarget::ThemeRow(1));
        hits.clear();

        assert!(hits.is_empty());
        assert_eq!(hits.hit(1, 1), None);
    }

    #[test]
    fn zero_sized_rects_never_match() {
        let mut hits = HitMap::default();
        hits.push(rect(5, 5, 0, 1), HitTarget::ThemeRow(0));
        hits.push(rect(5, 6, 3, 0), HitTarget::ThemeRow(1));

        assert_eq!(hits.hit(5, 5), None);
        assert_eq!(hits.hit(5, 6), None);
    }
}
