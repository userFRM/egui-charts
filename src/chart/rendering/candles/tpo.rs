//! # TPO (Time Price Opportunity) Chart Renderer
//!
//! Full Market Profile rendering with support for:
//! - TPO letters (A, B, C...) at each price level
//! - Point of Control (POC) highlighting
//! - Value Area shading (70% of volume)
//! - Initial Balance range
//! - Single prints highlighting
//! - Session separators
//! - Multiple color modes and display modes
//!
//! # Example
//!
//! ```ignore
//! use egui_charts::model::{TPOConfig, TPOProfile, to_tpo_profiles};
//! use egui_charts::chart::rendering::candles::tpo::TpoRenderer;
//!
//! let config = TPOConfig::default();
//! let profiles = to_tpo_profiles(&bars, &config);
//!
//! let renderer = TpoRenderer::new(config);
//! renderer.render(painter, rect, &profiles, &price_scale, colors);
//! ```

use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use crate::model::{TPOColorMode, TPOConfig, TPODisplayMode, TPOProfile};
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;

/// TPO rendering configuration (visual settings)
#[derive(Debug, Clone)]
pub struct TpoRenderConfig {
    /// Font size for TPO letters
    pub letter_font_size: f32,
    /// Width of each TPO column in pixels
    pub column_width: f32,
    /// Height of each price row in pixels
    pub row_height: f32,
    /// POC line color
    pub poc_color: Color32,
    /// POC line width
    pub poc_line_width: f32,
    /// Value Area background color (with alpha)
    pub value_area_color: Color32,
    /// Initial Balance bracket color
    pub initial_balance_color: Color32,
    /// Single prints highlight color
    pub single_print_color: Color32,
    /// Session separator color
    pub session_separator_color: Color32,
    /// Default letter color
    pub default_letter_color: Color32,
    /// Color palette for period-based coloring
    pub period_colors: Vec<Color32>,
    /// Opening range marker color
    pub opening_range_color: Color32,
    /// Show grid lines between price levels
    pub show_grid: bool,
    /// Grid line color
    pub grid_color: Color32,
    /// Letter spacing (fraction of column width)
    pub letter_spacing: f32,
}

impl Default for TpoRenderConfig {
    fn default() -> Self {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        Self {
            letter_font_size: 10.0,
            column_width: 12.0,
            row_height: 14.0,
            poc_color: tpo.poc,
            poc_line_width: 2.0,
            value_area_color: tpo.value_area,
            initial_balance_color: tpo.initial_balance,
            single_print_color: tpo.single_print,
            session_separator_color: tpo.session_separator,
            default_letter_color: tpo.letter_default,
            period_colors: Self::default_period_colors(),
            opening_range_color: tpo.opening_range,
            show_grid: true,
            grid_color: tpo.grid,
            letter_spacing: 0.1,
        }
    }
}

impl TpoRenderConfig {
    /// Default color palette for period-based coloring
    fn default_period_colors() -> Vec<Color32> {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        vec![
            tpo.period_1,  // Red
            tpo.period_2,  // Orange
            tpo.period_3,  // Yellow
            tpo.period_4,  // Green
            tpo.period_5,  // Blue
            tpo.period_6,  // Deep Purple
            tpo.period_7,  // Pink
            tpo.period_8,  // Cyan
            tpo.period_9,  // Light Green
            tpo.period_10, // Brown
            tpo.period_11, // Blue Grey
            tpo.period_12, // Deep Orange
        ]
    }

    /// Get color for a specific period index
    pub fn color_for_period(&self, period_idx: usize) -> Color32 {
        self.period_colors[period_idx % self.period_colors.len()]
    }
}

/// TPO Chart Renderer
///
/// Renders TPO profiles with full Market Profile visualization
pub struct TpoRenderer {
    /// TPO calculation config
    pub tpo_config: TPOConfig,
    /// Visual rendering config
    pub render_config: TpoRenderConfig,
}

impl TpoRenderer {
    /// Create a new TPO renderer with default visual settings
    pub fn new(tpo_config: TPOConfig) -> Self {
        Self {
            tpo_config,
            render_config: TpoRenderConfig::default(),
        }
    }

