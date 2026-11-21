//! Drawing studies anchored on the shared Zig Zag swings.

use super::swings::Swings;
use super::{ATR, ZigZag, ZigZagPivot};
use crate::drawings::{ChartPoint, Drawing, DrawingToolType, FibonacciConfig, FibonacciLevel};
use crate::model::Bar;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, Stepper, StudyInput,
};
use egui::Color32;

/// The study catalogue's swing size: a multiple of the average true range over a period.
const ATR_MULTIPLE: f64 = 3.0;
const ATR_PERIOD: usize = 10;

/// The least share of the leg before it that an extension's last leg takes back.
const MIN_RETRACEMENT: f64 = 0.236;

/// The most Fibonacci levels that the study inputs can hold.
const MAX_FIB_LEVELS: usize = 24;

/// The fields of each Fibonacci level among the inputs, as in `fib_ratio_2`.
const LEVEL_FIELDS: [&str; 5] = [
    "fib_visible",
    "fib_ratio",
    "fib_label",
    "fib_color",
    "fib_price",
];

/// The level index of a per-level input id such as `fib_ratio_2`.
fn level_slot(id: &str) -> Option<usize> {
    let (field, index) = id.rsplit_once('_')?;
    if LEVEL_FIELDS.contains(&field) {
        index.parse().ok()
    } else {
        None
    }
}

/// Construction to place on the latest Zig Zag pivots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutoDrawingKind {
    /// Last three pivots, with the base through the first and third and the parallel through the second.
    ParallelChannel,
    /// Last two confirmed pivots; the current, unfinished swing is excluded.
    FibonacciRetracement,
    /// Latest three confirmed pivots whose last leg takes back 23.6% to 100% of the leg before
    /// it, stepping back one pivot at a time until three qualify; the levels project the first
    /// leg from the third pivot.
    FibonacciExtension,
    /// Last three pivots, with the median from the first to the midpoint of the other two.
    Pitchfork,
}

impl AutoDrawingKind {
    pub(crate) const ALL: [Self; 4] = [
        Self::ParallelChannel,
        Self::FibonacciRetracement,
        Self::FibonacciExtension,
        Self::Pitchfork,
    ];

    /// Whether the construction is a Fibonacci one, set up by a [`FibonacciConfig`].
    fn fibonacci(self) -> bool {
        matches!(self, Self::FibonacciRetracement | Self::FibonacciExtension)
    }

    /// The Fibonacci levels and options a study of this kind starts with.
    fn default_fibonacci(self) -> FibonacciConfig {
        FibonacciConfig {
            extend_right: true,
            ..FibonacciConfig::for_tool(self.tool())
        }
    }

    /// How the catalogue's study of this kind finds its swings.
    fn default_swing_size(self) -> SwingSize {
        if self == Self::FibonacciExtension {
            SwingSize::Bars
        } else {
            SwingSize::Atr
        }
    }

    fn tool(self) -> DrawingToolType {
        match self {
            Self::ParallelChannel => DrawingToolType::ParallelChannel,
            Self::FibonacciRetracement => DrawingToolType::FibonacciRetracement,
            Self::FibonacciExtension => DrawingToolType::FibonacciExtension,
            Self::Pitchfork => DrawingToolType::Pitchfork,
        }
    }
}

/// How the swings an automatic drawing is anchored on are found, in the order of the
/// `swing_size` choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwingSize {
    /// Zig Zag reversals of a multiple of the average true range, as a percentage of the close.
    Atr,
    /// Zig Zag reversals of the fixed deviation percentage.
    Percent,
    /// Highs and lows found by half the depth of bars on each side, as the pattern studies find
    /// them; offered for the extension only.
    Bars,
}

/// The latest three consecutive alternating pivots, oldest first, whose last leg takes back
/// between 23.6% and 100% of the leg before it, stepping back one pivot at a time.
fn extension_anchors(
    count: usize,
    pivot: impl Fn(usize) -> ZigZagPivot,
) -> Option<[ZigZagPivot; 3]> {
    (2..count)
        .rev()
        .map(|c| [pivot(c - 2), pivot(c - 1), pivot(c)])
        .find(|[a, b, c]| {
            // Measured towards the second pivot, so both moves are positive for a valid leg.
            let sign = if a.is_high { 1.0 } else { -1.0 };
            let (leg, back) = (sign * (a.price - b.price), sign * (c.price - b.price));
            leg > 0.0 && back >= MIN_RETRACEMENT * leg && back <= leg
        })
}

/// A drawing recalculated from host bars with the same pivots as [`ZigZag`]. Swings use the
/// fixed Zig Zag percentage unless [`with_atr`](Self::with_atr) sizes them by the average true
/// range. The catalogue's extension finds its swings by bars instead (`swing_size` choice 2,
/// "Bars"): a high or low beyond those of the bars within half the depth before it and not
/// passed by those within half the depth after it, so confirmed half the depth later; of
/// consecutive ones of a kind, the more extreme counts.
///
/// Add through the normal study API or calculate directly and read [`Indicator::drawings`].
/// No drawing is emitted until enough pivots exist. A channel includes the moving last extreme;
/// a retracement uses the latest completed swing. An extension uses the latest three confirmed
/// pivots whose last leg takes back 23.6% to 100% of the leg before it, steps back one pivot at a
/// time until three qualify and draws nothing when none do; its levels project the first leg
/// from the third pivot. A pitchfork follows the latest three pivots. Updating or replacing bars
/// rebuilds the anchors; generated drawings belong to the study, not the editable drawing list.
/// Output 0 contains anchor prices for scaling and readouts; the drawing renderer paints the
/// construction instead of joining these values. Its colour, width and visibility use plot 0.
#[derive(Clone, Debug)]
pub struct AutoDrawing {
    kind: AutoDrawingKind,
    zigzag: ZigZag,
    /// How the swings are found: by the average true range, the fixed percentage or bars.
    swing_size: SwingSize,
    atr_multiple: f64,
    atr_period: usize,
    atr: ATR,
    fibonacci: FibonacciConfig,
    fork_colors: [Color32; 2],
    fork_transparency: usize,
    /// The swings found by bars, for [`SwingSize::Bars`].
    bar_swings: Stepper<Swings>,
    /// Source bars whose values hold the drawn anchors.
    anchors: Vec<usize>,
    drawings: Vec<Drawing>,
    values: IndicatorValues,
}

