//! Rendering with separate panes above and below the price pane.

use super::{
    Chart, IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneId, PaneInteraction,
    RenderedIndicatorPane,
};
use crate::chart::indicators::{
    SelectionDotConfig, hit_test_pane_indicator, render_indicator_selection_dots,
};
use crate::chart::selection::ChartElementId;
use crate::chart::series::calculate_dot_interval;
use crate::localization as l10n;
use crate::studies::{CustomIndicator, Indicator};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Response, Sense, Ui};

impl Chart {
    /// Turns the heights requested for the `expanded` panes into stretch factors that give each
    /// its height out of `total`, leaving the others the space they had relative to each other.
    fn apply_pane_heights(&mut self, expanded: &[usize], total: f32) {
        let panes = &mut self.pane_layout.panes;
        let requested: f32 = expanded.iter().filter_map(|idx| panes[*idx].height).sum();
        if requested <= 0.0 {
            return;
        }
        let others: Vec<_> = expanded
            .iter()
            .filter(|idx| panes[**idx].height.is_none())
            .collect();
        let shared: f32 = others.iter().map(|idx| panes[**idx].stretch).sum();
        let scale = if others.is_empty() {
            1.0
        } else {
            shared / (total - requested).max(40.0 * others.len() as f32)
        };
        for idx in expanded {
            if let Some(height) = panes[*idx].height.take() {
                panes[*idx].stretch = height * scale;
            }
        }
    }

