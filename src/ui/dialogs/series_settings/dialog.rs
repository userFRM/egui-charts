//! Series settings dialog with sidebar navigation.

use crate::chart::series::{SeriesId, SeriesSettings};
use egui::{Context, Ui, Vec2};

use super::actions::SeriesSettingsAction;
use super::config::SeriesSettingsConfig;
use super::tabs::{CanvasTab, ScalesTab, SeriesSettingsTab, StatusLineTab, SymbolTab};
use crate::ext::UiExt;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};

/// Series settings dialog with sidebar layout
pub struct SeriesSettingsDialog {
    pub config: SeriesSettingsConfig,
    pub is_open: bool,
    pub series_id: Option<SeriesId>,
    pub active_tab: SeriesSettingsTab,
    pub settings: SeriesSettings,
    pub original_settings: SeriesSettings,
}

impl Default for SeriesSettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SeriesSettingsDialog {
    pub fn new() -> Self {
        Self {
            config: SeriesSettingsConfig::default(),
            is_open: false,
            series_id: None,
            active_tab: SeriesSettingsTab::default(),
            settings: SeriesSettings::default(),
            original_settings: SeriesSettings::default(),
        }
    }

    pub fn open(&mut self, id: SeriesId, settings: SeriesSettings) {
        self.is_open = true;
        self.series_id = Some(id);
        self.settings = settings.clone();
        self.original_settings = settings;
        self.active_tab = SeriesSettingsTab::default();
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.series_id = None;
    }

