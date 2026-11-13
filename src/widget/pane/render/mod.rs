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
    PaneScale,
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
    /// The initial pane retains its overlay mappings when the main series leaves it.
    pub(in crate::widget) fn pane_study_mapping(
        &self,
        pane: PaneId,
        study: &dyn Indicator,
        mapping: &crate::chart::rendering::ChartMapping,
    ) -> crate::chart::rendering::ChartMapping {
        if pane == PaneId::MAIN
            && study.is_volume()
            && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
        {
            let (start, end) = self.state.visible_range();
            super::volume::mapping(
                mapping,
                super::volume::overlay_rect(mapping.rect, &self.config),
                study,
                &(start..end),
                Some(&self.merge_groups),
            )
        } else if pane == PaneId::MAIN
            || matches!(study.scale(), crate::studies::StudyScale::Shared(_))
        {
            crate::chart::rendering::study_mapping(study, mapping)
        } else {
            mapping.clone()
        }
    }

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

    /// Renders the existing panes in order, sharing the height by stretch factor, with aligned
    /// horizontal mappings and one time axis below them all.
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
        let main_pane = self.main_series_pane();
        let main_present = self.panes().contains(&main_pane);
        // The drawn panes from top to bottom, the price pane included, and whether each is
        // collapsed.
        let mut panes: Vec<_> = self
            .pane_layout
            .panes
            .iter()
            .enumerate()
            .filter(|(_, pane)| {
                pane.id == main_pane
                    || (pane.id == PaneId::MAIN || self.pane_shown(pane))
                        && maximized.is_none_or(|id| id == pane.id)
            })
            .map(|(idx, pane)| (idx, pane.id, false))
            .collect();
        let pane_count = panes.len().saturating_sub(usize::from(main_present));
        let gap_count = panes.len().saturating_sub(1);
        let axis_height = self.time_axis_height().min(available.height());
        let gap = if maximized.is_some() {
            0.0
        } else {
            DESIGN_TOKENS
                .spacing
                .hairline
                .min((available.height() - axis_height).max(0.0) / gap_count.max(1) as f32)
        };
        let usable_height = (available.height() - axis_height - gap * gap_count as f32).max(0.0);
        let main_collapsed = maximized.is_none()
            && self
                .pane_layout
                .panes
                .iter()
                .any(|pane| pane.id == main_pane && pane.collapsed == Some(true));
        let main_expanded = main_present && !main_collapsed;
        let main_minimum = if main_expanded { 40.0 } else { 0.0 };
        let row_count = pane_count + usize::from(main_collapsed);
        let row_height = 24.0_f32.min(usable_height / row_count.max(1) as f32);
        // A collapsed price pane is one header row and takes no share of the rest.
        let auto_collapse = (usable_height - if main_collapsed { row_height } else { 0.0 })
            / (if main_expanded { 3.0 } else { 0.0 } + pane_count as f32).max(1.0)
            < 40.0;
        let mut expanded_budget = ((usable_height - row_height * row_count as f32 - main_minimum)
            / (40.0 - row_height))
            .max(0.0) as usize;
        for (idx, id, collapsed) in &mut panes {
            if *id == main_pane {
                *collapsed = main_collapsed;
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
            .filter(|idx| self.pane_layout.panes[**idx].id != main_pane)
            .map(stretch)
            .sum();
        let mut desired: Vec<_> = panes
            .iter()
            .map(|(idx, id, collapsed)| {
                if *id == main_pane || *collapsed {
                    0.0
                } else {
                    // Fractions first, so that large stretch factors cannot overflow.
                    let share = |total| remaining * (stretch(idx) / total);
                    share(total).max(40.0).min(share(pane_total))
                }
            })
            .collect();
        if maximized.is_some_and(|id| id != main_pane) {
            for (height, (_, id, _)) in desired.iter_mut().zip(&panes) {
                if *id != main_pane {
                    *height = usable_height;
                }
            }
        }
        let main_min = if !main_expanded || maximized.is_some_and(|id| id != main_pane) {
            0.0
        } else {
            40.0
        };
        let expanded_count = expanded.len().saturating_sub(usize::from(main_expanded));
        let pane_budget = (remaining - main_min).max(0.0);
        let minimum = (pane_budget / expanded_count.max(1) as f32).min(40.0);
        let excess: f32 = desired
            .iter()
            .map(|height| (*height - minimum).max(0.0))
            .sum();
        let factor =
            ((pane_budget - minimum * expanded_count as f32).max(0.0) / excess.max(1.0)).min(1.0);
        for (height, (_, id, collapsed)) in desired.iter_mut().zip(&panes) {
            if *id != main_pane && !collapsed {
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
            let height = if *collapsed {
                row_height
            } else if *id == main_pane {
                main_height
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
        let main_at = panes.iter().position(|(_, id, _)| *id == main_pane);
        // The retained horizontal scale and time axis still run through the price frame.
        let main_rect = main_at.map_or_else(
            || Rect::from_min_size(available.min, egui::vec2(available.width(), 0.0)),
            |at| rects[at],
        );
        let plot = self
            .pane_layout_rects(if main_present { main_rect } else { available }, false)
            .price_rect;
        let time_axis = Rect::from_min_max(
            Pos2::new(plot.left(), available.bottom() - axis_height),
            Pos2::new(plot.right(), available.bottom()),
        );
        // The price frame has no height for time navigation in these layouts.
        let shared_plot =
            (main_at.is_none() || main_collapsed || maximized.is_some_and(|id| id != main_pane))
                .then(|| {
                    ui.interact(
                        Rect::from_min_max(
                            Pos2::new(plot.left(), available.top()),
                            Pos2::new(plot.right(), time_axis.top()),
                        ),
                        ui.scope_id().with((maximized, "plot")),
                        Sense::click_and_drag(),
                    )
                });
        let price_frame = if main_collapsed {
            Rect::from_min_size(main_rect.min, egui::vec2(main_rect.width(), 0.0))
        } else {
            main_rect
        };
        let response = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(price_frame)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
                |ui| self.show_internal(ui, Some(time_axis), shared_plot.as_ref(), footprint),
            )
            .inner;
        let touch_readout = crate::widget::interaction::touch_crosshair(ui, response.id);

        // A key handled by the price frame can remove a pane and shift the ones after it.
        let recorded = panes.len();
        let mut kept = 0;
        for at in 0..recorded {
            let (_, id, collapsed) = panes[at];
            if let Some(idx) = self.pane_layout.panes.iter().position(|pane| pane.id == id) {
                panes[kept] = (idx, id, collapsed);
                rects[kept] = rects[at];
                kept += 1;
            }
        }
        panes.truncate(kept);
        rects.truncate(kept);
        let main_at = panes.iter().position(|(_, id, _)| *id == main_pane);

        let horizontal = self.get_chart_mapping().with_rect(plot);
        let displayed =
            horizontal.x_to_idx(plot.left())..horizontal.x_to_idx(plot.right()).saturating_add(1);
        for (idx, id, _) in &panes {
            if *id != main_pane {
                self.fit_pane_scales(*idx, ui.painter(), displayed.clone());
            }
        }
        self.update_shared_study_maps(plot);

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
        let mut coords = IndicatorCoordParams::new(
            time_scale.bar_spacing(),
            time_scale.right_offset_precise(),
            self.state.shown().len().saturating_sub(1),
            start_idx,
        );
        coords.numeric_positions = time_scale.numeric_positions().cloned();
        if main_pane != PaneId::MAIN {
            let mapping = self.get_chart_mapping();
            for (idx, id) in self.study_ids.iter().enumerate() {
                if self.series_pane(*id) == Some(main_pane) && self.is_series_visible(*id) {
                    let study_mapping = crate::chart::rendering::study_mapping(
                        studies.indicators()[idx].as_ref(),
                        &mapping,
                    );
                    self.last_rendered_indicator_panes
                        .push(RenderedIndicatorPane {
                            indicator_idx: idx,
                            panel_rect: main_rect,
                            chart_rect: mapping.rect,
                            y_min: study_mapping.price.range().min,
                            y_max: study_mapping.price.range().max,
                            coords: coords.clone(),
                            mapping: study_mapping,
                        });
                }
            }
        }

        // The plot column of every pane, where the pointer drives their shared crosshair.
        let hover_rect = Rect::from_min_max(
            Pos2::new(
                plot.left(),
                if main_at == Some(0) {
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
            if pane_id == main_pane {
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
            let members = self.pane_series(pane_id);
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
            let scale = self.pane_price_scale(pane_id, crate::model::SeriesScale::Right);
            let map = scale
                .map(scale.transform(), panel_rect.top(), panel_rect.bottom())
                .with_thousands_sep(thousands_sep);
            let drawing_map = members
                .first()
                .and_then(|id| self.series_price_map(*id))
                .unwrap_or(map);
            let axes: Vec<_> = self.pane_scale_axes(pane_id, panel_rect, plot).collect();
            let mut drawing_response = None;
            let mut scale_tags = std::collections::HashMap::<PaneScale, Vec<Option<Rect>>>::new();
            let mut fixed_axis_tags =
                std::collections::HashMap::<PaneScale, Vec<Option<Rect>>>::new();
            let mut axis_foregrounds = Vec::new();
            let mut drawn_axes = Vec::new();
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
                            self.series_map(item, panel_rect).price_precision(),
                            self.series_map(item, panel_rect).thousands_sep(),
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
                            self.hit_price_series(
                                item,
                                pos,
                                pane_plot,
                                self.series_map(item, pane_plot),
                            )
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
                                            self.pane_study_mapping(pane_id, study, &pane_mapping),
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
                                    &self.pane_study_mapping(pane_id, study, &pane_mapping),
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
            // The crosshair lines are drawn above the series pass: the host series' bars and
            // the primitives drawn with them.
            let mut series_crosshair = None;
            let first = entries
                .iter()
                .min_by_key(|entry| entry.is_some_and(|(id, _)| Some(id) == on_top))
                .copied();
            let mut panels = Vec::with_capacity(entries.len());
            let mut legend_offset = 0.0;
            for entry in entries {
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
                config.show_grid &= Some(entry) == first;
                let mut panel = IndicatorPane::with_config(config);
                panel.legend_backdrop = self.config.indicator_backdrop_opacity;
                panel.axis_options = crate::config::PriceAxisOptions {
                    show_study_values: self.config.price_axis.show_study_values,
                    ..self.config.price_axis
                };
                let assignment = id
                    .and_then(|id| self.source_axis_key(pane_id, id))
                    .unwrap_or(PaneScale::Named(crate::model::SeriesScale::Right));
                let member_map = if let Some(item) =
                    id.and_then(|id| self.price_series.iter().find(|item| item.id == id))
                {
                    self.series_map(item, panel_rect)
                } else {
                    self.pane_study_mapping(pane_id, indicator, &pane_mapping)
                        .price
                };
                panel.price_map = Some(member_map);
                if study.is_none() && id.is_some() {
                    panel.price_formatter = id.and_then(|id| {
                        self.series_label_formatter(id, member_map.transform().mode)
                    });
                    panel.axis_options.show_study_values = false;
                } else if matches!(
                    member_map.transform().mode,
                    crate::scales::PriceScaleMode::Percentage
                        | crate::scales::PriceScaleMode::IndexedTo100
                ) {
                    panel.price_formatter = self.percentage_formatter.clone();
                }
                panel.axis_rect = axes
                    .iter()
                    .find(|(key, _)| *key == assignment)
                    .map(|(_, rect)| *rect);
                panel.legend_offset = legend_offset;
                panel.pane_controls = true;
                // A series legend reports its value at any bar, the last close without a crosshair.
                panel.bar_index = hovered.or(id.filter(|_| study.is_none()).map(|_| 0));
                let legend_row_height = panel
                    .legend_galley(
                        ui.painter(),
                        pane_plot,
                        indicator,
                        ui.active_theme().ui.text,
                    )
                    .size()
                    .y
                    + ui.active_theme().spacing.md;
                legend_offset += legend_row_height;
                panels.push((entry, indicator, panel, legend_row_height, assignment));
            }
            panels.sort_by_key(|(entry, ..)| entry.is_some_and(|(id, _)| Some(id) == on_top));
            // The first member reserves each scale's primitive tags against its tick labels.
            // Fixed tags and crosshair labels paint after the pane's value and drawing tags.
            let mut primitive_axis_labels = std::collections::HashMap::<PaneScale, Vec<_>>::new();
            if !collapsed {
                for (item, view) in self.pane_primitives(pane_id, &members, &pane_mapping) {
                    let Some((_, _, panel, _, assignment)) =
                        panels.iter().find(|(entry, indicator, panel, ..)| {
                            item.pane.is_none()
                                && entry.is_some_and(|(id, _)| id == item.series)
                                && indicator.scale() != crate::studies::StudyScale::None
                                && panel.axis_rect.is_some()
                        })
                    else {
                        continue;
                    };
                    let map = panel.price_map.expect("member mapping");
                    primitive_axis_labels
                        .entry(*assignment)
                        .or_default()
                        .extend(
                            item.get()
                                .price_axis_labels(&view)
                                .into_iter()
                                .map(|label| (label, map)),
                        );
                }
            }
            let mut below_grouped = false;
            let mut series_backgrounds = Vec::new();
            for (paint_row, (entry, indicator, mut panel, legend_row_height, assignment)) in
                panels.into_iter().enumerate()
            {
                let id = entry.map(|(id, _)| id);
                let study = entry.and_then(|(_, idx)| idx);
                let key = (pane_id, id);
                panel.overlay = paint_row > 0;
                panel.axis_tags = scale_tags.get(&assignment).cloned().unwrap_or_default();
                panel.fixed_axis_tags = fixed_axis_tags
                    .get(&assignment)
                    .cloned()
                    .unwrap_or_default();
                if indicator.scale() != crate::studies::StudyScale::None {
                    panel.primitive_axis_labels = primitive_axis_labels
                        .remove(&assignment)
                        .unwrap_or_default();
                }
                let axis_owner = series
                    .iter()
                    .map(|at| &self.price_series[*at])
                    .find(|item| PaneScale::Named(item.options.scale) == assignment)
                    .map(|item| item.id);
                panel.draw_axis = !drawn_axes.contains(&assignment)
                    && axis_owner.is_none_or(|owner| Some(owner) == id);
                if panel.draw_axis {
                    drawn_axes.push(assignment);
                }
                let member_map = panel.price_map.expect("member mapping");
                let pointer = if touch_readout.touch_input
                    && self.chart_options.tooltip.enabled
                    && self.chart_options.tooltip.on_long_press
                {
                    touch_readout.position
                } else {
                    self.axis_hover.map(|(pos, _)| pos).or_else(|| {
                        ui.input(|input| input.pointer.hover_pos()).filter(|pos| {
                            hover_rect.contains(*pos) && ui.rect_contains_pointer(hover_rect)
                        })
                    })
                };
                let host_position = pointer.is_none() && self.synced_crosshair_price.is_some();
                let target_pane = self.synced_crosshair_series.is_some_and(|target| {
                    members.contains(&target) && self.is_series_visible(target)
                });
                let target_panel = target_pane && id == self.synced_crosshair_series;
                let mut hover = pointer
                    .or_else(|| {
                        self.synced_crosshair_bar_idx
                            .filter(|index| index.is_finite() && *index >= 0.0)
                            .map(|index| {
                                Pos2::new(
                                    pane_mapping.index_to_x(index),
                                    if target_panel {
                                        member_map
                                            .price_to_y(self.synced_crosshair_price.unwrap_or(0.0))
                                    } else {
                                        plot.center().y
                                    },
                                )
                            })
                            .filter(|pos| plot.x_range().contains(pos.x))
                    })
                    .filter(|_| !self.menu_open && !egui::Popup::is_any_open(ui.ctx()))
                    .and_then(|pos| self.crosshair_position(pos, &pane_mapping));
                if let Some(pos) = hover {
                    panel.bar_index = panel.bar_index.filter(|index| {
                        self.pointer_time(pos).and_then(|(time, _)| time)
                            == self.state.shown().get(*index).map(|bar| bar.time)
                    });
                }
                let mut crosshair_options = self.chart_options.crosshair.resolved(ui.ctx());
                if eraser_mode && crosshair_options.style != crate::config::CrosshairStyle::Hidden {
                    crosshair_options.style = crate::config::CrosshairStyle::Arrow;
                }

                if host_position {
                    crosshair_options.mode = crate::config::CrosshairMode::Normal;
                    if !target_panel {
                        crosshair_options.horz_line_visible = false;
                        crosshair_options.horz_label_visible = false;
                        if crosshair_options.style == crate::config::CrosshairStyle::Dot {
                            crosshair_options.style = crate::config::CrosshairStyle::Arrow;
                        }
                    }
                }
                if host_position && target_pane && !target_panel {
                    hover = None;
                } else if crosshair_options.mode.snaps() {
                    if let Some(pos) = &mut hover
                        && panel_rect.contains(*pos)
                        && let Some(bar) = panel.bar_index
                        && let Some(y) = members
                            .iter()
                            .flat_map(|id| {
                                if let Some(item) = self
                                    .price_series
                                    .iter()
                                    .find(|item| item.id == *id && item.visible)
                                {
                                    let map = self.series_map(item, pane_plot);
                                    return self
                                        .series_close_at(item, bar)
                                        .map(|value| map.price_to_y(value))
                                        .into_iter()
                                        .collect::<Vec<_>>();
                                }
                                let Some(idx) = self.study_ids.iter().position(|study| study == id)
                                else {
                                    return Vec::new();
                                };
                                let study = studies.indicators()[idx].as_ref();
                                if !study.is_visible() {
                                    return Vec::new();
                                }
                                let map = if matches!(
                                    study.scale(),
                                    crate::studies::StudyScale::Shared(_)
                                ) {
                                    crate::chart::rendering::study_mapping(study, &pane_mapping)
                                        .price
                                } else {
                                    map
                                };
                                (0..study.line_count())
                                    .filter_map(|line| {
                                        let plot = study.plot_style(line);
                                        (plot.visible
                                            && !matches!(
                                                plot.plot_type,
                                                crate::studies::PlotType::BarColor
                                                    | crate::studies::PlotType::BackgroundColor
                                            ))
                                        .then(|| study.plot_value(line, bar))
                                        .flatten()
                                        .map(|value| map.price_to_y(value))
                                    })
                                    .collect()
                            })
                            .filter(|y| y.is_finite())
                            .min_by(|a, b| (a - pos.y).abs().total_cmp(&(b - pos.y).abs()))
                    {
                        pos.y = y;
                    }
                    crosshair_options.mode = crate::config::CrosshairMode::Normal;
                }
                if (!series.is_empty() || !panel.overlay)
                    && !collapsed
                    && let Some(pos) = hover
                {
                    // The panel draws the price tag; the pane draws the lines after the series
                    // pass, from the first row in a pane without host series.
                    series_crosshair = Some((pos, crosshair_options));
                    crosshair_options.vert_line_color = egui::Color32::TRANSPARENT;
                    crosshair_options.horz_line_color = egui::Color32::TRANSPARENT;
                }

                if panel.overlay {
                    crosshair_options.vert_line_visible = false;
                    crosshair_options.horz_line_visible = false;
                }
                crosshair_options.horz_label_visible &=
                    panel.draw_axis || host_position && target_panel;

                // Use show_aligned_interactive to get pane info for hit testing
                let mut child =
                    ui.new_child(egui::UiBuilder::new().id_salt(key).max_rect(panel_rect));
                if let Some(interaction) = panel.show_aligned_interactive_in(
                    &mut child,
                    indicator,
                    self.state.shown(),
                    coords.clone(),
                    Some(plot.x_range()),
                    collapsed,
                    panel.axis_rect.map(|_| {
                        if self.pane_scale_side(pane_id, assignment)
                            == Some(crate::model::AxisSide::Right)
                        {
                            crate::scales::PriceScaleId::Right
                        } else {
                            crate::scales::PriceScaleId::Left
                        }
                    }),
                    hover.map(|pos| (pos, crosshair_options)),
                    &id.into_iter()
                        .flat_map(|id| self.primitives_of(id))
                        .collect::<Vec<_>>(),
                    study.is_none() && id.is_some(),
                    &mut axis_foregrounds,
                    |painter, pass| {
                        use crate::widget::{PrimitivePass, primitives};
                        if pass == PrimitivePass::Below && paint_row == 0 {
                            if let Some(watermark) = &self.pane_layout.panes[pane_idx].watermark {
                                crate::chart::rendering::render_watermark(
                                    ui, painter, pane_plot, watermark,
                                );
                            }
                            let backgrounds = primitives::backgrounds(
                                painter,
                                self.pane_primitives(pane_id, &members, &pane_mapping),
                                pass,
                                self.merge_groups.factor(),
                            );
                            below_grouped = !backgrounds.is_empty();
                            primitives::paint_backgrounds(painter, backgrounds);
                            if below_grouped {
                                for (item, view) in
                                    self.pane_primitives(pane_id, &members, &pane_mapping)
                                {
                                    item.draw(painter, &view, pass, self.merge_groups.factor());
                                }
                            }
                        }
                        if (pass == PrimitivePass::Below && !below_grouped)
                            || (pass == PrimitivePass::Series && series_backgrounds.is_empty())
                        {
                            for (item, view) in self
                                .pane_primitives(pane_id, &members, &pane_mapping)
                                .filter(|(item, _)| {
                                    if item.replaces {
                                        Some(item.series) == id
                                    } else {
                                        pass == PrimitivePass::Below && paint_row == 0
                                    }
                                })
                            {
                                item.draw(painter, &view, pass, self.merge_groups.factor());
                            }
                        }
                        if pass == PrimitivePass::Below && paint_row == 0 {
                            series_backgrounds = primitives::backgrounds(
                                painter,
                                self.pane_primitives(pane_id, &members, &pane_mapping),
                                PrimitivePass::Series,
                                self.merge_groups.factor(),
                            );
                        }
                    },
                ) {
                    if !collapsed
                        && let Some(item) =
                            id.and_then(|id| self.price_series.iter().find(|item| item.id == id))
                    {
                        let painter = ui
                            .painter()
                            .with_clip_rect(interaction.chart_rect.intersect(ui.clip_rect()));
                        self.paint_series(&painter, interaction.chart_rect, item, member_map);
                        if series_backgrounds.is_empty() {
                            let mut mapping = pane_mapping.clone();
                            mapping.price = member_map;
                            let view = self.primitive_view(item.id, mapping);
                            for primitive in
                                self.primitives_of(item.id).filter(|item| item.replaces)
                            {
                                primitive.draw(
                                    &painter,
                                    &view,
                                    crate::widget::PrimitivePass::Series,
                                    self.merge_groups.factor(),
                                );
                            }
                        }
                    }
                    self.measured_axis_width =
                        self.measured_axis_width.max(panel.measured_axis_width);
                    scale_tags.insert(assignment, panel.axis_tags.clone());
                    fixed_axis_tags.insert(assignment, panel.fixed_axis_tags.clone());
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

                    let mut pane_response = match shared_plot.as_ref().filter(|_| {
                        maximized.is_some()
                            || ui
                                .input(|input| input.pointer.interact_pos())
                                .is_some_and(|pos| chart_rect.contains(pos))
                    }) {
                        Some(plot) => pane_response.union(plot.clone()),
                        None => pane_response.interact(Sense::click_and_drag()),
                    };
                    if self.chart_options.tooltip.enabled
                        && self.chart_options.tooltip.on_long_press
                        && touch_readout.touch_input
                        && !drawing_tool_active
                        && !eraser_mode
                    {
                        pane_response
                            .flags
                            .remove(egui::response::Flags::LONG_TOUCHED);
                        if touch_readout.owns_gesture {
                            pane_response.flags.remove(
                                egui::response::Flags::CLICKED
                                    | egui::response::Flags::DRAGGED
                                    | egui::response::Flags::DRAG_STARTED,
                            );
                        }
                    }

                    // A click on the legend's close "x" removes this study or series.
                    let close_clicked = close_response.as_ref().is_some_and(|r| r.clicked());
                    let visibility_clicked =
                        visibility_response.as_ref().is_some_and(|r| r.clicked());
                    let settings_clicked = settings_response.as_ref().is_some_and(|r| r.clicked());
                    let control_clicked =
                        close_clicked || visibility_clicked || settings_clicked || menu.is_some();
                    use crate::chart::rendering::StudyAction;
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
                    if toggle.clicked() && !control_clicked && !ui.input(|i| i.modifiers.command) {
                        pending_pane_collapse = Some((pane_id, !collapsed));
                        ui.ctx().request_repaint();
                    }
                    if self.pane_collapse_click(ui.ctx()) {
                        pane_response.flags.remove(egui::response::Flags::CLICKED);
                    }
                    let mut pane_coords = coords.to_mapping(chart_rect, y_min, y_max);
                    pane_coords.price = member_map;
                    // Primitives are hit tested at the pointer, before the chart state is
                    // borrowed mutably.
                    let pointer = ui
                        .input(|input| input.pointer.interact_pos())
                        .filter(|pos| chart_rect.contains(*pos));
                    let pane_primitives = || self.pane_primitives(pane_id, &members, &pane_mapping);
                    let primitive_hit = pointer.and_then(|pos| {
                        crate::widget::primitives::primitive_at(
                            pane_primitives().filter(|(item, _)| {
                                !series_backgrounds.is_empty() || !item.replaces
                            }),
                            pos,
                        )
                    });
                    let builtin_at = |pos| {
                        members
                            .iter()
                            .rev()
                            .filter(|id| !self.has_renderer(**id))
                            .find_map(|id| {
                                if let Some(item) = self
                                    .price_series
                                    .iter()
                                    .find(|item| item.id == *id && item.visible)
                                {
                                    return self
                                        .hit_price_series(
                                            item,
                                            pos,
                                            chart_rect,
                                            self.series_map(item, chart_rect),
                                        )
                                        .map(|_| ChartElementId::Series(*id));
                                }
                                let idx = self.study_ids.iter().position(|study| study == id)?;
                                hit_test_pane_indicator(
                                    pos,
                                    studies.indicators()[idx].as_ref(),
                                    idx,
                                    visible_range.clone(),
                                    chart_rect,
                                    &self.pane_study_mapping(
                                        pane_id,
                                        studies.indicators()[idx].as_ref(),
                                        &pane_coords,
                                    ),
                                )
                                .map(|hit| {
                                    ChartElementId::Indicator(self.study_ids[hit.indicator_idx])
                                })
                            })
                    };
                    let renderer_hit = pointer.and_then(|pos| {
                        if !series_backgrounds.is_empty() {
                            return crate::widget::primitives::primitive_at(
                                pane_primitives().filter(|(item, _)| item.replaces),
                                pos,
                            );
                        }
                        let builtin = builtin_at(pos).and_then(|element| {
                            let (ChartElementId::Indicator(id) | ChartElementId::Series(id)) =
                                element;
                            members.iter().position(|member| *member == id)
                        });
                        members
                            .iter()
                            .enumerate()
                            .rev()
                            .filter(|(at, _)| builtin.is_none_or(|builtin| *at > builtin))
                            .fold(None, |best, (_, id)| {
                                crate::widget::primitives::best_hit(
                                    best,
                                    crate::widget::primitives::primitive_at(
                                        pane_primitives().filter(|(item, _)| {
                                            item.replaces && item.series == *id
                                        }),
                                        pos,
                                    ),
                                )
                            })
                    });

                    let study_at = |pos| {
                        renderer_hit
                            .as_ref()
                            .filter(|_| Some(pos) == pointer)
                            .and_then(|hit| match hit.target {
                                crate::widget::PointerTarget::Element(id) => Some(id),
                                _ => None,
                            })
                            .or_else(|| builtin_at(pos))
                    };
                    if !collapsed
                        && !toggle.clicked()
                        && !control_clicked
                        && pane_response.clicked()
                        && !drawing_tool_active
                        && !eraser_mode
                        && let Some(click_pos) = pane_response.interact_pointer_pos()
                    {
                        let hit = study_at(click_pos);
                        if hit.is_some() || pending_pane_selection.is_none() {
                            pending_pane_selection = Some(hit);
                        }
                    }
                    if !collapsed
                        && self.host_events.annotation_target.borrow().is_some()
                        && id.is_some_and(|id| self.study_ids.contains(&id))
                        && pointer.and_then(study_at) == id.map(ChartElementId::Indicator)
                    {
                        self.cover_annotation_hit();
                    }
                    // What the pointer is over, for host pointer events: drawings sit on top.
                    if !collapsed && let Some(pos) = pointer {
                        #[cfg(feature = "drawings")]
                        let drawing = self.drawings.hit_test(pos);
                        #[cfg(not(feature = "drawings"))]
                        let drawing: Option<usize> = None;
                        let renderer = renderer_hit
                            .as_ref()
                            .filter(|hit| {
                                study_at(pos).map(crate::widget::PointerTarget::Element)
                                    == Some(hit.target.clone())
                            })
                            .cloned();
                        let hit = crate::widget::primitives::best_hit(primitive_hit, renderer);
                        if let Some(hit) = hit.as_ref().filter(|_| {
                            drawing.is_none()
                                && pane_response.hovered()
                                && !pane_response.dragged()
                                && !eraser_mode
                                && !drawing_tool_active
                        }) {
                            ui.ctx().set_cursor_icon(hit.result.cursor);
                        }
                        self.host_events.target = drawing
                            .map(crate::widget::PointerTarget::Drawing)
                            .or(hit.as_ref().map(|hit| hit.target.clone()))
                            .or_else(|| study_at(pos).map(crate::widget::PointerTarget::Element));
                        if series_backgrounds.is_empty()
                            && hit.as_ref().is_some_and(|hit| {
                                hit.result.pass == crate::widget::PrimitivePass::Series
                                    && self.primitives.iter().any(|item| {
                                        Some(item.series) == id
                                            && hit.target
                                                == if item.replaces {
                                                    crate::widget::PointerTarget::Element(
                                                        ChartElementId::Indicator(item.series),
                                                    )
                                                } else {
                                                    crate::widget::PointerTarget::Primitive(item.id)
                                                }
                                    })
                            })
                        {
                            self.cover_annotation_hit();
                        }
                        self.host_events.primitive_hit =
                            hit.filter(|_| drawing.is_none()).map(|hit| hit.result);
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

                    drawing_response = Some((pane_response, chart_rect, pane_coords.clone()));
                    // Store the pane info for hit testing by platform
                    if let Some(idx) = study {
                        self.last_rendered_indicator_panes
                            .push(RenderedIndicatorPane {
                                indicator_idx: idx,
                                panel_rect,
                                chart_rect,
                                y_min,
                                y_max,
                                coords: coords.clone(),
                                mapping: self.pane_study_mapping(pane_id, indicator, &pane_coords),
                            });
                    }
                }
            }
            if let Some((_, chart_rect, _)) = &drawing_response {
                let chart_rect = *chart_rect;
                if !collapsed {
                    let grouped = !series_backgrounds.is_empty();
                    let painter = ui
                        .painter()
                        .with_clip_rect(chart_rect.intersect(ui.clip_rect()));
                    crate::widget::primitives::paint_backgrounds(&painter, series_backgrounds);
                    let mut hit_drawn = grouped;
                    for (item, view) in self
                        .pane_primitives(pane_id, &members, &pane_mapping)
                        .filter(|(item, _)| grouped || !item.replaces)
                    {
                        item.draw(
                            &painter,
                            &view,
                            crate::widget::PrimitivePass::Series,
                            self.merge_groups.factor(),
                        );
                        hit_drawn |= self.host_events.target
                            == Some(crate::widget::PointerTarget::Primitive(item.id));
                    }
                    if hit_drawn
                        && ui
                            .input(|input| input.pointer.interact_pos())
                            .is_some_and(|pos| chart_rect.contains(pos))
                        && self
                            .host_events
                            .primitive_hit
                            .as_ref()
                            .is_some_and(|hit| hit.pass == crate::widget::PrimitivePass::Series)
                    {
                        self.cover_annotation_hit();
                    }
                }
                if !collapsed {
                    self.paint_pane_marks(
                        ui.painter(),
                        chart_rect,
                        &members,
                        crate::model::MarkerZOrder::AboveSeries,
                    );
                }
                let painter = ui
                    .painter()
                    .with_clip_rect(panel_rect.intersect(ui.clip_rect()));
                if !collapsed && let Some((pos, options)) = series_crosshair {
                    crate::chart::rendering::render_crosshair_lines(
                        &crate::chart::rendering::RenderContext::new(&painter, chart_rect),
                        pos,
                        &options,
                        chart_rect.bottom(),
                    );
                    if let Some(index) = hovered
                        && options.style != crate::config::CrosshairStyle::Hidden
                    {
                        for at in &series {
                            let item = &self.price_series[*at];
                            self.paint_series_crosshair_marker(
                                &painter,
                                chart_rect,
                                item,
                                self.series_map(item, chart_rect),
                                index,
                                |_| ui.active_theme().chart.bg,
                            );
                        }
                    }
                }
                #[cfg(feature = "drawings")]
                let drawing_key = members
                    .first()
                    .and_then(|id| {
                        self.series_scale(*id).or_else(|| {
                            let study = self.indicators().find(|(study, _)| study == id)?.1;
                            match study.scale() {
                                crate::studies::StudyScale::Shared(key) => Some(key),
                                _ => None,
                            }
                        })
                    })
                    .map(PaneScale::Named)
                    .unwrap_or(PaneScale::Named(crate::model::SeriesScale::Right));
                #[cfg(feature = "drawings")]
                let drawing_axis = axes
                    .iter()
                    .find(|(key, _)| *key == drawing_key)
                    .map(|(_, axis)| *axis);
                #[cfg(feature = "drawings")]
                if !collapsed && let Some(axis) = drawing_axis {
                    let axis_edge = axis.right();
                    let label_rect = super::super::series::axis_label_rect(chart_rect, axis);
                    let map = drawing_map;
                    let axis_tags = scale_tags.entry(drawing_key).or_default();
                    let formatter = members.first().map_or_else(
                        || self.price_formatter.clone(),
                        |id| {
                            self.series_label_formatter(
                                self.series_axis_owner(*id),
                                map.transform().mode,
                            )
                        },
                    );
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
                            formatter.as_deref(),
                        );
                        axis_tags.push(crate::chart::rendering::axes::render_axis_value(
                            &painter,
                            label_rect,
                            axis_edge,
                            map.price_to_y(value),
                            label,
                            (crate::chart::rendering::axes::tag_text_color(fill), fill),
                            axis_tags,
                            self.config.price_axis.allow_label_overlap,
                        ));
                    }
                }
            }
            if !collapsed {
                let painter = ui
                    .painter()
                    .with_clip_rect(panel_rect.intersect(ui.clip_rect()));
                for at in &series {
                    let item = &self.price_series[*at];
                    let Some((_, axis)) = axes
                        .iter()
                        .find(|(key, _)| *key == PaneScale::Named(item.options.scale))
                    else {
                        continue;
                    };
                    let map = self.series_map(item, panel_rect);
                    let tags = scale_tags
                        .entry(PaneScale::Named(item.options.scale))
                        .or_default();
                    for (value, colors, last) in
                        self.series_axis_values(item).filter(|(value, ..)| {
                            map.range().contains(map.transform().to_scale(*value))
                        })
                    {
                        let tag = crate::chart::rendering::axes::render_axis_value(
                            &painter,
                            super::super::series::axis_label_rect(pane_plot, *axis),
                            axis.right(),
                            map.price_to_y(value),
                            self.series_tag_label(item, &map, value, last),
                            colors,
                            tags,
                            self.config.price_axis.allow_label_overlap,
                        );
                        tags.push(tag);
                    }
                }
            }
            crate::widget::primitives::paint_backgrounds(ui.painter(), axis_foregrounds);
            if !collapsed {
                for tag in axes
                    .iter()
                    .filter_map(|(key, _)| scale_tags.get(key))
                    .flatten()
                    .flatten()
                {
                    self.measured_axis_width = self.measured_axis_width.max(tag.width() + 11.0);
                }
            }
            if !collapsed && let Some((pane_response, chart_rect, _)) = drawing_response {
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
                    // Sampled once: inferred spacing sorts the gaps of the bars in view.
                    let bar_seconds = self.time_label_seconds();
                    let time_formatter =
                        self.time_formatter_in(self.label_timezone(|| bar_seconds));
                    self.handle_pane_drawings(
                        ui,
                        &mut drawings,
                        &mut cursors,
                        &pane_response,
                        chart_rect,
                        &drawing_map,
                        &painter,
                        &timescale,
                        (time_formatter.as_ref(), bar_seconds),
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
                #[cfg(feature = "drawings")]
                if let Some(id) = ui
                    .input(|input| input.pointer.interact_pos())
                    .filter(|pos| chart_rect.contains(*pos))
                    .and_then(|pos| self.drawings.hit_test(pos))
                {
                    self.cover_annotation_hit();
                    self.host_events.target = Some(crate::widget::PointerTarget::Drawing(id));
                    self.host_events.primitive_hit = None;
                }
                self.paint_pane_marks(
                    ui.painter(),
                    chart_rect,
                    &members,
                    crate::model::MarkerZOrder::Top,
                );
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
                            price: drawing_map.y_to_price(pos.y),
                        }
                    });
                }
                // The last pass of the pane's primitives, above its drawings.
                if !self.hide_foreground {
                    let painter = ui
                        .painter()
                        .with_clip_rect(chart_rect.intersect(ui.clip_rect()));
                    crate::widget::primitives::paint_pass(
                        &painter,
                        self.pane_primitives(pane_id, &members, &pane_mapping),
                        crate::widget::PrimitivePass::Above,
                        self.merge_groups.factor(),
                    );
                    if self.host_events.annotation_target.borrow().is_some()
                        && ui
                            .input(|input| input.pointer.interact_pos())
                            .filter(|pos| chart_rect.contains(*pos))
                            .and_then(|pos| {
                                crate::widget::primitives::primitive_at(
                                    self.pane_primitives(pane_id, &members, &pane_mapping),
                                    pos,
                                )
                            })
                            .is_some_and(|hit| {
                                hit.result.pass == crate::widget::PrimitivePass::Above
                            })
                    {
                        self.cover_annotation_hit();
                    }
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
        let drawn: Vec<_> = panes
            .iter()
            .zip(&rects)
            .map(|((idx, _, collapsed), rect)| (*idx, *rect, *collapsed))
            .collect();
        // Until the panes reflow on the next frame, a separator would span a removed pane.
        if panes.len() == recorded {
            self.pane_separators(ui, &drawn);
        }
        for ((_, id, collapsed), rect) in panes.iter().copied().zip(rects) {
            if id == main_pane {
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

        for response in host_clicks {
            self.record_host_click(ui, &response);
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

        if main_collapsed {
            self.collapsed_price_header(ui, main_rect);
        }
        self.pane_collapse_shortcut(ui);

        ui.advance_cursor_after_rect(available);
        ui.spacing_mut().item_spacing = host_spacing;
        if main_present {
            response
        } else {
            Response {
                rect: available,
                ..response
            }
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests;
