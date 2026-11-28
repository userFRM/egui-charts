//! Apply and read the chart settings exposed by the shared dialog.

use super::{
    BackgroundType, ButtonVisibility, ChartSettingsState, DateFormat, GridLinesMode, PrecisionMode,
    ThousandsSeparator, WatermarkMode,
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

/// The right margin the dialog shows, in bars: the margin the view keeps, which for a margin set
/// in pixels is its width at the current bar spacing.
fn right_margin_bars(chart: &Chart) -> f32 {
    let options = &chart.chart_options().time_scale;
    options
        .right_offset_pixels
        .map_or(options.right_offset, |pixels| {
            pixels / chart.state().time_scale().bar_spacing()
        })
        .max(0.0)
}

/// The right margin in pixels of a chart that keeps it in pixels; a negative one reads as zero,
/// as its width in bars does.
fn right_margin_pixels(chart: &Chart) -> Option<f32> {
    chart
        .chart_options()
        .time_scale
        .right_offset_pixels
        .map(|pixels| pixels.max(0.0))
}

fn thousands_separator(separator: Option<char>) -> ThousandsSeparator {
    match separator {
        None => ThousandsSeparator::Off,
        Some(',') => ThousandsSeparator::Comma,
        Some(' ') => ThousandsSeparator::Space,
        Some('\'') => ThousandsSeparator::Apostrophe,
        Some(other) => ThousandsSeparator::Other(other),
    }
}

fn thousands_sep(separator: ThousandsSeparator) -> Option<char> {
    match separator {
        ThousandsSeparator::Off => None,
        ThousandsSeparator::Comma => Some(','),
        ThousandsSeparator::Space => Some(' '),
        ThousandsSeparator::Apostrophe => Some('\''),
        ThousandsSeparator::Other(other) => Some(other),
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
    /// A margin is not finite, is negative or is above 100 percent. Margins that together leave
    /// no room for prices are accepted, and ignored by the fit.
    Margins {
        /// The top margin in percent.
        top: f32,
        /// The bottom margin in percent.
        bottom: f32,
    },
    /// The right margin, in bars or in pixels, is not finite or is negative.
    RightMargin(f32),
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
            Self::RightMargin(_) => "Right margin must be zero or more bars",
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

/// Stores `color` in an optional colour unless the colour, with `fallback` for `None`, already
/// resolves to it, so that the state read from a chart applies back unchanged.
fn set_unless_resolved(current: &mut Option<Color32>, fallback: Color32, color: Color32) {
    if current.unwrap_or(fallback) != color {
        *current = Some(color);
    }
}

impl ChartSettingsState {
    /// Read the chart's current display settings and the futures data choices it recorded.
    /// Host-managed trading and alert groups retain defaults; hosts preserve them when refreshing
    /// settings.
    pub fn from_chart(chart: &Chart) -> Self {
        let config = chart.config();
        let mut settings = Self {
            chart_type: chart.chart_type(),
            chart_type_params: Some(chart.chart_type_params().clone()),
            price_source: Some(chart.config().price_source),
            events: chart.event_visibility().clone(),
            show_session_breaks: Some(config.show_session_breaks),
            extended_hours: Some(chart.extended_hours()),
            overnight_hours: Some(chart.overnight_hours()),
            back_adjust_futures: chart.back_adjust_futures,
            settlement_as_close: chart.settlement_as_close,
            session_background: Some(super::SessionBackgroundSettings {
                show_extended_hours: config.show_extended_hours,
                extended_hours_color: config.extended_hours_color,
                premarket_color: config.premarket_color,
                postmarket_color: config.postmarket_color,
                shade_premarket: config.shade_premarket,
                shade_postmarket: config.shade_postmarket,
                shade_overnight: config.shade_overnight,
                overnight_hours_color: config.overnight_hours_color,
            }),
            order_flow: super::OrderFlowSettings {
                show_heatmap: chart
                    .feed_link
                    .as_ref()
                    .is_some_and(|link| link.heatmap.is_some()),
                heatmap: chart.liquidity_heatmap_config(),
                heatmap_retention: chart
                    .feed_link
                    .as_ref()
                    .and_then(|link| link.depth_retention),
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
            price_target_positive_color: Some(config.price_target_positive_color),
            price_target_negative_color: Some(config.price_target_negative_color),
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
                color_by_previous_close: candles.color_by_previous_close,
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
                color_by_previous_close: candles.color_by_previous_close,
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
                color_by_previous_close: candles.color_by_previous_close,
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
        settings.pane_separator_color = chart.pane_options().separator_color;
        settings.status_line.show_logo = config.show_legend_badge;
        settings.status_line.show_market_status = config.show_market_status;
        settings.status_line.show_symbol = config.show_symbol_labels;
        settings.status_line.title_as_description = config.title_as_description;
        settings.status_line.show_quote_boxes = config.show_quote_boxes;
        settings.status_line.quote_panel = config.quote_panel;
        settings.status_line.show_ohlc = config.show_ohlc_info;
        settings.status_line.show_change = config.show_ohlc_change;
        settings.status_line.show_change_percent = config.show_ohlc_change_percent;
        settings.status_line.show_volume = config.show_legend_volume;
        settings.status_line.show_indicator_titles = config.show_indicator_labels;
        settings.status_line.show_indicator_inputs = config.show_indicator_inputs;
        settings.status_line.show_indicator_values = config.show_indicator_last_val;
        settings.status_line.show_last_day_change = config.show_last_day_change;
        settings.status_line.show_indicator_background = config.show_indicator_backdrop;
        settings.status_line.indicator_background_opacity =
            (config.indicator_backdrop_opacity.clamp(0.0, 1.0) * 100.0).round() as u8;
        settings.scales_and_lines.axis_layout = chart
            .pane_axis_options(chart.main_series_pane())
            .unwrap_or_default();
        settings.scales_and_lines.price_scale_left = chart.main_axis_on_left();
        settings.scales_and_lines.show_currency = config.show_currency;
        settings.scales_and_lines.show_unit = config.show_unit;
        settings.scales_and_lines.currency_unit_on_hover = config.currency_unit_on_hover;
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
        let main_style = chart.series_style(crate::widget::SeriesId::MAIN);
        settings.scales_and_lines.symbol_price_color =
            main_style.and_then(|style| style.price_line_color);
        settings.scales_and_lines.price_line_source =
            main_style.map(|style| style.price_line_source);
        settings.scales_and_lines.price_line_width = main_style.map(|style| style.price_line_width);
        settings.scales_and_lines.price_line_style = main_style.map(|style| style.price_line_style);
        settings.scales_and_lines.thousands_sep = Some(thousands_separator(config.thousands_sep));
        settings.scales_and_lines.lock_price_to_bar_ratio =
            chart.state().price_scale().bar_ratio_locked();
        // An unlocked chart offers the ratio of the view it shows, which ticking the lock keeps; one
        // not drawn yet has no view and offers 1.
        let mut view = chart.state().price_scale().clone();
        view.set_bar_ratio_locked(
            true,
            chart.state().time_scale().bar_spacing(),
            chart.get_rendered_price_rect().height(),
        );
        settings.scales_and_lines.price_to_bar_ratio = chart
            .state()
            .price_scale()
            .bar_ratio()
            .or(view.bar_ratio())
            .unwrap_or(1.0);
        settings.scales_and_lines.show_last_price_label = config.show_symbol_last_val;
        settings.scales_and_lines.show_last_price_change = config.show_symbol_last_change;
        settings.scales_and_lines.show_price_line = config.show_price_line;
        settings.scales_and_lines.show_prev_close_label = config.show_symbol_prev_close;
        settings.scales_and_lines.show_prev_close = config.show_prev_close_line;
        settings.scales_and_lines.show_countdown = config.show_countdown;
        settings.scales_and_lines.show_bid_ask = config.show_bid_ask;
        settings.scales_and_lines.show_bid_ask_lines = config.show_bid_ask_lines;
        settings.scales_and_lines.prev_close_line = Some(config.prev_close_line);
        settings.scales_and_lines.show_high_low = config.show_high_low;
        settings.scales_and_lines.show_high_low_lines = config.show_high_low_lines;
        settings.margins.top_percent =
            (chart.state().price_scale().options().scale_margins.top * 100.0) as f32;
        settings.margins.bottom_percent =
            (chart.state().price_scale().options().scale_margins.bottom * 100.0) as f32;
        settings.margins.right_bars = right_margin_bars(chart);
        settings.margins.right_pixels = right_margin_pixels(chart);
        let scale = chart.state().price_scale();
        settings.scales_and_lines.auto_scale = !scale.is_manual();
        settings.scales_and_lines.log_scale = scale.options().mode == PriceScaleMode::Logarithmic;
        settings.scales_and_lines.percentage_scale =
            scale.options().mode == PriceScaleMode::Percentage;
        settings.scales_and_lines.indexed_to_100 =
            Some(scale.options().mode == PriceScaleMode::IndexedTo100);
        settings.scales_and_lines.invert_scale = scale.options().invert_scale;

        settings.set_timezone_mode(&chart.chart_options().time_scale.timezone);
        let time_scale = &chart.chart_options().time_scale;
        settings.scales_and_lines.use_24_hour = Some(time_scale.use_24_hour);
        settings.scales_and_lines.weekday_names = Some(time_scale.weekday_names);
        settings.scales_and_lines.keep_left_edge_on_interval_change =
            Some(time_scale.keep_left_edge_on_interval_change);
        settings.scales_and_lines.date_format = Some(match &chart.date_format {
            Some(pattern) => DateFormat::Pattern(pattern.clone()),
            None => DateFormat::Default,
        });
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
        settings.crosshair_lines = Some(super::CrosshairVisibility {
            vertical_line: crosshair.vert_line_visible,
            horizontal_line: crosshair.horz_line_visible,
            time_label: crosshair.label_visible && crosshair.vert_label_visible,
            price_label: crosshair.label_visible && crosshair.horz_label_visible,
        });
        settings.precision = PrecisionMode::from_scale(chart.state().price_scale());
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
        settings.scales_appearance.font_family = Some(config.axis_font_family.clone());
        settings.scales_appearance.lines_color = axis_line_color(config);
        settings.btns.pane = button_visibility(config.pane_buttons);

        settings
    }

    /// Writes `mode` to [`timezone`](Self::timezone) as saved settings hold it: `UTC`, `Local`,
    /// a zone name, or `Exchange:` and the fallback zone.
    pub(crate) fn set_timezone_mode(&mut self, mode: &TimezoneMode) {
        self.timezone = match mode {
            TimezoneMode::Utc => "UTC".into(),
            TimezoneMode::Local => "Local".into(),
            TimezoneMode::Timezone(zone) => zone.to_string(),
            TimezoneMode::Exchange(zone) => format!("Exchange:{zone}"),
        };
    }

    /// The mode [`timezone`](Self::timezone) names.
    pub(crate) fn timezone_mode(&self) -> Result<TimezoneMode, ChartSettingsError> {
        Ok(match self.timezone.as_str() {
            "UTC" => TimezoneMode::Utc,
            "Local" => TimezoneMode::Local,
            name => {
                if let Some(name) = name.strip_prefix("Exchange:") {
                    TimezoneMode::Exchange(name.parse().map_err(ChartSettingsError::Timezone)?)
                } else {
                    TimezoneMode::Timezone(name.parse().map_err(ChartSettingsError::Timezone)?)
                }
            }
        })
    }

    pub(crate) fn validate(&self) -> Result<TimezoneMode, ChartSettingsError> {
        let timezone = self.timezone_mode()?;
        let top = self.margins.top_percent;
        let bottom = self.margins.bottom_percent;
        if !(0.0..=100.0).contains(&top) || !(0.0..=100.0).contains(&bottom) {
            return Err(ChartSettingsError::Margins { top, bottom });
        }
        let right = self.margins.right_bars;
        if !right.is_finite() || right < 0.0 {
            return Err(ChartSettingsError::RightMargin(right));
        }
        if let Some(pixels) = self.margins.right_pixels
            && (!pixels.is_finite() || pixels < 0.0)
        {
            return Err(ChartSettingsError::RightMargin(pixels));
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
    /// A changed hours choice reloads history on the next frame.
    /// Host-owned trading and alerts are left to their providers. The futures data choices are
    /// recorded on the chart, which leaves reloading history for them to the host. Invalid timezone,
    /// margins or crosshair width returns an error before changing the chart. The optional
    /// choices that hold `None`, as in settings saved before they existed, keep the chart's,
    /// while the optional colours apply as they are: `None` gives the last price its candle
    /// direction colour, study values their plot colours and pane separators the theme's grid
    /// line colour. A changed TPO period length, profile period or its count drops the TPO
    /// merges and splits carried over from the chart, which were made for the old values;
    /// [`Chart::set_chart_type_params`] sets them as given.
    pub fn apply_to_chart(&self, chart: &mut Chart) -> Result<(), ChartSettingsError> {
        let timezone = self.validate()?;
        if let Some(params) = &self.chart_type_params {
            if let Some(error) = params.invalid(chart.chart_type()) {
                return Err(ChartSettingsError::ChartTypeParams {
                    chart_type: chart.chart_type(),
                    reason: error,
                });
            }
            let mut params = params.clone();
            params.drop_stale_tpo_edits(chart.chart_type_params());
            chart.apply_chart_type_params(params);
        }
        if let Some(source) = self.price_source {
            chart.config_mut().price_source = source;
        }
        chart.set_liquidity_heatmap_config(self.order_flow.heatmap);
        chart.set_depth_retention(self.order_flow.heatmap_retention);
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
        if let Some(overnight) = self.overnight_hours {
            chart.set_overnight_hours(overnight);
        }
        if let Some(extended) = self.extended_hours {
            chart.set_extended_hours(extended);
        }
        chart.back_adjust_futures = self.back_adjust_futures;
        chart.settlement_as_close = self.settlement_as_close;
        if let Some(background) = &self.session_background {
            let config = chart.config_mut();
            config.show_extended_hours = background.show_extended_hours;
            config.extended_hours_color = background.extended_hours_color;
            config.premarket_color = background.premarket_color;
            config.postmarket_color = background.postmarket_color;
            config.shade_premarket = background.shade_premarket;
            config.shade_postmarket = background.shade_postmarket;
            config.shade_overnight = background.shade_overnight;
            config.overnight_hours_color = background.overnight_hours_color;
        }
        chart.chart_options.time_scale.timezone = timezone;
        let scales = &self.scales_and_lines;
        if let Some(use_24_hour) = scales.use_24_hour {
            chart.chart_options.time_scale.use_24_hour = use_24_hour;
        }
        if let Some(weekday_names) = scales.weekday_names {
            chart.chart_options.time_scale.weekday_names = weekday_names;
        }
        if let Some(keep) = scales.keep_left_edge_on_interval_change {
            chart
                .chart_options
                .time_scale
                .keep_left_edge_on_interval_change = keep;
        }
        if let Some(date_format) = &scales.date_format {
            chart.set_date_format(match date_format {
                DateFormat::Default => None,
                DateFormat::Pattern(pattern) => Some(pattern.clone()),
            });
        }
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
        if let Some(shown) = self.crosshair_lines {
            crosshair.vert_line_visible = shown.vertical_line;
            crosshair.horz_line_visible = shown.horizontal_line;
            // Unedited painted labels keep the master switch and record no undo step.
            let labels = (
                crosshair.label_visible && crosshair.vert_label_visible,
                crosshair.label_visible && crosshair.horz_label_visible,
            );
            if labels != (shown.time_label, shown.price_label) {
                crosshair.label_visible = true;
                crosshair.vert_label_visible = shown.time_label;
                crosshair.horz_label_visible = shown.price_label;
            }
        }
        chart.pane_options_mut().separator_color = self.pane_separator_color;
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
            set_unless_resolved(
                &mut config.bullish_border_color,
                config.bullish_color,
                self.candle_colors.border_up,
            );
            set_unless_resolved(
                &mut config.bearish_border_color,
                config.bearish_color,
                self.candle_colors.border_down,
            );
            set_unless_resolved(
                &mut config.bullish_wick_color,
                config.bullish_border_color.unwrap_or(config.bullish_color),
                self.candle_colors.wick_up,
            );
            set_unless_resolved(
                &mut config.bearish_wick_color,
                config.bearish_border_color.unwrap_or(config.bearish_color),
                self.candle_colors.wick_down,
            );
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
        config.show_indicator_backdrop = self.status_line.show_indicator_background;
        config.indicator_backdrop_opacity =
            f32::from(self.status_line.indicator_background_opacity.min(100)) / 100.0;
        config.show_symbol_labels = self.status_line.show_symbol;
        config.title_as_description = self.status_line.title_as_description;
        config.show_quote_boxes = self.status_line.show_quote_boxes;
        config.quote_panel = self.status_line.quote_panel;
        config.show_ohlc_change = self.status_line.show_change;
        config.show_ohlc_change_percent = self.status_line.show_change_percent;
        config.show_legend_volume = self.status_line.show_volume;
        config.show_indicator_labels = self.status_line.show_indicator_titles;
        config.show_indicator_inputs = self.status_line.show_indicator_inputs;
        config.show_indicator_last_val = self.status_line.show_indicator_values;
        // The placement reads both axes hidden or both shown as right, which an unedited Ok
        // keeps.
        let left = self.scales_and_lines.price_scale_left;
        let chart_left = config.show_left_axis && !config.show_right_axis;
        if axis_layout.side.is_none() && left != chart_left {
            config.show_right_axis = !left;
            config.show_left_axis = left;
        }
        config.show_currency = self.scales_and_lines.show_currency;
        config.show_unit = self.scales_and_lines.show_unit;
        config.currency_unit_on_hover = self.scales_and_lines.currency_unit_on_hover;
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
        config.show_symbol_last_change = self.scales_and_lines.show_last_price_change;
        config.show_price_line = self.scales_and_lines.show_price_line;
        config.show_symbol_prev_close = self.scales_and_lines.show_prev_close_label;
        config.show_prev_close_line = self.scales_and_lines.show_prev_close;
        config.show_countdown = self.scales_and_lines.show_countdown;
        config.show_bid_ask = self.scales_and_lines.show_bid_ask;
        if let Some(lines) = self.scales_and_lines.show_bid_ask_lines {
            config.show_bid_ask_lines = Some(lines);
        }
        if let Some(line) = self.scales_and_lines.prev_close_line {
            config.prev_close_line = line;
        }
        config.show_high_low = self.scales_and_lines.show_high_low;
        if let Some(lines) = self.scales_and_lines.show_high_low_lines {
            config.show_high_low_lines = Some(lines);
        }
        if let Some(separator) = self.scales_and_lines.thousands_sep {
            config.thousands_sep = thousands_sep(separator);
        }
        if let Some(show) = self.show_session_breaks {
            config.show_session_breaks = show;
        }
        if let Some(color) = self.price_target_positive_color {
            config.price_target_positive_color = color;
        }
        if let Some(color) = self.price_target_negative_color {
            config.price_target_negative_color = color;
        }

        set_unless_resolved(
            &mut config.vertical_grid_color,
            config.grid_color,
            self.grid_lines.vertical_color,
        );
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
        if let Some(family) = &self.scales_appearance.font_family {
            config.axis_font_family.clone_from(family);
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
        // A margin in pixels is kept in pixels, which a chart keeping the same pixels at another
        // bar spacing already shows. Otherwise an unedited margin keeps the chart's, in bars or in
        // pixels, and an edited one is in bars. Settings saved before the margin could hold part
        // of a bar, which hold none of the choices `from_chart` fills, hold the whole bars of the
        // bar margin for an unedited one.
        if let Some(pixels) = self.margins.right_pixels {
            if right_margin_pixels(chart) != Some(pixels) {
                let mut options = chart.chart_options().clone();
                options.time_scale.right_offset_pixels = Some(pixels);
                chart.set_chart_options(options);
            }
        } else {
            let whole_bars = chart
                .chart_options()
                .time_scale
                .right_offset
                .max(0.0)
                .trunc();
            let saved_unedited = self.scales_appearance.font_family.is_none()
                && self.margins.right_bars == whole_bars;
            if self.margins.right_bars != right_margin_bars(chart) && !saved_unedited {
                chart.set_right_margin(self.margins.right_bars);
            }
        }
        let mode = if self.scales_and_lines.log_scale {
            PriceScaleMode::Logarithmic
        } else if self.scales_and_lines.percentage_scale {
            PriceScaleMode::Percentage
        } else if self
            .scales_and_lines
            .indexed_to_100
            .unwrap_or(chart.state.price_scale().options().mode == PriceScaleMode::IndexedTo100)
        {
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
        let scale = chart.state.price_scale_mut();
        // A fraction shows over decimals a host chose, which it leaves for when it is cleared.
        if self.precision.fraction().is_none() {
            scale.set_price_precision(self.precision.decimals().map(|value| value as u8));
        }
        scale.set_price_fraction(self.precision.fraction());
        if let Some(style) = chart.series_style_mut(crate::widget::SeriesId::MAIN) {
            style.price_line_color = self.scales_and_lines.symbol_price_color;
            if let Some(source) = self.scales_and_lines.price_line_source {
                style.price_line_source = source;
            }
            if let Some(width) = self.scales_and_lines.price_line_width {
                style.price_line_width = width;
            }
            if let Some(line_style) = self.scales_and_lines.price_line_style {
                style.price_line_style = line_style;
            }
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
            style.body_up = colors.body_up;
            style.body_down = colors.body_down;
            style.border_up = colors.border_up;
            style.border_down = colors.border_down;
            style.wick_up = colors.wick_up;
            style.wick_down = colors.wick_down;
            if chart.chart_type_params() != &params {
                chart.apply_chart_type_params(params);
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
                chart.apply_chart_type_params(params);
            }
        } else if self.chart_type == crate::model::ChartType::VolumeCandles {
            let mut params = chart.chart_type_params().clone();
            let style = &mut params.styles.volume_candles;
            let colors = &self.candle_colors;
            if self
                .chart_type_params
                .as_ref()
                .is_some_and(|saved| saved.styles.volume_candles.is_saved_without_style())
            {
                // Settings saved before the style hold the candles' values, of which volume
                // candles drew only the body colours.
                *style = crate::model::VolumeCandlesStyle::drawn_before_style(
                    colors.body_up,
                    colors.body_down,
                );
            } else {
                style.body_visible = colors.show_body;
                style.border_visible = colors.show_border;
                style.wick_visible = colors.show_wick;
                style.body_up = colors.body_up;
                style.body_down = colors.body_down;
                style.border_up = colors.border_up;
                style.border_down = colors.border_down;
                style.wick_up = colors.wick_up;
                style.wick_down = colors.wick_down;
            }
            if chart.chart_type_params() != &params {
                chart.apply_chart_type_params(params);
            }
        } else if self.chart_type != crate::model::ChartType::Bars {
            let candles = CandlesStyle {
                body_visible: self.candle_colors.show_body,
                wick_visible: self.candle_colors.show_wick,
                color_by_previous_close: self.candle_colors.color_by_previous_close,
            };
            if chart.chart_type_params().styles.candles != candles {
                let mut params = chart.chart_type_params().clone();
                params.styles.candles = candles;
                chart.apply_chart_type_params(params);
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

    #[cfg(feature = "ui")]
    struct DepthFeed;

    #[cfg(feature = "ui")]
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
    #[cfg(feature = "ui")]
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
                flow.heatmap_retention = Some(chrono::TimeDelta::hours(1));
                flow.heatmap.brightness = 0.5;
                flow.heatmap.lower_cutoff = 2.0;
                flow.heatmap.full_intensity_size = Some(20.0);
                flow.volume_bubbles.color_by = BubbleColor::Split;
                flow.volume_bubbles.grouping =
                    BubbleGrouping::TimeWindow(chrono::TimeDelta::milliseconds(250));
                flow.volume_bubbles.scaling_mode = BubbleScalingMode::SquareRoot;
                flow.volume_bubbles.min_size_filter = Some(3.0);
                flow.volume_bubbles.clamp_std_devs = clamp_std_devs;
                flow.volume_bubbles.shaded = false;
                settings.apply_to_chart(&mut chart).unwrap();
                assert_eq!(
                    chart
                        .depth_history(crate::widget::SeriesId::MAIN)
                        .map(|h| h.config().columns),
                    (flags & 1 != 0).then_some(15)
                );
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
                // Settings saved before the flat style existed take the shaded default.
                let mut unshaded = json.clone();
                unshaded["settings"]["order_flow"]
                    .as_object_mut()
                    .unwrap()
                    .remove("heatmap_retention");
                unshaded["settings"]["order_flow"]["volume_bubbles"]
                    .as_object_mut()
                    .unwrap()
                    .remove("shaded");
                chart
                    .restore(&serde_json::from_value(unshaded).unwrap())
                    .unwrap();
                assert!(
                    ChartSettingsState::from_chart(&chart)
                        .order_flow
                        .volume_bubbles
                        .shaded
                );
                assert_eq!(
                    ChartSettingsState::from_chart(&chart)
                        .order_flow
                        .heatmap_retention,
                    None
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
    #[cfg(feature = "ui")]
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
                    settings
                        .chart_type_params
                        .as_mut()
                        .unwrap()
                        .styles
                        .heikin
                        .color_by_previous_close = previous;
                    let c = &mut settings.candle_colors;
                    c.show_body = flags & 1 != 0;
                    c.show_border = flags & 2 != 0;
                    c.show_wick = flags & 4 != 0;
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
    #[cfg(feature = "ui")]
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
                let last = chart.state.data().bars.clone();
                assert_eq!(
                    chart.main_value_color(&last, last.len() - 1),
                    style.bar_color
                );

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

    /// The state read from a chart applies back without changing its configuration, so pressing
    /// Ok without edits adds no undo step; colours that fell back to another stay unset. A colour
    /// left as shown while the one it fell back to changes keeps what it showed. A size resolved
    /// from history keeps its value until its method is edited, also through undo and redo.
    #[test]
    fn unedited_settings_apply_records_no_undo() {
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
        let mut chart = Chart::new(bars());
        let before = chart.config().clone();
        chart.user_action(|chart| {
            ChartSettingsState::from_chart(chart)
                .apply_to_chart(chart)
                .unwrap();
        });
        assert_eq!(chart.config(), &before);
        assert!(!chart.can_undo());

        let body = Color32::from_rgb(1, 100, 2);
        let grid = Color32::from_rgb(3, 4, 102);
        let (old_body, old_grid) = (before.bullish_color, before.grid_color);
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.candle_colors.body_up = body;
        settings.grid_lines.horizontal_color = grid;
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        let config = chart.config();
        assert_eq!(config.bullish_color, body);
        assert_eq!(config.bullish_border_color, Some(old_body));
        assert_eq!(
            config.bullish_wick_color.or(config.bullish_border_color),
            Some(old_body)
        );
        assert_eq!(config.grid_color, grid);
        assert_eq!(config.vertical_grid_color, Some(old_grid));
        assert!(chart.can_undo());

        // A hidden master label switch applies back without changing the individual switches.
        let mut chart = Chart::new(bars());
        chart.chart_options.crosshair.label_visible = false;
        chart.user_action(|chart| {
            ChartSettingsState::from_chart(chart)
                .apply_to_chart(chart)
                .unwrap();
        });
        let crosshair = &chart.chart_options().crosshair;
        assert!(crosshair.vert_label_visible && crosshair.horz_label_visible);
        assert!(!chart.can_undo());

        // Margins leaving no room for prices, which the fit ignores, are kept.
        let mut chart = Chart::new(bars());
        chart.price_scale_mut().options_mut().scale_margins = crate::scales::PriceScaleMargins {
            top: 0.6,
            bottom: 0.4,
        };
        chart.user_action(|chart| {
            ChartSettingsState::from_chart(chart)
                .apply_to_chart(chart)
                .unwrap();
        });
        assert!(!chart.can_undo());

        // A precision the host chose stays, also one the list lacks, on an instrument of 2.
        for decimals in [0, 1, 7, 9] {
            let mut chart = Chart::new(bars());
            let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(2);
            chart.set_instrument(Some(instrument));
            chart.price_scale_mut().set_price_precision(Some(decimals));
            chart.user_action(|chart| {
                ChartSettingsState::from_chart(chart)
                    .apply_to_chart(chart)
                    .unwrap();
            });
            let precision = chart.state().price_scale().price_precision();
            assert_eq!(precision, Some(decimals), "precision {decimals}");
            assert!(!chart.can_undo(), "precision {decimals}");
        }

        // A chosen fraction over host decimals and extended hours also apply back without
        // recording an edit.
        let mut chart = Chart::new(bars());
        chart.set_instrument(Some(
            crate::model::Instrument::new("SAMPLE").with_price_decimals(2),
        ));
        chart.price_scale_mut().set_price_precision(Some(3));
        chart
            .price_scale_mut()
            .set_price_fraction(Some(crate::model::PriceFraction::Eighths));
        chart.set_extended_hours(true);
        chart.user_action(|chart| {
            ChartSettingsState::from_chart(chart)
                .apply_to_chart(chart)
                .unwrap();
        });
        let scale = chart.state().price_scale();
        assert_eq!(
            (scale.chosen_fraction(), scale.chosen_precision()),
            (Some(crate::model::PriceFraction::Eighths), Some(3))
        );
        assert!(chart.extended_hours());
        assert!(!chart.can_undo());

        // Hidden and doubled price axes read as a right placement, which keeps them as they are.
        for (left, right) in [(false, false), (true, true)] {
            let mut chart = Chart::new(bars());
            chart.config_mut().show_left_axis = left;
            chart.config_mut().show_right_axis = right;
            chart.user_action(|chart| {
                ChartSettingsState::from_chart(chart)
                    .apply_to_chart(chart)
                    .unwrap();
            });
            let config = chart.config();
            assert_eq!(
                (config.show_left_axis, config.show_right_axis),
                (left, right)
            );
            assert!(!chart.can_undo(), "axes {left} {right}");
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.scales_and_lines.price_scale_left = true;
            settings.apply_to_chart(&mut chart).unwrap();
            let config = chart.config();
            assert_eq!(
                (config.show_left_axis, config.show_right_axis),
                (true, false)
            );
        }

        // An ATR(14) reversal of 2 on 14 bars with a true range of 2 stays through a streamed
        // bar with a true range of 16 and an unedited apply, which would make it
        // (13 * 2 + 16) / 14 = 3. An edited period resolves again: ATR(15) seeds 44 / 15.
        use crate::model::{BoxSizeMethod, ChartType, ChartTypeParams};
        let bar = |minute, high| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
            Bar::new(time, 100.0, high, 99.0, 100.0, 0.0)
        };
        let atr = |period| BoxSizeMethod::Atr {
            period,
            multiplier: 1.0,
        };
        let mut chart = Chart::new(BarData::from_bars((0..14).map(|i| bar(i, 101.0)).collect()));
        chart.set_chart_type(ChartType::Kagi);
        chart.set_chart_type_params(ChartTypeParams {
            kagi_size_method: atr(14),
            ..Default::default()
        });
        chart.update(crate::widget::SeriesId::MAIN, bar(14, 115.0));
        chart.user_action(|chart| {
            ChartSettingsState::from_chart(chart)
                .apply_to_chart(chart)
                .unwrap();
        });
        assert!((chart.chart_type_params().kagi_reversal - 2.0).abs() < 1e-12);
        assert!(!chart.can_undo());
        // An edited candle style, its undo and redo, and an unedited series settings apply keep
        // it too.
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.candle_colors.show_wick = !settings.candle_colors.show_wick;
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        assert!(chart.undo());
        assert!((chart.chart_type_params().kagi_reversal - 2.0).abs() < 1e-12);
        assert!(chart.redo());
        let series = chart.series_settings();
        chart.apply_series_settings(&series);
        assert!((chart.chart_type_params().kagi_reversal - 2.0).abs() < 1e-12);
        // Undoing the edited period gives ATR(14) a size from the 15 bars loaded now.
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings
            .chart_type_params
            .as_mut()
            .unwrap()
            .kagi_size_method = atr(15);
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        assert!((chart.chart_type_params().kagi_reversal - 44.0 / 15.0).abs() < 1e-12);
        assert!(chart.undo());
        assert!((chart.chart_type_params().kagi_reversal - 3.0).abs() < 1e-12);
    }

    /// A size that waits for more history than is loaded is not invalid: the chart names the
    /// bars it waits for and other settings apply. A fixed size of 0 is still rejected. Whitespace
    /// bars do not count, so the size resolves once 14 bars with prices are loaded. A changed
    /// method resolves again at once, also while the size waits.
    #[test]
    fn settings_apply_while_a_history_derived_size_waits() {
        use crate::model::{BoxSizeMethod, ChartType, ChartTypeParams};
        use crate::widget::test_frames::shapes;
        let time = |minute| chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minute);
        let bar = |minute| Bar::new(time(minute), 100.0, 101.0, 99.0, 100.0, 0.0);
        let bars = (-10..0)
            .map(|minute| Bar::whitespace(time(minute)))
            .chain((0..5).map(bar))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_size_method: BoxSizeMethod::Atr {
                period: 14,
                multiplier: 1.0,
            },
            ..Default::default()
        });
        ChartSettingsState::from_chart(&chart)
            .apply_to_chart(&mut chart)
            .unwrap();
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.scales_and_lines.show_price_line = !chart.config().show_price_line;
        settings.apply_to_chart(&mut chart).unwrap();
        assert_eq!(
            chart.config().show_price_line,
            settings.scales_and_lines.show_price_line
        );
        let ctx = Context::default();
        let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(t)
            if t.galley.text() == "Waiting for 14 bars of history to size the chart")));

        let params = settings.chart_type_params.as_mut().unwrap();
        params.renko_size_method = BoxSizeMethod::Fixed;
        params.renko_brick_size = 0.0;
        assert!(matches!(
            settings.apply_to_chart(&mut chart),
            Err(ChartSettingsError::ChartTypeParams {
                chart_type: ChartType::Renko,
                ..
            })
        ));
        for minute in 5..14 {
            chart.update(crate::widget::SeriesId::MAIN, bar(minute));
        }
        chart.show_elements();
        assert!((chart.chart_type_params().renko_brick_size - 2.0).abs() < 1e-12);
        // A changed method reads the loaded bars at once, also while the size waits: ATR(20)
        // waits for 6 more bars, and ATR(5) of true ranges of 2 is 2.
        for (period, size) in [(20, 0.0), (5, 2.0)] {
            let params = settings.chart_type_params.as_mut().unwrap();
            params.renko_size_method = BoxSizeMethod::Atr {
                period,
                multiplier: 1.0,
            };
            settings.apply_to_chart(&mut chart).unwrap();
            assert!((chart.chart_type_params().renko_brick_size - size).abs() < 1e-12);
        }
    }

    /// Hollow candles colour by the close against the previous close, and are hollow when the
    /// close is above the open; volume candles colour by their open or the previous close and are
    /// always filled. Each component has its own switch and colours, and saved styles and undo
    /// keep them apart from the plain candle settings.
    #[test]
    #[cfg(feature = "ui")]
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
                settings
                    .chart_type_params
                    .as_mut()
                    .unwrap()
                    .styles
                    .volume_candles
                    .color_by_previous_close = by_previous && kind == ChartType::VolumeCandles;
                let c = &mut settings.candle_colors;
                c.show_body = flags & 1 != 0;
                c.show_border = flags & 2 != 0;
                c.show_wick = flags & 4 != 0;
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

    /// Hollow candles always colour by the previous close and hide that switch. A template saved
    /// from one and loaded on a candles chart, under the open chart's type, keeps the candles'
    /// colouring by the open.
    #[test]
    fn hollow_template_keeps_the_candles_colour_rule() {
        use crate::model::ChartType;
        let bars = vec![Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            10.0,
            18.0,
            8.0,
            14.0,
            0.0,
        )];
        let mut hollow = Chart::new(BarData::from_bars(bars.clone()));
        hollow.set_chart_type(ChartType::HollowCandles);
        let mut template = ChartSettingsState::from_chart(&hollow);
        let mut candles = Chart::new(BarData::from_bars(bars));
        template.chart_type = candles.chart_type();
        template.apply_to_chart(&mut candles).unwrap();
        let styles = &candles.chart_type_params().styles;
        assert!(!styles.candles.color_by_previous_close);
    }

    /// Templates keep the previous-close rule of each style, in either direction, including
    /// when the source and destination use the same chart type and after serialization.
    #[test]
    fn bars_template_keeps_the_candles_colour_rule() {
        use crate::model::ChartType;
        let make = |kind| {
            let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                10.0,
                18.0,
                8.0,
                14.0,
                0.0,
            )]));
            chart.set_chart_type(kind);
            chart
        };
        for kind in [ChartType::Bars, ChartType::VolumeCandles, ChartType::Heikin] {
            for candle_rule in [false, true] {
                for source in [kind, ChartType::Candles] {
                    let mut chart = make(source);
                    let mut params = chart.chart_type_params().clone();
                    params.styles.candles.color_by_previous_close = candle_rule;
                    params.styles.bars.color_by_previous_close = !candle_rule;
                    params.styles.volume_candles.color_by_previous_close = !candle_rule;
                    params.styles.heikin.color_by_previous_close = !candle_rule;
                    chart.set_chart_type_params(params);
                    let template: ChartSettingsState = serde_json::from_str(
                        &serde_json::to_string(&ChartSettingsState::from_chart(&chart)).unwrap(),
                    )
                    .unwrap();
                    for destination in [ChartType::Candles, kind] {
                        let mut loaded = template.clone();
                        loaded.chart_type = destination;
                        let mut restored = make(destination);
                        loaded.apply_to_chart(&mut restored).unwrap();
                        let styles = &restored.chart_type_params().styles;
                        assert_eq!(
                            styles.candles.color_by_previous_close, candle_rule,
                            "{source:?} to {destination:?}"
                        );
                        assert_eq!(styles.bars.color_by_previous_close, !candle_rule);
                        assert_eq!(styles.volume_candles.color_by_previous_close, !candle_rule);
                        assert_eq!(styles.heikin.color_by_previous_close, !candle_rule);
                        assert_eq!(loaded.candle_colors.color_by_previous_close, candle_rule);
                    }
                }
            }
        }
    }

    /// Every TPO setting for blocks, profile length, the volume profile's levels, values, colours
    /// and side, and the price line applies as one undoable step and restores from a saved
    /// layout as edited.
    #[test]
    #[cfg(feature = "ui")]
    fn tpo_settings_undo_and_restore_every_new_field() {
        use crate::model::{ChartType, LineStyle, PriceSource, TPOProfilePeriod};
        let bars = || {
            BarData::from_bars(vec![Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                10.0,
                18.0,
                8.0,
                14.0,
                1.0,
            )])
        };
        let mut chart = Chart::new(bars());
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let before = chart.chart_type_params().clone();
        let mut edited = before.clone();
        edited.tpo_blocks = false;
        edited.tpo_letters = false;
        edited.tpo_profile_period = TPOProfilePeriod::Week;
        edited.tpo_profile_periods = 3;
        edited.tpo_volume_profile = true;
        let volume = &mut edited.tpo_volume;
        (volume.poc, volume.value_area_high, volume.value_area_low) = (true, true, true);
        (volume.values, volume.left) = (true, true);
        volume.poc_color = Some(Color32::from_rgb(1, 2, 3));
        volume.poc_style = LineStyle::Dashed;
        volume.value_area_high_color = Some(Color32::from_rgb(4, 5, 6));
        volume.value_area_high_style = LineStyle::Dotted;
        volume.value_area_low_color = Some(Color32::from_rgb(7, 8, 9));
        volume.value_area_low_style = LineStyle::Dashed;
        volume.values_color = Some(Color32::from_rgb(10, 11, 12));
        volume.color = Some(Color32::from_rgba_unmultiplied(200, 0, 0, 128));
        volume.value_area_color = Some(Color32::from_rgb(0, 0, 200));
        let line = &mut edited.styles.tpo_line;
        (line.color, line.width, line.style) =
            (Color32::from_rgb(13, 14, 15), 2.5, LineStyle::Dotted);
        edited.styles.tpo_price_source = PriceSource::High;
        chart.user_action(|chart| {
            let mut settings = ChartSettingsState::from_chart(chart);
            settings.chart_type_params = Some(edited.clone());
            settings.apply_to_chart(chart).unwrap();
        });
        assert_eq!(chart.chart_type_params(), &edited);
        assert!(chart.undo());
        assert_eq!(chart.chart_type_params(), &before);
        assert!(chart.redo());
        assert_eq!(chart.chart_type_params(), &edited);
        let saved = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
        let mut restored = Chart::new(bars());
        restored
            .restore(&serde_json::from_str(&saved).unwrap())
            .unwrap();
        assert_eq!(restored.chart_type_params(), &edited);
        // Display choices saved before Hidden retain their indices and their appearance.
        let legacy: crate::studies::StudyTemplate = serde_json::from_str(r#"{"name":"Time Price Opportunity","inputs":[["period",{"Length":30}],["row_ticks",{"Length":0}],["value_area",{"Number":70.0}],["split_sessions",{"Toggle":false}],["single_prints",{"Toggle":false}],["midpoint",{"Toggle":false}],["statistics",{"Toggle":false}],["display",{"Choice":0}],["sessions",{"Choice":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TimePriceOpportunity.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[16,16,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        let restored = legacy.create().unwrap();
        assert_eq!(restored.settings(), legacy.settings);
        assert_eq!(
            restored.tpo().unwrap().display_mode,
            crate::model::TPODisplayMode::Letters
        );
    }

    /// A Heikin-Ashi layout without the chart type's style, or without any chart type styles,
    /// tagged only the source close on its scale and restores that way; one saved with the style
    /// keeps the pair. The source (10, 18, 8, 14) closes at 14 and its transformed close is
    /// (10 + 18 + 8 + 14) / 4 = 12.5.
    #[test]
    #[cfg(feature = "ui")]
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
        for (missing, expected) in [
            (None, &[14.0, 12.5][..]),
            (Some("heikin"), &[14.0][..]),
            (Some("styles"), &[14.0][..]),
        ] {
            let mut chart = Chart::new(bars());
            chart.set_chart_type(ChartType::Heikin);
            chart.config_mut().show_volume = false;
            let mut saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            for path in ["/chart_type_params", "/settings/chart_type_params"] {
                let params = saved.pointer_mut(path).unwrap().as_object_mut().unwrap();
                match missing {
                    Some("heikin") => {
                        let styles = params["styles"].as_object_mut().unwrap();
                        assert!(styles.remove("heikin").is_some());
                    }
                    Some(key) => assert!(params.remove(key).is_some()),
                    None => {}
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
                missing.is_some()
            );
        }
    }

    /// Before hollow candles had a style, a layout held the candles' switches for them, Borders
    /// off by default, and the painter ignored them: rising candles outlined, falling candles
    /// filled, every wick drawn. Such a layout restores as it was painted, with its switches on.
    #[test]
    #[cfg(feature = "ui")]
    fn hollow_layout_saved_before_the_style_restores_its_candles() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let (up, down) = (
            Color32::from_rgb(8, 153, 129),
            Color32::from_rgb(242, 54, 69),
        );
        let bars: Vec<Bar> = [
            (10.0, 18.0, 8.0, 14.0),
            (13.0, 17.0, 8.5, 11.0),
            (11.0, 14.0, 6.0, 12.0),
            (12.0, 15.0, 9.0, 10.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c))| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
            Bar::new(time, o, h, l, c, 0.0)
        })
        .collect();
        for (show_body, show_border, show_wick) in [(true, false, true), (false, false, false)] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_chart_type(ChartType::HollowCandles);
            chart.config_mut().show_volume = false;
            let mut saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            for path in [
                "/chart_type_params/styles",
                "/settings/chart_type_params/styles",
            ] {
                let styles = saved.pointer_mut(path).unwrap().as_object_mut().unwrap();
                assert!(styles.remove("hollow").is_some());
            }
            for (name, value) in [
                ("show_body", show_body),
                ("show_border", show_border),
                ("show_wick", show_wick),
            ] {
                saved["settings"]["candle_colors"][name] = value.into();
            }
            let saved: ChartSnapshot = serde_json::from_value(saved).unwrap();
            let ctx = Context::default();
            let mut restored = Chart::new(BarData::from_bars(bars.clone()));
            restored.restore(&saved).unwrap();
            frame(&ctx, &mut restored, 0.0, vec![]);
            let painted = shapes(&ctx, &mut restored, 0.1, vec![]);
            let plot = restored.get_rendered_price_rect();
            let (mut outlined, mut filled, mut wicks) = (0, 0, 0);
            for shape in &painted {
                match shape {
                    Shape::Rect(r) if plot.contains(r.rect.center()) => {
                        if r.fill == Color32::TRANSPARENT
                            && r.stroke.width > 0.0
                            && r.stroke.color == up
                        {
                            outlined += 1;
                        } else if r.fill == down {
                            filled += 1;
                        }
                    }
                    Shape::LineSegment { points, stroke }
                        if points[0].x == points[1].x && [up, down].contains(&stroke.color) =>
                    {
                        wicks += 1;
                    }
                    _ => {}
                }
            }
            assert_eq!((outlined, filled, wicks), (2, 2, 8));
            let colors = ChartSettingsState::from_chart(&restored).candle_colors;
            assert!(colors.show_body && colors.show_border && colors.show_wick);
        }
    }

    /// Before volume candles had a style, a layout or chart settings template held the candle
    /// settings for them and the painter ignored all but the body colours: body and both wicks in
    /// the body colour of the bar's direction against its open, without a border. Such a document
    /// restores as it was painted, whatever else its candle settings held, and a layout without a
    /// configuration still gives it the candle colours of its settings. The third bar closes at
    /// 12.5, below its open of 13 and above the previous close of 11.
    #[test]
    #[cfg(feature = "ui")]
    fn volume_candle_layout_saved_before_the_style_restores_as_drawn() {
        use super::super::CandleColorConfig;
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
        let (teal, red) = (
            Color32::from_rgb(8, 153, 129),
            Color32::from_rgb(242, 54, 69),
        );
        let (border_up, border_down) =
            (Color32::from_rgb(3, 4, 102), Color32::from_rgb(103, 5, 104));
        let make = || {
            let time = chrono::DateTime::UNIX_EPOCH;
            let mut chart = Chart::new(BarData::from_bars(
                [
                    (10.0, 18.0, 8.0, 14.0),
                    (13.0, 17.0, 8.5, 11.0),
                    (13.0, 15.0, 10.0, 12.5),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (o, h, l, c))| {
                    Bar::new(time + chrono::Duration::days(i as i64), o, h, l, c, 100.0)
                })
                .collect(),
            ));
            chart.set_chart_type(ChartType::VolumeCandles);
            chart.config_mut().show_volume = false;
            chart
        };
        let saved_colors = [
            CandleColorConfig {
                color_by_previous_close: false,
                show_body: true,
                show_border: false,
                show_wick: true,
                body_up: up,
                body_down: down,
                border_up: teal,
                border_down: red,
                wick_up: teal,
                wick_down: red,
            },
            CandleColorConfig {
                color_by_previous_close: true,
                show_body: true,
                show_border: true,
                show_wick: false,
                body_up: up,
                body_down: down,
                border_up,
                border_down,
                wick_up: teal,
                wick_down: red,
            },
        ];
        // What each kind of document leaves out of its current form.
        let strip_layout = |saved: &mut serde_json::Value, what: &str| {
            let settings = saved["settings"].as_object_mut().unwrap();
            if what == "settings" || what == "version 1" {
                settings.remove("chart_type").unwrap();
                settings.remove("chart_type_params").unwrap();
            }
            if what == "version 1" {
                let layout = saved.as_object_mut().unwrap();
                layout.remove("config").unwrap();
                layout.remove("options").unwrap();
            }
            for path in ["/chart_type_params", "/settings/chart_type_params"] {
                let Some(params) = saved.pointer_mut(path) else {
                    continue;
                };
                let params = params.as_object_mut().unwrap();
                if what == "styles" {
                    params.remove("styles").unwrap();
                } else {
                    let styles = params["styles"].as_object_mut().unwrap();
                    styles.remove("volume_candles").unwrap();
                }
            }
        };
        for colors in saved_colors {
            for what in ["style", "styles", "settings", "version 1", "template"] {
                let mut restored = make();
                if what == "template" {
                    let mut saved =
                        serde_json::to_value(ChartSettingsState::from_chart(&make())).unwrap();
                    saved
                        .pointer_mut("/chart_type_params/styles")
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove("volume_candles")
                        .unwrap();
                    saved["candle_colors"] = serde_json::to_value(&colors).unwrap();
                    let saved: ChartSettingsState = serde_json::from_value(saved).unwrap();
                    saved.apply_to_chart(&mut restored).unwrap();
                } else {
                    let mut saved = serde_json::to_value(make().snapshot().unwrap()).unwrap();
                    saved["settings"]["candle_colors"] = serde_json::to_value(&colors).unwrap();
                    strip_layout(&mut saved, what);
                    let saved: ChartSnapshot = serde_json::from_value(saved).unwrap();
                    restored.restore(&saved).unwrap();
                    if what == "version 1" {
                        // A layout without a configuration gives it the candle colours of its
                        // settings, as it did before.
                        let config = restored.config();
                        assert_eq!(
                            (config.bullish_color, config.bearish_color),
                            (up, down),
                            "{colors:?}"
                        );
                    }
                }
                let ctx = Context::default();
                frame(&ctx, &mut restored, 0.0, vec![]);
                let painted = shapes(&ctx, &mut restored, 0.1, vec![]);
                let plot = restored.get_rendered_price_rect();
                let known = [up, down, teal, red, border_up, border_down];
                let (mut bodies, mut wicks) = (Vec::new(), Vec::new());
                for shape in painted {
                    match shape {
                        Shape::Rect(r)
                            if plot.contains(r.rect.center()) && known.contains(&r.fill) =>
                        {
                            bodies.push((r.fill, r.stroke.width));
                        }
                        Shape::LineSegment { points, stroke }
                            if points[0].x == points[1].x && known.contains(&stroke.color) =>
                        {
                            wicks.push(stroke.color);
                        }
                        _ => {}
                    }
                }
                assert_eq!(
                    bodies,
                    [(up, 0.0), (down, 0.0), (down, 0.0)],
                    "{what} {colors:?}"
                );
                assert_eq!(wicks, [up, up, down, down, down, down], "{what} {colors:?}");
                // Settings that predate chart types also retain the plain candles' switch.
                let candle_rule =
                    colors.color_by_previous_close && matches!(what, "settings" | "version 1");
                let styles = &restored.chart_type_params().styles;
                assert_eq!(
                    (
                        styles.candles.color_by_previous_close,
                        styles.volume_candles.color_by_previous_close,
                    ),
                    (candle_rule, false),
                    "{what} {colors:?}"
                );
                let shown = ChartSettingsState::from_chart(&restored).candle_colors;
                assert_eq!(
                    shown,
                    CandleColorConfig {
                        color_by_previous_close: candle_rule,
                        show_body: true,
                        show_border: false,
                        show_wick: true,
                        body_up: up,
                        body_down: down,
                        border_up: up,
                        border_down: down,
                        wick_up: up,
                        wick_down: down,
                    },
                    "{what} {colors:?}"
                );
                // Settings read from the restored chart are no longer those of an old document.
                let mut edited = ChartSettingsState::from_chart(&restored);
                edited.candle_colors.wick_up = teal;
                edited.apply_to_chart(&mut restored).unwrap();
                let style = &restored.chart_type_params().styles.volume_candles;
                assert_eq!(style.wick_up, teal, "{what} {colors:?}");
            }
        }
        // A layout of another chart type has nothing of the old drawing to keep.
        let mut other = make();
        other.set_chart_type(ChartType::Candles);
        let mut saved = serde_json::to_value(other.snapshot().unwrap()).unwrap();
        strip_layout(&mut saved, "style");
        let saved: ChartSnapshot = serde_json::from_value(saved).unwrap();
        other.restore(&saved).unwrap();
        assert_eq!(
            other.chart_type_params().styles.volume_candles,
            crate::model::VolumeCandlesStyle::default()
        );
    }

    /// The default price labels are printed in the price scale's text colour, so they read at the
    /// contrast the built-in themes hold their price text to on the chart background: 4.5:1, and
    /// 7:1 in high contrast. No fixed colour meets both on white and on black.
    #[test]
    fn high_low_default_labels_read_on_every_theme() {
        use crate::model::ChartType;
        use crate::widget::test_frames::{frame, shapes};

        for (name, theme) in crate::theme::presets() {
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
            chart.set_chart_type(ChartType::HighLow);
            chart.config_mut().show_volume = false;
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.state.time_scale_mut().set_bar_spacing(80.0);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let ratios: Vec<f32> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(t)
                        if t.galley.job.sections[0].format.font_id.size == 7.0
                            && plot.contains(t.visual_bounding_rect().center()) =>
                    {
                        let color = t.galley.job.sections[0].format.color;
                        Some(crate::theme::contrast(color, theme.chart.bg))
                    }
                    _ => None,
                })
                .collect();
            let least = if name == "high-contrast" { 7.0 } else { 4.5 };
            assert_eq!(ratios.len(), 2, "{name}: labels at {ratios:?}");
            assert!(ratios.iter().all(|r| *r >= least), "{name}: {ratios:?}");
        }
    }

    /// Text is drawn at 1 to 256 points (`styles::typography::FONT_SIZES`): a larger high-low
    /// label is drawn at 256 points, and `Chart::restore` refuses it as it refuses the other font
    /// sizes in a saved layout, and `Chart::snapshot` refuses a chart that holds one. Label sizes of
    /// zero or below hide the labels and still restore.
    #[test]
    #[cfg(feature = "ui")]
    fn high_low_label_size_is_bounded_like_other_text() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        use egui::Color32;

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
        let mut chart = Chart::new(bars());
        chart.set_chart_type(ChartType::HighLow);
        chart.config_mut().show_volume = false;
        let saved_at = |size: f32| {
            let mut saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            saved["chart_type_params"]["styles"]["high_low"]["label_font_size"] =
                serde_json::json!(size);
            saved["settings"]["chart_type_params"]["styles"]["high_low"]["label_font_size"] =
                serde_json::json!(size);
            serde_json::from_value::<ChartSnapshot>(saved).unwrap()
        };
        // Restored into charts that are never painted: laying the label out at the oversized
        // size is what the bound exists to prevent.
        let refused = [100000.0, 0.5].into_iter().all(|size| {
            matches!(
                Chart::new(bars()).restore(&saved_at(size)),
                Err(crate::widget::RestoreError::Layout(
                    "Font sizes must be between 1 and 256"
                ))
            )
        });
        let hidden_restore = [0.0, -3.0]
            .into_iter()
            .all(|size| Chart::new(bars()).restore(&saved_at(size)).is_ok());

        // A single bar always fits its labels, so the size reaches the painter.
        let label = Color32::from_rgb(3, 4, 103);
        let mut params = chart.chart_type_params().clone();
        params.styles.high_low.label_color = label;
        params.styles.high_low.label_font_size = 300.0;
        chart.set_chart_type_params(params);
        let snapshot_refused = chart.snapshot().is_err();
        let ctx = Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let sizes: Vec<f32> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(t)
                    if t.galley.text() == "18.00"
                        && t.galley.job.sections[0].format.color == label =>
                {
                    Some(t.galley.job.sections[0].format.font_id.size)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            (refused, hidden_restore, snapshot_refused, sizes),
            (true, true, true, vec![256.0])
        );
    }

    /// A high-low document without the chart type's style printed no price labels, so a layout or
    /// a chart settings template saved then restores without them; a layout saved with the style
    /// keeps its six. Labels are picked by their text and size, whatever their colour.
    #[test]
    #[cfg(feature = "ui")]
    fn high_low_layout_saved_before_the_style_restores_without_labels() {
        use crate::model::ChartType;
        use crate::widget::ChartSnapshot;
        use crate::widget::test_frames::{frame, shapes};
        let bars = || {
            BarData::from_bars(
                [
                    (10.0, 18.0, 8.0, 14.0),
                    (13.0, 17.0, 8.5, 11.0),
                    (12.0, 15.0, 9.0, 10.0),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (o, h, l, c))| {
                    let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(i as i64);
                    Bar::new(time, o, h, l, c, 0.0)
                })
                .collect(),
            )
        };
        let source = || {
            let mut chart = Chart::new(bars());
            chart.set_chart_type(ChartType::HighLow);
            chart.config_mut().show_volume = false;
            chart
        };
        let without_style = |saved: &mut serde_json::Value, paths: &[&str]| {
            for path in paths {
                let styles = saved.pointer_mut(path).unwrap().as_object_mut().unwrap();
                assert!(styles.remove("high_low").is_some());
            }
        };
        let layouts = [
            "/chart_type_params/styles",
            "/settings/chart_type_params/styles",
        ];
        let layout = |strip: bool| {
            let mut saved = serde_json::to_value(source().snapshot().unwrap()).unwrap();
            if strip {
                without_style(&mut saved, &layouts);
            }
            let saved: ChartSnapshot = serde_json::from_value(saved).unwrap();
            let mut restored = Chart::new(bars());
            restored.restore(&saved).unwrap();
            restored
        };
        let template = || {
            let mut saved =
                serde_json::to_value(ChartSettingsState::from_chart(&source())).unwrap();
            without_style(&mut saved, &["/chart_type_params/styles"]);
            let saved: ChartSettingsState = serde_json::from_value(saved).unwrap();
            let mut chart = source();
            saved.apply_to_chart(&mut chart).unwrap();
            chart
        };
        for (name, mut chart, expected) in [
            ("layout with the style", layout(false), 6),
            ("layout without the style", layout(true), 0),
            ("template without the style", template(), 0),
        ] {
            let ctx = Context::default();
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.state.time_scale_mut().set_bar_spacing(80.0);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let prices = ["18.00", "8.00", "17.00", "8.50", "15.00", "9.00"];
            let labels = painted
                .iter()
                .filter(|shape| {
                    matches!(shape, Shape::Text(t)
                    if prices.contains(&t.galley.text())
                    && t.galley.job.sections[0].format.font_id.size == 7.0
                    && plot.contains(t.pos))
                })
                .count();
            assert_eq!(labels, expected, "{name}");
            assert_eq!(
                chart.chart_type_params().styles.high_low.labels_visible,
                expected > 0,
                "{name}"
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
    #[cfg(feature = "ui")]
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

    /// The last price line's width and pattern and the change on the last price tag reach the
    /// painted main series on normal, percentage and indexed scales and read back; settings and
    /// layouts saved without them keep the chart's line and the value alone.
    #[test]
    fn last_price_line_and_change_tag_apply_and_read_back() {
        use crate::model::LineStyle;
        use crate::widget::test_frames::{chart, frame, shapes};
        /// The (width, length) of each horizontal stroke of the gold line at 115, the plot
        /// width and the text of the gold tag.
        fn painted(ctx: &Context, chart: &mut Chart) -> (Vec<(f32, f32)>, f32, String) {
            frame(ctx, chart, 0.0, vec![]);
            frame(ctx, chart, 0.1, vec![]);
            let painted = shapes(ctx, chart, 0.2, vec![]);
            let plot = chart.last_rendered_price_rect;
            let y = chart.last_price_map.price_to_y(115.0);
            let strokes = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == Color32::GOLD
                            && (points[0].y - y).abs() < 0.001
                            && (points[1].y - y).abs() < 0.001 =>
                    {
                        Some((stroke.width, (points[1].x - points[0].x).abs()))
                    }
                    _ => None,
                })
                .collect();
            let tag = painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Rect(r) if r.fill == Color32::GOLD && r.rect.left() >= plot.right() => {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .expect("last price tag");
            let text = painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Text(t) if tag.contains(t.pos + t.galley.size() / 2.0) => {
                        Some(t.galley.text().to_owned())
                    }
                    _ => None,
                })
                .expect("last price tag text");
            (strokes, plot.width(), text)
        }
        // Daily bars: bar 99 closes at 115 and bar 98, the previous session, at 110, so the
        // change is +5 and 5 / 110 = 4.5454...%.
        const CHANGE: &str = "+5.00 (+4.55%)";
        for mode in [
            PriceScaleMode::Normal,
            PriceScaleMode::Percentage,
            PriceScaleMode::IndexedTo100,
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            if mode == PriceScaleMode::IndexedTo100 {
                // The settings have no indexed switch; applying keeps an indexed scale.
                chart.price_scale_mut().options_mut().mode = mode;
            }
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.scales_and_lines.symbol_price_color = Some(Color32::GOLD);
            settings.scales_and_lines.percentage_scale = mode == PriceScaleMode::Percentage;
            settings.apply_to_chart(&mut chart).unwrap();
            // Today's line and tag: 1 point dashes 4 points long and the value alone.
            let (strokes, _, value) = painted(&ctx, &mut chart);
            assert!(
                strokes.len() > 1 && strokes.iter().all(|&(w, l)| w == 1.0 && l <= 4.001),
                "{mode:?}: {strokes:?}"
            );
            assert!(!value.contains('\n'), "{mode:?}: {value}");
            if mode == PriceScaleMode::Normal {
                assert_eq!(value, "115.00");
            }
            for (style, width) in [(LineStyle::Solid, 3.0), (LineStyle::LargeDashed, 2.0)] {
                settings.scales_and_lines.price_line_style = Some(style);
                settings.scales_and_lines.price_line_width = Some(width);
                settings.scales_and_lines.show_last_price_change = true;
                settings.scales_and_lines.currency_unit_on_hover = true;
                settings.apply_to_chart(&mut chart).unwrap();
                let choices = |scales: &crate::widget::settings::ScalesAndLinesSettings| {
                    (
                        scales.price_line_width,
                        scales.price_line_style,
                        scales.show_last_price_change,
                        scales.currency_unit_on_hover,
                    )
                };
                assert_eq!(
                    choices(&ChartSettingsState::from_chart(&chart).scales_and_lines),
                    choices(&settings.scales_and_lines)
                );
                let (strokes, plot_width, text) = painted(&ctx, &mut chart);
                let longest = strokes.iter().map(|s| s.1).fold(0.0, f32::max);
                assert!(
                    !strokes.is_empty() && strokes.iter().all(|s| s.0 == width),
                    "{mode:?} {style:?}: {strokes:?}"
                );
                match style {
                    LineStyle::Solid => assert!(longest > plot_width - 1.0, "{longest}"),
                    _ => assert!(
                        strokes.len() > 1 && (longest - 8.0).abs() < 0.001,
                        "{longest}"
                    ),
                }
                // Percentage and indexed scales keep their label and add the price move.
                let price = text
                    .strip_suffix(&format!("\n{CHANGE}"))
                    .unwrap_or_else(|| panic!("{mode:?}: {text}"));
                assert!(!price.contains('\n'), "{mode:?}: {text}");
                if mode == PriceScaleMode::Normal {
                    assert_eq!(price, "115.00");
                }
            }
            // The change follows the chosen precision, as in the legend: +5 becomes +5.000.
            settings.precision = PrecisionMode::Decimals3;
            settings.apply_to_chart(&mut chart).unwrap();
            let (_, _, text) = painted(&ctx, &mut chart);
            assert!(text.ends_with("\n+5.000 (+4.55%)"), "{mode:?}: {text}");
            // Settings and templates saved before the fields existed keep the chart's line and
            // show the value alone.
            let mut saved = serde_json::to_value(&settings).unwrap();
            let scales = saved["scales_and_lines"].as_object_mut().unwrap();
            for key in [
                "price_line_width",
                "price_line_style",
                "show_last_price_change",
                "currency_unit_on_hover",
            ] {
                assert!(scales.remove(key).is_some(), "{key}");
            }
            let old: ChartSettingsState = serde_json::from_value(saved).unwrap();
            old.apply_to_chart(&mut chart).unwrap();
            let (strokes, _, text) = painted(&ctx, &mut chart);
            assert!(strokes.iter().all(|s| s.0 == 2.0), "{mode:?}");
            assert!(!text.contains('\n'), "{mode:?}: {text}");
            assert!(!chart.config().currency_unit_on_hover);
        }
        // Layouts saved before the switches existed load with the value alone and the currency
        // always shown.
        let mut saved = serde_json::to_value(ChartConfig::default()).unwrap();
        let config = saved.as_object_mut().unwrap();
        for key in ["show_symbol_last_change", "currency_unit_on_hover"] {
            assert!(config.remove(key).is_some(), "{key}");
        }
        let config: ChartConfig = serde_json::from_value(saved).unwrap();
        assert!(!config.show_symbol_last_change && !config.currency_unit_on_hover);
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
                    // `hover` shows the labels only while the pointer is over the price scale:
                    // `Some(over)` puts the pointer over it or not.
                    for (currency, unit, hover, shown) in [
                        (true, true, None, true),
                        (true, false, None, true),
                        (false, true, None, true),
                        (false, false, None, false),
                        (true, true, Some(false), true),
                        (true, true, Some(true), true),
                    ] {
                        settings.scales_and_lines.show_currency = currency;
                        settings.scales_and_lines.show_unit = unit;
                        settings.scales_and_lines.currency_unit_on_hover = hover.is_some();
                        settings.scales_and_lines.show_study_values = shown;
                        settings.scales_and_lines.show_last_price_label = shown;
                        settings.scales_and_lines.show_extended_session_price = shown;
                        settings.apply_to_chart(&mut chart).unwrap();
                        for frame in 0..3 {
                            let plot = chart.last_rendered_price_rect;
                            let axis_x = if left {
                                plot.left() - 10.0
                            } else {
                                plot.right() + 10.0
                            };
                            let input = egui::RawInput {
                                events: hover
                                    .filter(|over| *over)
                                    .map(|_| egui::Event::PointerMoved(egui::pos2(axis_x, 300.0)))
                                    .into_iter()
                                    .collect(),
                                ..Default::default()
                            };
                            let mut output = ctx.run_ui(input, |ui| {
                                chart.show(ui);
                            });
                            let shapes = std::mem::take(&mut output.shapes);
                            output.drop_without_applying_deltas();
                            if frame == 2 {
                                let plot = chart.last_rendered_price_rect;
                                let expected = match (currency, unit, hover) {
                                    (_, _, Some(false)) => None,
                                    (true, true, _) => Some("USD · contracts per barrel"),
                                    (true, false, _) => Some("USD"),
                                    (false, true, _) => Some("contracts per barrel"),
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
    /// study and time axes and in a maximized pane, and read back from the chart. So does the
    /// scale text colour, which also reaches the axis of a study on a scale of its own.
    #[test]
    fn scale_appearance_and_pane_button_settings_render_and_read_back() {
        use crate::studies::{RSI, SMA, StudyScale};
        use crate::widget::PaneId;
        use crate::widget::test_frames::{chart, frame, shapes};
        const TEXT: Color32 = Color32::from_rgb(1, 2, 3);
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
            let average = chart.add_indicator(Box::new(SMA::new(5)));
            assert!(chart.set_indicator_scale(average, StudyScale::Independent));
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.pane_layout.maximized = maximized.then(|| chart.series_pane(study).unwrap());
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.scales_appearance.font_size = 18;
            settings.scales_appearance.text_color = TEXT;
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
            // The price axis and the average's own axis beside the price pane, the study pane's
            // axis and the time axis.
            let mut axes = vec![
                egui::Rect::from_x_y_ranges(
                    pane.chart_rect.right()..=pane.panel_rect.right(),
                    pane.chart_rect.y_range(),
                ),
                chart.time_axis_rect,
            ];
            if !maximized {
                let price_pane = &chart.pane_layout.panes[0];
                assert_eq!(price_pane.id, PaneId::MAIN);
                let beside: Vec<_> = chart
                    .pane_scale_axes(PaneId::MAIN, price_pane.rect, plot)
                    .map(|(_, axis)| axis)
                    .collect();
                assert_eq!(beside.len(), 2, "{beside:?}");
                axes.extend(beside);
            }
            for axis in axes {
                let label = painted.iter().any(|shape| {
                    matches!(shape, Shape::Text(text)
                    if axis.contains(text.pos)
                        && text.galley.job.sections[0].format.color == TEXT)
                });
                assert!(label, "no label in the text colour on the axis at {axis:?}");
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

    /// Ticking the ratio lock in the settings of an unlocked chart, as the dialog does, locks the
    /// view the chart shows, as switching on the configuration's lock does.
    #[test]
    fn locking_the_ratio_in_settings_keeps_the_view() {
        use crate::widget::test_frames::frame;
        let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let bars = (0..50)
            .map(|i| {
                let close = (10.0 + (i % 5) as f64) / 1000.0;
                let time = start + chrono::Duration::minutes(i);
                Bar::new(time, close, 0.016, 0.009, close, 1.0)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(crate::model::Timeframe::Min1);
        chart.config_mut().show_volume = false;
        let ctx = Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.scales_and_lines.lock_price_to_bar_ratio = true;
        // The dialog turns auto scale off with the lock.
        settings.scales_and_lines.auto_scale = false;
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        frame(&ctx, &mut chart, 1.0, vec![]);
        let range = chart.state().price_scale().range();
        // The bars span 0.009 to 0.016: ten times the span they are fitted in is below 0.0875,
        // while one price unit per bar spans the plot's height in bar widths, about 91.
        assert!(range.max - range.min < 0.0875, "{range:?}");
    }

    /// Fractional settings reach axis ticks, price tags and readouts, read back, survive layout
    /// restoration and undo, and give way to decimals. An apostrophe groups no thousands.
    #[test]
    fn fractional_precision_reaches_the_axis_and_reads_back() {
        use crate::chart::rendering::axes::price_marks;
        use crate::model::{Instrument, PriceFraction};
        use crate::widget::{
            SeriesId,
            test_frames::{chart, shapes},
        };
        let ctx = Context::default();
        let mut chart = chart(&[1234.0], &[1234.5]);
        chart.set_instrument(Some(Instrument::new("ZB").with_price_decimals(2)));
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.precision = PrecisionMode::Fraction(PriceFraction::ThirtySeconds);
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
        assert_eq!(
            chart.format_series_price(SeriesId::MAIN, 1234.5).as_deref(),
            Some("1,234'16")
        );
        assert!(
            painted.iter().any(
                |shape| matches!(shape, Shape::Text(text) if text.galley.text() == "1,234'16")
            )
        );
        let map = chart.series_price_map(SeriesId::MAIN).unwrap();
        let formatter = chart
            .series_label_formatter(SeriesId::MAIN, PriceScaleMode::Normal)
            .unwrap();
        let marks = price_marks(&map, Some(formatter.as_ref()));
        assert!(!marks.is_empty());
        assert!(
            marks
                .iter()
                .all(|mark| (mark.price * 32.0).fract() == 0.0 && mark.label.contains('\'')),
            "{marks:?}"
        );
        assert!(marks.windows(2).all(|pair| pair[0].label != pair[1].label));
        assert_eq!(
            ChartSettingsState::from_chart(&chart).precision,
            settings.precision
        );

        #[cfg(feature = "ui")]
        {
            let saved = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
            let mut restored = crate::widget::test_frames::chart(&[1234.0], &[1234.5]);
            restored.set_instrument(Some(Instrument::new("ZB").with_price_decimals(2)));
            restored
                .restore(&serde_json::from_str(&saved).unwrap())
                .unwrap();
            shapes(&ctx, &mut restored, 0.1, vec![]);
            assert_eq!(
                restored
                    .format_series_price(SeriesId::MAIN, 1234.5)
                    .as_deref(),
                Some("1,234'16")
            );
        }

        chart.config_mut().thousands_sep = Some('\'');
        shapes(&ctx, &mut chart, 0.2, vec![]);
        assert_eq!(
            chart.format_series_price(SeriesId::MAIN, 1234.5).as_deref(),
            Some("1234'16")
        );
        chart.config_mut().thousands_sep = Some(',');
        assert!(chart.undo());
        shapes(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(
            chart.format_series_price(SeriesId::MAIN, 1234.5).as_deref(),
            Some("1,234.50")
        );
        assert!(chart.redo());
        settings.precision = PrecisionMode::Decimals3;
        chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
        shapes(&ctx, &mut chart, 0.4, vec![]);
        assert_eq!(
            chart.format_series_price(SeriesId::MAIN, 1234.5).as_deref(),
            Some("1,234.500")
        );
    }

    /// Optional label and display choices reach the chart, read back and are undone. Settings
    /// saved before they could be chosen keep the chart's choices.
    #[test]
    fn optional_choices_apply_read_back_and_survive_older_saves() {
        use crate::model::PriceLineSource::{LastBar, LastVisible};
        use crate::scales::{
            TickMarkType,
            WeekdayNames::{Always, Never},
        };
        use crate::widget::{SeriesId, test_frames::chart};
        use chrono::TimeZone;
        let time = chrono::Utc
            .with_ymd_and_hms(2024, 3, 13, 14, 30, 0)
            .unwrap();
        let read = |chart: &Chart| {
            let style = chart.series_style(SeriesId::MAIN).unwrap();
            let labels = chart.build_time_formatter();
            (
                chart.config().show_session_breaks,
                style.price_line_source,
                chart.config().prev_close_line,
                chart.config().show_bid_ask_lines,
                chart.config().show_high_low_lines,
                [
                    labels.format_crosshair(time, 60),
                    labels.format_crosshair(time, 86_400),
                    labels.format(time, TickMarkType::Time),
                ],
                (chart.extended_hours(), chart.overnight_hours()),
                chart
                    .chart_options()
                    .time_scale
                    .keep_left_edge_on_interval_change,
            )
        };
        let mut chart = chart(&[100.0], &[110.0]);
        let new_chart = ChartSettingsState::from_chart(&chart);
        let solid = crate::config::ChartLineStyle {
            color: Some(Color32::GOLD),
            width: 2.0,
            style: crate::model::LineStyle::Solid,
        };
        let rows = [
            (
                true,
                LastVisible,
                DateFormat::Default,
                false,
                Never,
                ["13 Mar '24 02:30 PM", "Mar 13, 2024", "02:30 PM"],
                solid,
                true,
                true,
            ),
            (
                false,
                LastBar,
                DateFormat::Default,
                true,
                Always,
                ["Wed 13 Mar '24 14:30", "Wed Mar 13, 2024", "14:30"],
                Default::default(),
                false,
                false,
            ),
            (
                false,
                LastBar,
                DateFormat::Pattern("%d.%m.%Y".into()),
                true,
                Always,
                ["Wed 13.03.2024 14:30", "Wed 13.03.2024", "14:30"],
                Default::default(),
                false,
                true,
            ),
        ];
        for (breaks, source, date_format, use_24_hour, weekday_names, labels, line, lines, keep) in
            &rows
        {
            let mut settings = ChartSettingsState::from_chart(&chart);
            settings.show_session_breaks = Some(*breaks);
            settings.extended_hours = Some(*breaks);
            settings.overnight_hours = Some(*breaks);
            let scales = &mut settings.scales_and_lines;
            scales.price_line_source = Some(*source);
            scales.prev_close_line = Some(*line);
            scales.show_bid_ask_lines = Some(*lines);
            scales.show_high_low_lines = Some(*lines);
            scales.date_format = Some(date_format.clone());
            scales.use_24_hour = Some(*use_24_hour);
            scales.weekday_names = Some(*weekday_names);
            scales.keep_left_edge_on_interval_change = Some(*keep);
            chart.user_action(|chart| settings.apply_to_chart(chart).unwrap());
            assert_eq!(
                read(&chart),
                (
                    *breaks,
                    *source,
                    *line,
                    Some(*lines),
                    Some(*lines),
                    labels.map(String::from),
                    (*breaks, *breaks),
                    *keep
                )
            );
            let settings = ChartSettingsState::from_chart(&chart);
            let saved: ChartSettingsState =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            let mut target = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            saved.apply_to_chart(&mut target).unwrap();
            assert_eq!(
                read(&target),
                read(&chart),
                "template applied to another chart"
            );
            let scales = &saved.scales_and_lines;
            assert_eq!(
                (
                    settings.show_session_breaks,
                    scales.price_line_source,
                    scales.date_format.clone(),
                    scales.use_24_hour,
                    scales.weekday_names,
                    scales.prev_close_line,
                    scales.show_bid_ask_lines,
                    scales.show_high_low_lines,
                    (settings.extended_hours, settings.overnight_hours),
                    scales.keep_left_edge_on_interval_change
                ),
                (
                    Some(*breaks),
                    Some(*source),
                    Some(date_format.clone()),
                    Some(*use_24_hour),
                    Some(*weekday_names),
                    Some(*line),
                    Some(*lines),
                    Some(*lines),
                    (Some(*breaks), Some(*breaks)),
                    Some(*keep)
                )
            );
        }
        for row in [&rows[1], &rows[0]] {
            assert!(chart.undo());
            assert_eq!(
                read(&chart),
                (
                    row.0,
                    row.1,
                    row.6,
                    Some(row.7),
                    Some(row.7),
                    row.5.map(String::from),
                    (row.0, row.0),
                    row.8
                )
            );
        }
        // A date-only action also redoes, then returns to the older-save setup.
        assert!(chart.redo());
        assert!(chart.redo());
        assert_eq!(
            read(&chart),
            (
                rows[2].0,
                rows[2].1,
                rows[2].6,
                Some(rows[2].7),
                Some(rows[2].7),
                rows[2].5.map(String::from),
                (rows[2].0, rows[2].0),
                rows[2].8
            )
        );
        assert!(chart.undo());
        assert!(chart.undo());

        chart.set_date_format(Some("%d.%m.%Y".into()));
        let mut options = chart.chart_options().clone();
        options.time_scale.use_24_hour = false;
        options.time_scale.weekday_names = Always;
        options.time_scale.keep_left_edge_on_interval_change = true;
        chart.set_chart_options(options);
        chart.config_mut().axis_font_family = egui::FontFamily::Monospace;
        chart.config_mut().thousands_sep = Some('_');
        let mut saved = serde_json::to_value(new_chart).unwrap();
        for (group, choice) in [
            ("", "show_session_breaks"),
            ("", "extended_hours"),
            ("", "overnight_hours"),
            ("/scales_and_lines", "price_line_source"),
            ("/scales_and_lines", "prev_close_line"),
            ("/scales_and_lines", "show_bid_ask_lines"),
            ("/scales_and_lines", "show_high_low_lines"),
            ("/scales_and_lines", "thousands_sep"),
            ("/scales_and_lines", "date_format"),
            ("/scales_and_lines", "use_24_hour"),
            ("/scales_and_lines", "weekday_names"),
            ("/scales_and_lines", "keep_left_edge_on_interval_change"),
            ("/scales_appearance", "font_family"),
        ] {
            let group = saved.pointer_mut(group).unwrap().as_object_mut().unwrap();
            group.remove(choice).expect(choice);
        }
        let saved: ChartSettingsState = serde_json::from_value(saved).unwrap();
        saved.apply_to_chart(&mut chart).unwrap();
        assert_eq!(
            read(&chart),
            (
                true,
                LastVisible,
                solid,
                Some(true),
                Some(true),
                ["Wed 13.03.2024 02:30 PM", "Wed 13.03.2024", "02:30 PM"].map(String::from),
                (true, true),
                true
            )
        );
        assert_eq!(chart.config().axis_font_family, egui::FontFamily::Monospace);
        assert_eq!(chart.config().thousands_sep, Some('_'));
        // Regular hours win even when saved flags contradict one another.
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.extended_hours = Some(false);
        settings.overnight_hours = Some(true);
        settings.apply_to_chart(&mut chart).unwrap();
        assert_eq!(
            (chart.extended_hours(), chart.overnight_hours()),
            (false, false)
        );
    }

    /// The right margin row shows the margin the view keeps: part of a bar as it is, and a margin
    /// set in pixels as its width in bars at the bar spacing. Unedited settings leave the bar and
    /// pixel margins as they were, and an edit sets the margin in bars. Settings saved before the
    /// row could hold part of a bar hold the whole bars of the bar margin and keep the chart's.
    #[test]
    fn right_margin_row_shows_the_margin_the_view_keeps() {
        use crate::widget::test_frames::chart;
        // A new chart, and one keeping 60 pixels at the default bar spacing of 6 points.
        for (pixels, shown) in [(None, 2.5), (Some(60.0), 10.0)] {
            let mut options = crate::config::ChartOptions::default();
            options.time_scale.right_offset_pixels = pixels;
            let mut chart = chart(&[100.0], &[110.0]).with_chart_options(options);
            let kept = chart.chart_options().time_scale.clone();
            let mut settings = ChartSettingsState::from_chart(&chart);
            assert_eq!(settings.margins.right_bars, shown, "{pixels:?}");
            settings.apply_to_chart(&mut chart).unwrap();
            assert_eq!(
                chart.chart_options().time_scale,
                kept,
                "unedited, {pixels:?}"
            );

            // The margins a new chart saved before the change, without the later choices.
            let mut saved = serde_json::to_value(&settings).unwrap();
            saved["margins"] = serde_json::from_str(
                r#"{"top_percent":10.0,"bottom_percent":10.0,"right_bars":2}"#,
            )
            .unwrap();
            let appearance = saved["scales_appearance"].as_object_mut().unwrap();
            appearance.remove("font_family").unwrap();
            let saved: ChartSettingsState = serde_json::from_value(saved).unwrap();
            saved.apply_to_chart(&mut chart).unwrap();
            assert_eq!(chart.chart_options().time_scale, kept, "saved, {pixels:?}");

            // An edit, also to the whole bars of the bar margin, is in bars, as the dialog makes it.
            for bars in [2.0, 7.5] {
                settings.margins.right_bars = bars;
                settings.margins.right_pixels = None;
                settings.apply_to_chart(&mut chart).unwrap();
                let time_scale = &chart.chart_options().time_scale;
                assert_eq!(
                    (time_scale.right_offset, time_scale.right_offset_pixels),
                    (bars, None),
                    "{pixels:?}"
                );
            }
            settings.margins.right_bars = -1.0;
            assert_eq!(
                settings.apply_to_chart(&mut chart),
                Err(ChartSettingsError::RightMargin(-1.0))
            );
            settings.margins.right_bars = 0.0;
            settings.margins.right_pixels = Some(-1.0);
            assert_eq!(
                settings.apply_to_chart(&mut chart),
                Err(ChartSettingsError::RightMargin(-1.0))
            );
        }
    }
}
