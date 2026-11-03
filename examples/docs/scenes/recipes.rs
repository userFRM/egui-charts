/// Attach a host level and a marker at the latest stored bar.
pub fn annotations(chart: &mut egui_charts::Chart) {
    use eframe::egui::Color32;
    use egui_charts::{
        model::{Marker, MarkerPos, MarkerShape, PriceLine},
        widget::SeriesId,
    };

    let Some(last) = chart.bars(SeriesId::MAIN).last() else {
        return;
    };
    let (time, close) = (last.time, last.close);
    let color = Color32::from_rgb(126, 87, 194);
    chart.set_price_lines(
        SeriesId::MAIN,
        vec![PriceLine::new(close - 2.0, color).with_title("Reference")],
    );
    chart.set_markers(
        SeriesId::MAIN,
        vec![
            Marker::new(time)
                .with_pos(MarkerPos::BelowBar)
                .with_shape(MarkerShape::ArrowUp)
                .with_color(color)
                .with_text("Signal")
                .with_tooltip("Host annotation at the latest bar")
                .with_id("latest-signal"),
        ],
    );
}

/// Plot the host's volume on its own pane and return the id for streaming updates.
pub fn price_volume(chart: &mut egui_charts::Chart) -> egui_charts::widget::SeriesId {
    use eframe::egui::Color32;
    use egui_charts::{
        ChartType,
        model::Bar,
        widget::{SeriesId, SeriesOptions},
    };

    chart.config_mut().show_volume = false;
    let volume = chart.add_series(SeriesOptions {
        name: "Volume".into(),
        chart_type: ChartType::Columns,
        up_color: Color32::from_rgb(126, 87, 194),
        down_color: Color32::from_rgb(126, 87, 194),
        columns_base: Some(0.0),
        ..Default::default()
    });
    let values = chart
        .bars(SeriesId::MAIN)
        .iter()
        .map(|bar| Bar::from_value(bar.time, bar.volume))
        .collect();
    chart.set_data(volume, values);
    let pane = chart.add_pane();
    chart.move_series_to_pane(volume, pane);
    volume
}

/// Add a reference in the main series' price units and retain its identity.
pub fn second_series(chart: &mut egui_charts::Chart) -> egui_charts::widget::SeriesId {
    use eframe::egui::Color32;
    use egui_charts::{
        model::Bar,
        widget::{SeriesId, SeriesOptions},
    };

    let reference = chart.add_series(SeriesOptions {
        name: "Reference (+1)".into(),
        color: Color32::from_rgb(126, 87, 194),
        ..Default::default()
    });
    let values = chart
        .bars(SeriesId::MAIN)
        .iter()
        .map(|bar| Bar::from_value(bar.time, bar.close + 1.0))
        .collect();
    chart.set_data(reference, values);
    reference
}

/// Keep the last local cursor event for a host legend and an additional tooltip.
#[derive(Default)]
pub struct HostReadout {
    pointer: Option<egui_charts::widget::ChartPointer>,
    clicked: Option<chrono::DateTime<chrono::Utc>>,
}

impl HostReadout {
    /// Show the chart, dispatch its events once, then paint the host tooltip.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, chart: &mut egui_charts::Chart) {
        use eframe::egui;
        use egui_charts::widget::ChartEvent;

