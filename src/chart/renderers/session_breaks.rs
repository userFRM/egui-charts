use super::context::RenderContext;
/// Session break rendering for charts
/// Displays vertical lines and labels for trading session boundaries
use crate::model::{Bar, SessionBreak, SessionBreakType, SessionProvider, find_session_breaks};
use crate::theme::Theme;
use egui::{Color32, FontId, Pos2, Rect, Stroke};

/// Configuration for session break rendering
#[derive(Debug, Clone)]
pub struct SessionBreakRenderConfig {
    /// Show vertical lines at session breaks
    pub show_lines: bool,
    /// Show labels for session breaks
    pub show_labels: bool,
    /// Line style for session breaks
    pub line_width: f32,
    /// Line color override (None uses theme default)
    pub line_color: Option<Color32>,
    /// Label font size
    pub label_font_size: f32,
    /// Show only major breaks (e.g., weekly/monthly, not daily)
    pub major_breaks_only: bool,
}

impl Default for SessionBreakRenderConfig {
    fn default() -> Self {
        Self {
            show_lines: true,
            show_labels: true,
            line_width: 1.0,
            line_color: None,
            label_font_size: 10.0,
            major_breaks_only: false,
        }
    }
}

/// Renderer for session breaks
pub struct SessionBreakRenderer<'a> {
    config: SessionBreakRenderConfig,
    theme: &'a Theme,
}

impl<'a> SessionBreakRenderer<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            config: SessionBreakRenderConfig::default(),
            theme,
        }
    }

    pub fn with_config(mut self, config: SessionBreakRenderConfig) -> Self {
        self.config = config;
        self
    }

    /// Render session breaks for visible data
    pub fn render(
        &self,
        context: &RenderContext,
        visible_data: &[Bar],
        provider: &dyn SessionProvider,
        bar_spacing: f32,
        origin_x: f32,
    ) {
        if !self.config.show_lines && !self.config.show_labels {
            return;
        }

        // Find all session breaks in visible data
        let breaks = find_session_breaks(visible_data, provider);

        for (bar_idx, session_break) in breaks {
            // Skip minor breaks if configured
            if self.config.major_breaks_only
                && matches!(session_break.break_type, SessionBreakType::Daily)
            {
                continue;
            }

            // Calculate x position for this break
            let x = origin_x + (bar_idx as f32 * bar_spacing);

            // Check if within visible chart area
            if x < context.rect.min.x || x > context.rect.max.x {
                continue;
            }

            // Determine color and style based on break type
            let (line_color, line_width, label_color) =
                self.get_style_for_break_type(&session_break.break_type);

            // Draw vertical line
            if self.config.show_lines {
                let from = Pos2::new(x, context.rect.min.y);
                let to = Pos2::new(x, context.rect.max.y);
                context
                    .painter
                    .line_segment([from, to], Stroke::new(line_width, line_color));
            }

            // Draw label
            if self.config.show_labels {
                if let Some(label) = &session_break.label {
                    let label_pos = Pos2::new(x + 3.0, context.rect.min.y + 5.0);
                    context.painter.text(
                        label_pos,
                        egui::Align2::LEFT_TOP,
                        label,
                        FontId::proportional(self.config.label_font_size),
                        label_color,
                    );
                } else {
                    // Default label based on break type
                    let default_label = self.get_default_label(&session_break);
                    let label_pos = Pos2::new(x + 3.0, context.rect.min.y + 5.0);
                    context.painter.text(
                        label_pos,
                        egui::Align2::LEFT_TOP,
                        default_label,
                        FontId::proportional(self.config.label_font_size),
                        label_color,
                    );
                }
            }
        }
    }

    fn get_style_for_break_type(&self, break_type: &SessionBreakType) -> (Color32, f32, Color32) {
        let base_color = self.config.line_color.unwrap_or(self.theme.grid());

        match break_type {
            SessionBreakType::Daily => {
                // Subtle gray for daily breaks
                let color = base_color.gamma_multiply(0.5);
                (color, self.config.line_width, color)
            }
            SessionBreakType::Weekly => {
                // Medium emphasis for weekly breaks
                let color = base_color.gamma_multiply(0.8);
                (color, self.config.line_width * 1.5, color)
            }
            SessionBreakType::Monthly => {
                // Strong emphasis for monthly breaks
                let color = base_color;
                (color, self.config.line_width * 2.0, color)
            }
            SessionBreakType::Custom => {
                // Custom breaks use theme color
                let color = self.theme.text().gamma_multiply(0.6);
                (color, self.config.line_width * 1.2, color)
            }
        }
    }

    fn get_default_label(&self, session_break: &SessionBreak) -> String {
        let date = session_break.ts.format("%Y-%m-%d").to_string();
        match session_break.break_type {
            SessionBreakType::Daily => format!("Day {date}"),
            SessionBreakType::Weekly => {
                format!("Week {}", session_break.ts.format("%Y-W%V"))
            }
            SessionBreakType::Monthly => format!("{}", session_break.ts.format("%B %Y")),
            SessionBreakType::Custom => date,
        }
    }
}

