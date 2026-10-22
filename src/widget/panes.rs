//! Rendering with separate indicator panes below the main chart.

use super::{
    Chart, IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction,
    RenderedIndicatorPane, SeriesId,
};
use crate::chart::indicators::{
    SelectionDotConfig, hit_test_pane_indicator, render_indicator_selection_dots,
};
use crate::chart::selection::ChartElementId;
use crate::chart::series::calculate_dot_interval;
use crate::tokens::DESIGN_TOKENS;
use egui::{Response, Ui};

impl Chart {
    /// Renders the main chart and, below it, a pane for each visible study that is not an
    /// overlay (RSI, MACD, Stochastic), with x-axes aligned to the main chart.
    pub(super) fn show_with_panes(&mut self, ui: &mut Ui) -> Response {
        // Panes are stacked edge to edge with explicit gaps; restore the host spacing after.
        let host_spacing = std::mem::take(&mut ui.spacing_mut().item_spacing);

        // Calculate total height needed for indicator panes FIRST
        // This allows us to reserve space before the main chart
        let indicator_pane_height = {
            let mut total_height = 0.0f32;
            let mut pane_count = 0;

            for indicator in self.studies.indicators() {
                if indicator.is_overlay() || !indicator.is_visible() {
                    continue;
                }
                pane_count += 1;
                let height = match indicator.name() {
                    "RSI" => IndicatorPaneConfig::rsi().height,
                    "MACD" => IndicatorPaneConfig::macd().height,
                    "Stochastic" => IndicatorPaneConfig::stochastic().height,
                    _ => IndicatorPaneConfig::default().height,
                };
                total_height += height;
            }

            if pane_count > 0 {
                // Add minimal gap between panes (seamless panes)
                total_height + 1.0 * pane_count as f32
            } else {
                0.0
            }
        };

        // Calculate available height and reserve space for indicators
        let available = ui.available_size();
        let main_chart_height = (available.y - indicator_pane_height).max(200.0);

        // Allocate fixed height for main chart (prevents it from taking all space)
        let response = ui
            .allocate_ui_with_layout(
                egui::vec2(available.x, main_chart_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.show_internal(ui),
            )
            .inner;

        // Render separate pane indicators below the main chart
        let (start_idx, end_idx) = self.state.visible_range();
        let visible_range = start_idx..end_idx;
        let bars = &self.state.data().bars;

        // Get coordinate parameters from time scale for x-axis alignment
        let time_scale = self.state.time_scale();
        let coords = IndicatorCoordParams::new(
            time_scale.bar_spacing(),
            time_scale.right_offset(),
            self.state.data().len().saturating_sub(1),
            start_idx,
        );

        // A pane click is resolved after the loop so the immutable `bars`
        // borrow can be released before mutating selection state. `Some`
        // means a pane was clicked; the inner value is the element to
        // select, or `None` to clear (click on empty pane area).
        let mut pending_pane_selection: Option<Option<ChartElementId>> = None;
        // Study whose pane legend "x" was clicked this frame, if any.
        // Removed after the immutable `bars` borrow ends.
        let mut pending_indicator_remove: Option<SeriesId> = None;
        let current_selection = self.selection.selected_id();

        for (idx, indicator) in self.studies.indicators().iter().enumerate() {
            let id = self.study_ids[idx];
            if indicator.is_overlay() || !indicator.is_visible() {
                continue;
            }

            // Minimal gap, no visible separator (seamless panes)
            ui.add_space(DESIGN_TOKENS.spacing.hairline);

            let config = match indicator.name() {
                "RSI" => IndicatorPaneConfig::rsi(),
                "MACD" => IndicatorPaneConfig::macd(),
                "Stochastic" => IndicatorPaneConfig::stochastic(),
                _ => IndicatorPaneConfig::default(),
            };

            let mut panel = IndicatorPane::with_config(config);

            // Use show_aligned_interactive to get pane info for hit testing
            if let Some(interaction) = panel.show_aligned_interactive(
                ui,
                indicator.as_ref(),
                bars,
                visible_range.clone(),
                coords,
            ) {
                let PaneInteraction {
                    panel_rect,
                    chart_rect,
                    y_min,
                    y_max,
                    response: pane_response,
                    close_response,
                } = interaction;

                // A click on the legend's close "x" removes this study.
                let close_clicked = close_response.is_some_and(|r| r.clicked());
                if close_clicked {
                    pending_indicator_remove = Some(id);
                }

                let pane_coords = coords.to_mapping(chart_rect, y_min, y_max);

                // Resolve a click on this pane: a hit on the line selects
                // it, an empty-pane click clears the whole selection. A
                // close-"x" click is handled above and must not also
                // change the selection.
                if !close_clicked
                    && pane_response.clicked()
                    && let Some(click_pos) = pane_response.interact_pointer_pos()
                {
                    let hit = hit_test_pane_indicator(
                        click_pos,
                        indicator.as_ref(),
                        idx,
                        visible_range.clone(),
                        chart_rect,
                        y_min,
                        y_max,
                        &pane_coords,
                    );
                    pending_pane_selection = Some(
                        hit.map(|h| ChartElementId::Indicator(self.study_ids[h.indicator_idx])),
                    );
                }

                // Draw selection handles when this pane is selected.
                if current_selection == Some(ChartElementId::Indicator(id)) {
                    let dot_config = SelectionDotConfig {
                        dot_interval: calculate_dot_interval(pane_coords.bar_spacing),
                        ..Default::default()
                    };
                    for line_idx in 0..indicator.line_cnt() {
                        render_indicator_selection_dots(
                            ui.painter(),
                            indicator.as_ref(),
                            line_idx,
                            visible_range.clone(),
                            &pane_coords,
                            |value| pane_coords.price_to_y(value),
                            &dot_config,
                        );
                    }
                }

                // Store the pane info for hit testing by platform
                self.last_rendered_indicator_panes
                    .push(RenderedIndicatorPane {
                        indicator_idx: idx,
                        panel_rect,
                        chart_rect,
                        y_min,
                        y_max,
                        coords,
                    });
            }
        }

        // Apply the deferred pane selection now that `bars` is no longer
        // borrowed.
        if let Some(decision) = pending_pane_selection {
            match decision {
                Some(id) => self.selection.select(id, None),
                None => self.selection.deselect(),
            }
        }

        // The panes reflow on the next frame.
        if let Some(id) = pending_indicator_remove {
            self.remove_series(id);
        }

        ui.spacing_mut().item_spacing = host_spacing;
        response
    }
}
