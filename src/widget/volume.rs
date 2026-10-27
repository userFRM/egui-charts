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
            let chart = ctx.active_theme().chart;
            study.set_colors(vec![chart.volume_up(), chart.volume_down()]);
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

/// The zero-based range of the visible values. With the main bars in `groups`, histograms are
/// read at the highest volume of each group, as [`paint`] draws them.
pub(super) fn range(
    study: &dyn Indicator,
    visible: &Range<usize>,
    groups: Option<&super::merge::Groups>,
) -> (f64, f64) {
    let values = study.values();
    let max = (0..study.line_count())
        .map(|line| (line, study.plot_style(line)))
        .filter(|(_, plot)| plot.visible)
        .map(
            |(line, plot)| match groups.filter(|groups| groups.factor() > 1) {
                Some(groups) if plot.plot_type == PlotType::Histogram => groups.groups
                    [groups.covering(visible.clone())]
                .iter()
                .filter_map(|group| values.value(line, group.peak))
                .fold(0.0, f64::max),
                _ => visible
                    .clone()
                    .filter_map(|bar| values.value(line, bar))
                    .fold(0.0, f64::max),
            },
        )
        .fold(0.0, f64::max);
    (0.0, if max > 0.0 { max } else { 1.0 })
}

/// Paints the visible volume. With the main bars in `groups` of a factor, each histogram draws
/// one column per group, the study's value at the group's highest volume, and each line the
/// [`super::merge::column_points`] of the group.
pub(super) fn paint(
    painter: &Painter,
    rect: Rect,
    study: &dyn Indicator,
    visible: &Range<usize>,
    coords: &ChartMapping,
    min: f64,
    max: f64,
    groups: Option<&super::merge::Groups>,
) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let y = |value: f64| rect.bottom() - ((value - min) / (max - min)) as f32 * rect.height();
    let groups = groups.filter(|groups| groups.factor() > 1);
    let factor = groups.map_or(1, super::merge::Groups::factor);
    let x = |bar| coords.idx_to_x(bar);
    for line in 0..study.line_count() {
        let mut plot = study.plot_style(line);
        if !plot.visible {
            continue;
        }
        let histogram = plot.plot_type == PlotType::Histogram;
        if histogram {
            plot.width = coords.bar_spacing * factor as f32 * 0.8;
        }
        let mut points = Vec::new();
        for group in visible.start / factor..visible.end.div_ceil(factor) {
            let start = points.len();
            match groups {
                None => {
                    if let Some(value) = study.values().value(line, group) {
                        points.push(Pos2::new(x(group), y(value)));
                    }
                }
                Some(groups) if histogram => {
                    let peak = groups.groups[group].peak;
                    if let Some(value) = study.values().value(line, peak) {
                        let last = ((group + 1) * factor).min(study.values().len()) - 1;
                        let centre = (x(group * factor) + x(last)) / 2.0;
                        points.push(Pos2::new(centre, y(value)));
                    }
                }
                Some(_) => {
                    let bars = (group * factor).max(visible.start)
                        ..((group + 1) * factor).min(visible.end);
                    let values =
                        bars.filter_map(|bar| Some((bar, study.values().value(line, bar)?)));
                    points.extend(
                        super::merge::column_points(values)
                            .into_iter()
                            .map(|(bar, value)| Pos2::new(x(bar), y(value))),
                    );
                }
            }
            if points.len() == start && !points.is_empty() {
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
    axis: &crate::config::PriceAxisOptions,
) {
    let Some(bar) = study.values().len().checked_sub(1) else {
        return;
    };
    if !axis.show_study_values
        || !study.is_visible()
        || study.scale() == crate::studies::StudyScale::None
    {
        return;
    }
    // Keep tags opaque even when volume columns use transparency.
    for line in 0..study.line_count() {
        let plot = study.plot_style(line);
        if let Some(value) = study.values().value(line, bar)
            && plot.visible
            && map.range().contains(value)
        {
            let label = study.format_value(value);
            let color = axis.study_value_color.unwrap_or(plot.color).to_opaque();
            occupied.push(crate::chart::rendering::axes::render_axis_value(
                painter,
                rect,
                edge,
                map.price_to_y(value),
                label,
                (
                    axis.study_value_color.map_or(
                        egui::Color32::WHITE,
                        crate::chart::rendering::axes::tag_text_color,
                    ),
                    color,
                ),
                occupied,
                axis.allow_label_overlap,
            ));
        }
    }
}

impl super::Chart {
    pub(super) fn shows_volume(&self) -> bool {
        let visible = self.studies.indicators().iter().any(|study| {
            study.is_volume()
                && study.is_overlay()
                && study.is_visible()
                && (0..study.line_count()).any(|line| study.plot_style(line).visible)
        }) || (!cfg!(feature = "studies") && self.config.show_volume);
        self.main_series_visible && self.chart_type.supports_volume() && visible
    }
}

pub(super) fn mapping(
    coords: &ChartMapping,
    rect: Rect,
    study: &dyn Indicator,
    visible: &Range<usize>,
    groups: Option<&super::merge::Groups>,
) -> ChartMapping {
    let (min, max) = range(study, visible, groups);
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
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("period", InputValue::Length(2))
                .unwrap();
        });
        for (time, redo, period) in [(1.01, false, 9), (1.02, true, 2), (1.03, false, 9)] {
            assert!(if redo { chart.redo() } else { chart.undo() });
            shapes(&ctx, &mut chart, time, vec![]);
            assert!(
                chart.config().show_volume,
                "history must retain the volume switch"
            );
            assert_eq!(
                chart.indicator_mut(id).unwrap().inputs()[0].value,
                InputValue::Length(period)
            );
        }
        let painted = shapes(&ctx, &mut chart, 1.04, vec![]);
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.galley.text() == "1.00M"
                    && text.pos.x >= chart.last_rendered_price_rect.right()
            )),
            "the last column's volume is tagged on the price axis"
        );
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
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("show_average", InputValue::Toggle(true))
            .unwrap();
        let painted = shapes(&ctx, &mut chart, 2.0, vec![]);
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("Volume")
                        && text.galley.text().matches("1.00M").count() == 2
            )),
            "pane legend includes the columns and moving average"
        );
        assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
        assert!(chart.get_rendered_indicator_panes()[0].y_max >= 1_000_000.0);
        assert!(chart.get_chart_mapping().price.range().max < 200.0);
        chart.user_action(|chart| chart.config_mut().show_volume = false);
        shapes(&ctx, &mut chart, 3.0, vec![]);
        assert!(!chart.indicators().any(|(study, _)| study == id));
        assert!(chart.get_rendered_indicator_panes().is_empty());
        assert!(chart.undo());
        shapes(&ctx, &mut chart, 3.1, vec![]);
        assert!(chart.indicators().any(|(study, _)| study == id));
        assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
        assert!(chart.redo());
        shapes(&ctx, &mut chart, 3.2, vec![]);
        assert!(!chart.indicators().any(|(study, _)| study == id));
        chart.config_mut().show_volume = true;
        shapes(&ctx, &mut chart, 4.0, vec![]);
        let next = chart
            .indicators()
            .find(|(_, study)| study.name() == "Volume")
            .unwrap()
            .0;
        assert_ne!(next, id);
        // Pinning to the shared right scale includes the million-unit columns in that
        // range; a shared left scale keeps those units independent of 100..110 prices.
        for (time, key, shares_prices) in [
            (4.1, crate::model::SeriesScale::Right, true),
            (4.2, crate::model::SeriesScale::Left, false),
        ] {
            assert!(chart.set_indicator_scale(next, crate::studies::StudyScale::Shared(key)));
            shapes(&ctx, &mut chart, time, vec![]);
            assert_eq!(
                chart.get_chart_mapping().price.range().max >= 1_000_000.0,
                shares_prices
            );
            let study = chart.indicators().find(|(id, _)| *id == next).unwrap().1;
            let map =
                crate::chart::renderers::study_mapping(study, &chart.get_chart_mapping()).price;
            assert!(map.range().min <= 0.0 && map.range().max >= 1_000_000.0);
            assert!(map.price_to_y(1_000_000.0) < map.price_to_y(0.0));
        }
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
