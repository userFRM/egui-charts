//! Host-owned feeds attached as native comparison series, and a second instrument drawn as
//! candles in a pane of its own.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{ChartType, LinePlotStyle, SeriesScale, Timeframe};
use egui_charts::services::SymbolExpression;
use egui_charts::ui::top_toolbar::{CompareAction, ComparePlacement, CompareSymbol};
use egui_charts::widget::{Chart, PaneId, SeriesId, SeriesOptions};
use std::sync::Arc;

/// An isolated draft: closing the editor never mutates the chart.
pub(super) struct ComparisonSettings {
    id: SeriesId,
    options: SeriesOptions,
    scale: egui_charts::scales::PriceScale,
    original_scale: egui_charts::scales::PriceScale,
    independent_scale: SeriesScale,
    overlay_scale: SeriesScale,
}

impl ComparisonSettings {
    /// The line, bar, area and last-price settings of the chosen chart type. Taking over a
    /// style starts from what the series draws without it, so the series looks the same until
    /// a setting changes.
    fn style_section(&mut self, ui: &mut egui::Ui) {
        use egui_charts::model::{AreaStyle, LastPriceAnimation};
        use egui_charts::ui_kit::{FormGrid, ext::UiExt};
        let options = &mut self.options;
        let kind = options.chart_type;
        let line_only = matches!(
            kind,
            ChartType::Line | ChartType::StepLine | ChartType::LineWithMarkers
        );
        let pulses = line_only || matches!(kind, ChartType::Area | ChartType::Baseline);
        if !pulses && kind != ChartType::Bars {
            return;
        }
        let own_line = series_line(options, options.color);
        ui.collapsing("Line, bars and area", |ui| {
            if line_only {
                let mut custom = options.line_style.is_some();
                if ui.checkbox(&mut custom, "Independent line style").changed() {
                    options.line_style = custom.then(|| own_line.clone());
                }
            }
            if kind == ChartType::Area {
                let mut custom = options.area_style.is_some();
                if ui.checkbox(&mut custom, "Independent area style").changed() {
                    let color = options.color;
                    options.area_style = custom.then(|| AreaStyle {
                        line: options.line_style.clone().unwrap_or(own_line),
                        top_fill: egui::Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            115,
                        ),
                        bottom_fill: egui::Color32::TRANSPARENT,
                        ..Default::default()
                    });
                }
            }
            if kind == ChartType::Bars {
                ui.checkbox(&mut options.bars_style.open_visible, "Show open");
                ui.checkbox(&mut options.bars_style.thin_bars, "Thin bars");
            }
            FormGrid::new("comparison_type_style").show(ui, |ui| {
                if let Some(style) = options.line_style.as_mut().filter(|_| line_only) {
                    line_style_rows(ui, style, kind != ChartType::StepLine);
                }
                if let Some(style) = options
                    .area_style
                    .as_mut()
                    .filter(|_| kind == ChartType::Area)
                {
                    line_style_rows(ui, &mut style.line, true);
                    for (label, fill) in [
                        ("Top fill", &mut style.top_fill),
                        ("Bottom fill", &mut style.bottom_fill),
                    ] {
                        let label = ui.label(label);
                        ui.color_edit_button_srgba(fill).labelled_by(label.id);
                        ui.end_row();
                    }
                    ui.checkbox(&mut style.relative_gradient, "Relative gradient");
                    ui.end_row();
                    ui.checkbox(&mut style.invert_filled_area, "Invert fill");
                    ui.end_row();
                }
                if pulses {
                    let label = ui.label("Last price animation");
                    ui.label_inputs_by(label.id, |ui| {
                        ui.combo_select(
                            "last_price_animation",
                            &mut options.last_price_animation,
                            [
                                LastPriceAnimation::Disabled,
                                LastPriceAnimation::Continuous,
                                LastPriceAnimation::OnDataUpdate,
                            ],
                            |animation| match animation {
                                LastPriceAnimation::Disabled => "Off".into(),
                                LastPriceAnimation::Continuous => "Continuous".into(),
                                LastPriceAnimation::OnDataUpdate => "On each update".into(),
                            },
                        );
                    });
                    ui.end_row();
                }
            });
        });
    }

    fn show(&mut self, ctx: &egui::Context, chart: &Chart, open: &mut bool) -> bool {
        use egui_charts::model::{LineStyle, PriceLineSource, SeriesDisplayFormat, SeriesFormat};
        use egui_charts::scales::PriceScaleMode;
        use egui_charts::ui_kit::{FormGrid, dialog::DialogFooter, ext::UiExt, modal::Modal};
        let mut apply = false;
        let mut cancelled = false;
        Modal::new(
            "comparison_settings",
            format!("{} · Settings", self.options.name),
        )
        .width(520.0)
        .height(600.0)
        .show(ctx, open, |ui| {
            let height = (ui.available_height() - 64.0).max(0.0);
            egui::ScrollArea::both()
                .id_salt("comparison_content")
                .max_height(height)
                .show(ui, |ui| {
                    FormGrid::new("comparison_style").show(ui, |ui| {
                        let label = ui.label("Chart type");
                        ui.label_inputs_by(label.id, |ui| {
                            ui.combo_select(
                                "type",
                                &mut self.options.chart_type,
                                // Added series draw neither footprints nor volume profiles.
                                ChartType::all().iter().copied().filter(|kind| {
                                    !kind.is_time_independent()
                                        && !matches!(
                                            kind,
                                            ChartType::TimePriceOpportunity
                                                | ChartType::VolumeFootprint
                                                | ChartType::SessionVolumeProfile
                                        )
                                }),
                                |kind| kind.name().to_owned(),
                            );
                        });
                        ui.end_row();
                        for (label, color) in [
                            ("Colour", &mut self.options.color),
                            ("Rising colour", &mut self.options.up_color),
                            ("Falling colour", &mut self.options.down_color),
                        ] {
                            let label = ui.label(label);
                            ui.color_edit_button_srgba(color).labelled_by(label.id);
                            ui.end_row();
                        }
                        let label = ui.label("Line width");
                        ui.add(egui::DragValue::new(&mut self.options.width).range(0.1..=10.0))
                            .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Candle border width");
                        ui.add(
                            egui::DragValue::new(&mut self.options.candle_border_width)
                                .range(0.0..=10.0),
                        )
                        .labelled_by(label.id);
                        ui.end_row();
                    });
                    ui.collapsing("Markers and baseline", |ui| {
                        FormGrid::new("comparison_markers").show(ui, |ui| {
                            let mut markers = self.options.marker_radius.is_some();
                            if ui.checkbox(&mut markers, "Custom point radius").changed() {
                                self.options.marker_radius = markers.then_some(3.0);
                            }
                            if let Some(radius) = &mut self.options.marker_radius {
                                ui.add(egui::DragValue::new(radius).range(0.0..=20.0));
                            }
                            ui.end_row();
                            let label = ui.label("Crosshair marker radius");
                            ui.add(
                                egui::DragValue::new(&mut self.options.crosshair_marker_radius)
                                    .range(0.0..=20.0),
                            )
                            .labelled_by(label.id);
                            ui.end_row();
                            optional_color(
                                ui,
                                "Crosshair fill",
                                &mut self.options.crosshair_marker_fill,
                                self.options.color,
                            );
                            optional_color(
                                ui,
                                "Crosshair border",
                                &mut self.options.crosshair_marker_border_color,
                                egui::Color32::WHITE,
                            );
                            let label = ui.label("Crosshair border width");
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.options.crosshair_marker_border_width,
                                )
                                .range(0.0..=10.0),
                            )
                            .labelled_by(label.id);
                            ui.end_row();
                            let mut base = self.options.columns_base.is_some();
                            if ui.checkbox(&mut base, "Column base").changed() {
                                self.options.columns_base = base.then_some(0.0);
                            }
                            if let Some(value) = &mut self.options.columns_base {
                                ui.add(egui::DragValue::new(value));
                            }
                            ui.end_row();
                        });
                        if self.options.chart_type == ChartType::Baseline {
                            let mut custom = self.options.baseline_style.is_some();
                            if ui
                                .checkbox(&mut custom, "Independent baseline style")
                                .changed()
                            {
                                self.options.baseline_style = custom.then(|| {
                                    let fill = |color: egui::Color32| {
                                        egui::Color32::from_rgba_unmultiplied(
                                            color.r(),
                                            color.g(),
                                            color.b(),
                                            30,
                                        )
                                    };
                                    egui_charts::model::BaselineStyle {
                                        above: series_line(&self.options, self.options.up_color),
                                        below: series_line(&self.options, self.options.down_color),
                                        above_fill: fill(self.options.up_color),
                                        below_fill: fill(self.options.down_color),
                                        ..Default::default()
                                    }
                                });
                            }
                            if let Some(style) = &mut self.options.baseline_style {
                                FormGrid::new("comparison_baseline").show(ui, |ui| {
                                    for (label, line, fill, at_base) in [
                                        (
                                            "Above",
                                            &mut style.above,
                                            &mut style.above_fill,
                                            &mut style.above_fill_at_base,
                                        ),
                                        (
                                            "Below",
                                            &mut style.below,
                                            &mut style.below_fill,
                                            &mut style.below_fill_at_base,
                                        ),
                                    ] {
                                        ui.label(format!("{label} line"));
                                        ui.horizontal(|ui| {
                                            ui.color_edit_button_srgba(&mut line.color);
                                            ui.add(
                                                egui::DragValue::new(&mut line.width)
                                                    .range(0.0..=10.0),
                                            );
                                            ui.combo_select(
                                                label,
                                                &mut line.style,
                                                [
                                                    LineStyle::Solid,
                                                    LineStyle::Dashed,
                                                    LineStyle::Dotted,
                                                ],
                                                |v| format!("{v:?}"),
                                            );
                                        });
                                        ui.end_row();
                                        ui.label(format!("{label} fill"));
                                        ui.color_edit_button_srgba(fill);
                                        ui.end_row();
                                        optional_color(
                                            ui,
                                            &format!("{label} fill at base"),
                                            at_base,
                                            *fill,
                                        );
                                    }
                                });
                            }
                        }
                    });
                    self.style_section(ui);
                    ui.separator();
                    ui.checkbox(
                        &mut self.options.style.last_value_visible,
                        "Last-value label",
                    );
                    ui.checkbox(&mut self.options.style.price_line_visible, "Price line");
                    FormGrid::new("comparison_lines").show(ui, |ui| {
                        let label = ui.label("Value source");
                        ui.label_inputs_by(label.id, |ui| {
                            ui.combo_select(
                                "value_source",
                                &mut self.options.style.price_line_source,
                                [PriceLineSource::LastBar, PriceLineSource::LastVisible],
                                |v| match v {
                                    PriceLineSource::LastBar => "Last bar".into(),
                                    PriceLineSource::LastVisible => "Last visible bar".into(),
                                },
                            );
                        });
                        ui.end_row();
                        optional_color(
                            ui,
                            "Price line colour",
                            &mut self.options.style.price_line_color,
                            self.options.color,
                        );
                        let label = ui.label("Price line width");
                        ui.add(
                            egui::DragValue::new(&mut self.options.style.price_line_width)
                                .range(0.1..=10.0),
                        )
                        .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Price line pattern");
                        ui.label_inputs_by(label.id, |ui| {
                            ui.combo_select(
                                "price_line_pattern",
                                &mut self.options.style.price_line_style,
                                [LineStyle::Solid, LineStyle::Dashed, LineStyle::Dotted],
                                |v| format!("{v:?}"),
                            );
                        });
                        ui.end_row();
                    });
                    ui.checkbox(&mut self.options.style.base_line_visible, "Base line");
                    ui.collapsing("Number format", |ui| {
                        let mut custom = self.options.style.display_format.is_some();
                        if ui.checkbox(&mut custom, "Custom number format").changed() {
                            self.options.style.display_format =
                                custom.then(SeriesDisplayFormat::default);
                        }
                        if let Some(format) = &mut self.options.style.display_format {
                            FormGrid::new("comparison_format").show(ui, |ui| {
                                let label = ui.label("Units");
                                ui.label_inputs_by(label.id, |ui| {
                                    ui.combo_select(
                                        "units",
                                        &mut format.kind,
                                        [
                                            SeriesFormat::Price,
                                            SeriesFormat::Volume,
                                            SeriesFormat::Percentage,
                                        ],
                                        |v| format!("{v:?}"),
                                    );
                                });
                                ui.end_row();
                                let label = ui.label("Decimals");
                                ui.add(egui::DragValue::new(&mut format.precision).range(0..=12))
                                    .labelled_by(label.id);
                                ui.end_row();
                                let label = ui.label("Minimum movement");
                                ui.add(
                                    egui::DragValue::new(&mut format.min_move)
                                        .range(0.00000001..=1e10)
                                        .speed(0.01),
                                )
                                .labelled_by(label.id);
                                ui.end_row();
                            });
                        }
                    });
                    ui.separator();
                    FormGrid::new("comparison_scale").show(ui, |ui| {
                        let label = ui.label("Price scale");
                        if chart.series_pane(self.id) == Some(PaneId::MAIN) {
                            ui.label_inputs_by(label.id, |ui| {
                                if ui.combo_select(
                                    "scale",
                                    &mut self.options.scale,
                                    [
                                        SeriesScale::Right,
                                        SeriesScale::Left,
                                        self.independent_scale,
                                        self.overlay_scale,
                                    ],
                                    |v| match v {
                                        SeriesScale::Right => "Right".into(),
                                        SeriesScale::Left => "Left".into(),
                                        SeriesScale::Separate(_) => "New price scale".into(),
                                        SeriesScale::Overlay(_) => "No axis labels".into(),
                                    },
                                ) {
                                    self.scale = chart
                                        .shared_price_scale(self.options.scale)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            egui_charts::scales::PriceScale::new(Default::default())
                                        });
                                    self.original_scale = self.scale.clone();
                                }
                            });
                        } else {
                            ui.label("Pane price scale");
                        }
                        ui.end_row();
                        let label = ui.label("Scale mode");
                        ui.label_inputs_by(label.id, |ui| {
                            ui.combo_select(
                                "scale_mode",
                                &mut self.scale.options_mut().mode,
                                [
                                    PriceScaleMode::Normal,
                                    PriceScaleMode::Logarithmic,
                                    PriceScaleMode::Percentage,
                                    PriceScaleMode::IndexedTo100,
                                ],
                                |v| v.to_string(),
                            );
                        });
                        ui.end_row();
                    });
                    ui.checkbox(&mut self.scale.options_mut().invert_scale, "Invert scale");
                    let mut automatic = !self.scale.is_manual();
                    if ui.checkbox(&mut automatic, "Auto scale").changed() {
                        if automatic {
                            self.scale.reset();
                        } else {
                            let range = self.scale.range();
                            self.scale.set_range(range.min, range.max);
                        }
                    }
                    ui.weak("Series on the same price scale share its mode and direction.");
                });
            let (ok, cancel) = DialogFooter::new("Ok").secondary("Cancel").show(ui);
            apply = ok;
            cancelled = cancel;
        });
        if cancelled {
            *open = false;
        }
        apply
    }
}

