//! Moving average combinations and their crossing markers.

use super::{EMA, HMA, SMA, SmoothedMA, VWMA, WMA};
use crate::model::{Bar, PriceSource};
use crate::studies::stepper::Stepper;
use crate::studies::{
    CrossDirection, Indicator, IndicatorFill, IndicatorLineStyle, IndicatorMarker, IndicatorValue,
    IndicatorValues, InputKind, StudyInput,
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

    pub(super) fn build(self, period: usize) -> Box<dyn Indicator> {
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

/// The most averages a combination holds.
const MAX_AVERAGES: usize = 14;

/// The average index of a per-average input id such as `period_2`.
fn average_slot(id: &str) -> Option<usize> {
    let (field, index) = id.rsplit_once('_')?;
    if matches!(field, "period" | "kind" | "source") {
        index.parse().ok()
    } else {
        None
    }
}

/// Two to fourteen independently configured averages. Two to four form a
/// combination; longer configurations form a ribbon. Each line keeps its
/// standalone study's seed and warm-up. A missing line is NaN.
///
/// Crossings of lines 0 and 1 are marked by default. A touch does not cross:
/// the marker appears only when the last nonzero difference changes sign.
/// A missing value breaks that comparison, so a gap cannot create a signal.
///
/// Its inputs hold the number of averages, the crossing pair and the colouring by direction,
/// then the length, formula and price of each average, so that a template recreates it.
#[derive(Clone)]
pub struct MACombination {
    lines: Vec<AverageLine>,
    defaults: Vec<AverageLine>,
    values: IndicatorValues,
    averages: Vec<Box<dyn Indicator>>,
    sourced: Vec<Vec<Bar>>,
    crossing: Stepper<(usize, Option<std::cmp::Ordering>)>,
    markers: Vec<IndicatorMarker>,
    /// The two averages whose crossings are marked, numbered from 1; 0 picks none.
    cross: [usize; 2],
    default_cross: [usize; 2],
    pub(super) wait_for_all: bool,
    /// Whether each line is coloured by its own direction, the slowest wide and filled.
    directional: bool,
    default_directional: bool,
    visible: bool,
}

impl MACombination {
    /// Create two to fourteen lines. Returns an error for any other count.
    pub fn new(lines: Vec<AverageLine>) -> Result<Self, &'static str> {
        if !(2..=MAX_AVERAGES).contains(&lines.len()) {
            return Err("a combination needs 2 to 14 averages");
        }
        let lines: Vec<_> = lines
            .into_iter()
            .map(|mut line| {
                line.period = line
                    .period
                    .max(if line.kind == AverageType::Hull { 2 } else { 1 });
                line
            })
            .collect();
        Ok(Self {
            defaults: lines.clone(),
            lines,
            values: IndicatorValues::new(),
            averages: Vec::new(),
            sourced: Vec::new(),
            crossing: Stepper::default(),
            markers: Vec::new(),
            cross: [1, 2],
            default_cross: [1, 2],
            wait_for_all: false,
            directional: false,
            default_directional: false,
            visible: true,
        })
    }

    /// Choose the ordered pair to mark. Up means the first crosses above the
    /// second. Both indices must exist and differ.
    pub fn with_crossing_pair(mut self, first: usize, second: usize) -> Result<Self, &'static str> {
        if first == second || first >= self.lines.len() || second >= self.lines.len() {
            return Err("crossing indices must name two different averages");
        }
        self.cross = [first + 1, second + 1];
        self.default_cross = self.cross;
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
        study.directional = true;
        study.default_directional = true;
        study.without_crossings()
    }

    /// Mark no crossings until a pair is chosen.
    pub(super) fn without_crossings(mut self) -> Self {
        self.cross = [0, 0];
        self.default_cross = [0, 0];
        self
    }

    /// A single-hue ribbon, all lines 1 px, without a fill or crossing markers.
    pub fn muted_ribbon() -> Self {
        let mut study = Self::ribbon();
        study.directional = false;
        study.default_directional = false;
        for (i, line) in study.lines.iter_mut().enumerate() {
            line.color = Color32::from_rgb(
                75 + (44 * i / 11) as u8,
                13 + (8 * i / 11) as u8,
                76 + (45 * i / 11) as u8,
            );
        }
        study.defaults = study.lines.clone();
        study
    }

    /// The zero-based averages whose crossings are marked: two different picks among the lines.
    fn crossing_pair(&self) -> Option<(usize, usize)> {
        let [first, second] = self.cross;
        let picked = |pick: usize| (1..=self.lines.len()).contains(&pick);
        if first != second && picked(first) && picked(second) {
            Some((first - 1, second - 1))
        } else {
            None
        }
    }

    /// The inputs a preset offers: those of its averages, its crossing pair among them or None,
    /// and, for a ribbon, its colouring by direction. A preset keeps its number of averages.
    pub(super) fn preset_inputs(&self, ribbon: bool) -> Vec<StudyInput> {
        self.inputs()
            .into_iter()
            .filter(|input| match input.id.as_str() {
                "average_count" => false,
                "directional" => ribbon,
                _ => true,
            })
            .map(|mut input| {
                if let ("cross_first" | "cross_second", InputKind::Choice(picks)) =
                    (input.id.as_str(), &mut input.kind)
                {
                    picks.truncate(self.lines.len() + 1);
                }
                input
            })
            .collect()
    }
}

