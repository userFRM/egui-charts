//! Series selection state for interactive chart elements.
//!
//! Manages which chart series (candles, lines, etc.) are currently
//! selected or hovered by the user.

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

/// Result of a hit test on chart series
#[derive(Clone, Debug)]
pub struct SeriesHitResult {
    /// The series that was hit
    pub series_id: SeriesId,
    /// Index of the bar that was hit
    pub bar_idx: usize,
    /// Screen position of the hit
    pub position: egui::Pos2,
}

/// Selection state for chart series
#[derive(Clone, Debug, Default)]
pub struct SeriesSelectionState {
    /// Currently selected series
    pub selected_series: Option<SeriesId>,
    /// Currently hovered series (desktop only)
    pub hovered_series: Option<SeriesId>,
    /// Index of selected bar within series
    pub selected_bar_idx: Option<usize>,
}

impl SeriesSelectionState {
    /// Create new selection state
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a series
    pub fn select(&mut self, series_id: SeriesId, bar_idx: Option<usize>) {
        self.selected_series = Some(series_id);
        self.selected_bar_idx = bar_idx;
    }

    /// Deselect all series
    pub fn deselect(&mut self) {
        self.selected_series = None;
        self.selected_bar_idx = None;
    }

    /// Set hovered series
    pub fn set_hovered(&mut self, series_id: Option<SeriesId>) {
        self.hovered_series = series_id;
    }

    /// Check if a series is selected
    pub fn is_selected(&self, series_id: SeriesId) -> bool {
        self.selected_series == Some(series_id)
    }

    /// Check if a series is hovered
    pub fn is_hovered(&self, series_id: SeriesId) -> bool {
        self.hovered_series == Some(series_id)
    }

    /// Check if main series is selected
    pub fn is_main_series_selected(&self) -> bool {
        self.is_selected(SeriesId::MAIN)
    }

    /// Check if volume series is selected
    pub fn is_volume_selected(&self) -> bool {
        self.is_selected(SeriesId::VOLUME)
    }

    /// Check if any series is selected
    pub fn has_selection(&self) -> bool {
        self.selected_series.is_some()
    }

    /// Get the currently selected series ID
    pub fn selected(&self) -> Option<SeriesId> {
        self.selected_series
    }
}
