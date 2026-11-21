//! Series settings dialog with sidebar navigation.

use crate::chart::series::{SeriesId, SeriesSettings};
use crate::config::ChartConfig;
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
#[derive(Debug)]
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

    /// Show the dialog and return the requested action.
    pub fn show(&mut self, ctx: &Context) -> SeriesSettingsAction {
        if !self.is_open {
            return SeriesSettingsAction::None;
        }

        let mut action = SeriesSettingsAction::None;

        let object = match self.series_id {
            Some(SeriesId::MAIN) => l10n::text(ctx, "Main series"),
            Some(id) => l10n::format_text(ctx, "Series {number}", &[("number", id.0.to_string())]),
            None => l10n::text(ctx, "Series"),
        };
        let title = format!(
            "{object} · {}",
            l10n::text(ctx, self.settings.chart_type.name())
        );
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
                    // The theme does not change on Ok, so the chart will not follow it afterwards.
                    crate::widget::palette::follow_candle_colors(
                        &mut self.settings.chart_type_params.styles,
                        &[ChartConfig::default()],
                        &ChartConfig::default().with_theme(&theme),
                    );
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
    fn the_sidebar_paints_the_full_scales_and_lines_label() {
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let mut dialog = SeriesSettingsDialog::new();
            dialog.open(SeriesId::MAIN, SeriesSettings::default());
            let mut h = Harness::builder()
                .allow_missing_glyphs()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &theme);
                })
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    |ui, dialog: &mut SeriesSettingsDialog| {
                        dialog.show(ui.ctx());
                    },
                    dialog,
                );
            h.run();
            for selected in [false, true] {
                if selected {
                    h.get_by_role_and_label(Role::Tab, "Scales and lines")
                        .click();
                    h.run();
                }
                let label = h
                    .output()
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Scales and lines" => {
                            Some((shape.clip_rect, text))
                        }
                        _ => None,
                    })
                    .expect("tab label is painted");
                assert!(!label.1.galley.elided, "selected: {selected}");
                assert!(label.0.contains_rect(egui::Rect::from_min_size(
                    label.1.pos,
                    label.1.galley.size()
                )));
            }
        }
    }

    #[test]
    fn accepting_preserves_candle_colours_and_reset_uses_the_active_theme() {
        for (_, theme) in crate::theme::presets() {
            for chart_type in [ChartType::Candles, ChartType::HlcArea] {
                for reset in [false, true] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut chart = crate::ChartBuilder::new().with_theme(theme.clone()).build();
                    chart.set_chart_type(chart_type);
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
                    let styles = &applied.chart_type_params.styles;
                    let [high_line, low_line, high_to_close_fill, close_to_low_fill] = if reset {
                        let (up, down) = (theme.chart.candle_up, theme.chart.candle_down);
                        let [r, g, b, _] = up.to_srgba_unmultiplied();
                        let [dr, dg, db, _] = down.to_srgba_unmultiplied();
                        [
                            up,
                            down,
                            egui::Color32::from_rgba_unmultiplied(r, g, b, 51),
                            egui::Color32::from_rgba_unmultiplied(dr, dg, db, 51),
                        ]
                    } else {
                        let style = &original.chart_type_params.styles.hlc_area;
                        [
                            style.high_line.color,
                            style.low_line.color,
                            style.high_to_close_fill,
                            style.close_to_low_fill,
                        ]
                    };
                    assert_eq!(styles.hlc_area.high_line.color, high_line);
                    assert_eq!(styles.hlc_area.low_line.color, low_line);
                    assert_eq!(styles.hlc_area.high_to_close_fill, high_to_close_fill);
                    assert_eq!(styles.hlc_area.close_to_low_fill, close_to_low_fill);
                    let volume = &original.chart_type_params.styles.volume_candles;
                    let (ups, downs) = if reset {
                        ([theme.chart.candle_up; 3], [theme.chart.candle_down; 3])
                    } else {
                        (
                            [volume.body_up, volume.border_up, volume.wick_up],
                            [volume.body_down, volume.border_down, volume.wick_down],
                        )
                    };
                    let volume = &styles.volume_candles;
                    assert_eq!([volume.body_up, volume.border_up, volume.wick_up], ups);
                    assert_eq!(
                        [volume.body_down, volume.border_down, volume.wick_down],
                        downs
                    );
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

    /// The accepted colours of a hollow or volume candles series reach its candles, a border or
    /// wick colour of `None` meaning the body colour. The first bar closes above its open and is
    /// outlined as a hollow candle and filled as a volume candle; the second closes below its open
    /// and is filled. An opaque border in the colour of its fill paints as the fill alone.
    #[test]
    fn series_settings_colours_reach_hollow_and_volume_candles() {
        use crate::model::{Bar, BarData};
        let time = chrono::DateTime::UNIX_EPOCH;
        let up = egui::Color32::from_rgb(1, 100, 2);
        let down = egui::Color32::from_rgb(101, 2, 3);
        let none = egui::Color32::TRANSPARENT;
        for (kind, bodies_expected) in [
            (ChartType::HollowCandles, [(none, up), (down, none)]),
            (ChartType::VolumeCandles, [(up, none), (down, none)]),
        ] {
            let mut chart = crate::Chart::new(BarData::from_bars(vec![
                Bar::new(time, 10.0, 18.0, 8.0, 14.0, 100.0),
                Bar::new(
                    time + chrono::Duration::days(1),
                    13.0,
                    17.0,
                    8.5,
                    11.0,
                    100.0,
                ),
            ]));
            chart.config_mut().show_volume = false;
            chart.set_chart_type(kind);
            let candle = chart.config().bullish_color;
            let mut settings = chart.series_settings();
            assert_eq!(settings.bullish_border_color, None);
            assert_eq!(settings.bearish_wick_color, None);
            settings.bullish_color = up;
            settings.bearish_color = down;
            chart.apply_series_settings(&settings);
            let (bodies, wicks) = painted_candles(&mut chart, &[up, down]);
            assert_eq!(bodies, bodies_expected, "{kind:?}");
            assert_eq!(wicks, [up, up, down, down], "{kind:?}");
            assert_eq!(chart.series_settings().bullish_color, up, "{kind:?}");
            chart.set_chart_type(ChartType::Candles);
            assert_eq!(chart.series_settings().bullish_color, candle, "{kind:?}");
        }
    }

    /// A chart of the closes 100, 102 and 101.5 with the colours `up` and `down` as its candle
    /// body colours and no last price line: one thin Kagi line from 100 to 102, or one column of
    /// Xs from 100 to 102.
    fn price_based_chart(kind: ChartType, [up, down]: [egui::Color32; 2]) -> crate::Chart {
        use crate::model::{Bar, BarData};
        let time = chrono::DateTime::UNIX_EPOCH;
        let bars = [100.0, 102.0, 101.5]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(time + chrono::Duration::days(i as i64), c, c, c, c, 0.0))
            .collect();
        let mut chart = crate::Chart::new(BarData::from_bars(bars));
        chart.config_mut().show_volume = false;
        chart.config_mut().bullish_color = up;
        chart.config_mut().bearish_color = down;
        chart.set_chart_type(kind);
        // The last price line takes the colours too; only the elements are read.
        let style = chart
            .series_style_mut(crate::widget::SeriesId::MAIN)
            .unwrap();
        style.price_line_visible = false;
        chart
    }

    /// The colours of the strokes `chart` paints in its plot, without repeats.
    fn painted_strokes(chart: &mut crate::Chart) -> Vec<egui::Color32> {
        use crate::widget::test_frames::{frame, shapes};
        let ctx = Context::default();
        frame(&ctx, chart, 0.0, vec![]);
        let painted = shapes(&ctx, chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let mut colors = Vec::new();
        for shape in painted {
            if let egui::Shape::LineSegment { points, stroke } = shape
                && plot.contains(points[0])
                && !colors.contains(&stroke.color)
            {
                colors.push(stroke.color);
            }
        }
        colors
    }

    /// Kagi lines and Point and Figure columns take the colours accepted for them and leave the
    /// candle colours alone, also when their type is chosen again. A layout or template saved
    /// before they had colours draws them in its candle body colours, as it was drawn.
    #[test]
    fn series_settings_colours_reach_kagi_lines_and_point_and_figure_columns() {
        let candles = [
            egui::Color32::from_rgb(8, 153, 129),
            egui::Color32::from_rgb(242, 54, 69),
        ];
        let own = [
            egui::Color32::from_rgb(1, 2, 3),
            egui::Color32::from_rgb(4, 5, 6),
        ];
        let saved = [
            egui::Color32::from_rgb(1, 100, 2),
            egui::Color32::from_rgb(101, 2, 3),
        ];
        // The thin Kagi line takes the falling colour, the X column the rising one.
        for (kind, side) in [(ChartType::Kagi, 1), (ChartType::PointAndFigure, 0)] {
            let mut chart = price_based_chart(kind, candles);
            let mut settings = chart.series_settings();
            let styles = &mut settings.chart_type_params.styles;
            [styles.kagi.up_color, styles.kagi.down_color] = own;
            [styles.point_figure.up_color, styles.point_figure.down_color] = own;
            chart.apply_series_settings(&settings);
            assert!(painted_strokes(&mut chart).contains(&own[side]), "{kind:?}");
            chart.set_chart_type(ChartType::Candles);
            let config = chart.config();
            assert_eq!([config.bullish_color, config.bearish_color], candles);
            let (bodies, _) = painted_candles(&mut chart, &own);
            assert!(bodies.is_empty(), "{kind:?}: {bodies:?}");
            chart.set_chart_type(kind);
            assert!(painted_strokes(&mut chart).contains(&own[side]), "{kind:?}");

            // The Kagi entry as saved before its colours, and no Point and Figure entry.
            let former: serde_json::Value =
                serde_json::from_str(r#"{"thick_width":2.5,"thin_width":1.5}"#).unwrap();
            for template in [false, true] {
                let saved_chart = price_based_chart(kind, saved);
                let mut restored = price_based_chart(kind, candles);
                if template {
                    let state =
                        crate::widget::settings::ChartSettingsState::from_chart(&saved_chart);
                    let mut json = serde_json::to_value(state).unwrap();
                    let styles = &mut json["chart_type_params"]["styles"];
                    styles["kagi"] = former.clone();
                    styles.as_object_mut().unwrap().remove("point_figure");
                    let state: crate::widget::settings::ChartSettingsState =
                        serde_json::from_value(json).unwrap();
                    state.apply_to_chart(&mut restored).unwrap();
                } else {
                    let snapshot = saved_chart.snapshot().unwrap();
                    let mut json = serde_json::to_value(snapshot).unwrap();
                    for params in ["/chart_type_params", "/settings/chart_type_params"] {
                        let styles = &mut json.pointer_mut(params).unwrap()["styles"];
                        styles["kagi"] = former.clone();
                        styles.as_object_mut().unwrap().remove("point_figure");
                    }
                    let snapshot: crate::widget::ChartSnapshot =
                        serde_json::from_value(json).unwrap();
                    restored.restore(&snapshot).unwrap();
                }
                let strokes = painted_strokes(&mut restored);
                assert!(
                    strokes.contains(&saved[side]),
                    "{kind:?} {template}: {strokes:?}"
                );
            }
        }
    }

    /// Every setting of the price-based chart types accepted in the dialog comes back from a
    /// saved layout.
    #[test]
    fn price_based_settings_survive_a_layout() {
        let color = |v: u8| Some(egui::Color32::from_rgb(v, 2 * v, 3 * v));
        let mut chart = price_based_chart(ChartType::Renko, [egui::Color32::GREEN; 2]);
        let mut settings = chart.series_settings();
        let params = &mut settings.chart_type_params;
        params.pnf_one_step_back = true;
        params.renko_real_price = false;
        params.kagi_real_price = false;
        params.line_break_real_price = false;
        params.pnf_real_price = false;
        let styles = &mut params.styles;
        [styles.kagi.up_color, styles.kagi.down_color] = [color(10), color(11)].map(Option::unwrap);
        let point_figure = [color(12), color(13)].map(Option::unwrap);
        [styles.point_figure.up_color, styles.point_figure.down_color] = point_figure;
        styles.range.thin_bars = true;
        styles.range.phantom_bars = true;
        styles.renko = crate::model::RenkoStyle {
            border_visible: false,
            border_up: color(14),
            border_down: color(15),
            wick_up: color(16),
            wick_down: color(17),
        };
        styles.line_break = crate::model::LineBreakStyle {
            border_visible: false,
            border_up: color(18),
            border_down: color(19),
        };
        chart.apply_series_settings(&settings);
        let json = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
        let mut restored = price_based_chart(ChartType::Candles, [egui::Color32::RED; 2]);
        restored
            .restore(&serde_json::from_str(&json).unwrap())
            .unwrap();
        assert_eq!(restored.chart_type_params(), &settings.chart_type_params);
    }

    /// Each placement, width, values and box control of the session profile edits only the draft:
    /// Cancel drops the edit, and Ok applies it as one undoable step.
    #[test]
    fn session_profile_layout_controls_apply_on_ok_only_and_undo() {
        use egui::Color32;
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        use serde_json::json;
        let picked =
            serde_json::to_value(Color32::from_rgba_unmultiplied(49, 27, 146, 128)).unwrap();
        // Colour wells are found by their seeded colour, so their label is unused.
        let cases = [
            ("Placement", Role::ComboBox, "right_aligned", json!(true)),
            ("Width", Role::SpinButton, "width", json!(0.99)),
            ("Volume values", Role::CheckBox, "values", json!(true)),
            ("", Role::ColorWell, "values_color", picked.clone()),
            ("Session box", Role::CheckBox, "session_box", json!(false)),
            ("", Role::ColorWell, "session_box_color", picked),
        ];
        for (label, role, field, value) in cases {
            for accept in [false, true] {
                let mut chart = crate::ChartBuilder::new().build();
                chart.set_chart_type(ChartType::SessionVolumeProfile);
                // Theme colours can repeat across rows; seeded ones label each well apart.
                let mut params = chart.chart_type_params().clone();
                params.profile.values_color =
                    Some(Color32::from_rgba_unmultiplied(10, 20, 30, 128));
                params.profile.session_box_color =
                    Some(Color32::from_rgba_unmultiplied(40, 50, 60, 128));
                chart.set_chart_type_params(params);
                let seeded = chart.trade_profile_settings().clone();
                let saved = serde_json::to_value(&seeded).unwrap();
                let mut expected = saved.clone();
                expected[field] = value.clone();
                let label = if role == Role::ColorWell {
                    let color: Color32 = serde_json::from_value(saved[field].clone()).unwrap();
                    let [r, g, b, _] = color.to_srgba_unmultiplied();
                    format!("#{r:02X}{g:02X}{b:02X}")
                } else {
                    label.to_owned()
                };
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(SeriesId::MAIN, chart.series_settings());
                let mut harness = Harness::builder()
                    .with_context_setup(|ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                    })
                    .with_size(Vec2::new(1440.0, 900.0))
                    .build_ui_state(
                        |ui, (dialog, chart): &mut (SeriesSettingsDialog, crate::Chart)| {
                            if let SeriesSettingsAction::Apply(_, settings) = dialog.show(ui.ctx())
                            {
                                chart.user_action(|c| c.apply_series_settings(&settings));
                            }
                        },
                        (dialog, chart),
                    );
                harness.run();
                // The profile section is taller than the dialog.
                harness.get_by_role_and_label(role, &label).scroll_to_me();
                harness.run();
                let control = harness.get_by_role_and_label(role, &label);
                match role {
                    Role::SpinButton => control.decrement_accesskit(),
                    _ => control.click(),
                }
                harness.run();
                match role {
                    Role::ComboBox => harness.get_by_label("Right").click(),
                    Role::ColorWell => harness
                        .get_by_role_and_label(Role::ColorWell, "#311B92")
                        .click(),
                    _ => {}
                }
                harness.run();
                assert_eq!(
                    harness.state().1.trade_profile_settings(),
                    &seeded,
                    "{field}: draft only"
                );
                harness
                    .get_by_label(if accept { "Ok" } else { "Cancel" })
                    .click();
                harness.run();
                let chart = &mut harness.state_mut().1;
                let current = |chart: &crate::Chart| {
                    serde_json::to_value(chart.trade_profile_settings()).unwrap()
                };
                if accept {
                    assert_eq!(current(chart), expected, "{field}");
                    assert!(chart.undo(), "{field}");
                    assert_eq!(current(chart), saved, "{field}");
                    assert!(chart.redo(), "{field}");
                    assert_eq!(current(chart), expected, "{field}");
                } else {
                    assert_eq!(current(chart), saved, "{field}");
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
                        let inactive = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if matches!(text.galley.text(), "Trading" | "Alerts" | "Events" | "Coming soon")));
                        output.drop_without_applying_deltas();
                        assert!(!inactive, "placeholder settings must not be offered");
                        if frame > 0 {
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
