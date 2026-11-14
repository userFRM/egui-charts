//! Controls for the demo's chart readouts and series annotations.

use super::DemoApp;
use egui_charts::config::TooltipMode;

impl DemoApp {
    pub(super) fn show_readout_controls(&mut self, ui: &mut egui::Ui) {
        let mut chart_options = self.chart.chart_options().clone();
        let previous = chart_options.tooltip.clone();
        let options = &mut chart_options.tooltip;
        ui.checkbox(&mut options.enabled, "Show readout");
        for mode in [
            TooltipMode::Floating,
            TooltipMode::Tracking,
            TooltipMode::Magnifier,
        ] {
            ui.radio_value(&mut options.mode, mode, mode.to_string());
        }
        ui.separator();
        ui.radio_value(&mut options.on_long_press, false, "Hover to inspect");
        ui.radio_value(&mut options.on_long_press, true, "Hold to inspect");
        ui.small(if options.on_long_press {
            "Hold the primary button over a bar. On touch, press and wait."
        } else {
            "Move the pointer over a bar. On touch, press and wait."
        });
        if options.mode == TooltipMode::Magnifier {
            ui.add(
                egui::Slider::new(&mut options.magnifier_size, 60.0..=360.0).text("Lens diameter"),
            );
            ui.add(egui::Slider::new(&mut options.magnifier_zoom, 1.0..=8.0).text("Zoom"));
        } else {
            ui.checkbox(&mut options.show_time, "Time");
            ui.checkbox(&mut options.show_ohlc, "OHLC");
            ui.checkbox(&mut options.show_volume, "Volume");
            ui.checkbox(&mut options.show_change, "Change");
            ui.checkbox(&mut options.show_indicators, "Studies");
            ui.checkbox(&mut options.show_volume_details, "Held volume details");
        }
        if chart_options.tooltip != previous {
            self.chart.set_chart_options(chart_options);
            ui.ctx().request_repaint();
        }
    }
}

