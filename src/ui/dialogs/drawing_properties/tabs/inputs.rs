//! Inputs that correspond to the selected tool's saved drawing configuration.

use crate::drawings::domain::{GANN_MAX_ARC, GANN_SQUARE_UNITS};
use crate::drawings::{
    Drawing, DrawingToolType as Tool, ElliottDegree, FibonacciLevel, GannConfig, GannFanDirection,
    GannLevel, PitchforkConfig, PitchforkLevel, PositionSize, RegressionTrendConfig, VwapBand,
};

use crate::data::{ProfileDisplay, VolumeProfileConfig};
use crate::localization as l10n;
use crate::model::LineStyle;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid, buttons::Button};
use egui::Ui;

use super::{CoordinatePoint, CoordinatesState};

pub(crate) struct InputsTab;

impl InputsTab {
    pub(crate) fn available(tool: Tool) -> bool {
        matches!(
            tool,
            Tool::FibonacciRetracement
                | Tool::FibonacciChannel
                | Tool::FibonacciExtension
                | Tool::FibonacciWedge
                | Tool::FibonacciTimeZones
                | Tool::FibonacciSpeedFan
                | Tool::TrendBasedFibTime
                | Tool::FibonacciCircles
                | Tool::FibonacciSpeedResistanceArcs
                | Tool::Pitchfork
                | Tool::SchiffPitchfork
                | Tool::ModifiedSchiffPitchfork
                | Tool::InsidePitchfork
                | Tool::Pitchfan
                | Tool::GannBox
                | Tool::GannSquare
                | Tool::GannFixed
                | Tool::GannFan
                | Tool::GhostFeed
                | Tool::LongPos
                | Tool::ShortPos
                | Tool::FixedRangeVolumeProfile
                | Tool::AnchoredVolumeProfile
                | Tool::AnchoredVWAP
                | Tool::RegressionTrend
                | Tool::ElliottImpulse
                | Tool::ElliottCorrection
                | Tool::ElliottTriangle
                | Tool::ElliottDoubleCombo
                | Tool::ElliottTripleCombo
        )
    }

