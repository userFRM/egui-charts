//! Host-fed bar series and their shared price scales.

use crate::chart::rendering;
use crate::data::FeedState;
use crate::model::{
    AreaStyle, Bar, BarsStyle, BaselineStyle, ChartType, ChartTypeParams, ChartTypeStyles,
    LastPriceAnimation, LinePlotStyle, LineStyle, Marker, MarkerOptions, PriceLine, SeriesScale,
    SeriesStyle,
};
use crate::scales::StudyFormat;
use crate::scales::{PriceMap, PriceScale, PriceScaleMode};
use crate::services::SymbolExpression;
use crate::studies::{Indicator, IndicatorValues, PlotStyle, PlotType, StudyDataRequest};
use crate::tokens::DESIGN_TOKENS;
use crate::widget::{Chart, ChartElementId, PaneId, SeriesId};
use egui::Color32;
use serde::{Deserialize, Serialize};

/// The spread a series name spells, such as `AAPL-MSFT`; `None` for a plain symbol.
fn spread_of(name: &str) -> Option<SymbolExpression> {
    SymbolExpression::parse(name)
        .ok()
        .filter(SymbolExpression::is_spread)
}

/// The symbols the feed supplies for a series name: the symbol itself, or a spread's legs.
fn fed_symbols(name: &str) -> Vec<String> {
    match SymbolExpression::parse(name) {
        Ok(expression) => expression
            .symbols()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        Err(_) => vec![name.to_owned()],
    }
}

/// Identifies a price axis within a pane, as returned by [`Chart::pane_scales`]; see
/// [`Chart::pane_scale`] for the scale behind it.
/// Shared assignments identify one scale; independently mapped studies identify their own axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneScale {
    /// A shared bar-series or study assignment.
    Named(SeriesScale),
    /// An independently mapped study.
    Study(SeriesId),
}

/// Display settings of a host-fed bar series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesOptions {
    /// Host-supplied title, displayed beside the series values.
    pub name: String,
    /// How the bars are drawn. The types that replace the time axis with their own elements
    /// (range bars, Renko, Kagi, Line Break, Point and Figure) and TPO profiles are drawn as
    /// [`ChartType::Line`] and read back as such.
    pub chart_type: ChartType,
    /// Colour of the line, step, marker, area and high-low types, the legend, and the last-value
    /// tag and price line of the high-low type and the types drawn from the close.
    pub color: Color32,
    /// Rising bars of the OHLC and column types, and the baseline type above its base. High-low
    /// bars do not use it.
    pub up_color: Color32,
    /// Falling bars of the OHLC and column types, and the baseline type below its base. High-low
    /// bars do not use it.
    pub down_color: Color32,
    /// Candle border width in logical points, drawn inside the body. Defaults to zero;
    /// non-positive or non-finite widths disable borders, including per-bar border colours.
    #[serde(default)]
    pub candle_border_width: f32,
    /// Line, Step line and Line with markers style. `None` uses `color` and `width`,
    /// with straight, solid segments. Also supplies the Area line without `area_style`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<LinePlotStyle>,
    /// OHLC open ticks and stroke thickness, independent of the main series.
    #[serde(default)]
    pub bars_style: BarsStyle,
    /// Area line and gradient style. `None` uses `line_style` (or `color` and `width`),
    /// a fill in `color` fading to transparent, a relative gradient and downward fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_style: Option<AreaStyle>,
    /// Pulse at the last valued bar of a line-based series, independently of other series.
    /// The first displayed history starts no update pulse; a changed last time or close does.
    #[serde(default)]
    pub last_price_animation: LastPriceAnimation,
    /// Independent baseline line and fill styles. `None` uses `up_color`, `down_color` and
    /// `width`, with flat fills at the default opacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_style: Option<BaselineStyle>,
    /// Line width in logical pixels.
    pub width: f32,
    /// Point marker radius in logical points for Line with markers, Area and Baseline.
    /// Overrides `area_style`'s and `baseline_style`'s radii. `None` keeps their radius or the chart
    /// type's default: 3 for Line with markers, hidden for Area and Baseline.
    /// Non-positive or non-finite radii hide markers.
    #[serde(default)]
    pub marker_radius: Option<f32>,
    /// Crosshair marker radius in logical points for Line, Step line, Line with markers,
    /// Area and Baseline. Non-positive or non-finite radii hide it, the default.
    #[serde(default)]
    pub crosshair_marker_radius: f32,
    /// Crosshair marker fill, including opacity. `None` uses the hovered point's line colour.
    #[serde(default)]
    pub crosshair_marker_fill: Option<Color32>,
    /// Crosshair marker border colour. `None` uses the chart background.
    #[serde(default)]
    pub crosshair_marker_border_color: Option<Color32>,
    /// Crosshair marker border width in logical points. Non-positive or non-finite widths
    /// hide the border; the default is the design-token thick stroke.
    #[serde(default = "default_crosshair_marker_border_width")]
    pub crosshair_marker_border_width: f32,
    /// Scale shared with members carrying the same assignment in this series' pane.
    /// Assignments in different panes have independent options and ranges.
    pub scale: SeriesScale,
    /// The price [`ChartType::Columns`] rise above or fall below, which the scale's fit then
    /// includes; `None` raises them from the pane bottom.
    pub columns_base: Option<f64>,
    /// Numeric display format, last value label, price line, base line and autoscale. By
    /// default the series labels its last value without a price line.
    pub style: SeriesStyle,
}

