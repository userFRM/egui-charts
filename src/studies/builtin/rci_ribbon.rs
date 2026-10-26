//! RCI Ribbon: three Rank Correlation Index lines over short, middle and long lookbacks.
//!
//! Each line is [`RankCorrelationIndex`](super::RankCorrelationIndex): Spearman's rank correlation of
//! the closes in the window with their bar positions, tied closes sharing the mean of their ranks,
//! times 100. Rising prices give +100 and falling prices -100; a window of equal closes has no value.
//! Lengths default to 10, 30 and 50, with levels at +80, 0 and -80.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, RciRibbon};
//!
//! let mut ribbon = RciRibbon::default();
//! ribbon.calculate(&bars);
//! assert!(ribbon.values().value(0, 9).is_some());
//! assert!(ribbon.values().value(2, 49).is_some());
//! ```

use super::RankCorrelationIndex;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// RCI Ribbon: short, middle and long Rank Correlation Index lines in a separate pane.
#[derive(Clone)]
pub struct RciRibbon {
    lengths: [usize; 3],
    values: IndicatorValues,
    /// Colors: [Short, Middle, Long]
    colors: Vec<Color32>,
    visible: bool,
}

impl RciRibbon {
    /// Create a ribbon with the short, middle and long lengths (10, 30 and 50 by convention). A length
    /// below 2 is raised to 2, the shortest window a rank correlation exists for.
    pub fn new(short: usize, middle: usize, long: usize) -> Self {
        Self {
            lengths: [short, middle, long].map(|length| length.max(2)),
            values: IndicatorValues::new(),
            colors: vec![palette::ACCENT, palette::RED, palette::GREEN],
            visible: true,
        }
    }
}

impl Default for RciRibbon {
    fn default() -> Self {
        Self::new(10, 30, 50)
    }
}

impl Indicator for RciRibbon {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        lengths[0]: length("Short RCI length", 2, usize::MAX),
        lengths[1]: length("Middle RCI length", 2, usize::MAX),
        lengths[2]: length("Long RCI length", 2, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "RCI Ribbon"
    }

    fn desc(&self) -> &str {
        "RCI Ribbon - Rank correlation of price with time over three lookbacks [-100, 100]"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let lines = self.lengths.map(|length| {
            let mut rci = RankCorrelationIndex::new(length);
            rci.calculate(data);
            rci
        });
        self.values = IndicatorValues::new();
        for bar in 0..data.len() {
            self.values.push_lines(
                &lines
                    .each_ref()
                    .map(|rci| rci.values().value(0, bar).unwrap_or(f64::NAN)),
            );
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        let gray = palette::GRAY;
        crate::studies::StudyPane {
            height: None,
            range: Some((-100.0, 100.0)),
            levels: vec![
                (80.0, gray, "Upper Band"),
                (0.0, gray, "Middle Band"),
                (-80.0, gray, "Lower Band"),
            ],
            fill_between_levels: false,
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_cnt(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        let [short, middle, long] = self.lengths;
        vec![
            format!("Short RCI({short})"),
            format!("Middle RCI({middle})"),
            format!("Long RCI({long})"),
        ]
    }
}
