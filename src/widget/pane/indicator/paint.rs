use super::{IndicatorCoordParams, IndicatorPane, layout};
use crate::model::Bar;
use crate::painting::dashed_line::draw_dashed_line;
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::Indicator;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::ops::Range;

impl IndicatorPane {
    pub(super) fn draw_zone_fills(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        indicator: &dyn Indicator,
    ) {
        if self.config.fill_between_zones {
            let lower = self
                .config
                .zones
                .iter()
                .map(|z| z.0)
                .fold(f64::INFINITY, f64::min);
            let upper = self
                .config
                .zones
                .iter()
                .map(|z| z.0)
                .fold(f64::NEG_INFINITY, f64::max);
            if lower < upper {
                painter.rect_filled(
                    Rect::from_two_pos(
                        Pos2::new(rect.left(), self.value_to_y(upper, rect, y_min, y_max)),
                        Pos2::new(rect.right(), self.value_to_y(lower, rect, y_min, y_max)),
                    )
                    .intersect(rect),
                    0.0,
                    indicator.level_fill_color(),
                );
            }
            return;
        }
        // Design-token colours shade beyond labelled levels only for studies without their own
        // presentation; a configured study shows the colours of its Style tab.
        if indicator.has_settings() {
            return;
        }
        let colors = painter.ctx().active_theme().chart;
        let (bullish, bearish) = (colors.candle_up, colors.candle_down);
        for (level, color, label) in &self.config.zones {
            let live_color = if *color == DESIGN_TOKENS.semantic.extended.bullish {
                bullish
            } else if *color == DESIGN_TOKENS.semantic.extended.bearish {
                bearish
            } else {
                continue; // Skip neutral zones for fills
            };

            let y = self.value_to_y(*level, rect, y_min, y_max);

            // Create semi-transparent fill color
            let fill_color = Color32::from_rgba_unmultiplied(
                live_color.r(),
                live_color.g(),
                live_color.b(),
                25, // Very transparent
            );

            if label.contains("Overbought") || label.contains("above") {
                let fill_rect = Rect::from_two_pos(
                    Pos2::new(rect.left(), self.value_to_y(y_max, rect, y_min, y_max)),
                    Pos2::new(rect.right(), y),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            } else if label.contains("Oversold") || label.contains("below") {
                let fill_rect = Rect::from_two_pos(
                    Pos2::new(rect.left(), y),
                    Pos2::new(rect.right(), self.value_to_y(y_min, rect, y_min, y_max)),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
        }
    }

    pub(super) fn draw_horizontal_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        y_min: f64,
        y_max: f64,
        grid_color: Color32,
        indicator: &dyn Indicator,
    ) {
        // Draw zone lines with dashed style
        if self.config.show_zones {
            // Design-token colours stand for the grid colour only for studies without their own
            // presentation; a configured study shows the colours of its Style tab.
            let tokens = !indicator.has_settings();
            for (level, color, _label) in &self.config.zones {
                // A hidden level bounds the band but draws no line.
                if *color == Color32::TRANSPARENT {
                    continue;
                }
                let live_color = if tokens
                    && [
                        DESIGN_TOKENS.semantic.extended.bullish,
                        DESIGN_TOKENS.semantic.extended.bearish,
                        DESIGN_TOKENS.semantic.extended.chart_text,
                    ]
                    .contains(color)
                {
                    grid_color
                } else {
                    *color
                };

                let y = self.value_to_y(*level, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    let start = Pos2::new(rect.min.x, y);
                    let end = Pos2::new(rect.max.x, y);
                    let stroke = Stroke::new(1.0_f32, live_color);
                    let pattern = indicator.level_style(*level);
                    if pattern == crate::model::LineStyle::Solid {
                        painter.line_segment([start, end], stroke);
                        continue;
                    }
                    let (dash, gap) = if self.config.fill_between_zones
                        || matches!(
                            pattern,
                            crate::model::LineStyle::Dotted | crate::model::LineStyle::SparseDotted
                        ) {
                        pattern.hairline_pattern()
                    } else {
                        pattern.dash_pattern()
                    }
                    .unwrap_or((5.0, 3.0));
                    draw_dashed_line(painter, start, end, stroke, dash, gap);
                }
            }
        }

        // Draw subtle grid lines
        if self.config.show_grid {
            if let Some(map) = self.price_map {
                for mark in map.marks() {
                    painter.line_segment(
                        [
                            Pos2::new(rect.left(), mark.y_coord),
                            Pos2::new(rect.right(), mark.y_coord),
                        ],
                        Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
                    );
                }
                return;
            }
            let range = y_max - y_min;
            let step = self.calculate_nice_step(range, 5);

            let mut y_val = (y_min / step).ceil() * step;
            while y_val <= y_max {
                let y = self.value_to_y(y_val, rect, y_min, y_max);
                if y >= rect.min.y && y <= rect.max.y {
                    painter.line_segment(
                        [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                        Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
                    );
                }
                y_val += step;
            }
        }
    }

    /// Draw vertical grid lines aligned with main chart
    /// Uses the same coordinate system as the main chart for seamless grid appearance
    pub(super) fn draw_vertical_grid_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        coords: &IndicatorCoordParams,
        grid_color: Color32,
    ) {
        // Create mapping for coordinate conversion (y range doesn't matter for grid)
        let mapping = coords.to_mapping(rect, 0.0, 1.0);

        // Calculate grid interval based on bar spacing (same algorithm as main chart)
        let min_pixel_spacing = layout::MIN_GRID_SPACING;
        let bars_per_grid = (min_pixel_spacing / coords.bar_spacing).ceil().max(1.0) as usize;

        // Round to nice intervals: 1, 2, 5, 10, 20, 50, 100, etc.
        let bars_per_grid = crate::chart::rendering::grid::nice_interval(bars_per_grid);

        // Calculate visible bar range
        let chart_width = rect.width();
        let visible_bars = ((chart_width / coords.bar_spacing).ceil() as usize).saturating_add(2);

        // Use start_idx from coords
        let start_idx = coords.start_idx;

        // Find the first grid line that's visible
        let first_visible_idx = if start_idx > 0 {
            (start_idx / bars_per_grid) * bars_per_grid
        } else {
            0
        };

        // Draw grid lines
        for i in 0..=(visible_bars / bars_per_grid + 2) {
            let grid_idx = first_visible_idx.saturating_add(i.saturating_mul(bars_per_grid));

            // Convert bar index to x coord using ChartMapping
            let x = mapping.idx_to_x(grid_idx);

            // Skip if outside visible area
            if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
                continue;
            }

            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, grid_color),
            );
        }
    }

    pub(super) fn draw_indicator_lines(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        bars: &[Bar],
        visible_range: &Range<usize>,
        y_min: f64,
        y_max: f64,
    ) {
        if indicator.is_volume()
            && !matches!(indicator.scale(), crate::studies::StudyScale::Shared(_))
        {
            let count = visible_range.end.saturating_sub(visible_range.start);
            if count == 0 {
                return;
            }
            let spacing = rect.width() / count as f32;
            let coords = IndicatorCoordParams::new(
                spacing,
                0.0,
                visible_range.end.saturating_sub(1),
                visible_range.start,
            );
            crate::widget::volume::paint(
                painter,
                rect,
                indicator,
                visible_range,
                &coords.to_mapping(rect, y_min, y_max),
                y_min,
                y_max,
                None,
            );
            return;
        }
        let count = visible_range.end.saturating_sub(visible_range.start);
        if count == 0 {
            return;
        }
        let spacing = rect.width() / count as f32;
        let coords = IndicatorCoordParams::new(
            spacing,
            0.0,
            visible_range.end.saturating_sub(1),
            visible_range.start,
        );
        let mapping = coords.to_mapping(rect, y_min, y_max);
        crate::chart::renderers::IndicatorRenderer::render_study(
            &crate::chart::renderers::RenderContext::new(painter, rect),
            indicator,
            bars,
            0,
            &mapping.price,
            &mapping,
        );
    }

    pub(super) fn draw_indicator_lines_aligned(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        bars: &[Bar],
        y_min: f64,
        y_max: f64,
        coords: &IndicatorCoordParams,
    ) {
        let mut mapping = coords.to_mapping(rect, y_min, y_max);
        if let Some(map) = self.price_map {
            mapping.price = map;
        }
        crate::chart::renderers::IndicatorRenderer::render_study(
            &crate::chart::renderers::RenderContext::new(painter, rect),
            indicator,
            bars,
            0,
            &mapping.price,
            &mapping,
        );
    }

    pub(super) fn draw_y_axis_labels(
        &mut self,
        painter: &egui::Painter,
        full_rect: Rect,
        chart_rect: Rect,
        y_min: f64,
        y_max: f64,
        indicator: &dyn Indicator,
        axis: Option<crate::scales::PriceScaleId>,
        crosshair_tag: Option<Rect>,
    ) -> f32 {
        if indicator.scale() == crate::studies::StudyScale::None {
            return 0.0;
        }
        let Some(axis) = axis else {
            return 0.0;
        };
        let left = axis == crate::scales::PriceScaleId::Left;
        let align = if left {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        };
        let axis_edge = self.axis_rect.map_or_else(
            || {
                if left {
                    chart_rect.left()
                } else {
                    full_rect.right()
                }
            },
            |axis| axis.right(),
        );
        let text_color = painter.ctx().active_theme().chart.axis_text;
        let map = self
            .price_map
            .unwrap_or_else(|| {
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(y_min, y_max),
                    chart_rect.top(),
                    chart_rect.bottom(),
                    false,
                )
            })
            .with_price_precision(Some(indicator.precision().unwrap_or(2).min(255) as u8));
        let mut tags = self.axis_tags.clone();
        tags.push(crosshair_tag);
        if self.axis_options.show_study_values {
            if indicator.is_volume()
                && !matches!(indicator.scale(), crate::studies::StudyScale::Shared(_))
            {
                crate::widget::volume::tags(
                    painter,
                    chart_rect,
                    &map,
                    indicator,
                    axis_edge,
                    &mut tags,
                    &self.axis_options,
                );
            } else {
                crate::chart::rendering::axes::render_study_tags(
                    painter,
                    chart_rect,
                    &map,
                    indicator,
                    axis_edge,
                    &mut tags,
                    None,
                    &self.axis_options,
                );
            }
        }
        let mut width = if indicator.is_volume()
            && !matches!(indicator.scale(), crate::studies::StudyScale::Shared(_))
        {
            0.0
        } else {
            crate::chart::rendering::axes::price_axis_width(
                painter,
                &map,
                &[],
                self.price_formatter.as_deref(),
            )
        };
        for tag in tags.iter().flatten() {
            width = width.max(tag.width() + 5.0 + 6.0);
        }
        self.axis_tags = tags.clone();
        if self.overlay {
            return width;
        }
        if self.axis_options.border_visible {
            let x = if left {
                axis_edge - 0.5
            } else {
                chart_rect.right() + 0.5
            };
            painter.vline(
                x,
                chart_rect.y_range(),
                egui::Stroke::new(
                    1.0,
                    self.axis_options
                        .border_color
                        .unwrap_or_else(|| painter.ctx().active_theme().ui.border),
                ),
            );
        }
        let label_x = if left {
            self.axis_rect
                .map_or(chart_rect.left(), |axis| axis.right())
                - DESIGN_TOKENS.spacing.sm
        } else {
            chart_rect.right() + DESIGN_TOKENS.spacing.sm
        };

        for mark in crate::chart::renderers::price_marks(&map, self.price_formatter.as_deref()) {
            let y = mark.y_coord;
            if y >= full_rect.min.y + 10.0 && y <= full_rect.max.y - 10.0 {
                let label = if self.price_formatter.is_some() {
                    mark.label
                } else {
                    indicator.format_value(mark.price)
                };
                let galley = painter.layout_no_wrap(
                    label,
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
                width = width.max(galley.size().x + 5.0 + 8.0);
                let bounds = align.anchor_size(Pos2::new(label_x, y), galley.size());
                if tags.iter().flatten().any(|tag| tag.intersects(bounds)) {
                    continue;
                }
                painter.text(
                    Pos2::new(label_x, y),
                    align,
                    galley.text(),
                    crate::chart::rendering::axes::axis_font(painter.ctx()),
                    text_color,
                );
            }
        }
        width
    }

    /// Layout each value with its plot colour, using a swatch when text would be faint.
    fn legend_job(
        &self,
        indicator: &dyn Indicator,
        text_color: Color32,
        background: Color32,
        font_size: f32,
    ) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        let format = |color| egui::TextFormat::simple(egui::FontId::proportional(font_size), color);
        if self.config.show_name {
            job.append(indicator.name(), 0.0, format(text_color));
        }
        if !self.config.inputs.is_empty() {
            if !job.text.is_empty() {
                job.append(" ", 0.0, format(text_color));
            }
            job.append(&self.config.inputs, 0.0, format(text_color));
        }
        if self.config.show_values
            && let Some(bar) = self
                .bar_index
                .or_else(|| indicator.values().len().checked_sub(1))
        {
            let names = indicator.line_names();
            for line in (0..indicator.line_count()).filter(|line| indicator.legend_visible(*line)) {
                if let Some(value) = indicator.plot_value(line, bar) {
                    if !job.text.is_empty() {
                        job.append(" ", 0.0, format(text_color));
                    }
                    let color = indicator.plot_style(line).color;
                    let readable = crate::theme::contrast(color, background) >= 4.5;
                    if !readable {
                        let mut swatch = format(color);
                        swatch.background = color;
                        swatch.font_id.size = 8.0;
                        swatch.valign = egui::Align::Center;
                        job.append("  ", 0.0, swatch);
                        job.append(" ", 0.0, format(text_color));
                    }
                    if indicator.line_count() > 1
                        && let Some(name) = names.get(line)
                    {
                        job.append(&format!("{name}: "), 0.0, format(text_color));
                    }
                    let label = indicator.format_plot_value(line, value);
                    job.append(
                        &label,
                        0.0,
                        format(if readable { color } else { text_color }),
                    );
                }
            }
        }
        job
    }

    /// Reserves three controls after the legend, keeping them inside short panes.
    pub(super) fn legend_controls_rect(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
    ) -> Option<Rect> {
        if !self.config.show_legend {
            return None;
        }
        let theme = painter.ctx().active_theme();
        let mut job = self.legend_job(indicator, theme.ui.text, theme.chart.bg, theme.text.ohlc);
        job.wrap.max_width = self.legend_width(rect, theme.spacing.sm);
        let galley = painter.layout_job(job);
        let x = (rect.left() + theme.spacing.sm + galley.size().x + theme.spacing.md)
            .min(rect.right() - if self.pane_controls { 120.0 } else { 80.0 })
            .max(rect.left());
        Some(
            Rect::from_min_size(
                Pos2::new(
                    x,
                    rect.top()
                        + self.legend_offset
                        + (theme.text.ohlc + theme.spacing.md - 20.0).max(0.0) / 2.0,
                ),
                egui::vec2(80.0, 20.0),
            )
            .intersect(rect),
        )
    }

    /// Draws the selected bar's study values beside its name and inputs.
    pub(super) fn draw_legend(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        indicator: &dyn Indicator,
        text_color: Color32,
    ) {
        if !self.config.show_legend {
            return;
        }
        let theme = painter.ctx().active_theme();
        let mut job = self.legend_job(indicator, text_color, theme.chart.bg, theme.text.ohlc);
        job.wrap.max_width = self.legend_width(rect, theme.spacing.sm);
        let galley = painter.layout_job(job);
        let pos = Pos2::new(
            rect.left() + theme.spacing.sm,
            rect.top() + self.legend_offset + theme.spacing.sm.min(rect.height() / 2.0),
        );
        painter.rect_filled(
            Rect::from_min_size(pos, galley.size())
                .expand(2.0)
                .intersect(rect),
            0.0,
            theme
                .chart
                .bg
                .gamma_multiply(self.legend_backdrop.clamp(0.0, 1.0)),
        );
        painter.galley(pos, galley, text_color);
    }

    fn legend_width(&self, rect: Rect, padding: f32) -> f32 {
        (rect.width() - if self.pane_controls { 120.0 } else { 80.0 } - 2.0 * padding).max(1.0)
    }

    fn value_to_y(&self, value: f64, rect: Rect, y_min: f64, y_max: f64) -> f32 {
        if let Some(map) = self.price_map {
            map.price_to_y(value)
        } else {
            let norm = (value - y_min) / (y_max - y_min);
            rect.max.y - (norm as f32 * rect.height())
        }
    }

    fn calculate_nice_step(&self, range: f64, target_lines: usize) -> f64 {
        let raw_step = range / target_lines as f64;
        let magnitude = 10f64.powf(raw_step.log10().floor());
        let normalized = raw_step / magnitude;

        let nice = if normalized <= 1.0 {
            1.0
        } else if normalized <= 2.0 {
            2.0
        } else if normalized <= 5.0 {
            5.0
        } else {
            10.0
        };

        nice * magnitude
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::super::IndicatorPaneConfig;
    use super::*;
    use crate::studies::{CustomIndicator, IndicatorValue};

    /// The vertical grids of a time-independent main chart and of an indicator pane keep their
    /// lines the minimum grid spacing apart when a tiny bar spacing shows millions of bars.
    #[test]
    fn vertical_grids_stay_bounded_at_any_bar_spacing() {
        use crate::widget::test_frames::{chart, shapes};
        let mut chart = chart(&[10.0], &[11.0]);
        chart.set_chart_type(crate::model::ChartType::Renko);
        chart.config_mut().grid_line_style = crate::model::LineStyle::Solid;
        chart.add_indicator(Box::new(crate::studies::RSI::default()));
        let mut options = chart.chart_options().clone();
        options.time_scale.max_bar_spacing = 1e-6;
        chart.set_chart_options(options);
        let vertical = shapes(&egui::Context::default(), &mut chart, 0.0, Vec::new())
            .iter()
            .filter(|shape| {
                matches!(shape, egui::Shape::LineSegment { points, .. } if points[0].x == points[1].x)
            })
            .count();
        // An 800 point wide chart has room for ten lines 80 points apart in each grid.
        assert!(vertical <= 30, "{vertical} vertical lines");
    }

    #[test]
    fn rsi_paints_the_band_and_guides_of_its_preset_and_its_level_settings() {
        let config = IndicatorPaneConfig::rsi();
        assert_eq!(config.height, 138.0);
        assert_eq!(
            config.zones.iter().map(|z| z.0).collect::<Vec<_>>(),
            vec![70.0, 30.0]
        );
        let mut edited =
            crate::studies::ConfiguredStudy::new(Box::new(crate::studies::RSI::default()));
        let mut settings = edited.settings();
        settings.levels[0].value = Some(80.0);
        settings.levels[0].color = Color32::WHITE;
        settings.levels[0].pattern = crate::model::LineStyle::LargeDashed;
        settings.levels[1].visible = false;
        settings.level_fill_color = Color32::from_rgba_unmultiplied(1, 2, 3, 40);
        edited.apply_settings(&[], settings).unwrap();
        let bare = crate::studies::RSI::default();
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::dark());
        // Band edges and guide ys, not inverted and inverted, on a 100 point high pane of 0..100.
        let rows = [
            (
                config,
                &bare as &dyn Indicator,
                [(30.0, 70.0), (30.0, 70.0)],
                [&[30.0, 70.0][..], &[30.0, 70.0]],
                crate::studies::palette::GRAY,
                4.0,
                Color32::from_rgba_unmultiplied(126, 87, 194, 26),
            ),
            (
                IndicatorPaneConfig::from_study(&edited),
                &edited,
                [(20.0, 70.0), (30.0, 80.0)],
                [&[20.0][..], &[80.0]],
                Color32::WHITE,
                8.0,
                Color32::from_rgba_unmultiplied(1, 2, 3, 40),
            ),
        ];
        for (config, study, band, guide_ys, color, dash, fill) in rows {
            for inverted in [false, true] {
                let mut pane = IndicatorPane::with_config(config.clone());
                pane.price_map = Some(PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(0.0, 100.0),
                    0.0,
                    100.0,
                    inverted,
                ));
                let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 100.0));
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    pane.draw_zone_fills(ui.painter(), rect, 0.0, 100.0, study);
                    pane.draw_horizontal_lines(
                        ui.painter(),
                        rect,
                        0.0,
                        100.0,
                        Color32::BLACK,
                        study,
                    );
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                let bands: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Rect(r) => Some(r),
                        _ => None,
                    })
                    .collect();
                let (top, bottom) = band[inverted as usize];
                assert_eq!(bands.len(), 1);
                assert!((bands[0].rect.top() - top).abs() < 0.001);
                assert!((bands[0].rect.bottom() - bottom).abs() < 0.001);
                assert_eq!(bands[0].fill, fill);
                let guides: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::LineSegment { points, stroke }
                            if stroke.color != Color32::BLACK =>
                        {
                            Some((points, stroke.color))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!guides.is_empty());
                for (guide, guide_color) in guides {
                    assert_eq!(guide_color, color);
                    assert!(
                        guide_ys[inverted as usize]
                            .iter()
                            .any(|y| (guide[0].y - y).abs() < 0.001)
                    );
                    assert!((guide[1].x - guide[0].x - dash).abs() < 0.001);
                }
            }
        }
    }

    #[test]
    fn volume_units_reach_pane_axes_and_legends() {
        use crate::scales::StudyFormat;
        use crate::studies::OnBalanceVolume;
        let bars: Vec<_> = [10.0, 11.0, 12.0]
            .into_iter()
            .map(|price| Bar::new(chrono::Utc::now(), price, price, price, price, 1_000_000.0))
            .collect();
        let shifted = CustomIndicator::new("Shifted", |_| {
            vec![
                IndicatorValue::Single(0.0),
                IndicatorValue::Single(1_000_000.0),
                IndicatorValue::Single(2_000_000.0),
            ]
        })
        .with_value_format(StudyFormat::Volume)
        .with_offsets(vec![1]);
        for (mut study, expected) in [
            (
                Box::new(OnBalanceVolume::new()) as Box<dyn Indicator>,
                "2.00M",
            ),
            (Box::new(shifted), "1.00M"),
        ] {
            study.calculate(&bars);
            let mut pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
            let job = pane.legend_job(
                study.as_ref(),
                Color32::WHITE,
                Color32::BLACK,
                crate::theme::Theme::default().text.ohlc,
            );
            assert!(job.text.contains(expected), "{}", job.text);
            let ctx = egui::Context::default();
            let mut study = crate::studies::ConfiguredStudy::new(study);
            study.set_overlay(true);
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(0);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
                pane.draw_y_axis_labels(
                    ui.painter(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(300.0, 300.0)),
                    0.0,
                    3_000_000.0,
                    studies[0].as_ref(),
                    Some(crate::scales::PriceScaleId::Right),
                    None,
                );
                let mut legend = crate::chart::renderers::Legend::new("SAMPLE", "1h", &bars[2]);
                legend.studies = &studies;
                legend.bar_index = 2;
                legend.instrument = Some(&instrument);
                legend.show(ui, rect);
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            // Both the last-value tag and overlay legend use displayed values and study units.
            assert_eq!(
                labels
                    .iter()
                    .filter(|label| label.as_str() == expected)
                    .count(),
                2,
                "{labels:?}"
            );
        }
    }

    #[test]
    fn legend_values_identify_each_plot_without_low_contrast_text() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let colors = vec![Color32::BLUE, Color32::YELLOW, Color32::GREEN];
            let mut study = CustomIndicator::new("Lines", |_| {
                vec![IndicatorValue::Multiple(vec![1.0, 2.0, 3.0])]
            })
            .with_colors(colors.clone())
            .with_line_count(3)
            .with_line_names(vec![
                "Ask levels".into(),
                "Bid levels".into(),
                "Total levels".into(),
            ]);
            study.calculate(&[]);
            let mut pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
            for opacity in [0.0, 0.35, 1.0] {
                pane.legend_backdrop = opacity;
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    pane.draw_legend(ui.painter(), ui.max_rect(), &study, theme.ui.text);
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                let text = shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .unwrap();
                let sections = &text.galley.job.sections;
                for name in study.line_names() {
                    assert!(text.galley.text().contains(&format!("{name}: ")));
                }
                let backdrop = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) => Some(rect),
                        _ => None,
                    })
                    .expect("pane legend backdrop");
                assert_eq!(backdrop.fill, theme.chart.bg.gamma_multiply(opacity));
                assert!(
                    backdrop
                        .rect
                        .contains_rect(Rect::from_min_size(text.pos, text.galley.size()))
                );
                for (i, color) in colors.iter().enumerate() {
                    let value = format!("{:.2}", (i + 1) as f64);
                    let section = sections
                        .iter()
                        .find(|s| {
                            text.galley.job.text[s.byte_range.start.0..s.byte_range.end.0]
                                .contains(&value)
                        })
                        .unwrap();
                    assert!(crate::theme::contrast(section.format.color, theme.chart.bg) >= 4.5);
                    assert!(
                        sections.iter().any(|s| s.format.color == *color),
                        "missing plot colour {color:?}"
                    );
                }
                assert_eq!(sections[0].format.color, theme.ui.text);
            }
        }
    }

    /// Four executions each second, two fills per order, give 4 executions/s and 2 orders/s.
    /// A bar's 240 or 1200 executions must not enter either rate's automatic range.
    #[test]
    fn rates_keep_totals_in_the_legend_without_flattening_per_second_plots() {
        use crate::data::OrderFlowKind;
        use crate::model::{BarClock, OrderSide, Tick, Timeframe};
        use crate::studies::{ConfiguredStudy, OrderFlowStudy};
        use chrono::{TimeDelta, TimeZone, Utc};
        let start = Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap();
        for (timeframe, duration) in [(Timeframe::Min1, 60), (Timeframe::Min5, 300)] {
            for (kind, expected_rate, total_name, total) in [
                (
                    OrderFlowKind::TradeRate {
                        volume: false,
                        seconds: 60,
                        instant: 5,
                    },
                    4.0,
                    "Bar executions",
                    duration * 4,
                ),
                (
                    OrderFlowKind::AggressorRate {
                        seconds: 60,
                        instant: 5,
                    },
                    2.0,
                    "Bar orders",
                    duration * 2,
                ),
            ] {
                let clock = BarClock::new(timeframe);
                let mut study = ConfiguredStudy::new(Box::new(OrderFlowStudy::new(kind)));
                study.set_bar_clock(&clock);
                for at in 0..duration * 4 {
                    study.order_flow_inputs()[0].push_trade(
                        &clock,
                        &Tick::new(
                            start + TimeDelta::milliseconds(at * 250),
                            100.0,
                            1.0,
                            Some(OrderSide::Buy),
                        )
                        .with_aggressor_order_id((at / 2) as u64),
                    );
                }
                study.calculate(&[Bar::new(start, 100.0, 100.0, 100.0, 100.0, 0.0)]);
                let pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
                let (min, max) = pane.calculate_y_range(&study, &(0..1));
                assert!(
                    min < expected_rate && max > expected_rate && max < expected_rate * 2.0,
                    "{kind:?}: {min}..{max}"
                );
                let job = pane.legend_job(&study, Color32::WHITE, Color32::BLACK, 12.0);
                assert!(
                    job.text.contains(&format!("{total_name}: {total} ")),
                    "{}",
                    job.text
                );
                assert!(
                    job.text.contains(&format!(
                        "Rolling 60s ({}): {expected_rate:.2}",
                        if expected_rate == 4.0 {
                            "executions/s"
                        } else {
                            "orders/s"
                        }
                    )),
                    "{}",
                    job.text
                );
                assert!(job.text.contains("Instant 5s"), "{}", job.text);
            }
        }
    }

    /// Wrapped legends keep every named output inside the plot and clear of the controls.
    #[test]
    fn multi_output_legend_wraps_inside_narrow_panes() {
        let ctx = egui::Context::default();
        let mut study = CustomIndicator::new("Size ranges", |_| {
            vec![IndicatorValue::Multiple(vec![12.0, 34.0, 56.0])]
        })
        .with_line_count(3)
        .with_line_names(vec![
            "Delta [0, 2] lots".into(),
            "Delta [3, 5] lots".into(),
            "Delta [10, ∞] lots".into(),
        ]);
        study.calculate(&[]);
        let pane = IndicatorPane::with_config(IndicatorPaneConfig::default());
        for width in [320.0, 800.0, 1440.0] {
            let mut controls_left = 0.0;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 150.0));
                let controls = pane
                    .legend_controls_rect(ui.painter(), rect, &study)
                    .unwrap();
                controls_left = controls.left();
                pane.draw_legend(ui.painter(), rect, &study, Color32::WHITE);
            });
            output.textures_delta.clear();
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    assert!(text.pos.x + text.galley.size().x <= controls_left);
                    if width == 320.0 {
                        assert!(text.galley.rows.len() > 1);
                    }
                    assert!(!text.galley.elided);
                }
            }
            output.drop_without_applying_deltas();
        }
    }
}