    pub(crate) fn show(
        ui: &mut Ui,
        drawing: &mut Drawing,
        pickers: &mut Vec<Vec<ColorPickerState>>,
        coordinates: &mut CoordinatesState,
        tick_size: Option<f64>,
    ) {
        ui.space_md();
        match drawing.tool_type {
            Tool::FibonacciRetracement
            | Tool::FibonacciExtension
            | Tool::FibonacciChannel
            | Tool::FibonacciWedge
            | Tool::FibonacciTimeZones
            | Tool::FibonacciSpeedFan
            | Tool::TrendBasedFibTime
            | Tool::FibonacciCircles
            | Tool::FibonacciSpeedResistanceArcs => {
                fibonacci(ui, drawing, grid_pickers(pickers, 0))
            }
            Tool::Pitchfork
            | Tool::SchiffPitchfork
            | Tool::ModifiedSchiffPitchfork
            | Tool::InsidePitchfork
            | Tool::Pitchfan => pitchfork(ui, drawing, grid_pickers(pickers, 0)),
            Tool::GannBox | Tool::GannSquare | Tool::GannFixed | Tool::GannFan => {
                gann(ui, drawing, pickers, coordinates)
            }
            Tool::GhostFeed => {
                let mut config = drawing.ghost_feed.unwrap_or_default();
                let original = config;
                FormGrid::new("ghost_feed_inputs").show(ui, |ui| {
                    number(
                        ui,
                        "Average range (ticks)",
                        &mut config.average_range_ticks,
                        f64::MIN_POSITIVE..=f64::MAX,
                    );
                    number(ui, "Noise (ticks)", &mut config.noise_ticks, 0.0..=f64::MAX);
                });
                if config != original {
                    drawing.ghost_feed = Some(config);
                }
            }
            Tool::LongPos | Tool::ShortPos => {
                position_levels(ui, coordinates, drawing.tool_type, tick_size);
                let colors = drawing.position_colors();
                let pickers = grid_pickers(pickers, 0);
                pickers.resize_with(2, ColorPickerState::default);
                FormGrid::new("position_colors").show(ui, |ui| {
                    let (target_picker, stop_picker) = pickers.split_at_mut(1);
                    for (label, saved, color, picker) in [
                        (
                            "Target colour",
                            &mut drawing.position_target_color,
                            colors[0],
                            &mut target_picker[0],
                        ),
                        (
                            "Stop colour",
                            &mut drawing.position_stop_color,
                            colors[1],
                            &mut stop_picker[0],
                        ),
                    ] {
                        let label = ui.label(l10n::text(ui.ctx(), label));
                        let mut color = color.to_srgba_unmultiplied();
                        if ui
                            .push_id(label.id, |ui| {
                                ui.label_inputs_by(label.id, |ui| {
                                    level_color(ui, &mut color, picker)
                                })
                            })
                            .inner
                        {
                            *saved = Some(color);
                        }
                        ui.end_row();
                    }
                });
                let mut size = drawing.position_size.unwrap_or_default();
                let original = size;
                let mut mode = match size {
                    PositionSize::Quantity(_) => 0,
                    PositionSize::RiskAmount(_) => 1,
                    PositionSize::RiskPercent { .. } => 2,
                };
                FormGrid::new("position_inputs").show(ui, |ui| {
                    let label = ui.label(l10n::text(ui.ctx(), "Size by"));
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select("position_size_mode", &mut mode, [0, 1, 2], |v| {
                            ["Quantity", "Risk amount", "Risk percent"][*v].into()
                        });
                    });
                    ui.end_row();
                    if mode
                        != match size {
                            PositionSize::Quantity(_) => 0,
                            PositionSize::RiskAmount(_) => 1,
                            PositionSize::RiskPercent { .. } => 2,
                        }
                    {
                        size = match mode {
                            0 => PositionSize::Quantity(1.0),
                            1 => PositionSize::RiskAmount(100.0),
                            _ => PositionSize::RiskPercent {
                                account_size: 10_000.0,
                                percent: 1.0,
                            },
                        };
                    }
                    match &mut size {
                        PositionSize::Quantity(value) => {
                            number(ui, "Quantity", value, f64::MIN_POSITIVE..=f64::MAX)
                        }
                        PositionSize::RiskAmount(value) => {
                            number(ui, "Risk amount", value, f64::MIN_POSITIVE..=f64::MAX)
                        }
                        PositionSize::RiskPercent {
                            account_size,
                            percent,
                        } => {
                            number(
                                ui,
                                "Account size",
                                account_size,
                                f64::MIN_POSITIVE..=f64::MAX,
                            );
                            number(ui, "Risk (%)", percent, f64::MIN_POSITIVE..=100.0);
                        }
                    }
                    let sizing = &mut drawing.position_sizing;
                    number(
                        ui,
                        "Point value",
                        &mut sizing.point_value,
                        f64::MIN_POSITIVE..=f64::MAX,
                    );
                    number(
                        ui,
                        "Lot size",
                        &mut sizing.lot_size,
                        f64::MIN_POSITIVE..=f64::MAX,
                    );
                    let label = ui.label(l10n::text(ui.ctx(), "Quantity step"));
                    ui.add(
                        egui::DragValue::new(&mut sizing.quantity_step)
                            .range(0.0..=f64::MAX)
                            .speed(0.001)
                            .custom_formatter(|value, _| value.to_string()),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                    number(
                        ui,
                        "Leverage",
                        &mut sizing.leverage,
                        f64::MIN_POSITIVE..=f64::MAX,
                    );
                });
                if size != original {
                    drawing.position_size = Some(size);
                }
            }
            Tool::FixedRangeVolumeProfile | Tool::AnchoredVolumeProfile => {
                let original = drawing.volume_profile.unwrap_or_default();
                let mut config = original;
                let mut rows = config.rows();
                let mut fixed = config.row_size().is_some();
                let mut height = config.row_size().unwrap_or(1.0);
                let mut percent = config.value_area() * 100.0;
                FormGrid::new("profile_inputs").show(ui, |ui| {
                    let label = ui.label(l10n::text(ui.ctx(), "Row layout"));
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select("profile_layout", &mut fixed, [false, true], |fixed| {
                            if *fixed {
                                "Price height".into()
                            } else {
                                "Number of rows".into()
                            }
                        });
                    });
                    ui.end_row();
                    if fixed {
                        number(
                            ui,
                            "Price height",
                            &mut height,
                            f64::MIN_POSITIVE..=f64::MAX,
                        );
                    } else {
                        let label = ui.label(l10n::text(ui.ctx(), "Rows"));
                        ui.add(egui::DragValue::new(&mut rows).range(VolumeProfileConfig::ROWS))
                            .labelled_by(label.id);
                        ui.end_row();
                    }
                    number(
                        ui,
                        "Value area (%)",
                        &mut percent,
                        f64::MIN_POSITIVE..=100.0,
                    );
                    let label = ui.label(l10n::text(ui.ctx(), "Volume"));
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select(
                            "profile_display",
                            &mut config.display,
                            [
                                ProfileDisplay::Total,
                                ProfileDisplay::UpDown,
                                ProfileDisplay::Delta,
                            ],
                            |mode| match mode {
                                ProfileDisplay::Total => "Total".into(),
                                ProfileDisplay::UpDown => "Up / down".into(),
                                ProfileDisplay::Delta => "Delta".into(),
                            },
                        );
                    });
                    ui.end_row();
                    if config.display != ProfileDisplay::Total {
                        let pickers = grid_pickers(pickers, 0);
                        pickers.resize_with(2, ColorPickerState::default);
                        let label = ui.label(l10n::text(ui.ctx(), "Up colour"));
                        ui.label_inputs_by(label.id, |ui| {
                            level_color(ui, &mut config.up_color, &mut pickers[0]);
                        });
                        ui.end_row();
                        let label = ui.label(l10n::text(ui.ctx(), "Down colour"));
                        ui.label_inputs_by(label.id, |ui| {
                            level_color(ui, &mut config.down_color, &mut pickers[1]);
                        });
                        ui.end_row();
                    }
                });
                if let Some(mut updated) = VolumeProfileConfig::new(rows, percent / 100.0)
                    .and_then(|c| c.with_row_size(fixed.then_some(height)))
                {
                    updated.display = config.display;
                    updated.up_color = config.up_color;
                    updated.down_color = config.down_color;
                    if updated != original {
                        drawing.volume_profile = Some(updated);
                    }
                }
            }
            Tool::AnchoredVWAP => {
                let original = drawing.vwap.unwrap_or_default();
                let mut config = original;
                for (i, band) in config.bands.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            let name = |ui: &Ui, property| {
                                l10n::format_text(ui.ctx(), property, &[("0", (i + 1).to_string())])
                            };
                            ui.checkbox(&mut band.enabled, name(ui, "Band {0}"));
                            let mut multiplier = band.multiplier();
                            if ui
                                .add(
                                    egui::DragValue::new(&mut multiplier)
                                        .range(f64::MIN_POSITIVE..=10.0)
                                        .speed(0.1),
                                )
                                .accessible_name(name(ui, "Band {0} multiplier"))
                                .changed()
                                && let Some(updated) = VwapBand::new(band.enabled, multiplier)
                            {
                                *band = updated;
                            }
                        });
                    });
                }
                if config != original {
                    drawing.vwap = Some(config);
                }
            }
            Tool::RegressionTrend => {
                let config = drawing.regression_trend.unwrap_or_default();
                let mut deviations = config.deviations();
                FormGrid::new("regression_inputs").show(ui, |ui| {
                    number(ui, "Deviations", &mut deviations, f64::MIN_POSITIVE..=10.0)
                });
                if deviations != config.deviations() {
                    drawing.regression_trend = RegressionTrendConfig::new(deviations);
                }
            }
            Tool::ElliottImpulse
            | Tool::ElliottCorrection
            | Tool::ElliottTriangle
            | Tool::ElliottDoubleCombo
            | Tool::ElliottTripleCombo => {
                FormGrid::new("elliott_inputs").show(ui, |ui| {
                    let label = ui.label(l10n::text(ui.ctx(), "Degree"));
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select(
                            "elliott_degree",
                            &mut drawing.elliott_degree,
                            ElliottDegree::ALL,
                            |degree| {
                                format!(
                                    "{}: {}, {}",
                                    degree.name(),
                                    degree.label('1'),
                                    degree.label('A')
                                )
                            },
                        );
                    });
                    ui.end_row();
                });
            }
            _ => {}
        }
    }
}

