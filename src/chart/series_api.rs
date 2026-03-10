//! TradingView-compatible Series, TimeScale, PriceScale, and Study API traits.
//!
//! These traits mirror the TradingView Lightweight Charts API surface, providing
//! programmatic access to coordinate conversion, bar data, pane management,
//! series options, and price/time scale configuration. They are designed for
//! use by plugin authors and scripting layers.
//!
//! # Key Traits
//!
//! - [`ISeriesApi`] -- coordinate conversion, bar data access, pane movement, options
//! - [`ITimeScaleApi`] -- visible range control, fit-content, subscriptions
//! - [`IPriceScaleApi`] -- price scale mode and options
//! - [`IStudyApi`] -- indicator/study lifecycle and visibility

use egui::Rect;

/// Trait for programmatic control of a chart series.
///
/// Mirrors the TradingView `ISeriesApi` interface, providing coordinate
/// conversion, bar data access, multi-pane support, and options management.
pub trait ISeriesApi {
    /// Get the series unique identifier
    fn series_id(&self) -> String;

    /// Get the series symbol
    fn symbol(&self) -> String;

    /// Get the series current price
    fn current_price(&self) -> Option<f64>;

    /// Get the series visible price range
    fn price_range(&self) -> (f64, f64);

    // =========================================================================
    // Coordinate Conversion (Critical for Pine Script and Trading)
    // =========================================================================

    /// Convert a price value to a Y coordinate in screen space
    ///
    /// This is essential for placing trading primitives (orders, positions)
    /// and for Pine Script overlays.
    ///
    /// # Arguments
    /// * `price` - The price to convert
    ///
    /// # Returns
    /// Y coordinate in screen space, or None if not visible
    fn price_to_coordinate(&self, price: f64) -> Option<f64>;

    /// Convert a Y coordinate to a price value
    ///
    /// Used for translating mouse positions to price values.
    ///
    /// # Arguments
    /// * `y` - Y coordinate in screen space
    ///
    /// # Returns
    /// Price value at that coordinate
    fn coordinate_to_price(&self, y: f64) -> Option<f64>;

    /// Convert a bar index to an X coordinate
    ///
    /// # Arguments
    /// * `bar_index` - The bar index (0-based from left)
    ///
    /// # Returns
    /// X coordinate in screen space
    fn bar_index_to_coordinate(&self, bar_index: usize) -> Option<f64>;

    /// Convert an X coordinate to a bar index
    ///
    /// # Arguments
    /// * `x` - X coordinate in screen space
    ///
    /// # Returns
    /// Bar index, or None if outside visible range
    fn coordinate_to_bar_index(&self, x: f64) -> Option<usize>;

    // =========================================================================
    // Bar Data Access
    // =========================================================================

    /// Get bars in a logical range
    ///
    /// # Arguments
    /// * `from` - Start logical index
    /// * `to` - End logical index (exclusive)
    ///
    /// # Returns
    /// Vector of bar data in the range
    fn bars_in_logical_range(&self, from: usize, to: usize) -> Vec<BarData>;

    /// Get the number of bars in the series
    fn bar_count(&self) -> usize;

    /// Get the first visible bar index
    fn first_visible_bar(&self) -> Option<usize>;

    /// Get the last visible bar index
    fn last_visible_bar(&self) -> Option<usize>;

    // =========================================================================
    // Pane Movement (Multi-Pane Support)
    // =========================================================================

    /// Move this series to a different pane
    ///
    /// # Arguments
    /// * `pane_index` - Target pane index (0-based)
    ///
    /// # Returns
    /// Ok(()) on success, Err if pane doesn't exist
    fn move_to_pane(&mut self, pane_index: usize) -> Result<(), String>;

    /// Merge this series with another pane
    ///
    /// This overlays the series on top of another pane.
    ///
    /// # Arguments
    /// * `pane_index` - Target pane index to merge with
    fn merge_with_pane(&mut self, pane_index: usize) -> Result<(), String>;

    /// Detach this series from its current pane
    ///
    /// Creates a new pane with just this series.
    ///
    /// # Returns
    /// The new pane index
    fn detach_pane(&mut self) -> usize;

    /// Get the current pane index
    fn current_pane(&self) -> usize;

    // =========================================================================
    // Series Options
    // =========================================================================

    /// Apply options to the series
    ///
    /// # Arguments
    /// * `options` - Series options to apply
    fn apply_options(&mut self, options: SeriesOptions);

    /// Get the current series options
    fn options(&self) -> SeriesOptions;

    /// Set series visibility
    fn set_visible(&mut self, visible: bool);

    /// Check if series is visible
    fn is_visible(&self) -> bool;

    /// Get the price scale associated with this series
    fn price_scale(&self) -> Box<dyn IPriceScaleApi>;
}

