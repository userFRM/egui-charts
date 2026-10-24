//! Volume study layout and painting on its independent zero-based scale.

use crate::chart::renderers::{ChartMapping, paint_study_plot};
use crate::studies::{Indicator, PlotType};
use egui::{Painter, Pos2, Rect};
use std::ops::Range;

#[cfg(feature = "studies")]
impl super::Chart {
    pub(super) fn sync_volume_study(&mut self, ctx: &egui::Context) {
        if self.config.show_volume == self.volume_setting {
            return;
        }
        self.volume_setting = self.config.show_volume;
        if self.config.show_volume {
            let existing = self
                .indicators()
                .find(|(_, study)| study.is_volume())
                .map(|(id, _)| id);
            if let Some(id) = existing {
                self.volume_study = Some(id);
                return;
            }
            use crate::studies::Indicator;
            use crate::theme::ThemeExt;
            let mut study = crate::studies::Volume::default();
            let alpha = ctx.active_theme().chart.volume_alpha;
            study.set_colors(vec![
                egui::Color32::from_rgba_unmultiplied(38, 198, 218, alpha),
                egui::Color32::from_rgba_unmultiplied(236, 64, 122, alpha),
            ]);
            let id = self.add_indicator(Box::new(study));
            self.volume_study = Some(id);
        } else if let Some(id) = self.volume_study.take() {
            self.remove_series(id);
        }
    }
}

pub(super) fn overlay_rect(price: Rect, config: &crate::config::ChartConfig) -> Rect {
    let bottom = price.bottom() - config.volume_baseline_inset.clamp(0.0, price.height());
    Rect::from_min_max(
        Pos2::new(
            price.left(),
            bottom - (bottom - price.top()) * config.volume_height_fraction.clamp(0.0, 1.0),
        ),
        Pos2::new(price.right(), bottom),
    )
}

pub(super) fn range(study: &dyn Indicator, visible: &Range<usize>) -> (f64, f64) {
    let max = (0..study.line_cnt())
        .filter(|line| study.plot_style(*line).visible)
        .flat_map(|line| {
            visible
                .clone()
                .filter_map(move |bar| study.values().value(line, bar))
        })
        .fold(0.0, f64::max);
    (0.0, if max > 0.0 { max } else { 1.0 })
}

pub(super) fn paint(
    painter: &Painter,
    rect: Rect,
    study: &dyn Indicator,
    visible: &Range<usize>,
    coords: &ChartMapping,
    min: f64,
    max: f64,
) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let y = |value: f64| rect.bottom() - ((value - min) / (max - min)) as f32 * rect.height();
    for line in 0..study.line_cnt() {
        let mut plot = study.plot_style(line);
        if !plot.visible {
            continue;
        }
        if plot.plot_type == PlotType::Histogram {
            plot.width = coords.bar_spacing * 0.8;
        }
        let mut points = Vec::new();
        for bar in visible.clone() {
            if let Some(value) = study.values().value(line, bar) {
                points.push(Pos2::new(coords.idx_to_x(bar), y(value)));
            } else if !points.is_empty() {
                paint_study_plot(&painter, &points, plot, y(0.0));
                points.clear();
            }
        }
        if !points.is_empty() {
            paint_study_plot(&painter, &points, plot, y(0.0));
        }
    }
}

pub(super) fn tags(
    painter: &Painter,
    rect: Rect,
    map: &crate::scales::PriceMap,
    study: &dyn Indicator,
    edge: f32,
    occupied: &mut Vec<Option<Rect>>,
) {
    let Some(bar) = study.values().len().checked_sub(1) else {
        return;
    };
    let plot = study.plot_style(2);
    if !study.is_visible() || !plot.visible {
        return;
    }
    if let Some(value) = study.values().value(2, bar)
        && map.range().contains(value)
    {
        let label = crate::scales::PriceFormatter::format(
            &crate::scales::VolumeFormatter {
                precision: study.precision().unwrap_or(3),
            },
            value,
        );
        occupied.push(crate::chart::rendering::axes::render_axis_value(
            painter,
            rect,
            edge,
            map.price_to_y(value),
            label,
            (egui::Color32::WHITE, plot.color),
            occupied,
        ));
    }
}