/// Edit staged prices, leaving the anchors' times and bar indices intact.
fn position_levels(
    ui: &mut Ui,
    coordinates: &mut CoordinatesState,
    tool: Tool,
    tick_size: Option<f64>,
) {
    let [entry, target, ..] = coordinates.points.as_slice() else {
        return;
    };
    let mirrored = entry
        .price
        .parse::<f64>()
        .ok()
        .zip(target.price.parse::<f64>().ok())
        .map(|(entry, target)| (2.0 * entry - target).to_string())
        .unwrap_or_default();
    let stop = coordinates
        .points
        .get(2)
        .map_or(mirrored.as_str(), |stop| stop.price.as_str());
    let prices = [entry.price.as_str(), target.price.as_str(), stop]
        .map(|price| price.parse::<f64>().ok().filter(|price| price.is_finite()));
    let [
        Some(mut entry_price),
        Some(mut target_price),
        Some(mut stop_price),
    ] = prices
    else {
        ui.label(l10n::text(ui.ctx(), "Enter finite prices in Coordinates"));
        return;
    };
    let original = [entry_price, target_price, stop_price];
    let tick = tick_size.filter(|tick| tick.is_finite() && *tick > 0.0);
    let side = if tool == Tool::LongPos { 1.0 } else { -1.0 };
    FormGrid::new("position_levels").show(ui, |ui| {
        if position_number(
            ui,
            "Entry price",
            &mut entry_price,
            tick.unwrap_or(0.1),
            false,
            true,
        ) {
            let delta = entry_price - original[0];
            target_price += delta;
            stop_price += delta;
        }
        for (price_label, ticks_label, price, direction) in [
            ("Target price", "Target (ticks)", &mut target_price, side),
            ("Stop price", "Stop (ticks)", &mut stop_price, -side),
        ] {
            position_number(ui, price_label, price, tick.unwrap_or(0.1), false, true);
            let mut ticks = tick.map_or(0.0, |tick| direction * (*price - entry_price) / tick);
            let enabled = tick.is_some() && ticks.is_finite();
            let changed = position_number(ui, ticks_label, &mut ticks, 1.0, true, enabled);
            if changed && let Some(tick) = tick {
                *price = entry_price + direction * ticks * tick;
            }
        }
    });
    let updated = [entry_price, target_price, stop_price];
    if updated != original && updated.iter().all(|price| price.is_finite()) {
        if coordinates.points.len() == 2 {
            let target = &coordinates.points[1];
            let mut stop = CoordinatePoint::default();
            stop.date.clone_from(&target.date);
            stop.time.clone_from(&target.time);
            stop.bar_index.clone_from(&target.bar_index);
            coordinates.points.push(stop);
        }
        for (point, price) in coordinates.points.iter_mut().zip(updated) {
            point.price = price.to_string();
        }
    }
    if tick.is_none() {
        ui.label(l10n::text(ui.ctx(), "Tick size unavailable"));
    }
}