fn series_line(options: &SeriesOptions, color: egui::Color32) -> LinePlotStyle {
    LinePlotStyle {
        color,
        width: options.width,
        ..Default::default()
    }
}

/// The colour, width, pattern and, where the chart type draws one, the curve of a series line.
fn line_style_rows(ui: &mut egui::Ui, style: &mut LinePlotStyle, curves: bool) {
    use egui_charts::model::{LineStyle, LineType};
    use egui_charts::ui_kit::ext::UiExt;
    let label = ui.label("Line colour");
    ui.color_edit_button_srgba(&mut style.color)
        .labelled_by(label.id);
    ui.end_row();
    let label = ui.label("Line thickness");
    ui.add(egui::DragValue::new(&mut style.width).range(0.1..=10.0))
        .labelled_by(label.id);
    ui.end_row();
    let label = ui.label("Line pattern");
    ui.label_inputs_by(label.id, |ui| {
        ui.combo_select(
            "line_pattern",
            &mut style.style,
            LineStyle::all().iter().copied(),
            |pattern| pattern.name().to_owned(),
        );
    });
    ui.end_row();
    if curves {
        let label = ui.label("Line shape");
        ui.label_inputs_by(label.id, |ui| {
            ui.combo_select(
                "line_shape",
                &mut style.line_type,
                [LineType::Simple, LineType::Curved],
                |shape| match shape {
                    LineType::Simple => "Straight".into(),
                    LineType::Curved => "Curved".into(),
                },
            );
        });
        ui.end_row();
    }
}