fn default_crosshair_marker_border_width() -> f32 {
    DESIGN_TOKENS.stroke.thick
}

impl Default for SeriesOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            chart_type: ChartType::Line,
            color: Color32::from_rgb(41, 98, 255),
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
            candle_border_width: 0.0,
            line_style: None,
            bars_style: BarsStyle::default(),
            area_style: None,
            last_price_animation: LastPriceAnimation::Disabled,
            baseline_style: None,
            width: 2.0,
            marker_radius: None,
            crosshair_marker_radius: 0.0,
            crosshair_marker_fill: None,
            crosshair_marker_border_color: None,
            crosshair_marker_border_width: default_crosshair_marker_border_width(),
            scale: SeriesScale::Right,
            columns_base: None,
            style: SeriesStyle {
                price_line_visible: false,
                ..SeriesStyle::default()
            },
        }
    }
}

impl SeriesOptions {
    fn sanitized(mut self) -> Self {
        if !self.width.is_finite() || self.width <= 0.0 {
            self.width = 2.0;
        }
        if !self.candle_border_width.is_finite() || self.candle_border_width < 0.0 {
            self.candle_border_width = 0.0;
        }
        if self.chart_type.is_time_independent()
            || self.chart_type == ChartType::TimePriceOpportunity
        {
            self.chart_type = ChartType::Line;
        }
        self
    }

