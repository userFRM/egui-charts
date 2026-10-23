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
use egui::{Pos2, Rect, Response, Sense, Ui};

impl Chart {
    /// Renders the main chart and, below it, a pane for each visible study that is not an
    /// overlay (RSI, MACD, Stochastic), with x-axes aligned to the main chart.
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
        let pane_state = ui.next_auto_id().with("pane_collapsed");
        let pane_count = self
            .studies
            .indicators()
            .iter()
            .filter(|study| !study.is_overlay() && study.is_visible())
            .count();
        let axis_height = if self.config.show_time_labels {
            crate::styles::sizing::chart::BOTTOM_PADDING_WITH_TIME.min(available.height())
        } else {
            0.0
        };
        let gap = DESIGN_TOKENS
            .spacing
            .hairline
            .min((available.height() - axis_height).max(0.0) / pane_count as f32);
        let usable_height = (available.height() - axis_height - gap * pane_count as f32).max(0.0);
        let auto_collapse = usable_height / (3.0 + pane_count as f32) < 40.0;
        let row_height = 24.0_f32.min(usable_height / pane_count as f32);
        let mut expanded_budget = ((usable_height - row_height * pane_count as f32)
            / (40.0 - row_height))
            .max(0.0) as usize;
        let panes: Vec<_> = self
            .studies
            .indicators()
            .iter()
            .enumerate()
            .filter(|(_, study)| !study.is_overlay() && study.is_visible())
            .map(|(idx, _)| {
                let id = self.study_ids[idx];
                let collapsed = ui
                    .data(|d| d.get_temp::<bool>(pane_state.with(id)))
                    .unwrap_or(auto_collapse)
                    || expanded_budget == 0;
                if !collapsed {
                    expanded_budget -= 1;
                }
                (idx, id, collapsed)
            })
            .collect();
        let collapsed_count = panes.iter().filter(|(_, _, collapsed)| *collapsed).count();
        let expanded_count = pane_count - collapsed_count;
        let remaining = (usable_height - row_height * collapsed_count as f32).max(0.0);
        let pane_height = if expanded_count > 0 {
            (remaining / (3.0 + expanded_count as f32))
                .max(40.0)
                .min(remaining / expanded_count as f32)
        } else {
            0.0
        };
        let main_height = (remaining - pane_height * expanded_count as f32).max(0.0);
        let main_rect =
            Rect::from_min_size(available.min, egui::vec2(available.width(), main_height));
        let plot = self.pane_layout_rects(main_rect, false).price_rect;
        let time_axis = Rect::from_min_max(
            Pos2::new(plot.left(), available.bottom() - axis_height),
            Pos2::new(plot.right(), available.bottom()),
        );
        let response = ui
            .allocate_ui_with_layout(
                main_rect.size(),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| self.show_internal(ui, Some(time_axis), footprint),
            )
            .inner;

        // Render separate pane indicators below the main chart
        let (start_idx, end_idx) = self.state.visible_range();
        let visible_range = start_idx..end_idx;

        // Get coordinate parameters from time scale for x-axis alignment
        let time_scale = self.state.time_scale();
        let coords = IndicatorCoordParams::new(
            time_scale.bar_spacing(),
            time_scale.right_offset(),
            self.state.data().len().saturating_sub(1),
            start_idx,
        );

        // A pane click is resolved after the loop so the immutable study
        // borrow can be released before mutating selection state. `Some`
        // means a pane was clicked; the inner value is the element to
        // select, or `None` to clear (click on empty pane area).
        let mut pending_pane_selection: Option<Option<ChartElementId>> = None;
        // Study whose pane legend "x" was clicked this frame, if any.
        // Removed after the immutable study borrow ends.
        let mut pending_indicator_remove: Option<SeriesId> = None;
        let current_selection = self.selection.selected_id();

