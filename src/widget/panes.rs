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
        let desired: Vec<_> = panes
            .iter()
            .map(|(idx, _, collapsed)| {
                if *collapsed {
                    0.0
                } else if self.studies.indicators()[*idx].name() == "RSI" {
                    IndicatorPaneConfig::rsi().height
                } else {
                    pane_height
                }
            })
            .collect();
        let desired_total: f32 = desired.iter().sum();
        let scale = ((remaining - 40.0).max(0.0) / desired_total.max(1.0)).min(1.0);
        let main_height = (remaining - desired_total * scale).max(0.0);
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
        let mut pending_indicator_action = None;
        let current_selection = self.selection.selected_id();

        for ((_, id, collapsed), height) in panes.into_iter().zip(desired) {
            let Some(idx) = self.study_ids.iter().position(|study| *study == id) else {
                continue;
            };
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
            if self.config.show_indicator_inputs
                && indicator.is_volume()
                && indicator.plot_style(2).visible
                && config.inputs.is_empty()
            {
                config.inputs = indicator.line_names().get(2).cloned().unwrap_or_default();
            }
            config.show_legend =
                config.show_name || config.show_values || !config.inputs.is_empty();
            config.height = if collapsed {
                row_height
            } else {
                height * scale
            };
            let mut panel = IndicatorPane::with_config(config);
            panel.bar_index = self.last_hover_bar_idx.and_then(|index| {
                let index = index.round() as usize;
                (index < self.state.shown().len()).then_some(index)
            });
            let hover_rect = Rect::from_min_max(plot.min, Pos2::new(plot.right(), time_axis.top()));
            let hover = self
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
            if self.drawings.eraser_mode {
                crosshair_options.style = crate::config::CrosshairStyle::Arrow;
            }

            // Use show_aligned_interactive to get pane info for hit testing
            if let Some(interaction) = panel.show_aligned_interactive_in(
                ui,
                indicator.as_ref(),
                visible_range.clone(),
                coords,
                Some(plot.x_range()),
                collapsed,
                if self.config.show_right_axis {
                    Some(crate::scales::PriceScaleId::Right)
                } else if self.config.show_left_axis {
                    Some(crate::scales::PriceScaleId::Left)
                } else {
                    None
                },
                hover.map(|pos| (pos, crosshair_options)),
            ) {
                self.measured_axis_width = self.measured_axis_width.max(panel.measured_axis_width);
                let PaneInteraction {
                    panel_rect,
                    chart_rect,
                    y_min,
                    y_max,
                    response: pane_response,
                    close_response,
                    visibility_response,
                    settings_response,
                } = interaction;

                // A click on the legend's close "x" removes this study.
                let close_clicked = close_response.as_ref().is_some_and(|r| r.clicked());
                if pane_response.double_clicked() {
                    self.indicator_settings_request = Some(id);
                }
                if close_clicked {
                    pending_indicator_action =
                        Some(crate::chart::renderers::StudyAction::Remove(idx));
                }

                let visibility_clicked = visibility_response.as_ref().is_some_and(|r| r.clicked());
                let settings_clicked = settings_response.as_ref().is_some_and(|r| r.clicked());
                if visibility_clicked {
                    pending_indicator_action =
                        Some(crate::chart::renderers::StudyAction::ToggleVisibility(idx));
                }
                if settings_clicked {
                    pending_indicator_action =
                        Some(crate::chart::renderers::StudyAction::Settings(idx));
                }
                let control_clicked = close_clicked || visibility_clicked || settings_clicked;
                let header = Rect::from_min_max(
                    chart_rect.min,
                    Pos2::new(
                        visibility_response
                            .as_ref()
                            .map_or(chart_rect.right(), |r| {
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
                if toggle.clicked() && !control_clicked {
                    ui.data_mut(|d| d.insert_temp(pane_state.with(id), !collapsed));
                }
                let pane_coords = coords.to_mapping(chart_rect, y_min, y_max);

                // Resolve a click on this pane: a hit on the line selects
                // it, an empty-pane click clears the whole selection. A
                // close-"x" click is handled above and must not also
                // change the selection.
                if !collapsed
                    && !toggle.clicked()
                    && !control_clicked
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
        if let Some(action) = pending_indicator_action {
            self.apply_study_action(action);
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
                    (bounds.bottom() - last.bottom() - 28.0).abs() < 1.0,
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
            chart.chart_options.crosshair.line_style = crate::config::CrosshairLineStyle::Solid;
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
                if stroke.color == egui::Color32::BLUE && (points[0].y - y).abs() < 0.01
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
            }
        }
    }
}
