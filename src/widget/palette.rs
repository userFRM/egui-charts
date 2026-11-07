//! Active palette propagation for chart configuration colours.

use super::Chart;
use crate::config::ChartConfig;
use crate::model::{ChartTypeStyles, HlcAreaStyle};
use crate::theme::Theme;

impl Chart {
    pub(super) fn follow_chart_theme(&mut self, theme: &Theme) {
        if self.inherited_theme.as_ref() == Some(theme) && self.restored_palettes.is_empty() {
            return;
        }
        let mut old = std::mem::take(&mut self.restored_palettes);
        if old.is_empty() {
            old.push(self.followed_palette());
        }
        let new = ChartConfig::default().with_theme(theme);
        follow_palette(&mut self.config, &old, &new);
        self.chart_type_params
            .styles
            .follow_candle_colors(&old, &new);
        self.inherited_theme = Some(theme.clone());
    }

    /// The colours the chart's unedited fields follow.
    pub(super) fn followed_palette(&self) -> ChartConfig {
        self.inherited_theme
            .as_ref()
            .map_or_else(ChartConfig::default, |theme| {
                ChartConfig::default().with_theme(theme)
            })
    }

    /// Lets the colours of a restored configuration that still match the palette they were saved
    /// with follow a per-chart theme immediately, or the active theme on the next frame. Layouts
    /// saved without that palette are compared with both built-in themes.
    #[cfg(feature = "ui")]
    pub(super) fn follow_restored_palette(&mut self, saved: Option<&ChartConfig>) {
        self.restored_palettes = saved.map_or_else(
            || {
                [Theme::light(), Theme::dark()]
                    .iter()
                    .map(|theme| ChartConfig::default().with_theme(theme))
                    .collect()
            },
            |saved| vec![saved.clone()],
        );
        if let Some(theme) = self.theme_override.clone() {
            self.follow_chart_theme(&theme);
        }
    }
}

impl ChartTypeStyles {
    /// Replaces every colour of the styles painting from their own colours that is equal to a
    /// candle colour of one of the `old` palettes by the one of `new`; the others were edited and
    /// stay.
    pub(crate) fn follow_candle_colors(&mut self, old: &[ChartConfig], new: &ChartConfig) {
        // Volume candles paint from their own style, which follows like the chart's candle colours.
        let style = &mut self.volume_candles;
        for color in [&mut style.body_up, &mut style.border_up, &mut style.wick_up] {
            if old.iter().any(|old| *color == old.bullish_color) {
                *color = new.bullish_color;
            }
        }
        for color in [
            &mut style.body_down,
            &mut style.border_down,
            &mut style.wick_down,
        ] {
            if old.iter().any(|old| *color == old.bearish_color) {
                *color = new.bearish_color;
            }
        }
        // So do the HLC area's high and low lines and fills; a fill keeps its opacity.
        let style = &mut self.hlc_area;
        for (line, fill, side) in [
            (
                &mut style.high_line.color,
                &mut style.high_to_close_fill,
                (|config| config.bullish_color) as fn(&ChartConfig) -> egui::Color32,
            ),
            (
                &mut style.low_line.color,
                &mut style.close_to_low_fill,
                |config| config.bearish_color,
            ),
        ] {
            if old.iter().any(|old| *line == side(old)) {
                *line = side(new);
            }
            let alpha = fill.a();
            if old
                .iter()
                .any(|old| *fill == HlcAreaStyle::fill_tint(side(old), alpha))
            {
                *fill = HlcAreaStyle::fill_tint(side(new), alpha);
            }
        }
    }
}

/// Replaces every colour of `config` equal to the same colour of one of the `old` palettes by
/// the one of `new`; the others were edited and stay.
fn follow_palette(config: &mut ChartConfig, old: &[ChartConfig], new: &ChartConfig) {
    macro_rules! follow {
        ($($field:ident),+ $(,)?) => { $(if old.iter().any(|old| config.$field == old.$field) { config.$field = new.$field; })+ };
    }
    follow!(
        background_color,
        grid_color,
        text_color,
        bullish_color,
        bearish_color,
        bullish_border_color,
        bearish_border_color,
        bullish_wick_color,
        bearish_wick_color,
        session_break_color,
        watermark_color
    );
}

