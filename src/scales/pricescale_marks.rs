//! Smart price-axis tick mark generation.
//!
//! [`PriceMarkGenerator`] uses the "nice numbers" algorithm (Heckbert) to
//! place tick marks at visually pleasing, round price intervals.  It supports
//! both linear and logarithmic scales and automatically adjusts label
//! precision based on the step size.

use super::{PriceMap, PriceScaleMode};

/// A single price tick mark on the Y axis
#[derive(Debug, Clone)]
pub struct PriceMark {
    /// Axis value: the price in normal and logarithmic mode, the percentage or
    /// index value otherwise
    pub price: f64,

    /// Formatted label text
    pub label: String,

    /// Y coord in pixels
    pub y_coord: f32,

    /// Weight of this mark (for hierarchical rendering)
    pub weight: u8,
}

/// Configuration for price mark generation, set per scale through
/// [`PriceScaleOptions::tick_marks`](super::PriceScaleOptions::tick_marks).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceMarkGeneratorConfig {
    /// Min spacing between marks in pixels, which keeps labels from overlapping at any density
    pub min_spacing: f32,

    /// Max number of marks, capped at 1,000 during generation.
    pub max_marks: usize,

    /// Target density (marks per 100 pixels): marks stay at least `100 / target_density`
    /// pixels apart, and no closer than `min_spacing`
    pub target_density: f32,

    /// Minimum price tick size. Normal and logarithmic marks lie on its multiples;
    /// invalid sizes are ignored.
    pub min_price_step: Option<f64>,
}

impl PriceMarkGeneratorConfig {
    pub(crate) const MAX_MARKS: usize = 1_000;
}

impl Default for PriceMarkGeneratorConfig {
    fn default() -> Self {
        Self {
            min_spacing: 20.0,
            max_marks: 100,
            target_density: 2.5,
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
    pub fn with_config(mut config: PriceMarkGeneratorConfig) -> Self {
        config.max_marks = config.max_marks.min(PriceMarkGeneratorConfig::MAX_MARKS);
        Self { config }
    }

    /// Generates marks for the visible range of `map`, positioned through it.
    ///
    /// Logarithmic scales get marks at 1, 2 and 5 times powers of ten; the other
    /// modes get evenly stepped marks in scale units, labelled as prices, signed
    /// percentages or index values.
    pub fn generate_marks(&self, map: &PriceMap) -> Vec<PriceMark> {
        let range = map.range();
        let (top, bottom) = map.extent();
        let height_pixels = bottom - top;
        if range.min >= range.max || height_pixels <= 0.0 {
            return Vec::new();
        }

        let transform = map.transform();
        if transform.mode != PriceScaleMode::Logarithmic {
            return self.generate_linear_marks(range.min, range.max, height_pixels, map, false);
        }
        let (min_price, max_price) = (transform.to_price(range.min), transform.to_price(range.max));
        let marks = self.generate_log_marks(min_price, max_price, height_pixels, map);
        if marks.len() >= 3 {
            return marks;
        }
        // A narrow log range holds few multiples of powers of ten: step it in prices instead.
        self.generate_linear_marks(min_price, max_price, height_pixels, map, true)
    }

    /// Generate evenly stepped marks between two scale values, or between two
    /// prices when `prices` is set
    fn generate_linear_marks(
        &self,
        min_price: f64,
        max_price: f64,
        height_pixels: f32,
        map: &PriceMap,
        prices: bool,
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

        // An absolute epsilon would discard representable sub-unit ranges.
        if price_range <= 0.0 {
            return Vec::new();
        }

        // Calculate optimal number of marks
        let target_marks = (height_pixels / 100.0 * self.config.target_density)
            .max(3.0)
            .min(self.config.max_marks as f32);

        // Calculate raw step
        let raw_step = price_range / target_marks as f64;

        // Get "nice" step value
        let mut nice_step = self.calculate_nice_step(raw_step);
        if let Some(decimals) = map.price_precision() {
            let unit = 10f64.powi(-(decimals as i32));
            nice_step = Self::round_up_to_multiple(nice_step, unit);
        }
        let min_step = self
            .min_step()
            .filter(|_| prices || map.transform().mode == PriceScaleMode::Normal);
        if let Some(min_step) = min_step {
            nice_step = Self::round_up_to_multiple(nice_step, min_step);
        }

        // Guard against zero or invalid step (prevents infinite loop)
        if nice_step <= 0.0 || !nice_step.is_finite() {
            return Vec::new();
        }

        // Calculate starting price (round down to nice boundary)
        let start_index = (min_price / nice_step).floor();
        let start_price = start_index * nice_step;

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
                let y = if prices {
                    map.price_to_y(curr_price)
                } else {
                    map.scale_to_y(curr_price)
                };

                // Determine precision based on price value and step
                let precision = map.price_precision().unwrap_or_else(|| {
                    let decimals = self.calculate_precision(nice_step);
                    if matches!(
                        map.transform().mode,
                        PriceScaleMode::Normal | PriceScaleMode::Logarithmic
                    ) {
                        decimals.max(2)
                    } else {
                        decimals
                    }
                });
                let label = if map.price_precision().is_none() && precision > 8 {
                    let mut label = self.format_price(curr_price, precision, true);
                    if map.transform().mode == PriceScaleMode::Percentage {
                        if !label.starts_with('-') {
                            label.insert(0, '+');
                        }
                        label.push('%');
                    }
                    label
                } else if prices {
                    self.format_price(curr_price, precision, false)
                } else {
                    map.transform().label(curr_price, precision, None)
                };

                // Calculate weight (higher for rounder numbers)
                let weight = self.calculate_weight(curr_price, nice_step);

                marks.push(PriceMark {
                    price: curr_price,
                    label,
                    y_coord: y,
                    weight,
                });
            }

            iterations += 1;
            curr_price = if min_step.is_some() {
                (start_index + iterations as f64) * nice_step
            } else {
                curr_price + nice_step
            };
        }

        // Apply spacing constraints
        self.apply_spacing_constraints(marks)
    }

