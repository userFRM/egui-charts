//! Rendering with separate indicator panes below the main chart.

use super::{
    Chart, IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction,
    RenderedIndicatorPane,
};
use crate::chart::indicators::{
    SelectionDotConfig, hit_test_pane_indicator, render_indicator_selection_dots,
};
use crate::chart::selection::ChartElementId;
use crate::chart::series::calculate_dot_interval;
use crate::drawings::DrawingManager;
use crate::studies::IndicatorRegistry;
use crate::tokens::DESIGN_TOKENS;
use egui::{Response, Ui};

impl Chart {
    /// Renders the chart with indicators and drawing tools.
    ///
    /// This is the most feature-complete rendering method. Overlay indicators
    /// (moving averages, Bollinger Bands, etc.) are drawn on the main price chart.
    /// Separate-pane indicators (RSI, MACD, Stochastic) are rendered in dedicated
    /// panels below the main chart with aligned x-axes.
    ///
    /// After rendering, use [`Chart::get_rendered_indicator_panes`] to access
    /// indicator pane layout information for hit testing.
    ///
    /// # Arguments
    ///
    /// * `ui` -- The egui UI to render into
    /// * `drawing_manager` -- Optional drawing tool manager for trend lines, etc.
    /// * `indicators` -- Optional indicator registry containing computed indicators
    pub fn show_with_indicators(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        // Clear previous frame's indicator pane info
        self.last_rendered_indicator_panes.clear();

        // Panes are stacked edge to edge with explicit gaps; restore the host spacing after.
        let host_spacing = std::mem::take(&mut ui.spacing_mut().item_spacing);

        // Calculate total height needed for indicator panes FIRST
        // This allows us to reserve space before the main chart
        let indicator_pane_height = if let Some(indicators) = indicators {
            let mut total_height = 0.0f32;
            let mut pane_count = 0;

            for indicator in indicators.indicators() {
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
        } else {
            0.0
        };

        // Calculate available height and reserve space for indicators
        let available = ui.available_size();
        let main_chart_height = (available.y - indicator_pane_height).max(200.0);

        // Allocate fixed height for main chart (prevents it from taking all space)
        let response = ui
            .allocate_ui_with_layout(
                egui::vec2(available.x, main_chart_height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.show_internal(ui, drawing_manager, indicators),
            )
            .inner;

        // Render separate pane indicators below the main chart
        if let Some(indicators) = indicators {
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

            let mut has_pane_indicators = false;
            for indicator in indicators.indicators() {
                if indicator.is_overlay() || !indicator.is_visible() {
                    continue;
                }
                has_pane_indicators = true;
                break;
            }

            if has_pane_indicators {
                // A pane click is resolved after the loop so the immutable `bars`
                // borrow can be released before mutating selection state. `Some`
                // means a pane was clicked; the inner value is the element to
                // select, or `None` to clear (click on empty pane area).
                let mut pending_pane_selection: Option<Option<ChartElementId>> = None;
                // Registry index of a pane whose legend "x" was clicked this
                // frame, if any. Applied after the immutable `bars` borrow ends.
                let mut pending_indicator_remove: Option<usize> = None;
                let current_selection = self.selection.selected_id();

                for (idx, indicator) in indicators.indicators().iter().enumerate() {
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

                        // A click on the legend's close "x" requests removal of
                        // this pane indicator. Recorded by registry index and
                        // drained once by the host via `take_indicator_remove`.
                        let close_clicked = close_response.is_some_and(|r| r.clicked());
                        if close_clicked {
                            pending_indicator_remove = Some(idx);
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
                            pending_pane_selection =
                                Some(hit.map(|h| ChartElementId::PaneIndicator(h.indicator_idx)));
                        }

                        // Draw selection handles when this pane is selected.
                        if current_selection == Some(ChartElementId::PaneIndicator(idx)) {
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

                // Record a pane remove request for the host to drain. The chart
                // does not own the registry, so it cannot remove the indicator
                // itself; it surfaces the index via `take_indicator_remove`.
                if pending_indicator_remove.is_some() {
                    self.indicator_remove = pending_indicator_remove;
                }
            }
        }

        ui.spacing_mut().item_spacing = host_spacing;
        response
    }

    /// Renders the chart with indicators using simple per-bar x-positioning.
    ///
    /// Unlike [`Chart::show_with_indicators`], which uses aligned coordinate
    /// parameters from the main chart's time scale, this method creates
    /// indicator panes with basic visible-range positioning. It is simpler
    /// but may not perfectly align indicator data points with the main chart
    /// when the user scrolls or zooms. Prefer [`Chart::show_with_indicators`]
    /// for production use.
    pub fn show_with_indicators_plot(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        let response = self.show_with_drawings(ui, drawing_manager);

        if let Some(indicators) = indicators {
            ui.separator();

            let (start_idx, end_idx) = self.state.visible_range();
            let visible_range = start_idx..end_idx;
            let bars = &self.state.data().bars;

            for indicator in indicators.indicators() {
                if indicator.is_overlay() {
                    continue;
                }

                if !indicator.is_visible() {
                    continue;
                }

                let config = match indicator.name() {
                    "RSI" => IndicatorPaneConfig::rsi(),
                    "MACD" => IndicatorPaneConfig::macd(),
                    "Stochastic" => IndicatorPaneConfig::stochastic(),
                    _ => IndicatorPaneConfig::default(),
                };

                let mut panel = IndicatorPane::with_config(config);

                panel.show(ui, indicator.as_ref(), bars, visible_range.clone());
            }
        }

        response
    }
}
