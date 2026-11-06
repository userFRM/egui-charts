//! Symbol tab - chart type, price source, and line/candle style settings.

use crate::chart::series::SeriesSettings;
use crate::localization as l10n;
use crate::model::{
    BaselineLevel, BoxSizeMethod, ChartType, ChartTypeParams, FootprintColor, FootprintMode,
    FootprintStatistic, HighLowStyle, LinePlotStyle, LineStyle, PriceBasedSource, PriceSource,
    ProfileRows, ProfileSession, TimezoneId,
};
use crate::theme::ThemeExt;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid};
use chrono::{NaiveTime, Timelike};
use egui::{Color32, RichText, Ui};

/// Symbol tab renderer - shows chart type and style settings
pub struct SymbolTab;

impl SymbolTab {
    /// Show the symbol tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();
        match settings.chart_type {
            ChartType::VolumeFootprint => {
                let candles = [settings.bullish_color, settings.bearish_color];
                Self::footprint(ui, &mut settings.chart_type_params, candles);
                return;
            }
            ChartType::SessionVolumeProfile => {
                Self::profile(ui, &mut settings.chart_type_params);
                return;
            }
            _ => {}
        }

        ui.label(
            RichText::new(l10n::text(ui.ctx(), settings.chart_type.name()))
                .size(11.0)
                .strong(),
        );
        ui.space_sm();
        FormGrid::new("symbol_style_grid").show(ui, |ui| {
            if !settings.chart_type.uses_ohlc() {
                let label = ui.label(l10n::text(ui.ctx(), "Price source"));
                ui.label_inputs_by(label.id, |ui| {
                    let source = if settings.chart_type == ChartType::Columns {
                        &mut settings.chart_type_params.styles.columns.price_source
                    } else {
                        &mut settings.price_source
                    };
                    Self::price_source_dropdown(ui, source);
                });
                ui.end_row();
            }
            Self::parameters(ui, settings);
            let styles = &mut settings.chart_type_params.styles;
            match settings.chart_type {
                ChartType::Line => Self::line_style(ui, &mut styles.line, "line_style"),
                ChartType::StepLine => Self::line_style(ui, &mut styles.step_line, "line_style"),
                ChartType::LineWithMarkers => {
                    Self::line_style(ui, &mut styles.line_with_markers, "line_style");
                    Self::marker_radius_row(ui, &mut styles.marker_radius);
                }
                ChartType::Area => {
                    Self::line_style(ui, &mut styles.area.line, "line_style");
                    Self::fill_row(ui, "Top fill", &mut styles.area.top_fill);
                    Self::fill_row(ui, "Bottom fill", &mut styles.area.bottom_fill);
                    Self::marker_radius_row(ui, &mut styles.area.marker_radius);
                }
                ChartType::HlcArea => {
                    let style = &mut styles.hlc_area;
                    Self::hlc_line_row(
                        ui,
                        "High line",
                        Some(&mut style.high_line_visible),
                        &mut style.high_line,
                    );
                    Self::hlc_line_row(
                        ui,
                        "Low line",
                        Some(&mut style.low_line_visible),
                        &mut style.low_line,
                    );
                    Self::hlc_line_row(ui, "Close line", None, &mut style.close_line);
                    for (label, visible, color) in [
                        (
                            "High to close fill",
                            &mut style.high_to_close_fill_visible,
                            &mut style.high_to_close_fill,
                        ),
                        (
                            "Close to low fill",
                            &mut style.close_to_low_fill_visible,
                            &mut style.close_to_low_fill,
                        ),
                    ] {
                        ui.checkbox(visible, l10n::text(ui.ctx(), label));
                        ui.add_enabled_ui(*visible, |ui| Self::color_picker(ui, color));
                        ui.end_row();
                    }
                }
                ChartType::Baseline => {
                    ui.label(l10n::text(ui.ctx(), "Above base"));
                    ui.end_row();
                    Self::line_style(ui, &mut styles.baseline.above, "above_style");
                    Self::fill_row(ui, "Above fill", &mut styles.baseline.above_fill);
                    ui.label(l10n::text(ui.ctx(), "Below base"));
                    ui.end_row();
                    Self::line_style(ui, &mut styles.baseline.below, "below_style");
                    Self::fill_row(ui, "Below fill", &mut styles.baseline.below_fill);
                    Self::marker_radius_row(ui, &mut styles.baseline.marker_radius);
                }
                ChartType::HighLow => Self::high_low(ui, &mut styles.high_low),
                ChartType::Kagi => {
                    for (name, width) in [
                        ("Thick line width", &mut styles.kagi.thick_width),
                        ("Thin line width", &mut styles.kagi.thin_width),
                    ] {
                        let label = ui.label(l10n::text(ui.ctx(), name));
                        ui.add(
                            egui::DragValue::new(width)
                                .range(0.0..=10.0)
                                .speed(0.1)
                                .suffix(l10n::text(ui.ctx(), " pt")),
                        )
                        .labelled_by(label.id);
                        ui.end_row();
                    }
                    Self::candle_colors_section(ui, settings);
                }
                _ => Self::candle_colors_section(ui, settings),
            }
        });
    }

    fn section(ui: &mut Ui, name: &str, open: bool, content: impl FnOnce(&mut Ui)) {
        egui::CollapsingHeader::new(l10n::text(ui.ctx(), name))
            .default_open(open)
            .show(ui, |ui| {
                FormGrid::new(name).show(ui, content);
            });
    }

    fn switch(ui: &mut Ui, name: &str, value: &mut bool) {
        let label = ui.label(l10n::text(ui.ctx(), name));
        ui.checkbox(value, "").labelled_by(label.id);
        ui.end_row();
    }

    fn optional_number(
        ui: &mut Ui,
        name: &str,
        value: &mut Option<f64>,
        default: f64,
        range: std::ops::RangeInclusive<f64>,
        suffix: &str,
    ) {
        let mut enabled = value.is_some();
        let label = ui.checkbox(&mut enabled, l10n::text(ui.ctx(), name));
        if enabled != value.is_some() {
            *value = enabled.then_some(default);
        }
        if let Some(value) = value {
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(0.1)
                    .suffix(suffix),
            )
            .labelled_by(label.id);
        } else {
            ui.label("");
        }
        ui.end_row();
    }

    fn mode_name(mode: &FootprintMode) -> String {
        match mode {
            FootprintMode::BidAsk => "Bid × ask",
            FootprintMode::Volume => "Volume",
            FootprintMode::Delta => "Delta",
            FootprintMode::DeltaVolume => "Delta / volume",
            FootprintMode::DominantSide => "Dominant side",
            FootprintMode::BidAskTrades => "Sell × buy trades",
            FootprintMode::Trades => "Trades",
            FootprintMode::AverageSize => "Average size",
            FootprintMode::Ladder => "Sell and buy around the candle",
        }
        .to_owned()
    }

    /// `candles` are the chart's up and down colours, which unset buy and sell colours follow.
    fn footprint(ui: &mut Ui, params: &mut ChartTypeParams, [up, down]: [Color32; 2]) {
        ui.strong(l10n::text(ui.ctx(), "Footprint"));
        Self::section(ui, "Display", true, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Columns"));
            let mut count = params.footprint_modes.len().clamp(1, 4);
            if ui
                .add(egui::DragValue::new(&mut count).range(1..=4))
                .labelled_by(label.id)
                .changed()
            {
                params.footprint_modes.resize(count, FootprintMode::Volume);
            }
            ui.end_row();
            for (i, mode) in params.footprint_modes.iter_mut().take(4).enumerate() {
                let label = ui.label(format!("{} {}", l10n::text(ui.ctx(), "Column"), i + 1));
                ui.label_inputs_by(label.id, |ui| {
                    ui.combo_select(
                        ("footprint_column", i),
                        mode,
                        [
                            FootprintMode::Ladder,
                            FootprintMode::BidAsk,
                            FootprintMode::Volume,
                            FootprintMode::Delta,
                            FootprintMode::DeltaVolume,
                            FootprintMode::DominantSide,
                            FootprintMode::BidAskTrades,
                            FootprintMode::Trades,
                            FootprintMode::AverageSize,
                        ],
                        Self::mode_name,
                    );
                });
                ui.end_row();
            }
            Self::switch(ui, "Profile in bar", &mut params.footprint_profile);
            Self::switch(ui, "Summary below bars", &mut params.footprint_info_box);
            let label = ui.label(l10n::text(ui.ctx(), "Colouring"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "footprint_colour",
                    &mut params.footprint_color_by,
                    [
                        FootprintColor::VolumeSteps,
                        FootprintColor::Volume,
                        FootprintColor::Delta,
                        FootprintColor::Imbalance,
                        FootprintColor::Fixed,
                        FootprintColor::DeltaOpacity,
                    ],
                    |color| {
                        match color {
                            FootprintColor::VolumeSteps => "Volume steps",
                            FootprintColor::Volume => "Volume",
                            FootprintColor::Delta => "Delta",
                            FootprintColor::Imbalance => "Imbalance",
                            FootprintColor::Fixed => "Fixed",
                            FootprintColor::DeltaOpacity => "Delta opacity",
                        }
                        .to_owned()
                    },
                );
            });
            ui.end_row();
            let text = ui.active_theme().chart.axis_text;
            for (i, (name, default)) in [
                ("Buy colour", up),
                ("Sell colour", down),
                ("Text colour", text),
                ("Neutral colour", text),
            ]
            .into_iter()
            .enumerate()
            {
                let label = ui.label(l10n::text(ui.ctx(), name));
                ui.horizontal(|ui| {
                    let color = &mut params.footprint_colors[i];
                    let mut selected = color.unwrap_or(default);
                    if Self::color_picker(ui, &mut selected) {
                        *color = Some(selected);
                    }
                    if ui
                        .small_button(l10n::text(ui.ctx(), "Auto"))
                        .labelled_by(label.id)
                        .clicked()
                    {
                        *color = None;
                    }
                });
                ui.end_row();
            }
        });
        Self::section(ui, "Rows", true, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Grouping"));
            let mut automatic = params.footprint_row_ticks == 0;
            let previous = automatic;
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select("footprint_rows", &mut automatic, [true, false], |auto| {
                    if *auto {
                        "Automatic by zoom"
                    } else {
                        "Fixed ticks"
                    }
                    .to_owned()
                });
            });
            if automatic != previous {
                params.footprint_row_ticks = if automatic { 0 } else { 1 };
            }
            ui.end_row();
            if !automatic {
                let label = ui.label(l10n::text(ui.ctx(), "Ticks per row"));
                ui.add(egui::DragValue::new(&mut params.footprint_row_ticks).range(1..=1_000_000))
                    .labelled_by(label.id);
                ui.end_row();
            }
        });
        Self::section(ui, "Levels", true, |ui| {
            Self::switch(ui, "Point of control", &mut params.footprint_poc);
            Self::value_area(ui, &mut params.footprint_value_area);
        });
        Self::section(ui, "Imbalance", true, |ui| {
            Self::switch(ui, "Edge strips", &mut params.footprint_imbalance);
            let label = ui.label(l10n::text(ui.ctx(), "Ratio"));
            ui.add(
                egui::DragValue::new(&mut params.footprint_imbalance_ratio)
                    .range(1.0..=1000.0)
                    .speed(0.1)
                    .suffix(":1"),
            )
            .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(l10n::text(ui.ctx(), "Comparison"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "footprint_comparison",
                    &mut params.footprint_diagonal,
                    [true, false],
                    |d| if *d { "Diagonal" } else { "Same price" }.to_owned(),
                );
            });
            ui.end_row();
            let label = ui.label(l10n::text(ui.ctx(), "Minimum volume"));
            ui.add(
                egui::DragValue::new(&mut params.footprint_imbalance_min_volume)
                    .range(0.0..=f64::MAX)
                    .speed(0.1),
            )
            .labelled_by(label.id);
            ui.end_row();
        });
        Self::section(ui, "Annotations", false, |ui| {
            for (name, value) in [
                (
                    "Stacked imbalances",
                    &mut params.footprint_stacked_imbalances,
                ),
                ("Unrevisited POC", &mut params.footprint_naked_poc),
                ("Delta POC", &mut params.footprint_delta_poc),
                (
                    "Unfinished auctions",
                    &mut params.footprint_unfinished_auctions,
                ),
                (
                    "Extreme ask/bid ratios",
                    &mut params.footprint_extreme_ratio,
                ),
                ("Pullback", &mut params.footprint_pullback),
            ] {
                Self::switch(ui, name, value);
            }
            if params.footprint_stacked_imbalances {
                let label = ui.label(l10n::text(ui.ctx(), "Adjacent imbalances"));
                ui.add(egui::DragValue::new(&mut params.footprint_stack).range(1..=1000))
                    .labelled_by(label.id);
                ui.end_row();
            }
            let mut cluster = params.footprint_cluster_share.map(|share| share * 100.0);
            Self::optional_number(ui, "Volume clusters", &mut cluster, 25.0, 0.0..=100.0, "%");
            if cluster != params.footprint_cluster_share.map(|share| share * 100.0) {
                params.footprint_cluster_share = cluster.map(|percent| percent / 100.0);
            }
            Self::optional_number(
                ui,
                "Single prints: maximum volume",
                &mut params.footprint_single_print_max,
                1.0,
                0.0..=f64::MAX,
                "",
            );
            Self::optional_number(
                ui,
                "Delta markers",
                &mut params.footprint_delta_percent,
                50.0,
                0.0..=100.0,
                "%",
            );
            Self::optional_number(
                ui,
                "Bid/ask ratio markers",
                &mut params.footprint_ratio,
                3.0,
                1.0..=1000.0,
                ":1",
            );
        });
        Self::section(ui, "Numbers", true, |ui| {
            Self::switch(ui, "Abbreviation", &mut params.footprint_abbreviate);
            Self::switch(ui, "Zero values", &mut params.footprint_show_zero);
            for (i, name) in ["Minimum font size", "Maximum font size"]
                .into_iter()
                .enumerate()
            {
                let label = ui.label(l10n::text(ui.ctx(), name));
                ui.add(
                    egui::DragValue::new(&mut params.footprint_font_size[i])
                        .range(8.0..=24.0)
                        .suffix(l10n::text(ui.ctx(), " pt")),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
        });
        Self::section(ui, "Statistics", true, |ui| {
            Self::switch(ui, "Bar statistics", &mut params.footprint_statistics);
            if params.footprint_statistics {
                for row in FootprintStatistic::ALL {
                    let mut enabled = params.footprint_statistics_rows.contains(&row);
                    let name = if row == FootprintStatistic::Volume {
                        "Volume"
                    } else {
                        row.label()
                    };
                    let was_enabled = enabled;
                    Self::switch(ui, name, &mut enabled);
                    if enabled != was_enabled {
                        let rows = &params.footprint_statistics_rows;
                        params.footprint_statistics_rows = FootprintStatistic::ALL
                            .into_iter()
                            .filter(|r| if *r == row { enabled } else { rows.contains(r) })
                            .collect();
                    }
                }
            }
        });
    }

    fn value_area(ui: &mut Ui, share: &mut Option<f64>) {
        let mut percent = share.map(|share| share * 100.0);
        Self::optional_number(ui, "Value area", &mut percent, 70.0, 1.0..=100.0, "%");
        if percent != share.map(|share| share * 100.0) {
            *share = percent.map(|percent| percent / 100.0);
        }
    }

    fn profile(ui: &mut Ui, params: &mut ChartTypeParams) {
        ui.strong(l10n::text(ui.ctx(), "Profile"));
        FormGrid::new("execution_profile_settings").show(ui, |ui| {
            let profile = &mut params.profile;
            let label = ui.label(l10n::text(ui.ctx(), "Display mode"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "profile_mode",
                    &mut profile.mode,
                    [
                        FootprintMode::BidAsk,
                        FootprintMode::Volume,
                        FootprintMode::Delta,
                    ],
                    Self::mode_name,
                );
            });
            ui.end_row();
            Self::profile_session(ui, &mut profile.session);
            Self::profile_rows(ui, &mut profile.rows);
            Self::value_area(ui, &mut profile.value_area);
            Self::switch(ui, "Value area guides", &mut profile.value_area_guides);
            Self::switch(ui, "Developing POC", &mut profile.developing_poc);
            Self::switch(
                ui,
                "Developing value area",
                &mut profile.developing_value_area,
            );
            Self::switch(ui, "Extend prior POC", &mut profile.extend_poc);
            Self::switch(ui, "Extend prior VAH", &mut profile.extend_vah);
            Self::switch(ui, "Extend prior VAL", &mut profile.extend_val);
            let mut nodes = profile.nodes.is_some();
            Self::switch(ui, "Volume nodes", &mut nodes);
            if nodes != profile.nodes.is_some() {
                profile.nodes = nodes.then(Default::default);
            }
            if let Some(nodes) = &mut profile.nodes {
                let label = ui.label(l10n::text(ui.ctx(), "Node smoothing"));
                ui.add(egui::DragValue::new(&mut nodes.smoothing).range(0..=1000))
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Node prominence"));
                ui.add(
                    egui::DragValue::new(&mut nodes.prominence)
                        .range(0.0..=1.0)
                        .speed(0.01),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
        });
    }

    /// Session hours: the chart's calendar, the instrument's regular or extended windows, or a
    /// daily window of its own.
    fn profile_session(ui: &mut Ui, session: &mut ProfileSession) {
        let label = ui.label(l10n::text(ui.ctx(), "Session"));
        let custom = || {
            let at = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
            ProfileSession::Custom(crate::model::CustomProfileSession {
                start: at(0),
                end: at(0),
                zone: None,
            })
        };
        let choice = |session: &ProfileSession| match session {
            ProfileSession::Chart => 0,
            ProfileSession::Regular => 1,
            ProfileSession::Extended => 2,
            ProfileSession::Custom(_) => 3,
        };
        let mut selected = choice(session);
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select("profile_session", &mut selected, 0..4, |i| {
                ["Chart hours", "Regular hours", "Extended hours", "Custom"][*i].to_owned()
            });
        });
        ui.end_row();
        if selected != choice(session) {
            *session = [
                ProfileSession::Chart,
                ProfileSession::Regular,
                ProfileSession::Extended,
                custom(),
            ][selected]
                .clone();
        }
        let ProfileSession::Custom(custom) = session else {
            return;
        };
        for (name, time) in [
            ("Session start", &mut custom.start),
            ("Session end", &mut custom.end),
        ] {
            let label = ui.label(l10n::text(ui.ctx(), name));
            ui.horizontal(|ui| {
                let (mut hour, mut minute) = (time.hour(), time.minute());
                ui.add(egui::DragValue::new(&mut hour).range(0..=23))
                    .labelled_by(label.id);
                ui.label(":");
                ui.add(egui::DragValue::new(&mut minute).range(0..=59))
                    .labelled_by(label.id);
                *time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or(*time);
            });
            ui.end_row();
        }
        let label = ui.label(l10n::text(ui.ctx(), "Session zone"));
        let zones: &[&str] = if cfg!(feature = "timezones") {
            &[
                "UTC",
                "America/New_York",
                "America/Chicago",
                "America/Los_Angeles",
                "Europe/London",
                "Europe/Paris",
                "Asia/Tokyo",
                "Asia/Hong_Kong",
                "Asia/Singapore",
                "Australia/Sydney",
            ]
        } else {
            &[
                "UTC", "-08:00", "-06:00", "-05:00", "+01:00", "+08:00", "+09:00", "+10:00",
            ]
        };
        ui.label_inputs_by(label.id, |ui| {
            let mut options: Vec<_> = std::iter::once(None)
                .chain(
                    zones
                        .iter()
                        .filter_map(|zone| TimezoneId::named(zone))
                        .map(Some),
                )
                .collect();
            // A saved zone outside the list stays selectable.
            if !options.contains(&custom.zone) {
                options.push(custom.zone.clone());
            }
            ui.combo_select("profile_zone", &mut custom.zone, options, |zone| {
                zone.as_ref()
                    .map_or("Exchange", |zone| zone.name())
                    .to_owned()
            });
        });
        ui.end_row();
    }

    /// Row grouping: the feed's rows, a row count, or ticks per row.
    fn profile_rows(ui: &mut Ui, rows: &mut ProfileRows) {
        let label = ui.label(l10n::text(ui.ctx(), "Rows layout"));
        let choice = |rows: &ProfileRows| match rows {
            ProfileRows::Feed => 0,
            ProfileRows::Count(_) => 1,
            ProfileRows::Ticks(_) => 2,
        };
        let mut selected = choice(rows);
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select("profile_rows", &mut selected, 0..3, |i| {
                ["Feed rows", "Number of rows", "Ticks per row"][*i].to_owned()
            });
        });
        ui.end_row();
        if selected != choice(rows) {
            *rows = [
                ProfileRows::Feed,
                ProfileRows::Count(24),
                ProfileRows::Ticks(1),
            ][selected];
        }
        let (name, size, range) = match rows {
            ProfileRows::Feed => return,
            ProfileRows::Count(count) => ("Row count", count, 1..=1000),
            ProfileRows::Ticks(ticks) => ("Row size", ticks, 1..=1_000_000),
        };
        let label = ui.label(l10n::text(ui.ctx(), name));
        ui.add(egui::DragValue::new(size).range(range))
            .labelled_by(label.id);
        ui.end_row();
    }

    fn marker_radius_row(ui: &mut Ui, radius: &mut f32) {
        let label = ui.label(l10n::text(ui.ctx(), "Marker radius"));
        ui.add(
            egui::DragValue::new(radius)
                .range(0.0..=20.0)
                .suffix(l10n::text(ui.ctx(), " pt")),
        )
        .labelled_by(label.id);
        ui.end_row();
    }

    fn parameters(ui: &mut Ui, settings: &mut SeriesSettings) {
        let params = &mut settings.chart_type_params;
        match settings.chart_type {
            ChartType::Renko => {
                Self::box_size(
                    ui,
                    &mut params.renko_size_method,
                    &mut params.renko_brick_size,
                );
                Self::transform_source(ui, &mut params.renko_source);
                let label = ui.label(l10n::text(ui.ctx(), "Wicks"));
                ui.checkbox(&mut params.renko_wicks, "")
                    .labelled_by(label.id);
                ui.end_row();
            }
            ChartType::Kagi => {
                Self::box_size(ui, &mut params.kagi_size_method, &mut params.kagi_reversal);
                Self::transform_source(ui, &mut params.kagi_source);
            }
            ChartType::Range => {
                Self::box_size(ui, &mut params.range_size_method, &mut params.range_size)
            }
            ChartType::PointAndFigure => {
                Self::box_size(ui, &mut params.pnf_size_method, &mut params.pnf_box_size);
                Self::transform_source(ui, &mut params.pnf_source);
                let label = ui.label(l10n::text(ui.ctx(), "Reversal boxes"));
                ui.add(egui::DragValue::new(&mut params.pnf_reversal).range(1..=100))
                    .labelled_by(label.id);
                ui.end_row();
            }
            ChartType::LineBreak => {
                let label = ui.label(l10n::text(ui.ctx(), "Number of lines"));
                ui.add(egui::DragValue::new(&mut params.line_break_count).range(1..=10))
                    .labelled_by(label.id);
                ui.end_row();
                Self::transform_source(ui, &mut params.line_break_source);
            }
            ChartType::TimePriceOpportunity => {
                let label = ui.label(l10n::text(ui.ctx(), "Period"));
                ui.add(
                    egui::DragValue::new(&mut params.tpo_period_minutes)
                        .range(1..=1440)
                        .suffix(l10n::text(ui.ctx(), " min")),
                )
                .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Initial balance"));
                ui.add(
                    egui::DragValue::new(&mut params.tpo_initial_balance_minutes)
                        .range(1..=1440)
                        .suffix(l10n::text(ui.ctx(), " min")),
                )
                .labelled_by(label.id);
                ui.end_row();
                for (name, enabled) in [
                    ("Poor highs and lows", &mut params.tpo_poor_highs_lows),
                    (
                        "Initial balance extensions",
                        &mut params.tpo_initial_balance_extensions,
                    ),
                ] {
                    let label = ui.label(l10n::text(ui.ctx(), name));
                    ui.checkbox(enabled, "").labelled_by(label.id);
                    ui.end_row();
                }
                let label = ui.label(l10n::text(ui.ctx(), "Ticks per row"));
                ui.add(
                    egui::DragValue::new(&mut params.tpo_row_ticks)
                        .range(0..=100_000)
                        .custom_formatter(|ticks, _| {
                            if ticks == 0.0 {
                                "Auto".to_owned()
                            } else {
                                ticks.to_string()
                            }
                        }),
                )
                .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Value area"));
                ui.add(
                    egui::DragValue::new(&mut params.tpo_value_area)
                        .range(1.0..=100.0)
                        .suffix("%"),
                )
                .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Split extended hours"));
                ui.checkbox(&mut params.tpo_split_sessions, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Single prints"));
                ui.checkbox(&mut params.tpo_single_prints, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "TPO Midpoint"));
                ui.checkbox(&mut params.tpo_midpoint, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Letters"));
                ui.checkbox(&mut params.tpo_letters, "")
                    .labelled_by(label.id);
                ui.end_row();
            }
            ChartType::Baseline => {
                let label = ui.label(l10n::text(ui.ctx(), "Base level"));
                let mut price = matches!(params.baseline, BaselineLevel::Price(_));
                let was_price = price;
                ui.label_inputs_by(label.id, |ui| {
                    ui.combo_select("base_level", &mut price, [false, true], |p| {
                        if *p { "Price" } else { "Pane %" }.to_owned()
                    });
                });
                if price != was_price {
                    params.baseline = if price {
                        BaselineLevel::Price(0.0)
                    } else {
                        BaselineLevel::PercentOfPane(50.0)
                    };
                }
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Base value"));
                ui.label_inputs_by(label.id, |ui| match &mut params.baseline {
                    BaselineLevel::Price(value) => {
                        ui.add(egui::DragValue::new(value).speed(0.1));
                    }
                    BaselineLevel::PercentOfPane(value) => {
                        ui.add(egui::DragValue::new(value).range(0.0..=100.0).suffix("%"));
                    }
                });
                ui.end_row();
            }
            _ => {}
        }
    }

    fn transform_source(ui: &mut Ui, source: &mut PriceBasedSource) {
        let label = ui.label(l10n::text(ui.ctx(), "Price source"));
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "transform_source",
                source,
                [PriceBasedSource::Close, PriceBasedSource::HighLow],
                |s| match s {
                    PriceBasedSource::Close => "Close".to_owned(),
                    PriceBasedSource::HighLow => "High / low".to_owned(),
                },
            );
        });
        ui.end_row();
    }

    fn box_size(ui: &mut Ui, method: &mut BoxSizeMethod, size: &mut f64) {
        let mut choice = match method {
            BoxSizeMethod::Fixed => 0,
            BoxSizeMethod::Atr { .. } => 1,
            BoxSizeMethod::Percentage(_) => 2,
        };
        let previous = choice;
        let label = ui.label(l10n::text(ui.ctx(), "Size method"));
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select("size_method", &mut choice, [0, 1, 2], |c| {
                match c {
                    0 => "Fixed",
                    1 => "Average true range",
                    _ => "Percentage",
                }
                .to_owned()
            });
        });
        if choice != previous {
            *method = match choice {
                0 => BoxSizeMethod::Fixed,
                1 => BoxSizeMethod::Atr {
                    period: 14,
                    multiplier: 1.0,
                },
                _ => BoxSizeMethod::Percentage(1.0),
            };
        }
        ui.end_row();
        match method {
            BoxSizeMethod::Fixed => {
                let label = ui.label(l10n::text(ui.ctx(), "Size"));
                ui.add(
                    egui::DragValue::new(size)
                        .range(f64::MIN_POSITIVE..=f64::MAX)
                        .speed(0.1),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
            BoxSizeMethod::Atr { period, multiplier } => {
                let label = ui.label(l10n::text(ui.ctx(), "ATR period"));
                ui.add(egui::DragValue::new(period).range(1..=10000))
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "ATR multiplier"));
                ui.add(
                    egui::DragValue::new(multiplier)
                        .range(0.01..=1000.0)
                        .speed(0.1),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
            BoxSizeMethod::Percentage(percent) => {
                let label = ui.label(l10n::text(ui.ctx(), "Percentage"));
                ui.add(
                    egui::DragValue::new(percent)
                        .range(0.001..=100.0)
                        .speed(0.1)
                        .suffix("%"),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
        }
    }

    fn price_source_dropdown(ui: &mut Ui, source: &mut PriceSource) {
        ui.combo_select("price_source", source, PriceSource::all(), |v| {
            v.label().to_string()
        });
    }

    fn high_low(ui: &mut Ui, style: &mut HighLowStyle) {
        Self::fill_row(ui, "Bar colour", &mut style.bar_color);
        Self::switch(ui, "Price labels", &mut style.labels_visible);
        Self::fill_row(ui, "Label colour", &mut style.label_color);
        let label = ui.label(l10n::text(ui.ctx(), "Label size"));
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "high_low_label_size",
                &mut style.label_font_size,
                [7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 20.0, 24.0],
                |size| size.to_string(),
            );
        });
        ui.end_row();
    }

    fn line_style(ui: &mut Ui, style: &mut LinePlotStyle, id: &str) {
        Self::fill_row(ui, "Line colour", &mut style.color);
        let label = ui.label(l10n::text(ui.ctx(), "Line width"));
        ui.add(
            egui::DragValue::new(&mut style.width)
                .range(0.0..=10.0)
                .speed(0.1)
                .suffix(l10n::text(ui.ctx(), " pt")),
        )
        .labelled_by(label.id);
        ui.end_row();
        let label = ui.label(l10n::text(ui.ctx(), "Line style"));
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                id,
                &mut style.style,
                LineStyle::all().iter().copied(),
                |s| s.name().to_owned(),
            );
        });
        ui.end_row();
    }

    /// A line of the HLC area on one row, so that the three lines and two fills fit the dialog.
    fn hlc_line_row(
        ui: &mut Ui,
        label: &str,
        visible: Option<&mut bool>,
        style: &mut LinePlotStyle,
    ) {
        let text = l10n::text(ui.ctx(), label);
        let enabled = visible.as_deref().is_none_or(|visible| *visible);
        let label_id = match visible {
            Some(visible) => ui.checkbox(visible, text).id,
            None => ui.label(text).id,
        };
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label_inputs_by(label_id, |ui| {
                    Self::color_picker(ui, &mut style.color);
                    ui.add(
                        egui::DragValue::new(&mut style.width)
                            .range(0.0..=10.0)
                            .speed(0.1)
                            .suffix(l10n::text(ui.ctx(), " pt")),
                    );
                    ui.combo_select(
                        label,
                        &mut style.style,
                        LineStyle::all().iter().copied(),
                        |s| s.name().to_owned(),
                    );
                });
            });
        });
        ui.end_row();
    }

    fn fill_row(ui: &mut Ui, label: &str, color: &mut Color32) {
        ui.label(l10n::text(ui.ctx(), label));
        Self::color_picker(ui, color);
        ui.end_row();
    }

    fn candle_colors_section(ui: &mut Ui, settings: &mut SeriesSettings) {
        let columns = settings.chart_type == ChartType::Columns;
        ui.label(l10n::text(
            ui.ctx(),
            if columns {
                "Up and down colors"
            } else {
                "Body"
            },
        ));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            Self::color_picker(ui, &mut settings.bullish_color);
            Self::color_picker(ui, &mut settings.bearish_color);
        });
        ui.end_row();

        // A column is one filled rectangle, so it has no border or wick.
        if settings.chart_type.is_time_independent() || columns {
            return;
        }

        for (label, up, down) in [
            (
                "Border",
                &mut settings.bullish_border_color,
                &mut settings.bearish_border_color,
            ),
            (
                "Wick",
                &mut settings.bullish_wick_color,
                &mut settings.bearish_wick_color,
            ),
        ] {
            ui.label(l10n::text(ui.ctx(), label));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                Self::optional_color_picker(ui, up, settings.bullish_color);
                Self::optional_color_picker(ui, down, settings.bearish_color);
                for (color, default, label) in [
                    (up, settings.bullish_color, "Custom up"),
                    (down, settings.bearish_color, "Custom down"),
                ] {
                    let mut custom = color.is_some();
                    if ui
                        .checkbox(&mut custom, l10n::text(ui.ctx(), label))
                        .changed()
                    {
                        *color = custom.then_some(color.unwrap_or(default));
                    }
                }
            });
            ui.end_row();
        }
    }

    pub(super) fn color_picker(ui: &mut Ui, color: &mut Color32) -> bool {
        let id = ui.next_auto_id();
        let mut state = ui
            .data(|data| data.get_temp::<ColorPickerState>(id))
            .unwrap_or_else(|| ColorPickerState::new(*color));
        if !state.is_open {
            state.sel_color = *color;
        }
        let picked = ColorPicker::new(&mut state, id).show(ui).inner;
        if let Some(selected) = picked {
            *color = selected;
        }
        ui.data_mut(|data| data.insert_temp(id, state));
        picked.is_some()
    }

    fn optional_color_picker(ui: &mut Ui, color: &mut Option<Color32>, default: Color32) {
        let mut selected = color.unwrap_or(default);
        ui.add_enabled_ui(color.is_some(), |ui| Self::color_picker(ui, &mut selected));
        if color.is_some() {
            *color = Some(selected);
        }
    }
}