impl AutoDrawing {
    /// Construct with a fixed Zig Zag deviation in percent and confirmation depth in bars.
    /// Parameters have the same bounds and skipped-bar policy as [`ZigZag::new`].
    /// [`with_atr`](Self::with_atr) sizes swings by volatility instead, as the study
    /// catalogue's automatic drawings do.
    pub fn new(kind: AutoDrawingKind, deviation_percent: f64, depth: usize) -> Self {
        Self {
            kind,
            zigzag: ZigZag::new(deviation_percent, depth).with_color(
                if kind == AutoDrawingKind::Pitchfork {
                    crate::studies::palette::BRIGHT_RED
                } else {
                    crate::studies::palette::BLUE
                },
            ),
            swing_size: SwingSize::Percent,
            atr_multiple: ATR_MULTIPLE,
            atr_period: ATR_PERIOD,
            atr: ATR::new(ATR_PERIOD),
            fibonacci: kind.default_fibonacci(),
            fork_colors: [
                crate::studies::palette::GREEN,
                crate::studies::palette::BLUE,
            ],
            fork_transparency: 85,
            bar_swings: Stepper::default(),
            anchors: Vec::new(),
            drawings: Vec::new(),
            values: IndicatorValues::new(),
        }
    }

    /// Size swings by recent volatility instead of the fixed percentage: a reversal counts when
    /// it moves from the last extreme by at least `multiple` times the average true range over
    /// `period` bars ([`ATR`]), taken as a percentage of the confirming bar's close. Bars before
    /// the average is defined, and bars without a finite positive close, confirm no reversal.
    /// `multiple` is clamped into 0 to `f64::MAX`, NaN to 0, where no swing is found; `period`
    /// is at least 1.
    pub fn with_atr(mut self, multiple: f64, period: usize) -> Self {
        self.swing_size = SwingSize::Atr;
        self.atr_multiple = super::clamp_param(multiple, 0.0, f64::MAX);
        self.atr_period = period.max(1);
        self.atr = ATR::new(self.atr_period);
        self
    }

    /// The study the catalogue registers: swings of 3 times the 10-bar average true range and a
    /// depth of 10, and for an extension, swings found by bars over that depth.
    pub(crate) fn catalogue(kind: AutoDrawingKind) -> Self {
        let mut study = Self::new(kind, 5.0, 10).with_atr(ATR_MULTIPLE, ATR_PERIOD);
        study.swing_size = kind.default_swing_size();
        study
    }

    fn fork_config(&self) -> crate::drawings::PitchforkConfig {
        let mut config = crate::drawings::PitchforkConfig::legacy();
        let colors = [
            self.zigzag.colors()[0],
            self.fork_colors[0],
            self.fork_colors[1],
        ];
        for level in &mut config.levels {
            level.visible = level.value <= 1.0;
            level.color = colors[if level.value == 0.0 {
                0
            } else if level.value == 0.5 {
                1
            } else {
                2
            }]
            .to_srgba_unmultiplied();
        }
        // Convert percent transparency to the nearest byte of opacity.
        config.background_opacity = (((100 - self.fork_transparency) * 255 + 50) / 100) as u8;
        config
    }

    /// Set the levels, orientation and price measurement of a Fibonacci construction. As the
    /// study inputs hold them, levels whose ratio is not finite are dropped, at most 24 are kept,
    /// and each label is kept on one line, without control characters or surrounding spaces.
    pub fn with_fibonacci_config(mut self, mut config: FibonacciConfig) -> Self {
        config.levels.retain(|level| level.value.is_finite());
        config.levels.truncate(MAX_FIB_LEVELS);
        for level in &mut config.levels {
            level.label = level.label.replace(char::is_control, " ").trim().to_owned();
        }
        self.fibonacci = config;
        self
    }
}

impl Indicator for AutoDrawing {
    fn name(&self) -> &str {
        match self.kind {
            AutoDrawingKind::ParallelChannel => "Auto Parallel Channel",
            AutoDrawingKind::FibonacciRetracement => "Auto Fibonacci Retracement",
            AutoDrawingKind::FibonacciExtension => "Auto Fibonacci Extension",
            AutoDrawingKind::Pitchfork => "Auto Pitchfork",
        }
    }

    fn desc(&self) -> &str {
        "Drawing anchored on Zig Zag swing extremes"
    }

