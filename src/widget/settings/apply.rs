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

/// Why [`ChartSettingsState::apply_to_chart`] left the chart unchanged. Its text is the one
/// the settings dialog shows.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ChartSettingsError {
    /// `timezone` names no time zone this build knows.
    Timezone(crate::model::ParseError),
    /// The margins are not finite, are negative or leave no room for prices.
    Margins {
        /// The top margin in percent.
        top: f32,
        /// The bottom margin in percent.
        bottom: f32,
    },
    /// The crosshair line width is not finite or not above zero.
    CrosshairWidth(f32),
    /// The axis font size is zero.
    FontSize(u8),
    /// The locked price-to-bar ratio is not finite or not above zero.
    PriceToBarRatio(f64),
    /// The trading execution sound volume is outside 0 to 1.
    ExecutionSoundVolume(f32),
    /// The alert sound volume is outside 0 to 1.
    AlertSoundVolume(f32),
    /// The chart type parameters do not fit the chart type.
    ChartTypeParams {
        /// The chart type they were checked against.
        chart_type: crate::model::ChartType,
        /// Why they were refused.
        reason: &'static str,
    },
}

impl std::fmt::Display for ChartSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Timezone(_) => "Invalid timezone",
            Self::Margins { .. } => "Margins must leave room for prices",
            Self::CrosshairWidth(_) => "Crosshair width must be above zero",
            Self::FontSize(_) => "Font sizes must be between 1 and 256",
            Self::PriceToBarRatio(_) => "Price-to-bar ratio must be positive and finite",
            Self::ExecutionSoundVolume(_) | Self::AlertSoundVolume(_) => {
                "Sound volume must be between zero and one"
            }
            Self::ChartTypeParams { reason, .. } => reason,
        })
    }
}
impl std::error::Error for ChartSettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Timezone(error) => Some(error),
            _ => None,
        }
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
        if chart.chart_type() == crate::model::ChartType::HollowCandles {
            let style = &chart.chart_type_params().styles.hollow;
            settings.candle_colors = super::CandleColorConfig {
                color_by_previous_close: true,
                show_body: style.body_visible,
                show_border: style.border_visible,
                show_wick: style.wick_visible,
                body_up: style.body_up,
                body_down: style.body_down,
                border_up: style.border_up,
                border_down: style.border_down,
                wick_up: style.wick_up,
                wick_down: style.wick_down,
            };
        }
        if chart.chart_type() == crate::model::ChartType::VolumeCandles {
            let style = &chart.chart_type_params().styles.volume_candles;
            settings.candle_colors = super::CandleColorConfig {
                color_by_previous_close: style.color_by_previous_close,
                show_body: style.body_visible,
                show_border: style.border_visible,
                show_wick: style.wick_visible,
                body_up: style.body_up,
                body_down: style.body_down,
                border_up: style.border_up,
                border_down: style.border_down,
                wick_up: style.wick_up,
                wick_down: style.wick_down,
            };
        }
        if chart.chart_type() == crate::model::ChartType::Heikin {
            let style = &chart.chart_type_params().styles.heikin;
            settings.candle_colors = super::CandleColorConfig {
                color_by_previous_close: style.color_by_previous_close,
                show_body: style.body_visible,
                show_border: style.border_visible,
                show_wick: style.wick_visible,
                body_up: style.body_up,
                body_down: style.body_down,
                border_up: style.border_up,
                border_down: style.border_down,
                wick_up: style.wick_up,
                wick_down: style.wick_down,
            };
        }
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
        settings.scales_and_lines.axis_layout = chart
            .pane_axis_options(chart.main_series_pane())
            .unwrap_or_default();
        settings.scales_and_lines.price_scale_left = chart.main_axis_on_left();
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

    pub(crate) fn validate(&self) -> Result<TimezoneMode, ChartSettingsError> {
        let timezone = match self.timezone.as_str() {
            "UTC" => TimezoneMode::Utc,
            "Local" => TimezoneMode::Local,
            name => {
                if let Some(name) = name.strip_prefix("Exchange:") {
                    TimezoneMode::Exchange(name.parse().map_err(ChartSettingsError::Timezone)?)
                } else {
                    TimezoneMode::Timezone(name.parse().map_err(ChartSettingsError::Timezone)?)
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
            return Err(ChartSettingsError::Margins { top, bottom });
        }
        if !self.crosshair.line_width.is_finite() || self.crosshair.line_width <= 0.0 {
            return Err(ChartSettingsError::CrosshairWidth(
                self.crosshair.line_width,
            ));
        }
        if self.scales_appearance.font_size == 0 {
            return Err(ChartSettingsError::FontSize(
                self.scales_appearance.font_size,
            ));
        }
        if self.scales_and_lines.lock_price_to_bar_ratio
            && (!self.scales_and_lines.price_to_bar_ratio.is_finite()
                || self.scales_and_lines.price_to_bar_ratio <= 0.0)
        {
            return Err(ChartSettingsError::PriceToBarRatio(
                self.scales_and_lines.price_to_bar_ratio,
            ));
        }
        let volume = self.trading.execution_sound_volume;
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(ChartSettingsError::ExecutionSoundVolume(volume));
        }
        let volume = self.alerts.sound_volume;
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(ChartSettingsError::AlertSoundVolume(volume));
        }
        if let Some(params) = &self.chart_type_params
            && let Some(error) = params.invalid(self.chart_type)
        {
            return Err(ChartSettingsError::ChartTypeParams {
                chart_type: self.chart_type,
                reason: error,
            });
        }
        Ok(timezone)
    }

    /// Apply the display controls offered by the dialog without rebuilding the chart or feed.
    /// Host-owned trading and alerts are left to their providers. Invalid timezone,
    /// margins or crosshair width returns an error before changing the chart.
    pub fn apply_to_chart(&self, chart: &mut Chart) -> Result<(), ChartSettingsError> {
        let timezone = self.validate()?;
        if let Some(params) = &self.chart_type_params {
            if let Some(error) = params.invalid(chart.chart_type()) {
                return Err(ChartSettingsError::ChartTypeParams {
                    chart_type: chart.chart_type(),
                    reason: error,
                });
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
        let mut axis_layout = self.scales_and_lines.axis_layout;
        if self.scales_and_lines.price_scale_left != chart.main_axis_on_left()
            && axis_layout.side.is_some()
        {
            axis_layout.side = Some(if self.scales_and_lines.price_scale_left {
                crate::model::AxisSide::Left
            } else {
                crate::model::AxisSide::Right
            });
        }
        chart.set_pane_axis_options(chart.main_series_pane(), axis_layout);
        let config = chart.config_mut();
        if !matches!(
            self.chart_type,
            crate::model::ChartType::Heikin
                | crate::model::ChartType::HollowCandles
                | crate::model::ChartType::VolumeCandles
        ) {
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
        if axis_layout.side.is_none() {
            config.show_right_axis = !self.scales_and_lines.price_scale_left;
            config.show_left_axis = self.scales_and_lines.price_scale_left;
        }
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
        if self.chart_type == crate::model::ChartType::Heikin {
            let mut params = chart.chart_type_params().clone();
            let style = &mut params.styles.heikin;
            let colors = &self.candle_colors;
            style.body_visible = colors.show_body;
            style.border_visible = colors.show_border;
            style.wick_visible = colors.show_wick;
            style.color_by_previous_close = colors.color_by_previous_close;
            style.body_up = colors.body_up;
            style.body_down = colors.body_down;
            style.border_up = colors.border_up;
            style.border_down = colors.border_down;
            style.wick_up = colors.wick_up;
            style.wick_down = colors.wick_down;
            if chart.chart_type_params() != &params {
                chart.set_chart_type_params(params);
            }
        } else if self.chart_type == crate::model::ChartType::HollowCandles {
            let mut params = chart.chart_type_params().clone();
            let style = &mut params.styles.hollow;
            let colors = &self.candle_colors;
            style.body_visible = colors.show_body;
            style.border_visible = colors.show_border;
            style.wick_visible = colors.show_wick;
            style.body_up = colors.body_up;
            style.body_down = colors.body_down;
            style.border_up = colors.border_up;
            style.border_down = colors.border_down;
            style.wick_up = colors.wick_up;
            style.wick_down = colors.wick_down;
            if chart.chart_type_params() != &params {
                chart.set_chart_type_params(params);
            }
        } else if self.chart_type == crate::model::ChartType::VolumeCandles {
            let mut params = chart.chart_type_params().clone();
            let style = &mut params.styles.volume_candles;
            let colors = &self.candle_colors;
            style.body_visible = colors.show_body;
            style.border_visible = colors.show_border;
            style.wick_visible = colors.show_wick;
            style.color_by_previous_close = colors.color_by_previous_close;
            style.body_up = colors.body_up;
            style.body_down = colors.body_down;
            style.border_up = colors.border_up;
            style.border_down = colors.border_down;
            style.wick_up = colors.wick_up;
            style.wick_down = colors.wick_down;
            if chart.chart_type_params() != &params {
                chart.set_chart_type_params(params);
            }
        } else {
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

    /// Component switches affect transformed candles, including borders without bodies;
    /// source OHLC direction must not choose their colours. Saved styles and undo stay local.
    #[test]
    #[cfg(feature = "drawings")]
    fn heikin_components_paint_and_keep_independent_saved_values() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let palette = [
            Color32::from_rgb(1, 100, 2),
            Color32::from_rgb(101, 2, 3),
            Color32::from_rgb(3, 4, 102),
            Color32::from_rgb(103, 5, 104),
            Color32::from_rgb(105, 106, 6),
            Color32::from_rgb(7, 107, 108),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for flags in 0..8 {
                for previous in [false, true] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let bars = [
                        (10.0, 18.0, 8.0, 14.0),
                        (13.0, 17.0, 8.5, 11.0),
                        (11.0, 14.0, 6.0, 7.0),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (o, h, l, c))| {
                        Bar::new(
                            chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64),
                            o,
                            h,
                            l,
                            c,
                            0.0,
                        )
                    })
                    .collect();
                    let mut chart = Chart::new(BarData::from_bars(bars));
                    chart.set_chart_type(ChartType::Heikin);
                    chart.config_mut().show_volume = false;
                    let border_width = if previous { 0.5 } else { 1.0 };
                    if previous {
                        chart.config_mut().candle_border_width = border_width;
                    }
                    let before = ChartSettingsState::from_chart(&chart);
                    let mut settings = before.clone();
                    let c = &mut settings.candle_colors;
                    c.show_body = flags & 1 != 0;
                    c.show_border = flags & 2 != 0;
                    c.show_wick = flags & 4 != 0;
                    c.color_by_previous_close = previous;
                    [
                        c.body_up,
                        c.body_down,
                        c.border_up,
                        c.border_down,
                        c.wick_up,
                        c.wick_down,
                    ] = palette;
                    let c = settings.candle_colors.clone();
                    chart.begin_user_action();
                    settings.apply_to_chart(&mut chart).unwrap();
                    chart.end_user_action();
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    let mut bodies = Vec::new();
                    let mut borders = Vec::new();
                    let mut wicks = Vec::new();
                    for shape in painted {
                        match shape {
                            Shape::Rect(r) if plot.contains(r.rect.center()) => {
                                if palette[..2].contains(&r.fill) {
                                    bodies.push((r.rect, r.fill));
                                }
                                if palette[2..4].contains(&r.stroke.color) {
                                    assert_eq!(r.stroke.width, border_width);
                                    borders.push((r.rect, r.stroke.color));
                                }
                            }
                            Shape::LineSegment { points, stroke }
                                if palette[4..].contains(&stroke.color)
                                    && points[0].x == points[1].x =>
                            {
                                wicks.push((points, stroke.color))
                            }
                            _ => {}
                        }
                    }
                    let directions = if previous { [0, 1, 1] } else { [0, 0, 1] };
                    // OHLC averages: 12.5, 12.375, 9.5; recursive opens: 12, 12.25, 12.3125.
                    let values = [
                        (12.0_f64, 12.5_f64, 18.0, 8.0),
                        (12.25, 12.375, 17.0, 8.5),
                        (12.3125, 9.5, 14.0, 6.0),
                    ];
                    for (part, offset, shown) in
                        [(&bodies, 0, c.show_body), (&borders, 2, c.show_border)]
                    {
                        assert_eq!(part.len(), if shown { 3 } else { 0 }, "flags={flags}");
                        for (i, (rect, color)) in part.iter().enumerate() {
                            assert_eq!(*color, palette[offset + directions[i]]);
                            let (open, close, _, _) = values[i];
                            let top = chart.last_price_map.price_to_y(open.max(close));
                            let bottom = chart.last_price_map.price_to_y(open.min(close));
                            assert!((rect.top() - top).abs() < 0.001);
                            assert!(
                                (rect.bottom() - (top + (bottom - top).max(1.0))).abs() < 0.001
                            );
                        }
                    }
                    assert_eq!(wicks.len(), if c.show_wick { 6 } else { 0 });
                    for (i, (points, color)) in wicks.iter().enumerate() {
                        let bar = i / 2;
                        assert_eq!(*color, palette[4 + directions[bar]]);
                        let (open, close, high, low) = values[bar];
                        let (from, to) = if i % 2 == 0 {
                            (high, open.max(close))
                        } else {
                            (open.min(close), low)
                        };
                        assert!(
                            (points[0].y - chart.last_price_map.price_to_y(from)).abs() < 0.001
                        );
                        assert!((points[1].y - chart.last_price_map.price_to_y(to)).abs() < 0.001);
                    }
                    assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
                    assert_eq!(
                        chart
                            .series_last_value(
                                crate::widget::SeriesId::MAIN,
                                crate::model::PriceLineSource::LastBar
                            )
                            .unwrap()
                            .color,
                        c.body_down
                    );
                    let saved: ChartSnapshot = serde_json::from_str(
                        &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                    )
                    .unwrap();
                    assert!(chart.undo());
                    assert_eq!(
                        ChartSettingsState::from_chart(&chart).candle_colors,
                        before.candle_colors
                    );
                    assert!(chart.redo());
                    chart.restore(&saved).unwrap();
                    assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
                    chart.set_chart_type(ChartType::Candles);
                    assert_eq!(chart.config().bullish_color, before.candle_colors.body_up);
                    assert_eq!(
                        chart.chart_type_params().styles.candles,
                        before.chart_type_params.as_ref().unwrap().styles.candles
                    );
                    chart.set_chart_type(ChartType::Heikin);
                    assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
                }
            }
        }
    }

    /// Every high-low bar takes one colour whatever its direction, and each end prints its own
    /// price outside the bar in the label colour and size, also on an inverted scale and on a
    /// flat bar. Saved styles and undo keep these apart from the candle settings.
    #[test]
    #[cfg(feature = "drawings")]
    fn high_low_style_paints_bars_and_labels_and_keeps_saved_values() {
        use crate::model::{ChartType, HighLowStyle};
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        use egui::Color32;

        let style = HighLowStyle {
            bar_color: Color32::from_rgb(1, 2, 101),
            labels_visible: true,
            label_color: Color32::from_rgb(3, 4, 103),
            label_font_size: 12.0,
        };
        // Rising, falling and flat bars, with the prices the labels print.
        let bars = [
            ((10.0, 18.0, 8.0, 14.0), "18.00", "8.00"),
            ((13.0, 17.0, 8.5, 11.0), "17.00", "8.50"),
            ((12.0, 12.0, 12.0, 12.0), "12.00", "12.00"),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for inverted in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = Chart::new(BarData::from_bars(
                    bars.iter()
                        .enumerate()
                        .map(|(i, ((o, h, l, c), ..))| {
                            Bar::new(
                                chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64),
                                *o,
                                *h,
                                *l,
                                *c,
                                0.0,
                            )
                        })
                        .collect(),
                ));
                chart.set_chart_type(ChartType::HighLow);
                chart.config_mut().show_volume = false;
                let before = ChartSettingsState::from_chart(&chart);
                let mut settings = before.clone();
                settings.scales_and_lines.invert_scale = inverted;
                settings.chart_type_params.as_mut().unwrap().styles.high_low = style;
                chart.begin_user_action();
                settings.apply_to_chart(&mut chart).unwrap();
                chart.end_user_action();
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.state.time_scale_mut().set_bar_spacing(80.0);

                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let plot = chart.get_rendered_price_rect();
                let map = chart.get_chart_mapping().price;
                let rects = |painted: &[Shape]| {
                    let mut rects: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Rect(r)
                                if r.fill == style.bar_color && plot.contains(r.rect.center()) =>
                            {
                                Some(r.rect)
                            }
                            _ => None,
                        })
                        .collect();
                    rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
                    rects
                };
                let labels = |painted: &[Shape]| -> Vec<_> {
                    painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(t)
                                if t.galley.job.sections[0].format.color == style.label_color =>
                            {
                                Some((t.galley.text().to_owned(), t.visual_bounding_rect()))
                            }
                            _ => None,
                        })
                        .collect()
                };
                let shown = rects(&painted);
                assert_eq!(shown.len(), bars.len());
                let printed = labels(&painted);
                assert_eq!(printed.len(), 2 * bars.len());
                for (rect, ((_, high, low, _), high_text, low_text)) in shown.iter().zip(bars) {
                    let (y_high, y_low) = (map.price_to_y(high), map.price_to_y(low));
                    assert_eq!(rect.top(), y_high.min(y_low));
                    assert_eq!(rect.bottom(), y_high.max(y_low).max(rect.top() + 1.0));
                    let at_end = |above: bool, text: &str| {
                        printed.iter().any(|(label, bounds)| {
                            label == text
                                && (bounds.center().x - rect.center().x).abs() < 1.0
                                && if above {
                                    bounds.bottom() <= rect.top()
                                } else {
                                    bounds.top() >= rect.bottom()
                                }
                        })
                    };
                    assert!(at_end(!inverted, high_text), "{high_text} {inverted}");
                    assert!(at_end(inverted, low_text), "{low_text} {inverted}");
                }
                assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(t)
                    if t.galley.job.sections[0].format.font_id.size == 12.0
                        && t.galley.text() == "18.00")));
                // The falling and flat bars share the bar colour on the last-price label too.
                let last = chart.state.data().bars.last().unwrap().clone();
                assert_eq!(chart.main_value_color(&last), style.bar_color);

                // Labels that do not fit the bar spacing go together; the bars stay visible.
                chart.state.time_scale_mut().set_bar_spacing(2.0);
                let dense = shapes(&ctx, &mut chart, 0.2, vec![]);
                assert!(labels(&dense).is_empty());
                let dense_rects = rects(&dense);
                assert_eq!(dense_rects.len(), bars.len());
                assert!(
                    dense_rects
                        .iter()
                        .all(|r| r.width() >= 1.0 && r.height() >= 1.0)
                );
                chart.state.time_scale_mut().set_bar_spacing(80.0);

                for hidden in [
                    HighLowStyle {
                        labels_visible: false,
                        ..style
                    },
                    HighLowStyle {
                        label_font_size: 0.0,
                        ..style
                    },
                ] {
                    let mut params = chart.chart_type_params().clone();
                    params.styles.high_low = hidden;
                    chart.set_chart_type_params(params);
                    let painted = shapes(&ctx, &mut chart, 0.3, vec![]);
                    assert!(labels(&painted).is_empty());
                    assert_eq!(rects(&painted).len(), bars.len());
                }
                let mut params = chart.chart_type_params().clone();
                params.styles.high_low = style;
                chart.set_chart_type_params(params);

                assert_eq!(chart.config().bullish_color, before.candle_colors.body_up);
                let saved: ChartSnapshot = serde_json::from_str(
                    &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                )
                .unwrap();
                assert!(chart.undo());
                assert_eq!(
                    chart.chart_type_params().styles.high_low,
                    HighLowStyle::default()
                );
                assert!(chart.redo());
                chart.restore(&saved).unwrap();
                assert_eq!(chart.chart_type_params().styles.high_low, style);
                chart.set_chart_type(ChartType::Candles);
                chart.set_chart_type(ChartType::HighLow);
                assert_eq!(chart.chart_type_params().styles.high_low, style);
            }
        }
    }

    /// Hollow candles colour by the close against the previous close, and are hollow when the
    /// close is above the open; volume candles colour by their open or the previous close and are
    /// always filled. Each component has its own switch and colours, and saved styles and undo
    /// keep them apart from the plain candle settings.
    #[test]
    #[cfg(feature = "drawings")]
    fn hollow_and_volume_components_paint_and_keep_independent_saved_values() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let palette = [
            Color32::from_rgb(1, 100, 2),
            Color32::from_rgb(101, 2, 3),
            Color32::from_rgb(3, 4, 102),
            Color32::from_rgb(103, 5, 104),
            Color32::from_rgb(105, 106, 6),
            Color32::from_rgb(7, 107, 108),
        ];
        // Open, high, low, close; the colour index (rising 0, falling 1) against the previous close
        // and against the open, and whether a hollow candle's body is filled. The last two bars
        // differ between the two comparisons.
        let bars = [
            ((10.0, 18.0, 8.0, 14.0), 0, 0, false),
            ((13.0, 17.0, 8.5, 11.0), 1, 1, true),
            ((11.0, 14.0, 6.0, 12.0), 0, 0, false),
            ((13.0, 15.0, 10.0, 12.5), 0, 1, true),
            ((9.0, 13.0, 8.0, 10.0), 1, 0, false),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for (kind, by_previous, flags) in [
                (ChartType::HollowCandles, true),
                (ChartType::VolumeCandles, false),
                (ChartType::VolumeCandles, true),
            ]
            .into_iter()
            .flat_map(|(kind, by_previous)| (0..8).map(move |flags| (kind, by_previous, flags)))
            {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = Chart::new(BarData::from_bars(
                    bars.iter()
                        .enumerate()
                        .map(|(i, ((o, h, l, c), _, _, _))| {
                            Bar::new(
                                chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64),
                                *o,
                                *h,
                                *l,
                                *c,
                                0.0,
                            )
                        })
                        .collect(),
                ));
                chart.set_chart_type(kind);
                chart.config_mut().show_volume = false;
                let before = ChartSettingsState::from_chart(&chart);
                let mut settings = before.clone();
                let c = &mut settings.candle_colors;
                c.show_body = flags & 1 != 0;
                c.show_border = flags & 2 != 0;
                c.show_wick = flags & 4 != 0;
                c.color_by_previous_close = by_previous;
                [
                    c.body_up,
                    c.body_down,
                    c.border_up,
                    c.border_down,
                    c.wick_up,
                    c.wick_down,
                ] = palette;
                let c = settings.candle_colors.clone();
                chart.begin_user_action();
                settings.apply_to_chart(&mut chart).unwrap();
                chart.end_user_action();
                frame(&ctx, &mut chart, 0.0, vec![]);
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let plot = chart.get_rendered_price_rect();
                let mut bodies = Vec::new();
                let mut wicks = Vec::new();
                for shape in painted {
                    match shape {
                        Shape::Rect(r)
                            if plot.contains(r.rect.center())
                                && (palette[..2].contains(&r.fill)
                                    || palette[2..4].contains(&r.stroke.color)) =>
                        {
                            bodies.push((r.fill, (r.stroke.width > 0.0).then_some(r.stroke.color)));
                        }
                        Shape::LineSegment { points, stroke }
                            if palette[4..].contains(&stroke.color)
                                && points[0].x == points[1].x =>
                        {
                            wicks.push(stroke.color)
                        }
                        _ => {}
                    }
                }
                let mut expected_bodies = Vec::new();
                let mut expected_wicks = Vec::new();
                for (_, previous, own, filled) in bars {
                    let dir = if by_previous { previous } else { own };
                    let filled = filled || kind == ChartType::VolumeCandles;
                    let fill = (c.show_body && filled).then_some(palette[dir]);
                    let stroke = c.show_border.then_some(palette[2 + dir]);
                    if fill.is_some() || stroke.is_some() {
                        expected_bodies.push((fill.unwrap_or(Color32::TRANSPARENT), stroke));
                    }
                    if c.show_wick {
                        expected_wicks.extend([palette[4 + dir]; 2]);
                    }
                }
                assert_eq!(bodies, expected_bodies, "flags={flags}");
                // The last-price label and the volume bars follow the body colours.
                assert_eq!(chart.main_body_colors(), (palette[0], palette[1]));
                assert_eq!(wicks, expected_wicks, "flags={flags}");
                assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
                let saved: ChartSnapshot = serde_json::from_str(
                    &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                )
                .unwrap();
                assert!(chart.undo());
                assert_eq!(
                    ChartSettingsState::from_chart(&chart).candle_colors,
                    before.candle_colors
                );
                assert!(chart.redo());
                chart.restore(&saved).unwrap();
                assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
                chart.set_chart_type(ChartType::Candles);
                assert_eq!(chart.config().bullish_color, before.candle_colors.body_up);
                assert_eq!(
                    chart.chart_type_params().styles.candles,
                    before.chart_type_params.as_ref().unwrap().styles.candles
                );
                chart.set_chart_type(kind);
                assert_eq!(ChartSettingsState::from_chart(&chart).candle_colors, c);
            }
        }
    }

    /// A Heikin-Ashi layout without the chart type's style tagged only the source close on its
    /// scale and restores that way; one saved with the style keeps the pair. The source (10, 18,
    /// 8, 14) closes at 14 and its transformed close is (10 + 18 + 8 + 14) / 4 = 12.5.
    #[test]
    #[cfg(feature = "drawings")]
    fn heikin_layout_saved_before_the_style_restores_its_scale() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let bars = || {
            BarData::from_bars(vec![Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                10.0,
                18.0,
                8.0,
                14.0,
                0.0,
            )])
        };
        for (saved_with_style, expected) in [(false, &[14.0][..]), (true, &[14.0, 12.5][..])] {
            let mut chart = Chart::new(bars());
            chart.set_chart_type(ChartType::Heikin);
            chart.config_mut().show_volume = false;
            let mut saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            if !saved_with_style {
                for path in [
                    "/chart_type_params/styles",
                    "/settings/chart_type_params/styles",
                ] {
                    let styles = saved.pointer_mut(path).unwrap().as_object_mut().unwrap();
                    assert!(styles.remove("heikin").is_some());
                }
            }
            let saved: ChartSnapshot = serde_json::from_value(saved).unwrap();
            let ctx = Context::default();
            let mut restored = Chart::new(bars());
            restored.restore(&saved).unwrap();
            frame(&ctx, &mut restored, 0.0, vec![]);
            let painted = shapes(&ctx, &mut restored, 0.1, vec![]);
            let plot = restored.get_rendered_price_rect();
            let up = restored.chart_type_params().styles.heikin.body_up;
            let tagged: Vec<f64> = [14.0, 12.5]
                .into_iter()
                .filter(|&price| {
                    let y = restored.last_price_map.price_to_y(price);
                    painted.iter().any(|shape| {
                        matches!(shape, Shape::Rect(r)
                        if r.fill == up
                        && (r.rect.right() <= plot.left() || r.rect.left() >= plot.right())
                        && r.rect.y_range().contains(y))
                    })
                })
                .collect();
            assert_eq!(tagged, expected);
            let settings = ChartSettingsState::from_chart(&restored);
            assert_eq!(
                settings
                    .chart_type_params
                    .unwrap()
                    .styles
                    .heikin
                    .real_price_only,
                !saved_with_style
            );
        }
    }

    /// A shape with both stops its fill at the middle of the stroke and shrinks it for a
    /// transparent stroke, so a border that is not opaque must leave the body a shape of its own.
    /// A distinct opaque border is painted; one in the colour of an opaque body is not.
    #[test]
    #[cfg(feature = "drawings")]
    fn heikin_border_is_shared_separate_or_skipped_by_its_colour() {
        use crate::model::ChartType;
        use crate::widget::test_frames::{frame, shapes};
        let opaque = Color32::from_rgb(1, 100, 2);
        let translucent = Color32::from_rgba_unmultiplied(3, 4, 102, 128);
        let distinct = Color32::from_rgb(3, 4, 102);
        // The stroke on the body's shape, and how many shapes of its own the border gets.
        let shared = egui::Stroke::new(1.0, distinct);
        for (body, border, body_stroke, border_shapes) in [
            (opaque, translucent, egui::Stroke::NONE, 1),
            (opaque, Color32::TRANSPARENT, egui::Stroke::NONE, 1),
            (opaque, distinct, shared, 0),
            (opaque, opaque, egui::Stroke::NONE, 0),
            (translucent, translucent, egui::Stroke::NONE, 1),
        ] {
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                10.0,
                18.0,
                8.0,
                14.0,
                0.0,
            )]));
            chart.set_chart_type(ChartType::Heikin);
            chart.config_mut().show_volume = false;
            let mut settings = ChartSettingsState::from_chart(&chart);
            let c = &mut settings.candle_colors;
            c.show_border = true;
            c.body_up = body;
            c.border_up = border;
            settings.apply_to_chart(&mut chart).unwrap();
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let rects: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(r) if plot.contains(r.rect.center()) => Some(r),
                    _ => None,
                })
                .collect();
            let bodies: Vec<_> = rects.iter().filter(|r| r.fill == body).collect();
            assert_eq!(bodies.len(), 1, "{body:?} {border:?}");
            assert_eq!(bodies[0].stroke, body_stroke, "{body:?} {border:?}");
            let own = rects
                .iter()
                .filter(|r| {
                    r.fill == Color32::TRANSPARENT
                        && r.stroke.width > 0.0
                        && r.stroke.color == border
                })
                .count();
            assert_eq!(own, border_shapes, "{body:?} {border:?}");
        }
    }

    /// The default scale tags both closes; selecting original prices suppresses only the
    /// synthetic tag, and the choice survives layouts and undo.
    #[test]
    #[cfg(feature = "drawings")]
    fn heikin_scale_prices_follow_the_saved_switch() {
        use crate::model::ChartType;
        use crate::widget::test_frames::{frame, shapes};
        for theme in [Theme::light(), Theme::dark()] {
            for left in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                    chrono::DateTime::UNIX_EPOCH,
                    10.0,
                    18.0,
                    8.0,
                    14.0,
                    0.0,
                )]));
                chart.set_chart_type(ChartType::Heikin);
                chart.config_mut().show_volume = false;
                let mut settings = ChartSettingsState::from_chart(&chart);
                settings.scales_and_lines.price_scale_left = left;
                settings.precision = super::super::PrecisionMode::Decimals3;
                settings.apply_to_chart(&mut chart).unwrap();
                frame(&ctx, &mut chart, 0.0, vec![]);
                for (original_only, show_labels, show_lines) in [
                    (false, true, true),
                    (true, true, true),
                    (false, false, true),
                    (false, true, false),
                ] {
                    let mut settings = ChartSettingsState::from_chart(&chart);
                    settings.scales_and_lines.show_last_price_label = show_labels;
                    settings.scales_and_lines.show_price_line = show_lines;
                    settings
                        .chart_type_params
                        .as_mut()
                        .unwrap()
                        .styles
                        .heikin
                        .real_price_only = original_only;
                    chart.begin_user_action();
                    settings.apply_to_chart(&mut chart).unwrap();
                    chart.end_user_action();
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                    let plot = chart.get_rendered_price_rect();
                    for (price, shown) in [(14.0, true), (12.5, !original_only)] {
                        let y = chart.last_price_map.price_to_y(price);
                        let tag = painted.iter().any(|shape| {
                            matches!(shape, Shape::Rect(r)
                            if (r.fill == chart.chart_type_params().styles.heikin.body_up)
                            && (r.rect.right() <= plot.left() || r.rect.left() >= plot.right())
                            && r.rect.y_range().contains(y))
                        });
                        assert_eq!(
                            tag,
                            shown && show_labels,
                            "price={price}, original_only={original_only}, left={left}"
                        );
                        let line = painted.iter().any(|shape| {
                            matches!(shape, Shape::LineSegment {points, stroke}
                            if stroke.color == chart.chart_type_params().styles.heikin.body_up
                            && points[0].y == points[1].y && (points[0].y - y).abs() < 0.001)
                        });
                        assert_eq!(
                            line,
                            shown && show_lines,
                            "price line {price}, original_only={original_only}"
                        );
                    }
                    let saved = chart.snapshot().unwrap();
                    if original_only {
                        assert!(chart.undo());
                        assert!(!chart.chart_type_params().styles.heikin.real_price_only);
                    }
                    chart.restore(&saved).unwrap();
                    assert_eq!(
                        chart.chart_type_params().styles.heikin.real_price_only,
                        original_only
                    );
                }
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
