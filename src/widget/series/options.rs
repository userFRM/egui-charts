//! Host-fed bar series and their shared price scales.

use crate::chart::renderers::{RenderContext, StyleColors};
use crate::chart::rendering;
use crate::data::FeedState;
use crate::localization as l10n;
use crate::model::{
    AreaStyle, Bar, BaselineStyle, ChartType, ChartTypeParams, ChartTypeStyles, LinePlotStyle,
    LineStyle, Marker, MarkerOptions, MarkerZOrder, PriceLine, PriceSource, SeriesScale,
    SeriesStyle,
};
use crate::scales::StudyFormat;
use crate::scales::{PriceMap, PriceScale, PriceScaleMode};
use crate::studies::{Indicator, IndicatorValues, PlotStyle, PlotType};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::widget::{Chart, ChartElementId, PaneId, SeriesId};
use egui::{Color32, Pos2, Rect};
use serde::{Deserialize, Serialize};

/// Display settings of a host-fed bar series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Scale shared with the series carrying the same assignment in the price pane. In another
    /// pane the series uses that pane's scale.
    pub scale: SeriesScale,
    /// The price [`ChartType::Columns`] rise above or fall below, which the scale's fit then
    /// includes; `None` raises them from the pane bottom.
    pub columns_base: Option<f64>,
    /// Last value label, price line, base line and autoscale. By default the series labels its
    /// last value without a price line.
    pub style: SeriesStyle,
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
        if self.chart_type.is_time_independent()
            || self.chart_type == ChartType::TimePriceOpportunity
        {
            self.chart_type = ChartType::Line;
        }
        self
    }

    /// The line, area and baseline styles in the series colours, with the fill opacities of
    /// the default styles, and the columns' base.
    fn type_params(&self) -> ChartTypeParams {
        let line = |color| LinePlotStyle {
            color,
            width: self.width,
            style: LineStyle::Solid,
            ..Default::default()
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
                    ..Default::default()
                },
                baseline: BaselineStyle {
                    above: line(self.up_color),
                    below: line(self.down_color),
                    above_fill: fill(self.up_color, 30),
                    below_fill: fill(self.down_color, 30),
                },
                ..Default::default()
            },
            columns_base: self.columns_base,
            ..Default::default()
        }
    }
}

/// The base line of a percentage or indexed scale mapped by `map`, at 0% or 100.
pub(in crate::widget) fn base_line(map: &PriceMap) -> Option<PriceLine> {
    let transform = map.transform();
    matches!(
        transform.mode,
        PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100
    )
    .then(|| {
        PriceLine::new(transform.base, DESIGN_TOKENS.semantic.extended.gray)
            .with_width(DESIGN_TOKENS.stroke.hairline)
            .with_style(LineStyle::Solid)
            .with_axis_label(false)
    })
}

pub(in crate::widget) struct PriceSeries {
    pub id: SeriesId,
    pub options: SeriesOptions,
    pub visible: bool,
    pub bars: Vec<Bar>,
    /// Changes when bars other than the last one change, as the main bars' revision does.
    pub revision: u64,
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

    /// The last-value tag colour: the bar's own colour, or its direction for the OHLC types.
    pub(in crate::widget) fn value_color(&self, bar: &Bar) -> Color32 {
        if let Some(color) = bar.colors.as_ref().and_then(|own| own.color) {
            return color;
        }
        match self.options.chart_type.uses_ohlc() {
            true if bar.close >= bar.open => self.options.up_color,
            true => self.options.down_color,
            false => self.options.color,
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
            heikin_ashi: Default::default(),
            feed: FeedState::default(),
            markers: Default::default(),
            marker_options: MarkerOptions::default(),
            change_marks: None,
            lines: Vec::new(),
        });
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

    /// The last value label, price line, base line, autoscale and click tolerance of a present
    /// bar series.
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

    /// Changes the last value label, price line, base line, autoscale or click tolerance of a
    /// present bar series. Changes take effect on the next rendered frame.
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

