//! Symbol tab - chart type, price source, and line/candle style settings.

use crate::chart::overlays::SessionColors;
use crate::chart::series::SeriesSettings;
use crate::localization as l10n;
use crate::model::{
    BaselineLevel, BoxSizeMethod, ChartType, ChartTypeParams, FootprintColor, FootprintIntensity,
    FootprintMode, FootprintStatistic, HighLowStyle, LinePlotStyle, LineStyle, PriceBasedSource,
    PriceSource, ProfileRows, ProfileSession, TPOProfilePeriod, TimezoneId, TradeProfileSettings,
};
use crate::theme::ThemeExt;
use crate::ui_kit::color_picker::keep_opacity;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid};
use chrono::{NaiveTime, Timelike};
use egui::{Color32, RichText, Ui};

/// The row that chooses between the last real price and the newest element's close on the
/// price scale of a price-based chart type.
const REAL_PRICE: &str = "Real price";

/// Symbol tab renderer - shows chart type and style settings
#[derive(Debug)]
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
            let tpo = settings.chart_type == ChartType::TimePriceOpportunity;
            if !settings.chart_type.uses_ohlc() || tpo {
                let label = ui.label(l10n::text(ui.ctx(), "Price source"));
                ui.label_inputs_by(label.id, |ui| {
                    let styles = &mut settings.chart_type_params.styles;
                    let source = match settings.chart_type {
                        ChartType::Columns => &mut styles.columns.price_source,
                        ChartType::TimePriceOpportunity => &mut styles.tpo_price_source,
                        _ => &mut settings.price_source,
                    };
                    Self::price_source_dropdown(ui, source);
                });
                ui.end_row();
            }
            if tpo {
                let line = &mut settings.chart_type_params.styles.tpo_line;
                ui.label(l10n::text(ui.ctx(), "Line colour"));
                // The theme's default shows as the colour it draws, until one is picked.
                let mut shown = if line.color == Color32::PLACEHOLDER {
                    ui.active_theme().ui.accent
                } else {
                    line.color
                };
                if Self::color_picker(ui, &mut shown) {
                    line.color = shown;
                }
                ui.end_row();
                Self::line_width_and_style(ui, line, "tpo_line_style");
            }
            Self::parameters(ui, settings);
            let styles = &mut settings.chart_type_params.styles;
            match settings.chart_type {
                ChartType::Line => {
                    let label = ui.label(l10n::text(ui.ctx(), "Colour type"));
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select(
                            "line_color_type",
                            &mut styles.line_gradient,
                            [false, true],
                            |gradient| if *gradient { "Gradient" } else { "Solid" }.to_owned(),
                        );
                    });
                    ui.end_row();
                    if styles.line_gradient {
                        Self::fill_row(ui, "Start colour", &mut styles.line_gradient_start);
                        Self::fill_row(ui, "End colour", &mut styles.line_gradient_end);
                    } else {
                        Self::fill_row(ui, "Line colour", &mut styles.line.color);
                    }
                    Self::line_width_and_style(ui, &mut styles.line, "line_style");
                }
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
                    Self::fill_at_base_row(
                        ui,
                        "Above fill at base",
                        &mut styles.baseline.above_fill_at_base,
                        styles.baseline.above_fill,
                    );
                    ui.label(l10n::text(ui.ctx(), "Below base"));
                    ui.end_row();
                    Self::line_style(ui, &mut styles.baseline.below, "below_style");
                    Self::fill_row(ui, "Below fill", &mut styles.baseline.below_fill);
                    Self::fill_at_base_row(
                        ui,
                        "Below fill at base",
                        &mut styles.baseline.below_fill_at_base,
                        styles.baseline.below_fill,
                    );
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
                    let candles = [settings.bullish_color, settings.bearish_color];
                    Self::own_color_rows(
                        ui,
                        [
                            ("Thick line colour", &mut styles.kagi.up_color),
                            ("Thin line colour", &mut styles.kagi.down_color),
                        ],
                        candles,
                    );
                }
                ChartType::PointAndFigure => {
                    let candles = [settings.bullish_color, settings.bearish_color];
                    let style = &mut styles.point_figure;
                    Self::own_color_rows(
                        ui,
                        [
                            ("Rising column colour", &mut style.up_color),
                            ("Falling column colour", &mut style.down_color),
                        ],
                        candles,
                    );
                }
                ChartType::Columns => {
                    let candles = [settings.bullish_color, settings.bearish_color];
                    let style = &mut styles.columns;
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.checkbox(
                            &mut style.color_by_previous_close,
                            l10n::text(ui.ctx(), "Color bars based on previous close"),
                        );
                    });
                    ui.end_row();
                    Self::own_color_rows(
                        ui,
                        [
                            ("Rising column colour", &mut style.up_color),
                            ("Falling column colour", &mut style.down_color),
                        ],
                        candles,
                    );
                }
                ChartType::Renko => {
                    Self::candle_colors_section(ui, settings);
                    let body = [settings.bullish_color, settings.bearish_color];
                    let style = &mut settings.chart_type_params.styles.renko;
                    Self::switch(ui, "Borders", &mut style.border_visible);
                    Self::custom_colors_row(
                        ui,
                        "Border",
                        [&mut style.border_up, &mut style.border_down],
                        body.map(crate::chart::rendering::candles::brick_border),
                    );
                    Self::custom_colors_row(
                        ui,
                        "Wick",
                        [&mut style.wick_up, &mut style.wick_down],
                        body,
                    );
                }
                ChartType::LineBreak => {
                    Self::candle_colors_section(ui, settings);
                    let body = [settings.bullish_color, settings.bearish_color];
                    let style = &mut settings.chart_type_params.styles.line_break;
                    Self::switch(ui, "Borders", &mut style.border_visible);
                    Self::custom_colors_row(
                        ui,
                        "Border",
                        [&mut style.border_up, &mut style.border_down],
                        body.map(crate::chart::rendering::candles::brick_border),
                    );
                }
                ChartType::Range => {
                    let candles = [settings.bullish_color, settings.bearish_color];
                    let style = &mut styles.range;
                    if Self::switch(ui, "Thin bars", &mut style.thin_bars) {
                        // Changing the switch ends the block look of a style saved before bold bars.
                        style.blocks = false;
                    }
                    Self::switch(ui, "Phantom bars", &mut style.phantom_bars);
                    Self::own_color_rows(
                        ui,
                        [
                            ("Rising bar colour", &mut style.up_color),
                            ("Falling bar colour", &mut style.down_color),
                        ],
                        candles,
                    );
                    // The forming bar's default shows as the completed bar colour it draws in.
                    let body = [
                        crate::model::or_candle(style.up_color, candles[0]),
                        crate::model::or_candle(style.down_color, candles[1]),
                    ];
                    ui.label(l10n::text(ui.ctx(), "Forming bar"));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for (color, body) in [&mut style.forming_up, &mut style.forming_down]
                            .into_iter()
                            .zip(body)
                        {
                            // A style saved before shows the body colour it draws in until picked.
                            let mut shown = crate::model::or_candle(*color, body);
                            if Self::color_picker(ui, &mut shown) {
                                *color = shown;
                            }
                        }
                    });
                    ui.end_row();
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

    fn switch(ui: &mut Ui, name: &str, value: &mut bool) -> bool {
        let label = ui.label(l10n::text(ui.ctx(), name));
        let changed = ui.checkbox(value, "").labelled_by(label.id).changed();
        ui.end_row();
        changed
    }

    /// A switch whose long name wraps in the label column, usable while `enabled`.
    fn wrapped_switch(ui: &mut Ui, name: &str, enabled: bool, value: &mut bool) {
        let label = ui
            .vertical(|ui| {
                ui.set_width(160.0);
                ui.add(
                    egui::Label::new(l10n::text(ui.ctx(), name))
                        .wrap()
                        .halign(egui::Align::Min),
                )
            })
            .inner;
        ui.add_enabled(enabled, egui::Checkbox::without_text(value))
            .labelled_by(label.id);
        ui.end_row();
    }

    fn optional_number<N: egui::emath::Numeric>(
        ui: &mut Ui,
        name: &str,
        value: &mut Option<N>,
        default: N,
        range: std::ops::RangeInclusive<N>,
        suffix: &str,
    ) {
        let mut enabled = value.is_some();
        let label = ui.checkbox(&mut enabled, l10n::text(ui.ctx(), name));
        if enabled != value.is_some() {
            *value = enabled.then_some(default);
        }
        if let Some(value) = value {
            // Whole-bar limits may be zero in host settings; opening the dialog keeps them.
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .clamp_existing_to_range(!N::INTEGRAL)
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
                        FootprintColor::LargerSide,
                        FootprintColor::Side,
                    ],
                    |color| {
                        match color {
                            FootprintColor::VolumeSteps => "Volume steps",
                            FootprintColor::Volume => "Volume",
                            FootprintColor::Delta => "Delta",
                            FootprintColor::Imbalance => "Imbalance",
                            FootprintColor::Fixed => "Fixed",
                            FootprintColor::DeltaOpacity => "Delta opacity",
                            FootprintColor::LargerSide => "Larger side",
                            FootprintColor::Side => "Side colours",
                        }
                        .to_owned()
                    },
                );
            });
            ui.end_row();
            let label = ui.label(l10n::text(ui.ctx(), "Shading scale"));
            let mut scale = match params.footprint_intensity {
                FootprintIntensity::Visible => 1,
                FootprintIntensity::Volume(_) => 2,
                _ => 0,
            };
            let previous = scale;
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select("footprint_intensity", &mut scale, [0, 1, 2], |scale| {
                    match scale {
                        0 => "Each bar",
                        1 => "Bars in view",
                        _ => "Fixed volume",
                    }
                    .to_owned()
                });
            });
            if scale != previous {
                params.footprint_intensity = match scale {
                    0 => FootprintIntensity::Bar,
                    1 => FootprintIntensity::Visible,
                    _ => FootprintIntensity::Volume(100.0),
                };
            }
            ui.end_row();
            if let FootprintIntensity::Volume(volume) = &mut params.footprint_intensity {
                let label = ui.label(l10n::text(ui.ctx(), "Full shade volume"));
                ui.add(
                    egui::DragValue::new(volume)
                        .range(0.001..=f64::MAX)
                        .clamp_existing_to_range(false)
                        .speed(1.0),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
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
                Self::followed_color(
                    ui,
                    name,
                    None,
                    &mut params.footprint_colors[i],
                    default,
                    "Auto",
                );
            }
            if params.footprint_color_by == FootprintColor::VolumeSteps {
                // Unset shades show the mix painted on the theme's chart background.
                let background = ui.active_theme().chart.bg.to_opaque();
                for (side, (name, color)) in [
                    ("Buy shade", params.footprint_colors[0].unwrap_or(up)),
                    ("Sell shade", params.footprint_colors[1].unwrap_or(down)),
                ]
                .into_iter()
                .enumerate()
                {
                    for step in 0..4 {
                        Self::followed_color(
                            ui,
                            &format!("{name} {}", step + 1),
                            None,
                            &mut params.footprint_step_colors[side][step],
                            crate::chart::overlays::step_color(color, step, background),
                            "Auto",
                        );
                    }
                }
            }
        });
        Self::section(ui, "Rows", true, |ui| {
            let label = ui.label(l10n::text(ui.ctx(), "Grouping"));
            // 0 groups by zoom, 1 by a fixed tick count and 2 by the average true range.
            let mut method = match (params.footprint_row_atr, params.footprint_row_ticks) {
                (true, _) => 2,
                (false, 0) => 0,
                (false, _) => 1,
            };
            let previous = method;
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select("footprint_rows", &mut method, [0, 1, 2], |method| {
                    match method {
                        0 => "Automatic by zoom",
                        1 => "Fixed ticks",
                        _ => "Average true range",
                    }
                    .to_owned()
                });
            });
            if method != previous {
                params.footprint_row_atr = method == 2;
                match method {
                    0 => params.footprint_row_ticks = 0,
                    1 => params.footprint_row_ticks = params.footprint_row_ticks.max(1),
                    _ => {}
                }
            }
            ui.end_row();
            if method == 0 {
                let label = ui.label(l10n::text(ui.ctx(), "Automatic row height"));
                ui.add(
                    egui::DragValue::new(&mut params.footprint_auto_row_height)
                        .range(10.0..=40.0)
                        .clamp_existing_to_range(false)
                        .suffix(l10n::text(ui.ctx(), " pt")),
                )
                .labelled_by(label.id);
                ui.end_row();
            } else if method == 1 {
                let label = ui.label(l10n::text(ui.ctx(), "Ticks per row"));
                ui.add(egui::DragValue::new(&mut params.footprint_row_ticks).range(1..=1_000_000))
                    .labelled_by(label.id);
                ui.end_row();
            } else {
                let label = ui.label(l10n::text(ui.ctx(), "ATR length"));
                ui.add(egui::DragValue::new(&mut params.footprint_row_atr_length).range(1..=200))
                    .labelled_by(label.id);
                ui.end_row();
            }
            let label = ui.label(l10n::text(ui.ctx(), "Smallest row with numbers"));
            ui.add(
                egui::DragValue::new(&mut params.footprint_min_text_height)
                    .range(10.0..=40.0)
                    .clamp_existing_to_range(false)
                    .suffix(l10n::text(ui.ctx(), " pt")),
            )
            .labelled_by(label.id);
            ui.end_row();
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
            // Unset, the marks of a side take its buy or sell colour.
            let sides = [
                params.footprint_colors[0].unwrap_or(up),
                params.footprint_colors[1].unwrap_or(down),
            ];
            for (i, name) in ["Buy imbalance colour", "Sell imbalance colour"]
                .into_iter()
                .enumerate()
            {
                let color = &mut params.footprint_imbalance_colors[i];
                Self::followed_color(ui, name, None, color, sides[i], "Same as cells");
            }
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
                Self::switch(
                    ui,
                    "Zones end only when broken",
                    &mut params.footprint_zones_until_broken,
                );
            }
            if params.footprint_stacked_imbalances
                || params.footprint_naked_poc
                || params.footprint_unfinished_auctions
            {
                Self::optional_number(
                    ui,
                    "Expire after bars",
                    &mut params.footprint_extend_bars,
                    50,
                    1..=100_000,
                    "",
                );
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
                    if enabled && row == FootprintStatistic::CumulativeDelta {
                        Self::switch(
                            ui,
                            "Restart CVD each session",
                            &mut params.footprint_session_cumulative_delta,
                        );
                    }
                }
            }
        });
    }

    /// A colour row whose `reset` button returns the colour to following `default`; with
    /// `visible`, its name is a checkbox for what the colour paints.
    fn followed_color(
        ui: &mut Ui,
        name: &str,
        visible: Option<&mut bool>,
        color: &mut Option<Color32>,
        default: Color32,
        reset: &str,
    ) {
        let text = l10n::text(ui.ctx(), name);
        let label = match visible {
            Some(visible) => ui.checkbox(visible, text),
            None => ui.label(text),
        };
        ui.horizontal(|ui| {
            let mut selected = color.unwrap_or(default);
            if Self::color_picker(ui, &mut selected) {
                *color = Some(selected);
            }
            if ui
                .small_button(l10n::text(ui.ctx(), reset))
                .labelled_by(label.id)
                .clicked()
            {
                *color = None;
            }
        });
        ui.end_row();
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
            let label = ui.label(l10n::text(ui.ctx(), "Placement"));
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "profile_placement",
                    &mut profile.right_aligned,
                    [false, true],
                    |right| if *right { "Right" } else { "Left" }.to_owned(),
                );
            });
            ui.end_row();
            let label = ui.label(l10n::text(ui.ctx(), "Width"));
            let mut percent = profile.width * 100.0;
            ui.add(
                egui::DragValue::new(&mut percent)
                    .range(1.0..=100.0)
                    .suffix("%"),
            )
            .labelled_by(label.id);
            if percent != profile.width * 100.0 {
                profile.width = percent / 100.0;
            }
            ui.end_row();
            Self::profile_session(ui, "Session", &mut profile.session);
            Self::profile_rows(ui, &mut profile.rows);
            Self::value_area(ui, &mut profile.value_area);
            let colors = SessionColors::new(&TradeProfileSettings::default(), &ui.active_theme());
            Self::followed_color(
                ui,
                "Point of control",
                Some(&mut profile.poc),
                &mut profile.poc_color,
                colors.poc,
                "Auto",
            );
            Self::switch(ui, "Value area guides", &mut profile.value_area_guides);
            // Without guides, the edge colours still paint their extensions.
            let guides = profile.value_area_guides;
            Self::followed_color(
                ui,
                "Value area high",
                guides.then_some(&mut profile.value_area_high),
                &mut profile.value_area_high_color,
                colors.value_area_high,
                "Auto",
            );
            Self::followed_color(
                ui,
                "Value area low",
                guides.then_some(&mut profile.value_area_low),
                &mut profile.value_area_low_color,
                colors.value_area_low,
                "Auto",
            );
            for (name, color, default) in [
                ("Up volume", &mut profile.up_volume_color, colors.volume[0]),
                (
                    "Down volume",
                    &mut profile.down_volume_color,
                    colors.volume[1],
                ),
                (
                    "Up volume in value area",
                    &mut profile.value_area_up_color,
                    colors.value_area_volume[0],
                ),
                (
                    "Down volume in value area",
                    &mut profile.value_area_down_color,
                    colors.value_area_volume[1],
                ),
            ] {
                Self::followed_color(ui, name, None, color, default, "Auto");
            }
            Self::followed_color(
                ui,
                "Volume values",
                Some(&mut profile.values),
                &mut profile.values_color,
                colors.values,
                "Auto",
            );
            Self::followed_color(
                ui,
                "Session box",
                Some(&mut profile.session_box),
                &mut profile.session_box_color,
                colors.session_box,
                "Auto",
            );
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

    /// Session hours: the chart's calendar, the instrument's regular or extended windows, its
    /// pre-market or post-market alone or each segment apart, or up to five custom windows.
    fn profile_session(ui: &mut Ui, name: &str, session: &mut ProfileSession) {
        let label = ui.label(l10n::text(ui.ctx(), name));
        let custom = || ProfileSession::Custom(crate::model::CustomProfileSessions::default());
        let choice = |session: &ProfileSession| match session {
            ProfileSession::Chart => 0,
            ProfileSession::Regular => 1,
            ProfileSession::Extended => 2,
            ProfileSession::Custom(_) => 3,
            ProfileSession::PreMarket => 4,
            ProfileSession::PostMarket => 5,
            ProfileSession::Segments => 6,
        };
        let mut selected = choice(session);
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select("profile_session", &mut selected, 0..7, |i| {
                [
                    "Chart hours",
                    "Regular hours",
                    "Extended hours",
                    "Custom",
                    "Pre-market only",
                    "Post-market only",
                    "Each segment of the day",
                ][*i]
                    .to_owned()
            });
        });
        ui.end_row();
        if selected != choice(session) {
            *session = [
                ProfileSession::Chart,
                ProfileSession::Regular,
                ProfileSession::Extended,
                custom(),
                ProfileSession::PreMarket,
                ProfileSession::PostMarket,
                ProfileSession::Segments,
            ][selected]
                .clone();
        }
        let ProfileSession::Custom(custom) = session else {
            return;
        };
        let mut remove = None;
        for (i, window) in custom
            .sessions
            .iter_mut()
            .take(crate::model::CustomProfileSessions::MAX)
            .enumerate()
        {
            let [start, end, zone] = if i == 0 {
                ["Session start", "Session end", "Session zone"]
                    .map(|label| l10n::text(ui.ctx(), label))
            } else {
                ["Session {0} start", "Session {0} end", "Session {0} zone"]
                    .map(|label| l10n::format_text(ui.ctx(), label, &[("0", (i + 1).to_string())]))
            };
            for (name, time) in [(start, &mut window.start), (end, &mut window.end)] {
                let label = ui.label(name);
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
            let label = ui.label(zone);
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
                if !options.contains(&window.zone) {
                    options.push(window.zone.clone());
                }
                ui.combo_select(("profile_zone", i), &mut window.zone, options, |zone| {
                    zone.as_ref()
                        .map_or("Exchange", |zone| zone.name())
                        .to_owned()
                });
            });
            ui.end_row();
            if i > 0 {
                ui.label("");
                if ui
                    .button(l10n::format_text(
                        ui.ctx(),
                        "Remove session {0}",
                        &[("0", (i + 1).to_string())],
                    ))
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.end_row();
            }
        }
        if let Some(i) = remove {
            custom.sessions.remove(i);
        }
        ui.label("");
        if ui
            .add_enabled(
                custom.sessions.len() < crate::model::CustomProfileSessions::MAX,
                egui::Button::new(l10n::text(ui.ctx(), "Add session")),
            )
            .clicked()
        {
            custom
                .sessions
                .push(crate::model::CustomProfileSession::new(
                    NaiveTime::MIN,
                    NaiveTime::MIN,
                ));
        }
        ui.end_row();
        let label = ui.label(l10n::text(ui.ctx(), "Overlapping hours"));
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "profile_overlap",
                &mut custom.overlap,
                [
                    crate::model::SessionOverlap::OwnProfile,
                    crate::model::SessionOverlap::InBoth,
                ],
                |choice| {
                    match choice {
                        crate::model::SessionOverlap::OwnProfile => "Own profile",
                        crate::model::SessionOverlap::InBoth => "In both sessions",
                    }
                    .to_owned()
                },
            );
        });
        ui.end_row();
        ui.label("");
        ui.checkbox(
            &mut custom.trading_hours_only,
            l10n::text(ui.ctx(), "Clip to trading hours"),
        );
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
                Self::switch(ui, REAL_PRICE, &mut params.renko_real_price);
            }
            ChartType::Kagi => {
                Self::box_size(ui, &mut params.kagi_size_method, &mut params.kagi_reversal);
                Self::transform_source(ui, &mut params.kagi_source);
                Self::switch(ui, REAL_PRICE, &mut params.kagi_real_price);
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
                // One step back applies to a reversal of one box only.
                let label = ui.label(l10n::text(ui.ctx(), "One step back"));
                let one_box = params.pnf_reversal == 1;
                let switch = egui::Checkbox::without_text(&mut params.pnf_one_step_back);
                ui.add_enabled(one_box, switch).labelled_by(label.id);
                ui.end_row();
                Self::switch(ui, REAL_PRICE, &mut params.pnf_real_price);
            }
            ChartType::LineBreak => {
                let label = ui.label(l10n::text(ui.ctx(), "Number of lines"));
                ui.add(egui::DragValue::new(&mut params.line_break_count).range(1..=10))
                    .labelled_by(label.id);
                ui.end_row();
                Self::transform_source(ui, &mut params.line_break_source);
                Self::switch(ui, REAL_PRICE, &mut params.line_break_real_price);
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
                let label = ui.label(l10n::text(ui.ctx(), "Profile period"));
                ui.label_inputs_by(label.id, |ui| {
                    ui.combo_select(
                        "tpo_profile_period",
                        &mut params.tpo_profile_period,
                        [
                            TPOProfilePeriod::Session,
                            TPOProfilePeriod::Week,
                            TPOProfilePeriod::Month,
                        ],
                        |period| {
                            match period {
                                TPOProfilePeriod::Session => "Days",
                                TPOProfilePeriod::Week => "Weeks",
                                TPOProfilePeriod::Month => "Months",
                            }
                            .to_owned()
                        },
                    );
                });
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Profile length"));
                // A host count above the range survives an unedited Ok.
                ui.add(
                    egui::DragValue::new(&mut params.tpo_profile_periods)
                        .range(1..=100)
                        .clamp_existing_to_range(false),
                )
                .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Initial balance"));
                ui.label_inputs_by(label.id, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut params.tpo_initial_balance, "");
                        ui.add(
                            egui::DragValue::new(&mut params.tpo_initial_balance_minutes)
                                .range(1..=1440)
                                .suffix(l10n::text(ui.ctx(), " min")),
                        );
                    })
                });
                ui.end_row();
                for (name, enabled) in [
                    ("Point of control", &mut params.tpo_poc),
                    ("Value area high and low", &mut params.tpo_value_area_levels),
                    ("Open price", &mut params.tpo_open_price),
                    ("Closing price", &mut params.tpo_close_price),
                    ("Volume profile", &mut params.tpo_volume_profile),
                    (
                        "Initial balance extensions",
                        &mut params.tpo_initial_balance_extensions,
                    ),
                ] {
                    let label = ui.label(l10n::text(ui.ctx(), name));
                    ui.checkbox(enabled, "").labelled_by(label.id);
                    ui.end_row();
                }
                Self::tpo_volume(ui, params);
                let mut poor = [
                    params.tpo_poor_high || params.tpo_poor_highs_lows,
                    params.tpo_poor_low || params.tpo_poor_highs_lows,
                ];
                for (name, i) in [("Poor high", 0), ("Poor low", 1)] {
                    let label = ui.label(l10n::text(ui.ctx(), name));
                    if ui
                        .checkbox(&mut poor[i], "")
                        .labelled_by(label.id)
                        .changed()
                    {
                        [params.tpo_poor_high, params.tpo_poor_low] = poor;
                        params.tpo_poor_highs_lows = false;
                    }
                    ui.end_row();
                }
                Self::wrapped_switch(
                    ui,
                    "Extend poor highs and lows until repaired",
                    poor[0] || poor[1],
                    &mut params.tpo_extend_poor,
                );
                for (name, enabled, value) in [
                    (
                        "Extend point of control until touched",
                        params.tpo_poc,
                        &mut params.tpo_extend_poc,
                    ),
                    (
                        "Extend value area high until touched",
                        params.tpo_value_area_levels,
                        &mut params.tpo_extend_vah,
                    ),
                    (
                        "Extend value area low until touched",
                        params.tpo_value_area_levels,
                        &mut params.tpo_extend_val,
                    ),
                ] {
                    Self::wrapped_switch(ui, name, enabled, value);
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
                // One-day profiles choose their hours as session volume profiles do.
                if !params.tpo_joins_sessions() {
                    let mut hours = params.tpo_hours();
                    Self::profile_session(ui, "Hours", &mut hours);
                    if hours != params.tpo_hours() {
                        params.tpo_session = hours;
                        params.tpo_split_sessions = false;
                    }
                }
                let label = ui.label(l10n::text(ui.ctx(), "Single prints"));
                ui.checkbox(&mut params.tpo_single_prints, "")
                    .labelled_by(label.id);
                ui.end_row();
                Self::wrapped_switch(
                    ui,
                    "Extend single prints until touched",
                    params.tpo_single_prints,
                    &mut params.tpo_extend_single_prints,
                );
                let label = ui.label(l10n::text(ui.ctx(), "TPO Midpoint"));
                ui.checkbox(&mut params.tpo_midpoint, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Letters"));
                ui.checkbox(&mut params.tpo_letters, "")
                    .labelled_by(label.id);
                ui.end_row();
                // Without letters, blocks are the only display.
                let label = ui.label(l10n::text(ui.ctx(), "Blocks"));
                ui.add_enabled(
                    params.tpo_letters,
                    egui::Checkbox::without_text(&mut params.tpo_blocks),
                )
                .labelled_by(label.id);
                ui.end_row();
                ui.checkbox(
                    &mut params.tpo_colored_blocks,
                    l10n::text(ui.ctx(), "Coloured blocks"),
                );
                let colored = params.tpo_colored_blocks;
                ui.add_enabled_ui(colored, |ui| {
                    ui.horizontal(|ui| {
                        for color in &mut params.tpo_block_colors {
                            Self::color_picker(ui, color);
                        }
                    })
                });
                ui.end_row();
                ui.label(l10n::text(ui.ctx(), "Block colour"));
                ui.add_enabled_ui(!colored, |ui| {
                    Self::color_picker(ui, &mut params.tpo_block_color)
                });
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Opacity outside value area"));
                ui.add(
                    egui::DragValue::new(&mut params.tpo_outside_value_area_opacity)
                        .range(0..=100)
                        .suffix("%"),
                )
                .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Split display"));
                ui.checkbox(&mut params.tpo_split_periods, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Statistics"));
                ui.checkbox(&mut params.tpo_statistics, "")
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Statistics rows"));
                ui.label_inputs_by(label.id, |ui| {
                    // One switch a line: the longer names would wrap into each other.
                    ui.vertical(|ui| {
                        let enabled = params.tpo_statistics;
                        for (_, name, shown) in params.tpo_statistics_rows.switches() {
                            let name = l10n::text(ui.ctx(), name);
                            // A name of its own: the Labels row has a `POC` switch too.
                            let row = l10n::format_text(
                                ui.ctx(),
                                "Statistics: {row}",
                                &[("row", name.clone())],
                            );
                            ui.add_enabled(enabled, egui::Checkbox::new(shown, name))
                                .accessible_name(row);
                        }
                    })
                });
                ui.end_row();
                let label = ui.label(l10n::text(ui.ctx(), "Labels"));
                ui.label_inputs_by(label.id, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let labels = &mut params.tpo_labels;
                        for (caption, name, shown) in [
                            ("POC", "Point of control", &mut labels.poc),
                            ("VAH", "Value area high", &mut labels.value_area_high),
                            ("VAL", "Value area low", &mut labels.value_area_low),
                            ("Mid", "Midpoint", &mut labels.midpoint),
                            (
                                "IBH",
                                "Initial balance high",
                                &mut labels.initial_balance_high,
                            ),
                            (
                                "IBL",
                                "Initial balance low",
                                &mut labels.initial_balance_low,
                            ),
                            ("Open", "Opening price", &mut labels.opening_price),
                            ("Close", "Closing price", &mut labels.closing_price),
                            ("PH", "Poor high", &mut labels.poor_high),
                            ("PL", "Poor low", &mut labels.poor_low),
                            ("SP", "Single prints", &mut labels.single_prints),
                        ] {
                            ui.checkbox(shown, l10n::text(ui.ctx(), caption))
                                .on_hover_text(l10n::text(ui.ctx(), name));
                        }
                    })
                });
                ui.end_row();
                Self::switch(ui, "Label prices", &mut params.tpo_labels.prices);
                Self::switch(ui, "Price scale tags", &mut params.tpo_level_tags);
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

    /// The TPO volume profile's levels, values, colours and side, editable while it is drawn.
    fn tpo_volume(ui: &mut Ui, params: &mut ChartTypeParams) {
        let enabled = params.tpo_volume_profile;
        let opacity = f32::from(params.tpo_outside_value_area_opacity.min(100)) / 100.0;
        let volume = &mut params.tpo_volume;
        for (name, shown) in [
            ("Volume point of control", &mut volume.poc),
            ("Volume value area high", &mut volume.value_area_high),
            ("Volume value area low", &mut volume.value_area_low),
            ("Volume values", &mut volume.values),
        ] {
            let label = ui.label(l10n::text(ui.ctx(), name));
            ui.add_enabled(enabled, egui::Checkbox::without_text(shown))
                .labelled_by(label.id);
            ui.end_row();
        }
        // Defaults show as the colours they draw, until one is picked.
        let chart = ui.active_theme().chart;
        let inside = volume.value_area_color.unwrap_or(chart.axis_text_secondary);
        let outside = chart.bg.lerp_to_gamma(inside, opacity);
        for (name, color, default) in [
            ("Volume colour", &mut volume.color, outside),
            (
                "Volume value area colour",
                &mut volume.value_area_color,
                inside,
            ),
        ] {
            ui.label(l10n::text(ui.ctx(), name));
            let mut shown = color.unwrap_or(default);
            if ui
                .add_enabled_ui(enabled, |ui| Self::color_picker(ui, &mut shown))
                .inner
            {
                *color = Some(shown);
            }
            ui.end_row();
        }
        let label = ui.label(l10n::text(ui.ctx(), "Volume placement"));
        ui.add_enabled_ui(enabled, |ui| {
            ui.label_inputs_by(label.id, |ui| {
                ui.combo_select(
                    "tpo_volume_placement",
                    &mut volume.left,
                    [true, false],
                    |left| if *left { "Left" } else { "Right" }.to_owned(),
                );
            })
        });
        ui.end_row();
    }

    fn price_source_dropdown(ui: &mut Ui, source: &mut PriceSource) {
        ui.combo_select("price_source", source, PriceSource::all(), |v| {
            v.label().to_string()
        });
    }

    fn high_low(ui: &mut Ui, style: &mut HighLowStyle) {
        Self::fill_row(ui, "Bar colour", &mut style.bar_color);
        Self::switch(ui, "Price labels", &mut style.labels_visible);
        ui.label(l10n::text(ui.ctx(), "Label colour"));
        // The default that follows the theme shows as the colour it prints in, until one is picked.
        let mut shown = if style.label_color == Color32::PLACEHOLDER {
            ui.active_theme().chart.axis_text
        } else {
            style.label_color
        };
        if Self::color_picker(ui, &mut shown) {
            style.label_color = shown;
        }
        ui.end_row();
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
        Self::line_width_and_style(ui, style, id);
    }

    /// The width and style rows of a line, below its colour rows.
    fn line_width_and_style(ui: &mut Ui, style: &mut LinePlotStyle, id: &str) {
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

    /// A fill at the base that, while `None`, shows `edge`, the colour of a flat fill, and becomes
    /// a gradient endpoint once picked.
    fn fill_at_base_row(ui: &mut Ui, label: &str, color: &mut Option<Color32>, edge: Color32) {
        ui.label(l10n::text(ui.ctx(), label));
        let mut shown = color.unwrap_or(edge);
        if Self::color_picker(ui, &mut shown) {
            *color = Some(shown);
        }
        ui.end_row();
    }

    fn candle_colors_section(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.label(l10n::text(ui.ctx(), "Body"));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            Self::color_picker(ui, &mut settings.bullish_color);
            Self::color_picker(ui, &mut settings.bearish_color);
        });
        ui.end_row();

        if settings.chart_type.is_time_independent() {
            return;
        }

        let body = [settings.bullish_color, settings.bearish_color];
        Self::custom_colors_row(
            ui,
            "Border",
            [
                &mut settings.bullish_border_color,
                &mut settings.bearish_border_color,
            ],
            body,
        );
        Self::custom_colors_row(
            ui,
            "Wick",
            [
                &mut settings.bullish_wick_color,
                &mut settings.bearish_wick_color,
            ],
            body,
        );
    }

    /// A row of rising and falling colours that are `None` until made custom, starting from
    /// `defaults`.
    fn custom_colors_row(
        ui: &mut Ui,
        label: &str,
        [up, down]: [&mut Option<Color32>; 2],
        [default_up, default_down]: [Color32; 2],
    ) {
        ui.label(l10n::text(ui.ctx(), label));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            Self::optional_color_picker(ui, up, default_up);
            Self::optional_color_picker(ui, down, default_down);
            for (color, default, label) in [
                (up, default_up, "Custom up"),
                (down, default_down, "Custom down"),
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

    /// Rows of a style's rising and falling colours; one saved before the style had them shows
    /// the candle body colour it draws in, `candles`, until picked.
    fn own_color_rows(ui: &mut Ui, rows: [(&str, &mut Color32); 2], candles: [Color32; 2]) {
        for ((label, color), candle) in rows.into_iter().zip(candles) {
            ui.label(l10n::text(ui.ctx(), label));
            let mut shown = crate::model::or_candle(*color, candle);
            if Self::color_picker(ui, &mut shown) {
                *color = shown;
            }
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
        let custom = state.custom_mode;
        let picked = ColorPicker::new(&mut state, id).show(ui).inner;
        if let Some(selected) = picked {
            // Palette swatches are opaque, so a swatch keeps the colour's opacity unless the Custom
            // editor set it.
            *color = keep_opacity(selected, *color, custom || state.custom_mode);
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
