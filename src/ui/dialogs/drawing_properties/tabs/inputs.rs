//! Inputs that correspond to the selected tool's saved drawing configuration.

use crate::drawings::{
    Drawing, DrawingToolType as Tool, FibonacciConfig, FibonacciLevel, GannConfig, LineStyle,
    PitchforkConfig, PitchforkLevel, PositionSize, RegressionTrendConfig, VolumeProfileConfig,
};
use crate::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid, buttons::Button};
use egui::Ui;

pub(crate) struct InputsTab;

impl InputsTab {
    pub(crate) fn available(tool: Tool) -> bool {
        matches!(
            tool,
            Tool::FibonacciRetracement
                | Tool::FibonacciChannel
                | Tool::FibonacciExtension
                | Tool::FibonacciWedge
                | Tool::Pitchfork
                | Tool::SchiffPitchfork
                | Tool::ModifiedSchiffPitchfork
                | Tool::InsidePitchfork
                | Tool::GannBox
                | Tool::GannSquare
                | Tool::GannFixed
                | Tool::LongPos
                | Tool::ShortPos
                | Tool::FixedRangeVolumeProfile
                | Tool::AnchoredVolumeProfile
                | Tool::RegressionTrend
                | Tool::ParallelChannel
        )
    }

    pub(crate) fn show(ui: &mut Ui, drawing: &mut Drawing, pickers: &mut Vec<ColorPickerState>) {
        ui.space_md();
        match drawing.tool_type {
            Tool::FibonacciRetracement
            | Tool::FibonacciExtension
            | Tool::FibonacciChannel
            | Tool::FibonacciWedge => fibonacci(ui, drawing, pickers),
            Tool::Pitchfork
            | Tool::SchiffPitchfork
            | Tool::ModifiedSchiffPitchfork
            | Tool::InsidePitchfork => pitchfork(ui, drawing, pickers),
            Tool::GannBox | Tool::GannSquare | Tool::GannFixed => {
                let mut config = drawing.gann.clone().unwrap_or_else(|| {
                    if drawing.tool_type == Tool::GannFixed {
                        GannConfig {
                            price_levels: vec![0.0, 0.25, 0.5, 0.75, 1.0],
                            time_levels: vec![0.0, 0.25, 0.5, 0.75, 1.0],
                        }
                    } else {
                        GannConfig::default()
                    }
                });
                let original = config.clone();
                fractions(ui, "Price divisions", &mut config.price_levels);
                fractions(ui, "Time divisions", &mut config.time_levels);
                if config != original {
                    drawing.gann = Some(config);
                }
            }
            Tool::LongPos | Tool::ShortPos => {
                let mut size = drawing.position_size.unwrap_or_default();
                let original = size;
                let mut mode = match size {
                    PositionSize::Quantity(_) => 0,
                    PositionSize::RiskAmount(_) => 1,
                    PositionSize::RiskPercent { .. } => 2,
                };
                FormGrid::new("position_inputs").show(ui, |ui| {
                    ui.label("Size by");
                    ui.combo_select("position_size_mode", &mut mode, [0, 1, 2], |v| {
                        ["Quantity", "Risk amount", "Risk percent"][*v].into()
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
                });
                if size != original {
                    drawing.position_size = Some(size);
                }
            }
            Tool::FixedRangeVolumeProfile | Tool::AnchoredVolumeProfile => {
                let config = drawing.volume_profile.unwrap_or_default();
                let mut rows = config.rows();
                let mut percent = config.value_area() * 100.0;
                FormGrid::new("profile_inputs").show(ui, |ui| {
                    ui.label("Rows");
                    ui.add(egui::DragValue::new(&mut rows).range(VolumeProfileConfig::ROWS));
                    ui.end_row();
                    number(
                        ui,
                        "Value area (%)",
                        &mut percent,
                        f64::MIN_POSITIVE..=100.0,
                    );
                });
                if rows != config.rows() || percent != config.value_area() * 100.0 {
                    drawing.volume_profile = VolumeProfileConfig::new(rows, percent / 100.0);
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
            Tool::ParallelChannel => {
                ui.checkbox(&mut drawing.channel_midline, "Show middle line");
            }
            _ => {}
        }
    }
}

fn number(ui: &mut Ui, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(range).speed(0.1));
    ui.end_row();
}

fn background(ui: &mut Ui, show: &mut bool, opacity: &mut u8) {
    ui.horizontal(|ui| {
        ui.checkbox(show, "Background");
        ui.add_enabled(
            *show,
            egui::DragValue::new(opacity)
                .range(0..=255)
                .prefix("Opacity "),
        );
    });
}

fn fibonacci(ui: &mut Ui, drawing: &mut Drawing, pickers: &mut Vec<ColorPickerState>) {
    let mut config = drawing.fib_config.clone().unwrap_or_else(|| {
        if drawing.tool_type == Tool::FibonacciExtension {
            FibonacciConfig::extension_default()
        } else {
            FibonacciConfig::default()
        }
    });
    let original = config.clone();
    pickers.resize_with(config.levels.len(), ColorPickerState::default);
    let wedge = drawing.tool_type == Tool::FibonacciWedge;
    let channel = drawing.tool_type == Tool::FibonacciChannel;
    ui.strong("Fibonacci levels");
    ui.label(if wedge {
        "Visible · Radius ratio"
    } else {
        "Visible · Ratio · Color"
    });
    let mut remove = None;
    for (i, level) in config.levels.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut level.visible, "");
                ui.add(
                    egui::DragValue::new(&mut level.value)
                        .speed(0.001)
                        .max_decimals(6),
                );
                if !wedge {
                    if !channel {
                        ui.add(egui::TextEdit::singleline(&mut level.label).desired_width(72.0));
                    }
                    level_color(ui, &mut level.color, &mut pickers[i]);
                    if !channel {
                        ui.checkbox(&mut level.show_price, "Price");
                    }
                }
                if Button::new("Remove").ghost().show(ui).clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        config.levels.remove(i);
        pickers.remove(i);
    }
    if Button::new("Add level").secondary().show(ui).clicked() {
        config
            .levels
            .push(FibonacciLevel::new(1.0, "100%", drawing.color));
    }
    if !wedge {
        background(
            ui,
            &mut config.show_background,
            &mut config.background_opacity,
        );
        ui.horizontal(|ui| {
            if drawing.tool_type == Tool::FibonacciRetracement {
                ui.checkbox(&mut config.reverse, "Reverse");
            }
            ui.checkbox(&mut config.extend_left, "Extend left");
            ui.checkbox(&mut config.extend_right, "Extend right");
        });
    }
    if matches!(
        drawing.tool_type,
        Tool::FibonacciRetracement | Tool::FibonacciExtension
    ) {
        ui.checkbox(&mut config.logarithmic, "Logarithmic levels");
    }
    if config != original {
        drawing.fib_config = Some(config);
    }
}

fn pitchfork(ui: &mut Ui, drawing: &mut Drawing, pickers: &mut Vec<ColorPickerState>) {
    let mut config = drawing.pitchfork.clone().unwrap_or_else(|| {
        let mut config = PitchforkConfig::default();
        for level in &mut config.levels {
            level.color = drawing.color;
        }
        config
    });
    let original = config.clone();
    pickers.resize_with(config.levels.len(), ColorPickerState::default);
    ui.strong("Pitchfork levels");
    ui.label("Multiples of the median-to-tine distance");
    let mut remove = None;
    for (i, level) in config.levels.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut level.visible, "");
                ui.add(
                    egui::DragValue::new(&mut level.value)
                        .range(0.0..=f32::MAX)
                        .speed(0.01),
                );
                level_color(ui, &mut level.color, &mut pickers[i]);
                ui.combo_select(
                    "level_style",
                    &mut level.line_style,
                    LineStyle::all().iter().copied(),
                    |v| v.name().to_owned(),
                );
                if Button::new("Remove").ghost().show(ui).clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        config.levels.remove(i);
        pickers.remove(i);
    }
    if Button::new("Add level").secondary().show(ui).clicked() {
        config.levels.push(PitchforkLevel {
            value: 2.0,
            visible: true,
            color: drawing.color,
            line_style: LineStyle::Solid,
        });
    }
    background(
        ui,
        &mut config.show_background,
        &mut config.background_opacity,
    );
    if config != original {
        drawing.pitchfork = Some(config);
    }
}