    /// Generate marks for logarithmic price scale
    fn generate_log_marks(
        &self,
        min_price: f64,
        max_price: f64,
        height_pixels: f32,
        map: &PriceMap,
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
        let mut min_order = min_price.log10().floor() as i32;
        if let Some(decimals) = map.price_precision() {
            min_order = min_order.max(-(decimals as i32));
        }
        let max_order = max_price.log10().ceil() as i32;

        let mut marks = Vec::new();

        // Generate marks at powers of 10 and their subdivisions
        for order in min_order..=max_order {
            let base = 10f64.powi(order);

            // Major marks: 1x, 2x, 5x for each order of magnitude
            for multiplier in [1.0, 2.0, 5.0] {
                let price = base * multiplier;

                if let Some(step) = self.min_step()
                    && (price - (price / step).round() * step).abs()
                        > price.abs().max(step) * f64::EPSILON * 4.0
                {
                    continue;
                }

                if price >= min_price && price <= max_price {
                    let y = map.price_to_y(price);

                    let precision = map
                        .price_precision()
                        .unwrap_or_else(|| self.calculate_precision(base).max(2));
                    let label =
                        self.format_price(price, precision, map.price_precision().is_none());

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
        self.apply_spacing_constraints(marks)
    }

    fn round_up_to_multiple(step: f64, unit: f64) -> f64 {
        let ratio = step / unit;
        let rounded = ratio.round();
        let multiple = if (ratio - rounded).abs() <= ratio * 1e-9 {
            rounded
        } else {
            ratio.ceil()
        };
        multiple.max(1.0) * unit
    }

    /// Calculate "nice" step value using modified algorithm from Heckbert
    /// Returns values like 1, 2, 2.5, 4, 5, 10, 20, 25, 40, 50, etc.
    fn calculate_nice_step(&self, raw_step: f64) -> f64 {
        if raw_step <= 0.0 {
            return 1.0;
        }

        if let Some(min_step) = self.min_step()
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
        } else if fraction <= 2.5 {
            2.5
        } else if fraction <= 4.0 {
            4.0
        } else if fraction <= 5.0 {
            5.0
        } else {
            10.0
        };

        nice_fraction * 10f64.powf(exponent)
    }

    /// Calculate the decimal precision for marks `step` apart, the same for every
    /// mark including zero
    fn calculate_precision(&self, step: f64) -> usize {
        // For very small steps, we need more precision
        if step <= 0.0001 {
            (1.0 - step.log10().floor()).max(8.0) as usize
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

    /// The configured minimum tick size, unless it is not a positive finite number.
    fn min_step(&self) -> Option<f64> {
        self.config
            .min_price_step
            .filter(|s| s.is_finite() && *s > 0.0)
    }

    /// Format price with given precision
    fn format_price(&self, price: f64, precision: usize, automatic: bool) -> String {
        if automatic && precision > 8 {
            let price = if price == 0.0 { 0.0 } else { price };
            let digits = (price.abs().log10().floor() + precision as f64).clamp(0.0, 17.0) as usize;
            format!("{price:.digits$e}")
        } else {
            format!("{price:.precision$}")
        }
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
    fn apply_spacing_constraints(&self, mut marks: Vec<PriceMark>) -> Vec<PriceMark> {
        if marks.is_empty() {
            return marks;
        }

        // Logarithmic ticks can be closer at one end even when the average spacing fits.
        // Sort by weight descending, then by price
        marks.sort_by(|a, b| {
            b.weight
                .cmp(&a.weight)
                .then_with(|| a.price.total_cmp(&b.price))
        });

        // Keep marks with sufficient spacing
        let spacing = self
            .config
            .min_spacing
            .max(100.0 / self.config.target_density);
        let mut filtered = vec![marks[0].clone()];

        for mark in marks.iter().skip(1) {
            if filtered
                .iter()
                .all(|m| (m.y_coord - mark.y_coord).abs() >= spacing)
            {
                filtered.push(mark.clone());
            }
        }

        // Sort back by price
        filtered.sort_by(|a, b| a.price.total_cmp(&b.price));
        marks = filtered;

        // Limit to max marks
        if marks.len() > self.config.max_marks {
            // Keep the highest weight marks
            marks.sort_by(|a, b| {
                b.weight
                    .cmp(&a.weight)
                    .then_with(|| a.price.total_cmp(&b.price))
            });
            marks.truncate(self.config.max_marks);
            marks.sort_by(|a, b| a.price.total_cmp(&b.price));
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
        assert_eq!(generator.calculate_nice_step(2.2), 2.5);
        assert_eq!(generator.calculate_nice_step(3.6), 4.0);
        assert_eq!(generator.calculate_nice_step(4.5), 5.0);
        assert_eq!(generator.calculate_nice_step(8.0), 10.0);
        assert_eq!(generator.calculate_nice_step(15.0), 20.0);
        assert_eq!(generator.calculate_nice_step(35.0), 40.0);
    }

    #[test]
    fn test_precision_calculation() {
        let generator = PriceMarkGenerator::new();

        assert_eq!(generator.calculate_precision(0.00001), 8);
        assert_eq!(generator.calculate_precision(0.001), 6);
        assert_eq!(generator.calculate_precision(0.01), 4);
        assert_eq!(generator.calculate_precision(0.1), 3);
        assert_eq!(generator.calculate_precision(1.0), 2);
        assert_eq!(generator.calculate_precision(10.0), 1);
        assert_eq!(generator.calculate_precision(100.0), 0);
    }

    #[test]
    fn ticks_stay_on_the_minimum_price_grid() {
        use PriceScaleMode::{Logarithmic, Normal};
        for (mode, min, max, step, precision, expected) in [
            (Normal, 4500.0, 4503.75, 0.25, None, None),
            (Normal, -3.75, 0.5, 0.25, Some(2), None),
            (Normal, 4500.0, 4503.75, 0.05, None, None),
            (Normal, 0.01, 0.9, 0.03, Some(2), None),
            (Logarithmic, 4500.0, 4503.75, 0.25, Some(2), None),
            (Logarithmic, 0.01, 1.0, 0.03, None, None),
            (Normal, 0.012, 0.013, 1e-6, None, Some("0.01210000")),
        ] {
            let (min, max) = if mode == Logarithmic {
                (f64::ln(min), f64::ln(max))
            } else {
                (min, max)
            };
            let marks = map(mode, min, max)
                .with_price_precision(precision)
                .with_tick_marks(PriceMarkGeneratorConfig {
                    min_price_step: Some(step),
                    ..Default::default()
                })
                .marks();
            assert!(marks.len() >= 3, "{mode:?}: {marks:?}");
            if let Some(expected) = expected {
                assert!(
                    marks.iter().any(|mark| mark.label == expected),
                    "{mode:?}: expected {expected}, got {marks:?}"
                );
            }
            for mark in marks {
                let ticks = mark.price / step;
                assert!(
                    (ticks - ticks.round()).abs() < 1e-9,
                    "{mode:?}: {} is off the {step} grid",
                    mark.label
                );
            }
        }
    }

    #[test]
    fn percentage_and_indexed_ticks_keep_their_own_units() {
        use PriceScaleMode::{IndexedTo100, Percentage};
        for (mode, min, max, expected) in [
            (Percentage, -2.0, 2.375, "-1.60%"),
            (IndexedTo100, 98.0, 102.375, "98.40"),
        ] {
            let marks = map(mode, min, max)
                .with_tick_marks(PriceMarkGeneratorConfig {
                    min_price_step: Some(0.25),
                    ..Default::default()
                })
                .marks();
            assert!(
                marks.iter().any(|mark| mark.label == expected),
                "{mode:?}: expected {expected}, got {marks:?}"
            );
        }
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

    fn map(mode: PriceScaleMode, min: f64, max: f64) -> PriceMap {
        let transform = crate::scales::PriceTransform { mode, base: 0.0 };
        PriceMap::new(
            transform,
            crate::scales::PriceRange::new(min, max),
            0.0,
            500.0,
            false,
        )
    }

    #[test]
    fn test_linear_marks_generation() {
        let generator = PriceMarkGenerator::new();
        let marks = generator.generate_marks(&map(PriceScaleMode::Normal, 100.0, 200.0));
        for max_marks in [1_000, 1_001, usize::MAX] {
            let bounded = PriceMarkGenerator::with_config(PriceMarkGeneratorConfig {
                max_marks,
                min_spacing: 0.0,
                target_density: 100.0,
                ..Default::default()
            });
            assert_eq!(
                bounded
                    .generate_marks(
                        &map(PriceScaleMode::Normal, 0.0, 2_000.0).with_bounds(0.0, 2_000.0)
                    )
                    .len(),
                1_000
            );
        }

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

        for (mode, min, max, precision, expected) in [
            (PriceScaleMode::Normal, 18_200.0, 22_800.0, None, "20000.00"),
            (PriceScaleMode::Normal, 700.0, 730.0, Some(3), "710.000"),
            (PriceScaleMode::Normal, 0.012, 0.013, Some(6), "0.012100"),
            (
                PriceScaleMode::Normal,
                1e-18,
                9e-18,
                Some(18),
                "0.000000000000000005",
            ),
            (
                PriceScaleMode::Logarithmic,
                700f64.ln(),
                730f64.ln(),
                Some(3),
                "710.000",
            ),
        ] {
            let map = map(mode, min, max).with_price_precision(precision);
            let marks = generator.generate_marks(&map);
            assert!(
                marks.iter().any(|mark| mark.label == expected),
                "{mode:?}: {marks:?}"
            );
        }
        for (mode, min, max, decimals) in [
            (PriceScaleMode::Normal, 700.0, 700.04, 2),
            (PriceScaleMode::Normal, 0.0, 30.0, 0),
            (PriceScaleMode::Logarithmic, 0.001_f64.ln(), 0.1_f64.ln(), 2),
        ] {
            let marks =
                generator.generate_marks(&map(mode, min, max).with_price_precision(Some(decimals)));
            assert!(!marks.is_empty());
            assert!(marks.windows(2).all(|pair| pair[0].label != pair[1].label));
            assert!(
                marks
                    .iter()
                    .all(|mark| (mark.label.parse::<f64>().unwrap() - mark.price).abs() < 1e-8)
            );
        }
        let marks = generator.generate_marks(&map(PriceScaleMode::Normal, 0.0, 70.0));
        assert!(
            marks
                .windows(2)
                .all(|pair| (pair[0].y_coord - pair[1].y_coord).abs() >= 40.0)
        );
        for (mode, min, max) in [
            (PriceScaleMode::Normal, 1e-10, 2e-10),
            (PriceScaleMode::Normal, -2e-18, 2e-18),
            (PriceScaleMode::Normal, 1e-100, 2e-100),
            (PriceScaleMode::Logarithmic, 1e-12_f64.ln(), 1e-9_f64.ln()),
            (
                PriceScaleMode::Logarithmic,
                1e-10_f64.ln(),
                1.1e-10_f64.ln(),
            ),
        ] {
            let marks = generator.generate_marks(&map(mode, min, max));
            assert!(marks.len() >= 3, "{mode:?}, {min}..{max}: {marks:?}");
            assert!(marks.windows(2).all(|pair| pair[0].label != pair[1].label));
            for mark in marks {
                let labelled = mark.label.parse::<f64>().unwrap();
                let tolerance = if mode == PriceScaleMode::Logarithmic {
                    mark.price.abs() * 1e-9
                } else {
                    (max - min) * 1e-9
                };
                assert!(
                    (labelled - mark.price).abs() <= tolerance,
                    "{mode:?}: {} labelled {}",
                    mark.price,
                    mark.label
                );
            }
        }
    }

    /// The scale's target density sets the number of labels: on 500 pixels showing 0 to 100,
    /// 1, 2.5 and 5 marks per 100 pixels give steps of 20, 10 and 4.
    #[test]
    fn target_density_sets_the_number_of_marks() {
        use crate::scales::{PriceScale, PriceScaleOptions, PriceTransform};
        for (density, count) in [(1.0, 6), (2.5, 11), (5.0, 26)] {
            let mut options = PriceScaleOptions::default();
            options.tick_marks.target_density = density;
            let mut scale = PriceScale::new(options);
            scale.set_range(0.0, 100.0);
            let marks = scale.map(PriceTransform::default(), 0.0, 500.0).marks();
            assert_eq!(marks.len(), count, "{density}: {marks:?}");
        }
    }

    #[test]
    fn test_log_marks_generation() {
        let generator = PriceMarkGenerator::new();
        let (lo, hi) = (10f64.ln(), 1000f64.ln());
        let marks = generator.generate_marks(&map(PriceScaleMode::Logarithmic, lo, hi));

        assert!(!marks.is_empty());

        // Should have marks at powers of 10
        let has_10 = marks.iter().any(|m| (m.price - 10.0).abs() < 0.01);
        let has_100 = marks.iter().any(|m| (m.price - 100.0).abs() < 0.01);
        let has_1000 = marks.iter().any(|m| (m.price - 1000.0).abs() < 0.01);

        assert!(has_10 || has_100 || has_1000);

        // 18,200..22,800 holds one multiple of a power of ten (20,000), so the marks
        // fall back to 400-unit price steps. Spacing drops the 22,400 mark near the top.
        let (lo, hi) = (18_200f64.ln(), 22_800f64.ln());
        let marks = generator.generate_marks(&map(PriceScaleMode::Logarithmic, lo, hi));
        let labels: Vec<&str> = marks.iter().map(|m| m.label.as_str()).collect();
        let expected = [
            "18400.00", "18800.00", "19200.00", "19600.00", "20000.00", "20400.00", "20800.00",
            "21200.00", "21600.00", "22000.00",
        ];
        assert_eq!(labels, expected);
    }
}