    /// Explicit per-type styles, with the series colours and widths as fallbacks.
    pub(super) fn type_params(&self) -> ChartTypeParams {
        let line = |color| LinePlotStyle {
            color,
            width: self.width,
            style: LineStyle::Solid,
            ..Default::default()
        };
        let fill = |color: Color32, alpha| {
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
        };
        let mut baseline = self
            .baseline_style
            .clone()
            .unwrap_or_else(|| BaselineStyle {
                above: line(self.up_color),
                below: line(self.down_color),
                above_fill: fill(self.up_color, 30),
                below_fill: fill(self.down_color, 30),
                ..Default::default()
            });
        if let Some(radius) = self.marker_radius {
            baseline.marker_radius = radius;
        }
        let line_style = self.line_style.clone().unwrap_or_else(|| line(self.color));
        let mut area = self.area_style.clone().unwrap_or_else(|| AreaStyle {
            line: line_style.clone(),
            top_fill: fill(self.color, 115),
            bottom_fill: Color32::TRANSPARENT,
            ..Default::default()
        });
        if let Some(radius) = self.marker_radius {
            area.marker_radius = radius;
        }
        ChartTypeParams {
            styles: ChartTypeStyles {
                line: line_style.clone(),
                step_line: line_style.clone(),
                line_with_markers: line_style,
                marker_radius: self.marker_radius.unwrap_or(3.0),
                area,
                hollow: crate::model::HollowStyle {
                    body_up: self.up_color,
                    body_down: self.down_color,
                    border_up: self.up_color,
                    border_down: self.down_color,
                    wick_up: self.up_color,
                    wick_down: self.down_color,
                    ..Default::default()
                },
                volume_candles: crate::model::VolumeCandlesStyle {
                    body_up: self.up_color,
                    body_down: self.down_color,
                    border_up: self.up_color,
                    border_down: self.down_color,
                    wick_up: self.up_color,
                    wick_down: self.down_color,
                    border_visible: self.candle_border_width > 0.0,
                    ..Default::default()
                },
                hlc_area: crate::model::HlcAreaStyle::with_colors(self.up_color, self.down_color),
                heikin: crate::model::HeikinStyle {
                    body_up: self.up_color,
                    body_down: self.down_color,
                    border_up: self.up_color,
                    border_down: self.down_color,
                    wick_up: self.up_color,
                    wick_down: self.down_color,
                    border_visible: self.candle_border_width > 0.0,
                    ..Default::default()
                },
                bars: self.bars_style.clone(),
                high_low: crate::model::HighLowStyle {
                    bar_color: self.color,
                    labels_visible: false,
                    label_color: self.color,
                    ..Default::default()
                },
                last_price_animation: self.last_price_animation,
                baseline,
                crosshair_marker_radius: self.crosshair_marker_radius,
                crosshair_marker_fill: self.crosshair_marker_fill,
                crosshair_marker_border_color: self.crosshair_marker_border_color,
                crosshair_marker_border_width: self.crosshair_marker_border_width,
                ..Default::default()
            },
            columns_base: self.columns_base,
            ..Default::default()
        }
    }
}

/// The base line of a percentage or indexed scale mapped by `map`, at 0% or 100.
pub(in crate::widget) fn base_line(map: &PriceMap, style: &SeriesStyle) -> Option<PriceLine> {
    let transform = map.transform();
    (style.base_line_visible
        && style.base_line_width.is_finite()
        && style.base_line_width > 0.0
        && matches!(
            transform.mode,
            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
        ))
    .then(|| {
        PriceLine::new(transform.base, style.base_line_color)
            .with_width(style.base_line_width)
            .with_style(style.base_line_style)
            .with_axis_label(false)
    })
}

pub(in crate::widget) struct PriceSeries {
    pub id: SeriesId,
    pub options: SeriesOptions,
    pub visible: bool,
    /// Feed ownership travels with the series through undo and redo.
    pub fed: bool,
    pub bars: Vec<Bar>,
    /// Changes when bars other than the last one change, as the main bars' revision does.
    pub revision: u64,
    /// Last observed time and close, and the input time the pulse started.
    pub last_price_pulse: std::cell::Cell<Option<(chrono::DateTime<chrono::Utc>, f64, f64)>>,
    pub heikin_ashi: rendering::candles::HeikinAshiCache,
    pub feed: FeedState,
    pub markers: crate::widget::marks::TimeOrdered<Marker>,
    pub marker_options: MarkerOptions,
    pub change_marks: Option<crate::widget::change_marks::ChangeMarks>,
    pub lines: Vec<(crate::widget::PriceLineId, PriceLine)>,
}

impl PriceSeries {
    /// The prices of `bar` that the series draws, lowest first.
    pub(in crate::widget) fn extremes(&self, bar: &Bar) -> [f64; 2] {
        if self.options.chart_type.uses_ohlc() {
            [bar.low, bar.high]
        } else {
            [bar.close; 2]
        }
    }

    /// The last-value tag colour: the bar's own colour, or its direction for the OHLC types except
    /// high-low, whose bars share `color`.
    pub(in crate::widget) fn value_color(&self, bar: &Bar) -> Color32 {
        if let Some(color) = bar.colors.as_ref().and_then(|own| own.color) {
            return color;
        }
        match self.options.chart_type {
            ChartType::HighLow => self.options.color,
            kind if kind.uses_ohlc() && bar.close >= bar.open => self.options.up_color,
            kind if kind.uses_ohlc() => self.options.down_color,
            _ => self.options.color,
        }
    }
}