fn position_number(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    speed: f64,
    ticks: bool,
    enabled: bool,
) -> bool {
    let label = ui.label(l10n::text(ui.ctx(), label));
    let changed = ui
        .add_enabled(
            enabled,
            egui::DragValue::new(value)
                .range(if ticks { 0.0 } else { -f64::MAX }..=f64::MAX)
                .clamp_existing_to_range(false)
                .speed(speed)
                .custom_formatter(move |value, _| {
                    if ticks {
                        egui::emath::format_with_decimals_in_range(value, 0..=8)
                    } else {
                        value.to_string()
                    }
                }),
        )
        .labelled_by(label.id)
        .changed();
    ui.end_row();
    changed
}

fn number(ui: &mut Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    let label = ui.label(l10n::text(ui.ctx(), label));
    ui.add(egui::DragValue::new(value).range(range).speed(0.1))
        .labelled_by(label.id);
    ui.end_row();
}

fn background(ui: &mut Ui, show: &mut bool, opacity: &mut u8) {
    ui.horizontal(|ui| {
        ui.checkbox(show, l10n::text(ui.ctx(), "Background"));
        let mut percent = f32::from(*opacity) * 100.0 / 255.0;
        if ui
            .add_enabled(
                *show,
                egui::DragValue::new(&mut percent)
                    .range(0.0..=100.0)
                    .max_decimals(1)
                    .suffix("%")
                    .prefix(l10n::text(ui.ctx(), "Opacity ")),
            )
            .changed()
        {
            *opacity = (percent * 255.0 / 100.0).round() as u8;
        }
    });
}

/// Borrowed row fields let each tool keep its own saved level model.
struct LevelRow<'a> {
    visible: &'a mut bool,
    value: &'a mut f32,
    legacy_label: Option<&'a mut String>,
    color: &'a mut [u8; 4],
    line_style: Option<&'a mut LineStyle>,
    width: Option<(&'a mut Option<f32>, f32)>,
    range: std::ops::RangeInclusive<f32>,
}

/// The colour pickers of the tab's `index`th level grid.
fn grid_pickers(
    pickers: &mut Vec<Vec<ColorPickerState>>,
    index: usize,
) -> &mut Vec<ColorPickerState> {
    if pickers.len() <= index {
        pickers.resize_with(index + 1, Vec::new);
    }
    &mut pickers[index]
}

/// `item` is the English noun of one row, which names its controls and scopes their ids.
fn level_grid<T>(
    ui: &mut Ui,
    item: &str,
    levels: &mut Vec<T>,
    pickers: &mut Vec<ColorPickerState>,
    new_level: T,
    row: impl Fn(&mut T) -> LevelRow<'_>,
) {
    ui.push_id(item, |ui| {
        pickers.resize_with(levels.len() + 1, ColorPickerState::default);
        ui.horizontal(|ui| {
            ui.label(l10n::text(ui.ctx(), "One colour for all levels"));
            let mut color = levels
                .first_mut()
                .map_or([128, 128, 128, 255], |l| *row(l).color);
            if level_color(ui, &mut color, &mut pickers[0]) {
                for level in levels.iter_mut() {
                    *row(level).color = color;
                }
            }
        });
        let mut remove = None;
        for (i, level) in levels.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let fields = row(level);
                    let name = |ui: &Ui, property: &str| {
                        l10n::format_text(
                            ui.ctx(),
                            &format!("{item} {{0}} {property}"),
                            &[("0", (i + 1).to_string())],
                        )
                    };
                    ui.checkbox(fields.visible, "")
                        .accessible_name(name(ui, "visible"));
                    if ui
                        .add(
                            egui::DragValue::new(fields.value)
                                .range(fields.range)
                                .speed(0.001)
                                .max_decimals(6),
                        )
                        .accessible_name(name(ui, "ratio"))
                        .changed()
                        && let Some(label) = fields.legacy_label
                    {
                        label.clear();
                    }
                    level_color(ui, fields.color, &mut pickers[i + 1]);
                    if let Some(style) = fields.line_style {
                        egui::ComboBox::from_id_salt("level_style")
                            .icon(crate::ui_kit::buttons::combo_chevron)
                            .selected_text(l10n::text(ui.ctx(), style.name()))
                            .show_ui(ui, |ui| {
                                for choice in LineStyle::all() {
                                    ui.selectable_value(
                                        style,
                                        *choice,
                                        l10n::text(ui.ctx(), choice.name()),
                                    );
                                }
                            })
                            .response
                            .accessible_name(name(ui, "line style"));
                    }
                    if let Some((width, inherited)) = fields.width {
                        let mut value = width.unwrap_or(inherited);
                        if ui
                            .add(
                                egui::DragValue::new(&mut value)
                                    .range(
                                        f32::MIN_POSITIVE
                                            ..=crate::drawings::validation::MAX_STROKE_WIDTH,
                                    )
                                    .speed(0.1)
                                    .suffix(" px"),
                            )
                            .accessible_name(name(ui, "width"))
                            .changed()
                        {
                            *width = Some(value);
                        }
                    }
                    if Button::new("Remove").ghost().show(ui).clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            levels.remove(i);
            pickers.remove(i + 1);
        }
        if Button::new("Add level").secondary().show(ui).clicked() {
            levels.push(new_level);
        }
    });
}