fn optional_color(
    ui: &mut egui::Ui,
    label: &str,
    color: &mut Option<egui::Color32>,
    default: egui::Color32,
) {
    let mut custom = color.is_some();
    if ui.checkbox(&mut custom, label).changed() {
        *color = custom.then_some(default);
    }
    if let Some(color) = color {
        ui.color_edit_button_srgba(color);
    }
    ui.end_row();
}

pub(super) struct Spread {
    expression: SymbolExpression,
    id: SeriesId,
    feeds: Vec<Arc<LiveFeed>>,
    snapshot: Option<Vec<super::live::ComparisonSnapshot>>,
    replay_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl Spread {
    pub(super) fn loading(&self, chart: &Chart) -> bool {
        chart.series_order(self.id).is_some() && self.feeds.iter().any(|feed| feed.loading())
    }

    fn new(expression: SymbolExpression, id: SeriesId) -> Self {
        let now = chrono::Utc::now();
        let feeds = expression
            .symbols()
            .into_iter()
            .map(|symbol| Arc::new(LiveFeed::new(symbol, now)))
            .collect();
        Self {
            expression,
            id,
            feeds,
            snapshot: None,
            replay_time: None,
        }
    }

    pub(super) fn copy(&self, chart: &mut Chart, source: &Chart) -> Option<Self> {
        Some(Self {
            expression: self.expression.clone(),
            snapshot: None,
            replay_time: None,
            id: copy_series(chart, source, self.id)?,
            feeds: self
                .feeds
                .iter()
                .map(|feed| Arc::new(feed.fork()))
                .collect(),
        })
    }
}

/// Host series metadata kept beside the library's chart snapshots.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedComparison {
    symbol: String,
    #[serde(with = "SavedOptions")]
    options: SeriesOptions,
    visible: bool,
    pane: usize,
    scale: egui_charts::scales::PriceScale,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "SeriesOptions")]
struct SavedOptions {
    name: String,
    chart_type: ChartType,
    color: egui::Color32,
    up_color: egui::Color32,
    down_color: egui::Color32,
    #[serde(default)]
    candle_border_width: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline_style: Option<egui_charts::model::BaselineStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_style: Option<egui_charts::model::LinePlotStyle>,
    #[serde(default)]
    bars_style: egui_charts::model::BarsStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    area_style: Option<egui_charts::model::AreaStyle>,
    #[serde(default)]
    last_price_animation: egui_charts::model::LastPriceAnimation,
    width: f32,
    #[serde(default)]
    marker_radius: Option<f32>,
    #[serde(default)]
    crosshair_marker_radius: f32,
    #[serde(default)]
    crosshair_marker_fill: Option<egui::Color32>,
    #[serde(default)]
    crosshair_marker_border_color: Option<egui::Color32>,
    #[serde(default = "default_crosshair_marker_border_width")]
    crosshair_marker_border_width: f32,
    scale: SeriesScale,
    columns_base: Option<f64>,
    #[serde(with = "SavedStyle")]
    style: egui_charts::model::SeriesStyle,
}

fn default_crosshair_marker_border_width() -> f32 {
    SeriesOptions::default().crosshair_marker_border_width
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "egui_charts::model::SeriesStyle")]
struct SavedStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_format: Option<egui_charts::model::SeriesDisplayFormat>,
    last_value_visible: bool,
    price_line_visible: bool,
    #[serde(with = "SavedPriceLineSource")]
    price_line_source: egui_charts::model::PriceLineSource,
    price_line_color: Option<egui::Color32>,
    price_line_width: f32,
    price_line_style: egui_charts::model::LineStyle,
    base_line_visible: bool,
    #[serde(default = "default_base_line_color")]
    base_line_color: egui::Color32,
    #[serde(default = "default_base_line_width")]
    base_line_width: f32,
    #[serde(default)]
    base_line_style: egui_charts::model::LineStyle,
    #[serde(skip)]
    autoscale: Option<egui_charts::model::AutoscaleProvider>,
    hit_tolerance: f32,
    #[serde(default)]
    alerts: egui_charts::model::SeriesAlerts,
}

fn default_base_line_color() -> egui::Color32 {
    egui_charts::model::SeriesStyle::default().base_line_color
}

fn default_base_line_width() -> f32 {
    egui_charts::model::SeriesStyle::default().base_line_width
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "egui_charts::model::PriceLineSource")]
enum SavedPriceLineSource {
    LastBar,
    LastVisible,
}

pub(super) fn capture(
    chart: &Chart,
    comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
    spreads: &[Spread],
) -> Result<Vec<SavedComparison>, String> {
    let panes = chart.panes();
    comparisons
        .iter()
        .map(|(symbol, id, _)| (symbol.clone(), *id))
        .chain(
            spreads
                .iter()
                .map(|spread| (spread.expression.to_string(), spread.id)),
        )
        .filter_map(|(symbol, id)| {
            let (_, options) = chart.series().find(|(current, _)| *current == id)?;
            Some(if options.style.autoscale.is_some() {
                Err("Could not save a comparison with a custom autoscale callback".into())
            } else {
                Ok(SavedComparison {
                    symbol,
                    options: options.clone(),
                    visible: chart.is_series_visible(id),
                    pane: chart
                        .series_pane(id)
                        .and_then(|pane| panes.iter().position(|p| *p == pane))
                        .unwrap_or(0),
                    scale: chart
                        .series_price_scale(id)
                        .expect("comparison scale")
                        .clone(),
                })
            })
        })
        .collect()
}

pub(super) fn validate(saved: &[SavedComparison], panes: usize) -> Result<(), String> {
    if saved.iter().any(|entry| {
        entry.pane >= panes
            || !SymbolExpression::parse(&entry.symbol).is_ok_and(|expression| {
                expression.symbols().iter().all(|component| {
                    DemoSymbol::catalog()
                        .iter()
                        .any(|symbol| symbol.symbol == *component)
                })
            })
            || !entry.options.width.is_finite()
            || entry.options.width <= 0.0
            || entry
                .options
                .columns_base
                .is_some_and(|base| !base.is_finite())
            || !entry.scale.range().min.is_finite()
            || !entry.scale.range().max.is_finite()
            || entry.scale.range().min >= entry.scale.range().max
    }) {
        return Err("Could not open comparisons: invalid series metadata".into());
    }
    Ok(())
}

pub(super) fn restore(
    chart: &mut Chart,
    comparisons: &mut Vec<(String, SeriesId, Arc<LiveFeed>)>,
    spreads: &mut Vec<Spread>,
    saved: Vec<SavedComparison>,
) {
    let panes = chart.panes();
    for entry in saved {
        let id = chart.add_series(entry.options);
        if let Some(pane) = panes.get(entry.pane) {
            chart.move_series_to_pane(id, *pane);
        }
        if DemoSymbol::catalog()
            .iter()
            .any(|symbol| symbol.symbol == entry.symbol)
        {
            let feed = Arc::new(LiveFeed::new(&entry.symbol, chrono::Utc::now()));
            refresh(chart, id, &feed);
            comparisons.push((entry.symbol, id, feed));
        } else {
            let expression = SymbolExpression::parse(&entry.symbol).expect("validated comparison");
            spreads.push(Spread::new(expression, id));
        }
        chart.set_series_visible(id, entry.visible);
        *chart
            .series_price_scale_mut(id)
            .expect("restored comparison") = entry.scale;
    }
    refresh_spreads(chart, spreads, false);
}