/// A host series as a member of a pane below or above the price pane, which supplies the
/// pane's legend row and its controls. The chart type renderer draws the bars, so the study
/// holds no values to plot; its output reports the close at the crosshair, or the last close,
/// at every bar.
#[derive(Clone)]
pub(in crate::widget) struct SeriesLegend {
    name: String,
    style: PlotStyle,
    close: Option<f64>,
    values: IndicatorValues,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    /// Decimals of the series' price scale, when it sets them.
    precision: Option<usize>,
    /// Groups the thousands of built-in prices, from the series' price scale.
    thousands_sep: Option<char>,
}

impl SeriesLegend {
    pub(in crate::widget) fn new(
        item: &PriceSeries,
        close: Option<f64>,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
        precision: Option<usize>,
        thousands_sep: Option<char>,
    ) -> Self {
        Self {
            name: item.options.name.clone(),
            style: PlotStyle {
                color: item.options.color,
                width: item.options.width,
                pattern: LineStyle::Solid,
                plot_type: PlotType::Line,
                visible: true,
            },
            close,
            formatter,
            precision,
            thousands_sep,
            values: IndicatorValues::default(),
        }
    }
}

impl Indicator for SeriesLegend {
    fn plot_style(&self, _line: usize) -> PlotStyle {
        self.style
    }

    fn plot_value(&self, _line: usize, _bar: usize) -> Option<f64> {
        self.close
    }

    fn format_value(&self, value: f64) -> String {
        self.formatter.as_ref().map_or_else(
            || {
                crate::scales::group_thousands(
                    StudyFormat::Price.format(value, self.precision),
                    self.thousands_sep,
                )
            },
            |formatter| formatter.format(value),
        )
    }

    fn value_format(&self) -> StudyFormat {
        StudyFormat::Price
    }

    fn precision(&self) -> Option<usize> {
        self.precision
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn calculate(&mut self, _data: &[Bar]) {}

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.style.color]
    }

    fn set_colors(&mut self, _colors: Vec<Color32>) {}

    fn line_names(&self) -> Vec<String> {
        vec![self.name.clone()]
    }

    fn set_visible(&mut self, _visible: bool) {}

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

impl Chart {
    /// Adds a bar series of any time-based chart type, with independent historical and
    /// streaming bars, drawn above the main series in the price pane. Supply data through
    /// [`set_data`](Self::set_data) and [`update`](Self::update); the host owns feeds. Times
    /// are projected onto the main series' time axis, including its whitespace. Move the
    /// series to another pane with [`move_series_to_pane`](Self::move_series_to_pane).
    /// Without the original price pane, it joins the surviving main series, or the first
    /// remaining pane when the main series is absent.
    pub fn add_series(&mut self, options: SeriesOptions) -> SeriesId {
        let options = options.sanitized();
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.ensure_series_scale(options.scale);
        self.price_series.push(PriceSeries {
            id,
            options,
            visible: true,
            fed: false,
            bars: Vec::new(),
            revision: crate::model::next_revision(),
            last_price_pulse: std::cell::Cell::new(None),
            heikin_ashi: Default::default(),
            feed: FeedState::default(),
            markers: Default::default(),
            marker_options: MarkerOptions::default(),
            change_marks: None,
            lines: Vec::new(),
        });
        if !self.panes().contains(&PaneId::MAIN) {
            self.sync_panes();
        }
        self.host_events
            .push(crate::widget::ChartEvent::SeriesAdded(id));
        id
    }

    /// Replaces the display settings of an added bar series, including its chart type, style
    /// and scale assignment, as [`add_series`](Self::add_series) accepts them. Returns `false`
    /// for an unknown id. The main series is styled through
    /// [`set_chart_type`](Self::set_chart_type),
    /// [`set_chart_type_params`](Self::set_chart_type_params), the colours of
    /// [`config_mut`](Self::config_mut) and [`series_style_mut`](Self::series_style_mut), and
    /// assigned with [`set_series_scale`](Self::set_series_scale).
    pub fn set_series_options(&mut self, id: SeriesId, options: SeriesOptions) -> bool {
        let Some(at) = self.price_series.iter().position(|item| item.id == id) else {
            return false;
        };
        let options = options.sanitized();
        self.ensure_series_scale(options.scale);
        let renamed = self.price_series[at].options.name != options.name;
        self.price_series[at].options = options;
        self.series_price_scale_mut(id);
        if renamed && self.is_fed(id) {
            self.set_data(id, Vec::new());
            self.fill_fed_series();
        }
        self.prune_series_scales();
        true
    }

