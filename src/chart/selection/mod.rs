//! Generic selection state for chart elements.
//!
//! Provides a shared selection pattern used by series, indicators, and other
//! selectable chart elements.
//!
//! The whole chart tracks a single selection through one
//! `SelectionState`. The [`ChartElementId`] enum distinguishes
//! the kinds of selectable things (a price/volume series or a study, drawn on
//! the main chart or in a pane below it), so one selection model can express
//! "whatever the user last clicked" regardless of which layer it lives on.

/// Identifier shared by bar series, the volume series and studies.
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
/// Read the chart's current selection with [`Chart::selected_element`](crate::Chart::selected_element).
/// Series and studies can be selected in the main pane or an additional pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChartElementId {
    /// A bar or volume series.
    Series(SeriesId),
    /// A study, identified by the id [`Chart::add_indicator`](crate::Chart::add_indicator)
    /// returned.
    Indicator(SeriesId),
}

/// Selection and bar index shared by series and studies.
#[derive(Clone, Debug, Default)]
pub(crate) struct SelectionState {
    /// Currently selected element
    pub selected: Option<ChartElementId>,
    /// Additional selection metadata (bar index, etc.)
    pub bar_idx: Option<usize>,
}

impl SelectionState {
    /// Create new empty selection state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Select an element.
    pub fn select(&mut self, id: ChartElementId, bar_idx: Option<usize>) {
        self.selected = Some(id);
        self.bar_idx = bar_idx;
    }

    /// Deselect all elements.
    pub fn deselect(&mut self) {
        self.selected = None;
        self.bar_idx = None;
    }

    /// Get the currently selected element ID.
    pub fn selected_id(&self) -> Option<ChartElementId> {
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

    #[test]
    fn test_select_deselect() {
        let mut state = SelectionState::new();
        assert_eq!(state.selected_id(), None);

        state.select(ChartElementId::Series(SeriesId::MAIN), Some(42));
        assert_eq!(
            state.selected_id(),
            Some(ChartElementId::Series(SeriesId::MAIN))
        );
        assert_eq!(state.selected_bar(), Some(42));

        state.deselect();
        assert_eq!(state.selected_id(), None);
        assert_eq!(state.selected_bar(), None);
    }
}