    /// The scale shared by this present bar series, including its options and fitted range.
    pub fn series_price_scale(&self, id: SeriesId) -> Option<&PriceScale> {
        let scale = self.series_scale(id)?;
        Some(
            match self
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.members.contains(&id))
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
            .position(|pane| pane.members.contains(&id))
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
        if self.series_pane(id)? != PaneId::MAIN {
            return self
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.members.contains(&id) && pane.rect.is_positive())
                .map(|pane| {
                    pane.price_map()
                        .with_thousands_sep(self.config.thousands_sep)
                });
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
        Some(map.label(
            transform.to_scale(price),
            map.price_precision().unwrap_or(2),
            self.series_label_formatter(id, transform.mode).as_deref(),
        ))
    }

    pub(in crate::widget) fn hit_price_series(
        &self,
        item: &PriceSeries,
        pos: Pos2,
        rect: Rect,
        map: PriceMap,
    ) -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let main = self.state.shown();
        let logical =
            self.state
                .time_scale()
                .coord_to_idx_precise(pos.x, rect.left(), rect.width());
        let time = crate::model::time_at(main, logical)?;
        let at = item.bars.partition_point(|bar| bar.time < time);
        let x = |bar: &Bar| {
            crate::model::index_at(main, bar.time).map(|index| {
                self.state
                    .time_scale()
                    .idx_to_coord_precise(index, rect.left(), rect.width())
            })
        };
        let tolerance = item.options.style.hit_tolerance;
        for index in at.saturating_sub(1)..(at + 2).min(item.bars.len()) {
            let bar = &item.bars[index];
            if bar.is_whitespace() {
                continue;
            }
            let px = x(bar)?;
            let close = Pos2::new(px, map.price_to_y(bar.close));
            let hit = if item.options.chart_type.uses_ohlc() {
                let half_width = self.state.time_scale().bar_spacing() * 0.35;
                let body = Rect::from_two_pos(
                    Pos2::new(px - half_width, map.price_to_y(bar.open)),
                    Pos2::new(px + half_width, close.y),
                );
                body.expand(1.0).contains(pos)
                    || ((pos.x - px).abs() <= tolerance
                        && (map.price_to_y(bar.low).min(map.price_to_y(bar.high))
                            ..=map.price_to_y(bar.low).max(map.price_to_y(bar.high)))
                            .contains(&pos.y))
            } else if item.options.chart_type == ChartType::Columns {
                let base = item
                    .options
                    .columns_base
                    .map_or(rect.bottom(), |base| map.price_to_y(base));
                Rect::from_two_pos(
                    Pos2::new(px - self.state.time_scale().bar_spacing() * 0.4, base),
                    Pos2::new(px + self.state.time_scale().bar_spacing() * 0.4, close.y),
                )
                .contains(pos)
            } else if pos.distance(close) <= tolerance {
                true
            } else if let Some(next) = item.bars.get(index + 1).filter(|bar| !bar.is_whitespace()) {
                let next = Pos2::new(x(next)?, map.price_to_y(next.close));
                if item.options.chart_type == ChartType::StepLine {
                    let corner = Pos2::new(next.x, close.y);
                    crate::chart::hit_test::point_to_segment_distance(pos, close, corner).min(
                        crate::chart::hit_test::point_to_segment_distance(pos, corner, next),
                    ) <= tolerance
                } else {
                    crate::chart::hit_test::point_to_segment_distance(pos, close, next) <= tolerance
                }
            } else {
                false
            };
            if hit {
                return crate::model::index_at(main, bar.time)
                    .map(|index| index.round().max(0.0) as usize);
            }
        }
        None
    }

    /// Paints the bars, price lines, change marks and markers of `item` in `rect` with `map`.
    /// Markers placed above the series are left to `paint_marks` on the price pane; in another
    /// pane they are painted here, after the pane's studies.
    pub(in crate::widget) fn paint_series(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
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
                .and_then(|bar| crate::model::index_at(main, bar.time))
                .map_or(f32::NAN, |at| {
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(at, rect.left(), rect.width())
                })
        };
        rendering::render_chart_type(
            options.chart_type,
            &rendering::CandleDataContext {
                price_ctx: &context,
                volume_ctx: &context,
                price_scale: &map,
                colors: &colors,
                visible_data: visible,
                full_data: &item.bars,
                revision: item.revision,
                shows_elements: false,
                heikin_ashi: &item.heikin_ashi,
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
        let style = &item.options.style;
        let price_line = self
            .series_last_bar(item)
            .filter(|_| self.config.show_price_line && style.price_line_visible)
            .map(|bar| {
                PriceLine::new(
                    bar.close,
                    style.price_line_color.unwrap_or(item.value_color(bar)),
                )
                .with_width(style.price_line_width)
                .with_style(style.price_line_style)
                .with_axis_label(false)
            });
        let base_line = base_line(&map).filter(|_| style.base_line_visible);
        let lines = item.lines.iter().map(|(_, line)| line);
        for line in base_line.iter().chain(&price_line).chain(lines) {
            crate::chart::rendering::render_price_line(
                plot,
                rect,
                &map,
                line,
                rect.left(),
                false,
                rect.right(),
            );
        }
        if let Some(marks) = &item.change_marks {
            marks.paint(plot, |time, close| {
                let index = crate::model::index_at(main, time)?;
                let x =
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(index, rect.left(), rect.width());
                Some(Pos2::new(x, map.price_to_y(close)))
            });
        }
        if item.marker_options.z_order == MarkerZOrder::Normal || self.in_pane(item.id) {
            self.paint_series_markers(plot, rect, item, map);
        }
    }

    /// Paints the markers of `item` with `map`, each at the time and price of its bar.
    pub(in crate::widget) fn paint_series_markers(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
    ) {
        let (from, to) =
            crate::widget::marks::window(self.state.shown(), &self.get_chart_mapping(), rect);
        for marker in item
            .markers
            .within(from, to)
            .into_iter()
            .map(|index| &item.markers[index])
        {
            let Some(index) = crate::model::index_at(self.state.shown(), marker.time) else {
                continue;
            };
            let at = item
                .bars
                .partition_point(|bar| bar.time <= marker.time)
                .saturating_sub(1);
            if let Some(bar) = item.bars.get(at) {
                let coords = crate::chart::coords::ChartMapping {
                    start_idx: 0,
                    right_offset: self.get_chart_mapping().right_offset - index,
                    price: map,
                    ..self.get_chart_mapping()
                };
                let bar = Bar {
                    time: marker.time,
                    ..bar.clone()
                };
                let hits = crate::chart::renderers::render_markers(
                    &RenderContext::new(plot, rect),
                    [(0, marker)],
                    &[bar],
                    &map,
                    &coords,
                );
                self.record_mark_hits(item.id, hits);
            }
        }
    }

    /// Paints a legend row for each of `items`, the first at `top`. The values follow a pointer
    /// over `rect`. Returns the bottom of the last row, or `top` without rows.
    pub(in crate::widget) fn paint_secondary_legend<'a>(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        top: f32,
        items: impl Iterator<Item = &'a PriceSeries>,
    ) -> f32 {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        let font = egui::FontId::proportional(ui.active_theme().text.ohlc);
        let row_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), self.config.text_color)
            .size()
            .y
            + ui.active_theme().spacing.sm;
        let main = self.state.shown();
        let mut bottom = top;
        for (row, item) in items.enumerate() {
            if self.config.show_indicator_labels || self.config.show_indicator_last_val {
                let selected_time = ui
                    .input(|input| input.pointer.hover_pos())
                    .filter(|point| rect.contains(*point))
                    .and_then(|point| {
                        main.get(
                            self.state
                                .time_scale()
                                .coord_to_idx_precise(point.x, rect.left(), rect.width())
                                .round()
                                .max(0.0) as usize,
                        )
                    })
                    .map(|bar| bar.time);
                let bar = match selected_time {
                    Some(time) => item
                        .bars
                        .binary_search_by_key(&time, |bar| bar.time)
                        .ok()
                        .map(|at| &item.bars[at])
                        .filter(|bar| !bar.is_whitespace()),
                    None => crate::model::last_with_value(&item.bars),
                };
                let formatter = self.series_label_formatter(item.id, PriceScaleMode::Normal);
                let value = bar.map_or_else(
                    || "∅".into(),
                    |bar| {
                        formatter.as_ref().map_or_else(
                            || {
                                let map = self.series_map(item, rect);
                                crate::scales::group_thousands(
                                    format!("{:.*}", map.price_precision().unwrap_or(2), bar.close),
                                    map.thousands_sep(),
                                )
                            },
                            |formatter| formatter.format(bar.close),
                        )
                    },
                );
                let label = plot.text(
                    Pos2::new(rect.left() + 8.0, top + row as f32 * row_height),
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
                    font.clone(),
                    item.options.color,
                );
                bottom = label.bottom();
            }
        }
        bottom
    }

    pub(in crate::widget) fn secondary_axes(
        &self,
        widget: Rect,
        rect: Rect,
    ) -> Vec<(SeriesScale, Rect)> {
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

    pub(in crate::widget) fn handle_secondary_axes(
        &mut self,
        ui: &egui::Ui,
        widget: Rect,
        rect: Rect,
    ) {
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

    pub(in crate::widget) fn paint_series_axes(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        widget: Rect,
        rect: Rect,
        colors: &StyleColors,
    ) -> f32 {
        let study_axis = crate::config::PriceAxisOptions {
            show_study_values: self.config.show_indicator_last_val
                && self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let mut measured = 0.0_f32;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let position = if key == SeriesScale::Right {
                crate::scales::PriceScaleId::Right
            } else {
                crate::scales::PriceScaleId::Left
            };
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            let assigned: Vec<_> = self
                .rendered_studies()
                .indicators()
                .iter()
                .filter(|study| {
                    study.is_visible()
                        && study.is_overlay()
                        && study.scale() == crate::studies::StudyScale::Shared(key)
                })
                .map(|study| study.as_ref())
                .collect();
            let Some(item) = self
                .main_pane_series()
                .find(|item| item.options.scale == key)
            else {
                if let Some(study) = assigned.first() {
                    let coords = crate::chart::renderers::ChartMapping {
                        rect,
                        price: study.scale_mapping().unwrap_or(self.last_price_map),
                        ..self.get_chart_mapping()
                    };
                    measured = measured.max(crate::chart::renderers::render_study_axis(
                        painter,
                        &assigned,
                        &coords,
                        axis,
                        &study_axis,
                        self.percentage_formatter.as_deref(),
                    ));
                }
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
            for (value, colors, mapping, label) in self.secondary_axis_values(key, rect) {
                tags.push(crate::chart::rendering::axes::render_axis_value(
                    &clipped,
                    label_rect,
                    axis.right(),
                    mapping.price_to_y(value),
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                ));
            }
            if study_axis.show_study_values {
                for study in &assigned {
                    let mapping = study
                        .scale_mapping()
                        .unwrap_or(map)
                        .with_bounds(rect.top(), rect.bottom())
                        .with_thousands_sep(map.thousands_sep());
                    crate::chart::rendering::axes::render_study_tags(
                        &clipped,
                        label_rect,
                        &mapping,
                        *study,
                        axis.right(),
                        &mut tags,
                        None,
                        &study_axis,
                    );
                }
            }
            let formatter = self.series_label_formatter(item.id, map.transform().mode);
            crate::chart::renderers::render_price_labels_at_pos(
                &ctx,
                &map,
                colors,
                &self.config.price_axis,
                position,
                &tags,
                slot,
                formatter.as_deref(),
            );
            measured = measured.max(crate::chart::rendering::axes::price_axis_width(
                &clipped,
                &map,
                &[],
                formatter.as_deref(),
            ));
            ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            )
            .on_hover_text(l10n::text(ui.ctx(), &item.options.name));
        }
        measured
    }
}
