//! Series settings dialog with sidebar navigation.

use crate::chart::series::{SeriesId, SeriesSettings};
use crate::localization as l10n;
use egui::{Context, Ui, Vec2};

use super::actions::SeriesSettingsAction;
use super::config::SeriesSettingsConfig;
use super::tabs::{CanvasTab, ScalesTab, SeriesSettingsTab, StatusLineTab, SymbolTab};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};

/// Series settings dialog with sidebar layout
pub struct SeriesSettingsDialog {
    /// Display and layout options for this component.
    pub config: SeriesSettingsConfig,
    /// Whether the dialog is currently visible.
    pub is_open: bool,
    /// Identifier of the series being edited, when open.
    pub series_id: Option<SeriesId>,
    /// Currently selected tab.
    pub active_tab: SeriesSettingsTab,
    /// Editable settings currently shown by the dialog.
    pub settings: SeriesSettings,
    /// Settings captured when the dialog opened.
    pub original_settings: SeriesSettings,
}

impl Default for SeriesSettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SeriesSettingsDialog {
    /// Create a [`SeriesSettingsDialog`] with default settings.
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

    /// Open the dialog with the supplied state.
    pub fn open(&mut self, id: SeriesId, settings: SeriesSettings) {
        self.is_open = true;
        self.series_id = Some(id);
        self.settings = settings.clone();
        self.original_settings = settings;
        self.active_tab = SeriesSettingsTab::default();
    }

    /// Close the dialog.
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

    /// Show the dialog and return the requested action.
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

        // Footer
        let (ok, cancel) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .show_with_left(ui, |ui| {
                if ui
                    .button(l10n::text(ui.ctx(), "Reset to defaults"))
                    .clicked()
                {
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

    /// The candle bodies as (fill, border) and the wick colours of `chart` in painting order. Fills
    /// and strokes outside `known` belong to other shapes in the plot.
    fn painted_candles(
        chart: &mut crate::Chart,
        known: &[egui::Color32],
    ) -> (Vec<(egui::Color32, egui::Color32)>, Vec<egui::Color32>) {
        use crate::widget::test_frames::{frame, shapes};
        let ctx = Context::default();
        frame(&ctx, chart, 0.0, vec![]);
        let painted = shapes(&ctx, chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let (mut bodies, mut wicks) = (Vec::new(), Vec::new());
        for shape in painted {
            match shape {
                egui::Shape::Rect(r)
                    if plot.contains(r.rect.center())
                        && (known.contains(&r.fill) || known.contains(&r.stroke.color)) =>
                {
                    bodies.push((r.fill, r.stroke.color));
                }
                egui::Shape::LineSegment { points, stroke }
                    if points[0].x == points[1].x && known.contains(&stroke.color) =>
                {
                    wicks.push(stroke.color);
                }
                _ => {}
            }
        }
        (bodies, wicks)
    }

    /// The accepted colours of a Heikin-Ashi series reach its candles and come back from
    /// `series_settings`; the chart keeps its candle colours for the other chart types. The bar
    /// (10, 18, 8, 14) has the transformed open (10 + 14) / 2 = 12 and close 50 / 4 = 12.5, so it
    /// rises and takes the rising colours.
    #[test]
    fn series_settings_colours_reach_heikin_candles() {
        use crate::model::{Bar, BarData};
        let mut chart = crate::Chart::new(BarData::from_bars(vec![Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            10.0,
            18.0,
            8.0,
            14.0,
            0.0,
        )]));
        chart.config_mut().show_volume = false;
        chart.set_chart_type(ChartType::Heikin);
        let candle = chart.config().bullish_color;
        let body = egui::Color32::from_rgb(1, 100, 2);
        let border = egui::Color32::from_rgb(3, 4, 102);
        let wick = egui::Color32::from_rgb(105, 106, 6);
        let mut settings = chart.series_settings();
        settings.bullish_color = body;
        settings.bullish_border_color = Some(border);
        settings.bullish_wick_color = Some(wick);
        chart.apply_series_settings(&settings);
        assert_eq!(
            painted_candles(&mut chart, &[body, border, wick]),
            (vec![(body, border)], vec![wick, wick])
        );
        let read = chart.series_settings();
        assert_eq!(read.bullish_color, body);
        assert_eq!(read.bullish_border_color, Some(border));
        assert_eq!(read.bullish_wick_color, Some(wick));
        chart.set_chart_type(ChartType::Candles);
        assert_eq!(chart.series_settings().bullish_color, candle);
    }

    /// The accepted colours of a hollow candles series reach its candles, a border or wick colour
    /// of `None` meaning the body colour. The first bar closes above its open and is outlined; the
    /// second closes below its open and is filled.
    #[test]
    fn series_settings_colours_reach_hollow_candles() {
        use crate::model::{Bar, BarData};
        let time = chrono::DateTime::UNIX_EPOCH;
        let mut chart = crate::Chart::new(BarData::from_bars(vec![
            Bar::new(time, 10.0, 18.0, 8.0, 14.0, 0.0),
            Bar::new(time + chrono::Duration::days(1), 13.0, 17.0, 8.5, 11.0, 0.0),
        ]));
        chart.config_mut().show_volume = false;
        chart.set_chart_type(ChartType::HollowCandles);
        let candle = chart.config().bullish_color;
        let up = egui::Color32::from_rgb(1, 100, 2);
        let down = egui::Color32::from_rgb(101, 2, 3);
        let mut settings = chart.series_settings();
        assert_eq!(settings.bullish_border_color, None);
        assert_eq!(settings.bearish_wick_color, None);
        settings.bullish_color = up;
        settings.bearish_color = down;
        chart.apply_series_settings(&settings);
        let (bodies, wicks) = painted_candles(&mut chart, &[up, down]);
        assert_eq!(
            bodies.iter().map(|body| body.0).collect::<Vec<_>>(),
            [egui::Color32::TRANSPARENT, down]
        );
        assert_eq!(bodies[0].1, up, "the rising candle is outlined");
        assert_eq!(wicks, [up, up, down, down]);
        assert_eq!(chart.series_settings().bullish_color, up);
        chart.set_chart_type(ChartType::Candles);
        assert_eq!(chart.series_settings().bullish_color, candle);
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
