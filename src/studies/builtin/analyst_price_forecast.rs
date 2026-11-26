//! Analyst price forecast.
//!
//! Shows the analyst price targets that the chart's data feed answers through
//! [`Datafeed::price_targets`](crate::data::Datafeed::price_targets): dotted projections of the
//! high, average and low estimates from the close on their publication date to their horizon
//! date, each labelled with its price and its change from the last close. The study computes
//! nothing from the bars, so neither output has values. A chart draws the targets while the
//! study is shown in the main series' pane, and its targets setting,
//! `Chart::set_price_targets_visible`, follows the study: adding the study turns it on, removing
//! the last one turns it off, and turning it on adds the study.
//!
//! Output 0, "Positive", styles the targets above the publication close and output 1,
//! "Negative", those below it. Each output's colour gives the lines and labels, drawn opaque,
//! and the area between the projections on its side, filled at the colour's opacity; hiding an
//! output hides that side. Width, line style and plot type do not apply: the projections stay
//! dotted. On a chart, the outputs take the colours of the chart configuration's
//! `price_target_positive_color` and `price_target_negative_color`, and editing them in the
//! study's settings inside `Chart::user_action`, as the study settings dialog does, changes
//! those.

use crate::model::{Bar, HlcAreaStyle};
use crate::studies::{Indicator, IndicatorValues, palette};
use egui::Color32;

/// Analyst price forecast: the data feed's analyst price targets, drawn by the chart.
///
/// ```
/// # #[cfg(feature = "widget")]
/// # {
/// use egui_charts::{ChartBuilder, studies::AnalystPriceForecast};
///
/// let mut chart = ChartBuilder::new().build();
/// chart.add_indicator(Box::new(AnalystPriceForecast::default()));
/// assert!(chart.price_targets_visible());
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct AnalystPriceForecast {
    positive: Color32,
    negative: Color32,
    values: IndicatorValues,
    visible: bool,
}

impl AnalystPriceForecast {
    /// The name the catalogue lists, a chart recognises and layouts save.
    pub const NAME: &'static str = "Analyst price forecast";
}

/// The theme-independent defaults: the built-in up and down colours at 20% opacity.
impl Default for AnalystPriceForecast {
    fn default() -> Self {
        Self {
            positive: HlcAreaStyle::fill_tint(palette::UP, 51),
            negative: HlcAreaStyle::fill_tint(palette::DOWN, 51),
            values: IndicatorValues::new(),
            visible: true,
        }
    }
}

impl Indicator for AnalystPriceForecast {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn desc(&self) -> &str {
        "Analyst price forecast - The data feed's analyst estimates, drawn from their publication date"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
    }

    // A calculation stores only the bar count, so recalculating does no work per bar.
    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.positive, self.negative]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.positive = color;
        }
        if let Some(&color) = colors.get(1) {
            self.negative = color;
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    // The chart draws the targets of its own symbol.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    // The two outputs style host projections and have no calculated values to read out.
    fn legend_lines(&self) -> Vec<usize> {
        Vec::new()
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Positive".to_string(), "Negative".to_string()]
    }

    // The chart draws the targets dotted at a fixed width, whatever the plot style.
    fn plot_color_only(&self, _line: usize) -> bool {
        true
    }
}