    /// Sync dialog open/close state from an external flag.
    ///
    /// Opening requires series context, so only close is synced here.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open && self.is_open {
            self.close();
        }
    }

    pub fn show(&mut self, ctx: &Context) -> SeriesSettingsAction {
        if !self.is_open {
            return SeriesSettingsAction::None;
        }

        let mut action = SeriesSettingsAction::None;

        let object = if self.series_id == Some(SeriesId::MAIN) {
            "Main series".to_owned()
        } else {
            format!("Series {:?}", self.series_id)
        };
        let title = format!("{object} · {}", self.settings.chart_type.name());
        let mut open = true;
        Modal::new("series_settings", title)
            .width(self.config.width + 32.0)
            .height(self.config.height + 32.0)
            .show(ctx, &mut open, |ui| {
                action = self.render_contents(ui);
            });
        if !open {
            self.is_open = false;
            action = SeriesSettingsAction::Cancel;
        }

        action
    }

    fn render_contents(&mut self, ui: &mut Ui) -> SeriesSettingsAction {
        let mut action = SeriesSettingsAction::None;

        // Main content: sidebar + content area
        let sidebar_width = DESIGN_TOKENS.sizing.dialog.series_sidebar_width;
        let seam_top = ui.cursor().top();
        let seam_x = ui.cursor().left()
            + sidebar_width.min(ui.available_width() * 0.4)
            + ui.spacing().item_spacing.x;
        let active_tab = self.active_tab;
        let active_tab_mut = &mut self.active_tab;
        let settings = &mut self.settings;

        // Reserve the footer before the sidebar layout consumes the remaining height.
        let body_height = (ui.available_height() - 64.0).max(0.0);
        ui.allocate_ui(Vec2::new(ui.available_width(), body_height), |ui| {
            SidebarLayout::new(sidebar_width.min(ui.available_width() * 0.4)).show(
                ui,
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("series_tabs")
                        .show(ui, |ui| {
                            ui.space_sm();
                            for tab in
                                [SeriesSettingsTab::Symbol, SeriesSettingsTab::ScalesAndLines]
                            {
                                if SidebarTab::new(tab.label(), *active_tab_mut == tab)
                                    .icon(tab.icon())
                                    .show(ui)
                                {
                                    *active_tab_mut = tab;
                                }
                            }
                        });
                },
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("series_content")
                        .show(ui, |ui| {
                            ui.space_md();
                            match active_tab {
                                SeriesSettingsTab::Symbol => SymbolTab::show(ui, settings),
                                SeriesSettingsTab::StatusLine => StatusLineTab::show(ui, settings),
                                SeriesSettingsTab::ScalesAndLines => ScalesTab::show(ui, settings),
                                SeriesSettingsTab::Canvas => CanvasTab::show(ui, settings),
                            }
                        });
                },
            );
        });

        ui.painter().vline(
            seam_x,
            seam_top..=ui.cursor().top() + 8.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        // Footer
        let (ok, cancel) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .show_with_left(ui, |ui| {
                if ui.button("Reset to defaults").clicked() {
                    let theme = ui.active_theme();
                    self.settings = SeriesSettings {
                        chart_type: self.settings.chart_type,
                        bullish_color: theme.chart.candle_up,
                        bearish_color: theme.chart.candle_down,
                        ..Default::default()
                    };
                }
            });
        if ok && let Some(id) = self.series_id {
            action = SeriesSettingsAction::Apply(id, Box::new(self.settings.clone()));
            self.is_open = false;
        }
        if cancel {
            action = SeriesSettingsAction::Cancel;
            self.is_open = false;
        }

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChartType;

    #[test]
    fn accepting_preserves_candle_colours_and_reset_uses_the_active_theme() {
        for (_, theme) in crate::theme::presets() {
            for reset in [false, true] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::ChartBuilder::new().with_theme(theme.clone()).build();
                chart.config_mut().bullish_color = egui::Color32::YELLOW;
                chart.config_mut().bearish_wick_color = Some(egui::Color32::BLUE);
                let original = chart.series_settings();
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(SeriesId::MAIN, original.clone());
                let mut frame = |events, label: &str| {
                    let mut action = SeriesSettingsAction::None;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(1440.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| action = dialog.show(ui.ctx()),
                    );
                    let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    (action, pos)
                };
                let mut click = |label: &str| {
                    frame(vec![], label);
                    let pos = frame(vec![], label).1.expect("footer control is visible");
                    frame(vec![egui::Event::PointerMoved(pos)], label);
                    let button = |pressed| egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    };
                    frame(vec![button(true)], label);
                    frame(vec![button(false)], label).0
                };
                if reset {
                    click("Reset to defaults");
                }
                let SeriesSettingsAction::Apply(_, settings) = click("Ok") else {
                    panic!("expected Apply")
                };
                chart.apply_series_settings(&settings);
                let applied = chart.series_settings();
                assert_eq!(
                    applied.bullish_color,
                    if reset {
                        theme.chart.candle_up
                    } else {
                        original.bullish_color
                    }
                );
                assert_eq!(
                    applied.bearish_color,
                    if reset {
                        theme.chart.candle_down
                    } else {
                        original.bearish_color
                    }
                );
                for (actual, expected) in [
                    (applied.bullish_border_color, original.bullish_border_color),
                    (applied.bearish_border_color, original.bearish_border_color),
                    (applied.bullish_wick_color, original.bullish_wick_color),
                    (applied.bearish_wick_color, original.bearish_wick_color),
                ] {
                    assert_eq!(actual, if reset { None } else { expected });
                }
            }
        }
    }

    #[test]
    fn type_controls_leave_the_footer_inside_short_and_narrow_windows() {
        for dark in [false, true] {
            for size in [
                Vec2::new(1440.0, 868.0),
                Vec2::new(770.0, 520.0),
                Vec2::new(420.0, 360.0),
            ] {
                for kind in [ChartType::Area, ChartType::Baseline, ChartType::Renko] {
                    let ctx = Context::default();
                    let theme = if dark {
                        crate::theme::Theme::dark()
                    } else {
                        crate::theme::Theme::light()
                    };
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                    let mut dialog = SeriesSettingsDialog::new();
                    dialog.open(
                        SeriesId::MAIN,
                        SeriesSettings {
                            chart_type: kind,
                            ..Default::default()
                        },
                    );
                    for frame in 0..3 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| {
                                dialog.show(ui.ctx());
                            },
                        );
                        let ok = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "Ok" => Some((
                                shape.clip_rect,
                                egui::Rect::from_min_size(text.pos, text.galley.size()),
                            )),
                            _ => None,
                        });
                        let inert_colour_controls = output.shapes.iter().any(|shape| {
                            matches!(
                                &shape.shape, egui::Shape::Text(text)
                                if matches!(text.galley.text(), "Border" | "Wick")
                            )
                        });
                        let inactive = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if matches!(text.galley.text(), "Trading" | "Alerts" | "Events" | "Coming soon")));
                        output.drop_without_applying_deltas();
                        assert!(!inactive, "placeholder settings must not be offered");
                        if frame > 0 {
                            if kind == ChartType::Renko {
                                assert!(
                                    !inert_colour_controls,
                                    "Renko must only offer colours its renderer reads"
                                );
                            }
                            let ok = ok.expect("apply button is painted");
                            assert!(
                                screen.contains_rect(ok.1) && ok.0.contains_rect(ok.1),
                                "{kind:?} {size:?}: footer {ok:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
