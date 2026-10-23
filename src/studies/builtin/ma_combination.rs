//! Moving average combinations and their crossing markers.

use super::{EMA, HMA, SMA, SmoothedMA, VWMA, WMA};
use crate::model::{Bar, PriceSource};
use crate::studies::{
    CrossDirection, Indicator, IndicatorFill, IndicatorLineStyle, IndicatorMarker, IndicatorValue,
};
use egui::Color32;

/// Average formula used by a line of [`MACombination`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AverageType {
    /// Arithmetic mean of a complete window.
    Simple,
    /// Exponential smoothing with alpha 2/(period+1), seeded by the first price.
    Exponential,
    /// Linear weights 1 through period, newest heaviest.
    Weighted,
    /// Wilder smoothing with alpha 1/period, seeded by a complete SMA.
    Smoothed,
    /// Price weighted by volume over a complete window.
    VolumeWeighted,
    /// Hull's weighted average of twice the half-length WMA minus the full WMA.
    Hull,
}

impl AverageType {
    fn label(self) -> &'static str {
        match self {
            Self::Simple => "SMA",
            Self::Exponential => "EMA",
            Self::Weighted => "WMA",
            Self::Smoothed => "SMMA",
            Self::VolumeWeighted => "VWMA",
            Self::Hull => "HMA",
        }
    }

    fn build(self, period: usize) -> Box<dyn Indicator> {
        match self {
            Self::Simple => Box::new(SMA::new(period)),
            Self::Exponential => Box::new(EMA::new(period)),
            Self::Weighted => Box::new(WMA::new(period)),
            Self::Smoothed => Box::new(SmoothedMA::new(period)),
            Self::VolumeWeighted => Box::new(VWMA::new(period)),
            Self::Hull => Box::new(HMA::new(period)),
        }
    }
}

/// One moving average's formula, length, input price and colour.
#[derive(Clone, Debug)]
pub struct AverageLine {
    /// Average formula.
    pub kind: AverageType,
    /// Window length; zero is treated as one (Hull requires two).
    pub period: usize,
    /// Source price; defaults to close.
    pub source: PriceSource,
    /// Line colour.
    pub color: Color32,
}

impl AverageLine {
    /// A moving average of closing prices.
    pub fn new(kind: AverageType, period: usize, color: Color32) -> Self {
        Self {
            kind,
            period: period.max(1),
            source: PriceSource::Close,
            color,
        }
    }

    /// Select this line's input price independently of the other lines.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ribbon {
    None,
    Directional,
    Muted,
}

/// Two to fourteen independently configured averages. Two to four form a
/// combination; longer configurations form a ribbon. Each line keeps its
/// standalone study's seed and warm-up. A missing line is NaN.
///
/// Crossings of lines 0 and 1 are marked by default. A touch does not cross:
/// the marker appears only when the last nonzero difference changes sign.
/// A missing value breaks that comparison, so a gap cannot create a signal.
#[derive(Clone)]
pub struct MACombination {
    lines: Vec<AverageLine>,
    values: Vec<IndicatorValue>,
    markers: Vec<IndicatorMarker>,
    pair: Option<(usize, usize)>,
    pub(super) wait_for_all: bool,
    ribbon: Ribbon,
    visible: bool,
}

impl MACombination {
    /// Create two to fourteen lines. Returns an error for any other count.
    pub fn new(lines: Vec<AverageLine>) -> Result<Self, &'static str> {
        if !(2..=14).contains(&lines.len()) {
            return Err("a combination needs 2 to 14 averages");
        }
        Ok(Self {
            lines,
            values: Vec::new(),
            markers: Vec::new(),
            pair: Some((0, 1)),
            wait_for_all: false,
            ribbon: Ribbon::None,
            visible: true,
        })
    }

    /// Choose the ordered pair to mark. Up means the first crosses above the
    /// second. Both indices must exist and differ.
    pub fn with_crossing_pair(mut self, first: usize, second: usize) -> Result<Self, &'static str> {
        if first == second || first >= self.lines.len() || second >= self.lines.len() {
            return Err("crossing indices must name two different averages");
        }
        self.pair = Some((first, second));
        Ok(self)
    }

    /// Twelve exponential averages of lengths 5, 10, ..., 60, coloured by their
    /// own direction. The slowest is 3 px with a purple fill fading to the bottom.
    pub fn ribbon() -> Self {
        let purple = Color32::from_rgb(98, 55, 174);
        let mut study = Self::new(
            (1..=12)
                .map(|i| AverageLine::new(AverageType::Exponential, i * 5, purple))
                .collect(),
        )
        .unwrap();
        study.ribbon = Ribbon::Directional;
        study.pair = None;
        study
    }

    /// A single-hue ribbon, all lines 1 px, without a fill or crossing markers.
    pub fn muted_ribbon() -> Self {
        let mut study = Self::ribbon();
        study.ribbon = Ribbon::Muted;
        for (i, line) in study.lines.iter_mut().enumerate() {
            line.color = Color32::from_rgb(
                75 + (44 * i / 11) as u8,
                13 + (8 * i / 11) as u8,
                76 + (45 * i / 11) as u8,
            );
        }
        study
    }
}