        let close = self
            .pointer
            .as_ref()
            .and_then(|point| point.bar.as_ref())
            .filter(|bar| !bar.is_whitespace())
            .map_or_else(
                || "Move over a candle".into(),
                |bar| format!("Host close: {:.2}", bar.close),
            );
        ui.horizontal_wrapped(|ui| {
            ui.label(close);
            if let Some(time) = self.clicked {
                ui.label(format!("Clicked: {} UTC", time.format("%H:%M")));
            }
        });
        chart.show(ui);
        for event in chart.drain_events() {
            match event {
                ChartEvent::CrosshairMoved(pointer) => {
                    self.pointer = pointer;
                    ui.ctx().request_repaint();
                }
                ChartEvent::Click { position, .. } => {
                    self.clicked = position.time;
                    ui.ctx().request_repaint();
                }
                ChartEvent::EventsDropped(_) => {
                    self.pointer = None;
                    self.clicked = None;
                    ui.ctx().request_repaint();
                }
                _ => {}
            }
        }
        if let Some(point) = &self.pointer
            && let Some(bar) = point.bar.as_ref().filter(|bar| !bar.is_whitespace())
        {
            egui::Area::new(ui.scope_id().with("host-readout"))
                .order(egui::Order::Tooltip)
                .fixed_pos(point.pos + egui::vec2(16.0, 90.0))
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("Host tooltip");
                        ui.label(format!("{} UTC", bar.time.format("%H:%M")));
                        ui.label(format!("Close {:.2}", bar.close));
                    });
                });
        }
    }
}

/// Share UTC positions while keeping each chart's price scale independent.
pub struct Synchronized {
    other: egui_charts::Chart,
    last_crosshair: Option<(usize, chrono::DateTime<chrono::Utc>)>,
}

impl Synchronized {
    /// Retain the second chart and draw its prices as a line.
    pub fn new(mut other: egui_charts::Chart) -> Self {
        other.set_chart_type(egui_charts::ChartType::Line);
        other.config_mut().show_volume = false;
        Self {
            other,
            last_crosshair: None,
        }
    }

    /// Show both charts before forwarding the local cursor or a changed viewport.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, chart: &mut egui_charts::Chart) {
        use eframe::egui;

        let mut charts = [chart, &mut self.other];
        let before = charts.each_ref().map(|chart| chart.visible_time_range());
        let height = (ui.available_height() - ui.spacing().item_spacing.y) / 2.0;
        let mut source = None;
        for (i, chart) in charts.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let response = ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| chart.show(ui),
                );
                if ui.rect_contains_pointer(response.response.rect) {
                    source = Some(i);
                }
            });
        }
        if let Some(i) = source {
            let time = charts[i].hover_time();
            let crosshair = time.map(|time| (i, time));
            if crosshair != self.last_crosshair {
                charts[i].set_synced_crosshair_time(None);
                charts[1 - i].set_synced_crosshair_time(time);
                self.last_crosshair = crosshair;
                ui.ctx().request_repaint();
            }
            let range = charts[i].visible_time_range();
            if range != before[i]
                && let Some(range) = range
            {
                charts[1 - i].apply_synced_time_range(range);
                ui.ctx().request_repaint();
            }
        } else if self.last_crosshair.take().is_some() {
            for chart in &mut charts {
                chart.set_synced_crosshair_time(None);
            }
            ui.ctx().request_repaint();
        }
        for chart in &mut charts {
            chart.drain_events().for_each(drop);
        }
    }
}

/// Return encoded image bytes to the host without choosing a file or download destination.
#[derive(Default)]
pub struct Export {
    /// The most recently prepared SVG; the host takes and delivers these bytes once.
    pub bytes: Option<Vec<u8>>,
    error: Option<String>,
}

impl Export {
    /// Capture after showing the chart so the first export has a completed frame.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, chart: &mut egui_charts::Chart) {
        use egui_charts::widget::ImageExportOptions;

        let mut prepare = false;
        ui.horizontal_wrapped(|ui| {
            prepare = ui.button("Prepare SVG").clicked();
            if let Some(bytes) = &self.bytes {
                ui.label(format!("{} SVG bytes ready for the host", bytes.len()));
            }
            if let Some(error) = &self.error {
                ui.label(error);
            }
        });
        chart.show(ui);
        if prepare {
            let mut options = ImageExportOptions::default();
            options.size = [1440, 900];
            options.include_timestamp = false;
            match chart.export_svg(ui.ctx(), &options) {
                Ok(svg) => {
                    self.bytes = Some(svg.into_bytes());
                    self.error = None;
                }
                Err(error) => {
                    self.bytes = None;
                    self.error = Some(error.to_string());
                }
            }
            ui.ctx().request_repaint();
        }
    }
}
