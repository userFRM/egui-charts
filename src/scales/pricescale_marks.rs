//! Smart price-axis tick mark generation.
//!
//! [`PriceMarkGenerator`] uses the "nice numbers" algorithm (Heckbert) to
//! place tick marks at visually pleasing, round price intervals.  It supports
//! both linear and logarithmic scales and automatically adjusts label
//! precision based on the step size.

/// Smart price axis mark generation.
/// Reference: lightweight-charts PriceFormatter.
use super::PriceScaleMode;

/// A single price tick mark on the Y axis
#[derive(Debug, Clone)]
pub struct PriceMark {
    /// Price value
    pub price: f64,

    /// Formatted label text
    pub label: String,

    /// Y coord in pixels
    pub y_coord: f32,

    /// Weight of this mark (for hierarchical rendering)
    pub weight: u8,
}

/// Configuration for price mark generation
#[derive(Debug, Clone)]
pub struct PriceMarkGeneratorConfig {
    /// Min spacing between marks in pixels
    pub min_spacing: f32,

    /// Max number of marks
    pub max_marks: usize,

    /// Target density (marks per 100 pixels)
    pub target_density: f32,

    /// Min price step (for crypto/forex precision)
    pub min_price_step: Option<f64>,
}

impl Default for PriceMarkGeneratorConfig {
    fn default() -> Self {
        Self {
            min_spacing: 30.0,
            max_marks: 20,
            target_density: 3.0, // Slightly denser than time axis
            min_price_step: None,
        }
    }
}

/// Smart price axis mark generator
/// Implements "nice numbers" algorithm for price ticks
pub struct PriceMarkGenerator {
    config: PriceMarkGeneratorConfig,
}