        for (idx, id, collapsed) in panes {
            let indicator = &self.studies.indicators()[idx];
            ui.add_space(gap);

            let mut config = match indicator.name() {
                "RSI" => IndicatorPaneConfig::rsi(),
                "MACD" => IndicatorPaneConfig::macd(),
                "Stochastic" => IndicatorPaneConfig::stochastic(),
                _ => IndicatorPaneConfig::default(),
            };

            config.show_name = self.config.show_indicator_labels;
            config.show_values = self.config.show_indicator_last_val;
            config.inputs = if self.config.show_indicator_inputs {
                self.study_inputs[idx].clone()
            } else {
                String::new()
            };
            config.show_legend =
                config.show_name || config.show_values || !config.inputs.is_empty();
            config.height = if collapsed { row_height } else { pane_height };
            let mut panel = IndicatorPane::with_config(config);

            // Use show_aligned_interactive to get pane info for hit testing
            if let Some(interaction) = panel.show_aligned_interactive_in(
                ui,
                indicator.as_ref(),
                visible_range.clone(),
                coords,
                Some(plot.x_range()),
                collapsed,
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
                let close_clicked = close_response.as_ref().is_some_and(|r| r.clicked());
                if close_clicked {
                    pending_indicator_remove = Some(id);
                }

                let header = Rect::from_min_max(
                    chart_rect.min,
                    Pos2::new(
                        close_response.as_ref().map_or(chart_rect.right(), |r| {
                            r.rect.left().min(chart_rect.right())
                        }),
                        (chart_rect.top() + 24.0).min(chart_rect.bottom()),
                    ),
                );
                let toggle = ui
                    .interact(header, pane_state.with((id, "toggle")), Sense::click())
                    .on_hover_text(if collapsed {
                        "Expand pane"
                    } else {
                        "Collapse pane"
                    });
                if toggle.clicked() && !close_clicked {
                    ui.data_mut(|d| d.insert_temp(pane_state.with(id), !collapsed));
                }
                let pane_coords = coords.to_mapping(chart_rect, y_min, y_max);

                // Resolve a click on this pane: a hit on the line selects
                // it, an empty-pane click clears the whole selection. A
                // close-"x" click is handled above and must not also
                // change the selection.
                if !collapsed
                    && !toggle.clicked()
                    && !close_clicked
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
                if !collapsed && current_selection == Some(ChartElementId::Indicator(id)) {
                    let dot_config = SelectionDotConfig {
                        dot_interval: calculate_dot_interval(pane_coords.bar_spacing),
                        ..Default::default()
                    };
                    for line_idx in 0..indicator.line_cnt() {
                        render_indicator_selection_dots(
                            &ui.painter()
                                .with_clip_rect(chart_rect.intersect(ui.clip_rect())),
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

        // Apply the deferred pane selection now that the studies are no longer
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

        ui.allocate_space(egui::vec2(available.width(), axis_height));
        ui.spacing_mut().item_spacing = host_spacing;
        response
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::{MACD, RSI};
    use crate::widget::test_frames::chart;
    use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

    #[test]
    fn panes_fit_and_share_the_lowest_time_axis() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for size in [
                (1440.0, 900.0),
                (1440.0, 868.0),
                (720.0, 450.0),
                (640.0, 450.0),
                (600.0, 400.0),
                (600.0, 180.0),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0, 105.0], &[110.0, 115.0]);
                let rsi = chart.add_indicator(Box::new(RSI::new(14)));
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
                let last = panes.last().unwrap().panel_rect;
                assert!(
                    (bounds.bottom() - last.bottom() - 28.0).abs() < 1.0,
                    "{size:?}: {last:?} in {bounds:?}"
                );
                if size.1 > 200.0 {
                    assert!(
                        (price.height() - 3.0 * panes[0].panel_rect.height()).abs() < 1.0,
                        "{size:?}"
                    );
                    assert!((panes[0].panel_rect.height() - last.height()).abs() < 1.0);
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
                                || text.galley.text().contains("Feb") =>
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
                            egui::vec2(size.0, 100.0),
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
}
