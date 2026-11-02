//! Apply and read the chart settings exposed by the shared dialog.

use super::{
    BackgroundType, ButtonVisibility, ChartSettingsState, GridLinesMode, PrecisionMode,
    WatermarkMode,
};
use crate::{
    Chart,
    config::{BackgroundStyle, ChartConfig, NavigationButtons},
    model::CandlesStyle,
    scales::{PriceScaleMode, TimezoneMode},
    theme::Theme,
};
use egui::Color32;

/// The axis label size the dialog shows: the configured one, else the default theme's.
fn axis_font_size(config: &ChartConfig) -> u8 {
    config
        .axis_font_size
        .unwrap_or(Theme::default().text.label)
        .round() as u8
}

/// The axis line colour the dialog shows; transparent when the price axis draws no line.
fn axis_line_color(config: &ChartConfig) -> Color32 {
    if config.price_axis.border_visible {
        config
            .price_axis
            .border_color
            .unwrap_or(Theme::default().ui.border)
    } else {
        Color32::TRANSPARENT
    }
}

fn button_visibility(buttons: NavigationButtons) -> ButtonVisibility {
    match buttons {
        NavigationButtons::Always => ButtonVisibility::AlwaysVisible,
        NavigationButtons::OnHover => ButtonVisibility::VisibleOnMouseOver,
        NavigationButtons::Never => ButtonVisibility::Hidden,
    }
}

fn navigation_buttons(visibility: ButtonVisibility) -> NavigationButtons {
    match visibility {
        ButtonVisibility::AlwaysVisible => NavigationButtons::Always,
        ButtonVisibility::VisibleOnMouseOver => NavigationButtons::OnHover,
        ButtonVisibility::Hidden => NavigationButtons::Never,
    }
}

