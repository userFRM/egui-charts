//! Host-fed bar series and their shared price scales.

use super::{Chart, ChartElementId, SeriesId};
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::chart::rendering;
use crate::data::FeedState;
use crate::model::{
    AreaStyle, Bar, BaselineStyle, ChartType, ChartTypeParams, ChartTypeStyles, LinePlotStyle,
    LineStyle, Marker, PriceLine, PriceSource, SeriesScale,
};
use crate::scales::{PriceMap, PriceScale, PriceScaleMode, PriceScaleOptions, PriceTransform};
use crate::studies::{Indicator, IndicatorValues, PlotPattern, PlotStyle, PlotType, StudyFormat};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect};

/// Display settings of a host-fed bar series.
#[derive(Debug, Clone)]
pub struct SeriesOptions {
    /// Host-supplied title, displayed beside the series values.
    pub name: String,
    /// How the bars are drawn. The types that replace the time axis with their own elements
    /// (range bars, Renko, Kagi, Line Break, Point and Figure) and TPO profiles are drawn as
    /// [`ChartType::Line`] and read back as such.
    pub chart_type: ChartType,
    /// Colour of the line, step, marker and area types, the legend and the last-value tag of
    /// the types drawn from the close.
    pub color: Color32,
    /// Rising bars of the OHLC and column types, and the baseline type above its base.
    pub up_color: Color32,
    /// Falling bars of the OHLC and column types, and the baseline type below its base.
    pub down_color: Color32,
    /// Line width in logical pixels.
    pub width: f32,
    /// Scale shared with the series carrying the same assignment in the main pane. In another
    /// pane the series uses that pane's scale.
    pub scale: SeriesScale,
}

impl Default for SeriesOptions {
    fn default() -> Self {
        Self {
            name: String::new(),
            chart_type: ChartType::Line,
            color: Color32::from_rgb(41, 98, 255),
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
            width: 2.0,
            scale: SeriesScale::Right,
        }
    }
}

impl SeriesOptions {
    fn sanitized(mut self) -> Self {
        if !self.width.is_finite() || self.width <= 0.0 {
            self.width = 2.0;
        }
        if self.chart_type.is_time_independent()
            || self.chart_type == ChartType::TimePriceOpportunity
        {
            self.chart_type = ChartType::Line;
        }
        self
    }