    /// Whether the chart's feed supplies the bars of a series, as for
    /// [`add_feed_series`](Self::add_feed_series).
    pub fn is_fed(&self, id: SeriesId) -> bool {
        self.feed_link.is_some()
            && self
                .price_series
                .iter()
                .any(|item| item.id == id && item.fed)
    }

    /// The fed series that are still present, with the names they are fed for.
    pub(in crate::widget) fn fed_series(&self) -> Vec<(SeriesId, String)> {
        self.price_series
            .iter()
            .filter(|item| self.is_fed(item.id))
            .map(|item| (item.id, item.options.name.clone()))
            .collect()
    }

    /// The histories the feed supplies for the fed series, at the chart's interval.
    pub(in crate::widget) fn fed_requests(&self) -> Vec<StudyDataRequest> {
        let mut requests = Vec::new();
        for (_, name) in self.fed_series() {
            for symbol in fed_symbols(&name) {
                let request = StudyDataRequest {
                    symbol,
                    timeframe: Some(self.timeframe().unwrap_or_default()),
                    metric: None,
                };
                if !requests.contains(&request) {
                    requests.push(request);
                }
            }
        }
        requests
    }

    /// Replaces the bars of the fed series with what the feed supplied for their symbols.
    pub(in crate::widget) fn fill_fed_series(&mut self) {
        let fed = self.fed_series();
        if fed.is_empty() {
            return;
        }
        let updates: Vec<_> = {
            let data =
                self.study_data
                    .data(self.fed_requests(), self.symbol(), self.timeframe(), None);
            let history = |symbol: &str| {
                data.iter()
                    .find(|data| data.request.symbol == symbol)
                    .map(|data| data.bars)
            };
            fed.iter()
                .map(|(id, name)| {
                    let bars = match spread_of(name) {
                        Some(spread) => spread.bars(history).unwrap_or_default(),
                        None => fed_symbols(name)
                            .first()
                            .and_then(|symbol| history(symbol))
                            .unwrap_or_default()
                            .to_vec(),
                    };
                    (*id, bars)
                })
                .collect()
        };
        for (id, bars) in updates {
            if self.bars(id) != bars.as_slice() {
                self.set_data(id, bars);
            }
        }
    }

    /// The bar series in `pane`, back to front, excluding studies and absent series.
    fn ordered_bar_series(&self, pane: PaneId) -> Vec<SeriesId> {
        let items = if pane == PaneId::MAIN {
            self.main_pane_order(self.pane_layout.main.clone())
        } else {
            self.pane_series(pane)
        };
        items
            .into_iter()
            .filter(|id| self.series_scale(*id).is_some())
            .collect()
    }

    /// The zero-based position of a bar series among the bar series in its pane, back to
    /// front. Includes the main series and hidden series; excludes studies. A later series
    /// covers an earlier one. Returns `None` for an absent series or a study.
    pub fn series_order(&self, id: SeriesId) -> Option<usize> {
        self.ordered_bar_series(self.series_pane(id)?)
            .iter()
            .position(|item| *item == id)
    }

    /// Moves a bar series, including the main series, to position `order` among the bar
    /// series in its pane, or to the last position for a larger value. Other panes keep their
    /// order. Returns `false` for an absent series or a study. The order survives snapshots;
    /// [`hovered_series_on_top`](crate::config::ChartOptions::hovered_series_on_top) can raise
    /// a hovered series for one frame without changing it.
    pub fn set_series_order(&mut self, id: SeriesId, order: usize) -> bool {
        let Some(pane) = self.series_pane(id) else {
            return false;
        };
        let mut series = self.ordered_bar_series(pane);
        let Some(at) = series.iter().position(|item| *item == id) else {
            return false;
        };
        let order = order.min(series.len() - 1);
        if at == order {
            return true;
        }
        self.sync_panes();
        let items = if pane == PaneId::MAIN {
            &mut self.pane_layout.main
        } else {
            &mut self
                .pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane)
                .expect("series belongs to pane")
                .members
        };
        items.retain(|item| *item != id);
        series.remove(at);
        let to = series
            .get(order)
            .and_then(|next| items.iter().position(|item| item == next))
            .unwrap_or_else(|| {
                series
                    .last()
                    .and_then(|last| items.iter().position(|item| item == last))
                    .map_or(items.len(), |at| at + 1)
            });
        items.insert(to, id);
        series.insert(order, id);

