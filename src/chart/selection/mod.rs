//! Generic selection state for chart elements.
//!
//! Provides a shared selection pattern used by series, indicators, and other
//! selectable chart elements.
//!
//! The whole chart tracks a single selection through one
//! `SelectionState<ChartElementId>`. The [`ChartElementId`] enum distinguishes
//! the kinds of selectable things (a price/volume series or a study, drawn on
//! the main chart or in a pane below it), so one selection model can express
//! "whatever the user last clicked" regardless of which layer it lives on.

use std::marker::PhantomData;

/// Unique numeric identifier for a chart series.
///
/// Well-known constants: [`MAIN`](Self::MAIN) (candlesticks/bars) and
/// [`VOLUME`](Self::VOLUME).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeriesId(pub usize);

impl SeriesId {
    /// Main chart series (candlesticks/bars)
    pub const MAIN: SeriesId = SeriesId(0);
    /// Volume series
    pub const VOLUME: SeriesId = SeriesId(1);

    /// Get display name for this series
    pub fn name(&self) -> &'static str {
        match self.0 {
            0 => "Main Series",
            1 => "Volume",
            _ => "Series",
        }
    }
}

/// Identifies any selectable element across the entire chart.
///
/// A single `SelectionState<ChartElementId>` tracks the one element the user
/// has clicked, whether it is a series or a study, drawn on the main chart
/// (SMA, EMA, …) or in a pane below it (RSI, MACD, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChartElementId {
    /// A price or volume series on the main chart.
    Series(SeriesId),
    /// A study, identified by the id [`Chart::add_indicator`](crate::Chart::add_indicator)
    /// returned.
    Indicator(SeriesId),
}

/// Trait bound for IDs that can be used with `SelectionState`.
///
/// Automatically implemented for any type that is `Copy + Eq + Debug`.
pub(crate) trait SelectableId: Copy + Eq + std::fmt::Debug {}

// Blanket implementation for types that meet the requirements
impl<T: Copy + Eq + std::fmt::Debug> SelectableId for T {}

/// Generic selection state for chart elements, parameterized by an ID type.
///
/// Tracks which element is selected and the bar index
/// where the selection occurred. Used by both series and indicator selection.
#[derive(Clone, Debug)]
pub(crate) struct SelectionState<Id: SelectableId> {
    /// Currently selected element
    pub selected: Option<Id>,
    /// Additional selection metadata (bar index, etc.)
    pub bar_idx: Option<usize>,
    _phantom: PhantomData<Id>,
}

impl<Id: SelectableId> Default for SelectionState<Id> {
    fn default() -> Self {
        Self {
            selected: None,
            bar_idx: None,
            _phantom: PhantomData,
        }
    }
}

impl<Id: SelectableId> SelectionState<Id> {
    /// Create new empty selection state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Select an element.
    pub fn select(&mut self, id: Id, bar_idx: Option<usize>) {
        self.selected = Some(id);
        self.bar_idx = bar_idx;
    }

    /// Deselect all elements.
    pub fn deselect(&mut self) {
        self.selected = None;
        self.bar_idx = None;
    }

    /// Get the currently selected element ID.
    pub fn selected_id(&self) -> Option<Id> {
        self.selected
    }

    /// Get the bar index where selection occurred.
    pub fn selected_bar(&self) -> Option<usize> {
        self.bar_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestId(usize);

    #[test]
    fn test_select_deselect() {
        let mut state = SelectionState::<TestId>::new();
        assert_eq!(state.selected_id(), None);

        state.select(TestId(1), Some(42));
        assert_eq!(state.selected_id(), Some(TestId(1)));
        assert_eq!(state.selected_bar(), Some(42));

        state.deselect();
        assert_eq!(state.selected_id(), None);
        assert_eq!(state.selected_bar(), None);
    }
}
