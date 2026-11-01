//! Rendering with separate panes above and below the price pane.

use crate::chart::indicators::{
    SelectionDotConfig, hit_test_pane_indicator, render_indicator_selection_dots,
};
use crate::chart::selection::ChartElementId;
use crate::chart::series::calculate_dot_interval;
use crate::localization as l10n;
use crate::studies::{CustomIndicator, Indicator};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::widget::{
    Chart, IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneId, PaneInteraction,
};
use egui::{Pos2, Rect, Response, Sense, Ui};

/// Information about a rendered indicator pane, used for hit testing and coordinate mapping.
///
/// After calling [`Chart::show`], each visible separate-pane indicator
/// (RSI, MACD, etc.) produces a `RenderedIndicatorPane` entry stored in the chart.
/// Platform code can use these to implement click-on-indicator-line selection,
/// tooltip display, or other interactive features.
///
/// Retrieve with [`Chart::get_rendered_indicator_panes`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RenderedIndicatorPane {
    /// Position of the study in [`Chart::indicators`] when the pane was drawn. After a pane's
    /// close button removed its study, that pane stays in the list until the next frame, and it
    /// and the panes after it point one position too far.
    pub indicator_idx: usize,
    /// Full panel rect (including y-axis labels)
    pub panel_rect: Rect,
    /// Chart drawing area rect (excluding y-axis labels)
    pub chart_rect: Rect,
    /// Y-axis minimum value
    pub y_min: f64,
    /// Y-axis maximum value
    pub y_max: f64,
    /// Coordinate parameters for x-axis calculation
    pub coords: IndicatorCoordParams,
    /// Full mapping, including manual ranges, scale modes and inversion.
    pub mapping: crate::chart::coords::ChartMapping,
}

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
    pub(in crate::widget) fn show_with_panes(
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
            // Panes without a visible bar series (study panes) keep their own formats.
            let thousands_sep = self.config.thousands_sep.filter(|_| !series.is_empty());
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
            let map = pane_scale
                .map(transform, panel_rect.top(), panel_rect.bottom())
                .with_thousands_sep(thousands_sep);
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
                        crate::widget::series::SeriesLegend::new(
                            item,
                            close,
                            self.series_label_formatter(
                                item.id,
                                crate::scales::PriceScaleMode::Normal,
                            ),
                            map.price_precision(),
                            map.thousands_sep(),
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
                            let hit = crate::widget::primitives::primitive_at(
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
                    .filter(|_| !self.menu_open && !egui::Popup::is_any_open(ui.ctx()));
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
                            primitive.draw(&painter, &view, crate::widget::PrimitivePass::Series);
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
                        crate::widget::primitives::primitive_at(
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
                                    let hit = crate::widget::primitives::primitive_at(
                                        pane_primitives().filter(|(item, _)| {
                                            item.replaces && item.series == *id
                                        }),
                                        pos,
                                    )?;
                                    return match hit.target {
                                        crate::widget::PointerTarget::Element(id) => Some(id),
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
                            crate::widget::primitives::primitive_at(
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
                            .map(crate::widget::PointerTarget::Drawing)
                            .or(primitive_hit.map(|hit| hit.target))
                            .or_else(|| study_at(pos).map(crate::widget::PointerTarget::Element));
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
                        let label = map.label(
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
                    crate::widget::tool_interaction::render_pane_eraser_highlight(
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
                        crate::widget::RightClickTarget::Drawing {
                            drawing_id: id,
                            pos,
                        }
                    } else {
                        crate::widget::RightClickTarget::Background {
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
                    item.draw(&painter, &view, crate::widget::PrimitivePass::Above);
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
mod tests;