impl ChartSettingsState {
    /// Read the chart's current display settings. Host-managed trading and alert groups
    /// retain defaults; hosts preserve their current values when refreshing chart settings.
    pub fn from_chart(chart: &Chart) -> Self {
        let config = chart.config();
        let mut settings = Self {
            chart_type: chart.chart_type(),
            chart_type_params: Some(chart.chart_type_params().clone()),
            price_source: Some(chart.config().price_source),
            events: chart.event_visibility().clone(),
            order_flow: super::OrderFlowSettings {
                show_heatmap: chart
                    .feed_link
                    .as_ref()
                    .is_some_and(|link| link.heatmap.is_some()),
                heatmap: chart.liquidity_heatmap_config(),
                show_order_book: chart
                    .feed_link
                    .as_ref()
                    .is_some_and(|link| link.book_visible.unwrap_or(link.heatmap.is_some())),
                order_book: chart.order_book_view(),
                show_volume_bubbles: chart
                    .feed_link
                    .as_ref()
                    .is_some_and(|link| link.bubbles.is_some()),
                volume_bubbles: chart
                    .feed_link
                    .as_ref()
                    .map(|link| link.bubbles_config.clone())
                    .unwrap_or_default(),
            },
            show_price_targets: chart.price_targets_visible(),
            ..Self::default()
        };
        settings.candle_colors.body_up = config.bullish_color;
        settings.candle_colors.body_down = config.bearish_color;
        settings.candle_colors.border_up =
            config.bullish_border_color.unwrap_or(config.bullish_color);
        settings.candle_colors.border_down =
            config.bearish_border_color.unwrap_or(config.bearish_color);
        settings.candle_colors.wick_up = config
            .bullish_wick_color
            .unwrap_or(settings.candle_colors.border_up);
        settings.candle_colors.wick_down = config
            .bearish_wick_color
            .unwrap_or(settings.candle_colors.border_down);
        let candles = &chart.chart_type_params().styles.candles;
        settings.candle_colors.show_body = candles.body_visible;
        settings.candle_colors.show_border = config.candle_border_width > 0.0;
        settings.candle_colors.show_wick = candles.wick_visible;
        settings.candle_colors.color_by_previous_close = candles.color_by_previous_close;
        settings.chart_basic_styles.background_color = config.background_color;
        settings.grid_lines.horizontal_color = config.grid_color;
        settings.grid_lines.vertical_color =
            config.vertical_grid_color.unwrap_or(config.grid_color);
        settings.status_line.show_logo = config.show_legend_badge;
        settings.status_line.show_market_status = config.show_market_status;
        settings.status_line.show_symbol = config.show_symbol_labels;
        settings.status_line.title_as_description = config.title_as_description;
        settings.status_line.show_quote_boxes = config.show_quote_boxes;
        settings.status_line.show_ohlc = config.show_ohlc_info;
        settings.status_line.show_change = config.show_ohlc_change;
        settings.status_line.show_change_percent = config.show_ohlc_change_percent;
        settings.status_line.show_volume = config.show_legend_volume;
        settings.status_line.show_indicator_titles = config.show_indicator_labels;
        settings.status_line.show_indicator_inputs = config.show_indicator_inputs;
        settings.status_line.show_indicator_values = config.show_indicator_last_val;
        settings.status_line.show_last_day_change = config.show_last_day_change;
        settings.status_line.indicator_background_opacity =
            (config.indicator_backdrop_opacity.clamp(0.0, 1.0) * 100.0).round() as u8;
        settings.scales_and_lines.price_scale_left =
            !config.show_right_axis && config.show_left_axis;
        settings.scales_and_lines.show_currency = config.show_currency;
        settings.scales_and_lines.show_unit = config.show_unit;
        settings.scales_and_lines.show_scale_mode_buttons = config.show_scale_mode_buttons;
        settings.scales_and_lines.show_plus_button = config.show_plus_button;
        settings.scales_and_lines.allow_label_overlap = config.price_axis.allow_label_overlap;
        settings.scales_and_lines.show_study_values = config.price_axis.show_study_values;
        settings.scales_and_lines.study_value_color = config.price_axis.study_value_color;
        settings.scales_and_lines.show_symbol_name = config.price_axis.show_symbol_name;
        settings.scales_and_lines.show_study_names = config.price_axis.show_study_names;
        settings.scales_and_lines.show_extended_session_price = config.show_extended_session_price;
        settings.scales_and_lines.extended_session_price_color =
            config.extended_session_price_color;
        settings.scales_and_lines.symbol_price_color = chart
            .series_style(crate::widget::SeriesId::MAIN)
            .and_then(|style| style.price_line_color);
        settings.scales_and_lines.lock_price_to_bar_ratio =
            chart.state().price_scale().bar_ratio_locked();
        settings.scales_and_lines.price_to_bar_ratio =
            chart.state().price_scale().bar_ratio().unwrap_or(1.0);
        settings.scales_and_lines.show_last_price_label = config.show_symbol_last_val;
        settings.scales_and_lines.show_price_line = config.show_price_line;
        settings.scales_and_lines.show_prev_close_label = config.show_symbol_prev_close;
        settings.scales_and_lines.show_prev_close = config.show_prev_close_line;
        settings.scales_and_lines.show_countdown = config.show_countdown;
        settings.scales_and_lines.show_bid_ask = config.show_bid_ask;
        settings.scales_and_lines.show_high_low = config.show_high_low;
        settings.margins.top_percent =
            (chart.state().price_scale().options().scale_margins.top * 100.0) as f32;
        settings.margins.bottom_percent =
            (chart.state().price_scale().options().scale_margins.bottom * 100.0) as f32;
        settings.margins.right_bars = chart.chart_options().time_scale.right_offset.max(0.0) as u32;
        let scale = chart.state().price_scale();
        settings.scales_and_lines.auto_scale = !scale.is_manual();
        settings.scales_and_lines.log_scale = scale.options().mode == PriceScaleMode::Logarithmic;
        settings.scales_and_lines.percentage_scale =
            scale.options().mode == PriceScaleMode::Percentage;
        settings.scales_and_lines.invert_scale = scale.options().invert_scale;

        settings.timezone = match &chart.chart_options().time_scale.timezone {
            TimezoneMode::Utc => "UTC".into(),
            TimezoneMode::Local => "Local".into(),
            TimezoneMode::Timezone(zone) => zone.to_string(),
            TimezoneMode::Exchange(zone) => format!("Exchange:{zone}"),
        };
        settings.grid_lines.mode = match (
            config.show_grid && config.show_horizontal_grid,
            config.show_grid && config.show_vertical_grid,
        ) {
            (true, true) => GridLinesMode::Both,
            (true, false) => GridLinesMode::Horizontal,
            (false, true) => GridLinesMode::Vertical,
            _ => GridLinesMode::None,
        };
        settings.btns.navigation = button_visibility(config.navigation_buttons);
        let (kind, first, second) = match config.background_style {
            BackgroundStyle::Solid => (
                BackgroundType::Solid,
                config.background_color,
                config.background_color,
            ),
            BackgroundStyle::VerticalGradient {
                top_color,
                bottom_color,
            } => (BackgroundType::VerticalGradient, top_color, bottom_color),
            BackgroundStyle::HorizontalGradient {
                left_color,
                right_color,
            } => (BackgroundType::HorizontalGradient, left_color, right_color),
        };
        settings.chart_basic_styles.background_type = kind;
        settings.chart_basic_styles.background_gradient_top = first;
        settings.chart_basic_styles.background_gradient_bottom = second;
        let crosshair = &chart.chart_options().crosshair;
        settings.crosshair.color = crosshair.horz_line_color;
        settings.crosshair.line_width = crosshair.horz_line_width;
        settings.crosshair.line_style = crosshair.horz_line_style;
        let decimals = chart
            .state()
            .price_scale()
            .map(Default::default(), 0.0, 1.0)
            .price_precision();
        settings.precision = PrecisionMode::all()
            .iter()
            .copied()
            .find(|mode| mode.decimals() == decimals)
            .unwrap_or_default();
        settings.watermark.mode = if !config.show_watermark {
            WatermarkMode::ReplayMode
        } else if config.watermark_text.is_some() {
            WatermarkMode::Custom
        } else if config.watermark_description {
            WatermarkMode::SymbolAndDescription
        } else {
            WatermarkMode::Symbol
        };
        settings.watermark.color = config.watermark_color;
        settings.scales_appearance.text_color = config.text_color;
        settings.scales_appearance.font_size = axis_font_size(config);
        settings.scales_appearance.lines_color = axis_line_color(config);
        settings.btns.pane = button_visibility(config.pane_buttons);

        settings
    }

