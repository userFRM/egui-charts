//! Active palette propagation for chart configuration colours.

use super::Chart;
use crate::config::ChartConfig;
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
        follow_palette(
            &mut self.config,
            &old,
            &ChartConfig::default().with_theme(theme),
        );
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
    /// with follow the active theme from the next frame. Layouts saved without that palette are
    /// compared with both built-in themes.
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
        bearish_wick_color
    );
}

#[cfg(test)]
mod tests {
    use crate::theme::{Theme, apply_to_egui};
    use crate::widget::test_frames::{chart, frame};

    #[cfg(feature = "ui")]
    #[test]
    fn restored_snapshots_follow_the_active_palette_and_keep_edited_colours() {
        let ctx = egui::Context::default();
        apply_to_egui(&ctx, &Theme::light());
        let mut saved = chart(&[100.0], &[110.0]);
        frame(&ctx, &mut saved, 0.0, vec![]);
        saved.config_mut().bearish_color = egui::Color32::RED;
        let snapshot = saved.snapshot().unwrap();
        apply_to_egui(&ctx, &Theme::dark());
        let dark = Theme::dark();
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
            assert_eq!(reopened.config().background_color, dark.chart.bg);
            assert_eq!(reopened.config().grid_color, dark.chart.grid_line);
            assert_eq!(reopened.config().bullish_color, dark.chart.candle_up);
            assert_eq!(reopened.config().bearish_color, egui::Color32::RED);
        }
    }

    #[test]
    fn context_palette_changes_reach_charts_and_keep_host_edits() {
        let ctx = egui::Context::default();
        let mut shared = chart(&[100.0], &[110.0]);
        let mut built = crate::widget::ChartBuilder::new().build();
        let mut local = chart(&[100.0], &[110.0]);
        shared.config_mut().bearish_color = egui::Color32::RED;
        local.set_theme_override(Some(Theme::high_contrast()));
        for (step, theme) in [Theme::light(), Theme::dark(), Theme::high_contrast()]
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
            assert_eq!(shared.config().bearish_color, egui::Color32::RED);
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
}