    /// The swing size (for an extension also "Bars", which reads only the depth), ATR multiple
    /// and period, the Zig Zag deviation and depth and, for a Fibonacci construction, its
    /// options, its number of levels and each level's visibility, ratio, label, colour and price
    /// label, or the pitchfork's background transparency.
    fn inputs(&self) -> Vec<StudyInput> {
        let mut sizes = vec!["ATR multiple".to_owned(), "Percent".to_owned()];
        if self.kind == AutoDrawingKind::FibonacciExtension {
            sizes.push("Bars".to_owned());
        }
        let mut inputs = vec![
            StudyInput::new(
                "swing_size",
                "Swing size",
                InputValue::Choice(self.swing_size as usize),
                InputKind::Choice(sizes),
            )
            .with_default(InputValue::Choice(self.kind.default_swing_size() as usize)),
            StudyInput::new(
                "atr_multiple",
                "ATR multiple",
                InputValue::Number(self.atr_multiple),
                InputKind::Number(0.0, f64::MAX),
            )
            .with_default(InputValue::Number(ATR_MULTIPLE)),
            StudyInput::new(
                "atr_period",
                "ATR period",
                InputValue::Length(self.atr_period),
                InputKind::Length(1, usize::MAX),
            )
            .with_default(InputValue::Length(ATR_PERIOD)),
        ];
        inputs.extend(
            self.zigzag
                .inputs()
                .into_iter()
                .filter(|input| matches!(input.id.as_str(), "deviation_percent" | "depth")),
        );
        if self.kind == AutoDrawingKind::Pitchfork {
            inputs.push(
                StudyInput::new(
                    "fork_transparency",
                    "Background transparency",
                    InputValue::Length(self.fork_transparency),
                    InputKind::Length(0, 100),
                )
                .with_default(InputValue::Length(85)),
            );
        }
        if !self.kind.fibonacci() {
            return inputs;
        }
        let config = &self.fibonacci;
        let defaults = self.kind.default_fibonacci();
        let toggle = |id: String, label: String, value: bool, default: bool| {
            StudyInput::new(id, label, InputValue::Toggle(value), InputKind::Toggle)
                .with_default(InputValue::Toggle(default))
        };
        let colour = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let optional = |id, label, value: Option<bool>, default: Option<bool>| {
            let index = |value| match value {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };
            StudyInput::new(
                id,
                label,
                InputValue::Choice(index(value)),
                InputKind::Choice(["Default", "Hidden", "Shown"].map(str::to_owned).to_vec()),
            )
            .with_default(InputValue::Choice(index(default)))
        };
        inputs.extend([
            optional(
                "fib_show_prices",
                "Price labels",
                config.show_prices,
                defaults.show_prices,
            ),
            optional(
                "fib_show_levels",
                "Level labels",
                config.show_levels,
                defaults.show_levels,
            ),
            toggle(
                "fib_levels_as_percent".into(),
                "Levels as percentages".into(),
                config.levels_as_percent,
                defaults.levels_as_percent,
            ),
            toggle(
                "fib_labels_on_left".into(),
                "Labels on left".into(),
                config.labels_on_left,
                defaults.labels_on_left,
            ),
            toggle(
                "fib_reverse".into(),
                "Reverse".into(),
                config.reverse,
                defaults.reverse,
            ),
            toggle(
                "fib_extend_left".into(),
                "Extend left".into(),
                config.extend_left,
                defaults.extend_left,
            ),
            toggle(
                "fib_extend_right".into(),
                "Extend right".into(),
                config.extend_right,
                defaults.extend_right,
            ),
            toggle(
                "fib_background".into(),
                "Background".into(),
                config.show_background,
                defaults.show_background,
            ),
            StudyInput::new(
                "fib_background_opacity",
                "Background opacity",
                InputValue::Length(config.background_opacity.into()),
                InputKind::Length(0, 255),
            )
            .with_default(InputValue::Length(defaults.background_opacity.into())),
            toggle(
                "fib_logarithmic".into(),
                "Logarithmic levels".into(),
                config.logarithmic,
                defaults.logarithmic,
            ),
            StudyInput::new(
                "fib_level_count",
                "Levels",
                InputValue::Length(config.levels.len()),
                InputKind::Length(0, MAX_FIB_LEVELS),
            )
            .with_default(InputValue::Length(defaults.levels.len())),
        ]);
        for (i, level) in config.levels.iter().enumerate() {
            // A level added after construction has no construction default; it is its own.
            let default = defaults.levels.get(i).unwrap_or(level);
            let n = i + 1;
            inputs.extend([
                toggle(
                    format!("fib_visible_{i}"),
                    format!("Level {n} visible"),
                    level.visible,
                    default.visible,
                ),
                StudyInput::new(
                    format!("fib_ratio_{i}"),
                    format!("Level {n} ratio"),
                    InputValue::Number(level.value.into()),
                    InputKind::Number((-f32::MAX).into(), f32::MAX.into()),
                )
                .with_default(InputValue::Number(default.value.into())),
                StudyInput::new(
                    format!("fib_label_{i}"),
                    format!("Level {n} label"),
                    InputValue::Text(level.label.clone()),
                    InputKind::Text,
                )
                .with_default(InputValue::Text(default.label.clone())),
                StudyInput::new(
                    format!("fib_color_{i}"),
                    format!("Level {n} colour"),
                    InputValue::Color(colour(level.color)),
                    InputKind::Color,
                )
                .with_default(InputValue::Color(colour(default.color))),
                toggle(
                    format!("fib_price_{i}"),
                    format!("Level {n} price visible"),
                    level.show_price,
                    default.show_price,
                ),
            ]);
        }
        inputs
    }