    /// Create with custom render configuration
    pub fn with_render_config(tpo_config: TPOConfig, render_config: TpoRenderConfig) -> Self {
        Self {
            tpo_config,
            render_config,
        }
    }

    /// Render a single TPO profile
    pub fn render_profile(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        min_price: f64,
        max_price: f64,
        bullish_color: Color32,
        bearish_color: Color32,
    ) {
        if profile.letters.is_empty() {
            return;
        }

        let tick_size = self.tpo_config.tick_size;
        let price_range = max_price - min_price;
        if price_range <= 0.0 {
            return;
        }

        // Calculate price to y coordinate
        let price_to_y = |price: f64| -> f32 {
            let normalized = (price - min_price) / price_range;
            rect.bottom() - (normalized as f32 * rect.height())
        };

        // Step 1: Draw Value Area background
        if self.tpo_config.show_value_area {
            self.render_value_area(painter, rect, profile, price_to_y);
        }

        // Step 2: Draw grid lines (if enabled)
        if self.render_config.show_grid {
            self.render_grid(painter, rect, profile, tick_size, price_to_y);
        }

        // Step 3: Draw TPO letters/blocks
        self.render_tpo_letters(
            painter,
            rect,
            profile,
            tick_size,
            price_to_y,
            bullish_color,
            bearish_color,
        );

        // Step 4: Draw POC line
        if self.tpo_config.show_poc {
            self.render_poc_line(painter, rect, profile, price_to_y);
        }

        // Step 5: Draw Initial Balance brackets
        if self.tpo_config.show_initial_balance {
            self.render_initial_balance(painter, rect, profile, price_to_y);
        }

        // Step 6: Highlight single prints
        if self.tpo_config.show_single_prints {
            self.render_single_prints(painter, rect, profile, tick_size, price_to_y);
        }

        // Step 7: Show opening range marker
        if self.tpo_config.show_opening_range {
            self.render_opening_range(painter, rect, profile, price_to_y);
        }
    }

    /// Render multiple TPO profiles side by side
    pub fn render_profiles(
        &self,
        painter: &Painter,
        rect: Rect,
        profiles: &[TPOProfile],
        min_price: f64,
        max_price: f64,
        bullish_color: Color32,
        bearish_color: Color32,
    ) {
        if profiles.is_empty() {
            return;
        }

        // Calculate total width needed
        let total_periods: usize = profiles.iter().map(|p| p.width()).sum();
        let profile_widths: Vec<usize> = profiles.iter().map(|p| p.width()).collect();

        let column_width = if total_periods > 0 {
            (rect.width() / total_periods as f32).max(self.render_config.column_width)
        } else {
            self.render_config.column_width
        };

        let mut x_offset = 0.0;

        for (profile_idx, profile) in profiles.iter().enumerate() {
            let profile_width = profile_widths[profile_idx] as f32 * column_width;

            // Create sub-rect for this profile
            let profile_rect = Rect::from_min_size(
                Pos2::new(rect.left() + x_offset, rect.top()),
                egui::vec2(profile_width, rect.height()),
            );

            // Render the profile
            self.render_profile(
                painter,
                profile_rect,
                profile,
                min_price,
                max_price,
                bullish_color,
                bearish_color,
            );

            // Draw session separator if not the last profile
            if profile_idx < profiles.len() - 1 {
                let separator_x = rect.left() + x_offset + profile_width;
                painter.line_segment(
                    [
                        Pos2::new(separator_x, rect.top()),
                        Pos2::new(separator_x, rect.bottom()),
                    ],
                    Stroke::new(
                        DESIGN_TOKENS.stroke.hairline,
                        self.render_config.session_separator_color,
                    ),
                );
            }

            x_offset += profile_width;
        }
    }

    /// Render value area background shading
    fn render_value_area(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let va_top = price_to_y(profile.value_area_high);
        let va_bottom = price_to_y(profile.value_area_low);

        let va_rect = Rect::from_min_max(
            Pos2::new(rect.left(), va_top),
            Pos2::new(rect.right(), va_bottom),
        );

        painter.rect_filled(va_rect, 0.0, self.render_config.value_area_color);
    }