pub(super) fn clear(
    chart: &mut Chart,
    comparisons: &mut Vec<(String, SeriesId, Arc<LiveFeed>)>,
    spreads: &mut Vec<Spread>,
) {
    for id in comparisons
        .drain(..)
        .map(|(_, id, _)| id)
        .chain(spreads.drain(..).map(|spread| spread.id))
    {
        chart.remove_series(id);
    }
}

pub(super) fn refresh(chart: &mut Chart, id: SeriesId, feed: &LiveFeed) {
    if chart.series_order(id).is_none() {
        return;
    }
    feed.refresh_comparison(chart, id);
}

pub(super) fn copy(
    chart: &mut Chart,
    source: &Chart,
    comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
) -> Vec<(String, SeriesId, Arc<LiveFeed>)> {
    let mut copied = Vec::new();
    for (symbol, id, feed) in comparisons {
        let Some(next) = copy_series(chart, source, *id) else {
            continue;
        };
        let feed = Arc::new(feed.fork());
        copied.push((symbol.clone(), next, feed));
    }
    copied
}

fn copy_series(chart: &mut Chart, source: &Chart, id: SeriesId) -> Option<SeriesId> {
    let (_, options) = source.series().find(|(current, _)| *current == id)?;
    let next = chart.add_series(options.clone());
    if let Some(pane) = source.series_pane(id).filter(|pane| *pane != PaneId::MAIN) {
        let target = chart.add_pane();
        chart.move_series_to_pane(next, target);
        if let Some(factor) = source.pane_stretch_factor(pane) {
            chart.set_pane_stretch_factor(target, factor);
        }
        if let Some(at) = source.panes().iter().position(|item| *item == pane) {
            chart.move_pane(target, at);
        }
    }
    chart.set_data(next, source.bars(id).to_vec());
    chart.set_series_visible(next, source.is_series_visible(id));
    *chart.series_price_scale_mut(next).expect("copied series") = source
        .series_price_scale(id)
        .expect("source series")
        .clone();
    Some(next)
}

pub(super) fn refresh_spreads(chart: &mut Chart, spreads: &mut [Spread], paused: bool) {
    let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
    let clock = chart.bar_clock();
    let extended_hours = chart.extended_hours();
    // Replay windows the supplied history at the cursor. A count bar has no closing time, so
    // one still open there is built from the executions before the cursor.
    let replay_time = chart.replay_time().filter(|_| timeframe.is_trade_based());
    let session = if !clock.has_session() {
        egui_charts::ui::timeframe_toolbar::SessionType::Full24x7
    } else if extended_hours {
        egui_charts::ui::timeframe_toolbar::SessionType::ETH
    } else {
        egui_charts::ui::timeframe_toolbar::SessionType::RTH
    };
    for spread in spreads {
        if chart.series_order(spread.id).is_none() {
            continue;
        }
        for feed in &spread.feeds {
            feed.set_view(session, chart.conversion() == Some("adjusted"));
            feed.set_paused(paused);
            feed.advance(chrono::Utc::now());
        }
        // Seed empty spreads from cached history before their executions finish loading.
        if spread.loading(chart) && !chart.bars(spread.id).is_empty() {
            continue;
        }
        let key: Vec<_> = spread
            .feeds
            .iter()
            .map(|feed| feed.comparison_snapshot(chart, spread.id))
            .collect();
        if spread.snapshot.as_ref() == Some(&key) && spread.replay_time == replay_time {
            continue;
        }
        let histories: Vec<_> = spread
            .feeds
            .iter()
            .map(|feed| feed.snapshot_before(timeframe, clock.clone(), extended_hours, replay_time))
            .collect();
        let symbols = spread.expression.symbols();
        let history = |symbol: &str| {
            let at = symbols.iter().position(|current| *current == symbol)?;
            Some(&histories[at][..])
        };
        if let Ok(bars) = spread.expression.bars(history) {
            let loaded = chart.bars(spread.id);
            let tail = loaded.len().saturating_sub(1);
            if !loaded.is_empty()
                && chart.replay_time().is_none()
                && bars.len() >= loaded.len()
                && bars[..tail] == loaded[..tail]
                // Bars of another interval, as a replay can retain, are replaced.
                && bars[tail].time == loaded[tail].time
            {
                let start = if bars[tail] == loaded[tail] {
                    tail + 1
                } else {
                    tail
                };
                for bar in bars.into_iter().skip(start) {
                    chart.update(spread.id, bar);
                }
            } else {
                chart.set_data(spread.id, bars);
            }
            spread.replay_time = replay_time;
            spread.snapshot = Some(
                spread
                    .feeds
                    .iter()
                    .map(|feed| feed.comparison_snapshot(chart, spread.id))
                    .collect(),
            );
        }
    }
}

impl DemoApp {
    /// The comparisons on the chart, without those an undoable edit removed.
    fn live_comparisons(&self) -> impl Iterator<Item = &(String, SeriesId, Arc<LiveFeed>)> {
        self.comparisons
            .iter()
            .filter(|(_, id, _)| self.chart.series_order(*id).is_some())
    }

    pub(super) fn sync_comparisons(&mut self) {
        // shortcut: removed entries stay until history is empty, including those past its
        // 100-step cap; prune per step if retaining their feeds becomes costly.
        if !self.chart.can_undo() && !self.chart.can_redo() {
            let chart = &self.chart;
            self.comparisons
                .retain(|(_, id, _)| chart.series_order(*id).is_some());
            self.spreads
                .retain(|spread| chart.series_order(spread.id).is_some());
        }
        for (_, id, feed) in &self.comparisons {
            refresh(&mut self.chart, *id, feed);
        }
        self.compare_dialog.compared_symbols = self
            .comparisons
            .iter()
            .filter_map(|(symbol, id, _)| {
                let (_, series) = self.chart.series().find(|(current, _)| current == id)?;
                let spec = DemoSymbol::find(symbol);
                let mut entry =
                    CompareSymbol::new(symbol, spec.company_name, spec.exchange, series.color);
                entry.visible = self.chart.is_series_visible(*id);
                Some(entry)
            })
            .collect();
        refresh_spreads(
            &mut self.chart,
            &mut self.spreads,
            self.live.paused() || self.replay.is_some(),
        );
    }