    /// Renders the price pane and every other drawn pane in their order, sharing the height by
    /// stretch factor, with x-axes aligned to the price pane and one time axis below them all.
    pub(super) fn show_with_panes(
        &mut self,
        ui: &mut Ui,
        footprint: Option<(
            &crate::data::Footprint,
            &crate::chart::overlays::FootprintConfig,
        )>,
    ) -> Response {
        // Panes are stacked edge to edge with explicit gaps; restore the host spacing after.
        let host_spacing = std::mem::take(&mut ui.spacing_mut().item_spacing);

        let available = ui.available_rect_before_wrap();
        let maximized = self.pane_layout.maximized;
        // The drawn panes from top to bottom, the price pane included, and whether each is
        // collapsed.
        let mut panes: Vec<_> = self
            .pane_layout
            .panes
            .iter()
            .enumerate()
            .filter(|(_, pane)| {
                pane.id == PaneId::MAIN
                    || self.pane_shown(pane) && maximized.is_none_or(|id| id == pane.id)
            })
            .map(|(idx, pane)| (idx, pane.id, false))
            .collect();
        let pane_count = panes.len().saturating_sub(1);
        let axis_height = self.time_axis_height().min(available.height());
        let gap = if maximized.is_some() {
            0.0
        } else {
            DESIGN_TOKENS
                .spacing
                .hairline
                .min((available.height() - axis_height).max(0.0) / pane_count.max(1) as f32)
        };
        let usable_height = (available.height() - axis_height - gap * pane_count as f32).max(0.0);
        let auto_collapse = usable_height / (3.0 + pane_count as f32) < 40.0;
        let row_height = 24.0_f32.min(usable_height / pane_count.max(1) as f32);
        let mut expanded_budget = ((usable_height - row_height * pane_count as f32 - 40.0)
            / (40.0 - row_height))
            .max(0.0) as usize;
        for (idx, id, collapsed) in &mut panes {
            if *id == PaneId::MAIN {
                continue;
            }
            *collapsed = maximized.is_none()
                && (self.pane_layout.panes[*idx]
                    .collapsed
                    .unwrap_or(auto_collapse)
                    || expanded_budget == 0);
            if !*collapsed {
                expanded_budget = expanded_budget.saturating_sub(1);
            }
        }
        let collapsed_count = panes.iter().filter(|(_, _, collapsed)| *collapsed).count();
        let remaining = (usable_height - row_height * collapsed_count as f32).max(0.0);
        let expanded: Vec<_> = panes
            .iter()
            .filter(|(_, _, collapsed)| !collapsed)
            .map(|(idx, _, _)| *idx)
            .collect();
        if maximized.is_none() && remaining > 0.0 {
            self.apply_pane_heights(&expanded, remaining);
        }
        let stretch = |idx: &usize| self.pane_layout.panes[*idx].stretch;
        let total: f32 = expanded.iter().map(stretch).sum();
        let pane_total: f32 = expanded
            .iter()
            .filter(|idx| self.pane_layout.panes[**idx].id != PaneId::MAIN)
            .map(stretch)
            .sum();
        let mut desired: Vec<_> = panes
            .iter()
            .map(|(idx, id, collapsed)| {
                if *id == PaneId::MAIN || *collapsed {
                    0.0
                } else {
                    // Fractions first, so that large stretch factors cannot overflow.
                    let share = |total| remaining * (stretch(idx) / total);
                    share(total).max(40.0).min(share(pane_total))
                }
            })
            .collect();
        if maximized.is_some_and(|id| id != PaneId::MAIN) {
            for (height, (_, id, _)) in desired.iter_mut().zip(&panes) {
                if *id != PaneId::MAIN {
                    *height = usable_height;
                }
            }
        }
        let main_min = if maximized.is_some_and(|id| id != PaneId::MAIN) {
            0.0
        } else {
            40.0
        };
        let expanded_count = expanded.len() - 1;
        let pane_budget = (remaining - main_min).max(0.0);
        let minimum = (pane_budget / expanded_count.max(1) as f32).min(40.0);
        let excess: f32 = desired
            .iter()
            .map(|height| (*height - minimum).max(0.0))
            .sum();
        let factor =
            ((pane_budget - minimum * expanded_count as f32).max(0.0) / excess.max(1.0)).min(1.0);
        for (height, (_, id, collapsed)) in desired.iter_mut().zip(&panes) {
            if *id != PaneId::MAIN && !collapsed {
                *height = minimum + (*height - minimum).max(0.0) * factor;
            }
        }
        let main_height = (remaining - desired.iter().sum::<f32>()).max(0.0);
        let mut top = available.top();
        let mut rects = Vec::with_capacity(panes.len());
        for (at, ((idx, id, collapsed), height)) in panes.iter().zip(&desired).enumerate() {
            if at > 0 {
                top += gap;
            }
            let height = if *id == PaneId::MAIN {
                main_height
            } else if *collapsed {
                row_height
            } else {
                *height
            };
            let rect = Rect::from_min_size(
                Pos2::new(available.left(), top),
                egui::vec2(available.width(), height),
            );
            top += height;
            self.pane_layout.panes[*idx].rect = rect;
            rects.push(rect);
        }
        let main_at = panes
            .iter()
            .position(|(_, id, _)| *id == PaneId::MAIN)
            .expect("the price pane is always drawn");
        let main_rect = rects[main_at];
        let plot = self.pane_layout_rects(main_rect, false).price_rect;
        let time_axis = Rect::from_min_max(
            Pos2::new(plot.left(), available.bottom() - axis_height),
            Pos2::new(plot.right(), available.bottom()),
        );
        let maximized_plot = maximized.filter(|id| *id != PaneId::MAIN).map(|id| {
            ui.interact(
                Rect::from_min_max(
                    Pos2::new(plot.left(), available.top()),
                    Pos2::new(plot.right(), time_axis.top()),
                ),
                ui.scope_id().with((id, "plot")),
                Sense::click_and_drag(),
            )
        });
        let response = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(main_rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
                |ui| self.show_internal(ui, Some(time_axis), maximized_plot.as_ref(), footprint),
            )
            .inner;

        let mut host_clicks = Vec::new();
        let studies = if self.chart_type.is_time_independent() {
            &self.element_studies.studies
        } else {
            &self.studies
        };
        // Render the other panes around the price pane
        let (start_idx, end_idx) = self.state.visible_range();
        let visible_range = start_idx..end_idx;

        // Get coordinate parameters from time scale for x-axis alignment
        let time_scale = self.state.time_scale();
        let coords = IndicatorCoordParams::new(
            time_scale.bar_spacing(),
            time_scale.right_offset_precise(),
            self.state.shown().len().saturating_sub(1),
            start_idx,
        );

        let horizontal = coords.to_mapping(plot, 0.0, 1.0);
        let displayed_range =
            horizontal.x_to_idx(plot.left())..horizontal.x_to_idx(plot.right()).saturating_add(1);
        // The plot column of every pane, where the pointer drives their shared crosshair.
        let hover_rect = Rect::from_min_max(
            Pos2::new(
                plot.left(),
                if main_at == 0 {
                    plot.top()
                } else {
                    available.top()
                },
            ),
            Pos2::new(plot.right(), time_axis.top()),
        );

        // A pane click is resolved after the loop so the immutable study
        // borrow can be released before mutating selection state. `Some`
        // means a pane was clicked; the inner value is the element to
        // select, or `None` to clear (click on empty pane area).
        let mut pending_pane_selection: Option<Option<ChartElementId>> = None;
        // Study whose pane legend "x" was clicked this frame, if any.
        // Removed after the immutable study borrow ends.
        let mut pending_indicator_action = None;
        // A host series whose legend asked to remove it, hide it, or open its settings.
        let mut pending_series_action = None;
        let mut pending_pane_collapse = None;
        let mut pending_pane_maximize = None;
        let current_selection = self.selection.selected_id();
        // Draws the background, grid, axis and crosshair of a pane without a visible study.
        let frame = CustomIndicator::new("", |_| Vec::new())
            .with_overlay(false)
            .with_line_count(0);

        for ((pane_idx, pane_id, collapsed), panel_rect) in
            panes.iter().copied().zip(rects.iter().copied())
        {
            if pane_id == PaneId::MAIN {
                continue;
            }
            #[cfg(feature = "drawings")]
            let eraser_mode = self.drawings.eraser_mode;
            #[cfg(not(feature = "drawings"))]
            let eraser_mode = false;
            #[cfg(feature = "drawings")]
            let drawing_tool_active = self.drawings.active_tool.is_some();
            #[cfg(not(feature = "drawings"))]
            let drawing_tool_active = false;

            let height = panel_rect.height();
            let members = self.pane_layout.panes[pane_idx].members.clone();
            let primary = if self.config.show_right_axis {
                crate::model::SeriesScale::Right
            } else {
                crate::model::SeriesScale::Left
            };
            let series: Vec<_> = members
                .iter()
                .filter_map(|id| {
                    self.price_series
                        .iter()
                        .position(|item| item.id == *id && item.visible)
                })
                .collect();
            // The fitted ranges of the visible host series in view, and the prices of their
            // markers anchored in view, which share the pane scale.
            let prices: Vec<f64> = series
                .iter()
                .flat_map(|at| {
                    let item = &self.price_series[*at];
                    self.series_fit(item)
                        .into_iter()
                        .flat_map(|(min, max)| [min, max])
                        .chain(self.marker_prices(&item.markers, &[], item.marker_options))
                        .chain(self.primitive_prices(item.id))
                })
                .filter(|price| price.is_finite())
                .collect();
            let mut shared_range = (f64::INFINITY, f64::NEG_INFINITY);
            let mut primitive_prices = Vec::new();
            for id in &members {
                let Some(idx) = self.study_ids.iter().position(|study| study == id) else {
                    continue;
                };
                let study = studies.indicators()[idx].as_ref();
                if study.is_visible()
                    && !matches!(study.scale(), crate::studies::StudyScale::Shared(key) if key != primary)
                {
                    let range = IndicatorPane::with_config(IndicatorPaneConfig::from_study(study))
                        .calculate_y_range(study, &displayed_range);
                    shared_range.0 = shared_range.0.min(range.0);
                    shared_range.1 = shared_range.1.max(range.1);
                    primitive_prices.extend(self.primitive_prices(*id));
                }
            }
            for price in primitive_prices.iter().filter(|price| price.is_finite()) {
                shared_range.0 = shared_range.0.min(*price);
                shared_range.1 = shared_range.1.max(*price);
            }
            if let (Some(min), Some(max)) = (
                prices.iter().copied().reduce(f64::min),
                prices.iter().copied().reduce(f64::max),
            ) {
                // Pane studies fit with 10% padding; series in the pane do likewise.
                let padding = crate::scales::auto_scale_span(min, max) * 0.1;
                shared_range.0 = shared_range.0.min(min - padding);
                shared_range.1 = shared_range.1.max(max + padding);
            }
            let first_close = series.iter().find_map(|at| {
                let item = &self.price_series[*at];
                item.bars[self.series_range(item)]
                    .iter()
                    .find(|bar| !bar.is_whitespace())
                    .map(|bar| bar.close)
            });
            let pane_scale = &mut self.pane_layout.panes[pane_idx].scale;
            let mode = pane_scale.options().mode;
            let base = members
                .iter()
                .find_map(|id| {
                    let idx = self.study_ids.iter().position(|study| study == id)?;
                    let study = studies.indicators()[idx].as_ref();
                    if !study.is_visible() {
                        return None;
                    }
                    (0..study.line_count())
                        .filter(|line| {
                            let plot = study.plot_style(*line);
                            plot.visible
                                && !matches!(
                                    plot.plot_type,
                                    crate::studies::PlotType::BarColor
                                        | crate::studies::PlotType::BackgroundColor
                                )
                        })
                        .find_map(|line| {
                            displayed_range
                                .clone()
                                .find_map(|bar| study.plot_value(line, bar))
                        })
                })
                .or(first_close)
                .filter(|base| *base != 0.0)
                .unwrap_or(1.0);
            let transform = crate::scales::PriceTransform { mode, base };
            pane_scale.rebase(transform);
            // Logarithmic scales fit drawable positive values, excluding zero histogram baselines.
            if mode == crate::scales::PriceScaleMode::Logarithmic {
                shared_range = (f64::INFINITY, f64::NEG_INFINITY);
                for id in &members {
                    let Some(idx) = self.study_ids.iter().position(|study| study == id) else {
                        continue;
                    };
                    let study = studies.indicators()[idx].as_ref();
                    if study.is_visible()
                        && !matches!(study.scale(), crate::studies::StudyScale::Shared(key) if key != primary)
                    {
                        for line in (0..study.line_count()).filter(|line| {
                            let plot = study.plot_style(*line);
                            plot.visible
                                && !matches!(
                                    plot.plot_type,
                                    crate::studies::PlotType::BarColor
                                        | crate::studies::PlotType::BackgroundColor
                                )
                        }) {
                            for value in displayed_range
                                .clone()
                                .filter_map(|bar| study.plot_value(line, bar))
                                .filter(|value| *value > 0.0)
                            {
                                shared_range.0 = shared_range.0.min(value);
                                shared_range.1 = shared_range.1.max(value);
                            }
                        }
                    }
                }
                for value in prices
                    .iter()
                    .chain(&primitive_prices)
                    .filter(|value| **value > 0.0)
                {
                    shared_range.0 = shared_range.0.min(*value);
                    shared_range.1 = shared_range.1.max(*value);
                }
            }
            pane_scale.autoscale(
                transform.to_scale(shared_range.0),
                transform.to_scale(shared_range.1),
            );
            let map = pane_scale.map(transform, panel_rect.top(), panel_rect.bottom());
            let mut drawing_response = None;
            let mut axis_tags = Vec::new();
            let hovered = self.last_hover_bar_idx.and_then(|index| {
                let index = index.round() as usize;
                (index < self.state.shown().len()).then_some(index)
            });
            // The legend of each visible host series, with its close at the crosshair or its
            // last close.
            let legends: Vec<_> = series
                .iter()
                .map(|at| {
                    let item = &self.price_series[*at];
                    let close = match hovered {
                        Some(bar) => self.series_close_at(item, bar),
                        None => crate::model::last_with_value(&item.bars).map(|bar| bar.close),
                    };
                    (
                        item.id,
                        super::price_series::SeriesLegend::new(
                            item,
                            close,
                            self.series_label_formatter(
                                item.id,
                                crate::scales::PriceScaleMode::Normal,
                            ),
                            map.price_precision(),
                        ),
                    )
                })
                .collect();
            let pane_plot = Rect::from_x_y_ranges(plot.x_range(), panel_rect.y_range());
            let mut pane_mapping = coords.to_mapping(pane_plot, 0.0, 1.0);
            pane_mapping.price = map;
            let on_top = ui
                .input(|input| input.pointer.hover_pos())
                .filter(|pos| {
                    self.chart_options.hovered_series_on_top
                        && !collapsed
                        && pane_plot.contains(*pos)
                })
                .and_then(|pos| {
                    members.iter().rev().find_map(|id| {
                        if let Some(item) = self
                            .price_series
                            .iter()
                            .find(|item| item.id == *id && item.visible)
                        {
                            self.hit_price_series(item, pos, pane_plot, map)
                                .map(|_| *id)
                        } else {
                            let idx = self.study_ids.iter().position(|study| study == id)?;
                            let study = studies.indicators()[idx].as_ref();
                            let hit = super::primitives::primitive_at(
                                self.primitives_of(*id).map(|item| {
                                    (
                                        item,
                                        self.primitive_view(
                                            *id,
                                            crate::chart::renderers::study_mapping(
                                                study,
                                                &pane_mapping,
                                            ),
                                        ),
                                    )
                                }),
                                pos,
                            )
                            .is_some();
                            (hit || (!self.has_renderer(*id)
                                && hit_test_pane_indicator(
                                    pos,
                                    study,
                                    idx,
                                    visible_range.clone(),
                                    pane_plot,
                                    &pane_mapping,
                                )
                                .is_some()))
                            .then_some(*id)
                        }
                    })
                });
            // Each visible study and host series draws its legend in turn, with the study index
            // of a study; the frame draws a pane with neither.
            let mut entries: Vec<_> = members
                .iter()
                .filter_map(
                    |id| match self.study_ids.iter().position(|study| study == id) {
                        Some(idx) => studies.indicators()[idx]
                            .is_visible()
                            .then_some((*id, Some(idx))),
                        None => legends
                            .iter()
                            .any(|(series, _)| series == id)
                            .then_some((*id, None)),
                    },
                )
                .map(Some)
                .collect();
            if entries.is_empty() {
                entries.push(None);
            }
            // The crosshair lines of a pane with host series are drawn above their bars.
            let mut series_crosshair = None;
            let legend_row_height = ui
                .painter()
                .layout_no_wrap(
                    "Ag".into(),
                    egui::FontId::proportional(ui.active_theme().text.ohlc),
                    ui.active_theme().ui.text,
                )
                .size()
                .y
                + ui.active_theme().spacing.md;
            let legend_order = entries.clone();
            entries.sort_by_key(|entry| entry.is_some_and(|(id, _)| Some(id) == on_top));
            for (paint_row, entry) in entries.into_iter().enumerate() {
                let member_row = legend_order
                    .iter()
                    .position(|original| *original == entry)
                    .unwrap_or(0);
                let id = entry.map(|(id, _)| id);
                let study = entry.and_then(|(_, idx)| idx);
                let indicator: &dyn Indicator = match (study, id) {
                    (Some(idx), _) => studies.indicators()[idx].as_ref(),
                    (None, Some(id)) => legends
                        .iter()
                        .find(|(series, _)| *series == id)
                        .map(|(_, legend)| legend)
                        .expect("a series entry has a legend"),
                    (None, None) => &frame,
                };
                let key = (pane_id, id);
                let mut config = IndicatorPaneConfig::from_study(indicator);
                config.show_name = self.config.show_indicator_labels;
                config.show_values = self.config.show_indicator_last_val;
                config.inputs = match study {
                    Some(idx) if self.config.show_indicator_inputs => {
                        self.study_inputs[idx].clone()
                    }
                    _ => String::new(),
                };
                if self.config.show_indicator_inputs
                    && indicator.is_volume()
                    && indicator.plot_style(2).visible
                    && config.inputs.is_empty()
                {
                    config.inputs = indicator.line_names().get(2).cloned().unwrap_or_default();
                }
                config.show_legend = entry.is_some()
                    && (config.show_name || config.show_values || !config.inputs.is_empty());
                config.height = height;
                config.show_grid &= paint_row == 0;
                let mut panel = IndicatorPane::with_config(config);
                panel.legend_backdrop = self.config.indicator_backdrop_opacity;
                panel.axis_options = crate::config::PriceAxisOptions {
                    show_study_values: self.config.show_indicator_last_val
                        && self.config.price_axis.show_study_values,
                    ..self.config.price_axis
                };
                panel.price_map = Some(
                    if matches!(indicator.scale(), crate::studies::StudyScale::Shared(_)) {
                        crate::chart::renderers::study_mapping(indicator, &pane_mapping).price
                    } else {
                        map
                    },
                );
                if study.is_none() && id.is_some() {
                    panel.price_formatter =
                        id.and_then(|id| self.series_label_formatter(id, map.transform().mode));
                }
                if let crate::studies::StudyScale::Shared(key) = indicator.scale()
                    && let Some((_, axis)) = self
                        .secondary_axes(available, plot)
                        .into_iter()
                        .find(|(scale, _)| *scale == key)
                {
                    panel.axis_rect =
                        Some(Rect::from_x_y_ranges(axis.x_range(), panel_rect.y_range()));
                }
                panel.axis_tags = axis_tags.clone();
                panel.overlay = paint_row > 0;

                panel.legend_offset = member_row as f32 * legend_row_height;
                panel.pane_controls = true;
                // A series legend reports its value at any bar, the last close without a crosshair.
                panel.bar_index = hovered.or(id.filter(|_| study.is_none()).map(|_| 0));
                let mut hover = self
                    .axis_hover
                    .map(|(pos, _)| pos)
                    .or_else(|| {
                        ui.input(|input| input.pointer.hover_pos()).filter(|pos| {
                            hover_rect.contains(*pos) && ui.rect_contains_pointer(hover_rect)
                        })
                    })
                    .or_else(|| {
                        self.synced_crosshair_bar_idx
                            .filter(|index| index.is_finite() && *index >= 0.0)
                            .map(|index| {
                                Pos2::new(
                                    coords
                                        .to_mapping(plot, 0.0, 1.0)
                                        .idx_to_x(index.round() as usize),
                                    plot.center().y,
                                )
                            })
                            .filter(|pos| plot.contains(*pos))
                    })
                    .filter(|_| !egui::Popup::is_any_open(ui.ctx()));
                let mut crosshair_options = self.chart_options.crosshair.resolved(ui.ctx());
                if eraser_mode && crosshair_options.style != crate::config::CrosshairStyle::Hidden {
                    crosshair_options.style = crate::config::CrosshairStyle::Arrow;
                }

                if panel.overlay {
                    hover = None;
                } else if crosshair_options.mode.snaps() {
                    if let Some(pos) = &mut hover
                        && panel_rect.contains(*pos)
                        && let Some(bar) = panel.bar_index
                        && let Some(value) = crate::chart::renderers::snap_to_values(
                            pos.y,
                            &map,
                            self.pane_layout.panes[pane_idx]
                                .members
                                .iter()
                                .filter_map(|id| {
                                    self.study_ids.iter().position(|study| study == id)
                                })
                                .map(|idx| studies.indicators()[idx].as_ref())
                                .filter(|study| study.is_visible())
                                .flat_map(|study| {
                                    (0..study.line_count())
                                        .filter(|line| {
                                            let plot = study.plot_style(*line);
                                            plot.visible
                                                && !matches!(
                                                    plot.plot_type,
                                                    crate::studies::PlotType::BarColor
                                                        | crate::studies::PlotType::BackgroundColor
                                                )
                                        })
                                        .filter_map(move |line| study.plot_value(line, bar))
                                })
                                .chain(series.iter().filter_map(|at| {
                                    self.series_close_at(&self.price_series[*at], bar)
                                })),
                        )
                    {
                        pos.y = map.price_to_y(value);
                    }
                    crosshair_options.mode = crate::config::CrosshairMode::Normal;
                }
                if !series.is_empty()
                    && !collapsed
                    && let Some(pos) = hover
                {
                    // The pane draws the price tag; the lines go above the series' bars.
                    series_crosshair = Some((pos, crosshair_options));
                    crosshair_options.vert_line_color = egui::Color32::TRANSPARENT;
                    crosshair_options.horz_line_color = egui::Color32::TRANSPARENT;
                }

                // Use show_aligned_interactive to get pane info for hit testing
                let mut child =
                    ui.new_child(egui::UiBuilder::new().id_salt(key).max_rect(panel_rect));
                if let Some(interaction) = panel.show_aligned_interactive_in(
                    &mut child,
                    indicator,
                    self.state.shown(),
                    coords,
                    Some(plot.x_range()),
                    collapsed,
                    if matches!(
                        indicator.scale(),
                        crate::studies::StudyScale::Shared(crate::model::SeriesScale::Overlay(_))
                    ) {
                        None
                    } else if matches!(
                        indicator.scale(),
                        crate::studies::StudyScale::Shared(
                            crate::model::SeriesScale::Left
                                | crate::model::SeriesScale::Separate(_)
                        )
                    ) {
                        Some(crate::scales::PriceScaleId::Left)
                    } else if self.config.show_right_axis {
                        Some(crate::scales::PriceScaleId::Right)
                    } else if self.config.show_left_axis {
                        Some(crate::scales::PriceScaleId::Left)
                    } else {
                        None
                    },
                    hover.map(|pos| (pos, crosshair_options)),
                    &id.into_iter()
                        .flat_map(|id| self.primitives_of(id))
                        .collect::<Vec<_>>(),
                    study.is_none() && id.is_some(),
                ) {
                    if !collapsed
                        && let Some(item) =
                            id.and_then(|id| self.price_series.iter().find(|item| item.id == id))
                    {
                        let painter = ui
                            .painter()
                            .with_clip_rect(interaction.chart_rect.intersect(ui.clip_rect()));
                        self.paint_series(&painter, interaction.chart_rect, item, map);
                        let view = self.primitive_view(item.id, pane_mapping);
                        for primitive in self.primitives_of(item.id) {
                            primitive.draw(&painter, &view, super::PrimitivePass::Series);
                        }
                    }
                    self.measured_axis_width =
                        self.measured_axis_width.max(panel.measured_axis_width);
                    axis_tags = panel.axis_tags.clone();
                    let PaneInteraction {
                        panel_rect,
                        chart_rect,
                        y_min,
                        y_max,
                        response: pane_response,
                        close_response,
                        visibility_response,
                        settings_response,
                        menu,
                    } = interaction;

                    let pane_response = match &maximized_plot {
                        Some(plot) => pane_response.union(plot.clone()),
                        None => pane_response.interact(Sense::click_and_drag()),
                    };

                    // A click on the legend's close "x" removes this study or series.
                    let close_clicked = close_response.as_ref().is_some_and(|r| r.clicked());
                    let visibility_clicked =
                        visibility_response.as_ref().is_some_and(|r| r.clicked());
                    let settings_clicked = settings_response.as_ref().is_some_and(|r| r.clicked());
                    let control_clicked =
                        close_clicked || visibility_clicked || settings_clicked || menu.is_some();
                    use crate::chart::renderers::StudyAction;
                    match (study, id) {
                        (Some(idx), _) if close_clicked => {
                            pending_indicator_action = Some(StudyAction::Remove(idx));
                        }
                        (Some(idx), _) if visibility_clicked => {
                            pending_indicator_action = Some(StudyAction::ToggleVisibility(idx));
                        }
                        (Some(idx), _) if settings_clicked => {
                            pending_indicator_action = Some(StudyAction::Settings(idx));
                        }
                        (Some(idx), _) if menu.is_some() => {
                            pending_indicator_action = Some(StudyAction::Menu {
                                index: idx,
                                pos: menu.unwrap(),
                            });
                        }
                        (None, Some(id)) if control_clicked => {
                            pending_series_action = Some((id, close_clicked, visibility_clicked));
                        }
                        _ => {}
                    }
                    let header = Rect::from_min_max(
                        chart_rect.min + egui::vec2(0.0, panel.legend_offset),
                        Pos2::new(
                            visibility_response
                                .as_ref()
                                .map_or(chart_rect.right() - 42.0, |r| {
                                    r.rect.left().min(chart_rect.right())
                                }),
                            (chart_rect.top() + panel.legend_offset + legend_row_height)
                                .min(chart_rect.bottom()),
                        ),
                    );
                    let toggle = ui
                        .interact(header, ui.scope_id().with((key, "toggle")), Sense::click())
                        .on_hover_text(l10n::text(
                            ui.ctx(),
                            if collapsed {
                                "Expand pane"
                            } else {
                                "Collapse pane"
                            },
                        ));
                    let translated_label = l10n::text(
                        ui.ctx(),
                        if collapsed {
                            "Expand pane"
                        } else {
                            "Collapse pane"
                        },
                    );
                    toggle.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            toggle.enabled(),
                            &translated_label,
                        )
                    });
                    ui.ctx()
                        .accesskit_node_builder(toggle.id, |node| node.set_expanded(!collapsed));
                    if toggle.clicked() && !control_clicked {
                        pending_pane_collapse = Some((pane_id, !collapsed));
                        ui.ctx().request_repaint();
                    }
                    let mut pane_coords = coords.to_mapping(chart_rect, y_min, y_max);
                    pane_coords.price = map;
                    // Primitives are hit tested at the pointer, before the chart state is
                    // borrowed mutably.
                    let pointer = ui
                        .input(|input| input.pointer.interact_pos())
                        .filter(|pos| chart_rect.contains(*pos));
                    let pane_primitives = || {
                        self.primitives
                            .iter()
                            .filter(|item| {
                                self.pane_layout.panes[pane_idx]
                                    .members
                                    .contains(&item.series)
                                    && self.is_series_visible(item.series)
                            })
                            .map(|item| {
                                let mut view = self.primitive_view(item.series, pane_coords);
                                if let Some(study) = view.study {
                                    view.mapping =
                                        crate::chart::renderers::study_mapping(study, &pane_coords);
                                }
                                (item, view)
                            })
                    };
                    let primitive_hit = pointer.and_then(|pos| {
                        super::primitives::primitive_at(
                            pane_primitives().filter(|(item, _)| !item.replaces),
                            pos,
                        )
                    });

                    // Resolve a click on this pane: a hit on the line selects
                    // it, an empty-pane click clears the whole selection. A
                    // close-"x" click is handled above and must not also
                    // change the selection.
                    let study_at = |pos| {
                        self.pane_layout.panes[pane_idx]
                            .members
                            .iter()
                            .rev()
                            .find_map(|id| {
                                if self.has_renderer(*id) {
                                    let hit = super::primitives::primitive_at(
                                        pane_primitives().filter(|(item, _)| {
                                            item.replaces && item.series == *id
                                        }),
                                        pos,
                                    )?;
                                    return match hit.target {
                                        super::PointerTarget::Element(id) => Some(id),
                                        _ => None,
                                    };
                                }

                                if let Some(item) = self
                                    .price_series
                                    .iter()
                                    .find(|item| item.id == *id && item.visible)
                                {
                                    return self
                                        .hit_price_series(item, pos, chart_rect, map)
                                        .map(|_| ChartElementId::Series(*id));
                                }
                                let idx = self.study_ids.iter().position(|study| study == id)?;
                                hit_test_pane_indicator(
                                    pos,
                                    studies.indicators()[idx].as_ref(),
                                    idx,
                                    visible_range.clone(),
                                    chart_rect,
                                    &pane_coords,
                                )
                                .map(|hit| {
                                    ChartElementId::Indicator(self.study_ids[hit.indicator_idx])
                                })
                            })
                    };
                    if !collapsed
                        && !toggle.clicked()
                        && !control_clicked
                        && pane_response.clicked()
                        && !drawing_tool_active
                        && !eraser_mode
                        && let Some(click_pos) = pane_response.interact_pointer_pos()
                        && self.mark_at(click_pos).is_none()
                    {
                        let hit = study_at(click_pos);
                        if hit.is_some() || pending_pane_selection.is_none() {
                            pending_pane_selection = Some(hit);
                        }
                    }
                    // What the pointer is over, for host pointer events: drawings sit on top.
                    if !collapsed && let Some(pos) = pointer {
                        #[cfg(feature = "drawings")]
                        let drawing = self.drawings.hit_test(pos);
                        #[cfg(not(feature = "drawings"))]
                        let drawing: Option<usize> = None;
                        let renderer = study_at(pos).and_then(|element| {
                            let ChartElementId::Indicator(id) = element else {
                                return None;
                            };
                            super::primitives::primitive_at(
                                pane_primitives()
                                    .filter(|(item, _)| item.replaces && item.series == id),
                                pos,
                            )
                        });
                        if let Some(hit) = primitive_hit.or(renderer).filter(|_| {
                            drawing.is_none()
                                && pane_response.hovered()
                                && !pane_response.dragged()
                                && !eraser_mode
                                && !drawing_tool_active
                        }) {
                            ui.ctx().set_cursor_icon(hit.cursor);
                        }
                        self.host_events.target = drawing
                            .map(super::PointerTarget::Drawing)
                            .or(primitive_hit.map(|hit| hit.target))
                            .or_else(|| study_at(pos).map(super::PointerTarget::Element));
                    }

                    // Draw selection handles when this pane is selected.
                    if !collapsed
                        && let Some(id) = id
                        && current_selection == Some(ChartElementId::Indicator(id))
                    {
                        let dot_config = SelectionDotConfig {
                            dot_interval: calculate_dot_interval(pane_coords.bar_spacing),
                            ..Default::default()
                        };
                        for line_idx in 0..indicator.line_count() {
                            render_indicator_selection_dots(
                                &ui.painter()
                                    .with_clip_rect(chart_rect.intersect(ui.clip_rect())),
                                indicator,
                                line_idx,
                                visible_range.clone(),
                                &pane_coords,
                                |value| pane_coords.price_to_y(value),
                                &dot_config,
                            );
                        }
                    }

                    drawing_response = Some((pane_response, chart_rect, pane_coords));
                    // Store the pane info for hit testing by platform
                    if let Some(idx) = study {
                        self.last_rendered_indicator_panes
                            .push(RenderedIndicatorPane {
                                indicator_idx: idx,
                                panel_rect,
                                chart_rect,
                                y_min,
                                y_max,
                                coords,
                                mapping: pane_coords,
                            });
                    }
                }
            }
            if let Some((_, chart_rect, _)) = &drawing_response {
                let chart_rect = *chart_rect;
                let painter = ui
                    .painter()
                    .with_clip_rect(panel_rect.intersect(ui.clip_rect()));
                let items: Vec<_> = series.iter().map(|at| &self.price_series[*at]).collect();
                let axis_edge = if self.config.show_right_axis {
                    Some(panel_rect.right())
                } else if self.config.show_left_axis {
                    Some(chart_rect.left())
                } else {
                    None
                };
                if !collapsed && let Some((mut pos, options)) = series_crosshair {
                    if let Some(index) = hovered {
                        pos.x = horizontal.idx_to_x(index);
                    }
                    crate::chart::renderers::render_crosshair_lines(
                        &crate::chart::renderers::RenderContext::new(&painter, chart_rect),
                        pos,
                        &options,
                        chart_rect.bottom(),
                    );
                }
                if !collapsed && let Some(axis_edge) = axis_edge {
                    #[cfg(feature = "drawings")]
                    for (value, fill) in self
                        .drawings
                        .drawings
                        .iter()
                        .filter(|drawing| {
                            drawing.is_visible_on_timeframe(&self.drawings.curr_timeframe)
                                && (drawing.pane == Some(pane_id.0)
                                    || members.iter().any(|id| drawing.pane == Some(id.0)))
                        })
                        .flat_map(|drawing| {
                            drawing.position_price_tags(ui.active_theme().chart.axis_text_secondary)
                        })
                        .filter(|(value, _)| map.range().contains(map.transform().to_scale(*value)))
                    {
                        let label = map.transform().label(
                            map.transform().to_scale(value),
                            crate::chart::rendering::axes::tag_decimals(
                                &map,
                                map.price_to_y(value),
                            ),
                            self.price_formatter.as_deref(),
                        );
                        axis_tags.push(crate::chart::rendering::axes::render_axis_value(
                            &painter,
                            chart_rect,
                            axis_edge,
                            map.price_to_y(value),
                            label,
                            (crate::chart::rendering::axes::tag_text_color(fill), fill),
                            &axis_tags,
                            self.config.price_axis.allow_label_overlap,
                        ));
                    }
                    for (item, (value, colors, last)) in items
                        .iter()
                        .flat_map(|item| {
                            self.series_axis_values(item)
                                .map(move |value| (item, value))
                        })
                        .filter(|(_, (value, ..))| {
                            map.range().contains(map.transform().to_scale(*value))
                        })
                    {
                        let tag = crate::chart::rendering::axes::render_axis_value(
                            &painter,
                            chart_rect,
                            axis_edge,
                            map.price_to_y(value),
                            self.series_tag_label(item, &map, value, last),
                            colors,
                            &axis_tags,
                            self.config.price_axis.allow_label_overlap,
                        );
                        axis_tags.push(tag);
                    }
                }
            }
            if !collapsed && let Some((pane_response, chart_rect, pane_coords)) = drawing_response {
                if pane_response.clicked() || pane_response.secondary_clicked() {
                    host_clicks.push(pane_response.clone());
                }
                #[cfg(feature = "drawings")]
                let drawing_hit = {
                    let history = self.history.start_drawing_action(&mut self.drawings);
                    let mut drawings = std::mem::take(&mut self.drawings);
                    let mut cursors = std::mem::take(&mut self.cursor_modes);
                    let timescale = self.state.time_scale().clone();
                    let painter = ui.painter().clone();
                    self.handle_pane_drawings(
                        ui,
                        &mut drawings,
                        &mut cursors,
                        &pane_response,
                        chart_rect,
                        &map,
                        &painter,
                        &timescale,
                        Some(pane_id),
                    );
                    let drawing_hit = pane_response
                        .interact_pointer_pos()
                        .and_then(|pos| drawings.hit_test(pos));
                    super::tool_interaction::render_pane_eraser_highlight(
                        &ui.painter().with_clip_rect(chart_rect),
                        &drawings,
                        &cursors,
                    );
                    self.drawings = drawings;
                    self.cursor_modes = cursors;
                    self.history
                        .finish_drawing_action(&mut self.drawings, history);
                    drawing_hit
                };
                #[cfg(not(feature = "drawings"))]
                let drawing_hit: Option<usize> = None;
                // Study settings stay on the legend button and the context menu.
                if pane_response.double_clicked() && drawing_hit.is_none() {
                    pending_pane_maximize = Some(pane_id);
                }
                if pane_response.clicked() && drawing_hit.is_some() {
                    pending_pane_selection = None;
                    self.selection.deselect();
                }
                if pane_response.secondary_clicked()
                    && let Some(pos) = pane_response.interact_pointer_pos()
                {
                    self.right_click = Some(if let Some(id) = drawing_hit {
                        #[cfg(feature = "drawings")]
                        {
                            if self.drawings.is_selected(id) {
                                let ids: Vec<_> = std::iter::once(id)
                                    .chain(self.drawings.selected_ids())
                                    .collect();
                                self.drawings.select_drawings(ids);
                            } else {
                                self.drawings.select(id);
                            }
                        }
                        super::RightClickTarget::Drawing {
                            drawing_id: id,
                            pos,
                        }
                    } else {
                        super::RightClickTarget::Background {
                            pos,
                            price: map.y_to_price(pos.y),
                        }
                    });
                }
                // The last pass of the pane's primitives, above its drawings.
                let painter = ui.painter().with_clip_rect(chart_rect);
                for item in self.primitives.iter().filter(|item| {
                    self.pane_layout.panes[pane_idx]
                        .members
                        .contains(&item.series)
                        && self.is_series_visible(item.series)
                }) {
                    let view = self.primitive_view(item.series, pane_coords);
                    item.draw(&painter, &view, super::PrimitivePass::Above);
                }
            }
            if !collapsed {
                let mut table_rect = plot;
                for id in &members {
                    if let Some(idx) = self.study_ids.iter().position(|study| study == id) {
                        let study = &studies.indicators()[idx];
                        if study.is_visible()
                            && let Some(table) = study.table()
                        {
                            table_rect.max.y = crate::studies::table::paint(
                                ui,
                                table_rect,
                                table,
                                ui.scope_id().with((pane_id, id)),
                            )
                            .top()
                                - 8.0;
                        }
                    }
                }
            }
        }
        for response in host_clicks {
            self.record_host_click(ui, &response);
        }
        let drawn: Vec<_> = panes
            .iter()
            .zip(&rects)
            .map(|((idx, _, collapsed), rect)| (*idx, *rect, *collapsed))
            .collect();
        self.pane_separators(ui, &drawn);
        for ((_, id, collapsed), rect) in panes.iter().copied().zip(rects) {
            if id == PaneId::MAIN {
                continue;
            }
            if let Some(idx) = self.pane_layout.panes.iter().position(|pane| pane.id == id) {
                self.pane_scale_interaction(ui, idx, rect, plot);
                self.pane_actions(ui, idx, id, rect, plot, collapsed);
            }
        }

        // Apply the deferred pane selection now that the studies are no longer
        // borrowed.
        if let Some(decision) = pending_pane_selection {
            match decision {
                Some(id) => self.selection.select(id, None),
                None => self.selection.deselect(),
            }
        }

        // The panes reflow on the next frame.
        if let Some(id) = pending_pane_maximize {
            self.user_action(|chart| {
                let maximized = chart.pane_layout.maximized == Some(id);
                chart.pane_layout.maximized = (!maximized).then_some(id);
            });
        }
        if let Some((id, collapsed)) = pending_pane_collapse {
            self.user_action(|chart| {
                if let Some(pane) = chart
                    .pane_layout
                    .panes
                    .iter_mut()
                    .find(|pane| pane.id == id)
                {
                    pane.collapsed = Some(collapsed);
                }
            });
        }
        if let Some(action) = pending_indicator_action {
            self.apply_study_action(action);
        }
        match pending_series_action {
            Some((id, true, _)) => {
                self.user_action(|chart| chart.remove_series(id));
            }
            Some((id, _, true)) => {
                self.user_action(|chart| chart.set_series_visible(id, false));
            }
            Some((id, ..)) => self.indicator_settings_request = Some(id),
            None => {}
        }

        ui.advance_cursor_after_rect(available);
        ui.spacing_mut().item_spacing = host_spacing;
        response
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::{MACD, RSI};
    use crate::widget::test_frames::chart;
    use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

    fn pointer(
        ctx: &Context,
        chart: &mut crate::widget::Chart,
        pos: Pos2,
        pressed: Option<bool>,
    ) -> egui::CursorIcon {
        let mut events = vec![Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        let time = ctx.input(|input| input.time) + 0.1;
        crate::widget::test_frames::frame(ctx, chart, time, events)
    }

    fn click(ctx: &Context, chart: &mut crate::widget::Chart, pos: Pos2) {
        for pressed in [None, Some(true), Some(false), None] {
            pointer(ctx, chart, pos, pressed);
        }
    }

    fn drag(ctx: &Context, chart: &mut crate::widget::Chart, from: Pos2, to: Pos2) {
        pointer(ctx, chart, from, None);
        pointer(ctx, chart, from, Some(true));
        pointer(ctx, chart, from + (to - from) * 0.5, None);
        pointer(ctx, chart, to, None);
        pointer(ctx, chart, to, Some(false));
        pointer(ctx, chart, to, None);
    }

    /// Maximized panes retain shared navigation and leave the hidden price range alone.
    #[test]
    fn maximized_panes_keep_pointer_navigation() {
        use crate::widget::test_frames::{frame, shapes};
        for (theme, width, left, above) in [
            (crate::theme::Theme::light(), 760.0, false, false),
            (crate::theme::Theme::dark(), 760.0, true, true),
            (crate::theme::Theme::light(), 320.0, true, false),
            (crate::theme::Theme::dark(), 320.0, false, true),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            chart.set_theme(&ctx, &theme);
            chart.set_size(Some(Vec2::new(width, 580.0)));
            chart.config.show_right_axis = !left;
            chart.config.show_left_axis = left;
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
            chart.chart_options.crosshair.horz_line_style = crate::model::LineStyle::Solid;
            chart.chart_options.crosshair.horz_line_color = egui::Color32::BLUE;
            let study = chart.add_indicator(Box::new(RSI::new(14)));
            frame(&ctx, &mut chart, 0.0, Vec::new());
            if above {
                chart.move_pane(chart.series_pane(study).unwrap(), 0);
            }
            chart.pane_layout.maximized = chart.series_pane(study);
            frame(&ctx, &mut chart, 0.1, Vec::new());
            frame(&ctx, &mut chart, 0.2, Vec::new());
            let pane = chart.get_rendered_indicator_panes()[0].clone();
            let pos = pane.chart_rect.center();
            let before = chart.state.time_scale().right_offset_precise();
            assert_eq!(chart.state.time_scale().bar_spacing(), 6.0);
            chart.state.price_scale_mut().set_range(90.0, 120.0);
            drag(&ctx, &mut chart, pos, pos + egui::vec2(40.0, 20.0));
            // Forty pixels at six pixels per bar move history by 6 2/3 bars.
            assert!(
                (chart.state.time_scale().right_offset_precise() - (before - 20.0 / 3.0)).abs()
                    < 1e-5
            );
            assert_eq!(chart.state.price_scale().range().min, 90.0);
            assert_eq!(chart.state.price_scale().range().max, 120.0);

            for event in [
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -20.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                },
                Event::Zoom(1.1),
            ] {
                let before = chart.state.time_scale().bar_spacing();
                let time = ctx.input(|input| input.time) + 1.0;
                frame(
                    &ctx,
                    &mut chart,
                    time,
                    vec![Event::PointerMoved(pos), event],
                );
                assert!(chart.state.time_scale().bar_spacing() > before);
            }
            let price_axis = Pos2::new(
                if left {
                    pane.panel_rect.left() + 5.0
                } else {
                    pane.panel_rect.right() - 5.0
                },
                pos.y,
            );
            let time_axis = chart.time_axis_rect.center();
            let before = chart.state.time_scale().bar_spacing();
            drag(
                &ctx,
                &mut chart,
                time_axis,
                time_axis + egui::vec2(30.0, 0.0),
            );
            assert!(chart.state.time_scale().bar_spacing() > before);
            let at = chart
                .pane_layout
                .panes
                .iter()
                .position(|pane| Some(pane.id) == chart.series_pane(study))
                .unwrap();
            drag(
                &ctx,
                &mut chart,
                price_axis,
                price_axis + egui::vec2(0.0, 30.0),
            );
            assert!(chart.pane_layout.panes[at].scale.is_manual());
            for axis in [price_axis, time_axis] {
                let time = ctx.input(|input| input.time) + 1.0;
                frame(&ctx, &mut chart, time, Vec::new());
                for pressed in [None, Some(true), Some(false), Some(true), Some(false)] {
                    pointer(&ctx, &mut chart, axis, pressed);
                }
            }
            assert!(!chart.pane_layout.panes[at].scale.is_manual());
            assert_eq!(
                chart.state.time_scale().bar_spacing(),
                chart.chart_options.time_scale.bar_spacing
            );

            let time = ctx.input(|input| input.time) + 1.0;
            let painted = shapes(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
            assert!(chart.get_hover_bar_idx().is_some());
            assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::BLUE && points.iter().all(|p| (p.y - pos.y).abs() < 0.1))));
            frame(&ctx, &mut chart, time + 0.1, vec![Event::Zoom(1.1)]);
            drag(&ctx, &mut chart, pos, pos + egui::vec2(40.0, 0.0));
            let offset = chart.state.time_scale().right_offset_precise();
            let spacing = chart.state.time_scale().bar_spacing();
            chart.pane_layout.maximized = None;
            pointer(&ctx, &mut chart, Pos2::ZERO, None);
            assert_eq!(chart.state.time_scale().right_offset_precise(), offset);
            assert_eq!(chart.state.time_scale().bar_spacing(), spacing);
        }
    }

    /// Pane drawings retain study units through editing, layout changes and storage.
    #[test]
    #[cfg(feature = "drawings")]
    fn drawings_use_the_owning_study_pane_scale() {
        use crate::drawings::DrawingToolType;
        use crate::scales::PriceScaleMode;
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (mode, inverted, low, high, middle, maximized) in [
                (PriceScaleMode::Normal, false, 0.0, 100.0, 50.0, false),
                (PriceScaleMode::Normal, true, 0.0, 100.0, 50.0, false),
                (
                    PriceScaleMode::Logarithmic,
                    false,
                    10.0,
                    1000.0,
                    100.0,
                    false,
                ),
                (PriceScaleMode::Normal, false, 0.0, 100.0, 50.0, true),
            ] {
                let ctx = Context::default();
                let mut chart = chart(&[1000.0], &[1100.0]);
                chart.set_theme(&ctx, &theme);
                let study = chart.add_indicator(Box::new(RSI::new(14)));
                chart.sync_panes();
                if maximized {
                    chart.pane_layout.maximized = chart.series_pane(study);
                }
                let scale = &mut chart.pane_layout.panes[1].scale;
                scale.options_mut().mode = mode;
                scale.options_mut().invert_scale = inverted;
                let transform = crate::scales::PriceTransform { mode, base: 1.0 };
                scale.rebase(transform);
                scale.set_range(transform.to_scale(low), transform.to_scale(high));
                crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
                let pane = chart.get_rendered_indicator_panes()[0].clone();
                // Halfway between 0 and 100 is 50; halfway between ln(10) and ln(1000) is ln(100).
                let first = Pos2::new(pane.mapping.idx_to_x(30), pane.chart_rect.center().y);
                let second = Pos2::new(pane.mapping.idx_to_x(70), pane.chart_rect.center().y);
                let viewport = chart.get_time_scale_state();
                chart
                    .drawings_mut()
                    .set_active_tool(Some(DrawingToolType::TrendLine));
                click(&ctx, &mut chart, first);
                click(&ctx, &mut chart, second);
                assert_eq!(
                    chart.drawings().drawings.len(),
                    1,
                    "drawing on a study pane"
                );
                let id = chart.drawings().drawings[0].id;
                for point in &chart.drawings().drawings[0].chart_points {
                    assert!((point.price - middle).abs() < 1e-3);
                }
                assert_eq!(chart.drawings().hit_test(first.lerp(second, 0.5)), Some(id));
                assert_eq!(
                    chart
                        .drawings()
                        .hit_test(chart.get_rendered_price_rect().center()),
                    None
                );
                assert!(chart.drawings().is_selected(id));
                chart.drawings_mut().deselect();
                let body = first.lerp(second, 0.25);
                assert_eq!(
                    pointer(&ctx, &mut chart, body, None),
                    egui::CursorIcon::PointingHand
                );
                pointer(&ctx, &mut chart, body, Some(true));
                let body_to = body + egui::vec2(24.0, 10.0);
                assert_eq!(
                    pointer(&ctx, &mut chart, body_to, None),
                    egui::CursorIcon::Grabbing
                );
                pointer(&ctx, &mut chart, body_to, Some(false));
                assert!(chart.drawings().is_selected(id));
                assert_eq!(chart.get_time_scale_state(), viewport);
                assert!(chart.undo());
                pointer(&ctx, &mut chart, first, None);
                assert_eq!(
                    pointer(&ctx, &mut chart, first, None),
                    egui::CursorIcon::ResizeNwSe
                );
                let to = first + egui::vec2(0.0, 15.0);
                drag(&ctx, &mut chart, first, to);
                let moved = chart.drawings().drawings[0].chart_points[0].price;
                assert!((moved - pane.mapping.y_to_price(to.y)).abs() < 1e-3);
                assert_eq!(chart.get_time_scale_state(), viewport);
                assert!(chart.undo());
                assert!((chart.drawings().drawings[0].chart_points[0].price - middle).abs() < 1e-3);
                assert!(chart.redo());
                let saved = chart.save_drawings().unwrap().to_json().unwrap();
                chart.load_drawings(&saved).unwrap();
                chart.pane_layout.maximized = chart.series_pane(study);
                pointer(&ctx, &mut chart, Pos2::ZERO, None);
                let drawing = &chart.drawings().drawings[0];
                let pane = &chart.get_rendered_indicator_panes()[0];
                assert!((drawing.points[0].y - pane.mapping.price_to_y(moved)).abs() < 0.01);
                let on_line = drawing.points[0].lerp(drawing.points[1], 0.5);
                assert_eq!(chart.drawings().hit_test(on_line), Some(drawing.id));
                for width in [320.0, 360.0] {
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 600.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
                        },
                    )
                    .drop_without_applying_deltas();
                    let pane = &chart.get_rendered_indicator_panes()[0];
                    let drawing = &chart.drawings().drawings[0];
                    assert!(pane.chart_rect.width() > 0.0 && pane.chart_rect.right() <= width);
                    assert!((drawing.points[0].y - pane.mapping.price_to_y(moved)).abs() < 0.01);
                    let on_line = drawing.points[0].lerp(drawing.points[1], 0.5);
                    if pane.chart_rect.contains(on_line) {
                        assert_eq!(chart.drawings().hit_test(on_line), Some(drawing.id));
                    }
                }
                chart.set_indicator_visible(study, false);
                pointer(&ctx, &mut chart, Pos2::ZERO, None);
                assert_eq!(
                    chart.drawings().hit_test(on_line),
                    None,
                    "hidden panes have no drawing hits"
                );
            }
        }
    }

    #[test]
    fn pane_separators_resize_neighbors_and_maximize_restores_the_layout() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
            chart.add_indicator(Box::new(RSI::new(14)));
            chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
            let main = chart.get_rendered_price_rect();
            let before = chart.get_rendered_indicator_panes().to_vec();
            let separator = Pos2::new(main.center().x, before[1].panel_rect.top() - 0.5);
            drag(
                &ctx,
                &mut chart,
                separator,
                separator + egui::vec2(0.0, 25.0),
            );
            let resized = chart.get_rendered_indicator_panes().to_vec();
            assert!(
                (resized[0].panel_rect.height() - before[0].panel_rect.height() - 25.0).abs() < 0.1
            );
            assert!(
                (resized[1].panel_rect.height() - before[1].panel_rect.height() + 25.0).abs() < 0.1
            );
            assert_eq!(chart.get_rendered_price_rect(), main);
            let maximize = Pos2::new(
                resized[0].chart_rect.right() - 12.0,
                resized[0].panel_rect.top() + 12.0,
            );
            click(&ctx, &mut chart, maximize);
            let maximized = chart.get_rendered_indicator_panes();
            assert_eq!(maximized.len(), 1);
            assert!(maximized[0].panel_rect.height() > 500.0);
            assert_eq!(maximized[0].chart_rect.x_range(), main.x_range());
            let restore = Pos2::new(
                maximized[0].chart_rect.right() - 12.0,
                maximized[0].panel_rect.top() + 12.0,
            );
            click(&ctx, &mut chart, restore);
            for (restored, resized) in chart.get_rendered_indicator_panes().iter().zip(&resized) {
                assert_eq!(restored.panel_rect, resized.panel_rect);
            }
            // Double-clicking a pane maximizes it and again restores it, without opening settings.
            let inside = resized[0].chart_rect.center();
            for maximized in [true, false] {
                for pressed in [None, Some(true), Some(false), Some(true), Some(false), None] {
                    pointer(&ctx, &mut chart, inside, pressed);
                }
                assert_eq!(
                    chart.get_rendered_indicator_panes().len(),
                    if maximized { 1 } else { 2 }
                );
                assert_eq!(chart.take_indicator_settings_request(), None);
            }
            let separator = Pos2::new(main.center().x, resized[0].panel_rect.top() - 0.5);
            drag(
                &ctx,
                &mut chart,
                separator,
                separator - egui::vec2(0.0, 10_000.0),
            );
            assert!(chart.get_rendered_price_rect().height() >= 39.9);
            assert!(
                chart
                    .get_rendered_indicator_panes()
                    .iter()
                    .all(|pane| pane.panel_rect.height() >= 39.9)
            );
            let last = chart.get_rendered_indicator_panes().last().unwrap();
            assert!((last.panel_rect.bottom() - 557.0).abs() < 0.1);
            chart.pane_options_mut().resizable = false;
            let fixed = chart.get_rendered_indicator_panes().to_vec();
            let separator = Pos2::new(main.center().x, fixed[1].panel_rect.top() - 0.5);
            drag(
                &ctx,
                &mut chart,
                separator,
                separator + egui::vec2(0.0, 25.0),
            );
            for (pane, fixed) in chart.get_rendered_indicator_panes().iter().zip(&fixed) {
                assert_eq!(pane.panel_rect, fixed.panel_rect, "a fixed separator");
            }
            chart.pane_options_mut().resizable = true;
            let mut layout = crate::widget::layout::ChartLayout::default();
            let mut show = |chart: &mut crate::widget::Chart, pos, pressed: Option<bool>| {
                let mut events = vec![Event::PointerMoved(pos)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        time: Some(ctx.input(|input| input.time) + 0.1),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            layout.show(ui, &mut [chart]);
                        });
                    },
                )
                .drop_without_applying_deltas();
            };
            show(&mut chart, Pos2::ZERO, None);
            for maximized in [true, false] {
                let pane = &chart.get_rendered_indicator_panes()[0];
                let button =
                    Pos2::new(pane.chart_rect.right() - 12.0, pane.panel_rect.top() + 12.0);
                for pressed in [None, Some(true), Some(false), None] {
                    show(&mut chart, button, pressed);
                }
                assert_eq!(
                    chart.get_rendered_indicator_panes().len(),
                    if maximized { 1 } else { 2 },
                    "the layout menu must leave pane maximize and restore reachable"
                );
            }
        }
    }

    /// A host series moved into a pane leaves the price scale, panes share the height by
    /// stretch factor across resizes, and a pane can sit above the price pane.
    #[test]
    fn host_panes_take_series_share_height_by_stretch_and_reorder() {
        use crate::widget::{PaneId, SeriesId, SeriesOptions};
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        let series = chart.add_series(SeriesOptions::default());
        let bars = (chart.bars(SeriesId::MAIN).iter().enumerate())
            .map(|(i, bar)| {
                let close = 1000.0 + i as f64;
                crate::model::Bar::new(bar.time, close, close, close, close, 1.0)
            })
            .collect();
        chart.set_data(series, bars);
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.set_pane_stretch_factor(pane, 3.0));
        assert!(chart.move_pane(pane, 0));
        assert_eq!(chart.panes(), [pane, PaneId::MAIN]);
        assert_eq!(chart.pane_series(pane), [series]);
        assert_eq!(chart.pane_series(PaneId::MAIN), [SeriesId::MAIN]);
        let show = |chart: &mut crate::widget::Chart, height: f32| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, height))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
                },
            )
            .drop_without_applying_deltas();
        };
        show(&mut chart, 600.0);
        let heights = |chart: &crate::widget::Chart| {
            (
                chart.pane_height(pane).unwrap(),
                chart.pane_height(PaneId::MAIN).unwrap(),
            )
        };
        let (top, main) = heights(&chart);
        assert!((top - main).abs() < 0.01, "equal factors share equally");
        assert!(chart.pane_layout.panes[0].rect.bottom() <= chart.get_rendered_price_rect().top());
        let range = chart.pane_layout.panes[0].scale.range();
        assert!(range.min < 1000.0 && range.max > 1099.0);
        assert!(chart.get_chart_mapping().price.range().max < 1000.0);
        assert!(chart.set_pane_height(pane, 200.0));
        show(&mut chart, 600.0);
        let (top, main) = heights(&chart);
        assert!((top - 200.0).abs() < 0.01);
        let share = top / (top + main);
        show(&mut chart, 400.0);
        let (top, main) = heights(&chart);
        assert!(
            (top / (top + main) - share).abs() < 1e-4,
            "a resize keeps the shares"
        );
        // A factor near f32::MAX gives the pane most of the height instead of overflowing.
        assert!(chart.set_pane_stretch_factor(pane, 3e38));
        show(&mut chart, 600.0);
        let (top, main) = heights(&chart);
        assert!(top.is_finite() && top > main, "{top} {main}");
        assert!(chart.set_preserve_empty_pane(pane, true));
        assert!(chart.move_series_to_pane(series, PaneId::MAIN));
        assert_eq!(chart.panes(), [pane, PaneId::MAIN]);
        assert!(chart.set_preserve_empty_pane(pane, false));
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.move_series_to_pane(series, PaneId::MAIN));
        assert_eq!(chart.panes(), [PaneId::MAIN]);
    }

    /// The Hide and Remove controls in an added series' legend are one undo step each, and undo
    /// brings the series back with its price formatter and bar limit.
    #[test]
    fn added_series_legend_controls_undo_as_one_step() {
        use crate::scales::PriceScaleMode;
        use crate::widget::{SeriesId, SeriesOptions};
        let limit = std::num::NonZeroUsize::new(1000);
        // A name wider than the pane holds the controls at their rightmost place: 120 px from
        // the plot's right edge, 20 px each, in the order hide, settings, remove.
        for (control, removed) in [(110.0, false), (70.0, true)] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            let series = chart.add_series(SeriesOptions {
                name: "S".repeat(200),
                ..Default::default()
            });
            chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
            chart.set_bar_limit(series, limit);
            assert!(chart.set_series_formatter(
                series,
                Some(std::sync::Arc::new(crate::scales::CurrencyFormatter::usd())),
            ));
            let pane = chart.add_pane();
            assert!(chart.move_series_to_pane(series, pane));
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
            let state = |chart: &crate::widget::Chart| {
                (
                    chart.is_series_visible(series),
                    chart.bars(series).len(),
                    chart.bar_limit(series),
                    chart
                        .series_label_formatter(series, PriceScaleMode::Normal)
                        .is_some(),
                )
            };
            let before = state(&chart);
            assert_eq!(before, (true, 100, limit, true));

            let row = chart.pane_layout.panes.iter().find(|item| item.id == pane);
            let target = Pos2::new(
                chart.get_rendered_price_rect().right() - control,
                row.unwrap().rect.top() + 12.0,
            );
            click(&ctx, &mut chart, target);
            assert!(!chart.is_series_visible(series), "control {control}");
            assert_eq!(chart.series().any(|(id, _)| id == series), !removed);

            assert!(chart.undo(), "control {control} left no undo step");
            assert_eq!(state(&chart), before);
            assert_eq!(chart.pane_series(pane), [series]);
            assert!(!chart.can_undo(), "control {control} took more than a step");
        }
    }

    #[test]
    fn pane_defaults_do_not_depend_on_the_study_name() {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.add_indicator(Box::new(
            crate::studies::CustomIndicator::new("RSI", |bars| {
                bars.iter()
                    .map(|_| crate::studies::IndicatorValue::Single(200.0))
                    .collect()
            })
            .with_overlay(false),
        ));
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        let pane = &chart.get_rendered_indicator_panes()[0];
        // A flat value of 200 gets 10% of a 1% span (2): [199.8, 200.2].
        assert!((pane.y_min - 199.8).abs() < 1e-9);
        assert!((pane.y_max - 200.2).abs() < 1e-9);
    }

    #[test]
    fn merged_panes_share_scales_and_studies_can_return_to_the_main_pane() {
        use crate::scales::PriceScaleMode;
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        let first = chart.add_indicator(Box::new(RSI::new(14)));
        let second = chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        // Reach the pane menu through pointer input; the old pane has no such menu.
        let pane = &chart.get_rendered_indicator_panes()[1];
        let menu = Pos2::new(pane.chart_rect.right() - 32.0, pane.panel_rect.top() + 12.0);
        click(&ctx, &mut chart, menu);
        let shapes = crate::widget::test_frames::shapes(&ctx, &mut chart, 2.0, Vec::new());
        let target = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == "Merge with pane above" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("pane merge action");
        click(&ctx, &mut chart, target);
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            ctx.input(|input| input.time) + 0.1,
            Vec::new(),
        );
        let panes = chart.get_rendered_indicator_panes();
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].panel_rect, panes[1].panel_rect);
        assert_eq!(
            panes[0].mapping.price.range(),
            panes[1].mapping.price.range()
        );
        let axis = Pos2::new(
            panes[0].chart_rect.right() + 10.0,
            panes[0].panel_rect.center().y,
        );
        let before = panes[0].mapping.price.range();
        drag(&ctx, &mut chart, axis, axis + egui::vec2(0.0, 20.0));
        let after = chart.get_rendered_indicator_panes()[0]
            .mapping
            .price
            .range();
        assert!(after.max - after.min > before.max - before.min);
        assert!(chart.pane_layout.panes[1].scale.is_manual());
        for enabled in [false, true] {
            chart
                .chart_options
                .handle_scale
                .axis_double_click_reset
                .price = enabled;
            for pressed in [None, Some(true), Some(false), Some(true), Some(false)] {
                pointer(&ctx, &mut chart, axis, pressed);
            }
            assert_eq!(chart.pane_layout.panes[1].scale.is_manual(), !enabled);
        }
        #[cfg(feature = "ui")]
        {
            let mut menus = crate::ui::context_menu::ChartMenus::default();
            let mut shapes = Vec::new();
            for clicked in [true, false] {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        time: Some(ctx.input(|input| input.time) + 0.1),
                        events: if clicked {
                            vec![
                                Event::PointerMoved(axis),
                                Event::PointerButton {
                                    pos: axis,
                                    button: PointerButton::Secondary,
                                    pressed: true,
                                    modifiers: Modifiers::NONE,
                                },
                                Event::PointerButton {
                                    pos: axis,
                                    button: PointerButton::Secondary,
                                    pressed: false,
                                    modifiers: Modifiers::NONE,
                                },
                            ]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            chart.show(ui);
                        });
                        menus.show(&ctx, &mut chart);
                    },
                );
                shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
            }
            assert!(
                !shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "Lock price to bar ratio")),
                "the main-axis menu must not open over a study axis"
            );
            egui::Popup::close_all(&ctx);
        }
        // The same engine maps 10 and 1000 around 100 at the midpoint in logarithmic units.
        let scale = &mut chart.pane_layout.panes[1].scale;
        scale.options_mut().mode = PriceScaleMode::Logarithmic;
        scale.reset();
        scale.rebase(crate::scales::PriceTransform {
            mode: PriceScaleMode::Logarithmic,
            base: 1.0,
        });
        scale.set_range(10.0_f64.ln(), 1000.0_f64.ln());
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            ctx.input(|input| input.time) + 0.1,
            Vec::new(),
        );
        let pane = &chart.get_rendered_indicator_panes()[0];
        assert!((pane.mapping.price_to_y(100.0) - pane.chart_rect.center().y).abs() < 0.01);
        assert!(chart.move_indicator_to_pane(second, second));
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            ctx.input(|input| input.time) + 0.1,
            Vec::new(),
        );
        assert_ne!(
            chart.get_rendered_indicator_panes()[0].panel_rect,
            chart.get_rendered_indicator_panes()[1].panel_rect
        );
        assert!(chart.move_indicator_to_pane(second, crate::widget::SeriesId::MAIN));
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            ctx.input(|input| input.time) + 0.1,
            Vec::new(),
        );
        assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
        assert!(
            chart
                .indicators()
                .find(|(id, _)| *id == second)
                .unwrap()
                .1
                .is_overlay()
        );
        assert!(chart.move_indicator_to_pane(second, first));
        crate::widget::test_frames::frame(
            &ctx,
            &mut chart,
            ctx.input(|input| input.time) + 0.1,
            Vec::new(),
        );
        assert_eq!(
            chart.get_rendered_indicator_panes()[0].panel_rect,
            chart.get_rendered_indicator_panes()[1].panel_rect
        );
        assert!(!chart.move_indicator_to_pane(first, crate::widget::SeriesId(usize::MAX)));
        // Splitting the original member must leave independent pane controls.
        assert!(chart.move_indicator_to_pane(first, first));
        assert_ne!(chart.pane_layout.panes[1].id, chart.pane_layout.panes[2].id);
        crate::widget::test_frames::frame(&ctx, &mut chart, 20.0, Vec::new());
        let pane = &chart.get_rendered_indicator_panes()[1];
        let maximize = Pos2::new(pane.chart_rect.right() - 12.0, pane.panel_rect.top() + 12.0);
        click(&ctx, &mut chart, maximize);
        assert_eq!(chart.get_rendered_indicator_panes().len(), 1);
        assert_eq!(chart.get_rendered_indicator_panes()[0].indicator_idx, 0);
        for id in [first, second] {
            chart.set_indicator_visible(id, false);
        }
        crate::widget::test_frames::frame(&ctx, &mut chart, 25.0, Vec::new());
        assert_eq!(chart.pane_layout.maximized, None);
        assert!(chart.get_rendered_price_rect().height() > 500.0);
    }

    #[test]
    fn merged_panes_select_visible_lines_and_scale_from_visible_values() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            let studies: Vec<_> = [20.0, 40.0]
                .into_iter()
                .map(|value| {
                    chart.add_indicator(Box::new(
                        CustomIndicator::new("Line", move |bars| {
                            vec![IndicatorValue::Single(value); bars.len()]
                        })
                        .with_overlay(false),
                    ))
                })
                .collect();
            assert!(chart.move_indicator_to_pane(studies[1], studies[0]));
            chart.pane_layout.panes[1].scale.set_range(0.0, 60.0);
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
            for (id, value) in studies.iter().copied().zip([20.0, 40.0]) {
                let pane = &chart.get_rendered_indicator_panes()[0];
                let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.mapping.price_to_y(value));
                click(&ctx, &mut chart, pos);
                assert_eq!(
                    chart.selection.selected_id(),
                    Some(crate::chart::selection::ChartElementId::Indicator(id))
                );
            }
            chart.set_indicator_visible(studies[0], false);
            chart.pane_layout.panes[1].scale.options_mut().mode =
                crate::scales::PriceScaleMode::Percentage;
            chart.pane_layout.panes[1].scale.reset();
            crate::widget::test_frames::frame(&ctx, &mut chart, 5.0, Vec::new());
            let pane = &chart.get_rendered_indicator_panes()[0];
            assert_eq!(pane.mapping.price.transform().to_scale(40.0), 0.0);
        }
    }

    #[test]
    fn panes_fit_and_share_the_lowest_time_axis() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for size in [
                (1440.0, 900.0),
                (1440.0, 868.0),
                (720.0, 450.0),
                (640.0, 450.0),
                (240.0, 450.0),
                (320.0, 450.0),
                (600.0, 400.0),
                (600.0, 180.0),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
                chart.state.time_scale_mut().set_right_offset(0.0);
                let rsi = chart.add_indicator(Box::new(RSI::new(14)));
                chart.config.show_indicator_inputs = true;
                chart.study_inputs[0] =
                    "A long study input caption that must leave room for controls ".repeat(3);
                chart.selection.select(
                    crate::chart::selection::ChartElementId::Indicator(rsi),
                    None,
                );
                chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
                let mut bounds = Rect::NOTHING;
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size))),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            bounds = ui.available_rect_before_wrap();
                            chart.show(ui);
                        });
                    },
                );
                let painted = output.shapes.clone();
                output.drop_without_applying_deltas();
                let price = chart.get_rendered_price_rect();
                let panes = chart.get_rendered_indicator_panes();
                assert_eq!(panes.len(), 2, "{size:?}");
                for pane in panes {
                    assert!(
                        bounds.contains_rect(pane.panel_rect),
                        "{size:?}: {:?} in {bounds:?}",
                        pane.panel_rect
                    );
                    assert_eq!(pane.chart_rect.x_range(), price.x_range());
                }
                let mut dots = 0;
                for shape in &painted {
                    if let egui::Shape::Circle(dot) = &shape.shape
                        && panes[0].chart_rect.contains(dot.center)
                    {
                        assert!(
                            panes[0].chart_rect.contains_rect(shape.clip_rect),
                            "selection dot escapes pane at {size:?}: {:?}",
                            shape.clip_rect
                        );
                        dots += 1;
                    }
                }
                if size.1 > 200.0 {
                    assert!(dots > 0, "missing selection dots");
                }
                let mut legend_seen = false;
                for shape in &painted {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text().starts_with("RSI ")
                    {
                        legend_seen = true;
                        let visible = Rect::from_min_size(text.pos, text.galley.size())
                            .intersect(shape.clip_rect);
                        let controls_left = panes[0].chart_rect.right() - 60.0;
                        assert!(
                            visible.right() <= controls_left,
                            "legend overlaps controls at {size:?}"
                        );
                    }
                }

                assert!(legend_seen, "missing pane legend");
                let last = panes.last().unwrap().panel_rect;
                assert!(
                    (bounds.bottom() - last.bottom() - 35.0).abs() < 1.0,
                    "{size:?}: {last:?} in {bounds:?}"
                );
                if size.1 > 200.0 {
                    assert_eq!(panes[0].panel_rect.height(), 138.0, "{size:?}");
                    assert!(price.height() >= 40.0, "{size:?}");
                } else {
                    assert_eq!(last.height(), 24.0);
                }
                fn check(shape: &egui::Shape, bottom: f32, clip: Rect, count: &mut usize) {
                    match shape {
                        egui::Shape::Vec(shapes) => {
                            shapes.iter().for_each(|s| check(s, bottom, clip, count))
                        }
                        egui::Shape::Text(text)
                            if text.galley.text().contains("Jan")
                                || text.galley.text().contains("Dec")
                                || text.galley.text().contains("Feb")
                                || text.galley.text().contains(':')
                                || (text.pos.y >= bottom
                                    && text.galley.text().chars().all(|c| c.is_ascii_digit())) =>
                        {
                            assert!(
                                text.pos.y >= bottom,
                                "time label above lowest pane: {:?}",
                                text.pos
                            );
                            assert!(
                                text.pos.y >= clip.top()
                                    && text.pos.y + text.galley.size().y <= clip.bottom(),
                                "time label outside painter clip: {:?} in {clip:?}",
                                text.pos
                            );
                            *count += 1;
                        }
                        _ => {}
                    }
                }
                let mut count = 0;
                for shape in &painted {
                    check(&shape.shape, last.bottom(), shape.clip_rect, &mut count);
                }
                assert!(count > 0, "missing time labels at {size:?}");
                let header =
                    chart.get_rendered_indicator_panes()[0].chart_rect.min + egui::vec2(10.0, 12.0);
                let was_collapsed = size.1 <= 200.0;
                for events in [
                    vec![Event::PointerMoved(header)],
                    vec![Event::PointerButton {
                        pos: header,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    }],
                    vec![Event::PointerButton {
                        pos: header,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }],
                    Vec::new(),
                ] {
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                chart.show(ui);
                            });
                        },
                    )
                    .drop_without_applying_deltas();
                }
                let panes = chart.get_rendered_indicator_panes();
                assert_eq!(panes.len(), 2, "header click removed a study");
                if was_collapsed {
                    assert!(
                        panes[0].panel_rect.height() >= 40.0,
                        "pane did not restore at {size:?}"
                    );
                } else {
                    assert_eq!(
                        panes[0].panel_rect.height(),
                        24.0,
                        "pane did not collapse at {size:?}"
                    );
                }
                assert!(
                    panes
                        .iter()
                        .all(|pane| bounds.contains_rect(pane.panel_rect))
                );
                // A saved expanded state must still fit after the host becomes shorter.
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(size.0, 107.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            bounds = ui.available_rect_before_wrap();
                            chart.show(ui);
                        });
                    },
                )
                .drop_without_applying_deltas();
                for pane in chart.get_rendered_indicator_panes() {
                    assert!(bounds.contains_rect(pane.panel_rect));
                    assert_eq!(
                        pane.panel_rect.height(),
                        24.0,
                        "short panes collapse after resize"
                    );
                }
            }
        }
    }
    #[test]
    fn every_pane_shares_hover_readouts_and_uses_its_own_horizontal_scale() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::test_frames::{frame, shapes};
        for (left, theme) in [
            (false, crate::theme::Theme::light()),
            (true, crate::theme::Theme::light()),
            (false, crate::theme::Theme::dark()),
            (true, crate::theme::Theme::dark()),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            chart.set_instrument(Some(
                crate::model::Instrument::new("SAMPLE").with_volume_decimals(3),
            ));
            chart.config.show_right_axis = !left;
            chart.config.show_left_axis = left;
            for (name, base) in [("Readout", 20.0), ("Other", 30.0)] {
                chart.add_indicator(Box::new(
                    CustomIndicator::new(name, move |bars| {
                        (0..bars.len())
                            .map(|index| IndicatorValue::Single(base + index as f64 / 10.0))
                            .collect()
                    })
                    .with_overlay(false),
                ));
            }
            let overlay = chart.add_indicator(Box::new(CustomIndicator::new("Overlay", |bars| {
                vec![IndicatorValue::Single(107.25); bars.len()]
            })));
            chart.chart_options.crosshair.vert_line_style = crate::model::LineStyle::Solid;
            chart.chart_options.crosshair.horz_line_style = crate::model::LineStyle::Solid;
            chart.chart_options.crosshair.vert_line_color = egui::Color32::RED;
            chart.chart_options.crosshair.horz_line_color = egui::Color32::BLUE;
            frame(&ctx, &mut chart, 0.0, vec![]);
            frame(&ctx, &mut chart, 0.1, vec![]);
            frame(&ctx, &mut chart, 0.2, vec![]);
            let map = chart.get_chart_mapping();
            let panes = chart.get_rendered_indicator_panes();
            let rects = [
                chart.last_rendered_price_rect,
                chart.last_rendered_volume_rect,
                panes[0].chart_rect,
                panes[1].chart_rect,
            ];
            for (step, rect) in rects.iter().enumerate() {
                let pos = Pos2::new(map.idx_to_x(70) + 1.0, rect.center().y);
                let painted = shapes(
                    &ctx,
                    &mut chart,
                    1.0 + step as f64,
                    vec![Event::PointerMoved(pos)],
                );
                assert_eq!(
                    chart
                        .get_hover_bar_idx()
                        .map(|index| index.round() as usize),
                    Some(70)
                );
                let text: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                if step == 1 {
                    assert!(
                        text.contains(&"0.500"),
                        "volume cursor lost its precision: {text:?}"
                    );
                }
                for expected in ["Readout 27.00", "Other 37.00", "Open"] {
                    assert!(
                        text.iter().any(|text| text
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            == expected),
                        "{expected}: {text:?}"
                    );
                }
                assert!(
                    painted.iter().any(
                        |shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::BLUE && points[0] == Pos2::new(rect.left(), pos.y)
                    && points[1] == Pos2::new(rect.right(), pos.y))
                    ),
                    "horizontal in {rect:?}"
                );
                for pane in &rects[2..] {
                    assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::RED && points[0] == Pos2::new(map.idx_to_x(70), pane.top())
                        && points[1] == Pos2::new(map.idx_to_x(70), pane.bottom()))), "vertical in {pane:?}");
                }
            }

            // Empty space still has a cursor in every pane, without selecting a bar.
            let pos = Pos2::new(map.idx_to_x(102), rects[2].center().y);
            assert!(rects[2].contains(pos));
            let painted = shapes(&ctx, &mut chart, 9.0, vec![Event::PointerMoved(pos)]);
            assert_eq!(chart.get_hover_bar_idx(), None);
            for pane in &rects[2..] {
                assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::RED && points[0] == Pos2::new(pos.x, pane.top())
                        && points[1] == Pos2::new(pos.x, pane.bottom()))), "missing cursor in empty pane space");
            }

            chart.set_synced_crosshair_bar_idx(Some(70.0));
            let painted = shapes(&ctx, &mut chart, 9.5, vec![Event::PointerGone]);
            for pane in &rects[2..] {
                assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                    if stroke.color == egui::Color32::RED && points[0] == Pos2::new(map.idx_to_x(70), pane.top())
                        && points[1] == Pos2::new(map.idx_to_x(70), pane.bottom()))), "missing synced cursor in pane");
            }
            chart.set_synced_crosshair_bar_idx(None);
            let painted = shapes(&ctx, &mut chart, 10.0, vec![Event::PointerGone]);
            assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text().split_whitespace().collect::<Vec<_>>().join(" ") == "Readout 29.90")));
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
            // With OHLC at 100 and 110, 107.25 can only come from the overlay.
            // Each pane contains 20 or 30 plus i/10, so bar 70 reads 27 or 37.
            for (step, (rect, value, y)) in [
                (rects[0], 107.25, map.price_to_y(107.25)),
                (
                    rects[2],
                    27.0,
                    rects[2].bottom() - (27.0 - 19.01) / 11.88 * rects[2].height(),
                ),
                (
                    rects[3],
                    37.0,
                    rects[3].bottom() - (37.0 - 29.01) / 11.88 * rects[3].height(),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let painted = shapes(
                    &ctx,
                    &mut chart,
                    20.0 + step as f64,
                    vec![Event::PointerMoved(Pos2::new(map.idx_to_x(70), y + 1.0))],
                );
                assert!(
                    painted.iter().any(
                        |shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::BLUE && (points[0].y - y).abs() < 0.5
                    && points[0].x == rect.left())
                    ),
                    "snap to {value}"
                );
            }
            chart.set_indicator_visible(overlay, false);
            let painted = shapes(
                &ctx,
                &mut chart,
                30.0,
                vec![Event::PointerMoved(Pos2::new(
                    map.idx_to_x(70),
                    map.price_to_y(107.25),
                ))],
            );
            assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke }
            if stroke.color == egui::Color32::BLUE && (points[0].y - map.price_to_y(110.0)).abs() < 0.01)), "hidden study must not attract the cursor");
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
            for (offset, action) in [(30.0, "settings"), (50.0, "remove"), (10.0, "hide")] {
                let painted = shapes(&ctx, &mut chart, 40.0, vec![Event::PointerGone]);
                let pane = &chart.get_rendered_indicator_panes()[0];
                let study = chart.study_ids[pane.indicator_idx];
                let title = painted
                    .iter()
                    .find_map(|shape| match shape {
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with("Readout ")
                                || text.galley.text().starts_with("Other ") =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                    .unwrap();
                let pos = title.pos
                    + egui::vec2(
                        title.galley.size().x + crate::tokens::DESIGN_TOKENS.spacing.md + offset,
                        10.0,
                    );
                for pressed in [None, Some(true), Some(false)] {
                    let mut events = vec![Event::PointerMoved(pos)];
                    if let Some(pressed) = pressed {
                        events.push(Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    frame(&ctx, &mut chart, 41.0, events);
                }
                match action {
                    "settings" => {
                        assert_eq!(chart.take_indicator_settings_request(), Some(study));
                        assert!(chart.get_rendered_indicator_panes()[0].chart_rect.height() > 24.0);
                    }
                    "remove" => assert!(!chart.indicators().any(|(id, _)| id == study)),
                    _ => assert!(
                        !chart
                            .indicators()
                            .find(|(id, _)| *id == study)
                            .unwrap()
                            .1
                            .is_visible()
                    ),
                }
                if action != "settings" {
                    assert!(chart.undo());
                    assert!(
                        chart
                            .indicators()
                            .any(|(id, indicator)| id == study && indicator.is_visible())
                    );
                    assert!(chart.redo());
                }
            }
        }
    }
}