impl super::Chart {
    pub(super) fn shows_volume(&self) -> bool {
        let visible = self.studies.indicators().iter().any(|study| {
            study.is_volume()
                && study.is_overlay()
                && study.is_visible()
                && (0..study.line_cnt()).any(|line| study.plot_style(line).visible)
        }) || (!cfg!(feature = "studies") && self.config.show_volume);
        self.main_series_visible && self.chart_type.supports_volume() && visible
    }
}

pub(super) fn mapping(
    coords: &ChartMapping,
    rect: Rect,
    study: &dyn Indicator,
    visible: &Range<usize>,
) -> ChartMapping {
    let (min, max) = range(study, visible);
    ChartMapping {
        rect,
        price: crate::scales::PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(min, max),
            rect.top(),
            rect.bottom(),
            false,
        ),
        ..*coords
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::InputValue;
    use crate::widget::test_frames::{chart, shapes};
    use egui::Context;

    #[test]
    fn volume_switch_and_pane_keep_volume_off_the_price_scale() {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        let mut bars = chart.bars(crate::widget::SeriesId::MAIN).to_vec();
        for bar in &mut bars {
            bar.volume = 1_000_000.0;
        }
        chart.set_data(crate::widget::SeriesId::MAIN, bars);
        chart.config_mut().show_volume = false;
        shapes(&ctx, &mut chart, 0.0, vec![]);
        let price_range = chart.get_chart_mapping().price.range();
        chart.config_mut().show_volume = true;
        shapes(&ctx, &mut chart, 1.0, vec![]);
        let id = chart
            .indicators()
            .find(|(_, study)| study.name() == "Volume")
            .expect("volume setting adds the study")
            .0;
        assert_eq!(chart.get_chart_mapping().price.range().min, price_range.min);
        assert_eq!(chart.get_chart_mapping().price.range().max, price_range.max);
        let volume = chart.last_rendered_volume_rect;
        assert!(volume.is_positive());
        let pos = egui::pos2(chart.get_chart_mapping().idx_to_x(90), volume.center().y);
        for (time, pressed) in [(1.1, true), (1.2, false)] {
            shapes(
                &ctx,
                &mut chart,
                time,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(
            chart.selected_element(),
            Some(crate::chart::selection::ChartElementId::Indicator(id))
        );
        chart.set_main_series_visible(false);
        let hidden = shapes(&ctx, &mut chart, 1.3, vec![]);
        assert!(
            !hidden.iter().any(|shape| matches!(shape,
                egui::Shape::Circle(dot) if dot.radius == 4.0
            )),
            "hidden volume must not leave selection handles"
        );
        chart.set_main_series_visible(true);
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("overlay", InputValue::Toggle(false))
            .unwrap();
        let painted = shapes(&ctx, &mut chart, 2.0, vec![]);
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("Volume")
                        && text.galley.text().matches("1.000M").count() == 2
            )),
            "pane legend includes the columns and moving average"
        );
        assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
        assert!(chart.get_rendered_indicator_panes()[0].y_max >= 1_000_000.0);
        assert!(chart.get_chart_mapping().price.range().max < 200.0);
        chart.config_mut().show_volume = false;
        shapes(&ctx, &mut chart, 3.0, vec![]);
        assert!(!chart.indicators().any(|(study, _)| study == id));
        assert!(chart.get_rendered_indicator_panes().is_empty());
        chart.config_mut().show_volume = true;
        shapes(&ctx, &mut chart, 4.0, vec![]);
        let next = chart
            .indicators()
            .find(|(_, study)| study.name() == "Volume")
            .unwrap()
            .0;
        assert_ne!(next, id);
        assert!(chart.remove_series(next));
        shapes(&ctx, &mut chart, 5.0, vec![]);
        assert!(!chart.config().show_volume);
        assert!(
            !chart
                .indicators()
                .any(|(_, study)| study.name() == "Volume")
        );
    }
}
