//! Core indicator trait and value types.
//!
//! This module defines the [`Indicator`] trait that every technical indicator
//! (built-in or custom) must implement, as well as the [`IndicatorValue`]
//! enum used to represent computed output at each bar.

use crate::model::Bar;
use egui::Color32;

/// The output value of an indicator at a single bar position.
///
/// Indicators produce one `IndicatorValue` per input [`Bar`]. The variant
/// depends on the indicator:
///
/// - **`Single(f64)`** -- used by single-line indicators (SMA, EMA, RSI, ATR, ...).
/// - **`Multiple(Vec<f64>)`** -- used by multi-line indicators. For example,
///   MACD emits `[macd_line, signal_line, histogram]` and Bollinger Bands
///   emits `[upper, middle, lower]`.
/// - **`None`** -- emitted for bars that fall within the indicator's warmup
///   period (i.e. not enough prior data to compute a value).
#[derive(Debug, Clone)]
pub enum IndicatorValue {
    /// A single computed value (e.g. SMA output at one bar).
    Single(f64),
    /// Multiple computed values for multi-output indicators (e.g. MACD
    /// producing MACD line, signal line, and histogram).
    Multiple(Vec<f64>),
    /// No value available, typically during the warmup period.
    None,
}

/// The core trait that every technical indicator must implement.
///
/// The trait defines the full indicator lifecycle: naming, calculation,
/// value access, color management, and display properties. It is
/// object-safe so indicators can be stored as `Box<dyn Indicator>` in
/// the [`IndicatorRegistry`](super::IndicatorRegistry).
///
/// # Implementors
///
/// All 150+ built-in indicators implement this trait (e.g. [`SMA`](super::SMA),
/// [`RSI`](super::RSI), [`MACD`](super::MACD)), and users can implement it
/// for their own indicators or use [`CustomIndicator`](super::CustomIndicator)
/// to define one at runtime via a closure.
///
/// # Lifecycle
///
/// 1. Construct the indicator with desired parameters.
/// 2. Call [`calculate`](Indicator::calculate) with bar data.
/// 3. Read results via [`values`](Indicator::values).
/// 4. The rendering layer queries [`is_overlay`](Indicator::is_overlay),
///    [`colors`](Indicator::colors), [`line_names`](Indicator::line_names),
///    etc. for drawing.
pub trait Indicator: Send {
    /// Return the short display name of this indicator (e.g. `"SMA"`, `"RSI"`, `"MACD"`).
    fn name(&self) -> &str;

    /// Return a human-readable description of what this indicator measures.
    ///
    /// Defaults to an empty string. Override to provide a tooltip-friendly
    /// summary such as `"Relative Strength Index - Momentum oscillator (0-100)"`.
    fn desc(&self) -> &str {
        ""
    }

    /// Compute the indicator's output from the given bar data.
    ///
    /// After this call, [`values`](Indicator::values) returns one
    /// [`IndicatorValue`] per input bar. Implementations should clear
    /// any previous state so the indicator can be recalculated on new data.
    fn calculate(&mut self, data: &[Bar]);

    /// Return the slice of computed values (one per input bar).
    ///
    /// The length of the returned slice matches the length of the `data`
    /// slice most recently passed to [`calculate`](Indicator::calculate).
    fn values(&self) -> &[IndicatorValue];

    /// Return the colors used for rendering this indicator's lines.
    ///
    /// Single-line indicators return a `Vec` with one element; multi-line
    /// indicators return one color per line (e.g. MACD returns three).
    fn colors(&self) -> Vec<Color32>;

    /// Replace all line colors at once.
    ///
    /// For single-line indicators, pass a `Vec` with one color.
    /// For multi-line indicators, pass one color per line.
    fn set_colors(&mut self, colors: Vec<Color32>);

    /// Set the color of a single line by its zero-based index.
    ///
    /// This is a convenience wrapper around [`colors`](Indicator::colors)
    /// and [`set_colors`](Indicator::set_colors). If `line_idx` is out of
    /// range the call is silently ignored.
    fn set_color(&mut self, line_idx: usize, color: Color32) {
        let mut colors = self.colors();
        if line_idx < colors.len() {
            colors[line_idx] = color;
            self.set_colors(colors);
        }
    }

    /// Whether this indicator is drawn on the main price chart (overlay)
    /// or in a separate sub-pane.
    ///
    /// Returns `true` for overlays (e.g. SMA, Bollinger Bands, Ichimoku),
    /// `false` for oscillators and volume indicators that need their own
    /// Y-axis (e.g. RSI, MACD, OBV). Defaults to `true`.
    fn is_overlay(&self) -> bool {
        true
    }

    /// The number of lines this indicator plots.
    ///
    /// For example, SMA returns `1`, MACD returns `3` (MACD line, signal,
    /// histogram), and Ichimoku returns `5`. Defaults to `1`.
    fn line_cnt(&self) -> usize {
        1
    }

    /// Return display names for each line, used in chart legends.
    ///
    /// The length of the returned `Vec` should match [`line_cnt`](Indicator::line_cnt).
    /// Defaults to a single entry containing [`name`](Indicator::name).
    fn line_names(&self) -> Vec<String> {
        vec![self.name().to_string()]
    }

    /// Whether this indicator should currently be rendered.
    ///
    /// Defaults to `true`. Toggle with [`set_visible`](Indicator::set_visible).
    fn is_visible(&self) -> bool {
        true
    }

    /// Show or hide this indicator.
    fn set_visible(&mut self, visible: bool);

    /// Clone this indicator into a new `Box<dyn Indicator>`.
    ///
    /// This enables cloning of heterogeneous indicator collections.
    fn clone_box(&self) -> Box<dyn Indicator>;
}

/// Blanket `Clone` implementation for boxed indicators, delegating to
/// [`Indicator::clone_box`].
impl Clone for Box<dyn Indicator> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