impl PriceMarkGenerator {
    /// Create a new price mark generator with default config
    pub fn new() -> Self {
        Self {
            config: PriceMarkGeneratorConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: PriceMarkGeneratorConfig) -> Self {
        Self { config }
    }

    /// Generate price marks for a given price range and display height
    pub fn generate_marks(
        &self,
        min_price: f64,
        max_price: f64,
        height_pixels: f32,
        scale_mode: PriceScaleMode,
        rect_min_y: f32,
        rect_max_y: f32,
    ) -> Vec<PriceMark> {
        if min_price >= max_price || height_pixels <= 0.0 {
            return Vec::new();
        }

        match scale_mode {
            PriceScaleMode::Normal => self.generate_linear_marks(
                min_price,
                max_price,
                height_pixels,
                rect_min_y,
                rect_max_y,
            ),
            PriceScaleMode::Logarithmic => {
                self.generate_log_marks(min_price, max_price, height_pixels, rect_min_y, rect_max_y)
            }
            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100 => {
                // For percentage mode, we still use linear spacing
                // but the formatting will be different (handled by formatter)
                self.generate_linear_marks(
                    min_price,
                    max_price,
                    height_pixels,
                    rect_min_y,
                    rect_max_y,
                )
            }
        }
    }

    /// Generate marks for linear (normal) price scale
    fn generate_linear_marks(
        &self,
        min_price: f64,
        max_price: f64,
        height_pixels: f32,
        rect_min_y: f32,
        rect_max_y: f32,
    ) -> Vec<PriceMark> {
        // Guard against invalid inputs (NaN, infinite, or invalid ranges)
        if !min_price.is_finite()
            || !max_price.is_finite()
            || !height_pixels.is_finite()
            || height_pixels <= 0.0
        {
            return Vec::new();
        }

        let price_range = max_price - min_price;

        // Guard against zero or very small price range
        if price_range <= f64::EPSILON * 1000.0 {
            return Vec::new();
        }

        // Calculate optimal number of marks
        let target_marks = (height_pixels / 100.0 * self.config.target_density)
            .max(3.0)
            .min(self.config.max_marks as f32);

        // Calculate raw step
        let raw_step = price_range / target_marks as f64;

        // Get "nice" step value
        let nice_step = self.calculate_nice_step(raw_step);

        // Guard against zero or invalid step (prevents infinite loop)
        if nice_step <= 0.0 || !nice_step.is_finite() {
            return Vec::new();
        }

        // Calculate starting price (round down to nice boundary)
        let start_price = (min_price / nice_step).floor() * nice_step;

        // Guard against invalid start price
        if !start_price.is_finite() {
            return Vec::new();
        }

        // Generate marks with iteration limit to prevent infinite loops
        let mut marks = Vec::new();
        let mut curr_price = start_price;
        let max_iterations = self.config.max_marks * 10; // Safety limit
        let mut iterations = 0;

        while curr_price <= max_price && iterations < max_iterations {
            if curr_price >= min_price {
                // Calculate Y coord using full rect bounds
                let ratio = (curr_price - min_price) / price_range;
                let y = rect_max_y - (ratio as f32 * (rect_max_y - rect_min_y));

                // Determine precision based on price value and step
                let precision = self.calculate_precision(curr_price, nice_step);
                let label = self.format_price(curr_price, precision);

                // Calculate weight (higher for rounder numbers)
                let weight = self.calculate_weight(curr_price, nice_step);

                marks.push(PriceMark {
                    price: curr_price,
                    label,
                    y_coord: y,
                    weight,
                });
            }

            curr_price += nice_step;
            iterations += 1;
        }

        // Apply spacing constraints
        self.apply_spacing_constraints(marks, height_pixels)
    }

    /// Generate marks for logarithmic price scale
    fn generate_log_marks(
        &self,
        min_price: f64,
        max_price: f64,
        height_pixels: f32,
        rect_min_y: f32,
        rect_max_y: f32,
    ) -> Vec<PriceMark> {
        // Guard against invalid inputs
        if min_price <= 0.0
            || max_price <= 0.0
            || !min_price.is_finite()
            || !max_price.is_finite()
            || !height_pixels.is_finite()
            || height_pixels <= 0.0
            || min_price >= max_price
        {
            return Vec::new();
        }

        let log_min = min_price.ln();
        let log_max = max_price.ln();
        let log_range = log_max - log_min;

        // Guard against invalid log values
        if !log_min.is_finite() || !log_max.is_finite() || log_range <= f64::EPSILON {
            return Vec::new();
        }

        // For log scale, we want prices at nice round numbers
        // Find the order of magnitude range
        let min_order = min_price.log10().floor() as i32;
        let max_order = max_price.log10().ceil() as i32;

        let mut marks = Vec::new();

        // Generate marks at powers of 10 and their subdivisions
        for order in min_order..=max_order {
            let base = 10f64.powi(order);

            // Major marks: 1x, 2x, 5x for each order of magnitude
            for multiplier in [1.0, 2.0, 5.0] {
                let price = base * multiplier;

                if price >= min_price && price <= max_price {
                    // Calculate Y using logarithmic scale with full rect bounds
                    let log_price = price.ln();
                    let ratio = (log_price - log_min) / log_range;
                    let y = rect_max_y - (ratio as f32 * (rect_max_y - rect_min_y));

                    let precision = self.calculate_precision(price, base);
                    let label = self.format_price(price, precision);

                    let weight = if multiplier == 1.0 {
                        100
                    } else if multiplier == 5.0 {
                        80
                    } else {
                        60
                    };

                    marks.push(PriceMark {
                        price,
                        label,
                        y_coord: y,
                        weight,
                    });
                }
            }
        }

        // Apply spacing constraints
        self.apply_spacing_constraints(marks, height_pixels)
    }

    /// Calculate "nice" step value using modified algorithm from Heckbert
    /// Returns values like 1, 2, 5, 10, 20, 50, 100, etc.
    fn calculate_nice_step(&self, raw_step: f64) -> f64 {
        if raw_step <= 0.0 {
            return 1.0;
        }

        // Check for min step constraint
        if let Some(min_step) = self.config.min_price_step
            && raw_step < min_step
        {
            return min_step;
        }

        // Get order of magnitude
        let exponent = raw_step.log10().floor();
        let fraction = raw_step / 10f64.powf(exponent);

        // Round fraction to nice value (1, 2, 5, or 10)
        // Using standard Heckbert thresholds
        let nice_fraction = if fraction <= 1.0 {
            1.0
        } else if fraction <= 2.0 {
            2.0
        } else if fraction <= 5.0 {
            5.0
        } else {
            10.0
        };

        nice_fraction * 10f64.powf(exponent)
    }

    /// Calculate appropriate decimal precision for a price value
    fn calculate_precision(&self, price: f64, step: f64) -> usize {
        if price == 0.0 {
            return 2;
        }

        // For very small steps, we need more precision
        if step <= 0.0001 {
            8
        } else if step <= 0.001 {
            6
        } else if step <= 0.01 {
            4
        } else if step <= 0.1 {
            3
        } else if step <= 1.0 {
            2
        } else if step <= 10.0 {
            1
        } else {
            0
        }
    }

    /// Format price with given precision
    fn format_price(&self, price: f64, precision: usize) -> String {
        format!("{price:.precision$}")
    }

    /// Calculate weight for a price mark (higher for rounder numbers)
    fn calculate_weight(&self, price: f64, step: f64) -> u8 {
        // Check if price is a multiple of larger steps
        let step_10 = step * 10.0;
        let step_5 = step * 5.0;
        let step_2 = step * 2.0;

        // Use a tolerance for floating point comparison
        let tolerance = step * 0.01;

        if (price % step_10).abs() < tolerance {
            100
        } else if (price % step_5).abs() < tolerance {
            80
        } else if (price % step_2).abs() < tolerance {
            60
        } else {
            40
        }
    }

    /// Apply min spacing constraints
    fn apply_spacing_constraints(
        &self,
        mut marks: Vec<PriceMark>,
        height_pixels: f32,
    ) -> Vec<PriceMark> {
        if marks.is_empty() {
            return marks;
        }

        let pixels_per_mark = height_pixels / marks.len() as f32;

        // If spacing is too tight, filter by weight
        if pixels_per_mark < self.config.min_spacing {
            // Sort by weight descending, then by price
            marks.sort_by(|a, b| {
                b.weight.cmp(&a.weight).then_with(|| {
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });

            // Keep marks with sufficient spacing
            let mut filtered = vec![marks[0].clone()];

            for mark in marks.iter().skip(1) {
                if filtered
                    .iter()
                    .all(|m| (m.y_coord - mark.y_coord).abs() >= self.config.min_spacing)
                {
                    filtered.push(mark.clone());
                }
            }

            // Sort back by price
            filtered.sort_by(|a, b| {
                a.price
                    .partial_cmp(&b.price)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            marks = filtered;
        }

        // Limit to max marks
        if marks.len() > self.config.max_marks {
            // Keep the highest weight marks
            marks.sort_by(|a, b| {
                b.weight.cmp(&a.weight).then_with(|| {
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            marks.truncate(self.config.max_marks);
            marks.sort_by(|a, b| {
                a.price
                    .partial_cmp(&b.price)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        marks
    }
}

impl Default for PriceMarkGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nice_step_calculation() {
        let generator = PriceMarkGenerator::new();

        // Test various raw steps
        assert_eq!(generator.calculate_nice_step(0.7), 1.0);
        assert_eq!(generator.calculate_nice_step(1.8), 2.0);
        assert_eq!(generator.calculate_nice_step(4.5), 5.0);
        assert_eq!(generator.calculate_nice_step(8.0), 10.0);
        assert_eq!(generator.calculate_nice_step(15.0), 20.0);
        assert_eq!(generator.calculate_nice_step(35.0), 50.0);
    }

    #[test]
    fn test_precision_calculation() {
        let generator = PriceMarkGenerator::new();

        assert_eq!(generator.calculate_precision(100.0, 0.00001), 8);
        assert_eq!(generator.calculate_precision(100.0, 0.001), 6);
        assert_eq!(generator.calculate_precision(100.0, 0.01), 4);
        assert_eq!(generator.calculate_precision(100.0, 0.1), 3);
        assert_eq!(generator.calculate_precision(100.0, 1.0), 2);
        assert_eq!(generator.calculate_precision(100.0, 10.0), 1);
        assert_eq!(generator.calculate_precision(100.0, 100.0), 0);
    }

    #[test]
    fn test_weight_calculation() {
        let generator = PriceMarkGenerator::new();
        let step = 1.0;

        assert_eq!(generator.calculate_weight(10.0, step), 100); // Multiple of 10
        assert_eq!(generator.calculate_weight(5.0, step), 80); // Multiple of 5
        assert_eq!(generator.calculate_weight(2.0, step), 60); // Multiple of 2
        assert_eq!(generator.calculate_weight(1.0, step), 40); // Base step
    }

    #[test]
    fn test_linear_marks_generation() {
        let generator = PriceMarkGenerator::new();
        let marks = generator.generate_linear_marks(100.0, 200.0, 500.0, 0.0, 500.0);

        assert!(!marks.is_empty());
        assert!(marks.len() <= 20); // Max marks constraint

        // Check marks are sorted by price
        for i in 1..marks.len() {
            assert!(marks[i].price > marks[i - 1].price);
        }

        // Check all marks are in range
        for mark in &marks {
            assert!(mark.price >= 100.0 && mark.price <= 200.0);
        }
    }

    #[test]
    fn test_log_marks_generation() {
        let generator = PriceMarkGenerator::new();
        let marks = generator.generate_log_marks(10.0, 1000.0, 500.0, 0.0, 500.0);

        assert!(!marks.is_empty());

        // Should have marks at powers of 10
        let has_10 = marks.iter().any(|m| (m.price - 10.0).abs() < 0.01);
        let has_100 = marks.iter().any(|m| (m.price - 100.0).abs() < 0.01);
        let has_1000 = marks.iter().any(|m| (m.price - 1000.0).abs() < 0.01);

        assert!(has_10 || has_100 || has_1000);
    }
}
