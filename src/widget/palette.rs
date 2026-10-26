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
                theme.apply_to_config(ChartConfig::default())
            });
        let new = theme.apply_to_config(ChartConfig::default());
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
    }
}