fn fibonacci(ui: &mut Ui, drawing: &mut Drawing, pickers: &mut Vec<ColorPickerState>) {
    let mut config = drawing.fibonacci_config().into_owned();
    let original = config.clone();
    ui.strong(l10n::text(ui.ctx(), "Fibonacci levels"));
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(l10n::text(ui.ctx(), "Level width"));
        ui.add(
            egui::DragValue::new(&mut config.level_width)
                .range(0.1..=crate::drawings::validation::MAX_STROKE_WIDTH)
                .speed(0.1)
                .suffix(" px"),
        )
        .labelled_by(label.id);
        let label = ui.label(l10n::text(ui.ctx(), "Level style"));
        let ctx = ui.ctx().clone();
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "fib_level_style",
                &mut config.level_style,
                LineStyle::all().iter().copied(),
                |style| l10n::text(&ctx, style.name()),
            );
        });
    });
    level_grid(
        ui,
        "Level",
        &mut config.levels,
        pickers,
        FibonacciLevel::new(1.0, "", drawing.color),
        |level| LevelRow {
            visible: &mut level.visible,
            value: &mut level.value,
            legacy_label: Some(&mut level.label),
            color: &mut level.color,
            line_style: None,
            width: None,
            range: if matches!(
                drawing.tool_type,
                Tool::FibonacciCircles | Tool::FibonacciSpeedResistanceArcs | Tool::FibonacciWedge
            ) {
                0.0
            } else {
                -f32::MAX
            }..=f32::MAX,
        },
    );
    background(
        ui,
        &mut config.show_background,
        &mut config.background_opacity,
    );
    ui.horizontal_wrapped(|ui| {
        if matches!(
            drawing.tool_type,
            Tool::FibonacciRetracement | Tool::FibonacciExtension | Tool::FibonacciChannel
        ) {
            let mut prices = config.show_prices.unwrap_or(true);
            if ui
                .checkbox(&mut prices, l10n::text(ui.ctx(), "Prices"))
                .changed()
            {
                config.show_prices = Some(prices);
            }
        }
        let mut levels = config.show_levels.unwrap_or(true);
        if ui
            .checkbox(&mut levels, l10n::text(ui.ctx(), "Levels"))
            .changed()
        {
            config.show_levels = Some(levels);
        }
        ui.checkbox(
            &mut config.levels_as_percent,
            l10n::text(ui.ctx(), "Percent"),
        );
        ui.checkbox(
            &mut config.labels_on_left,
            l10n::text(ui.ctx(), "Labels on left"),
        );
    });
    if matches!(
        drawing.tool_type,
        Tool::FibonacciRetracement | Tool::FibonacciExtension | Tool::FibonacciChannel
    ) {
        ui.horizontal_wrapped(|ui| {
            if matches!(
                drawing.tool_type,
                Tool::FibonacciRetracement | Tool::FibonacciExtension
            ) {
                ui.checkbox(&mut config.reverse, l10n::text(ui.ctx(), "Reverse"));
            }
            ui.checkbox(&mut config.extend_left, l10n::text(ui.ctx(), "Extend left"));
            ui.checkbox(
                &mut config.extend_right,
                l10n::text(ui.ctx(), "Extend right"),
            );
        });
    }
    if matches!(
        drawing.tool_type,
        Tool::FibonacciRetracement | Tool::FibonacciExtension
    ) {
        ui.checkbox(
            &mut config.logarithmic,
            l10n::text(ui.ctx(), "Logarithmic levels"),
        );
    }
    if config != original {
        drawing.fib_config = Some(config);
    }
}

fn pitchfork(ui: &mut Ui, drawing: &mut Drawing, pickers: &mut Vec<ColorPickerState>) {
    // A fork saved without a config shows the levels it is drawn with.
    let mut config = drawing.pitchfork.clone().unwrap_or_else(|| {
        let mut config = PitchforkConfig::legacy();
        for level in &mut config.levels {
            level.color = drawing.color;
            level.width = Some(if level.value == 1.0 {
                crate::styles::stroke::HAIRLINE
            } else {
                drawing.stroke_width
            });
        }
        config
    });
    let original = config.clone();
    ui.strong(l10n::text(ui.ctx(), "Pitchfork levels"));
    ui.label(l10n::text(
        ui.ctx(),
        "Multiples of the median-to-tine distance",
    ));
    level_grid(
        ui,
        "Level",
        &mut config.levels,
        pickers,
        PitchforkLevel {
            value: 2.0,
            visible: true,
            color: drawing.color,
            line_style: LineStyle::Solid,
            width: Some(drawing.stroke_width),
        },
        |level| LevelRow {
            visible: &mut level.visible,
            value: &mut level.value,
            legacy_label: None,
            color: &mut level.color,
            line_style: Some(&mut level.line_style),
            width: Some((&mut level.width, drawing.stroke_width)),
            range: 0.0..=crate::drawings::validation::MAX_PITCHFORK_LEVEL,
        },
    );
    background(
        ui,
        &mut config.show_background,
        &mut config.background_opacity,
    );
    if config != original {
        drawing.pitchfork = Some(config);
    }
}