#[cfg(test)]
mod tests {
    use crate::theme::{Theme, apply_to_egui};
    use crate::widget::test_frames::{chart, frame};

    #[cfg(feature = "ui")]
    #[test]
    fn restored_snapshots_follow_the_active_palette_and_keep_edited_colours() {
        const LIGHT: egui::Color32 = egui::Color32::from_rgb(211, 212, 215);
        const DARK: egui::Color32 = egui::Color32::from_gray(34);
        for (from, to, from_watermark, to_watermark) in [
            (Theme::light(), Theme::dark(), LIGHT, DARK),
            (Theme::dark(), Theme::light(), DARK, LIGHT),
        ] {
            for edited in [false, true] {
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &from);
                let mut saved = chart(&[100.0], &[110.0]);
                frame(&ctx, &mut saved, 0.0, vec![]);
                saved.config_mut().bearish_color = egui::Color32::RED;
                if edited {
                    saved.config_mut().session_break_color = egui::Color32::BLUE;
                    saved.config_mut().watermark_color = egui::Color32::RED;
                }
                let settings = crate::widget::settings::ChartSettingsState::from_chart(&saved);
                assert_eq!(settings.watermark.color, saved.config().watermark_color);
                settings.apply_to_chart(&mut saved).unwrap();
                let watermark = if edited {
                    egui::Color32::RED
                } else {
                    from_watermark
                };
                assert_eq!(saved.config().watermark_color, watermark);
                let snapshot = saved.snapshot().unwrap();
                apply_to_egui(&ctx, &to);
                // Layouts saved before the palette was recorded behave the same.
                for with_palette in [true, false] {
                    let mut snapshot = snapshot.clone();
                    if !with_palette {
                        snapshot.palette = None;
                    }
                    let mut reopened = chart(&[100.0], &[110.0]);
                    frame(&ctx, &mut reopened, 1.0, vec![]);
                    reopened.restore(&snapshot).unwrap();
                    frame(&ctx, &mut reopened, 2.0, vec![]);
                    assert_eq!(reopened.config().background_color, to.chart.bg);
                    assert_eq!(reopened.config().grid_color, to.chart.grid_line);
                    assert_eq!(reopened.config().bullish_color, to.chart.candle_up);
                    assert_eq!(
                        reopened.config().session_break_color,
                        if edited {
                            egui::Color32::BLUE
                        } else {
                            to.chart.grid_line_major
                        }
                    );
                    assert_eq!(
                        reopened.config().watermark_color,
                        if edited {
                            egui::Color32::RED
                        } else {
                            to_watermark
                        }
                    );
                    assert_eq!(reopened.config().bearish_color, egui::Color32::RED);
                }
            }
        }
    }

    #[test]
    fn context_palette_changes_reach_charts_and_keep_host_edits() {
        let ctx = egui::Context::default();
        let mut shared = chart(&[100.0], &[110.0]);
        let mut built = crate::widget::ChartBuilder::new().build();
        let mut local = chart(&[100.0], &[110.0]);
        shared.config_mut().bearish_color = egui::Color32::RED;
        shared.chart_type_params.styles.volume_candles.wick_down = egui::Color32::RED;
        shared.chart_type_params.styles.hlc_area.low_line.color = egui::Color32::RED;
        local.set_theme_override(Some(Theme::high_contrast()));
        for (step, (theme, watermark)) in [
            (Theme::light(), egui::Color32::from_rgb(211, 212, 215)),
            (Theme::dark(), egui::Color32::from_gray(34)),
            (Theme::high_contrast(), egui::Color32::from_white_alpha(40)),
        ]
        .into_iter()
        .enumerate()
        {
            apply_to_egui(&ctx, &theme);
            frame(&ctx, &mut built, step as f64, vec![]);
            frame(&ctx, &mut shared, step as f64, vec![]);
            frame(&ctx, &mut local, step as f64, vec![]);
            assert_eq!(built.config().background_color, theme.chart.bg);
            assert_eq!(shared.config().background_color, theme.chart.bg);
            assert_eq!(shared.config().bullish_color, theme.chart.candle_up);
            assert_eq!(
                shared.config().session_break_color,
                theme.chart.grid_line_major
            );
            assert_eq!(shared.config().watermark_color, watermark);
            assert_eq!(shared.config().bearish_color, egui::Color32::RED);
            let volume_candles = &shared.chart_type_params.styles.volume_candles;
            assert_eq!(volume_candles.border_up, theme.chart.candle_up);
            assert_eq!(volume_candles.body_down, theme.chart.candle_down);
            assert_eq!(volume_candles.wick_down, egui::Color32::RED);
            let hlc_area = &shared.chart_type_params.styles.hlc_area;
            assert_eq!(hlc_area.high_line.color, theme.chart.candle_up);
            assert_eq!(hlc_area.low_line.color, egui::Color32::RED);
            assert_eq!(
                hlc_area.close_to_low_fill,
                crate::model::HlcAreaStyle::fill_tint(theme.chart.candle_down, 51)
            );
            assert_eq!(
                local.config().background_color,
                Theme::high_contrast().chart.bg
            );
        }
        apply_to_egui(&ctx, &Theme::light());
        local.config_mut().bullish_wick_color = Some(egui::Color32::BLUE);
        local.set_theme_override(None);
        frame(&ctx, &mut local, 4.0, vec![]);
        assert_eq!(local.config().background_color, Theme::light().chart.bg);
        assert_eq!(local.config().bullish_wick_color, Some(egui::Color32::BLUE));
        let mut resized = Theme::light();
        resized.text.chart_title = 23.0;
        resized.text.ohlc = 17.0;
        apply_to_egui(&ctx, &resized);
        shared.set_symbol("PALETTE");
        shared.add_indicator(Box::new(
            crate::studies::CustomIndicator::new("Pane font", |bars| {
                vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
            })
            .with_overlay(false),
        ));
        let painted = crate::widget::test_frames::shapes(&ctx, &mut shared, 5.0, vec![]);
        for (prefix, size) in [("PALETTE", 23.0), ("O ", 17.0), ("Pane font", 17.0)] {
            let matches = painted.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.galley.text().starts_with(prefix)
                    && text.galley.job.sections.iter()
                        .filter(|section| !text.galley.text()[section.byte_range.start.0..section.byte_range.end.0].trim().is_empty())
                        .all(|section| section.format.font_id.size == size)));
            assert!(matches, "theme text size for {prefix}");
        }
    }

    /// Both calls follow unedited chart type style colours as a theme change through the context
    /// does, and keep edited ones. The light preset's colours go to the high contrast preset's.
    #[test]
    fn set_theme_recolours_unedited_hlc_area_colours() {
        use egui::Color32;
        const EDITED: Color32 = Color32::from_rgb(1, 2, 3);
        let up = Color32::from_rgb(0, 220, 130);
        let down = Color32::from_rgb(255, 95, 95);
        let to = Theme::high_contrast();
        for via_override in [false, true] {
            for edited in [false, true] {
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &Theme::light());
                let mut chart = chart(&[100.0], &[110.0]);
                chart.set_chart_type(crate::model::ChartType::HlcArea);
                frame(&ctx, &mut chart, 0.0, vec![]);
                if edited {
                    let styles = &mut chart.chart_type_params.styles;
                    styles.hlc_area.high_line.color = EDITED;
                    styles.volume_candles.wick_up = EDITED;
                }
                if via_override {
                    chart.set_theme_override(Some(to.clone()));
                } else {
                    chart.set_theme(&ctx, &to);
                }
                frame(&ctx, &mut chart, 1.0, vec![]);
                assert_eq!(chart.config().bullish_color, up);
                let hlc_area = &chart.chart_type_params.styles.hlc_area;
                assert_eq!(hlc_area.high_line.color, if edited { EDITED } else { up });
                assert_eq!(hlc_area.low_line.color, down);
                assert_eq!(
                    hlc_area.high_to_close_fill,
                    Color32::from_rgba_unmultiplied(0, 220, 130, 51)
                );
                assert_eq!(
                    hlc_area.close_to_low_fill,
                    Color32::from_rgba_unmultiplied(255, 95, 95, 51)
                );
                let volume_candles = &chart.chart_type_params.styles.volume_candles;
                assert_eq!(volume_candles.body_up, up);
                assert_eq!(volume_candles.wick_up, if edited { EDITED } else { up });
                assert_eq!(volume_candles.wick_down, down);
            }
        }
    }
}