fn fractions(ui: &mut Ui, title: &str, levels: &mut Vec<f32>) {
    ui.strong(title);
    let mut remove = None;
    let columns = ((ui.available_width() / 96.0) as usize).clamp(1, 3);
    egui::Grid::new(title)
        .num_columns(columns * 2)
        .show(ui, |ui| {
            for (i, value) in levels.iter_mut().enumerate() {
                ui.add(egui::DragValue::new(value).range(0.0..=1.0).speed(0.001));
                if Button::icon(&crate::icons::icons::TRASH)
                    .ghost()
                    .tooltip("Remove division")
                    .show(ui)
                    .clicked()
                {
                    remove = Some(i);
                }
                if (i + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    if let Some(i) = remove {
        levels.remove(i);
    }
    if Button::new(format!("Add {}", title.to_lowercase()))
        .secondary()
        .show(ui)
        .clicked()
    {
        levels.push(0.5);
    }
    ui.space_sm();
}

fn level_color(ui: &mut Ui, color: &mut [u8; 4], state: &mut ColorPickerState) {
    state.sel_color = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    if let Some(picked) = ColorPicker::new(state, ui.scope_id().with("level_color"))
        .show(ui)
        .inner
    {
        *color = picked.to_srgba_unmultiplied();
    }
}
