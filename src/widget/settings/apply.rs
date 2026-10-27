//! Apply and read the chart settings exposed by the shared dialog.

use super::{
    BackgroundType, ButtonVisibility, ChartSettingsState, GridLinesMode, PrecisionMode,
    WatermarkMode,
};
use crate::{
    Chart,
    config::{BackgroundStyle, NavigationButtons},
    scales::{PriceScaleMode, TimezoneMode},
};
use egui::Color32;

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
        settings.chart_basic_styles.background_color = config.background_color;
        settings.grid_lines.horizontal_color = config.grid_color;
        settings.grid_lines.vertical_color =
            config.vertical_grid_color.unwrap_or(config.grid_color);
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
        settings.scales_and_lines.price_scale_left =
            !config.show_right_axis && config.show_left_axis;
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
        settings.btns.navigation = match config.navigation_buttons {
            NavigationButtons::Always => ButtonVisibility::AlwaysVisible,
            NavigationButtons::OnHover => ButtonVisibility::VisibleOnMouseOver,
            NavigationButtons::Never => ButtonVisibility::Hidden,
        };
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
        let symbol_description = chart
            .instrument()
            .and_then(|instrument| instrument.description.as_deref())
            .map_or_else(
                || chart.symbol.clone(),
                |description| format!("{} · {description}", chart.symbol),
            );
        settings.watermark.mode = if !config.show_watermark {
            WatermarkMode::ReplayMode
        } else if config.watermark_text.is_none() {
            WatermarkMode::Symbol
        } else if config.watermark_text.as_deref() == Some(symbol_description.as_str()) {
            WatermarkMode::SymbolAndDescription
        } else {
            WatermarkMode::Custom
        };
        settings.watermark.color = config.watermark_color;
        settings.scales_appearance.text_color = config.text_color;

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
        chart.set_event_visibility(self.events.clone());
        chart.set_price_targets_visible(self.show_price_targets);
        let watermark_text = if self.watermark.mode == WatermarkMode::SymbolAndDescription {
            Some(
                chart
                    .instrument()
                    .and_then(|instrument| instrument.description.as_deref())
                    .map_or_else(
                        || chart.symbol.clone(),
                        |description| format!("{} · {description}", chart.symbol),
                    ),
            )
        } else {
            None
        };
        chart.chart_options.time_scale.timezone = timezone;
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

        config.navigation_buttons = match self.btns.navigation {
            ButtonVisibility::AlwaysVisible => NavigationButtons::Always,
            ButtonVisibility::VisibleOnMouseOver => NavigationButtons::OnHover,
            ButtonVisibility::Hidden => NavigationButtons::Never,
        };
        config.show_ohlc_info = self.status_line.show_ohlc;
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
        config.show_watermark = self.watermark.mode != WatermarkMode::ReplayMode;
        config.watermark_color = self.watermark.color;
        if self.watermark.mode != WatermarkMode::Custom {
            config.watermark_text = watermark_text;
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
        Ok(())
    }
}