    /// Render horizontal grid lines at each price level
    fn render_grid(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        tick_size: f64,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let levels = profile.price_levels(tick_size);

        for price in levels {
            let y = price_to_y(price);
            if y >= rect.top() && y <= rect.bottom() {
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    Stroke::new(DESIGN_TOKENS.stroke.hairline, self.render_config.grid_color),
                );
            }
        }
    }

    /// Render TPO letters or blocks
    fn render_tpo_letters(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        tick_size: f64,
        price_to_y: impl Fn(f64) -> f32,
        _bullish_color: Color32,
        _bearish_color: Color32,
    ) {
        // Group letters by price level
        let price_levels = profile.price_levels(tick_size);

        // Calculate column width based on profile width
        let profile_width = profile.width();
        let column_width = if profile_width > 0 {
            (rect.width() / profile_width as f32).max(self.render_config.column_width * 0.5)
        } else {
            self.render_config.column_width
        };

        for price in price_levels {
            let y = price_to_y(price);
            if y < rect.top() || y > rect.bottom() {
                continue;
            }

            // Get letters at this price level, sorted by period
            let mut letters_at_price: Vec<_> = profile.letters_at(price, tick_size);
            letters_at_price.sort_by_key(|l| l.period_idx);

            // Track which columns are used at this price
            let mut used_columns: std::collections::HashSet<usize> =
                std::collections::HashSet::new();

            for letter in letters_at_price {
                // Skip if we already have a letter at this column
                if used_columns.contains(&letter.period_idx) {
                    continue;
                }
                used_columns.insert(letter.period_idx);

                let x =
                    rect.left() + (letter.period_idx as f32 * column_width) + (column_width * 0.5);

                // Determine letter color based on color mode
                let color = self.get_letter_color(letter.period_idx, price, profile);

                match self.tpo_config.display_mode {
                    TPODisplayMode::Letters => {
                        // Draw the letter
                        painter.text(
                            Pos2::new(x, y),
                            egui::Align2::CENTER_CENTER,
                            letter.letter.to_string(),
                            FontId::monospace(self.render_config.letter_font_size),
                            color,
                        );
                    }
                    TPODisplayMode::Blocks => {
                        // Draw a filled block
                        let block_rect = Rect::from_center_size(
                            Pos2::new(x, y),
                            egui::vec2(column_width * 0.8, self.render_config.row_height * 0.8),
                        );
                        painter.rect_filled(block_rect, 2.0, color.gamma_multiply(0.8));
                    }
                    TPODisplayMode::Both => {
                        // Draw block background
                        let block_rect = Rect::from_center_size(
                            Pos2::new(x, y),
                            egui::vec2(column_width * 0.9, self.render_config.row_height * 0.9),
                        );
                        painter.rect_filled(block_rect, 2.0, color.gamma_multiply(0.3));

                        // Draw letter on top
                        painter.text(
                            Pos2::new(x, y),
                            egui::Align2::CENTER_CENTER,
                            letter.letter.to_string(),
                            FontId::monospace(self.render_config.letter_font_size),
                            color,
                        );
                    }
                }
            }
        }
    }

    /// Get color for a TPO letter based on color mode
    fn get_letter_color(&self, period_idx: usize, price: f64, profile: &TPOProfile) -> Color32 {
        match self.tpo_config.color_mode {
            TPOColorMode::ByPeriod => self.render_config.color_for_period(period_idx),
            TPOColorMode::Solid => self.render_config.default_letter_color,
            TPOColorMode::ByValueArea => {
                if profile.is_in_value_area(price) {
                    // Use value_area color but make it solid (no alpha)
                    let va = self.render_config.value_area_color;
                    Color32::from_rgb(va.r(), va.g(), va.b())
                } else {
                    self.render_config.default_letter_color.gamma_multiply(0.5)
                }
            }
            TPOColorMode::ByInitialBalance => {
                if profile.is_in_initial_balance(price) {
                    self.render_config.initial_balance_color
                } else {
                    self.render_config.default_letter_color
                }
            }
        }
    }

    /// Render POC (Point of Control) line
    fn render_poc_line(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let poc_y = price_to_y(profile.poc_price);

        if poc_y >= rect.top() && poc_y <= rect.bottom() {
            // Draw POC line spanning full width
            painter.line_segment(
                [
                    Pos2::new(rect.left(), poc_y),
                    Pos2::new(rect.right(), poc_y),
                ],
                Stroke::new(
                    self.render_config.poc_line_width,
                    self.render_config.poc_color,
                ),
            );

            // Draw POC label
            let label_text = format!("POC {:.2}", profile.poc_price);
            painter.text(
                Pos2::new(rect.right() - 5.0, poc_y - 8.0),
                egui::Align2::RIGHT_BOTTOM,
                label_text,
                FontId::proportional(typography::TINY),
                self.render_config.poc_color,
            );
        }
    }

    /// Render Initial Balance brackets
    fn render_initial_balance(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let ib_top = price_to_y(profile.initial_balance_high);
        let ib_bottom = price_to_y(profile.initial_balance_low);

        // Clamp to visible area
        let ib_top_clamped = ib_top.max(rect.top());
        let ib_bottom_clamped = ib_bottom.min(rect.bottom());

        if ib_top_clamped > ib_bottom_clamped {
            return;
        }

        let ib_color = self.render_config.initial_balance_color;
        let bracket_width = 8.0;

        // Draw left bracket
        let left_x = rect.left();
        painter.add(Shape::line(
            vec![
                Pos2::new(left_x + bracket_width, ib_top_clamped),
                Pos2::new(left_x, ib_top_clamped),
                Pos2::new(left_x, ib_bottom_clamped),
                Pos2::new(left_x + bracket_width, ib_bottom_clamped),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.thick, ib_color),
        ));

        // Draw right bracket
        let right_x = rect.right();
        painter.add(Shape::line(
            vec![
                Pos2::new(right_x - bracket_width, ib_top_clamped),
                Pos2::new(right_x, ib_top_clamped),
                Pos2::new(right_x, ib_bottom_clamped),
                Pos2::new(right_x - bracket_width, ib_bottom_clamped),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.thick, ib_color),
        ));

        // Draw IB label
        painter.text(
            Pos2::new(rect.left() + 2.0, ib_top_clamped + 2.0),
            egui::Align2::LEFT_TOP,
            "IB",
            FontId::proportional(typography::MICRO),
            ib_color,
        );
    }

    /// Render single prints (price levels with only 1 TPO)
    fn render_single_prints(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        tick_size: f64,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        for &price in &profile.single_prints {
            let y = price_to_y(price);
            if y < rect.top() || y > rect.bottom() {
                continue;
            }

            // Get the TPO count at this price to find the column
            let letters_at = profile.letters_at(price, tick_size);
            if let Some(letter) = letters_at.first() {
                let profile_width = profile.width();
                let column_width = if profile_width > 0 {
                    rect.width() / profile_width as f32
                } else {
                    self.render_config.column_width
                };

                let x =
                    rect.left() + (letter.period_idx as f32 * column_width) + (column_width * 0.5);

                // Draw highlight behind the single print
                let highlight_rect = Rect::from_center_size(
                    Pos2::new(x, y),
                    egui::vec2(column_width, self.render_config.row_height),
                );
                painter.rect_filled(highlight_rect, 0.0, self.render_config.single_print_color);
            }
        }
    }

    /// Render opening range marker
    fn render_opening_range(
        &self,
        painter: &Painter,
        rect: Rect,
        profile: &TPOProfile,
        price_to_y: impl Fn(f64) -> f32,
    ) {
        let open_y = price_to_y(profile.opening_price);

        if open_y >= rect.top() && open_y <= rect.bottom() {
            // Draw opening price marker (triangle on left side)
            let marker_size = 6.0;
            let triangle = vec![
                Pos2::new(rect.left(), open_y),
                Pos2::new(rect.left() + marker_size, open_y - marker_size * 0.5),
                Pos2::new(rect.left() + marker_size, open_y + marker_size * 0.5),
            ];
            painter.add(Shape::convex_polygon(
                triangle,
                self.render_config.opening_range_color,
                Stroke::NONE,
            ));
        }
    }

    /// Render profile statistics overlay
    pub fn render_stats_overlay(&self, painter: &Painter, rect: Rect, profile: &TPOProfile) {
        let text_color = Color32::WHITE;
        let bg_color = Color32::from_rgba_unmultiplied(0, 0, 0, 180);
        let padding = 4.0;
        let line_height = 12.0;

        let stats = [
            format!("POC: {:.2}", profile.poc_price),
            format!("VAH: {:.2}", profile.value_area_high),
            format!("VAL: {:.2}", profile.value_area_low),
            format!("IBH: {:.2}", profile.initial_balance_high),
            format!("IBL: {:.2}", profile.initial_balance_low),
            format!("High: {:.2}", profile.profile_high),
            format!("Low: {:.2}", profile.profile_low),
            format!("Width: {} periods", profile.width()),
        ];

        let box_height = stats.len() as f32 * line_height + padding * 2.0;
        let box_width = 100.0;

        let stats_rect = Rect::from_min_size(
            Pos2::new(rect.right() - box_width - padding, rect.top() + padding),
            egui::vec2(box_width, box_height),
        );

        // Draw background
        painter.rect_filled(stats_rect, 4.0, bg_color);

        // Draw stats
        for (idx, stat) in stats.iter().enumerate() {
            painter.text(
                Pos2::new(
                    stats_rect.left() + padding,
                    stats_rect.top() + padding + idx as f32 * line_height,
                ),
                egui::Align2::LEFT_TOP,
                stat,
                FontId::monospace(9.0),
                text_color,
            );
        }
    }
}