        // Keep series() in the same relative order without moving another pane's entries.
        let slots: Vec<_> = self
            .price_series
            .iter()
            .enumerate()
            .filter(|(_, item)| series.contains(&item.id))
            .map(|(at, _)| at)
            .collect();
        for (slot, id) in slots
            .into_iter()
            .zip(series.into_iter().filter(|id| *id != SeriesId::MAIN))
        {
            let at = self
                .price_series
                .iter()
                .position(|item| item.id == id)
                .unwrap();
            self.price_series.swap(slot, at);
        }
        true
    }

    /// Adds a bar series like [`add_series`](Self::add_series) whose bars the chart's feed
    /// supplies: the history and live updates of the symbol in `options.name`, at the chart's
    /// interval, or of a spread such as `AAPL-MSFT` (see [`SymbolExpression`]), computed from
    /// the history of each of its symbols. The series pages back as far as the main series, and
    /// a new name is a new symbol. A chart without a feed adds a plain series, which the host
    /// supplies through [`set_data`](Self::set_data); a symbol that contains an operator sign
    /// cannot be named.
    pub fn add_feed_series(&mut self, options: SeriesOptions) -> SeriesId {
        let id = self.add_series(options);
        self.price_series.last_mut().expect("added series").fed = self.feed_link.is_some();
        self.fill_fed_series();
        id
    }

    /// Adds a comparison line on the main series' scale and switches that scale to percent.
    /// Each series uses its own first visible close as zero; scrolling recomputes references.
    pub fn add_comparison(&mut self, name: impl Into<String>, color: Color32) -> SeriesId {
        self.state.price_scale_mut().options_mut().mode = PriceScaleMode::Percentage;
        self.state.price_scale_mut().reset();
        self.add_series(SeriesOptions {
            name: name.into(),
            color,
            scale: self.main_series_scale,
            ..Default::default()
        })
    }

    /// Added bar series and their display settings, in draw order.
    pub fn series(&self) -> impl Iterator<Item = (SeriesId, &SeriesOptions)> {
        self.price_series
            .iter()
            .map(|item| (item.id, &item.options))
    }

    /// Whether a bar series or study is present and visible.
    pub fn is_series_visible(&self, id: SeriesId) -> bool {
        if id == SeriesId::MAIN {
            self.main_series_present && self.main_series_visible
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == id) {
            item.visible
        } else {
            self.indicators()
                .any(|(current, study)| current == id && study.is_visible())
        }
    }

    /// Shows or hides a bar series or study without unloading its data.
    /// Returns `false` when the id is absent. Hidden series do not contribute to auto-scaling.
    pub fn set_series_visible(&mut self, id: SeriesId, visible: bool) -> bool {
        if id == SeriesId::MAIN {
            if !self.main_series_present {
                return false;
            }
            self.set_main_series_visible(visible);
        } else if let Some(item) = self.price_series.iter_mut().find(|item| item.id == id) {
            item.visible = visible;
            if !visible && self.selection.selected_id() == Some(ChartElementId::Series(id)) {
                self.selection.deselect();
            }
        } else {
            return self.set_indicator_visible(id, visible);
        }
        true
    }

    /// Assigns a bar series to the right, left, a hidden overlay or an additional labelled scale.
    /// Shared assignments share options and manual ranges. Other series retain their assignments.
    pub fn set_series_scale(&mut self, id: SeriesId, scale: SeriesScale) -> bool {
        if id == SeriesId::MAIN && self.main_series_present {
            if scale == self.main_series_scale {
                return true;
            }
            self.ensure_series_scale(scale);
            let next = self
                .series_scales
                .remove(&scale)
                .expect("scale was inserted");
            let old = std::mem::replace(self.state.price_scale_mut(), next);
            // Joining a scale preserves its bounds in scale units, even with a new reference.
            if self.state.price_scale().is_manual() {
                let range = self.state.price_scale().range();
                self.state.fit_price_scale(std::iter::empty());
                self.state.price_scale_mut().set_range(range.min, range.max);
            }
            self.series_scales.insert(self.main_series_scale, old);
            self.main_series_scale = scale;
            self.config.show_left_axis = scale == SeriesScale::Left;
            self.config.show_right_axis =
                matches!(scale, SeriesScale::Right | SeriesScale::Separate(_));
        } else if let Some(at) = self.price_series.iter().position(|item| item.id == id) {
            self.ensure_series_scale(scale);
            self.price_series[at].options.scale = scale;
            self.series_price_scale_mut(id);
        } else {
            return false;
        }
        self.prune_series_scales();
        true
    }

    /// The scale assignment of a present bar series.
    pub fn series_scale(&self, id: SeriesId) -> Option<SeriesScale> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(self.main_series_scale)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.options.scale)
        }
    }

    /// The numeric display format, last value label, price line, base line, autoscale and
    /// click tolerance of a present bar series.
    pub fn series_style(&self, id: SeriesId) -> Option<&SeriesStyle> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(&self.main_series_style)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map(|item| &item.options.style)
        }
    }

    /// Changes the numeric display format, last value label, price line, base line, autoscale
    /// or click tolerance of a present bar series. Changes take effect on the next rendered frame.
    pub fn series_style_mut(&mut self, id: SeriesId) -> Option<&mut SeriesStyle> {
        if id == SeriesId::MAIN && self.main_series_present {
            Some(&mut self.main_series_style)
        } else {
            self.price_series
                .iter_mut()
                .find(|item| item.id == id)
                .map(|item| &mut item.options.style)
        }
    }

    /// The scale at `assignment` in the original price pane, including scales shared only by studies.
    /// Returns `None` for an absent pane or secondary scale.
    /// For a series in another pane, use [`series_price_scale`](Self::series_price_scale).
    pub fn shared_price_scale(&self, assignment: SeriesScale) -> Option<&PriceScale> {
        if !self.panes().contains(&PaneId::MAIN) {
            return None;
        }
        (assignment == self.main_series_scale || self.series_scales.contains_key(&assignment))
            .then(|| self.pane_price_scale(PaneId::MAIN, assignment))
    }

    /// The scale shared by this present bar series, including its options and fitted range.
    pub fn series_price_scale(&self, id: SeriesId) -> Option<&PriceScale> {
        Some(self.pane_price_scale(self.series_pane(id)?, self.series_scale(id)?))
    }

    /// Changes options or a manual range on the scale of this bar series, as returned by
    /// [`series_price_scale`](Self::series_price_scale).
    /// Range bounds use scale units, such as percent in percentage mode.
    pub fn series_price_scale_mut(&mut self, id: SeriesId) -> Option<&mut PriceScale> {
        let pane = self.series_pane(id)?;
        let key = self.series_scale(id)?;
        Some(self.pane_price_scale_mut(pane, key))
    }

    /// The price mapping of a present bar series, the main one included, as last drawn in its
    /// pane: [`PriceMap::price_to_y`] and [`PriceMap::y_to_price`] convert between its prices
    /// and the y coordinates of the chart's [`Ui`](egui::Ui). `None` for an absent series, or
    /// before its pane is first drawn.
    pub fn series_price_map(&self, id: SeriesId) -> Option<PriceMap> {
        let pane = self.series_pane(id)?;
        if pane != self.main_series_pane() {
            return self
                .pane_layout
                .panes
                .iter()
                .find(|pane| Some(pane.id) == self.series_pane(id) && pane.rect.is_positive())
                .map(|pane| {
                    if let Some(item) = self.price_series.iter().find(|item| item.id == id) {
                        self.series_map(item, pane.rect)
                    } else {
                        let study = self
                            .indicators()
                            .find(|(study, _)| *study == id)
                            .expect("present study")
                            .1;
                        let mut coords = self.get_chart_mapping().with_rect(pane.rect);
                        let scale = self.pane_price_scale(pane.id, SeriesScale::Right);
                        coords.price =
                            scale.map(scale.transform(), pane.rect.top(), pane.rect.bottom());
                        self.pane_study_mapping(pane.id, study, &coords).price
                    }
                });
        }
        let rect = self.last_rendered_price_rect;
        if !rect.is_positive() {
            None
        } else if id == SeriesId::MAIN {
            Some(self.last_price_map)
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map(|item| self.series_map(item, rect))
                .or_else(|| {
                    let study = self.indicators().find(|(study, _)| *study == id)?.1;
                    Some(
                        crate::chart::rendering::study_mapping(study, &self.get_chart_mapping())
                            .price,
                    )
                })
        }
    }

    /// Formats `price` as the scale of a present bar series labels it: a price, a signed
    /// percentage or an indexed value. `None` where [`series_price_map`](Self::series_price_map)
    /// is.
    pub fn format_series_price(&self, id: SeriesId, price: f64) -> Option<String> {
        let map = self.series_price_map(id)?;
        let transform = map.transform();
        Some(map.label(
            transform.to_scale(price),
            map.price_precision().unwrap_or(2),
            self.series_label_formatter(id, transform.mode).as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_marker_override_preserves_explicit_gradient_style() {
        let style = BaselineStyle {
            marker_radius: 4.0,
            above_fill_at_base: Some(Color32::TRANSPARENT),
            below_fill_at_base: Some(Color32::GOLD),
            ..Default::default()
        };
        for radius in [None, Some(0.0), Some(7.0)] {
            let options = SeriesOptions {
                baseline_style: Some(style.clone()),
                marker_radius: radius,
                ..Default::default()
            };
            let mut expected = style.clone();
            expected.marker_radius = radius.unwrap_or(style.marker_radius);
            assert_eq!(options.type_params().styles.baseline, expected);
            assert_eq!(
                serde_json::from_value::<SeriesOptions>(serde_json::to_value(&options).unwrap())
                    .unwrap(),
                options
            );
        }
    }

    #[test]
    fn added_baseline_series_keep_explicit_styles_and_legacy_colours() {
        for gradient in [false, true] {
            let mut saved = serde_json::to_value(SeriesOptions::default()).unwrap();
            saved["up_color"] = serde_json::json!([0, 0, 255, 255]);
            saved["down_color"] = serde_json::json!([255, 0, 0, 255]);
            if gradient {
                saved["baseline_style"] = serde_json::json!({
                    "above_fill": [0, 200, 0, 200], "above_fill_at_base": [0, 40, 0, 40],
                    "below_fill": [200, 0, 0, 200], "below_fill_at_base": [40, 0, 0, 40]
                });
            }
            let options: SeriesOptions = serde_json::from_value(saved).unwrap();
            let params = options.type_params();
            if gradient {
                let actual = serde_json::to_value(&params.styles.baseline).unwrap();
                assert_eq!(actual["above_fill"], serde_json::json!([0, 200, 0, 200]));
                assert_eq!(
                    actual["above_fill_at_base"],
                    serde_json::json!([0, 40, 0, 40])
                );
                assert_eq!(actual["below_fill"], serde_json::json!([200, 0, 0, 200]));
                assert_eq!(
                    actual["below_fill_at_base"],
                    serde_json::json!([40, 0, 0, 40])
                );
            } else {
                assert_eq!(params.styles.baseline.above.color, Color32::BLUE);
                assert_eq!(params.styles.baseline.below.color, Color32::RED);
                assert_eq!(
                    params.styles.baseline.above_fill,
                    Color32::from_rgba_unmultiplied(0, 0, 255, 30)
                );
                assert_eq!(
                    params.styles.baseline.below_fill,
                    Color32::from_rgba_unmultiplied(255, 0, 0, 30)
                );
            }
            assert_eq!(
                serde_json::from_value::<SeriesOptions>(serde_json::to_value(&options).unwrap())
                    .unwrap(),
                options
            );
        }
    }
}