impl Indicator for MACombination {
    fn name(&self) -> &str {
        "MA Combination"
    }
    fn desc(&self) -> &str {
        "Independently configured moving averages and crossing markers"
    }
    fn calculate(&mut self, data: &[Bar]) {
        let mut averages = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let mut average = line.kind.build(line.period);
            if line.source == PriceSource::Close {
                average.calculate(data);
            } else {
                let sourced: Vec<_> = data
                    .iter()
                    .map(|bar| {
                        let mut sourced = bar.clone();
                        sourced.close = line.source.compute(bar.open, bar.high, bar.low, bar.close);
                        sourced
                    })
                    .collect();
                average.calculate(&sourced);
            }
            averages.push(average);
        }
        self.values.clear();
        self.markers.clear();
        let mut previous = None;
        for i in 0..data.len() {
            let lines: Vec<_> = averages
                .iter()
                .map(|average| match average.values()[i] {
                    IndicatorValue::Single(value) => value,
                    _ => f64::NAN,
                })
                .collect();
            if lines.iter().all(|v| !v.is_finite())
                || (self.wait_for_all && lines.iter().any(|v| !v.is_finite()))
            {
                self.values.push(IndicatorValue::None);
                previous = None;
                continue;
            }
            if let Some((first, second)) = self.pair {
                if lines[first].is_finite() && lines[second].is_finite() {
                    let direction = lines[first].partial_cmp(&lines[second]).unwrap();
                    if direction != std::cmp::Ordering::Equal {
                        if previous.is_some_and(|p| p != direction) {
                            self.markers.push(IndicatorMarker {
                                bar: i,
                                value: lines[first],
                                direction: if direction == std::cmp::Ordering::Greater {
                                    CrossDirection::Up
                                } else {
                                    CrossDirection::Down
                                },
                            });
                        }
                        previous = Some(direction);
                    }
                } else {
                    previous = None;
                }
            }
            self.values.push(IndicatorValue::Multiple(lines));
        }
    }
    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        self.lines.iter().map(|line| line.color).collect()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.lines.iter_mut().zip(colors) {
            line.color = color;
        }
    }
    fn line_names(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| {
                format!(
                    "{}({})",
                    line.kind.label(),
                    line.period
                        .max(if line.kind == AverageType::Hull { 2 } else { 1 })
                )
            })
            .collect()
    }
    fn markers(&self) -> &[IndicatorMarker] {
        &self.markers
    }
    fn line_style(&self, line: usize) -> IndicatorLineStyle {
        let slowest = line + 1 == self.lines.len();
        if self.ribbon == Ribbon::Directional {
            IndicatorLineStyle {
                width: if slowest { 3.0 } else { 1.0 },
                rising: Some(self.lines[line].color),
                falling: Some(Color32::from_rgb(38, 198, 218)),
                fill: slowest.then_some(IndicatorFill {
                    color: Color32::from_rgb(98, 55, 174),
                    top_alpha: 38,
                    bottom_alpha: 10,
                }),
            }
        } else {
            IndicatorLineStyle::default()
        }
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
}

// Preserve the existing public preset types and their constructors while sharing all calculation.
macro_rules! impl_ma_preset {
    ($ty:ident, $name:literal, $description:literal) => {
        impl crate::studies::Indicator for $ty {
            fn name(&self) -> &str {
                $name
            }
            fn desc(&self) -> &str {
                $description
            }
            fn calculate(&mut self, data: &[crate::model::Bar]) {
                self.inner.calculate(data);
            }
            fn values(&self) -> &[crate::studies::IndicatorValue] {
                self.inner.values()
            }
            fn colors(&self) -> Vec<egui::Color32> {
                self.inner.colors()
            }
            fn set_colors(&mut self, colors: Vec<egui::Color32>) {
                if colors.len() >= self.inner.line_cnt() {
                    self.inner.set_colors(colors);
                }
            }
            fn line_names(&self) -> Vec<String> {
                let mut names = self.inner.line_names();
                if matches!($name, "MA Cross" | "EMA Cross") {
                    names[0] = format!("Fast {}", names[0]);
                    names[1] = format!("Slow {}", names[1]);
                }
                names
            }
            fn markers(&self) -> &[crate::studies::IndicatorMarker] {
                self.inner.markers()
            }
            fn line_style(&self, line: usize) -> crate::studies::IndicatorLineStyle {
                self.inner.line_style(line)
            }
            fn is_visible(&self) -> bool {
                self.inner.is_visible()
            }
            fn set_visible(&mut self, visible: bool) {
                self.inner.set_visible(visible);
            }
            fn clone_box(&self) -> Box<dyn crate::studies::Indicator> {
                Box::new(self.clone())
            }
        }
    };
}
pub(super) use impl_ma_preset;