/// OHLCV bar data returned by [`ISeriesApi::bars_in_logical_range`].
#[derive(Debug, Clone)]
pub struct BarData {
    pub index: usize,
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Configuration options for a chart series (colors, widths, visibility).
#[derive(Debug, Clone, Default)]
pub struct SeriesOptions {
    /// Line color (for line series)
    pub line_color: Option<[u8; 4]>,
    /// Line width
    pub line_width: Option<f32>,
    /// Area top color (for area series)
    pub area_top_color: Option<[u8; 4]>,
    /// Area bottom color (for area series)
    pub area_bottom_color: Option<[u8; 4]>,
    /// Price line visibility
    pub price_line_visible: Option<bool>,
    /// Last value label visibility
    pub last_value_visible: Option<bool>,
    /// Title
    pub title: Option<String>,
}

/// Trait for programmatic control of a price (Y) axis scale.
pub trait IPriceScaleApi {
    /// Apply price scale options
    fn apply_options(&mut self, options: PriceScaleOptions);

    /// Get current price scale options
    fn options(&self) -> PriceScaleOptions;

    /// Get the width of the price scale in pixels
    fn width(&self) -> f32;

    /// Set the price scale mode (normal, log, percentage)
    fn set_mode(&mut self, mode: PriceScaleMode);

    /// Get current mode
    fn mode(&self) -> PriceScaleMode;
}

/// Configuration options for a price scale axis.
#[derive(Debug, Clone, Default)]
pub struct PriceScaleOptions {
    /// Auto scale
    pub auto_scale: Option<bool>,
    /// Mode
    pub mode: Option<PriceScaleMode>,
    /// Invert scale
    pub invert_scale: Option<bool>,
    /// Align labels
    pub align_labels: Option<bool>,
    /// Border visible
    pub border_visible: Option<bool>,
    /// Border color
    pub border_color: Option<[u8; 4]>,
}

/// Price scale arithmetic mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceScaleMode {
    /// Normal arithmetic scale
    #[default]
    Normal,
    /// Logarithmic scale
    Logarithmic,
    /// Percentage scale
    Percentage,
    /// Indexed to 100 scale
    IndexedTo100,
}

/// Trait for programmatic control of the time (X) axis scale.
pub trait ITimeScaleApi {
    /// Get visible logical range
    fn get_visible_logical_range(&self) -> LogicalRange;

    /// Set visible logical range
    fn set_visible_logical_range(&mut self, range: LogicalRange);

    /// Get visible time range
    fn get_visible_range(&self) -> (i64, i64);

    /// Set visible time range
    fn set_visible_range(&mut self, from: i64, to: i64);

    /// Fit content to view
    fn fit_content(&mut self);

    /// Scroll to real-time (right edge)
    fn scroll_to_real_time(&mut self);

    /// Reset time scale to default
    fn reset_time_scale(&mut self);

    /// Subscribe to visible time range changes
    fn subscribe_visible_time_range_change(&mut self, callback: Box<dyn Fn(i64, i64)>);

    /// Unsubscribe from visible time range changes
    fn unsubscribe_visible_time_range_change(&mut self);

    /// Subscribe to visible logical range changes
    fn subscribe_visible_logical_range_change(&mut self, callback: Box<dyn Fn(LogicalRange)>);

    /// Unsubscribe from visible logical range changes
    fn unsubscribe_visible_logical_range_change(&mut self);
}

/// A range expressed in logical bar indices (fractional).
#[derive(Debug, Clone, Copy)]
pub struct LogicalRange {
    pub from: f64,
    pub to: f64,
}

impl LogicalRange {
    pub fn new(from: f64, to: f64) -> Self {
        Self { from, to }
    }

    pub fn length(&self) -> f64 {
        self.to - self.from
    }
}

/// Trait for programmatic control of a study (technical indicator).
pub trait IStudyApi {
    /// Get study ID
    fn study_id(&self) -> String;

    /// Apply study options
    fn apply_options(&mut self, options: StudyOptions);

    /// Get study options
    fn options(&self) -> StudyOptions;

    /// Set study visibility
    fn set_visible(&mut self, visible: bool);

    /// Check if study is visible
    fn is_visible(&self) -> bool;

    /// Move study to a different pane
    fn move_to_pane(&mut self, pane_index: usize) -> Result<(), String>;

    /// Merge study with a pane (overlay)
    fn merge_with_pane(&mut self, pane_index: usize) -> Result<(), String>;

    /// Detach study to its own pane
    fn detach_pane(&mut self) -> usize;

    /// Remove study
    fn remove(&mut self);
}

/// Configuration options for a study (inputs, styles, visibility, pane).
#[derive(Debug, Clone, Default)]
pub struct StudyOptions {
    /// Study visible
    pub visible: Option<bool>,
    /// Pane index
    pub pane_index: Option<usize>,
    /// Input values
    pub inputs: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Style overrides
    pub styles: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Default implementation of [`ISeriesApi`] for the main chart series.
pub struct SeriesApiImpl {
    chart_rect: Rect,
    price_range: (f64, f64),
    bar_count: usize,
    visible_range: (usize, usize),
    current_pane: usize,
    options: SeriesOptions,
    visible: bool,
}

impl SeriesApiImpl {
    pub fn new(chart_rect: Rect, price_range: (f64, f64), bar_count: usize) -> Self {
        Self {
            chart_rect,
            price_range,
            bar_count,
            visible_range: (0, bar_count),
            current_pane: 0,
            options: SeriesOptions::default(),
            visible: true,
        }
    }

