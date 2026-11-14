//! Navigation overlay layout, painting and actions owned by the chart.

use super::Chart;
use crate::chart::rendering;
use crate::config::NavigationButtons;
use crate::localization as l10n;
use crate::scales::PriceScaleMode;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

const LABELS: [&str; 8] = [
    "Zoom out",
    "Zoom in",
    "Scroll left",
    "Scroll right",
    "Reset view",
    "Auto-scale",
    "Logarithmic",
    "Percent",
];

impl Chart {
    pub(super) fn show_realtime_button(&mut self, ui: &Ui, rect: Option<Rect>, chart_id: egui::Id) {
        if let Some(rect) = rect
            && rendering::realtime_button(ui, rect, chart_id, &self.config).clicked()
        {
            self.user_action(|chart| chart.scroll_to_real_time(true));
            ui.ctx().request_repaint();
        }
    }

    /// The row's rect at the bottom of the main pane, or `None` when it does not fit in the pane.
    pub(super) fn navigation_rect(&self, ui: &Ui, chart_rect: Rect) -> Option<Rect> {
        let theme = ui.active_theme();
        let count = if self.config.show_scale_mode_buttons {
            8.0
        } else {
            5.0
        };
        let size = Vec2::new(
            count * DESIGN_TOKENS.sizing.button_md + (count + 1.0) * theme.spacing.sm,
            DESIGN_TOKENS.sizing.button_md + 2.0 * theme.spacing.sm,
        );
        (size.x <= chart_rect.width() && size.y + theme.spacing.lg <= chart_rect.height()).then(
            || {
                Rect::from_min_size(
                    Pos2::new(
                        chart_rect.center().x - size.x / 2.0,
                        chart_rect.bottom() - theme.spacing.lg - size.y,
                    ),
                    size,
                )
            },
        )
    }

    pub(super) fn navigation_visible(&self, ui: &Ui, chart_rect: Rect) -> bool {
        !self.state.data().is_empty()
            && match self.config.navigation_buttons {
                NavigationButtons::Always => true,
                NavigationButtons::OnHover => ui.input(|i| {
                    i.pointer
                        .hover_pos()
                        .is_some_and(|p| chart_rect.contains(p))
                }),
                NavigationButtons::Never => false,
            }
    }

    pub(super) fn interact_navigation(
        &mut self,
        ui: &Ui,
        rect: Rect,
        chart_rect: Rect,
        chart_id: egui::Id,
    ) -> [Response; 8] {
        let theme = ui.active_theme();
        let button_size = DESIGN_TOKENS.sizing.button_md;
        std::array::from_fn(|index| {
            let button_rect = Rect::from_center_size(
                Pos2::new(
                    rect.left()
                        + theme.spacing.sm
                        + button_size / 2.0
                        + index as f32 * (button_size + theme.spacing.sm),
                    rect.center().y,
                ),
                Vec2::splat(button_size),
            );
            let button_rect = if index >= 5 && !self.config.show_scale_mode_buttons {
                Rect::from_min_size(rect.min, Vec2::ZERO)
            } else {
                button_rect
            };
            let response = ui.interact(
                button_rect,
                chart_id.with(("navigation", index)),
                if index >= 5 && !self.config.show_scale_mode_buttons {
                    Sense::hover()
                } else {
                    Sense::click_and_drag()
                },
            );
            if response.clicked() {
                self.begin_user_action();
                self.tracking_mode_active = false;
                self.kinetic_scroll = super::state::KineticScrollState::new();
                match index {
                    0 | 1 => {
                        let step = self.chart_options.keyboard.zoom_step;
                        // The time scale's last pixel is width - 1, which keeps the right edge fixed.
                        self.state.time_scale_mut().zoom(
                            if index == 0 { -step } else { step },
                            chart_rect.width() - 1.0,
                            chart_rect.left(),
                            chart_rect.width(),
                        );
                    }
                    2 | 3 => {
                        let page = self
                            .state
                            .time_scale()
                            .visible_logical_range()
                            .length()
                            .ceil();
                        self.state.time_scale_mut().scroll_bars(if index == 2 {
                            page
                        } else {
                            -page
                        });
                    }
                    4 => self.reset_chart_view(),
                    5 => {
                        let price = self.state.price_scale_mut();
                        if price.is_manual() {
                            price.reset();
                        } else {
                            let range = price.range();
                            price.set_range(range.min, range.max);
                        }
                    }
                    6 | 7 => {
                        let mode = if index == 6 {
                            PriceScaleMode::Logarithmic
                        } else {
                            PriceScaleMode::Percentage
                        };
                        let price = self.state.price_scale_mut();
                        price.options_mut().mode = if price.options().mode == mode {
                            PriceScaleMode::Normal
                        } else {
                            mode
                        };
                        price.reset();
                        self.state.clear_zoom_history();
                    }
                    _ => unreachable!(),
                }
                self.end_user_action();
                ui.ctx().request_repaint();
            }
            response
        })
    }

