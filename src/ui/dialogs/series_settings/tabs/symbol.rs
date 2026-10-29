//! Symbol tab - chart type, price source, and line/candle style settings.

use crate::chart::series::SeriesSettings;
use crate::localization as l10n;
use crate::model::{
    BaselineLevel, BoxSizeMethod, ChartType, LinePlotStyle, LineStyle, PriceBasedSource,
    PriceSource,
};
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid};
use egui::{Color32, RichText, Ui};

/// Symbol tab renderer - shows chart type and style settings
pub struct SymbolTab;

impl SymbolTab {
    /// Show the symbol tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

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
                    Self::price_source_dropdown(ui, &mut settings.price_source);
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
                    let label = ui.label(l10n::text(ui.ctx(), "Marker radius"));
                    ui.add(
                        egui::DragValue::new(&mut styles.marker_radius)
                            .range(0.0..=20.0)
                            .suffix(l10n::text(ui.ctx(), " pt")),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                }
                ChartType::Area => {
                    Self::line_style(ui, &mut styles.area.line, "line_style");
                    Self::fill_row(ui, "Top fill", &mut styles.area.top_fill);
                    Self::fill_row(ui, "Bottom fill", &mut styles.area.bottom_fill);
                }
                ChartType::HlcArea => {
                    let label = ui.label(l10n::text(ui.ctx(), "Fill opacity"));
                    ui.add(
                        egui::DragValue::new(&mut styles.hlc_area.fill_opacity)
                            .range(0.0..=1.0)
                            .speed(0.01),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                    let label = ui.label(l10n::text(ui.ctx(), "Line width"));
                    ui.add(
                        egui::DragValue::new(&mut styles.hlc_area.line_width)
                            .range(0.0..=10.0)
                            .speed(0.1)
                            .suffix(l10n::text(ui.ctx(), " pt")),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                    Self::candle_colors_section(ui, settings);
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
                }
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

    fn fill_row(ui: &mut Ui, label: &str, color: &mut Color32) {
        ui.label(l10n::text(ui.ctx(), label));
        Self::color_picker(ui, color);
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

    pub(super) fn color_picker(ui: &mut Ui, color: &mut Color32) {
        let id = ui.next_auto_id();
        let mut state = ui
            .data(|data| data.get_temp::<ColorPickerState>(id))
            .unwrap_or_else(|| ColorPickerState::new(*color));
        if !state.is_open {
            state.sel_color = *color;
        }
        if let Some(selected) = ColorPicker::new(&mut state, id).show(ui).inner {
            *color = selected;
        }
        ui.data_mut(|data| data.insert_temp(id, state));
    }

    fn optional_color_picker(ui: &mut Ui, color: &mut Option<Color32>, default: Color32) {
        let mut selected = color.unwrap_or(default);
        ui.add_enabled_ui(color.is_some(), |ui| Self::color_picker(ui, &mut selected));
        if color.is_some() {
            *color = Some(selected);
        }
    }
}