impl Indicator for MACombination {
    fn inputs(&self) -> Vec<crate::studies::StudyInput> {
        use crate::studies::InputValue;
        let kinds = [
            AverageType::Simple,
            AverageType::Exponential,
            AverageType::Weighted,
            AverageType::Smoothed,
            AverageType::VolumeWeighted,
            AverageType::Hull,
        ];
        let sources = PriceSource::all();
        let picks: Vec<String> = (0..=MAX_AVERAGES)
            .map(|pick| {
                if pick == 0 {
                    "None".to_owned()
                } else {
                    pick.to_string()
                }
            })
            .collect();
        let mut inputs = vec![
            StudyInput::new(
                "average_count",
                "Averages",
                InputValue::Length(self.lines.len()),
                InputKind::Length(2, MAX_AVERAGES),
            )
            .with_default(InputValue::Length(self.defaults.len())),
            StudyInput::new(
                "directional",
                "Colour by direction",
                InputValue::Toggle(self.directional),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(self.default_directional)),
            StudyInput::new(
                "cross_first",
                "Crossing average",
                InputValue::Choice(self.cross[0]),
                InputKind::Choice(picks.clone()),
            )
            .with_default(InputValue::Choice(self.default_cross[0])),
            StudyInput::new(
                "cross_second",
                "Crossed average",
                InputValue::Choice(self.cross[1]),
                InputKind::Choice(picks),
            )
            .with_default(InputValue::Choice(self.default_cross[1])),
        ];
        inputs.extend(self.lines.iter().enumerate().flat_map(|(index, line)| {
            // An average added after construction has no construction default; it is its own.
            let default = self.defaults.get(index).unwrap_or(line);
            [
                StudyInput::new(
                    format!("period_{index}"),
                    format!("Length {}", index + 1),
                    InputValue::Length(line.period),
                    InputKind::Length(
                        if line.kind == AverageType::Hull { 2 } else { 1 },
                        usize::MAX,
                    ),
                )
                .with_default(InputValue::Length(default.period)),
                StudyInput::new(
                    format!("kind_{index}"),
                    format!("Average {}", index + 1),
                    InputValue::Choice(kinds.iter().position(|kind| *kind == line.kind).unwrap()),
                    InputKind::Choice(kinds.iter().map(|kind| kind.label().to_owned()).collect()),
                )
                .with_default(InputValue::Choice(
                    kinds.iter().position(|kind| *kind == default.kind).unwrap(),
                )),
                StudyInput::new(
                    format!("source_{index}"),
                    format!("Source {}", index + 1),
                    InputValue::Choice(
                        sources
                            .iter()
                            .position(|source| *source == line.source)
                            .unwrap(),
                    ),
                    InputKind::Choice(
                        sources
                            .iter()
                            .map(|source| source.label().to_owned())
                            .collect(),
                    ),
                )
                .with_default(InputValue::Choice(
                    sources
                        .iter()
                        .position(|source| *source == default.source)
                        .unwrap(),
                )),
            ]
        }));
        inputs
    }