    pub(super) fn paint_navigation(&self, ui: &Ui, rect: Rect, buttons: [Response; 8]) {
        let theme = ui.active_theme();
        ui.painter().rect(
            rect,
            theme.radius.sm,
            ui.visuals().window_fill,
            ui.visuals().window_stroke,
            egui::StrokeKind::Inside,
        );
        let price = self.state.price_scale();
        let selected = [
            false,
            false,
            false,
            false,
            false,
            !price.is_manual(),
            price.options().mode == PriceScaleMode::Logarithmic,
            price.options().mode == PriceScaleMode::Percentage,
        ];
        // Fallback text without the icons feature, and for the toggle that has no glyph.
        let text = ["-", "+", "<", ">", "R", "A", "log", "%"];
        #[cfg(feature = "icons")]
        let icons = {
            use crate::icons::icons;
            [
                Some(icons::ZOOM_OUT),
                Some(icons::ZOOM_IN),
                Some(icons::CHEVRON_LEFT),
                Some(icons::CHEVRON_RIGHT),
                Some(icons::ROTATE_CCW),
                Some(icons::MOVE_VERTICAL),
                None,
                Some(icons::PERCENT),
            ]
        };
        for (index, response) in buttons.into_iter().enumerate() {
            if index >= 5 && !self.config.show_scale_mode_buttons {
                continue;
            }
            let active = selected[index];
            let visuals = ui.style().interact_selectable(&response, active);
            if active || response.hovered() || response.is_pointer_button_down_on() {
                ui.painter()
                    .rect_filled(response.rect, theme.radius.sm, visuals.bg_fill);
            }
            crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.sm);
            let paint_text = || {
                ui.painter().text(
                    response.rect.center(),
                    egui::Align2::CENTER_CENTER,
                    l10n::text(ui.ctx(), text[index]),
                    egui::FontId::proportional(ui.active_theme().text.small),
                    visuals.text_color(),
                );
            };
            #[cfg(feature = "icons")]
            if let Some(icon) = &icons[index] {
                let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_md);
                icon.as_image_tinted(size, visuals.text_color())
                    .paint_at(ui, Rect::from_center_size(response.rect.center(), size));
            } else {
                paint_text();
            }
            #[cfg(not(feature = "icons"))]
            paint_text();
            let translated_label = l10n::text(ui.ctx(), LABELS[index]);
            response.widget_info(|| {
                if index < 5 {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                } else {
                    egui::WidgetInfo::selected(egui::Role::Button, true, active, &translated_label)
                }
            });
            response.on_hover_text(l10n::text(ui.ctx(), LABELS[index]));
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Event, Pos2, Shape};

    fn navigation_pos(chart: &crate::widget::Chart, button: usize) -> Pos2 {
        let rect = chart
            .last_rendered_price_rect
            .union(chart.last_rendered_volume_rect);
        Pos2::new(
            rect.center().x - 112.0 + 32.0 * button as f32,
            rect.bottom() - 26.0,
        )
    }

    fn navigation_click(ctx: &egui::Context, chart: &mut crate::widget::Chart, button: usize) {
        let pos = navigation_pos(chart, button);
        for (i, events) in [
            vec![Event::PointerMoved(pos)],
            vec![navigation_press(pos, true)],
            vec![navigation_press(pos, false)],
            Vec::new(),
        ]
        .into_iter()
        .enumerate()
        {
            frame(ctx, chart, 1.0 + i as f64 * 0.02, events);
        }
    }

    pub(in crate::widget) fn navigation_press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// On the 714 px time scale, spacing 20 shows ceil(714/20 - 1) = 35 bars.
    /// A zoom step of 0.2 adds +/-0.2 * 20/10 to spacing and keeps the right edge
    /// (offset -20) fixed; reset restores spacing 6 and the configured 60 px / 6 = 10 bar offset.
    #[test]
    fn navigation_buttons_apply_each_action() {
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        // (button, initial manual/mode, expected spacing/offset/manual/mode)
        for (button, initial_manual, initial_mode, spacing, offset, manual, mode) in [
            (0, false, Normal, 19.6, -20.0, false, Normal),
            (1, false, Normal, 20.4, -20.0, false, Normal),
            (2, false, Normal, 20.0, -55.0, false, Normal),
            (3, false, Normal, 20.0, 15.0, false, Normal),
            (4, true, Normal, 6.0, 10.0, false, Normal),
            (5, false, Normal, 20.0, -20.0, true, Normal),
            (5, true, Normal, 20.0, -20.0, false, Normal),
            (6, false, Normal, 20.0, -20.0, false, Logarithmic),
            (6, true, Logarithmic, 20.0, -20.0, false, Normal),
            (7, false, Normal, 20.0, -20.0, false, Percentage),
            (7, true, Percentage, 20.0, -20.0, false, Normal),
        ] {
            let ctx = egui::Context::default();
            let mut chart =
                chart(&[100.0], &[200.0]).with_chart_options(crate::config::ChartOptions {
                    time_scale: crate::config::TimeScaleOptions {
                        right_offset_pixels: Some(60.0),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            assert_eq!(chart.state.time_scale().width(), 714.0);
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: initial_mode,
                ..Default::default()
            });
            if initial_manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            }
            frame(&ctx, &mut chart, 0.1, Vec::new());
            navigation_click(&ctx, &mut chart, button);
            let ts = chart.state.time_scale();
            assert!(
                (ts.bar_spacing() - spacing).abs() < 1e-4,
                "button {button}: spacing {}",
                ts.bar_spacing()
            );
            assert!(
                (f64::from(ts.right_offset()) - offset).abs() < 1e-4,
                "button {button}: offset {}",
                ts.right_offset()
            );
            assert_eq!(
                chart.state.price_scale().is_manual(),
                manual,
                "button {button}"
            );
            assert_eq!(
                chart.state.price_scale().options().mode,
                mode,
                "button {button}"
            );
            assert!(chart.undo(), "button {button}: navigation edit is undoable");
            assert_eq!(chart.get_time_scale_state(), (20.0, -20.0));
            assert_eq!(chart.state.price_scale().is_manual(), initial_manual);
            assert_eq!(chart.state.price_scale().options().mode, initial_mode);
            assert!(chart.redo());
            assert!((chart.get_time_scale_state().0 - spacing).abs() < 1e-4);
            assert!((f64::from(chart.get_time_scale_state().1) - offset).abs() < 1e-4);
            assert_eq!(chart.state.price_scale().is_manual(), manual);
            assert_eq!(chart.state.price_scale().options().mode, mode);
        }
        // View movements are one undo step that keeps redo, here Left arrow presses after the
        // grid was hidden: (presses, whether the grid edit is undone before them)
        for (presses, undone) in [(120, false), (1, true)] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.user_action(|chart| chart.config.show_grid = false);
            if undone {
                assert!(chart.undo());
            }
            for press in 0..presses {
                let key = Event::Key {
                    key: egui::Key::ArrowLeft,
                    physical_key: None,
                    pressed: true,
                    repeat: press > 0,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(&ctx, &mut chart, 1.0 + f64::from(press) * 0.02, vec![key]);
            }
            assert_ne!(chart.get_time_scale_state(), (20.0, -20.0));
            if undone {
                assert!(chart.redo(), "a view movement keeps redo");
                assert!(!chart.config.show_grid);
            } else {
                assert!(chart.undo());
                assert_eq!(chart.get_time_scale_state(), (20.0, -20.0));
                assert!(!chart.config.show_grid);
                assert!(chart.undo(), "{presses} presses are one step");
                assert!(chart.config.show_grid);
                assert!(!chart.can_undo());
            }
        }
    }

    /// The selected fill follows price-axis scaling/reset and series settings on the next frame,
    /// and the row shows while hovering the main pane (volume included) per the visibility setting.
    #[test]
    fn navigation_buttons_show_visibility_and_scale_state() {
        use crate::config::NavigationButtons::{Always, Never, OnHover};
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        assert_eq!(chart.config.navigation_buttons, Never);
        #[cfg(feature = "ui")]
        assert_eq!(
            crate::widget::settings::ButtonsSettings::default().navigation,
            crate::widget::settings::ButtonVisibility::Hidden,
        );
        frame(&ctx, &mut chart, 0.0, Vec::new());
        for (i, (manual, mode, visibility, hover, selected)) in [
            (false, Normal, OnHover, true, [true, false, false]),
            (true, Normal, OnHover, true, [false, false, false]),
            (false, Logarithmic, OnHover, true, [true, true, false]),
            (false, Percentage, OnHover, true, [true, false, true]),
            (false, Normal, OnHover, false, [false, false, false]),
            (false, Normal, Always, false, [true, false, false]),
            (false, Normal, Never, true, [false, false, false]),
        ]
        .into_iter()
        .enumerate()
        {
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: mode,
                ..Default::default()
            });
            if manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            } else {
                chart.state.price_scale_mut().reset();
            }
            chart.config.navigation_buttons = visibility;
            let pointer = if hover {
                chart.last_rendered_volume_rect.center()
            } else {
                chart.last_rendered_price_rect.right_top() + egui::vec2(20.0, 20.0)
            };
            let painted = shapes(
                &ctx,
                &mut chart,
                0.1 + i as f64,
                vec![Event::PointerMoved(pointer)],
            );
            for (toggle, expected) in selected.into_iter().enumerate() {
                let center = navigation_pos(&chart, toggle + 5);
                let active = painted.iter().any(|shape| matches!(shape, Shape::Rect(r)
                    if r.rect.center().distance(center) < 0.1 && r.fill == ctx.global_style().visuals.selection.bg_fill));
                assert_eq!(
                    active, expected,
                    "manual {manual}, mode {mode:?}, toggle {toggle}"
                );
            }
        }
        chart.config.show_scale_mode_buttons = false;
        chart.config.navigation_buttons = Always;
        let painted = shapes(&ctx, &mut chart, 20.0, Vec::new());
        assert!(!painted.iter().any(|shape| matches!(shape, Shape::Text(text) if ["A", "log", "%"].contains(&text.galley.text()))));
        ctx.run_ui(Default::default(), |ui| {
            let plot = chart.last_rendered_price_rect;
            let nav = chart.navigation_rect(ui, plot).unwrap();
            let buttons =
                chart.interact_navigation(ui, nav, plot, egui::Id::unique("hidden_navigation"));
            assert!(
                buttons[5..]
                    .iter()
                    .all(|button| !button.sense.is_focusable()),
                "hidden scale controls must not receive keyboard focus"
            );
        })
        .drop_without_applying_deltas();
    }

    /// A drag starting on a button belongs to that button even after leaving the row.
    /// Clicking it also preserves a series selection while a drawing tool is active.
    #[test]
    #[cfg(feature = "drawings")]
    fn navigation_buttons_do_not_start_chart_gestures() {
        use crate::drawings::DrawingToolType;
        use crate::widget::{ChartElementId, SeriesId};
        for (tool, box_zoom) in [
            (None, false),
            (Some(DrawingToolType::TrendLine), false),
            (Some(DrawingToolType::Rect), false),
            (None, true),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.drawings.active_tool = tool;
            chart.zoom_mode_active = box_zoom;
            chart
                .selection
                .select(ChartElementId::Series(SeriesId::MAIN), Some(50));
            let start = navigation_pos(&chart, 5);
            let end = start + egui::vec2(-100.0, -60.0);
            for (i, events) in [
                vec![Event::PointerMoved(start)],
                vec![navigation_press(start, true)],
                vec![Event::PointerMoved(end)],
                vec![navigation_press(end, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.1 + i as f64 * 0.02, events);
                assert_eq!(
                    chart.state.time_scale().right_offset(),
                    -20.0,
                    "{tool:?}, box zoom {box_zoom}"
                );
                assert!(chart.scroll_start_pos.is_none());
                assert!(!chart.box_zoom.active);
                assert!(chart.drawings.curr_drawing.is_none());
                assert!(chart.drawings.drawings.is_empty());
            }
            navigation_click(&ctx, &mut chart, 5);
            assert_eq!(chart.state.time_scale().right_offset(), -20.0);
            assert!(chart.scroll_start_pos.is_none());
            assert_eq!(
                chart.selected_element(),
                Some(ChartElementId::Series(SeriesId::MAIN))
            );
            assert!(chart.drawings.curr_drawing.is_none());
        }
    }
}
