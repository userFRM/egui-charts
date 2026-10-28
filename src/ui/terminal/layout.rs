//! The frame: the order in which the terminal polls, draws its regions and performs actions.

use super::{Action, Terminal};
use crate::ui_kit::notifications::NotificationPanel;

/// Where a group draws in the frame, in drawing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Region {
    /// Above the top toolbar, such as the layout tabs.
    Top,
    /// The left edge: the 54 point drawing toolbar panel.
    Left,
    /// The right edge: the 49 point widget bar and the side panel beside it.
    Right,
    /// The bottom edge: the dock, then the 39 point bottom bar panel.
    Bottom,
    /// Over the chart area, after the charts are drawn.
    Chart,
    /// Floating windows and dialogs.
    Windows,
}

impl Terminal {
    /// Draws the terminal into `ui`, performs what its controls emitted, and returns the actions
    /// it did not perform: intercepted ones, host entries (`Host` variants, host widgets) and any
    /// it has no default for, in emission order.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let ctx = ui.ctx().clone();
        if self.ctx.is_none() {
            self.first_frame(&ctx);
        }
        self.update_theme(&ctx);
        crate::theme::apply_to_egui(&ctx, &self.theme);
        // The root Ui was created before the theme was applied this frame.
        ui.set_style(ctx.global_style());
        self.poll_settings(&ctx);
        self.chart_events.clear();
        self.drawings_poll(&ctx);
        self.studies_poll(&ctx);
        self.time_poll(&ctx);
        self.replay_poll(&ctx);
        self.alerts_poll(&ctx);
        self.panels_poll(&ctx);
        self.trading_poll(&ctx);
        self.layouts_poll(&ctx);
        self.compare_poll(&ctx);
        self.commands_poll(&ctx);

        let panel_bg = ui.style().visuals.panel_fill;
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, self.theme.ui.border);
        // Dialogs take their keys before the chart's shortcuts.
        if self.keyboard_dialog_open() {
            self.chart_mut().suppress_shortcuts(&ctx);
        }

        self.show_region(ui, Region::Top);
        let toolbar_bg = if self.theme.dark {
            panel_bg.lerp_to_gamma(self.theme.ui.text, 0.07)
        } else {
            panel_bg
        };
        egui::Panel::top("egui_charts_terminal_toolbar")
            .exact_size(self.top_toolbar.config.height + 4.0)
            .show_separator_line(false)
            .frame(panel_frame(toolbar_bg).outer_margin(egui::Margin {
                bottom: 4,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| self.show_top_toolbar(ui));
        self.show_region(ui, Region::Left);
        self.show_region(ui, Region::Right);
        self.show_region(ui, Region::Bottom);
        egui::CentralPanel::default()
            .frame(panel_frame(panel_bg))
            .show(ui, |ui| {
                let mut slots: Vec<_> = self.charts.iter_mut().collect();
                self.layout.show(ui, &mut slots);
            });
        if self.layout.active() != self.active {
            self.activate_slot(self.layout.active());
        }
        for (index, chart) in self.charts.iter_mut().enumerate() {
            self.emitted.extend(
                chart
                    .take_shortcut_requests()
                    .into_iter()
                    .map(Action::Shortcut),
            );
            self.chart_events
                .extend(chart.drain_events().map(|event| (index, event)));
        }
        self.show_region(ui, Region::Chart);
        if let Some(action) = self.menus.show(&ctx, &mut self.charts[self.active]) {
            self.emit(Action::ChartMenu(action));
        }
        self.show_region(ui, Region::Windows);

        self.perform_emitted(&ctx);
        self.show_notice(ui);
        std::mem::take(&mut self.returned)
    }

    /// Wires the slots and groups, applies stored settings and performs the actions run before
    /// this first frame.
    fn first_frame(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
        // The toolbars draw their embedded SVG icons through egui's image loaders.
        egui_extras::install_image_loaders(ctx);
        for index in 0..self.charts.len() {
            self.wire_slot(index);
        }
        self.drawings_init();
        self.studies_init();
        self.time_init();
        self.replay_init();
        self.alerts_init();
        self.panels_init();
        self.trading_init();
        self.layouts_init();
        self.compare_init();
        self.commands_init();
        self.update_theme(ctx);
        self.poll_settings(ctx);
        for action in std::mem::take(&mut self.pending) {
            if let Some(action) = self.perform(action) {
                self.returned.push(action);
            }
        }
    }

    fn show_region(&mut self, ui: &mut egui::Ui, region: Region) {
        self.drawings_show(ui, region);
        self.studies_show(ui, region);
        self.time_show(ui, region);
        self.replay_show(ui, region);
        self.alerts_show(ui, region);
        self.panels_show(ui, region);
        self.trading_show(ui, region);
        self.layouts_show(ui, region);
        self.compare_show(ui, region);
        self.commands_show(ui, region);
    }

    /// Performs the actions emitted this frame in order, after the controls are drawn, and
    /// returns the intercepted ones and what is left of the others.
    fn perform_emitted(&mut self, ctx: &egui::Context) {
        let mut performed = false;
        // Performing an action may emit another, which is handled in turn.
        while !self.emitted.is_empty() {
            for action in std::mem::take(&mut self.emitted) {
                if self
                    .intercept
                    .as_mut()
                    .is_some_and(|intercept| intercept(&action))
                {
                    self.returned.push(action);
                } else if let Some(action) = self.perform(action) {
                    self.returned.push(action);
                } else {
                    performed = true;
                }
            }
        }
        if performed {
            ctx.request_repaint();
        }
    }

    fn show_notice(&mut self, ui: &egui::Ui) {
        let spacing = self.theme.spacing;
        let width = (ui.clip_rect().right() - 2.0 * (spacing.xxl + spacing.lg) - 2.0).min(340.0);
        // Trading notices reserve 80 points for the icon, padding and close button.
        if width <= 80.0 {
            return;
        }
        NotificationPanel::new()
            .width(width)
            .show_with_toasts(ui.ctx(), &mut self.toasts);
    }
}

/// The frame of the terminal's panels: filled, rounded and without margins.
pub(super) fn panel_frame(bg: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .corner_radius(4)
        .fill(bg)
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin::ZERO)
}