fn gann_row(level: &mut GannLevel, range: std::ops::RangeInclusive<f32>) -> LevelRow<'_> {
    LevelRow {
        visible: &mut level.visible,
        value: &mut level.value,
        legacy_label: None,
        color: &mut level.color,
        line_style: None,
        width: None,
        range,
    }
}

fn gann(
    ui: &mut Ui,
    drawing: &mut Drawing,
    pickers: &mut Vec<Vec<ColorPickerState>>,
    coordinates: &mut CoordinatesState,
) {
    let tool = drawing.tool_type;
    let mut config = drawing
        .gann
        .clone()
        .unwrap_or_else(|| GannConfig::for_tool(tool));
    let original = config.clone();
    let square = matches!(tool, Tool::GannSquare | Tool::GannFixed);
    let max = if square { GANN_SQUARE_UNITS } else { 1.0 };
    let new = |value| GannLevel::new(value, drawing.color);
    if tool == Tool::GannFan {
        ui.strong(l10n::text(ui.ctx(), "Fan lines"));
        level_grid(
            ui,
            "Fan line",
            &mut config.fan_ratios,
            grid_pickers(pickers, 0),
            new(1.0),
            |level| gann_row(level, 0.01..=100.0),
        );
    } else {
        if square {
            ui.checkbox(&mut config.show_grid, l10n::text(ui.ctx(), "Show grid"));
        }
        if !square || config.show_grid {
            ui.strong(l10n::text(ui.ctx(), "Price levels"));
            level_grid(
                ui,
                "Price level",
                &mut config.price_levels,
                grid_pickers(pickers, 0),
                new(max / 2.0),
                |level| gann_row(level, 0.0..=max),
            );
            ui.strong(l10n::text(ui.ctx(), "Time levels"));
            level_grid(
                ui,
                "Time level",
                &mut config.time_levels,
                grid_pickers(pickers, 1),
                new(max / 2.0),
                |level| gann_row(level, 0.0..=max),
            );
        }
        if square {
            ui.checkbox(&mut config.show_fan, l10n::text(ui.ctx(), "Show fan"));
            if config.show_fan {
                let ctx = ui.ctx().clone();
                ui.combo_select(
                    "gann_fan_direction",
                    &mut config.fan_direction,
                    [
                        GannFanDirection::Forward,
                        GannFanDirection::Reverse,
                        GannFanDirection::Both,
                    ],
                    |v| {
                        l10n::text(
                            &ctx,
                            match v {
                                GannFanDirection::Forward => "Forward",
                                GannFanDirection::Reverse => "Reverse",
                                GannFanDirection::Both => "Both",
                            },
                        )
                    },
                );
                ui.strong(l10n::text(ui.ctx(), "Fan lines"));
                level_grid(
                    ui,
                    "Fan line",
                    &mut config.fan_ratios,
                    grid_pickers(pickers, 2),
                    new(1.0),
                    |level| gann_row(level, 0.01..=100.0),
                );
            }
            ui.checkbox(&mut config.show_arcs, l10n::text(ui.ctx(), "Show arcs"));
            if config.show_arcs {
                ui.strong(l10n::text(ui.ctx(), "Arcs"));
                level_grid(
                    ui,
                    "Arc",
                    &mut config.arc_ratios,
                    grid_pickers(pickers, 3),
                    new(1.0),
                    |level| gann_row(level, 0.01..=GANN_MAX_ARC),
                );
            }
        } else {
            ui.checkbox(&mut config.show_angles, l10n::text(ui.ctx(), "Angles"));
        }
    }
    background(
        ui,
        &mut config.show_background,
        &mut config.background_opacity,
    );
    if square {
        price_per_bar(ui, coordinates);
    }
    if config != original {
        drawing.gann = Some(config);
    }
}

/// The price per bar between the first two staged anchors; typing one moves the second anchor's
/// price, on its side of the first (above when they are equal). The anchors of a fixed square are
/// its first unit, whose ratio is the whole square's.
fn price_per_bar(ui: &mut Ui, coordinates: &mut CoordinatesState) {
    let [first, second, ..] = coordinates.points.as_slice() else {
        return;
    };
    let number = |text: &str| text.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    let (Some(from), Some(to), Some(start), Some(end)) = (
        number(&first.price),
        number(&second.price),
        number(&first.bar_index),
        number(&second.bar_index),
    ) else {
        return;
    };
    let bars = (end - start).abs();
    if bars == 0.0 {
        return;
    }
    let mut ratio = (to - from).abs() / bars;
    let (exact, shown) = (ratio, significant(ratio));
    // Dragging steps in proportion to the ratio; a level square has no ratio to scale by.
    let speed = if ratio > 0.0 { ratio * 0.01 } else { 1e-6 };
    FormGrid::new("gann_price_per_bar").show(ui, |ui| {
        let label = ui.label(l10n::text(ui.ctx(), "Price per bar"));
        let changed = ui
            .add(
                egui::DragValue::new(&mut ratio)
                    .range(0.0..=f64::MAX)
                    .speed(speed)
                    .custom_formatter(|value, _| significant(value))
                    // The shown text stands for the exact ratio, so leaving the field without
                    // typing keeps the anchor where it is.
                    .custom_parser(|text| {
                        if text.trim() == shown {
                            Some(exact)
                        } else {
                            number(text)
                        }
                    }),
            )
            .labelled_by(label.id)
            .changed();
        ui.end_row();
        if changed {
            let side = if to < from { -1.0 } else { 1.0 };
            coordinates.points[1].price = (from + side * ratio * bars).to_string();
        }
    });
}