/// Render TPO profile using the standard chart rendering interface
///
/// This function integrates with the existing render_chart_type flow
pub fn render_tpo_profile(
    painter: &Painter,
    price_rect: Rect,
    profiles: &[TPOProfile],
    min_price: f64,
    max_price: f64,
    config: &TPOConfig,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let renderer = TpoRenderer::new(config.clone());
    renderer.render_profiles(
        painter,
        price_rect,
        profiles,
        min_price,
        max_price,
        bullish_color,
        bearish_color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, to_tpo_profiles};
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        vec![
            Bar {
                time: start,
                open: 100.0,
                high: 105.0,
                low: 98.0,
                close: 103.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(30),
                open: 103.0,
                high: 107.0,
                low: 101.0,
                close: 106.0,
                volume: 1200.0,
            },
            Bar {
                time: start + Duration::minutes(60),
                open: 106.0,
                high: 108.0,
                low: 104.0,
                close: 105.0,
                volume: 800.0,
            },
        ]
    }

    #[test]
    fn test_tpo_renderer_creation() {
        let config = TPOConfig::default();
        let renderer = TpoRenderer::new(config);

        assert_eq!(renderer.render_config.letter_font_size, 10.0);
        assert!(renderer.render_config.period_colors.len() >= 12);
    }

    #[test]
    fn test_period_color_cycling() {
        let config = TpoRenderConfig::default();
        let num_colors = config.period_colors.len();

        // Should cycle through colors
        let color_0 = config.color_for_period(0);
        let color_wrap = config.color_for_period(num_colors);

        assert_eq!(color_0, color_wrap);
    }

    #[test]
    fn test_tpo_profile_generation() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            period_minutes: 30,
            ..Default::default()
        };

        let profiles = to_tpo_profiles(&bars, &config);
        assert!(!profiles.is_empty());

        let profile = &profiles[0];
        assert!(profile.poc_price > 0.0);
        assert!(profile.value_area_high >= profile.value_area_low);
    }

    #[test]
    fn test_render_config_defaults() {
        let config = TpoRenderConfig::default();

        assert!(config.poc_line_width > 0.0);
        assert!(config.letter_font_size > 0.0);
        assert!(config.row_height > 0.0);
    }
}