    pub(crate) fn validate(&self) -> Result<TimezoneMode, &'static str> {
        let timezone = match self.timezone.as_str() {
            "UTC" => TimezoneMode::Utc,
            "Local" => TimezoneMode::Local,
            name => {
                if let Some(name) = name.strip_prefix("Exchange:") {
                    TimezoneMode::Exchange(name.parse().map_err(|_| "Invalid timezone")?)
                } else {
                    TimezoneMode::Timezone(name.parse().map_err(|_| "Invalid timezone")?)
                }
            }
        };
        let top = self.margins.top_percent;
        let bottom = self.margins.bottom_percent;
        if !top.is_finite()
            || !bottom.is_finite()
            || top < 0.0
            || bottom < 0.0
            || top + bottom >= 100.0
        {
            return Err("Margins must leave room for prices");
        }
        if !self.crosshair.line_width.is_finite() || self.crosshair.line_width <= 0.0 {
            return Err("Crosshair width must be above zero");
        }
        if self.scales_appearance.font_size == 0 {
            return Err("Font sizes must be between 1 and 256");
        }
        if self.scales_and_lines.lock_price_to_bar_ratio
            && (!self.scales_and_lines.price_to_bar_ratio.is_finite()
                || self.scales_and_lines.price_to_bar_ratio <= 0.0)
        {
            return Err("Price-to-bar ratio must be positive and finite");
        }
        for volume in [
            self.trading.execution_sound_volume,
            self.alerts.sound_volume,
        ] {
            if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
                return Err("Sound volume must be between zero and one");
            }
        }
        if let Some(params) = &self.chart_type_params
            && let Some(error) = params.invalid(self.chart_type)
        {
            return Err(error);
        }
        Ok(timezone)
    }

    /// Apply the display controls offered by the dialog without rebuilding the chart or feed.
    /// Host-owned trading and alerts are left to their providers. Invalid timezone,
    /// margins or crosshair width returns an error before changing the chart.
    pub fn apply_to_chart(&self, chart: &mut Chart) -> Result<(), &'static str> {
        let timezone = self.validate()?;
        if let Some(params) = &self.chart_type_params {
            if let Some(error) = params.invalid(chart.chart_type()) {
                return Err(error);
            }
            chart.set_chart_type_params(params.clone());
        }
        if let Some(source) = self.price_source {
            chart.config_mut().price_source = source;
        }
        chart.set_liquidity_heatmap_config(self.order_flow.heatmap);
        chart.set_liquidity_heatmap(self.order_flow.show_heatmap);
        chart.set_order_book_view(self.order_flow.order_book.clone());
        chart.set_order_book_visible(self.order_flow.show_order_book);
        if let Some(link) = &mut chart.feed_link {
            link.bubbles_config = self.order_flow.volume_bubbles.clone();
        }
        chart.set_volume_bubbles(
            self.order_flow
                .show_volume_bubbles
                .then(|| self.order_flow.volume_bubbles.clone()),
        );
        chart.set_event_visibility(self.events.clone());
        chart.set_price_targets_visible(self.show_price_targets);
        chart.chart_options.time_scale.timezone = timezone;
        if self.scales_appearance.lines_color != axis_line_color(chart.config()) {
            // A transparent line is no line; any other colour draws on the price axes and the time axis.
            let line = (self.scales_appearance.lines_color != Color32::TRANSPARENT)
                .then_some(self.scales_appearance.lines_color);
            chart.config_mut().price_axis.border_visible = line.is_some();
            chart.config_mut().price_axis.border_color = line;
            chart.chart_options.time_scale.border_visible = line.is_some();
            chart.chart_options.time_scale.border_color = line;
        }
        let crosshair = &mut chart.chart_options.crosshair;
        crosshair.horz_line_color = self.crosshair.color;
        crosshair.vert_line_color = self.crosshair.color;
        crosshair.horz_line_width = self.crosshair.line_width;
        crosshair.vert_line_width = self.crosshair.line_width;
        crosshair.horz_line_style = self.crosshair.line_style;
        crosshair.vert_line_style = crosshair.horz_line_style;
        let config = chart.config_mut();
        config.bullish_color = self.candle_colors.body_up;
        config.bearish_color = self.candle_colors.body_down;
        config.bullish_border_color = Some(self.candle_colors.border_up);
        config.bearish_border_color = Some(self.candle_colors.border_down);
        config.bullish_wick_color = Some(self.candle_colors.wick_up);
        config.bearish_wick_color = Some(self.candle_colors.wick_down);
        // Borders keep a width the host chose and take a hairline when switched on.
        if !self.candle_colors.show_border {
            config.candle_border_width = 0.0;
        } else if config.candle_border_width <= 0.0 {
            config.candle_border_width = 1.0;
        }

        if self.chart_basic_styles.background_color != Color32::PLACEHOLDER {
            config.background_color = self.chart_basic_styles.background_color;
        }
        config.grid_color = self.grid_lines.horizontal_color;

        let (h, v) = match self.grid_lines.mode {
            GridLinesMode::None => (false, false),
            GridLinesMode::Horizontal => (true, false),
            GridLinesMode::Vertical => (false, true),
            GridLinesMode::Both => (true, true),
        };
        config.show_horizontal_grid = h;
        config.show_vertical_grid = v;
        config.show_grid = h || v;

        config.navigation_buttons = navigation_buttons(self.btns.navigation);
        config.pane_buttons = navigation_buttons(self.btns.pane);
        config.show_ohlc_info = self.status_line.show_ohlc;
        config.show_legend_badge = self.status_line.show_logo;
        config.show_market_status = self.status_line.show_market_status;
        config.show_last_day_change = self.status_line.show_last_day_change;
        config.indicator_backdrop_opacity =
            f32::from(self.status_line.indicator_background_opacity.min(100)) / 100.0;
        config.show_symbol_labels = self.status_line.show_symbol;
        config.title_as_description = self.status_line.title_as_description;
        config.show_quote_boxes = self.status_line.show_quote_boxes;
        config.show_ohlc_change = self.status_line.show_change;
        config.show_ohlc_change_percent = self.status_line.show_change_percent;
        config.show_legend_volume = self.status_line.show_volume;
        config.show_indicator_labels = self.status_line.show_indicator_titles;
        config.show_indicator_inputs = self.status_line.show_indicator_inputs;
        config.show_indicator_last_val = self.status_line.show_indicator_values;
        config.show_right_axis = !self.scales_and_lines.price_scale_left;
        config.show_left_axis = self.scales_and_lines.price_scale_left;
        config.show_currency = self.scales_and_lines.show_currency;
        config.show_unit = self.scales_and_lines.show_unit;
        config.show_scale_mode_buttons = self.scales_and_lines.show_scale_mode_buttons;
        config.show_plus_button = self.scales_and_lines.show_plus_button;
        config.price_axis.allow_label_overlap = self.scales_and_lines.allow_label_overlap;
        config.price_axis.show_study_values = self.scales_and_lines.show_study_values;
        config.price_axis.study_value_color = self.scales_and_lines.study_value_color;
        config.price_axis.show_symbol_name = self.scales_and_lines.show_symbol_name;
        config.price_axis.show_study_names = self.scales_and_lines.show_study_names;
        config.show_extended_session_price = self.scales_and_lines.show_extended_session_price;
        config.extended_session_price_color = self.scales_and_lines.extended_session_price_color;
        config.lock_price_to_bar_ratio = self.scales_and_lines.lock_price_to_bar_ratio;
        config.show_symbol_last_val = self.scales_and_lines.show_last_price_label;
        config.show_price_line = self.scales_and_lines.show_price_line;
        config.show_symbol_prev_close = self.scales_and_lines.show_prev_close_label;
        config.show_prev_close_line = self.scales_and_lines.show_prev_close;
        config.show_countdown = self.scales_and_lines.show_countdown;
        config.show_bid_ask = self.scales_and_lines.show_bid_ask;
        config.show_high_low = self.scales_and_lines.show_high_low;

        config.vertical_grid_color = Some(self.grid_lines.vertical_color);
        let resolved = |color| {
            if color == Color32::PLACEHOLDER {
                config.background_color
            } else {
                color
            }
        };
        let first = resolved(self.chart_basic_styles.background_gradient_top);
        let second = resolved(self.chart_basic_styles.background_gradient_bottom);
        config.background_style = match self.chart_basic_styles.background_type {
            BackgroundType::Solid => BackgroundStyle::Solid,
            BackgroundType::VerticalGradient => BackgroundStyle::VerticalGradient {
                top_color: first,
                bottom_color: second,
            },
            BackgroundType::HorizontalGradient => BackgroundStyle::HorizontalGradient {
                left_color: first,
                right_color: second,
            },
        };
        config.text_color = self.scales_appearance.text_color;
        // Values the dialog did not change keep following the theme.
        if self.scales_appearance.font_size != axis_font_size(config) {
            config.axis_font_size = Some(f32::from(self.scales_appearance.font_size));
        }
        config.show_watermark = self.watermark.mode != WatermarkMode::ReplayMode;
        config.watermark_color = self.watermark.color;
        if self.watermark.mode != WatermarkMode::Custom {
            config.watermark_text = None;
            config.watermark_description =
                self.watermark.mode == WatermarkMode::SymbolAndDescription;
        }
        chart.price_scale_mut().options_mut().scale_margins = crate::scales::PriceScaleMargins {
            top: f64::from(self.margins.top_percent.clamp(0.0, 100.0)) / 100.0,
            bottom: f64::from(self.margins.bottom_percent.clamp(0.0, 100.0)) / 100.0,
        };
        if self.margins.right_bars != chart.chart_options().time_scale.right_offset.max(0.0) as u32
        {
            chart.set_right_margin(self.margins.right_bars as f32);
        }
        let mode = if self.scales_and_lines.log_scale {
            PriceScaleMode::Logarithmic
        } else if self.scales_and_lines.percentage_scale {
            PriceScaleMode::Percentage
        } else if chart.state.price_scale().options().mode == PriceScaleMode::IndexedTo100 {
            PriceScaleMode::IndexedTo100
        } else {
            PriceScaleMode::Normal
        };
        // The mode belongs to the axis the main scale is drawn on, which the configured mode
        // would otherwise override at the next frame.
        let config = chart.config_mut();
        if config.show_left_axis && !config.show_right_axis {
            config.left_axis_scale_mode = mode;
        } else {
            config.right_axis_scale_mode = mode;
        }
        chart.applied_config_scale_mode = mode;
        if mode != chart.state.price_scale().options().mode {
            chart.state.price_scale_mut().reset();
            chart.state.clear_zoom_history();
        }
        if self.scales_and_lines.auto_scale {
            chart.state.price_scale_mut().reset();
        } else if !chart.state.price_scale().is_manual() {
            let range = chart.state.price_scale().range();
            chart
                .state
                .price_scale_mut()
                .set_range(range.min, range.max);
        }
        let options = chart.state.price_scale_mut().options_mut();
        options.mode = mode;
        options.invert_scale = self.scales_and_lines.invert_scale;
        let precision = self
            .precision
            .decimals()
            .map(|value| value as u8)
            .or_else(|| {
                chart
                    .instrument()
                    .and_then(crate::scales::instrument_price_precision)
            });
        chart.state.price_scale_mut().set_price_precision(precision);
        if let Some(style) = chart.series_style_mut(crate::widget::SeriesId::MAIN) {
            style.price_line_color = self.scales_and_lines.symbol_price_color;
        }
        chart.price_scale_mut().set_bar_ratio(
            self.scales_and_lines
                .lock_price_to_bar_ratio
                .then_some(self.scales_and_lines.price_to_bar_ratio),
        );
        chart.last_ratio_lock = self.scales_and_lines.lock_price_to_bar_ratio;
        let candles = CandlesStyle {
            body_visible: self.candle_colors.show_body,
            wick_visible: self.candle_colors.show_wick,
            color_by_previous_close: self.candle_colors.color_by_previous_close,
        };
        // Other chart type parameters re-resolve their box sizes when set, so only a change is set.
        if chart.chart_type_params().styles.candles != candles {
            let mut params = chart.chart_type_params().clone();
            params.styles.candles = candles;
            chart.set_chart_type_params(params);
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, Instrument, quote::SessionQuote};
    use crate::studies::{CustomIndicator, IndicatorValue};
    use crate::theme::Theme;
    use egui::{Context, Shape, Vec2};

    #[cfg(feature = "drawings")]
    struct DepthFeed;

    #[cfg(feature = "drawings")]
    impl crate::data::Datafeed for DepthFeed {
        fn resolve(&self, symbol: &str, reply: crate::data::Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
        }

        fn history(
            &self,
            _: crate::data::HistoryRequest,
            reply: crate::data::Reply<crate::data::HistoryPage>,
        ) {
            reply.send(Ok(crate::data::HistoryPage::new(
                vec![Bar::new(
                    chrono::DateTime::UNIX_EPOCH,
                    100.0,
                    110.0,
                    90.0,
                    105.0,
                    0.0,
                )],
                true,
            )));
        }

        fn subscribe(&self, subscription: crate::data::Subscription) {
            subscription.sink.push_depth_snapshot(
                1,
                crate::model::OrderBook {
                    ts: chrono::DateTime::UNIX_EPOCH,
                    symbol: subscription.symbol,
                    bids: vec![crate::model::OrderBookLevel {
                        price: 99.0,
                        size: 10.0,
                        order_cnt: None,
                    }],
                    asks: vec![],
                },
            );
        }
    }

    /// Saved layers and disabled preferences survive a layout round trip, while each book
    /// profile follows its own switch, irrespective of the heatmap or the chart type.
    #[test]
    #[cfg(feature = "drawings")]
    fn order_flow_settings_round_trip_and_render_independently() {
        use crate::chart::overlays::{BubbleColor, BubbleGrouping, BubbleScalingMode};
        use crate::widget::{ChartBuilder, ChartSnapshot};
        for (kind, clamp_std_devs) in [
            (crate::model::ChartType::Candles, 3.0),
            (crate::model::ChartType::Line, f64::INFINITY),
        ] {
            for flags in 0..8 {
                let ctx = Context::default();
                let mut chart = ChartBuilder::new()
                    .with_feed(std::sync::Arc::new(DepthFeed))
                    .with_symbol("A")
                    .with_timeframe(crate::model::Timeframe::Min1)
                    .with_chart_type(kind)
                    .build();
                chart.set_size(Some(Vec2::new(800.0, 600.0)));
                for _ in 0..4 {
                    ctx.run_ui(Default::default(), |ui| {
                        chart.show(ui);
                    })
                    .drop_without_applying_deltas();
                }
                let mut settings = ChartSettingsState::from_chart(&chart);
                assert_eq!(
                    settings.order_flow,
                    super::super::OrderFlowSettings::default()
                );
                let flow = &mut settings.order_flow;
                flow.show_heatmap = flags & 1 != 0;
                flow.show_order_book = flags & 2 != 0;
                flow.show_volume_bubbles = flags & 4 != 0;
                flow.heatmap.brightness = 0.5;
                flow.heatmap.lower_cutoff = 2.0;
                flow.heatmap.full_intensity_size = Some(20.0);
                flow.volume_bubbles.color_by = BubbleColor::Split;
                flow.volume_bubbles.grouping =
                    BubbleGrouping::TimeWindow(chrono::TimeDelta::milliseconds(250));
                flow.volume_bubbles.scaling_mode = BubbleScalingMode::SquareRoot;
                flow.volume_bubbles.min_size_filter = Some(3.0);
                flow.volume_bubbles.clamp_std_devs = clamp_std_devs;
                settings.apply_to_chart(&mut chart).unwrap();
                assert_eq!(chart.chart_type(), kind);
                assert_eq!(
                    ChartSettingsState::from_chart(&chart).order_flow,
                    settings.order_flow
                );
                let output = ctx.run_ui(Default::default(), |ui| {
                    chart.show(ui);
                });
                let book_drawn = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                    Shape::Rect(rect) if (rect.rect.width() - 60.0).abs() < 0.01
                        && rect.fill == chart.config().bullish_color.gamma_multiply(0.7))
                });
                output.drop_without_applying_deltas();
                assert_eq!(
                    book_drawn, settings.order_flow.show_order_book,
                    "flags={flags}"
                );
                let saved = chart.snapshot().unwrap();
                let json = serde_json::to_value(&saved).unwrap();
                let loaded: ChartSnapshot = serde_json::from_value(json.clone()).unwrap();
                chart.restore(&loaded).unwrap();
                assert_eq!(
                    ChartSettingsState::from_chart(&chart).order_flow,
                    settings.order_flow
                );
                let mut legacy = json;
                legacy["settings"]
                    .as_object_mut()
                    .unwrap()
                    .remove("order_flow");
                chart
                    .restore(&serde_json::from_value(legacy).unwrap())
                    .unwrap();
                assert_eq!(
                    ChartSettingsState::from_chart(&chart).order_flow,
                    super::super::OrderFlowSettings::default()
                );
            }
        }
    }

    #[test]
    fn scale_settings_render_values_and_preserve_the_entered_ratio() {
        for theme in [Theme::light(), Theme::dark()] {
            for (left, mode) in [
                (false, PriceScaleMode::Normal),
                (true, PriceScaleMode::Normal),
                (false, PriceScaleMode::Logarithmic),
                (true, PriceScaleMode::Logarithmic),
                (false, PriceScaleMode::Percentage),
                (true, PriceScaleMode::Percentage),
            ] {
                for width in [320.0, 800.0] {
                    let locked_ratio = if mode == PriceScaleMode::Logarithmic {
                        0.005
                    } else {
                        0.5
                    };
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let bars = (0..10)
                        .map(|i| {
                            Bar::new(
                                chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i),
                                100.0,
                                120.0,
                                90.0,
                                112.0,
                                0.0,
                            )
                        })
                        .collect();
                    let mut chart = Chart::new(BarData::from_bars(bars));
                    chart.set_size(Some(Vec2::new(width, 600.0)));
                    chart.set_instrument(Some(
                        Instrument::new("SAMPLE")
                            .with_currency("USD")
                            .with_unit("contracts per barrel"),
                    ));
                    chart.set_session_quote(Some(SessionQuote::new(115.0, "EXT")));
                    chart.add_indicator(Box::new(CustomIndicator::new("Constant", |bars| {
                        vec![IndicatorValue::Single(110.0); bars.len()]
                    })));
                    for _ in 0..2 {
                        ctx.run_ui(Default::default(), |ui| {
                            chart.show(ui);
                        })
                        .drop_without_applying_deltas();
                    }
                    let mut settings = ChartSettingsState::from_chart(&chart);
                    let scales = &mut settings.scales_and_lines;
                    scales.price_scale_left = left;
                    scales.log_scale = mode == PriceScaleMode::Logarithmic;
                    scales.percentage_scale = mode == PriceScaleMode::Percentage;
                    scales.show_scale_mode_buttons = false;
                    scales.show_plus_button = false;
                    scales.allow_label_overlap = true;
                    scales.auto_scale = false;
                    scales.lock_price_to_bar_ratio = true;
                    scales.price_to_bar_ratio = locked_ratio;
                    scales.symbol_price_color = Some(Color32::GOLD);
                    scales.study_value_color = Some(Color32::PURPLE);
                    scales.extended_session_price_color = Color32::LIGHT_RED;
                    settings.apply_to_chart(&mut chart).unwrap();
                    assert_eq!(
                        ChartSettingsState::from_chart(&chart).scales_and_lines,
                        settings.scales_and_lines
                    );
                    for (currency, unit, shown) in [
                        (true, true, true),
                        (true, false, true),
                        (false, true, true),
                        (false, false, false),
                    ] {
                        settings.scales_and_lines.show_currency = currency;
                        settings.scales_and_lines.show_unit = unit;
                        settings.scales_and_lines.show_study_values = shown;
                        settings.scales_and_lines.show_last_price_label = shown;
                        settings.scales_and_lines.show_extended_session_price = shown;
                        settings.apply_to_chart(&mut chart).unwrap();
                        for frame in 0..3 {
                            let mut output = ctx.run_ui(Default::default(), |ui| {
                                chart.show(ui);
                            });
                            let shapes = std::mem::take(&mut output.shapes);
                            output.drop_without_applying_deltas();
                            if frame == 2 {
                                let plot = chart.last_rendered_price_rect;
                                let expected = match (currency, unit) {
                                    (true, true) => Some("USD · contracts per barrel"),
                                    (true, false) => Some("USD"),
                                    (false, true) => Some("contracts per barrel"),
                                    _ => None,
                                };
                                let header = shapes.iter().find_map(|s| match &s.shape {
                                    Shape::Text(t)
                                        if [
                                            "USD · contracts per barrel",
                                            "USD",
                                            "contracts per barrel",
                                        ]
                                        .contains(&t.galley.text()) =>
                                    {
                                        Some(t.galley.text())
                                    }
                                    _ => None,
                                });
                                assert_eq!(header, expected);
                                if let Some(expected) = expected {
                                    let header_fits = shapes.iter().any(|s| match &s.shape {
                                        Shape::Text(t) if t.galley.text() == expected => {
                                            s.clip_rect.contains_rect(egui::Rect::from_min_size(
                                                t.pos,
                                                t.galley.size(),
                                            ))
                                        }
                                        _ => false,
                                    });
                                    assert!(header_fits, "unit header must fit its axis column");
                                }
                                for color in [Color32::GOLD, Color32::PURPLE, Color32::LIGHT_RED] {
                                    let tag = shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == color && (r.rect.right() <= plot.left() || r.rect.left() >= plot.right())));
                                    assert_eq!(tag, shown, "{color:?}, left={left}");
                                }
                                // A 0.5 unit/bar ratio and 6 point bars over 600 points imply
                                // a 50 unit span. Use actual plot height and spacing after layout.
                                let scale = chart.state().price_scale();
                                let span = scale.range().max - scale.range().min;
                                let ratio = span
                                    * f64::from(
                                        chart.state().time_scale().bar_spacing() / plot.height(),
                                    );
                                assert!((ratio - locked_ratio).abs() < 1e-6, "{ratio}");
                                if mode == PriceScaleMode::Logarithmic {
                                    assert!(
                                        (chart
                                            .last_price_map
                                            .transform()
                                            .to_scale(std::f64::consts::E)
                                            - 1.0)
                                            .abs()
                                            < 1e-12
                                    );
                                }
                            }
                        }
                    }
                    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
                        settings.scales_and_lines.price_to_bar_ratio = invalid;
                        assert!(settings.apply_to_chart(&mut chart).is_err());
                        assert_eq!(chart.state().price_scale().bar_ratio(), Some(locked_ratio));
                    }
                }
            }
        }
    }

    /// The saved scale font size, axis line colour and pane button visibility render on the main,
    /// study and time axes and in a maximized pane, and read back from the chart.
    #[test]
    fn scale_appearance_and_pane_button_settings_render_and_read_back() {
        use crate::studies::RSI;
        use crate::widget::test_frames::{chart, frame, shapes};
        for (visibility, maximized) in [
            (ButtonVisibility::AlwaysVisible, false),
            (ButtonVisibility::VisibleOnMouseOver, false),
            (ButtonVisibility::VisibleOnMouseOver, true),
            (ButtonVisibility::Hidden, false),
            (ButtonVisibility::Hidden, true),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            let study = chart.add_indicator(Box::new(RSI::new(14)));
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.pane_layout.maximized = maximized.then(|| chart.series_pane(study).unwrap());
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.scales_appearance.font_size = 18;
            settings.scales_appearance.lines_color = Color32::RED;
            settings.btns.pane = visibility;
            settings.apply_to_chart(&mut chart).unwrap();
            assert_eq!(
                ChartSettingsState::from_chart(&chart).scales_appearance,
                settings.scales_appearance
            );
            assert_eq!(ChartSettingsState::from_chart(&chart).btns, settings.btns);
            frame(&ctx, &mut chart, 0.1, Vec::new());
            let painted = shapes(&ctx, &mut chart, 0.2, Vec::new());
            let pane = chart.get_rendered_indicator_panes()[0].clone();
            let plot = if maximized {
                pane.chart_rect
            } else {
                chart.get_rendered_price_rect()
            };
            for rect in [plot, pane.chart_rect] {
                let axis_text = painted.iter().filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.pos.x >= rect.right() && rect.y_range().contains(text.pos.y) =>
                    {
                        Some(text.galley.job.sections[0].format.font_id.size)
                    }
                    _ => None,
                });
                let sizes: Vec<_> = axis_text.collect();
                assert!(
                    !sizes.is_empty() && sizes.iter().all(|size| *size == 18.0),
                    "{sizes:?}"
                );
                let line = painted.iter().any(|shape| {
                    matches!(shape, Shape::LineSegment { points, stroke }
                    if stroke.color == Color32::RED
                        && points[0].x == points[1].x
                        && points[0].x >= rect.right()
                        && rect.y_range().contains(points[0].y.max(points[1].y) - 1.0))
                });
                assert!(line, "axis line beside {rect:?}");
            }
            let button = egui::Rect::from_min_size(
                egui::Pos2::new(pane.chart_rect.right() - 22.0, pane.panel_rect.top() + 2.0),
                Vec2::splat(20.0),
            );
            let menu = painted.iter().any(|shape| {
                matches!(shape, Shape::Text(text)
                if text.galley.text() == "..." && pane.panel_rect.contains(text.pos))
            });
            assert_eq!(menu, visibility == ButtonVisibility::AlwaysVisible);
            let icon = painted
                .iter()
                .any(|shape| matches!(shape, Shape::Rect(rect) if button.contains_rect(rect.rect)));
            assert_eq!(
                icon,
                visibility == ButtonVisibility::AlwaysVisible
                    || (visibility == ButtonVisibility::VisibleOnMouseOver && maximized),
                "{visibility:?}, maximized={maximized}"
            );
            // A layout saved with this size could not be restored.
            settings.scales_appearance.font_size = 0;
            assert!(settings.apply_to_chart(&mut chart).is_err());
        }
    }

    /// Each watermark mode paints its lines for the symbol, interval and description shown, also
    /// after they change without the settings being applied again, and reads back as chosen. At
    /// the default font size of 48, custom text keeps that size; the symbol line is 96 and the
    /// description 44, whose capitals are about 68 and 31 pixels high in the interface font.
    #[test]
    fn watermark_modes_follow_symbol_and_interval_changes() {
        use crate::model::Timeframe;
        use crate::widget::test_frames::{chart, shapes};
        let color = Color32::from_rgb(1, 2, 3);
        for mode in WatermarkMode::all().iter().copied() {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config_mut().watermark_text =
                (mode == WatermarkMode::Custom).then(|| "Desk 4\nNight".to_owned());
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.watermark.mode = mode;
            settings.watermark.color = color;
            settings.apply_to_chart(&mut chart).unwrap();
            for (symbol, interval, description, title) in [
                ("AAPL", Timeframe::Day1, "Apple Inc", "AAPL, 1D"),
                ("MSFT", Timeframe::Day1, "Microsoft Corp", "MSFT, 1D"),
                ("MSFT", Timeframe::Hour4, "Microsoft Corp", "MSFT, 4h"),
            ] {
                chart.set_symbol(symbol);
                chart.set_timeframe(interval);
                chart.set_instrument(Some(Instrument::new(symbol).with_description(description)));
                let painted: Vec<_> = shapes(&ctx, &mut chart, 0.0, Vec::new())
                    .into_iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text) if text.fallback_color == color => Some((
                            text.galley.text().to_owned(),
                            text.galley.job.sections[0].format.font_id.size,
                        )),
                        _ => None,
                    })
                    .collect();
                let expected: Vec<(String, f32)> = match mode {
                    WatermarkMode::ReplayMode => vec![],
                    WatermarkMode::Symbol => vec![(title.into(), 96.0)],
                    WatermarkMode::SymbolAndDescription => {
                        vec![(title.into(), 96.0), (description.into(), 44.0)]
                    }
                    WatermarkMode::Custom => vec![("Desk 4".into(), 48.0), ("Night".into(), 48.0)],
                };
                assert_eq!(painted, expected, "{mode:?} {title}");
                assert_eq!(
                    ChartSettingsState::from_chart(&chart).watermark.mode,
                    mode,
                    "{title}"
                );
            }
        }
    }
}