/// Helper for rendering session background shading (alternating colors)
pub struct SessionBackgroundRenderer {
    alternating_colors: (Color32, Color32),
}

impl SessionBackgroundRenderer {
    pub fn new(theme: &Theme) -> Self {
        let color1 = theme.background();
        let color2 = theme.background().gamma_multiply(0.95);

        Self {
            alternating_colors: (color1, color2),
        }
    }

    pub fn with_colors(mut self, color1: Color32, color2: Color32) -> Self {
        self.alternating_colors = (color1, color2);
        self
    }

    /// Render alternating session backgrounds
    pub fn render(
        &self,
        context: &RenderContext,
        visible_data: &[Bar],
        provider: &dyn SessionProvider,
        bar_spacing: f32,
        origin_x: f32,
    ) {
        let breaks = find_session_breaks(visible_data, provider);

        let mut is_even = true;
        let mut prev_x = context.rect.min.x;

        for (bar_idx, _session_break) in breaks {
            let x = origin_x + (bar_idx as f32 * bar_spacing);

            if x >= context.rect.min.x && x <= context.rect.max.x {
                // Fill region from prev_x to x
                let region_rect = Rect::from_min_max(
                    Pos2::new(prev_x.max(context.rect.min.x), context.rect.min.y),
                    Pos2::new(x, context.rect.max.y),
                );

                let color = if is_even {
                    self.alternating_colors.0
                } else {
                    self.alternating_colors.1
                };

                context.painter.rect_filled(region_rect, 0.0, color);
                is_even = !is_even;
            }

            prev_x = x;
        }

        // Fill remaining region
        if prev_x < context.rect.max.x {
            let region_rect =
                Rect::from_min_max(Pos2::new(prev_x, context.rect.min.y), context.rect.max);

            let color = if is_even {
                self.alternating_colors.0
            } else {
                self.alternating_colors.1
            };

            context.painter.rect_filled(region_rect, 0.0, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DailySessionProvider, MonthlySessionProvider};
    use chrono::{TimeZone, Utc};

    fn create_test_bars() -> Vec<Bar> {
        vec![
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
                open: 102.0,
                high: 108.0,
                low: 101.0,
                close: 107.0,
                volume: 1200.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
                open: 107.0,
                high: 110.0,
                low: 105.0,
                close: 109.0,
                volume: 1100.0,
            },
        ]
    }

    #[test]
    fn test_session_break_config_default() {
        let config = SessionBreakRenderConfig::default();
        assert!(config.show_lines);
        assert!(config.show_labels);
        assert_eq!(config.line_width, 1.0);
    }

    #[test]
    fn test_session_break_renderer_creation() {
        let theme = Theme::dark();
        let renderer = SessionBreakRenderer::new(&theme);
        assert!(renderer.config.show_lines);
    }

    #[test]
    fn test_session_background_renderer() {
        let theme = Theme::dark();
        let renderer = SessionBackgroundRenderer::new(&theme);
        assert_eq!(renderer.alternating_colors.0, theme.background());
    }

    #[test]
    fn test_find_breaks_with_daily_provider() {
        let bars = create_test_bars();
        let provider = DailySessionProvider::default();
        let breaks = find_session_breaks(&bars, &provider);

        // Should find break between Jan 1 and Jan 2, and between Jan 2 and Feb 1
        assert!(!breaks.is_empty());
    }

    #[test]
    fn test_find_breaks_with_monthly_provider() {
        let bars = create_test_bars();
        let provider = MonthlySessionProvider;
        let breaks = find_session_breaks(&bars, &provider);

        // Should find break between January and February
        assert!(!breaks.is_empty());
    }
}
