//! The history chart's state machine.
//!
//! The chart plots one metric at a time — ten series on one axis would need
//! ten different units on it — so the only state this view has is which metric
//! that is, and the motions that change it.

use crate::sensors::Metric;

use super::{TuiApp, View};

impl TuiApp {
    pub fn open_history(&mut self) {
        self.view = View::History;
    }

    pub fn close_history(&mut self) {
        self.view = View::Dashboard;
    }

    /// The metric currently plotted, with its live reading.
    ///
    /// The cursor is clamped rather than trusted, so a metric list that grows
    /// or shrinks between frames cannot index past the end.
    pub fn history_metric(&self) -> Metric {
        let metrics = self.metrics();
        let index = self.history_cursor.min(metrics.len().saturating_sub(1));

        metrics
            .iter()
            .nth(index)
            .cloned()
            .unwrap_or_else(|| metrics.aqi().clone())
    }

    /// The plotted metric's recorded values, oldest first.
    pub fn history_series(&self) -> Vec<Option<f64>> {
        self.metric_history(self.history_metric().key)
    }

    pub fn history_next_metric(&mut self) {
        self.move_history_cursor(1);
    }

    pub fn history_previous_metric(&mut self) {
        self.move_history_cursor(-1);
    }

    /// `gg` — back to the first metric, which is AQI.
    pub fn history_first_metric(&mut self) {
        self.history_cursor = 0;
    }

    /// `G` — the last metric in the list.
    pub fn history_last_metric(&mut self) {
        self.history_cursor = self.metrics().len().saturating_sub(1);
    }

    /// `<C-d>` / `<C-u>`. The list is short, so half of it is the useful step —
    /// the same rule the theme picker and config editor use.
    pub fn history_half_page(&mut self, direction: i32) {
        let half = (self.metrics().len() / 2).max(1) as i32;
        self.move_history_cursor(direction * half);
    }

    /// Selects a metric by index, e.g. from a click on the chart's metric list.
    pub fn select_history_metric_index(&mut self, index: usize) {
        self.history_cursor = index.min(self.metrics().len().saturating_sub(1));
    }

    fn move_history_cursor(&mut self, delta: i32) {
        let last = self.metrics().len().saturating_sub(1) as i32;
        let next = (self.history_cursor as i32 + delta).clamp(0, last);
        self.history_cursor = next as usize;
    }
}
