//! Active palette propagation for chart configuration colours.

use super::Chart;
use crate::config::ChartConfig;
use crate::theme::Theme;

impl Chart {
    pub(super) fn follow_chart_theme(&mut self, theme: &Theme) {
        if self.inherited_theme.as_ref() == Some(theme) {
            return;
        }
        let old = self
            .inherited_theme
            .as_ref()
            .map_or_else(ChartConfig::default, |theme| {
                ChartConfig::default().with_theme(theme)
            });
        let new = ChartConfig::default().with_theme(theme);
        macro_rules! follow {
            ($($field:ident),+ $(,)?) => { $(if self.config.$field == old.$field { self.config.$field = new.$field; })+ };
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
        self.inherited_theme = Some(theme.clone());
    }
}

#[cfg(test)]
mod tests {
    use crate::theme::{Theme, apply_to_egui};
    use crate::widget::test_frames::{chart, frame};

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