/// `value` with eight significant digits and no trailing zeros, so a ratio of a low-priced
/// instrument reads in full: 5, 0.0000012, 0.000000003.
fn significant(value: f64) -> String {
    let decimals = if value != 0.0 && value.is_finite() {
        (7.0 - value.abs().log10().floor()).max(0.0) as usize
    } else {
        0
    };
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

fn level_color(ui: &mut Ui, color: &mut [u8; 4], state: &mut ColorPickerState) -> bool {
    state.sel_color = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    if let Some(picked) = ColorPicker::new(state, ui.scope_id().with("level_color"))
        .show(ui)
        .inner
    {
        *color = picked.to_srgba_unmultiplied();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chart;
    use crate::drawings::ChartPoint;
    use crate::model::{BarData, Instrument};
    use crate::ui::dialogs::{DrawingId, DrawingPropertiesDialog, PropertiesTab};
    use egui::{Context, Event, Key, Modifiers, Pos2, Rect, Shape};

    fn frame(
        dialog: &mut DrawingPropertiesDialog,
        chart: &mut Chart,
        ctx: &Context,
        events: Vec<Event>,
    ) -> Vec<(String, Rect)> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                dialog.show_and_apply(ui.ctx(), chart);
            },
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Text(text) => {
                    Some((text.galley.text().to_owned(), text.visual_bounding_rect()))
                }
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    fn click(dialog: &mut DrawingPropertiesDialog, chart: &mut Chart, ctx: &Context, pos: Pos2) {
        frame(dialog, chart, ctx, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                dialog,
                chart,
                ctx,
                vec![Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    fn field(texts: &[(String, Rect)], label: &str) -> (String, Pos2) {
        let label = texts.iter().find(|(text, _)| text == label).expect(label).1;
        texts
            .iter()
            .find(|(text, rect)| {
                rect.left() > label.right()
                    && (rect.center().y - label.center().y).abs() < 4.0
                    && text.parse::<f64>().is_ok()
            })
            .map(|(text, rect)| (text.clone(), rect.center()))
            .expect("numeric field")
    }

    fn enter(
        dialog: &mut DrawingPropertiesDialog,
        chart: &mut Chart,
        ctx: &Context,
        label: &str,
        value: &str,
    ) {
        let texts = frame(dialog, chart, ctx, vec![]);
        click(dialog, chart, ctx, field(&texts, label).1);
        frame(dialog, chart, ctx, vec![Event::Text(value.into())]);
        for pressed in [true, false] {
            frame(
                dialog,
                chart,
                ctx,
                vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    /// Over 10 bars a ratio of 1.5 is 15 price units from the first anchor's 100: above it for a
    /// second anchor above or level, below it for one below. A ratio of 0 is level. Entering the
    /// field without typing keeps a ratio of 0.000000003, which the field shows.
    #[test]
    fn typing_a_price_per_bar_moves_the_second_anchor_on_its_side() {
        for tool in [Tool::GannSquare, Tool::GannFixed] {
            for (first, second, typed, want) in [
                (100.0, 120.0, "1.5", 115.0),
                (100.0, 80.0, "1.5", 85.0),
                (100.0, 100.0, "1.5", 115.0),
                (100.0, 80.0, "0", 100.0),
                (0.0000120, 0.00001203, "", 0.00001203),
            ] {
                let ctx = Context::default();
                let mut chart = Chart::new(BarData::new());
                let mut drawing = Drawing::new(7, tool);
                drawing.chart_points =
                    vec![ChartPoint::new(0.0, first), ChartPoint::new(10.0, second)];
                drawing.completed = true;
                // Collapsed grids keep the field inside the dialog's scroll area.
                drawing.gann = Some(GannConfig {
                    show_grid: false,
                    show_fan: false,
                    show_arcs: false,
                    ..GannConfig::for_tool(tool)
                });
                chart.drawings_mut().drawings.push(drawing);
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open_for_chart(&chart, DrawingId(7));
                dialog.active_tab = PropertiesTab::Inputs;
                frame(&mut dialog, &mut chart, &ctx, vec![]);
                let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                if typed.is_empty() {
                    assert_eq!(field(&texts, "Price per bar").0, "0.000000003", "{tool:?}");
                }
                enter(&mut dialog, &mut chart, &ctx, "Price per bar", typed);
                let points = &chart.drawings().drawings[0].chart_points;
                assert_eq!(
                    [
                        points[0].price,
                        points[1].price,
                        f64::from(points[1].bar_idx)
                    ],
                    [first, want, 10.0],
                    "{tool:?}, second anchor at {second}, typed {typed}"
                );
                dialog.close();
                frame(&mut dialog, &mut chart, &ctx, vec![]);
            }
        }
    }

    /// At a quarter-point tick, 12 target ticks are three points and six stop ticks are
    /// 1.5 points. Moving entry from 100 to 110 keeps those distances in either direction.
    #[test]
    fn position_inputs_keep_prices_ticks_and_staged_anchors_in_sync() {
        for (tool, side) in [(Tool::LongPos, 1.0), (Tool::ShortPos, -1.0)] {
            for anchors in [2, 3] {
                let ctx = Context::default();
                let mut chart = Chart::new(BarData::new());
                chart.set_instrument(Some(Instrument::new("TEST").with_tick_size(0.25)));
                let mut drawing = Drawing::new(7, tool);
                drawing.chart_points = vec![
                    ChartPoint::new(2.0, 100.0),
                    ChartPoint::new(8.0, 100.0 + side * 5.0),
                    ChartPoint::new(8.0, 100.0 - side * 5.0),
                ];
                drawing.chart_points.truncate(anchors);
                drawing.completed = true;
                chart.drawings_mut().drawings.push(drawing);
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open_for_chart(&chart, DrawingId(7));
                dialog.active_tab = PropertiesTab::Inputs;
                frame(&mut dialog, &mut chart, &ctx, vec![]);
                let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                assert_eq!(field(&texts, "Target (ticks)").0, "20");
                assert_eq!(chart.drawings().drawings[0].chart_points.len(), anchors);
                if anchors == 2 {
                    // Even a stop at the target's price is a new anchor, not an unchanged target.
                    enter(
                        &mut dialog,
                        &mut chart,
                        &ctx,
                        "Stop price",
                        &(100.0 + side * 5.0).to_string(),
                    );
                    assert_eq!(chart.drawings().drawings[0].chart_points.len(), 3);
                    dialog.close();
                    frame(&mut dialog, &mut chart, &ctx, vec![]);
                    assert_eq!(chart.drawings().drawings[0].chart_points.len(), 2);
                    dialog.open_for_chart(&chart, DrawingId(7));
                    dialog.active_tab = PropertiesTab::Inputs;
                }
                enter(&mut dialog, &mut chart, &ctx, "Target (ticks)", "12");
                enter(&mut dialog, &mut chart, &ctx, "Stop (ticks)", "6");
                enter(&mut dialog, &mut chart, &ctx, "Entry price", "110");
                let points = &chart.drawings().drawings[0].chart_points;
                assert_eq!(
                    points.iter().map(|p| p.price).collect::<Vec<_>>(),
                    [110.0, 110.0 + side * 3.0, 110.0 - side * 1.5]
                );
                assert_eq!(
                    points.iter().map(|p| p.bar_idx).collect::<Vec<_>>(),
                    [2.0, 8.0, 8.0]
                );
                assert!(!chart.can_undo(), "preview retains history");
                enter(
                    &mut dialog,
                    &mut chart,
                    &ctx,
                    "Target price",
                    &(110.0 + side).to_string(),
                );
                let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                assert_eq!(field(&texts, "Target (ticks)").0, "4");
                assert_eq!(field(&texts, "Stop (ticks)").0, "6");
                // A Coordinates edit is reflected when returning to Inputs.
                dialog.coordinates.points[2].price = (110.0 - side * 1.25).to_string();
                let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                assert_eq!(field(&texts, "Stop (ticks)").0, "5");
                let ok = texts
                    .iter()
                    .find(|(text, _)| text == "Ok")
                    .unwrap()
                    .1
                    .center();
                click(&mut dialog, &mut chart, &ctx, ok);
                assert!(!dialog.is_open);
                assert!(chart.undo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0].price, 100.0);
                assert_eq!(chart.drawings().drawings[0].chart_points.len(), anchors);
                assert!(chart.redo());
                assert_eq!(
                    chart.drawings().drawings[0].chart_points[1].price,
                    110.0 + side
                );
                assert!(chart.undo());
                assert!(!chart.can_undo());
                for tick in [None, Some(0.0), Some(-0.25), Some(f64::NAN)] {
                    let mut instrument = Instrument::new("TEST");
                    instrument.tick_size = tick;
                    chart.set_instrument(Some(instrument));
                    dialog.open_for_chart(&chart, DrawingId(7));
                    dialog.active_tab = PropertiesTab::Inputs;
                    frame(&mut dialog, &mut chart, &ctx, vec![]);
                    let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                    assert!(
                        texts
                            .iter()
                            .any(|(text, _)| text == "Tick size unavailable")
                    );
                    enter(&mut dialog, &mut chart, &ctx, "Entry price", "105");
                    assert_eq!(chart.drawings().drawings[0].chart_points[0].price, 105.0);
                    dialog.close();
                    frame(&mut dialog, &mut chart, &ctx, vec![]);
                    assert_eq!(chart.drawings().drawings[0].chart_points[0].price, 100.0);
                }
                chart.set_instrument(Some(Instrument::new("TEST").with_tick_size(0.1)));
                dialog.open_for_chart(&chart, DrawingId(7));
                dialog.active_tab = PropertiesTab::Inputs;
                enter(
                    &mut dialog,
                    &mut chart,
                    &ctx,
                    "Target price",
                    &(100.0 + side * 0.3).to_string(),
                );
                let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
                assert_eq!(
                    field(&texts, "Target (ticks)").0,
                    "3",
                    "decimal tick rounding"
                );
                dialog.close();
                frame(&mut dialog, &mut chart, &ctx, vec![]);
            }
        }
    }
}