impl DemoApp {
    pub(super) fn show_annotation_editor(&mut self, ctx: &egui::Context) {
        use egui_charts::model::{
            LineStyle, Marker, MarkerPos, MarkerShape, MarkerZOrder, PriceLine,
        };
        use egui_charts::widget::SeriesId;
        let mut open = self.annotation_editor;
        egui::Window::new("Markers and price lines")
            .open(&mut open)
            .default_pos(egui::pos2(60.0, 120.0))
            .default_width(340.0)
            .default_height(640.0)
            .vscroll(true)
            .show(ctx, |ui| {
                let mut series = vec![(SeriesId::MAIN, self.chart.symbol().to_owned())];
                series.extend(
                    self.chart
                        .series()
                        .map(|(id, options)| (id, options.name.clone())),
                );
                series.retain(|(id, _)| self.chart.series_scale(*id).is_some());
                if !series.iter().any(|(id, _)| *id == self.annotation_series) {
                    let Some((id, _)) = series.first() else {
                        ui.label("Add a bar series to try annotations.");
                        return;
                    };
                    self.annotation_series = *id;
                }
                egui::ComboBox::from_label("Series")
                    .selected_text(
                        series
                            .iter()
                            .find(|(id, _)| *id == self.annotation_series)
                            .map(|(_, name)| name)
                            .unwrap(),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in &series {
                            ui.selectable_value(&mut self.annotation_series, *id, name);
                        }
                    });
                let id = self.annotation_series;
                let bars = self.chart.bars(id);
                let Some(bar) = bars.get(bars.len().saturating_sub(12)).cloned() else {
                    ui.label("Wait for series history to load.");
                    return;
                };
                let existing = self
                    .chart
                    .markers(id)
                    .iter()
                    .find(|marker| marker.id.as_deref() == Some("demo-editor"))
                    .cloned();
                let mut marker = existing.clone().unwrap_or_else(|| {
                    let mut marker = Marker::new(bar.time)
                        .with_text("Sample")
                        .with_color(self.theme.ui.info);
                    marker.id = Some("demo-editor".into());
                    marker
                });
                ui.separator();
                ui.label("Marker");
                let mut enabled = existing.is_some();
                let mut changed = ui.checkbox(&mut enabled, "Show marker").changed();
                ui.add_enabled_ui(enabled, |ui| {
                    let shapes = [
                        (MarkerShape::Circle, "Circle"),
                        (MarkerShape::Square, "Square"),
                        (MarkerShape::ArrowUp, "Arrow up"),
                        (MarkerShape::ArrowDown, "Arrow down"),
                        (MarkerShape::TriangleUp, "Triangle up"),
                        (MarkerShape::TriangleDown, "Triangle down"),
                        (MarkerShape::Diamond, "Diamond"),
                        (MarkerShape::Star, "Star"),
                        (MarkerShape::Cross, "Cross"),
                        (MarkerShape::Flag, "Flag"),
                    ];
                    egui::ComboBox::from_label("Shape")
                        .selected_text(
                            shapes
                                .iter()
                                .find(|(shape, _)| *shape == marker.shape)
                                .unwrap()
                                .1,
                        )
                        .show_ui(ui, |ui| {
                            for (shape, label) in shapes {
                                changed |= ui
                                    .selectable_value(&mut marker.shape, shape, label)
                                    .changed();
                            }
                        });
                    let positions = [
                        (MarkerPos::AboveBar, "Above bar"),
                        (MarkerPos::BelowBar, "Below bar"),
                        (MarkerPos::InBar, "In bar"),
                        (MarkerPos::Top, "Pane top"),
                        (MarkerPos::Bottom, "Pane bottom"),
                        (MarkerPos::Left, "Pane left"),
                        (MarkerPos::Right, "Pane right"),
                        (MarkerPos::AbsoluteUp, "Absolute up"),
                        (MarkerPos::AbsoluteDown, "Absolute down"),
                        (MarkerPos::Absolute, "Absolute"),
                        (MarkerPos::AtPriceTop, "Above explicit price"),
                        (MarkerPos::AtPriceBottom, "Below explicit price"),
                        (MarkerPos::AtPriceMiddle, "On explicit price"),
                    ];
                    egui::ComboBox::from_label("Position")
                        .selected_text(
                            positions
                                .iter()
                                .find(|(position, _)| *position == marker.position)
                                .unwrap()
                                .1,
                        )
                        .show_ui(ui, |ui| {
                            for (position, label) in positions {
                                changed |= ui
                                    .selectable_value(&mut marker.position, position, label)
                                    .changed();
                            }
                        });
                    if matches!(
                        marker.position,
                        MarkerPos::AtPriceTop | MarkerPos::AtPriceBottom | MarkerPos::AtPriceMiddle
                    ) {
                        changed |= ui
                            .add(
                                egui::DragValue::new(marker.price.get_or_insert(bar.close))
                                    .prefix("Price ")
                                    .speed(bar.close.abs().max(1.0) * 0.001),
                            )
                            .changed();
                    }
                    changed |= ui
                        .add(egui::Slider::new(&mut marker.size, 0.5..=4.0).text("Size"))
                        .changed();
                    let label = ui.label("Text");
                    let text = ui
                        .text_edit_singleline(marker.text.get_or_insert_with(String::new))
                        .labelled_by(label.id);
                    changed |= text.changed();
                    changed |= ui.color_edit_button_srgba(&mut marker.color).changed();
                });
                if changed {
                    let mut markers = self.chart.markers(id).to_vec();
                    markers.retain(|marker| marker.id.as_deref() != Some("demo-editor"));
                    if enabled {
                        markers.push(marker);
                    }
                    self.chart.set_markers(id, markers);
                }
                let mut options = self.chart.marker_options(id).unwrap_or_default();
                let mut changed = ui
                    .checkbox(
                        &mut options.auto_scale,
                        "Include explicit price in autoscale",
                    )
                    .changed();
                let layers = [
                    (MarkerZOrder::Normal, "With series"),
                    (MarkerZOrder::AboveSeries, "Above series"),
                    (MarkerZOrder::Top, "Above drawings"),
                ];
                egui::ComboBox::from_label("Layer")
                    .selected_text(
                        layers
                            .iter()
                            .find(|(layer, _)| *layer == options.z_order)
                            .unwrap()
                            .1,
                    )
                    .show_ui(ui, |ui| {
                        for (layer, label) in layers {
                            changed |= ui
                                .selectable_value(&mut options.z_order, layer, label)
                                .changed();
                        }
                    });
                if changed {
                    self.chart.set_marker_options(id, options);
                }
                ui.separator();
                ui.label("Price line");
                let existing = self
                    .chart
                    .price_lines(id)
                    .find(|(_, line)| line.title.as_deref() == Some("Sample line"))
                    .map(|(id, line)| (id, line.clone()));
                let mut line = existing
                    .as_ref()
                    .map(|(_, line)| line.clone())
                    .unwrap_or_else(|| {
                        PriceLine::new(bar.close, self.theme.ui.warning).with_title("Sample line")
                    });
                let mut enabled = existing.is_some();
                let mut changed = ui.checkbox(&mut enabled, "Show price line").changed();
                ui.add_enabled_ui(enabled, |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut line.price)
                                .prefix("Price ")
                                .speed(bar.close.abs().max(1.0) * 0.001),
                        )
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut line.width, 1.0..=5.0).text("Width"))
                        .changed();
                    egui::ComboBox::from_label("Stroke")
                        .selected_text(line.style.name())
                        .show_ui(ui, |ui| {
                            for &style in LineStyle::all() {
                                changed |= ui
                                    .selectable_value(&mut line.style, style, style.name())
                                    .changed();
                            }
                        });
                    changed |= ui.checkbox(&mut line.axis_label, "Axis label").changed();
                    changed |= ui.color_edit_button_srgba(&mut line.color).changed();
                });
                if changed {
                    match (existing, enabled) {
                        (Some((handle, _)), true) => {
                            self.chart.update_price_line(handle, line);
                        }
                        (Some((handle, _)), false) => {
                            self.chart.remove_price_line(handle);
                        }
                        (None, true) => {
                            self.chart.add_price_line(id, line);
                        }
                        (None, false) => {}
                    }
                }
                ui.separator();
                let previous = self.chart.change_markers(id);
                let mut changes = previous.unwrap_or_default();
                let mut enabled = previous.is_some();
                let mut changed = ui.checkbox(&mut enabled, "Live change markers").changed();
                let mut seconds = changes.duration.as_secs_f32();
                changed |= ui
                    .add_enabled(
                        enabled,
                        egui::Slider::new(&mut seconds, 0.2..=20.0).text("Duration (seconds)"),
                    )
                    .changed();
                if changed {
                    changes.duration = std::time::Duration::from_secs_f32(seconds);
                    self.chart
                        .set_change_markers(id, enabled.then_some(changes));
                }
                ui.small("Resume live updates to see the close move. History is kept in place.");
            });
        self.annotation_editor = open;
    }
}
