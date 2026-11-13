use egui_charts::Chart;

/// Add a host-fed line and update its forming value without changing its identity.
pub fn lifecycle() -> Chart {
    use eframe::egui::Color32;
    use egui_charts::{
        model::Bar,
        widget::{SeriesId, SeriesOptions},
    };
    let mut chart = crate::first_chart::first_chart();
    let bars = chart.bars(SeriesId::MAIN).to_vec();
    let id = chart.add_series(SeriesOptions {
        name: "Reference".into(),
        color: Color32::from_rgb(156, 97, 229),
        ..Default::default()
    });
    chart.set_data(
        id,
        bars.iter()
            .map(|bar| Bar::from_value(bar.time, bar.close + 1.0))
            .collect(),
    );
    let last = bars.last().unwrap();
    chart.update(id, Bar::from_value(last.time, last.close + 1.5));
    chart
}

/// Style the bodies and wicks without replacing the input bars.
pub fn candles() -> Chart {
    use egui_charts::ChartType;
    let mut chart = crate::first_chart::first_chart();
    chart.set_chart_type(ChartType::Candles);
    let mut params = chart.chart_type_params().clone();
    params.styles.candles.wick_visible = true;
    params.styles.candles.color_by_previous_close = true;
    chart.set_chart_type_params(params);
    chart
}

/// Draw one scalar per timestamp with visible point markers.
pub fn line() -> Chart {
    use egui_charts::{ChartType, model::Bar, widget::SeriesId};
    let mut chart = crate::first_chart::first_chart();
    let values = chart
        .bars(SeriesId::MAIN)
        .iter()
        .map(|bar| Bar::from_value(bar.time, bar.close))
        .collect();
    chart.set_data(SeriesId::MAIN, values);
    chart.set_chart_type(ChartType::LineWithMarkers);
    chart.config_mut().show_volume = false;
    chart.config_mut().show_legend_volume = false;
    let mut params = chart.chart_type_params().clone();
    params.styles.line_with_markers.width = 2.0;
    params.styles.marker_radius = 3.0;
    chart.set_chart_type_params(params);
    chart
}

/// Fill from the close line toward the bottom of the pane.
pub fn area() -> Chart {
    use eframe::egui::Color32;
    use egui_charts::ChartType;
    let mut chart = crate::first_chart::first_chart();
    chart.set_chart_type(ChartType::Area);
    let mut params = chart.chart_type_params().clone();
    params.styles.area.top_fill = Color32::from_rgba_unmultiplied(41, 98, 255, 115);
    params.styles.area.bottom_fill = Color32::TRANSPARENT;
    chart.set_chart_type_params(params);
    chart
}

/// Compare the close with a fixed price and fade each fill toward the base.
pub fn baseline() -> Chart {
    use eframe::egui::Color32;
    use egui_charts::{ChartType, model::BaselineLevel};
    let mut chart = crate::first_chart::first_chart();
    chart.set_chart_type(ChartType::Baseline);
    let mut params = chart.chart_type_params().clone();
    params.baseline = BaselineLevel::Price(108.0);
    params.styles.baseline.above_fill_at_base = Some(Color32::TRANSPARENT);
    params.styles.baseline.below_fill_at_base = Some(Color32::TRANSPARENT);
    chart.set_chart_type_params(params);
    chart
}

/// Plot volume as the scalar value of columns rising from zero.
pub fn histogram() -> Chart {
    use egui_charts::{ChartType, model::Bar, widget::SeriesId};
    let mut chart = crate::first_chart::first_chart();
    let values = chart
        .bars(SeriesId::MAIN)
        .iter()
        .map(|bar| Bar::from_value(bar.time, bar.volume))
        .collect();
    chart.set_data(SeriesId::MAIN, values);
    chart.set_chart_type(ChartType::Columns);
    chart.config_mut().show_volume = false;
    chart.config_mut().show_legend_volume = false;
    let mut params = chart.chart_type_params().clone();
    params.columns_base = Some(0.0);
    chart.set_chart_type_params(params);
    chart.price_scale_mut().set_range(0.0, 1_500.0);
    chart
}
