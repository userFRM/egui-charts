//! Navigation overlay layout, painting and actions owned by the chart.

use super::Chart;
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
    /// The row's rect at the bottom of the main pane, or `None` when it does not fit in the pane.
    pub(super) fn navigation_rect(&self, ui: &Ui, chart_rect: Rect) -> Option<Rect> {
        let theme = ui.active_theme();
        let size = Vec2::new(
            8.0 * DESIGN_TOKENS.sizing.button_md + 9.0 * theme.spacing.sm,
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
            let response = ui.interact(
                button_rect,
                chart_id.with(("navigation", index)),
                Sense::click_and_drag(),
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
                    4 => {
                        self.state
                            .time_scale_mut()
                            .set_bar_spacing(self.chart_options.time_scale.bar_spacing);
                        self.state.time_scale_mut().scroll_to_realtime();
                        self.state.price_scale_mut().reset();
                    }
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
                    egui::FontId::proportional(crate::styles::typography::SM),
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