    /// Charts a spread picked in symbol search, such as `AAPL-MSFT`, as candles in a pane of its
    /// own, from feeds of its symbols.
    pub(super) fn add_spread(&mut self, symbol: &str) {
        let Ok(expression) = SymbolExpression::parse(symbol) else {
            return;
        };
        let options = SeriesOptions {
            name: symbol.to_owned(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            ..Default::default()
        };
        let id = self.chart.user_action(|chart| {
            let id = chart.add_series(options);
            let pane = chart.add_pane();
            chart.move_series_to_pane(id, pane);
            id
        });
        self.spreads.push(Spread::new(expression, id));
        self.grid_edits = self.grid_edits.wrapping_add(1);
        self.sync_comparisons();
    }

    /// Feeds another symbol as candles in a pane of its own, above the price pane, on the left
    /// scale once moved to the price pane.
    pub(super) fn add_series_pane(&mut self) {
        let Some(symbol) = DemoSymbol::catalog()
            .iter()
            .map(|spec| spec.symbol)
            .find(|symbol| {
                *symbol != self.chart.symbol()
                    && !self
                        .live_comparisons()
                        .any(|(current, _, _)| current == symbol)
            })
            .map(str::to_owned)
        else {
            self.notify("All demo symbols are already shown. Remove one before adding a pane.");
            return;
        };
        let feed = Arc::new(LiveFeed::new(&symbol, chrono::Utc::now()));
        let options = SeriesOptions {
            name: symbol.clone(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            scale: SeriesScale::Left,
            ..Default::default()
        };
        let id = self.chart.user_action(|chart| {
            let id = chart.add_series(options);
            refresh(chart, id, &feed);
            let pane = chart.add_pane();
            chart.move_series_to_pane(id, pane);
            // Half the price pane's default share of the height.
            chart.set_pane_stretch_factor(pane, 1.5);
            chart.move_pane(pane, 0);
            id
        });
        self.comparisons.push((symbol, id, feed));
        self.grid_edits = self.grid_edits.wrapping_add(1);
        self.sync_comparisons();
    }

    pub(super) fn open_comparison_settings(&mut self, id: SeriesId) {
        let Some((_, options)) = self.chart.series().find(|(series, _)| *series == id) else {
            return;
        };
        let Some(scale) = self.chart.series_price_scale(id) else {
            return;
        };
        let occupied: Vec<_> = self
            .chart
            .series()
            .map(|(_, options)| options.scale)
            .chain(self.chart.series_scale(SeriesId::MAIN))
            .chain(
                self.chart
                    .indicators()
                    .filter_map(|(_, study)| match study.scale() {
                        egui_charts::studies::StudyScale::Shared(scale) => Some(scale),
                        _ => None,
                    }),
            )
            .collect();
        let key = (0..=u64::MAX)
            .find(|key| {
                !occupied.iter().any(|scale| {
                    matches!(
                        scale, SeriesScale::Separate(id) | SeriesScale::Overlay(id) if id == key
                    )
                })
            })
            .expect("a finite chart leaves unused scale identifiers");
        let independent_scale = match options.scale {
            SeriesScale::Separate(_) => options.scale,
            _ => SeriesScale::Separate(key),
        };
        let overlay_scale = match options.scale {
            SeriesScale::Overlay(_) => options.scale,
            _ => SeriesScale::Overlay(key),
        };
        self.comparison_settings = Some(ComparisonSettings {
            id,
            options: options.clone(),
            scale: scale.clone(),
            original_scale: scale.clone(),
            independent_scale,
            overlay_scale,
        });
    }

    pub(super) fn show_comparison_settings(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.comparison_settings.take() else {
            return;
        };
        if self.chart.series_order(draft.id).is_none() {
            return;
        }
        let mut open = true;
        if draft.show(ctx, &self.chart, &mut open) {
            self.chart.user_action(|chart| {
                let scale_changed = chart.series_scale(draft.id) != Some(draft.options.scale);
                chart.set_series_options(draft.id, draft.options);
                if scale_changed
                    || draft.scale.options() != draft.original_scale.options()
                    || draft.scale.is_manual() != draft.original_scale.is_manual()
                    || (draft.scale.is_manual()
                        && draft.scale.range() != draft.original_scale.range())
                {
                    *chart
                        .series_price_scale_mut(draft.id)
                        .expect("edited comparison") = draft.scale;
                }
            });
        } else if open {
            self.comparison_settings = Some(draft);
        }
    }

    pub(super) fn handle_comparison(&mut self, action: CompareAction) {
        match action {
            CompareAction::AddSymbol(symbol, placement) => {
                if self
                    .live_comparisons()
                    .any(|(current, _, _)| *current == symbol)
                    || !DemoSymbol::catalog()
                        .iter()
                        .any(|spec| spec.symbol == symbol)
                {
                    return;
                }
                let feed = Arc::new(LiveFeed::new(&symbol, chrono::Utc::now()));
                let colors = [
                    self.theme.ui.info,
                    self.theme.ui.warning,
                    self.theme.ui.success,
                ];
                let color = colors[self.live_comparisons().count() % colors.len()];
                let scale = match placement {
                    // The lowest additional scale id no series uses yet.
                    ComparePlacement::NewPriceScale => {
                        (1..).map(SeriesScale::Separate).find(|scale| {
                            self.chart.series_scale(SeriesId::MAIN) != Some(*scale)
                                && !self
                                    .chart
                                    .series()
                                    .any(|(_, series)| series.scale == *scale)
                        })
                    }
                    ComparePlacement::NewPane => Some(SeriesScale::Right),
                    _ => None,
                };
                let id = self.chart.user_action(|chart| {
                    let id = match scale {
                        Some(scale) => chart.add_series(SeriesOptions {
                            name: symbol.clone(),
                            color,
                            scale,
                            ..Default::default()
                        }),
                        None => chart.add_comparison(symbol.clone(), color),
                    };
                    if placement == ComparePlacement::NewPane {
                        let pane = chart.add_pane();
                        chart.move_series_to_pane(id, pane);
                    }
                    refresh(chart, id, &feed);
                    id
                });
                self.comparisons.push((symbol, id, feed));
                self.grid_edits = self.grid_edits.wrapping_add(1);
            }
            CompareAction::RemoveSymbol(symbol) => {
                let id = self
                    .live_comparisons()
                    .find(|(current, _, _)| *current == symbol)
                    .map(|(_, id, _)| *id);
                if let Some(id) = id {
                    self.chart.user_action(|chart| chart.remove_series(id));
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::ToggleVisibility(symbol) => {
                let id = self
                    .live_comparisons()
                    .find(|(current, _, _)| *current == symbol)
                    .map(|(_, id, _)| *id);
                if let Some(id) = id {
                    let visible = self.chart.is_series_visible(id);
                    self.chart
                        .user_action(|chart| chart.set_series_visible(id, !visible));
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::None | CompareAction::Close => {}
            _ => {}
        }
        self.sync_comparisons();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each style control of the editor reaches the chosen series through the accepted draft,
    /// and taking over a style starts from what the series already draws.
    #[test]
    fn comparison_settings_style_controls_edit_the_series() {
        use super::super::tests::{ui_click, ui_frame};
        use egui_charts::model::{AreaStyle, BarsStyle};
        let color = egui::Color32::from_rgb(10, 20, 30);
        let own_line = LinePlotStyle {
            color,
            width: 3.0,
            ..Default::default()
        };
        let own_area = AreaStyle {
            line: own_line.clone(),
            top_fill: egui::Color32::from_rgba_unmultiplied(10, 20, 30, 115),
            bottom_fill: egui::Color32::TRANSPARENT,
            relative_gradient: false,
            invert_filled_area: true,
            ..Default::default()
        };
        for (chart_type, clicks, expected_line, expected_bars, expected_area) in [
            (
                ChartType::Line,
                vec!["Independent line style"],
                Some(own_line.clone()),
                BarsStyle::default(),
                None,
            ),
            (
                ChartType::Bars,
                vec!["Show open", "Thin bars"],
                None,
                BarsStyle {
                    open_visible: false,
                    thin_bars: false,
                    ..Default::default()
                },
                None,
            ),
            (
                ChartType::Area,
                vec!["Independent area style", "Relative gradient", "Invert fill"],
                None,
                BarsStyle::default(),
                Some(own_area.clone()),
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.handle_comparison(CompareAction::AddSymbol(
                "MSFT".into(),
                ComparePlacement::NewPriceScale,
            ));
            let id = app.comparisons[0].1;
            super::super::tests::settle(&mut app, &ctx);
            app.open_indicator_settings(id);
            let draft = app.comparison_settings.as_mut().unwrap();
            draft.options.chart_type = chart_type;
            draft.options.color = color;
            draft.options.width = 3.0;
            let size = egui::vec2(1440.0, 900.0);
            let press = |app: &mut DemoApp, label: &str| {
                let mut texts = Vec::new();
                for _ in 0..3 {
                    texts = ui_frame(&ctx, size, vec![], |ui| app.show_dialogs(ui.ctx()));
                }
                let point = texts
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("{chart_type:?}: no control {label}"))
                    .1
                    .center();
                ui_click(&ctx, size, point, |ui| app.show_dialogs(ui.ctx()));
            };
            press(&mut app, "Line, bars and area");
            for label in clicks {
                press(&mut app, label);
            }
            press(&mut app, "Ok");
            let options = app
                .chart
                .series()
                .find(|(series, _)| *series == id)
                .unwrap()
                .1;
            assert_eq!(options.line_style, expected_line, "{chart_type:?}");
            assert_eq!(options.bars_style, expected_bars, "{chart_type:?}");
            assert_eq!(options.area_style, expected_area, "{chart_type:?}");
        }
    }

    /// Baseline takeover keeps both lines and flat fills until a setting is edited.
    #[test]
    fn baseline_takeover_starts_from_series() {
        use super::super::tests::{ui_click, ui_frame};
        use egui_charts::model::BaselineStyle;
        for theme in [
            egui_charts::theme::Theme::light(),
            egui_charts::theme::Theme::dark(),
        ] {
            for size in [egui::vec2(1440.0, 900.0), egui::vec2(320.0, 450.0)] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                app.theme = theme.clone();
                app.chart.set_theme(&ctx, &theme);
                app.handle_comparison(CompareAction::AddSymbol(
                    "MSFT".into(),
                    ComparePlacement::NewPriceScale,
                ));
                let id = app.comparisons[0].1;
                super::super::tests::settle(&mut app, &ctx);
                let mut options = app
                    .chart
                    .series()
                    .find(|(series, _)| *series == id)
                    .unwrap()
                    .1
                    .clone();
                options.chart_type = ChartType::Baseline;
                options.up_color = egui::Color32::from_rgb(200, 10, 10);
                options.down_color = egui::Color32::from_rgb(10, 10, 200);
                options.width = 3.0;
                app.chart.set_series_options(id, options);
                app.open_indicator_settings(id);
                for label in ["Markers and baseline", "Independent baseline style", "Ok"] {
                    let mut point = None;
                    for _ in 0..16 {
                        let mut texts = Vec::new();
                        for _ in 0..10 {
                            texts = ui_frame(&ctx, size, vec![], |ui| app.show_dialogs(ui.ctx()));
                        }
                        let footer = texts.iter().find(|(text, _)| text == "Ok").unwrap().1;
                        point = texts.iter().find_map(|(text, rect)| {
                            let point = rect.left_center() + egui::vec2(4.0, 0.0);
                            (text == label
                                && egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                                    .contains(point)
                                && (label == "Ok" || point.y < footer.top() - 24.0))
                                .then_some(point)
                        });
                        if point.is_some() {
                            break;
                        }
                        ui_frame(
                            &ctx,
                            size,
                            vec![
                                egui::Event::PointerMoved((size * 0.5).to_pos2()),
                                egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    delta: egui::vec2(0.0, -80.0),
                                    phase: egui::TouchPhase::Move,
                                    source: egui::MouseWheelSource::Unknown,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                            |ui| app.show_dialogs(ui.ctx()),
                        );
                    }
                    let point =
                        point.unwrap_or_else(|| panic!("{size:?}: no baseline control {label}"));
                    ui_click(&ctx, size, point, |ui| app.show_dialogs(ui.ctx()));
                }
                let options = app
                    .chart
                    .series()
                    .find(|(series, _)| *series == id)
                    .unwrap()
                    .1;
                assert_eq!(
                    options.baseline_style,
                    Some(BaselineStyle {
                        above: LinePlotStyle {
                            color: egui::Color32::from_rgb(200, 10, 10),
                            width: 3.0,
                            ..Default::default()
                        },
                        below: LinePlotStyle {
                            color: egui::Color32::from_rgb(10, 10, 200),
                            width: 3.0,
                            ..Default::default()
                        },
                        above_fill: egui::Color32::from_rgba_unmultiplied(200, 10, 10, 30),
                        below_fill: egui::Color32::from_rgba_unmultiplied(10, 10, 200, 30),
                        ..Default::default()
                    })
                );
            }
        }
    }

    /// Both settings routes retain the selected bar series; cancel, accept and undo stay local.
    #[test]
    fn comparison_settings_edit_only_the_chosen_series_and_save() {
        use super::super::tests::{ui_click, ui_frame};
        use super::super::{SeriesContextMenuAction, TopToolbarAction};
        use egui_charts::model::LineStyle;
        use egui_charts::scales::PriceScaleMode;
        use egui_charts::ui::top_toolbar::LayoutAction;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.handle_comparison(CompareAction::AddSymbol(
            "MSFT".into(),
            ComparePlacement::NewPriceScale,
        ));
        let id = app.comparisons[0].1;
        super::super::tests::settle(&mut app, &ctx);
        let main = app.chart.snapshot().unwrap();
        let original = app
            .chart
            .series()
            .find(|(series, _)| *series == id)
            .unwrap()
            .1
            .clone();
        let size = egui::vec2(1440.0, 900.0);
        let close = |app: &mut DemoApp, label: &str| {
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(&ctx, size, vec![], |ui| app.show_dialogs(ui.ctx()));
            }
            let point = texts
                .iter()
                .find(|(text, _)| text == label)
                .expect("editor button")
                .1
                .center();
            ui_click(&ctx, size, point, |ui| app.show_dialogs(ui.ctx()));
        };
        app.open_indicator_settings(id);
        assert!(!app.settings_dialog.is_open);
        assert!(!app.compare_dialog.is_open);
        app.comparison_settings
            .as_mut()
            .expect("added-series settings")
            .options
            .width = 7.0;
        close(&mut app, "Cancel");
        assert_eq!(
            app.chart
                .series()
                .find(|(series, _)| *series == id)
                .unwrap()
                .1,
            &original
        );
        app.open_indicator_settings(id);
        let draft = app.comparison_settings.as_mut().unwrap();
        draft.options.color = egui::Color32::YELLOW;
        draft.options.width = 4.0;
        draft.options.chart_type = ChartType::StepLine;
        draft.options.style.last_value_visible = false;
        draft.options.style.price_line_visible = true;
        draft.options.style.price_line_color = Some(egui::Color32::RED);
        draft.options.style.price_line_style = LineStyle::Dotted;
        draft.options.style.price_line_width = 3.0;
        draft.scale.set_range(100.0, 200.0);
        draft.scale.options_mut().mode = PriceScaleMode::Logarithmic;
        draft.scale.options_mut().invert_scale = true;
        let edited = draft.options.clone();
        close(&mut app, "Ok");
        assert_eq!(
            app.chart
                .series()
                .find(|(series, _)| *series == id)
                .unwrap()
                .1,
            &edited
        );
        assert!(
            app.chart
                .series_price_scale(id)
                .unwrap()
                .options()
                .invert_scale
        );
        assert_eq!(
            app.chart.series_price_scale(id).unwrap().options().mode,
            PriceScaleMode::Logarithmic
        );
        assert!(app.chart.series_price_scale(id).unwrap().is_manual());
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| {
                app.chart.show(ui);
            });
        }
        let range = app.chart.series_price_scale(id).unwrap().range();
        // ln(100) = 2 ln(10), ln(200) = ln(2) + 2 ln(10).
        assert!((range.min - 4.605170185988092).abs() < 1e-12);
        assert!((range.max - 5.298317366548036).abs() < 1e-12);
        assert_eq!(app.chart.chart_type(), main.chart_type);
        assert_eq!(
            app.chart.state().price_scale().options().mode,
            PriceScaleMode::Normal
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        assert_eq!(
            app.chart
                .series()
                .find(|(series, _)| *series == id)
                .unwrap()
                .1,
            &original
        );
        app.handle_top_toolbar(&ctx, TopToolbarAction::Redo);
        app.handle_layout_action(LayoutAction::SaveAs("Comparison style".into()));
        app.poll_storage(&ctx);
        app.chart.remove_series(id);
        app.handle_layout_action(LayoutAction::Open("Comparison style".into()));
        app.poll_storage(&ctx);
        let (restored, options) = app
            .chart
            .series()
            .find(|(_, options)| options.name == "MSFT")
            .unwrap();
        assert_eq!(options, &edited);
        assert!(
            app.chart
                .series_price_scale(restored)
                .unwrap()
                .options()
                .invert_scale
        );
        // The chart selects the right-clicked series before the host handles its action.
        app.chart.set_series_visible(restored, true);
        super::super::tests::settle(&mut app, &ctx);
        let mut bars = app.chart.bars(SeriesId::MAIN).to_vec();
        let at = bars.len() - 5;
        // The restored axis is inverted; keep the gesture above overlaid studies.
        bars[at].close = bars[at].low * 0.5;
        bars[at].low = bars[at].close;
        app.chart.set_data(restored, bars);
        app.chart.series_price_scale_mut(restored).unwrap().reset();
        for _ in 0..3 {
            ui_frame(&ctx, size, vec![], |ui| {
                app.chart.show(ui);
            });
        }
        let bars = app.chart.bars(restored);
        let index = bars.len() - 5;
        let bar = bars[index].clone();
        let map = app.chart.series_price_map(restored).unwrap();
        let point = egui::pos2(
            app.chart.time_to_x(bar.time).unwrap(),
            map.price_to_y(bar.close),
        );
        for pressed in [true, false] {
            ui_frame(
                &ctx,
                size,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                |ui| {
                    app.chart.show(ui);
                },
            );
        }
        assert_eq!(
            app.chart.selected_element(),
            Some(egui_charts::widget::ChartElementId::Series(restored))
        );
        app.handle_series_context(SeriesContextMenuAction::OpenSettings);
        assert_eq!(app.comparison_settings.as_ref().unwrap().id, restored);
        assert!(app.keyboard_dialog_open());
        let primary = *app.chart.state().price_scale().options();
        close(&mut app, "New price scale");
        close(&mut app, "Right");
        assert_eq!(
            *app.comparison_settings.as_ref().unwrap().scale.options(),
            primary
        );
        close(&mut app, "Ok");
        assert_eq!(app.chart.series_scale(restored), Some(SeriesScale::Right));
        assert_eq!(*app.chart.state().price_scale().options(), primary);
        assert!(app.chart.undo());
        app.open_indicator_settings(restored);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.activate_chart_slot(1);
        assert!(app.comparison_settings.is_none());
    }

    /// Live rows use series handles for every kind of added symbol and survive history and layouts.
    #[test]
    fn comparison_tree_tracks_series_actions_history_and_layouts() {
        use egui_charts::ui::top_toolbar::LayoutAction;
        use egui_charts::ui::widget_bar::panels::object_tree::ObjectTreeAction as A;
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.handle_comparison(CompareAction::AddSymbol(
            "MSFT".into(),
            ComparePlacement::NewPriceScale,
        ));
        app.add_series_pane();
        app.add_spread("AAPL-MSFT");
        super::super::tests::settle(&mut app, &ctx);
        let ids: Vec<_> = app.chart.series().map(|(id, _)| id).collect();
        assert_eq!(ids.len(), 3);
        let check = |app: &DemoApp| {
            let rows = app.build_source_items();
            for (id, options) in app.chart.series() {
                let rows: Vec<_> = rows
                    .iter()
                    .filter(|row| row.series_id == Some(id))
                    .collect();
                assert_eq!(rows.len(), 1, "each live series has one row");
                assert_eq!(rows[0].name, options.name);
                assert_eq!(rows[0].visible, app.chart.is_series_visible(id));
            }
        };
        check(&app);
        for id in &ids {
            let row = super::super::indicator_tree_id(*id);
            let sources = app.build_source_items();
            app.handle_object_tree(A::ToggleVisibility(row), &sources);
            assert!(!app.chart.is_series_visible(*id));
            assert!(app.chart.undo());
            assert!(app.chart.is_series_visible(*id));
            assert!(app.chart.redo());
            check(&app);
            app.chart.set_series_visible(*id, true);
            // Selecting a row keeps the view; zooming brings the latest bars of the series back.
            let bars = app.chart.bars(*id);
            let (earlier, latest) = (bars[bars.len() / 2].time, bars[bars.len() - 1].time);
            let settle_view = |app: &mut DemoApp| {
                for _ in 0..3 {
                    super::super::tests::ui_frame(&ctx, egui::vec2(1440.0, 900.0), vec![], |ui| {
                        app.chart.show(ui);
                    });
                }
                app.chart.visible_time_range().unwrap()
            };
            app.chart.scroll_to_time(earlier);
            let before = settle_view(&mut app);
            assert!(before.to < latest);
            let sources = app.build_source_items();
            app.handle_object_tree(A::Select(row), &sources);
            assert_eq!(settle_view(&mut app).to, before.to);
            let sources = app.build_source_items();
            app.handle_object_tree(A::ZoomTo(row), &sources);
            assert!(settle_view(&mut app).to >= latest);
            let sources = app.build_source_items();
            app.handle_object_tree(A::OpenProperties(row), &sources);
            assert_eq!(app.comparison_settings.take().unwrap().id, *id);
        }
        let row = super::super::indicator_tree_id(ids[0]);
        let sources = app.build_source_items();
        app.handle_object_tree(A::SendToBack(row), &sources);
        assert_eq!(app.chart.series_order(ids[0]), Some(0));
        let sources = app.build_source_items();
        app.handle_object_tree(A::BringToFront(row), &sources);
        assert!(app.chart.series_order(ids[0]).unwrap() > 0);
        // Rows follow the draw order of their pane, whichever series moved.
        let main = super::super::MAIN_SERIES_ID;
        let sources = app.build_source_items();
        app.handle_object_tree(A::BringToFront(main), &sources);
        let layer = |app: &DemoApp, id| {
            let rows = app.build_source_items();
            rows.iter().find(|item| item.id == id).unwrap().z_index
        };
        assert!(layer(&app, main) > layer(&app, row));
        app.handle_layout_action(LayoutAction::SaveAs("Series tree".into()));
        app.poll_storage(&ctx);
        super::super::tests::settle(&mut app, &ctx);
        let ids: Vec<_> = app.chart.series().map(|(id, _)| id).collect();
        check(&app);
        for id in ids {
            let sources = app.build_source_items();
            app.handle_object_tree(A::Delete(super::super::indicator_tree_id(id)), &sources);
            assert!(app.chart.series_order(id).is_none());
            check(&app);
            assert!(app.chart.undo());
            check(&app);
            assert!(app.chart.redo());
        }
        app.handle_layout_action(LayoutAction::Open("Series tree".into()));
        app.poll_storage(&ctx);
        assert_eq!(app.chart.series().count(), 3);
        check(&app);
    }

    /// Pointer frames keep loaded comparisons intact; trades and view changes still refresh
    /// their bars, and a history restore that cleared the bars loads them again.
    #[test]
    fn unchanged_comparisons_do_not_reload_history() {
        use chrono::{DateTime, TimeDelta};
        use egui_charts::model::BarData;
        use egui_charts::widget::ChartEvent;

        let now = DateTime::parse_from_rfc3339("2026-10-23T14:30:00Z")
            .unwrap()
            .to_utc();
        for count in [1, 2, 5] {
            let mut chart = Chart::new(BarData::new());
            chart.set_timeframe(Timeframe::Min1);
            let feeds: Vec<_> = (0..count)
                .map(|_| {
                    let id = chart.add_comparison("EURUSD", egui::Color32::BLUE);
                    let feed = LiveFeed::new("EURUSD", now);
                    super::super::tests::finish_feed(&feed);
                    refresh(&mut chart, id, &feed);
                    assert!(!chart.bars(id).is_empty());
                    (id, feed)
                })
                .collect();
            chart.drain_events().for_each(drop);
            for (id, feed) in &feeds {
                feed.advance(now + TimeDelta::nanoseconds(1));
                refresh(&mut chart, *id, feed);
            }
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "{count} comparisons must not reload on an unchanged frame"
            );
            for (id, feed) in &feeds {
                feed.advance(now + TimeDelta::seconds(2));
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min1, chart.bar_clock(), false)
                );
            }
            chart.set_timeframe(Timeframe::Min5);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min5, chart.bar_clock(), false)
                );
                chart.set_data(*id, Vec::new());
                refresh(&mut chart, *id, feed);
                assert!(!chart.bars(*id).is_empty());
            }
            chart.drain_events().for_each(drop);
            chart.set_extended_hours(true);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min5, chart.bar_clock(), true)
                );
            }
            assert_eq!(
                chart
                    .drain_events()
                    .filter(|event| matches!(event, ChartEvent::DataLoaded { .. }))
                    .count(),
                count,
                "a session change reloads each comparison"
            );
            chart.set_replay_time(Some(DateTime::UNIX_EPOCH));
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert!(chart.bars(*id).is_empty());
            }
            chart.drain_events().for_each(drop);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
            }
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "an empty replay window must not reload retained history"
            );
        }
    }

    /// Paused spreads leave loaded history intact, but trades, view changes and restoration
    /// still update it. Replay moves window the retained bars without rebuilding the expression.
    #[test]
    fn unchanged_spreads_do_not_reload_history() {
        use egui_charts::data::{MarketEvent, ReplayDriver, ReplayEvent};
        use egui_charts::model::{BarData, Instrument, Tick};
        use egui_charts::widget::ChartEvent;

        let mut chart = Chart::new(BarData::new());
        chart.set_timeframe(Timeframe::Min1);
        let id = chart.add_comparison("AAPL-MSFT", egui::Color32::BLUE);
        let now = "2026-10-23T14:30:00Z".parse().unwrap();
        let mut spreads = vec![Spread {
            expression: SymbolExpression::parse("AAPL-MSFT").unwrap(),
            id,
            feeds: ["AAPL", "MSFT"]
                .into_iter()
                .map(|symbol| Arc::new(LiveFeed::new(symbol, now)))
                .collect(),
            snapshot: None,
            replay_time: None,
        }];
        refresh_spreads(&mut chart, &mut spreads, true);
        assert!(
            !chart.bars(id).is_empty(),
            "a new spread must show cached history before bootstrap finishes"
        );
        assert!(spreads[0].loading(&chart));
        chart.drain_events().for_each(drop);
        refresh_spreads(&mut chart, &mut spreads, true);
        assert!(
            !chart
                .drain_events()
                .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
            "loading spreads must retain their cached history"
        );
        for feed in &spreads[0].feeds {
            super::super::tests::finish_feed(feed);
        }
        refresh_spreads(&mut chart, &mut spreads, true);
        assert!(!chart.bars(id).is_empty());
        let instrument = Instrument::new("SAMPLE")
            .with_tick_size(0.01)
            .with_time_zone(crate::sampledata::us_equity_zone())
            .with_session(crate::sampledata::us_equity_session());
        for sessions in [false, true] {
            chart.set_instrument(sessions.then(|| instrument.clone()));
            for timeframe in [Timeframe::Min1, Timeframe::Min5] {
                chart.set_timeframe(timeframe);
                for extended in [false, true] {
                    chart.set_extended_hours(extended);
                    refresh_spreads(&mut chart, &mut spreads, true);
                    let histories: Vec<_> = spreads[0]
                        .feeds
                        .iter()
                        .map(|feed| feed.snapshot(timeframe, chart.bar_clock(), extended))
                        .collect();
                    let expected = spreads[0]
                        .expression
                        .bars(|symbol| Some(&histories[usize::from(symbol == "MSFT")][..]))
                        .unwrap();
                    assert_eq!(chart.bars(id), expected);
                    if sessions {
                        assert!(chart.bars(id).iter().all(|bar| {
                            crate::sampledata::us_equity_hours().is_open(bar.time, extended)
                        }));
                    }

                    chart.drain_events().for_each(drop);
                    for _ in 0..3 {
                        refresh_spreads(&mut chart, &mut spreads, true);
                    }
                    assert!(
                        !chart
                            .drain_events()
                            .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                        "unchanged spreads must not reload history"
                    );
                    for feed in &spreads[0].feeds {
                        feed.set_paused(false);
                        feed.advance(feed.now() + chrono::TimeDelta::seconds(2));
                    }
                    refresh_spreads(&mut chart, &mut spreads, true);
                    let last = chart.bars(id).last().unwrap();
                    let closes: Vec<_> = spreads[0]
                        .feeds
                        .iter()
                        .map(|feed| {
                            feed.snapshot(timeframe, chart.bar_clock(), extended)
                                .last()
                                .unwrap()
                                .close
                        })
                        .collect();
                    assert!((last.close - (closes[0] - closes[1])).abs() < 1e-9);
                    chart.set_data(id, Vec::new());
                    refresh_spreads(&mut chart, &mut spreads, true);
                    assert!(
                        !chart.bars(id).is_empty(),
                        "restore must reload cleared bars"
                    );
                }
            }
        }
        for time in [chrono::DateTime::UNIX_EPOCH, spreads[0].feeds[0].now()] {
            chart.set_replay_time(Some(time));
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "a replay window must not repeatedly rebuild its source"
            );
        }
        chart.set_replay_time(None);
        refresh_spreads(&mut chart, &mut spreads, true);
        let history = chart.bars(id).to_vec();
        let events = [now - chrono::TimeDelta::minutes(30), now]
            .into_iter()
            .enumerate()
            .map(|(sequence, time)| {
                ReplayEvent::new(
                    "SAMPLE",
                    time,
                    sequence as u64,
                    MarketEvent::Trade(Tick::new(time, 100.0, 1.0, None)),
                )
            })
            .collect();
        let driver = Arc::new(ReplayDriver::new(events, vec![instrument.clone()]).unwrap());
        chart.set_market_replay(Some(driver.clone()));
        chart.set_instrument(Some(instrument));
        for time in [
            now - chrono::TimeDelta::minutes(20),
            now - chrono::TimeDelta::minutes(10),
            now - chrono::TimeDelta::minutes(30),
        ] {
            chart.set_replay_time(Some(time + chrono::TimeDelta::nanoseconds(1)));
            refresh_spreads(&mut chart, &mut spreads, true);
            let expected: Vec<_> = history
                .iter()
                .filter(|bar| {
                    bar.time + chrono::TimeDelta::minutes(5)
                        <= time + chrono::TimeDelta::nanoseconds(1)
                })
                .cloned()
                .collect();
            assert_eq!(
                chart.bars(id),
                expected,
                "market replay must hide future spread bars"
            );
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "a paused market replay must keep its spread history"
            );
        }
        let trade = Timeframe::Ticks(std::num::NonZeroU64::MIN);
        let first: Vec<_> = spreads[0]
            .feeds
            .iter()
            .map(|feed| {
                feed.snapshot(trade, egui_charts::model::BarClock::new(trade), true)[0].clone()
            })
            .collect();
        assert_eq!(first[0].time, first[1].time);
        chart.set_timeframe(Timeframe::Ticks(std::num::NonZeroU64::new(1000).unwrap()));
        driver.seek(first[0].time + chrono::TimeDelta::nanoseconds(1));
        refresh_spreads(&mut chart, &mut spreads, true);
        assert_eq!(chart.bars(id).len(), 1);
        let bar = &chart.bars(id)[0];
        let expected = first[0].close - first[1].close;
        assert!(
            [bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|value| (*value - expected).abs() < 1e-9),
            "a count bar cannot use executions after the cursor"
        );
        chart.set_timeframe(Timeframe::Min5);
        chart.set_market_replay(None);
        refresh_spreads(&mut chart, &mut spreads, true);
        assert_eq!(
            chart.bars(id),
            history,
            "exiting market replay restores the spread"
        );
    }

    fn listed(app: &DemoApp) -> Vec<&str> {
        app.compare_dialog
            .compared_symbols
            .iter()
            .map(|entry| entry.symbol.as_str())
            .collect()
    }

    /// Undo restores a removed comparison's feed and dialog entry; the removed symbol can
    /// also be compared anew, in its first palette colour, without its parked entry intercepting
    /// later dialog edits.
    #[test]
    fn undo_brings_back_a_removed_comparison_with_its_feed() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx));
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
        super::super::tests::finish_feed(&app.comparisons[0].2);
        let id = app.comparisons[0].1;
        let colour = |app: &DemoApp, id| {
            app.chart
                .series()
                .find(|(current, _)| *current == id)
                .unwrap()
                .1
                .color
        };
        let first = colour(&app, id);
        app.handle_comparison(CompareAction::RemoveSymbol("AAPL".into()));
        assert!(listed(&app).is_empty());

        assert!(app.chart.undo());
        app.chart.set_data(id, Vec::new());
        app.sync_comparisons();
        assert_eq!(listed(&app), ["AAPL"]);
        assert!(!app.chart.bars(id).is_empty());

        assert!(app.chart.redo());
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
        assert_eq!(listed(&app), ["AAPL"]);
        let again = app.comparisons.last().unwrap().1;
        assert_ne!(again, id);
        assert_eq!(colour(&app, again), first);
        app.handle_comparison(CompareAction::ToggleVisibility("AAPL".into()));
        assert!(!app.chart.is_series_visible(again));
        app.handle_comparison(CompareAction::RemoveSymbol("AAPL".into()));
        assert!(listed(&app).is_empty());
    }
}