    fn price_to_y(&self, price: f64) -> f64 {
        let (min_price, max_price) = self.price_range;
        if max_price == min_price {
            return self.chart_rect.center().y as f64;
        }
        let price_ratio = (price - min_price) / (max_price - min_price);

        self.chart_rect.max.y as f64 - price_ratio * self.chart_rect.height() as f64
    }

    fn y_to_price(&self, y: f64) -> f64 {
        let (min_price, max_price) = self.price_range;
        let y_ratio = (self.chart_rect.max.y as f64 - y) / self.chart_rect.height() as f64;
        min_price + y_ratio * (max_price - min_price)
    }
}

impl ISeriesApi for SeriesApiImpl {
    fn series_id(&self) -> String {
        "main".to_string()
    }

    fn symbol(&self) -> String {
        "SYMBOL".to_string()
    }

    fn current_price(&self) -> Option<f64> {
        None
    }

    fn price_range(&self) -> (f64, f64) {
        self.price_range
    }

    fn price_to_coordinate(&self, price: f64) -> Option<f64> {
        if price < self.price_range.0 || price > self.price_range.1 {
            return None;
        }
        Some(self.price_to_y(price))
    }

    fn coordinate_to_price(&self, y: f64) -> Option<f64> {
        if y < self.chart_rect.min.y as f64 || y > self.chart_rect.max.y as f64 {
            return None;
        }
        Some(self.y_to_price(y))
    }

    fn bar_index_to_coordinate(&self, bar_index: usize) -> Option<f64> {
        if bar_index >= self.bar_count {
            return None;
        }
        let bar_width =
            self.chart_rect.width() as f64 / (self.visible_range.1 - self.visible_range.0) as f64;
        let x = self.chart_rect.min.x as f64
            + (bar_index - self.visible_range.0) as f64 * bar_width
            + bar_width / 2.0;
        Some(x)
    }

    fn coordinate_to_bar_index(&self, x: f64) -> Option<usize> {
        if x < self.chart_rect.min.x as f64 || x > self.chart_rect.max.x as f64 {
            return None;
        }
        let bar_width =
            self.chart_rect.width() as f64 / (self.visible_range.1 - self.visible_range.0) as f64;
        let bar_index =
            ((x - self.chart_rect.min.x as f64) / bar_width) as usize + self.visible_range.0;
        if bar_index >= self.bar_count {
            return None;
        }
        Some(bar_index)
    }

    /// Returns an empty vec by default. Override in your DataSource implementation.
    fn bars_in_logical_range(&self, _from: usize, _to: usize) -> Vec<BarData> {
        Vec::new()
    }

    fn bar_count(&self) -> usize {
        self.bar_count
    }

    fn first_visible_bar(&self) -> Option<usize> {
        Some(self.visible_range.0)
    }

    fn last_visible_bar(&self) -> Option<usize> {
        Some(self.visible_range.1.min(self.bar_count.saturating_sub(1)))
    }

    fn move_to_pane(&mut self, pane_index: usize) -> Result<(), String> {
        self.current_pane = pane_index;
        Ok(())
    }

    fn merge_with_pane(&mut self, pane_index: usize) -> Result<(), String> {
        log::info!("Merging series with pane {pane_index}");
        Ok(())
    }

    fn detach_pane(&mut self) -> usize {
        let new_pane = self.current_pane + 1;
        self.current_pane = new_pane;
        new_pane
    }

    fn current_pane(&self) -> usize {
        self.current_pane
    }

    fn apply_options(&mut self, options: SeriesOptions) {
        self.options = options;
    }

    fn options(&self) -> SeriesOptions {
        self.options.clone()
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn price_scale(&self) -> Box<dyn IPriceScaleApi> {
        Box::new(PriceScaleApiImpl::default())
    }
}

/// Default implementation of [`IPriceScaleApi`].
#[derive(Default)]
pub struct PriceScaleApiImpl {
    options: PriceScaleOptions,
}

impl IPriceScaleApi for PriceScaleApiImpl {
    fn apply_options(&mut self, options: PriceScaleOptions) {
        self.options = options;
    }

    fn options(&self) -> PriceScaleOptions {
        self.options.clone()
    }

    fn width(&self) -> f32 {
        60.0
    }

    fn set_mode(&mut self, mode: PriceScaleMode) {
        self.options.mode = Some(mode);
    }

    fn mode(&self) -> PriceScaleMode {
        self.options.mode.unwrap_or(PriceScaleMode::Normal)
    }
}