    /// The line, area and baseline styles in the series colours, with the fill opacities of
    /// the default styles.
    fn type_params(&self) -> ChartTypeParams {
        let line = |color| LinePlotStyle {
            color,
            width: self.width,
            style: LineStyle::Solid,
        };
        let fill = |color: Color32, alpha| {
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
        };
        ChartTypeParams {
            styles: ChartTypeStyles {
                line: line(self.color),
                step_line: line(self.color),
                line_with_markers: line(self.color),
                area: AreaStyle {
                    line: line(self.color),
                    top_fill: fill(self.color, 115),
                    bottom_fill: Color32::TRANSPARENT,
                },
                baseline: BaselineStyle {
                    above: line(self.up_color),
                    below: line(self.down_color),
                    above_fill: fill(self.up_color, 30),
                    below_fill: fill(self.down_color, 30),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

pub(super) struct PriceSeries {
    pub id: SeriesId,
    pub options: SeriesOptions,
    pub visible: bool,
    pub bars: Vec<Bar>,
    /// Changes when bars other than the last one change, as the main bars' revision does.
    pub revision: u64,
    pub feed: FeedState,
    pub markers: Vec<Marker>,
    pub lines: Vec<PriceLine>,
}

impl PriceSeries {
    /// The prices of `bar` that the series draws, lowest first.
    pub(super) fn extremes(&self, bar: &Bar) -> [f64; 2] {
        if self.options.chart_type.uses_ohlc() {
            [bar.low, bar.high]
        } else {
            [bar.close; 2]
        }
    }

    /// The close at the crosshair's time, or the last close without a crosshair.
    pub(super) fn legend_value(&self, time: Option<chrono::DateTime<chrono::Utc>>) -> Option<f64> {
        match time {
            Some(time) => self
                .bars
                .binary_search_by_key(&time, |bar| bar.time)
                .ok()
                .map(|at| self.bars[at].close),
            None => self.bars.last().map(|bar| bar.close),
        }
    }

    /// The last-value tag colour: the bar's direction for the OHLC types.
    pub(super) fn value_color(&self, bar: &Bar) -> Color32 {
        match self.options.chart_type.uses_ohlc() {
            true if bar.close >= bar.open => self.options.up_color,
            true => self.options.down_color,
            false => self.options.color,
        }
    }
}

/// An added bar series as a member of a pane, which supplies the pane's background, grid,
/// axis, legend and controls. The chart type renderer draws the bars, so the study holds no
/// values to plot; its output reports the close at the crosshair, or the last close, at every
/// bar.
#[derive(Clone)]
pub(super) struct SeriesLegend {
    name: String,
    style: PlotStyle,
    close: Option<f64>,
    values: IndicatorValues,
}

impl SeriesLegend {
    pub(super) fn new(item: &PriceSeries, time: Option<chrono::DateTime<chrono::Utc>>) -> Self {
        Self {
            name: item.options.name.clone(),
            style: PlotStyle {
                color: item.options.color,
                width: item.options.width,
                pattern: PlotPattern::Solid,
                plot_type: PlotType::Line,
                visible: true,
            },
            close: item.legend_value(time),
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

    fn value_format(&self) -> StudyFormat {
        StudyFormat::Price
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
    /// streaming bars, drawn above the main series in the main pane. Supply data through
    /// [`set_data`](Self::set_data) and [`update`](Self::update); the host owns feeds. Times
    /// are projected onto the main series' time axis, including its whitespace. Move the
    /// series to another pane with [`move_series_to_pane`](Self::move_series_to_pane).
    pub fn add_series(&mut self, options: SeriesOptions) -> SeriesId {
        let options = options.sanitized();
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.ensure_series_scale(options.scale);
        self.price_series.push(PriceSeries {
            id,
            options,
            visible: true,
            bars: Vec::new(),
            revision: crate::model::next_revision(),
            feed: FeedState::default(),
            markers: Vec::new(),
            lines: Vec::new(),
        });
        id
    }

    /// Replaces the display settings of an added bar series, including its chart type and
    /// scale assignment, as [`add_series`](Self::add_series) accepts them. Returns `false` for
    /// an unknown id. The main series is styled through [`set_chart_type`](Self::set_chart_type),
    /// [`set_chart_type_params`](Self::set_chart_type_params) and the colours of
    /// [`config_mut`](Self::config_mut), and assigned with
    /// [`set_series_scale`](Self::set_series_scale).
    pub fn set_series_options(&mut self, id: SeriesId, options: SeriesOptions) -> bool {
        let Some(at) = self.price_series.iter().position(|item| item.id == id) else {
            return false;
        };
        let options = options.sanitized();
        self.ensure_series_scale(options.scale);
        self.price_series[at].options = options;
        self.prune_series_scales();
        true
    }

    /// The position of an added bar series in the draw order, which [`series`](Self::series)
    /// follows. Each pane draws its added series in this order: they are drawn above the main
    /// series, and a later one covers an earlier one.
    pub fn series_order(&self, id: SeriesId) -> Option<usize> {
        self.price_series.iter().position(|item| item.id == id)
    }

    /// Moves an added bar series to position `order` of the draw order, or to the end for a
    /// larger value. Returns `false` for an unknown id.
    pub fn set_series_order(&mut self, id: SeriesId, order: usize) -> bool {
        let Some(at) = self.series_order(id) else {
            return false;
        };
        let item = self.price_series.remove(at);
        self.price_series
            .insert(order.min(self.price_series.len()), item);
        true
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

    /// The scale of this present bar series, including its options and fitted range: in the
    /// main pane the scale shared by its assignment, in another pane that pane's scale.
    pub fn series_price_scale(&self, id: SeriesId) -> Option<&PriceScale> {
        let scale = self.series_scale(id)?;
        Some(
            match self
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.studies.contains(&id))
            {
                Some(pane) => &pane.scale,
                None => self.secondary_scale(scale),
            },
        )
    }

    /// Changes options or a manual range on the scale of this bar series, as returned by
    /// [`series_price_scale`](Self::series_price_scale).
    /// Range bounds use scale units, such as percent in percentage mode.
    pub fn series_price_scale_mut(&mut self, id: SeriesId) -> Option<&mut PriceScale> {
        let scale = self.series_scale(id)?;
        if let Some(at) = self
            .pane_layout
            .panes
            .iter()
            .position(|pane| pane.studies.contains(&id))
        {
            Some(&mut self.pane_layout.panes[at].scale)
        } else if scale == self.main_series_scale {
            Some(self.state.price_scale_mut())
        } else {
            self.series_scales.get_mut(&scale)
        }
    }

    /// The price mapping of a present bar series, the main one included, as last drawn in its
    /// pane: [`PriceMap::price_to_y`] and [`PriceMap::y_to_price`] convert between its prices
    /// and the y coordinates of the chart's [`Ui`](egui::Ui). `None` for an absent series, or
    /// before its pane is first drawn.
    pub fn series_price_map(&self, id: SeriesId) -> Option<PriceMap> {
        let pane = self.series_pane(id)?;
        if pane != SeriesId::MAIN {
            return self
                .last_pane_maps
                .iter()
                .find(|(drawn, ..)| *drawn == pane)
                .map(|(.., map)| *map);
        }
        let rect = self.last_rendered_price_rect;
        if !rect.is_positive() {
            None
        } else if id == SeriesId::MAIN {
            Some(self.last_price_map)
        } else {
            let item = self.price_series.iter().find(|item| item.id == id)?;
            Some(self.series_map(item, rect))
        }
    }

    /// Formats `price` as the scale of a present bar series labels it: a price, a signed
    /// percentage or an indexed value. `None` where [`series_price_map`](Self::series_price_map)
    /// is.
    pub fn format_series_price(&self, id: SeriesId, price: f64) -> Option<String> {
        let map = self.series_price_map(id)?;
        let transform = map.transform();
        Some(transform.label(
            transform.to_scale(price),
            map.price_precision().unwrap_or(2),
        ))
    }

    fn ensure_series_scale(&mut self, scale: SeriesScale) {
        if scale != self.main_series_scale {
            self.series_scales
                .entry(scale)
                .or_insert_with(|| PriceScale::new(PriceScaleOptions::default()));
        }
    }

    pub(super) fn prune_series_scales(&mut self) {
        self.series_scales.retain(|scale, _| {
            self.price_series
                .iter()
                .any(|item| item.options.scale == *scale)
        });
    }

    fn secondary_scale(&self, scale: SeriesScale) -> &PriceScale {
        if scale == self.main_series_scale {
            self.state.price_scale()
        } else {
            &self.series_scales[&scale]
        }
    }

    pub(super) fn series_range(&self, item: &PriceSeries) -> std::ops::Range<usize> {
        let logical = self.state.time_scale().visible_logical_range();
        let bars = self.state.shown();
        let times = super::sync::time_at(bars, logical.left.floor() as f64)
            .zip(super::sync::time_at(bars, logical.right.floor() as f64));
        match times {
            Some((first, last)) if first <= last => {
                item.bars.partition_point(|bar| bar.time < first)
                    ..item.bars.partition_point(|bar| bar.time <= last)
            }
            _ => 0..0,
        }
    }

    fn series_transform(&self, item: &PriceSeries) -> PriceTransform {
        let mode = self.secondary_scale(item.options.scale).options().mode;
        let range = self.series_range(item);
        let base = item.bars[range].first().map_or(f64::NAN, |bar| bar.close);
        PriceTransform { mode, base }
    }

    /// Fits every independent range and returns the secondary values sharing the main range.
    pub(super) fn fit_secondary_series(&mut self) -> Vec<f64> {
        let mut values =
            std::collections::HashMap::<SeriesScale, (PriceTransform, f64, f64)>::new();
        for item in self.main_pane_series() {
            let transform = self.series_transform(item);
            let range = self.series_range(item);
            let entry = values.entry(item.options.scale).or_insert((
                transform,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ));
            for value in item.bars[range]
                .iter()
                .flat_map(|bar| item.extremes(bar).map(|price| transform.to_scale(price)))
            {
                if value.is_finite() {
                    if !entry.1.is_finite() {
                        entry.0 = transform;
                    }
                    entry.1 = entry.1.min(value);
                    entry.2 = entry.2.max(value);
                }
            }
        }
        let mut main = Vec::new();
        for (key, (transform, min, max)) in values {
            if !min.is_finite() {
                continue;
            }
            if key == self.main_series_scale {
                main.extend([min, max]);
            } else if let Some(scale) = self.series_scales.get_mut(&key) {
                // Membership and scrolling change references, not a manual range's scale units.
                let manual = (scale.is_manual() && scale.transform().mode == transform.mode)
                    .then(|| scale.range());
                scale.rebase(transform);
                if let Some(range) = manual {
                    scale.set_range(range.min, range.max);
                }
                scale.autoscale(min, max);
            }
        }
        main
    }

    fn series_map(&self, item: &PriceSeries, rect: Rect) -> PriceMap {
        self.secondary_scale(item.options.scale).map(
            self.series_transform(item),
            rect.top(),
            rect.bottom(),
        )
    }

    pub(super) fn secondary_axis_values(
        &self,
        key: SeriesScale,
        rect: Rect,
    ) -> impl Iterator<Item = (f64, Color32, PriceMap)> + '_ {
        self.main_pane_series()
            .filter(move |item| item.options.scale == key)
            .flat_map(move |item| {
                let map = self.series_map(item, rect);
                item.lines
                    .iter()
                    .filter(|line| line.axis_label)
                    .map(|line| (line.price, line.color))
                    .chain(
                        item.bars
                            .last()
                            .filter(|_| self.config.show_symbol_last_val)
                            .map(|bar| (bar.close, item.value_color(bar))),
                    )
                    .filter(move |(value, _)| {
                        map.range().contains(map.transform().to_scale(*value))
                    })
                    .map(move |(value, color)| (value, color, map))
            })
    }

    pub(super) fn series_axis_keys(&self) -> Vec<SeriesScale> {
        let mut keys = Vec::new();
        for item in self.main_pane_series() {
            let key = item.options.scale;
            if key != self.main_series_scale
                && !matches!(key, SeriesScale::Overlay(_))
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        keys
    }

    /// The visible added series of the main pane, in draw order.
    fn main_pane_series(&self) -> impl Iterator<Item = &PriceSeries> {
        self.price_series
            .iter()
            .filter(|item| item.visible && self.series_pane(item.id) == Some(SeriesId::MAIN))
    }

    pub(super) fn paint_secondary_series(&self, painter: &egui::Painter, rect: Rect) {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        for item in self.main_pane_series() {
            self.paint_series(&plot, rect, item, &self.series_map(item, rect));
        }
    }

    /// Paints the bars, price lines and markers of `item` in `rect` with `map`.
    pub(super) fn paint_series(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: &PriceMap,
    ) {
        let main = self.state.shown();
        if main.is_empty() {
            return;
        }
        let range = self.series_range(item);
        // One neighbour at each edge keeps clipped segments continuous during scrolling.
        let start = range.start.saturating_sub(1);
        let end = (range.end + 1).min(item.bars.len());
        let visible = &item.bars[start..end];
        let options = &item.options;
        let colors = StyleColors {
            bullish: options.up_color,
            bearish: options.down_color,
            text: self.config.text_color,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        };
        let context = RenderContext::new(plot, rect);
        let clock = self.bar_clock();
        let type_params = options.type_params();
        // The renderers place the series' bars by index; each index maps through its bar's
        // time onto the main time axis.
        let x = |index: usize, _| {
            item.bars
                .get(index)
                .and_then(|bar| super::sync::index_at(main, bar.time))
                .map_or(f32::NAN, |at| {
                    self.state.time_scale().idx_to_coord_precise(
                        at as f32,
                        rect.left(),
                        rect.width(),
                    )
                })
        };
        rendering::render_chart_type(
            options.chart_type,
            &rendering::CandleDataContext {
                price_ctx: &context,
                volume_ctx: &context,
                price_scale: map,
                colors: &colors,
                visible_data: visible,
                full_data: &item.bars,
                revision: item.revision,
                start_idx: start,
                clock: &clock,
                tick_size: None,
            },
            &rendering::ChartTypeParams::new(
                rendering::BarDimensions::new(
                    self.state.time_scale().bar_spacing() * self.config.candle_width,
                    self.config.wick_width,
                ),
                rendering::VolumeSettings::new(
                    false,
                    visible
                        .iter()
                        .map(|bar| bar.volume)
                        .filter(|volume| volume.is_finite())
                        .max_by(f64::total_cmp)
                        .unwrap_or(1.0),
                ),
                &type_params,
                rendering::TradingColors::new(options.up_color, options.down_color),
                rendering::CoordMapping::new(rect.left()),
                PriceSource::Close,
            ),
            x,
        );
        for line in &item.lines {
            rendering::render_price_line(plot, rect, map, line, rect.left(), false, rect.right());
        }
        for marker in &item.markers {
            let Some(index) = super::sync::index_at(main, marker.time) else {
                continue;
            };
            let at = item
                .bars
                .partition_point(|bar| bar.time <= marker.time)
                .saturating_sub(1);
            if let Some(bar) = item.bars.get(at) {
                let coords = crate::chart::coords::ChartMapping {
                    start_idx: 0,
                    right_offset: self.get_chart_mapping().right_offset - index as f32,
                    price: *map,
                    ..self.get_chart_mapping()
                };
                let bar = Bar {
                    time: marker.time,
                    ..*bar
                };
                crate::chart::renderers::render_markers(
                    &RenderContext::new(plot, rect),
                    std::slice::from_ref(marker),
                    &[bar],
                    map,
                    &coords,
                );
            }
        }
    }

    pub(super) fn paint_secondary_legend(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        top: f32,
    ) {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        let main = self.state.shown();
        for (row, item) in self.main_pane_series().enumerate() {
            if self.config.show_indicator_labels || self.config.show_indicator_last_val {
                let selected_time = ui
                    .input(|input| input.pointer.hover_pos())
                    .filter(|point| rect.contains(*point))
                    .and_then(|point| {
                        main.get(
                            self.state
                                .time_scale()
                                .coord_to_idx(point.x, rect.left(), rect.width())
                                .round()
                                .max(0.0) as usize,
                        )
                    })
                    .map(|bar| bar.time);
                let value = item
                    .legend_value(selected_time)
                    .map_or_else(|| "∅".into(), |close| format!("{close:.2}"));
                plot.text(
                    Pos2::new(rect.left() + 8.0, top + row as f32 * 20.0),
                    egui::Align2::LEFT_TOP,
                    format!(
                        "{}  {}",
                        if self.config.show_indicator_labels {
                            &item.options.name
                        } else {
                            ""
                        },
                        if self.config.show_indicator_last_val {
                            &value
                        } else {
                            ""
                        }
                    ),
                    egui::FontId::proportional(crate::styles::typography::MD),
                    item.options.color,
                );
            }
        }
    }

    fn secondary_axes(&self, widget: Rect, rect: Rect) -> Vec<(SeriesScale, Rect)> {
        let keys = self.series_axis_keys();
        let left_count = keys
            .iter()
            .filter(|key| **key != SeriesScale::Right)
            .count();
        let primary_left = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let primary_right = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let study_count = self
            .studies
            .indicators()
            .iter()
            .filter(|study| {
                (self.config.show_left_axis || self.config.show_right_axis)
                    && study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
                    && study.scale() == crate::studies::StudyScale::Independent
            })
            .count();
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget.width().min(widget.height()).max(0.0) / 2.0);
        let mut left = (rect.left() - primary_left).max(widget.left() + padding);
        let slot_width =
            (left - widget.left() - padding).max(0.0) / (left_count + study_count).max(1) as f32;
        keys.into_iter()
            .map(|key| {
                let axis = if key == SeriesScale::Right {
                    Rect::from_min_max(
                        Pos2::new(
                            (rect.right() + primary_right).min(widget.right()),
                            rect.top(),
                        ),
                        Pos2::new(widget.right(), rect.bottom()),
                    )
                } else {
                    let axis = Rect::from_min_max(
                        Pos2::new(left - slot_width, rect.top()),
                        Pos2::new(left, rect.bottom()),
                    );
                    left -= slot_width;
                    axis
                };
                (key, axis)
            })
            .collect()
    }

    pub(super) fn handle_secondary_axes(&mut self, ui: &egui::Ui, widget: Rect, rect: Rect) {
        for (key, axis) in self.secondary_axes(widget, rect) {
            let response = ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            );
            let Some(scale) = self.series_scales.get_mut(&key) else {
                continue;
            };
            if response.double_clicked()
                && self
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .price
            {
                scale.reset();
            } else if response.dragged()
                && self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
            {
                let delta = ui.input(|input| input.pointer.delta().y);
                let range = scale.range();
                let anchor = (range.min + range.max) * 0.5;
                scale.zoom(
                    (f64::from(delta) / f64::from(rect.height().max(1.0))).exp(),
                    anchor,
                );
            }
        }
    }

    pub(super) fn paint_series_axes(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        widget: Rect,
        rect: Rect,
        colors: &StyleColors,
    ) -> f32 {
        let mut measured = 0.0_f32;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let position = if key == SeriesScale::Right {
                crate::scales::PriceScaleId::Right
            } else {
                crate::scales::PriceScaleId::Left
            };
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            let Some(item) = self
                .main_pane_series()
                .find(|item| item.options.scale == key)
            else {
                continue;
            };
            let map = self.series_map(item, rect);
            let label_rect = if key == SeriesScale::Right {
                Rect::from_min_max(rect.min, axis.left_bottom())
            } else {
                Rect::from_min_max(axis.right_top(), rect.max)
            };
            let ctx = RenderContext::new(&clipped, label_rect);
            let slot = clipped.add(egui::Shape::Noop);
            let mut tags = Vec::new();
            for (value, color, mapping) in self.secondary_axis_values(key, rect) {
                tags.push(crate::chart::rendering::axes::render_axis_value(
                    &clipped,
                    label_rect,
                    axis.right(),
                    mapping.price_to_y(value),
                    mapping
                        .transform()
                        .label(mapping.transform().to_scale(value), 2),
                    (crate::chart::rendering::axes::tag_text_color(color), color),
                    &tags,
                ));
            }
            crate::chart::renderers::render_price_labels_at_pos(
                &ctx, &map, colors, position, &tags, slot,
            );
            measured = measured.max(crate::chart::rendering::axes::price_axis_width(
                &clipped,
                &map,
                &[],
            ));
            ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            )
            .on_hover_text(&item.options.name);
        }
        measured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarData;
    use crate::widget::test_frames::frame;
    use chrono::{TimeDelta, Utc};

    fn bars(prices: &[f64]) -> Vec<Bar> {
        let start = "2024-01-01T00:00:00Z"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap();
        prices
            .iter()
            .enumerate()
            .map(|(index, &price)| {
                Bar::new(
                    start + TimeDelta::minutes(index as i64),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect()
    }

    /// Percent compares returns, not absolute prices: 100 -> 110 and 200 -> 220
    /// coincide at +10%; 200 -> 240 extends the shared range to 20% plus 10% margins.
    #[test]
    fn comparisons_share_percent_ranges_with_individual_visible_references() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
        let id = chart.add_comparison("Other", Color32::BLUE);
        chart.set_data(id, bars(&[200.0, 220.0, 240.0]));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let rect = chart.last_rendered_price_rect;
        let map = chart.series_map(&chart.price_series[0], rect);
        assert!((map.price_to_y(220.0) - chart.last_price_map.price_to_y(110.0)).abs() < 1e-4);
        let range = chart.state.price_scale().range();
        assert!(
            (range.min + 2.0).abs() < 1e-10 && (range.max - 22.0).abs() < 1e-10,
            "{range:?}"
        );
        chart.state.time_scale_mut().set_bar_spacing(100.0);
        chart.state.time_scale_mut().set_right_offset(0.0);
        for width in [200.0, 150.0] {
            chart.state.time_scale_mut().set_width(width);
            // Logical left edges 1 and 1.5 both retain the partially visible bar at 1.
            assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
        }
        chart.state.time_scale_mut().set_bar_spacing(6.0);
        // A missing first timestamp uses the series' first available visible close.
        chart.set_data(id, bars(&[200.0, 220.0, 240.0])[1..].to_vec());
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(chart.series_transform(&chart.price_series[0]).base, 220.0);
        for invalid in [0.0, f64::NAN] {
            chart.set_data(id, bars(&[invalid]));
            frame(&ctx, &mut chart, 0.2, vec![]);
            let map = chart.series_map(&chart.price_series[0], rect);
            assert!(!map.price_to_y(invalid).is_finite());
        }
        for change in ["hide", "remove", "empty", "join_main"] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            let first = chart.add_comparison("First", Color32::BLUE);
            let second = chart.add_comparison("Second", Color32::GREEN);
            for (id, prices) in [(first, [200.0, 220.0]), (second, [400.0, 440.0])] {
                chart.set_series_scale(id, SeriesScale::Left);
                chart.set_data(id, bars(&prices));
            }
            chart
                .series_price_scale_mut(first)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Percentage;
            frame(&ctx, &mut chart, 0.2, vec![]);
            chart
                .series_price_scale_mut(first)
                .unwrap()
                .set_range(-5.0, 25.0);
            match change {
                "hide" => {
                    chart.set_series_visible(first, false);
                }
                "remove" => {
                    chart.remove_series(first);
                }
                "join_main" => {
                    chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left);
                }
                _ => chart.set_data(first, Vec::new()),
            }
            frame(&ctx, &mut chart, 0.3, vec![]);
            let range = chart.series_price_scale(second).unwrap().range();
            assert_eq!((range.min, range.max), (-5.0, 25.0), "{change}");
            if change == "join_main" {
                chart.set_series_scale(SeriesId::MAIN, SeriesScale::Right);
                frame(&ctx, &mut chart, 0.4, vec![]);
                let range = chart.series_price_scale(second).unwrap().range();
                assert_eq!((range.min, range.max), (-5.0, 25.0), "main leaves scale");
            }
            chart
                .series_price_scale_mut(second)
                .unwrap()
                .options_mut()
                .mode = PriceScaleMode::Normal;
            frame(&ctx, &mut chart, 0.5, vec![]);
            let range = chart.series_price_scale(second).unwrap().range();
            let expected = if change == "join_main" {
                (190.0, 250.0)
            } else {
                (380.0, 500.0)
            };
            assert_eq!((range.min, range.max), expected, "mode change: {change}");
        }
    }

    /// Secondary history and corrections use the main merge rules, and visibility/removal
    /// cannot lose data or resurrect an id. A scale move preserves the other memberships.
    #[test]
    fn bar_series_keep_independent_data_and_scale_membership() {
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
        let first = chart.add_series(SeriesOptions::default());
        let second = chart.add_series(SeriesOptions {
            scale: SeriesScale::Left,
            ..Default::default()
        });
        let data = bars(&[200.0, 220.0]);
        chart.set_data(
            first,
            vec![
                data[1].clone(),
                data[0].clone(),
                bars(&[f64::NAN]).remove(0),
            ],
        );
        chart.update(first, bars(&[210.0]).remove(0));
        chart.update(second, data[1].clone());
        assert_eq!(
            chart
                .bars(first)
                .iter()
                .map(|bar| bar.close)
                .collect::<Vec<_>>(),
            [210.0, 220.0]
        );
        assert_eq!(chart.feed_state(first).unwrap().rejected_bars, 1);
        assert_eq!(chart.bars(second), &data[1..]);
        assert_eq!(chart.bars(SeriesId::MAIN), bars(&[100.0, 110.0]));
        for scale in [
            SeriesScale::Left,
            SeriesScale::Overlay(4),
            SeriesScale::Separate(7),
            SeriesScale::Right,
        ] {
            assert!(chart.set_series_scale(first, scale));
            assert_eq!(chart.series_scale(first), Some(scale));
            assert_eq!(chart.series_scale(second), Some(SeriesScale::Left));
            assert!(chart.set_series_visible(first, false));
            assert_eq!(chart.bars(first).len(), 2);
            assert!(!chart.is_series_visible(first));
            assert!(chart.set_series_visible(first, true));
        }
        assert!(chart.set_series_scale(SeriesId::MAIN, SeriesScale::Left));
        assert_eq!(chart.series_scale(first), Some(SeriesScale::Right));
        assert!(chart.remove_series(first));
        assert!(!chart.remove_series(first));
        chart.update(first, data[0].clone());
        assert!(chart.bars(first).is_empty());
        assert!(chart.remove_series(SeriesId::MAIN));
        assert!(!chart.remove_series(SeriesId::MAIN));
        assert!(!chart.set_series_visible(SeriesId::MAIN, true));
        assert!(chart.bars(SeriesId::MAIN).is_empty());
        assert_eq!(chart.bars(second), &data[1..]);
    }

    /// Every scale is fitted once across all its visible members; hidden and differently
    /// assigned values cannot stretch it. Labels reserve space only for visible axes.
    #[test]
    fn series_scales_fit_visible_members_and_reserve_their_axes() {
        let ctx = egui::Context::default();
        for key in [
            SeriesScale::Right,
            SeriesScale::Left,
            SeriesScale::Overlay(1),
            SeriesScale::Separate(2),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            let id = chart.add_series(SeriesOptions {
                scale: key,
                color: Color32::MAGENTA,
                ..Default::default()
            });
            chart.set_data(id, bars(&[200.0, 220.0]));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let range = chart.series_price_scale_mut(id).unwrap().range();
            let expected = if key == SeriesScale::Right {
                (88.0, 232.0)
            } else {
                (198.0, 222.0)
            };
            assert!(
                (range.min - expected.0).abs() < 1e-10 && (range.max - expected.1).abs() < 1e-10,
                "{key:?}: {range:?}"
            );
            assert_eq!(
                chart.series_axis_keys().is_empty(),
                matches!(key, SeriesScale::Right | SeriesScale::Overlay(_))
            );
            chart.set_series_visible(id, false);
            frame(&ctx, &mut chart, 0.1, vec![]);
            assert!(chart.series_axis_keys().is_empty());
            let range = chart.state.price_scale().range();
            assert!((range.min - 99.0).abs() < 1e-10 && (range.max - 111.0).abs() < 1e-10);
            // Secondary bars can occupy whitespace beyond the main history.
            let future = bars(&[200.0, 220.0])
                .into_iter()
                .map(|mut bar| {
                    bar.time += TimeDelta::minutes(10);
                    bar
                })
                .collect();
            chart.set_data(id, future);
            chart.series_price_scale_mut(id).unwrap().options_mut().mode =
                PriceScaleMode::Percentage;
            chart.set_series_visible(id, true);
            chart.state.time_scale_mut().set_bar_spacing(100.0);
            chart.state.time_scale_mut().set_right_offset(12.0);
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
            assert!(chart.state.visible_data().is_empty());
            if key == SeriesScale::Right {
                assert_eq!(
                    chart.last_price_map.transform().mode,
                    PriceScaleMode::Percentage
                );
            }
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Path(path)
                        if path.stroke.color == egui::epaint::ColorMode::Solid(Color32::MAGENTA)
                )),
                "secondary line in main whitespace: {key:?}"
            );
        }
        // Hidden study axes do not consume a secondary scale's reserved gutter.
        for width in [45.0, 240.0, 800.0] {
            let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0])));
            chart.set_series_scale(SeriesId::MAIN, SeriesScale::Overlay(0));
            chart.config.padding = 8.0;
            chart.add_series(SeriesOptions {
                scale: SeriesScale::Left,
                ..Default::default()
            });
            chart.add_indicator(Box::new(
                crate::studies::CustomIndicator::new("Independent", |_| Vec::new())
                    .with_scale(crate::studies::StudyScale::Independent),
            ));
            let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, 400.0));
            let plot = chart.calculate_layout_rects(widget).price_rect;
            let axes = chart.secondary_axes(widget, plot);
            assert_eq!(axes.len(), 1);
            assert_eq!(axes[0].1.left(), widget.left() + 8.0, "{width}");
            assert_eq!(axes[0].1.right(), plot.left(), "{width}");
        }
    }

    /// A candle series moved to its own pane is fitted to its lows and highs on that pane's
    /// scale and drawn there, leaving the main scale alone, and draws in the main pane again
    /// once moved back.
    #[test]
    fn series_move_between_panes_with_their_chart_type_and_scale() {
        let ctx = egui::Context::default();
        let up = Color32::from_rgb(1, 200, 2);
        let mut chart = Chart::new(BarData::from_bars(bars(&[100.0, 110.0, 120.0])));
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Candles,
            up_color: up,
            ..Default::default()
        });
        let candles = bars(&[0.0; 3])
            .into_iter()
            .map(|bar| Bar::new(bar.time, 200.0, 230.0, 190.0, 220.0, 1.0))
            .collect();
        chart.set_data(id, candles);
        assert!(!chart.move_series_to_pane(id, SeriesId(99)));
        assert!(chart.move_series_to_pane(id, id));
        assert_eq!(chart.series_pane(id), Some(id));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
        let main = chart.last_rendered_price_rect;
        assert!(chart.state.price_scale().range().max < 200.0);
        let range = chart.series_price_scale(id).unwrap().range();
        assert!(
            range.min <= 190.0 && (230.0..300.0).contains(&range.max),
            "{range:?}"
        );
        let map = chart.series_price_map(id).unwrap();
        assert!(map.price_to_y(230.0) > main.bottom());
        assert_eq!(
            chart.format_series_price(id, 230.0).as_deref(),
            Some("230.00")
        );
        // Candle bodies in the plot, not the last-value tag on the axis.
        let bodies = |painted: &[egui::Shape]| -> Vec<Rect> {
            painted
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Rect(body)
                        if body.fill == up && body.rect.right() < main.right() =>
                    {
                        Some(body.rect)
                    }
                    _ => None,
                })
                .collect()
        };
        let drawn = bodies(&painted);
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|body| body.top() > main.bottom()));

        assert!(chart.move_series_to_pane(id, SeriesId::MAIN));
        assert_eq!(chart.series_pane(id), Some(SeriesId::MAIN));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let main = chart.last_rendered_price_rect;
        let drawn = bodies(&painted);
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|body| main.contains_rect(*body)));
        assert!(chart.last_pane_maps.is_empty());
    }

    /// Bars can be popped from, and looked up in, any bar series; live changes are reported
    /// once per series in a row; the draw order can be changed.
    #[test]
    fn series_data_is_queried_popped_and_reported() {
        use crate::model::LogicalRange;
        use crate::widget::{BarsInRange, ChartEvent, MismatchDirection};
        let mut chart = Chart::new(BarData::from_bars(bars(&[1.0, 2.0, 3.0, 4.0, 5.0])));
        let id = chart.add_series(SeriesOptions::default());
        // Bars at the main bars 0, 2 and 4.
        let data = bars(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        chart.set_data(id, vec![data[0].clone(), data[2].clone(), data[4].clone()]);
        let minute = |index| data[0].time + TimeDelta::minutes(index);
        for (time, direction, expected) in [
            (minute(2), MismatchDirection::Exact, Some(1)),
            (minute(1), MismatchDirection::Exact, None),
            (minute(1), MismatchDirection::NearestLeft, Some(0)),
            (minute(1), MismatchDirection::NearestRight, Some(1)),
            (minute(-1), MismatchDirection::NearestLeft, None),
            (minute(5), MismatchDirection::NearestRight, None),
        ] {
            assert_eq!(
                chart.bar_index_at(id, time, direction),
                expected,
                "{time} {direction:?}"
            );
        }
        for ((left, right), bars, bars_before, bars_after) in [
            ((1.0, 3.0), 1..2, 1.0, 1.0),
            // Two bars of whitespace before the first bar.
            ((-2.0, 2.4), 0..2, -2.0, 1.0),
            // Five bars of whitespace after the last bar, at 4.
            ((2.6, 9.0), 2..3, 2.0, -5.0),
            ((0.6, 1.4), 1..1, 1.0, 2.0),
        ] {
            assert_eq!(
                chart.bars_in_logical_range(id, LogicalRange::new(left, right)),
                Some(BarsInRange {
                    bars,
                    bars_before,
                    bars_after
                }),
                "{left}..{right}"
            );
        }

        let _ = chart.drain_events().count();
        chart.update(id, data[3].clone());
        chart.update(id, bars(&[f64::NAN]).remove(0));
        chart.update(id, Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0));
        assert_eq!(
            chart.pop_bars(id, 2),
            [
                Bar::new(minute(3), 13.0, 15.0, 13.0, 15.0, 1.0),
                data[4].clone()
            ]
        );
        assert_eq!(
            chart.pop_bars(SeriesId::MAIN, 1),
            bars(&[1.0, 2.0, 3.0, 4.0, 5.0])[4..]
        );
        assert!(chart.pop_bars(SeriesId(99), 1).is_empty());
        assert_eq!(chart.bars(id), [data[0].clone(), data[2].clone()]);
        assert_eq!(chart.bars(SeriesId::MAIN).len(), 4);
        assert_eq!(
            chart.drain_events().collect::<Vec<_>>(),
            [
                ChartEvent::DataUpdated {
                    series: id,
                    bars: 2
                },
                ChartEvent::DataUpdated {
                    series: SeriesId::MAIN,
                    bars: 4
                },
            ]
        );

        let other = chart.add_series(SeriesOptions::default());
        assert!(chart.set_series_order(other, 0));
        assert_eq!(
            chart.series().map(|(id, _)| id).collect::<Vec<_>>(),
            [other, id]
        );
        assert_eq!(chart.series_order(id), Some(1));
    }
}