    /// `average_count` resizes the averages, so it comes first in an applied set of inputs; an
    /// input of an average past the last has nothing to edit and is accepted unchanged. Averages
    /// added take the settings the study was built with at their place, or copy the last one.
    /// A crossing pick past the averages, or the same average twice, marks no crossings.
    fn set_input(
        &mut self,
        id: &str,
        value: crate::studies::InputValue,
    ) -> Result<(), &'static str> {
        use crate::studies::InputValue;
        if average_slot(id).is_some_and(|index| (self.lines.len()..MAX_AVERAGES).contains(&index)) {
            return Ok(());
        }
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
        }
        match (id, value) {
            ("average_count", InputValue::Length(count)) => {
                self.lines.truncate(count);
                while self.lines.len() < count {
                    let index = self.lines.len();
                    let line = self
                        .defaults
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| self.lines[index - 1].clone());
                    self.lines.push(line);
                }
            }
            ("directional", InputValue::Toggle(directional)) => self.directional = directional,
            ("cross_first", InputValue::Choice(pick)) => self.cross[0] = pick,
            ("cross_second", InputValue::Choice(pick)) => self.cross[1] = pick,
            (id, value) => {
                let (field, index) = id.rsplit_once('_').ok_or("unknown study input")?;
                let index: usize = index.parse().map_err(|_| "unknown study input")?;
                let line = self.lines.get_mut(index).ok_or("unknown study input")?;
                match (field, value) {
                    ("period", InputValue::Length(period)) => line.period = period,
                    ("kind", InputValue::Choice(kind)) => {
                        line.kind = [
                            AverageType::Simple,
                            AverageType::Exponential,
                            AverageType::Weighted,
                            AverageType::Smoothed,
                            AverageType::VolumeWeighted,
                            AverageType::Hull,
                        ][kind];
                        line.period =
                            line.period
                                .max(if line.kind == AverageType::Hull { 2 } else { 1 });
                    }
                    ("source", InputValue::Choice(source)) => {
                        line.source = PriceSource::all()[source]
                    }
                    _ => return Err("unknown study input"),
                }
            }
        }
        self.calculate(&[]);
        Ok(())
    }

    fn name(&self) -> &str {
        "MA Combination"
    }
    fn desc(&self) -> &str {
        "Independently configured moving averages and crossing markers"
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        if start == 0 {
            self.averages = self
                .lines
                .iter()
                .map(|line| line.kind.build(line.period))
                .collect();
            self.sourced = vec![Vec::new(); self.lines.len()];
        }
        for ((line, average), sourced) in self
            .lines
            .iter()
            .zip(&mut self.averages)
            .zip(&mut self.sourced)
        {
            if line.source == PriceSource::Close {
                average.update(data, start);
            } else {
                sourced.truncate(start);
                sourced.extend(data[start..].iter().map(|bar| {
                    let mut bar = bar.clone();
                    bar.close = line.source.compute(bar.open, bar.high, bar.low, bar.close);
                    bar
                }));
                average.update(sourced, start);
            }
        }
        let pair = self.crossing_pair();
        self.markers
            .truncate(self.markers.partition_point(|marker| marker.bar < start));
        self.crossing
            .run(data, start, &mut self.values, |state, _bar| {
                let i = state.0;
                state.0 += 1;
                let lines: Vec<_> = self
                    .averages
                    .iter()
                    .map(|average| average.values().value(0, i).unwrap_or(f64::NAN))
                    .collect();
                if lines.iter().all(|v| !v.is_finite())
                    || (self.wait_for_all && lines.iter().any(|v| !v.is_finite()))
                {
                    state.1 = None;
                    return IndicatorValue::None;
                }
                if let Some((first, second)) = pair {
                    if lines[first].is_finite() && lines[second].is_finite() {
                        let direction = lines[first].partial_cmp(&lines[second]).unwrap();
                        if direction != std::cmp::Ordering::Equal {
                            if state.1.is_some_and(|p| p != direction) {
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
                            state.1 = Some(direction);
                        }
                    } else {
                        state.1 = None;
                    }
                }
                IndicatorValue::Multiple(lines)
            });
    }
    fn is_incremental(&self) -> bool {
        true
    }
    fn values(&self) -> &IndicatorValues {
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
        match self.lines.get(line) {
            Some(average) if self.directional => {
                let slowest = line + 1 == self.lines.len();
                IndicatorLineStyle {
                    width: if slowest { 3.0 } else { 1.0 },
                    rising: Some(average.color),
                    falling: Some(Color32::from_rgb(38, 198, 218)),
                    fill: slowest.then_some(IndicatorFill {
                        color: Color32::from_rgb(98, 55, 174),
                        top_alpha: 38,
                        bottom_alpha: 10,
                    }),
                }
            }
            _ => IndicatorLineStyle::default(),
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
    ($ty:ident, $name:literal, $description:literal $(, $marker:expr)?) => {
        impl crate::studies::Indicator for $ty {
            fn inputs(&self) -> Vec<crate::studies::StudyInput> {
                self.inner.preset_inputs(matches!($name, "MA Ribbon"))
            }
            fn set_input(
                &mut self,
                id: &str,
                value: crate::studies::InputValue,
            ) -> Result<(), &'static str> {
                let schema = self
                    .inputs()
                    .into_iter()
                    .find(|input| input.id == id)
                    .ok_or("unknown study input")?;
                if !schema.accepts(&value) {
                    return Err("study input is outside its accepted values");
                }
                self.inner.set_input(id, value)
            }
            fn name(&self) -> &str {
                $name
            }
            fn desc(&self) -> &str {
                $description
            }
            fn calculate(&mut self, data: &[crate::model::Bar]) {
                self.inner.calculate(data);
            }
            fn update(&mut self, data: &[crate::model::Bar], from: usize) {
                self.inner.update(data, from);
            }
            fn is_incremental(&self) -> bool {
                self.inner.is_incremental()
            }
            fn values(&self) -> &crate::studies::IndicatorValues {
                self.inner.values()
            }
            fn colors(&self) -> Vec<egui::Color32> {
                self.inner.colors()
            }
            fn set_colors(&mut self, colors: Vec<egui::Color32>) {
                if colors.len() >= self.inner.line_count() {
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
            $(fn marker_style(&self) -> Option<crate::studies::PlotStyle> {
                Some(crate::studies::PlotStyle {
                    color: $marker, width: 4.0,
                    pattern: crate::model::LineStyle::Solid,
                    plot_type: crate::studies::PlotType::Crosses, visible: true,
                })
            })?
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

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::test_support::make_bar;
    use crate::studies::{IndicatorFactory, InputKind, InputValue};

    /// Closes rise from 0 to 59, then fall from 60 to 1. At bar 59 the 9-bar mean (55) is above the
    /// 21-bar mean (49) and the 20-bar (49.5) above the 50-bar (34.5); at bar 119 the 9-bar (5) is
    /// below the 21-bar (11) and the 20-bar (10.5) below the 50-bar (25.5). So the first two
    /// averages of every preset cross.
    #[test]
    fn presets_offer_their_crossing_pair_with_none() {
        use InputValue::Choice;
        let bars: Vec<Bar> = (0..60)
            .chain((1..=60).rev())
            .map(|close| make_bar(f64::from(close)))
            .collect();
        for (name, averages, marked) in [
            ("MA Cross", 2, true),
            ("EMA Cross", 2, true),
            ("SMA/EMA Cross", 2, true),
            ("Triple MA", 3, true),
            ("Multi MA", 4, false),
            ("MA Ribbon", 12, false),
        ] {
            let mut study = IndicatorFactory::new().create(name).unwrap();
            let inputs = study.inputs();
            for id in ["cross_first", "cross_second"] {
                let input = inputs.iter().find(|input| input.id == id).unwrap();
                let InputKind::Choice(picks) = &input.kind else {
                    panic!("{name} {id}: {:?}", input.kind)
                };
                assert_eq!(
                    (picks.len(), picks[0].as_str()),
                    (averages + 1, "None"),
                    "{name}"
                );
            }
            study.calculate(&bars);
            assert_eq!(!study.markers().is_empty(), marked, "{name} by default");
            study.set_input("cross_first", Choice(1)).unwrap();
            study.set_input("cross_second", Choice(2)).unwrap();
            study.calculate(&bars);
            assert!(!study.markers().is_empty(), "{name} with its first two");
            study.set_input("cross_first", Choice(0)).unwrap();
            study.calculate(&bars);
            assert!(study.markers().is_empty(), "{name} with None");
            assert!(
                study
                    .set_input("cross_first", Choice(averages + 1))
                    .is_err(),
                "{name} past its averages"
            );
        }
    }
}