    /// `fib_level_count` resizes the Fibonacci levels, so it comes first among the level inputs of
    /// an applied set; an input of a level past the last has nothing to edit and is accepted
    /// unchanged. Levels added take the kind's default level at their place, or 100% in the
    /// study's colour, as the drawing properties add one.
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        if matches!(id, "deviation_percent" | "depth") {
            self.zigzag.set_input(id, value)?;
            self.calculate(&[]);
            return Ok(());
        }
        if self.kind.fibonacci()
            && level_slot(id)
                .is_some_and(|i| (self.fibonacci.levels.len()..MAX_FIB_LEVELS).contains(&i))
        {
            return Ok(());
        }
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
        if !schema.accepts(&value) {
            return Err(crate::studies::StudyError::invalid_input(id));
        }
        let config = &mut self.fibonacci;
        match (id, value) {
            ("swing_size", InputValue::Choice(index)) => {
                self.swing_size = [SwingSize::Atr, SwingSize::Percent, SwingSize::Bars][index];
            }
            ("fork_transparency", InputValue::Length(transparency)) => {
                self.fork_transparency = transparency;
            }
            ("atr_multiple", InputValue::Number(multiple)) => self.atr_multiple = multiple,
            ("atr_period", InputValue::Length(period)) => {
                self.atr_period = period;
                self.atr = ATR::new(period);
            }
            ("fib_show_prices", InputValue::Choice(index)) => {
                config.show_prices = [None, Some(false), Some(true)][index]
            }
            ("fib_show_levels", InputValue::Choice(index)) => {
                config.show_levels = [None, Some(false), Some(true)][index]
            }
            ("fib_levels_as_percent", InputValue::Toggle(on)) => config.levels_as_percent = on,
            ("fib_labels_on_left", InputValue::Toggle(on)) => config.labels_on_left = on,
            ("fib_reverse", InputValue::Toggle(on)) => config.reverse = on,
            ("fib_extend_left", InputValue::Toggle(on)) => config.extend_left = on,
            ("fib_extend_right", InputValue::Toggle(on)) => config.extend_right = on,
            ("fib_background", InputValue::Toggle(on)) => config.show_background = on,
            // The input bounds the opacity to 0..=255.
            ("fib_background_opacity", InputValue::Length(opacity)) => {
                config.background_opacity = opacity as u8;
            }
            ("fib_logarithmic", InputValue::Toggle(on)) => config.logarithmic = on,
            ("fib_level_count", InputValue::Length(count)) => {
                let defaults = self.kind.default_fibonacci().levels;
                let color = self.zigzag.colors()[0].to_srgba_unmultiplied();
                config.levels.truncate(count);
                while config.levels.len() < count {
                    let level = defaults
                        .get(config.levels.len())
                        .cloned()
                        .unwrap_or_else(|| FibonacciLevel::new(1.0, "100%", color));
                    config.levels.push(level);
                }
            }
            (id, value) => {
                let level = &mut config.levels[level_slot(id)
                    .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?];
                match (id.rsplit_once('_').map(|(field, _)| field), value) {
                    (Some("fib_visible"), InputValue::Toggle(on)) => level.visible = on,
                    (Some("fib_ratio"), InputValue::Number(ratio)) => level.value = ratio as f32,
                    (Some("fib_label"), InputValue::Text(label)) => level.label = label,
                    (Some("fib_color"), InputValue::Color(color)) => {
                        level.color = color.to_srgba_unmultiplied();
                    }
                    (Some("fib_price"), InputValue::Toggle(on)) => level.show_price = on,
                    _ => return Err(crate::studies::StudyError::UnknownInput(id.into())),
                }
            }
        }
        self.calculate(&[]);
        Ok(())
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            // Clear the old anchors by their exact source indices before the swings move.
            for &bar in &self.anchors {
                if bar < self.values.len() {
                    self.values.set(bar, IndicatorValue::None);
                }
            }
            self.values.resize(data.len(), IndicatorValue::None);
        }
        self.anchors.clear();
        self.drawings.clear();
        let recent: Vec<ZigZagPivot> = if self.swing_size == SwingSize::Bars {
            let half = (self.zigzag.depth() / 2).max(1);
            // shortcut: the stepper copies every pivot on each update, about 50 bytes a pivot;
            // keep a checkpoint of the newest pivot only if long histories make ticks slow.
            self.bar_swings
                .run(data, from, &mut IndicatorValues::new(), |swings, bar| {
                    if bar.high.is_finite() && bar.low.is_finite() {
                        swings.push(bar, half, half, usize::MAX);
                    } else {
                        swings.skip();
                    }
                    IndicatorValue::None
                });
            let pivots = &self.bar_swings.state().pivots;
            let Some(anchors) = extension_anchors(pivots.len(), |i| ZigZagPivot {
                bar_index: pivots[i].index,
                price: pivots[i].price,
                is_high: pivots[i].high,
            }) else {
                return;
            };
            anchors.to_vec()
        } else {
            if self.swing_size == SwingSize::Atr {
                self.atr.update(data, from);
            }
            let (atr, multiple) = (&self.atr, self.atr_multiple);
            let by_atr = |source: usize| {
                atr.values()
                    .value(0, source)
                    .map_or(f64::NAN, |atr| 100.0 * multiple * atr / data[source].close)
            };
            self.zigzag.update_swings(
                data,
                from,
                (self.swing_size == SwingSize::Atr).then_some(&by_atr as &dyn Fn(usize) -> f64),
            );
            let pivots = self.zigzag.pivots();
            // The retracement and extension leave out the last pivot, which still moves.
            let pivots = match self.kind {
                AutoDrawingKind::FibonacciRetracement | AutoDrawingKind::FibonacciExtension => {
                    &pivots[..pivots.len().saturating_sub(1)]
                }
                _ => pivots,
            };
            if self.kind == AutoDrawingKind::FibonacciExtension {
                let Some(anchors) = extension_anchors(pivots.len(), |i| pivots[i]) else {
                    return;
                };
                anchors.to_vec()
            } else {
                let needed = if self.kind == AutoDrawingKind::FibonacciRetracement {
                    2
                } else {
                    3
                };
                let Some(start) = pivots.len().checked_sub(needed) else {
                    return;
                };
                pivots[start..].to_vec()
            }
        };
        let order: &[usize] = match self.kind {
            AutoDrawingKind::ParallelChannel => &[0, 2, 1],
            AutoDrawingKind::FibonacciRetracement => &[0, 1],
            AutoDrawingKind::FibonacciExtension | AutoDrawingKind::Pitchfork => &[0, 1, 2],
        };
        let mut drawing = Drawing::new(0, self.kind.tool());
        drawing.completed = true;
        drawing.color = self.zigzag.colors()[0].to_srgba_unmultiplied();
        drawing.chart_points = order
            .iter()
            .map(|&i| {
                let pivot = recent[i];
                self.values.set_value(0, pivot.bar_index, pivot.price);
                self.anchors.push(pivot.bar_index);
                ChartPoint::new(pivot.bar_index as f32, pivot.price)
            })
            .collect();
        if self.kind.fibonacci() {
            drawing.fib_config = Some(self.fibonacci.clone());
        }
        if self.kind == AutoDrawingKind::Pitchfork {
            drawing.pitchfork = Some(self.fork_config());
        }
        self.drawings.push(drawing);
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn drawings(&self) -> &[Drawing] {
        &self.drawings
    }
    fn colors(&self) -> Vec<Color32> {
        let mut colors = self.zigzag.colors();
        if self.kind == AutoDrawingKind::Pitchfork {
            colors.extend(self.fork_colors);
        }
        colors
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if self.kind == AutoDrawingKind::Pitchfork
            && let Some(&color) = colors.first()
        {
            self.fork_colors = [
                colors.get(1).copied().unwrap_or(color),
                colors.get(2).copied().unwrap_or(color),
            ];
        }
        self.zigzag.set_colors(colors);
        if self.kind == AutoDrawingKind::Pitchfork {
            let config = self.fork_config();
            for drawing in &mut self.drawings {
                drawing.pitchfork = Some(config.clone());
            }
        }
    }
    fn set_color(&mut self, line: usize, color: Color32) {
        if line == 0 {
            self.set_colors(vec![color]);
        }
    }
    fn is_overlay(&self) -> bool {
        true
    }
    fn is_visible(&self) -> bool {
        self.zigzag.is_visible()
    }
    fn set_visible(&mut self, visible: bool) {
        self.zigzag.set_visible(visible);
    }
    // Its drawings sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{ConfiguredStudy, IndicatorFactory};

    fn bars() -> Vec<Bar> {
        // At deviation 5%, depth 3: confirmed lows 100 at 0 and 5, high 113 at 3;
        // the high 118 at 9 is still moving. The reversal confirms bar 5 on bar 6.
        [
            (101.0, 100.0),
            (105.0, 104.0),
            (108.0, 107.0),
            (113.0, 112.0),
            (110.0, 104.0),
            (103.0, 100.0),
            (107.0, 106.0),
            (110.0, 109.0),
            (114.0, 113.0),
            (118.0, 117.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (h, l))| {
            Bar::new(
                chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                l,
                h,
                l,
                l,
                1.0,
            )
        })
        .collect()
    }

    /// Correct anchors, completed swings and replacement on live changes at the study API.
    #[test]
    fn automatic_drawings_follow_shared_swings() {
        for (kind, tool, initial, updated) in [
            (
                AutoDrawingKind::ParallelChannel,
                DrawingToolType::ParallelChannel,
                vec![(3, 113.0), (9, 118.0), (5, 100.0)],
                vec![(5, 100.0), (12, 93.0), (9, 122.0)],
            ),
            (
                AutoDrawingKind::FibonacciRetracement,
                DrawingToolType::FibonacciRetracement,
                vec![(3, 113.0), (5, 100.0)],
                vec![(5, 100.0), (9, 122.0)],
            ),
            // The moving high at bar 9 takes no part; once confirmed, its leg takes back 22 of
            // 13 and the extension steps back to the swing before.
            (
                AutoDrawingKind::FibonacciExtension,
                DrawingToolType::FibonacciExtension,
                vec![(0, 100.0), (3, 113.0), (5, 100.0)],
                vec![(0, 100.0), (3, 113.0), (5, 100.0)],
            ),
            (
                AutoDrawingKind::Pitchfork,
                DrawingToolType::Pitchfork,
                vec![(3, 113.0), (5, 100.0), (9, 118.0)],
                vec![(5, 100.0), (9, 122.0), (12, 93.0)],
            ),
        ] {
            let mut data = bars();
            let name = AutoDrawing::new(kind, 5.0, 3).name().to_owned();
            let mut study = ConfiguredStudy::new(IndicatorFactory::new().create(&name).unwrap());
            study
                .set_input("swing_size", InputValue::Choice(1))
                .unwrap();
            study.set_input("depth", InputValue::Length(3)).unwrap();
            if kind == AutoDrawingKind::Pitchfork {
                let input = study
                    .inputs()
                    .into_iter()
                    .find(|i| i.id == "fork_transparency")
                    .unwrap();
                assert_eq!(input.value, InputValue::Length(85));
                assert!(
                    study
                        .set_input("fork_transparency", InputValue::Length(101))
                        .is_err()
                );
                study
                    .set_input("fork_transparency", InputValue::Length(50))
                    .unwrap();
            }
            let check = |study: &ConfiguredStudy, expected: &[(usize, f64)]| {
                assert_eq!(study.drawings().len(), 1, "{name}");
                let drawing = &study.drawings()[0];
                assert_eq!(drawing.tool_type, tool);
                let points: Vec<_> = drawing
                    .chart_points
                    .iter()
                    .map(|p| (p.bar_idx as usize, p.price))
                    .collect();
                assert_eq!(points, expected, "{name}");
                if kind == AutoDrawingKind::Pitchfork {
                    assert_eq!(drawing.pitchfork.as_ref().unwrap().background_opacity, 128);
                }
                if kind.fibonacci() {
                    let config = drawing.fib_config.as_ref().unwrap();
                    assert_eq!(config.levels.len(), 24, "{name}: listed levels");
                    assert_eq!(
                        config
                            .visible_levels()
                            .map(|level| level.value)
                            .collect::<Vec<_>>(),
                        [
                            0.0, 0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.618, 2.618, 3.618, 4.236
                        ]
                    );
                    assert!(config.show_background);
                    assert_eq!(config.background_opacity, 51);
                }
                if kind == AutoDrawingKind::FibonacciRetracement {
                    let mut drawing = drawing.clone();
                    drawing.update_screen_coords(|b| b, |p| p as f32);
                    // 0 at the end, 50% at the arithmetic midpoint, 100% at the start.
                    assert_eq!(drawing.level_ys[0], expected[1].1 as f32);
                    assert_eq!(
                        drawing.level_ys[3],
                        ((expected[0].1 + expected[1].1) / 2.0) as f32
                    );
                    assert_eq!(drawing.level_ys[6], expected[0].1 as f32);
                }
            };
            study.calculate(&data[..1]);
            assert!(study.drawings().is_empty());
            study.calculate(&data);
            check(&study, &initial);
            data[9].high = 122.0;
            study.update(&data, 9);
            let moved: Vec<_> = initial
                .iter()
                .map(|&(bar, p)| (bar, if bar == 9 { 122.0 } else { p }))
                .collect();
            check(&study, &moved);
            for (i, low) in [95.0, 94.0, 93.0].into_iter().enumerate() {
                data.push(Bar::new(
                    chrono::DateTime::from_timestamp((10 + i) as i64 * 60, 0).unwrap(),
                    low,
                    100.0,
                    low,
                    low,
                    1.0,
                ));
            }
            study.update(&data, 10);
            check(&study, &updated);
            study
                .set_input("deviation_percent", InputValue::Number(50.0))
                .unwrap();
            study.calculate(&data);
            assert!(study.drawings().is_empty());
            study
                .set_input("deviation_percent", InputValue::Number(5.0))
                .unwrap();
            study.set_input("depth", InputValue::Length(100)).unwrap();
            study.calculate(&data);
            assert!(study.drawings().is_empty());
            study.calculate(&[]);
            assert!(study.drawings().is_empty() && study.values().is_empty());
        }
    }

    /// The catalogue sizes swings by 3 times the 10-bar average true range as a percentage of
    /// the confirming close, so 0.3% minute swings draw and daily anchors match a hand count,
    /// calculated at once or bar by bar. Its extension finds the same swings by 5 bars on each
    /// side, all confirmed: each last leg takes back 100% and 16 / 17 of the one before.
    #[test]
    fn automatic_drawings_size_swings_by_average_true_range() {
        let series = |count: usize, seconds: i64, h: f64, mid: &dyn Fn(usize) -> f64| {
            (0..count)
                .map(|i| {
                    let mid = mid(i);
                    let time = chrono::DateTime::from_timestamp(i as i64 * seconds, 0).unwrap();
                    Bar::new(time, mid, mid + h, mid - h, mid, 1.0)
                })
                .collect::<Vec<_>>()
        };
        // Lows of 99.984375 at bars 0, 20 and 40, highs of 100.328125 at 10 and 30: 0.34%
        // swings against a threshold near 300 * 0.0469 / 100.2 = 0.14%.
        let minute = series(50, 60, 1.0 / 64.0, &|i| {
            let t = if i % 20 < 10 { i % 20 } else { 20 - i % 20 };
            100.0 + t as f64 / 32.0
        });
        // Every true range is 2, so a reversal needs 600 / close percent: the fall from 111 at
        // bar 40 reaches 5.518% at bar 51 against 5.667%, and the last pivot stays provisional.
        let daily = series(52, 86_400, 1.0, &|i| match i {
            0..=11 => 100.0 + i as f64,
            12..=26 => 111.0 - (i - 11) as f64,
            27..=40 => 96.0 + (i - 26) as f64,
            _ => 110.0 - 0.375 * (i - 40) as f64,
        });
        let (low, high) = (99.984375, 100.328125);
        for (label, bars, split, expected) in [
            (
                "minute",
                minute,
                25,
                [
                    vec![(20, low), (40, low), (30, high)],
                    vec![(20, low), (30, high)],
                    vec![(20, low), (30, high), (40, low)],
                    vec![(20, low), (30, high), (40, low)],
                ],
            ),
            (
                "daily",
                daily,
                45,
                [
                    vec![(11, 112.0), (40, 111.0), (26, 95.0)],
                    vec![(11, 112.0), (26, 95.0)],
                    vec![(11, 112.0), (26, 95.0), (40, 111.0)],
                    vec![(11, 112.0), (26, 95.0), (40, 111.0)],
                ],
            ),
        ] {
            for (kind, expected) in AutoDrawingKind::ALL.into_iter().zip(expected) {
                let name = AutoDrawing::new(kind, 5.0, 10).name().to_owned();
                let mut study = IndicatorFactory::new().create(&name).unwrap();
                let points = |study: &dyn Indicator| -> Vec<(usize, f64)> {
                    study.drawings()[0]
                        .chart_points
                        .iter()
                        .map(|p| (p.bar_idx as usize, p.price))
                        .collect()
                };
                study.calculate(&bars);
                assert_eq!(points(study.as_ref()), expected, "{label} {name}");
                study.calculate(&bars[..split]);
                study.update(&bars, split);
                assert_eq!(points(study.as_ref()), expected, "{label} {name} updated");
            }
        }
    }

    /// A configured automatic line is painted and selectable through the ordinary study path.
    #[test]
    fn automatic_drawing_uses_the_study_renderer_and_hit_test() {
        use crate::chart::coords::ChartMapping;
        use crate::chart::indicators::hit_test_indicator;
        use crate::chart::rendering::{IndicatorRenderer, RenderContext};
        use crate::scales::{PriceMap, PriceRange, PriceTransform};
        use egui::{Pos2, Rect};
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(90.0, 130.0),
            0.0,
            300.0,
            false,
        );
        let coords = ChartMapping::new(rect, 30.0, 0, 9, 0.0, price);
        let data = bars();
        let mut automatic = AutoDrawing::new(AutoDrawingKind::ParallelChannel, 5.0, 3);
        let mut line = Drawing::new(0, DrawingToolType::TrendLine);
        line.completed = true;
        line.chart_points = vec![ChartPoint::new(3.0, 113.0), ChartPoint::new(9.0, 118.0)];
        automatic.drawings = vec![line];
        let a = Pos2::new(coords.idx_to_x(3), price.price_to_y(113.0));
        let b = Pos2::new(coords.idx_to_x(9), price.price_to_y(118.0));
        let midpoint = a.lerp(b, 0.5);
        for (drawing_visible, plot_visible) in [(true, true), (true, false), (false, true)] {
            automatic.drawings[0].visible = drawing_visible;
            let mut study = ConfiguredStudy::new(Box::new(automatic.clone()));
            let mut settings = study.settings();
            settings.plots[0].visible = plot_visible;
            study.apply_settings(&[], settings).unwrap();
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    &study,
                    &data,
                    0,
                    &price,
                    &coords,
                );
            });
            let painted = output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::LineSegment{points,..} if points[0].distance(a)<0.01 && points[1].distance(b)<0.01));
            // Anchors scale the study; only the construction, not their plot geometry, is painted.
            assert!(
                !output
                    .shapes
                    .iter()
                    .any(|s| matches!(s.shape, egui::Shape::Circle(_) | egui::Shape::Path(_)))
            );
            output.drop_without_applying_deltas();
            let visible = drawing_visible && plot_visible;
            assert_eq!(painted, visible);
            assert_eq!(
                hit_test_indicator(midpoint, &study, 0, 0..10, &coords, |p| price.price_to_y(p))
                    .is_some(),
                visible
            );
            assert_eq!(
                crate::studies::plots::drawing_prices(&study, 0..10)
                    .next()
                    .is_some(),
                visible
            );
        }

        // The study's colours are drawn as its inputs set them on every theme, so a saved study
        // keeps its look: the 0.382 level stays orange on the light chart.
        let mut retracement = AutoDrawing::new(AutoDrawingKind::FibonacciRetracement, 5.0, 3);
        retracement.calculate(&data);
        let study = ConfiguredStudy::new(Box::new(retracement));
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::light());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            IndicatorRenderer::render_study(
                &RenderContext::new(ui.painter(), rect),
                &study,
                &data,
                0,
                &price,
                &coords,
            );
        });
        let orange = egui::Color32::from_rgb(255, 152, 0);
        let painted = output.shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == orange)
        });
        output.drop_without_applying_deltas();
        assert!(painted);

        // Zoom between anchors: the construction still supplies the independent scale's range.
        automatic.drawings[0].visible = true;
        let mut study = ConfiguredStudy::new(Box::new(automatic));
        study.set_scale(crate::studies::StudyScale::Independent);
        let zoomed = ChartMapping::new(rect, 400.0, 6, 7, 0.0, price);
        let mapped = crate::chart::rendering::study_mapping(&study, &zoomed);
        for value in [113.0, 118.0] {
            assert!((0.0..=300.0).contains(&mapped.price_to_y(value)));
        }

        // Projected levels must fit even when none of the swing anchors are in view.
        let mut extension = AutoDrawing::new(AutoDrawingKind::FibonacciExtension, 5.0, 3);
        extension.calculate(&data);
        let prices: Vec<_> = crate::studies::plots::drawing_prices(&extension, 10..12).collect();
        // C + 2.618*(B-A) = 100 + 2.618*(113-100) = 134.034.
        assert!(prices.iter().any(|p| (p - 134.034).abs() < 1e-4));
        let mut state = crate::studies::ChartState::new(crate::model::BarData::from_bars(data));
        state.time_scale_mut().set_width(400.0);
        let transform = state.fit_price_scale([&extension as &dyn Indicator]);
        let range = state.price_scale().range();
        // The 4.236 level: 100 + 4.236*13 = 155.068.
        assert!(transform.to_price(range.min) <= 100.0);
        assert!(transform.to_price(range.max) >= 155.068);
        let mut extension = ConfiguredStudy::new(Box::new(extension));
        let mut hidden = extension.settings();
        hidden.plots[0].visible = false;
        extension.apply_settings(&[], hidden).unwrap();
        assert!(
            crate::studies::plots::drawing_prices(&extension, 10..12)
                .next()
                .is_none()
        );

        // Without bands or extensions, only A..B is painted; C is the projection price.
        let mut extension = AutoDrawing::new(AutoDrawingKind::FibonacciExtension, 5.0, 3)
            .with_fibonacci_config(FibonacciConfig {
                show_background: false,
                extend_right: false,
                ..FibonacciConfig::extension_default()
            });
        extension.calculate(&bars());
        for (visible, reaches) in [(0..3, true), (3..5, true), (5..6, false), (6..9, false)] {
            assert_eq!(
                crate::studies::plots::drawing_prices(&extension, visible.clone())
                    .next()
                    .is_some(),
                reaches,
                "{visible:?}"
            );
        }
    }

    /// A study's trend line, ray, extended line, info line or trend angle extends past the anchor each side
    /// names. Drawn right to left from bar 9 to bar 3, Right runs on past bar 3 towards bar 0
    /// and Left past bar 9 towards later bars, so its prices count in a view over bars 0..2 or
    /// in one over bars 20..30, never both, and always in bars 4..6 between its anchors. A
    /// vertical one at bar 5 extends along the price axis and reaches neither view. A rectangle
    /// extends towards the pane's sides whichever way it was drawn.
    #[test]
    fn study_lines_drawn_right_to_left_scale_to_the_side_they_extend() {
        use crate::drawings::DrawingToolType as T;
        let (reversed, forward) = ([(9.0, 118.0), (3.0, 113.0)], [(3.0, 113.0), (9.0, 118.0)]);
        let vertical = [(5.0, 113.0), (5.0, 118.0)];
        let (right, left) = ((false, true), (true, false));
        // Counted in bars 0..2 and in bars 20..30: for the lines, then for the rectangle.
        for (anchors, flags, lines, sides) in [
            (reversed, right, (true, false), (false, true)),
            (reversed, left, (false, true), (true, false)),
            (forward, right, (false, true), (false, true)),
            (forward, left, (true, false), (true, false)),
            (vertical, right, (false, false), (false, true)),
            (vertical, left, (false, false), (true, false)),
        ] {
            for tool in [
                T::TrendLine,
                T::Ray,
                T::ExtendedLine,
                T::InfoLine,
                T::TrendAngle,
                T::Rect,
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.chart_points = anchors
                    .iter()
                    .map(|&(bar, price)| ChartPoint::new(bar, price))
                    .collect();
                (drawing.extend_left, drawing.extend_right) = flags;
                drawing.completed = true;
                let mut study = AutoDrawing::new(AutoDrawingKind::ParallelChannel, 5.0, 3);
                study.drawings = vec![drawing];
                let (before, after) = if tool == T::Rect { sides } else { lines };
                for (bars, visible) in [(0..2, before), (4..6, true), (20..30, after)] {
                    let prices: Vec<_> =
                        crate::studies::plots::drawing_prices(&study, bars.clone()).collect();
                    let want = if visible {
                        anchors.map(|(_, price)| price).to_vec()
                    } else {
                        Vec::new()
                    };
                    assert_eq!(prices, want, "{tool:?} {anchors:?} {flags:?} {bars:?}");
                }
            }
        }
    }

    /// The extension anchors on the latest three confirmed pivots whose last leg takes back
    /// 23.6% to 100% of the leg before it, which the catalogue's study finds by bars alone:
    /// small regular swings draw, a leg past the start of the one before draws nothing, a new
    /// extreme of the forming bar moves nothing, and a study saved with ATR swings keeps them.
    #[test]
    fn extension_anchors_on_confirmed_swings_that_retrace_their_leg() {
        let series = |count: usize, close: &dyn Fn(usize) -> f64| {
            (0..count)
                .map(|i| {
                    let c = close(i);
                    let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                    Bar::new(time, c, c + 0.5, c - 0.5, c, 1.0)
                })
                .collect::<Vec<_>>()
        };
        // Swing high 120.5 at bar 20, low 118 at 26 and high 119.9 at 32, each confirmed 5 bars
        // later: the last leg takes back 1.9 of the first leg's 2.5, 76%.
        let small = series(40, &|i| match i {
            0..=20 => 100.0 + i as f64,
            21..=26 => 120.0 - 0.25 * (i - 20) as f64,
            27..=32 => 118.5 + 0.15 * (i - 26) as f64,
            _ => 119.4 - 0.1 * (i - 32) as f64,
        });
        // Highs 130.5 at bar 15 and 140.5 at 45 around the low 109.5 at 30: the last leg takes
        // back 31 of the first leg's 21. The low at bar 0 has no bars before it.
        let overshoot = series(56, &|i| match i {
            0..=15 => 100.0 + 2.0 * i as f64,
            16..=30 => 130.0 - 4.0 * (i - 15) as f64 / 3.0,
            31..=45 => 110.0 + 2.0 * (i - 30) as f64,
            _ => 140.0 - 0.8 * (i - 45) as f64,
        });
        let anchors = |study: &dyn Indicator| -> Vec<(usize, f64)> {
            study
                .drawings()
                .iter()
                .flat_map(|drawing| &drawing.chart_points)
                .map(|p| (p.bar_idx as usize, p.price))
                .collect()
        };
        let near = |got: &[(usize, f64)], want: &[(usize, f64)]| {
            got.len() == want.len()
                && got
                    .iter()
                    .zip(want)
                    .all(|(g, w)| g.0 == w.0 && (g.1 - w.1).abs() < 1e-9)
        };
        let expected = [(20, 120.5), (26, 118.0), (32, 119.9)];
        let mut study = IndicatorFactory::new()
            .create("Auto Fibonacci Extension")
            .unwrap();
        study.calculate(&overshoot);
        assert_eq!(anchors(study.as_ref()), [], "overshoot");
        study.calculate(&small);
        assert!(
            near(&anchors(study.as_ref()), &expected),
            "{:?}",
            anchors(study.as_ref())
        );
        let (config, prices) = study.drawings()[0].fib_levels().unwrap();
        let one = config
            .visible_levels()
            .zip(prices)
            .find(|(level, _)| level.value == 1.0)
            .unwrap()
            .1;
        assert!((one - 117.4).abs() < 1e-9, "level 1 at {one}");
        // A new high, then a new low, on the forming bar is not confirmed and moves nothing.
        let mut forming = small.clone();
        for low in [forming[39].low, 100.0] {
            (forming[39].high, forming[39].low) = (130.0, low);
            study.update(&forming, 39);
            assert!(
                near(&anchors(study.as_ref()), &expected),
                "forming low {low}"
            );
        }
        // New studies find swings by bars, and reset to it.
        let swing_size = |study: &dyn Indicator| {
            let input = study
                .inputs()
                .into_iter()
                .find(|i| i.id == "swing_size")
                .unwrap();
            (input.value, input.default)
        };
        assert_eq!(
            swing_size(study.as_ref()),
            (InputValue::Choice(2), InputValue::Choice(2))
        );
        // Choice 0 is still the ATR multiple, so a study saved with it keeps Zig Zag swings,
        // which three average true ranges filter out of the small swings.
        study
            .set_input("swing_size", InputValue::Choice(0))
            .unwrap();
        study.calculate(&small);
        assert_eq!(anchors(study.as_ref()), []);
        assert!(
            study
                .set_input("swing_size", InputValue::Choice(3))
                .is_err()
        );
        let mut pitchfork = IndicatorFactory::new().create("Auto Pitchfork").unwrap();
        assert!(
            pitchfork
                .set_input("swing_size", InputValue::Choice(2))
                .is_err()
        );
    }
}
