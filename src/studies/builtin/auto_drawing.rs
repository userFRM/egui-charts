//! Drawing studies anchored on the shared Zig Zag swings.

use super::ZigZag;
use crate::drawings::{ChartPoint, Drawing, DrawingToolType, FibonacciConfig, FibonacciLevel};
use crate::model::Bar;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, StudyInput,
};
use egui::Color32;

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
pub enum AutoDrawingKind {
    /// Last two pivots of the same kind, including the moving last extreme.
    TrendLine,
    /// Last three pivots, with the base through the first and third and the parallel through the second.
    ParallelChannel,
    /// Last two confirmed pivots; the current, unfinished swing is excluded.
    FibonacciRetracement,
    /// Latest completed swing projected from the current leg's moving extreme.
    FibonacciExtension,
    /// Last three pivots, with the median from the first to the midpoint of the other two.
    Pitchfork,
}

impl AutoDrawingKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::TrendLine,
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
        let mut fibonacci = if self == Self::FibonacciExtension {
            FibonacciConfig::extension_default()
        } else {
            FibonacciConfig::default()
        };
        for level in &mut fibonacci.levels {
            let hex = match level.value {
                0.0 | 1.0 => [120, 123, 134, 255],
                0.236 | 2.618 => [244, 67, 54, 255],
                0.382 => [129, 199, 132, 255],
                0.5 => [76, 175, 80, 255],
                0.618 => [0, 150, 136, 255],
                0.786 => [100, 181, 246, 255],
                1.618 => [41, 98, 255, 255],
                _ => level.color,
            };
            level.color = hex;
            if level.value == 1.272 || level.value == 2.0 {
                level.visible = false;
            }
        }
        fibonacci.show_background = true;
        fibonacci.background_opacity = 38;
        fibonacci.extend_right = true;
        fibonacci
    }

    fn tool(self) -> DrawingToolType {
        match self {
            Self::TrendLine => DrawingToolType::TrendLine,
            Self::ParallelChannel => DrawingToolType::ParallelChannel,
            Self::FibonacciRetracement => DrawingToolType::FibonacciRetracement,
            Self::FibonacciExtension => DrawingToolType::FibonacciExtension,
            Self::Pitchfork => DrawingToolType::Pitchfork,
        }
    }
}

/// A drawing recalculated from host bars with the same pivots as [`ZigZag`].
///
/// Add through the normal study API or calculate directly and read [`Indicator::drawings`].
/// No drawing is emitted until enough pivots exist. A trend line and channel include the moving
/// last extreme; a retracement uses the latest completed swing, an extension projects that
/// swing from the current leg's extreme. A pitchfork follows the latest three pivots. Updating or replacing bars
/// rebuilds the anchors; generated drawings belong to the study, not the editable drawing list.
/// Output 0 contains anchor prices for scaling and readouts; the drawing renderer paints the
/// construction instead of joining these values. Its colour, width and visibility use plot 0.
#[derive(Clone)]
pub struct AutoDrawing {
    kind: AutoDrawingKind,
    zigzag: ZigZag,
    fibonacci: FibonacciConfig,
    fork_colors: [Color32; 2],
    drawings: Vec<Drawing>,
    values: IndicatorValues,
}

impl AutoDrawing {
    /// Construct with a Zig Zag deviation in percent and confirmation depth in bars.
    /// Parameters have the same bounds and skipped-bar policy as [`ZigZag::new`].
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
            fibonacci: kind.default_fibonacci(),
            fork_colors: [
                crate::studies::palette::GREEN,
                crate::studies::palette::BLUE,
            ],
            drawings: Vec::new(),
            values: IndicatorValues::new(),
        }
    }

    fn fork_config(&self) -> crate::drawings::PitchforkConfig {
        let mut config = crate::drawings::PitchforkConfig::default();
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
        config.background_opacity = 38;
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
            AutoDrawingKind::TrendLine => "Auto Trend Line",
            AutoDrawingKind::ParallelChannel => "Auto Parallel Channel",
            AutoDrawingKind::FibonacciRetracement => "Auto Fibonacci Retracement",
            AutoDrawingKind::FibonacciExtension => "Auto Fibonacci Extension",
            AutoDrawingKind::Pitchfork => "Auto Pitchfork",
        }
    }

    fn desc(&self) -> &str {
        "Drawing anchored on Zig Zag swing extremes"
    }

    /// The Zig Zag deviation and depth and, for a Fibonacci construction, its options, its number
    /// of levels and each level's visibility, ratio, label, colour and price label.
    fn inputs(&self) -> Vec<StudyInput> {
        let mut inputs: Vec<StudyInput> = self
            .zigzag
            .inputs()
            .into_iter()
            .filter(|input| input.id != "mode")
            .collect();
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
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if !id.starts_with("fib_") {
            if id == "mode" {
                return Err("unknown study input");
            }
            return self.zigzag.set_input(id, value);
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
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
        }
        let config = &mut self.fibonacci;
        match (id, value) {
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
                let level = &mut config.levels[level_slot(id).ok_or("unknown study input")?];
                match (id.rsplit_once('_').map(|(field, _)| field), value) {
                    (Some("fib_visible"), InputValue::Toggle(on)) => level.visible = on,
                    (Some("fib_ratio"), InputValue::Number(ratio)) => level.value = ratio as f32,
                    (Some("fib_label"), InputValue::Text(label)) => level.label = label,
                    (Some("fib_color"), InputValue::Color(color)) => {
                        level.color = color.to_srgba_unmultiplied();
                    }
                    (Some("fib_price"), InputValue::Toggle(on)) => level.show_price = on,
                    _ => return Err("unknown study input"),
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
            for pivot in self.zigzag.pivots().iter().rev().take(4) {
                if pivot.bar_index < self.values.len() {
                    self.values.set(pivot.bar_index, IndicatorValue::None);
                }
            }
            self.values.resize(data.len(), IndicatorValue::None);
        }
        self.zigzag.update_swings(data, from);
        self.drawings.clear();
        let pivots = self.zigzag.pivots();
        let pivots = if self.kind == AutoDrawingKind::FibonacciRetracement {
            &pivots[..pivots.len().saturating_sub(1)]
        } else {
            pivots
        };
        let needed = if self.kind == AutoDrawingKind::FibonacciRetracement {
            2
        } else {
            3
        };
        let Some(start) = pivots.len().checked_sub(needed) else {
            return;
        };
        let recent = &pivots[start..];
        let order: &[usize] = match self.kind {
            AutoDrawingKind::TrendLine => &[0, 2],
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
                AutoDrawingKind::TrendLine,
                DrawingToolType::TrendLine,
                vec![(3, 113.0), (9, 118.0)],
                vec![(5, 100.0), (12, 93.0)],
            ),
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
            (
                AutoDrawingKind::FibonacciExtension,
                DrawingToolType::FibonacciExtension,
                vec![(3, 113.0), (5, 100.0), (9, 118.0)],
                vec![(5, 100.0), (9, 122.0), (12, 93.0)],
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
            study.set_input("depth", InputValue::Length(3)).unwrap();
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

    /// A configured automatic line is painted and selectable through the ordinary study path.
    #[test]
    fn automatic_drawing_uses_the_study_renderer_and_hit_test() {
        use crate::chart::coords::ChartMapping;
        use crate::chart::indicators::hit_test_indicator;
        use crate::chart::renderers::{IndicatorRenderer, RenderContext};
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
        let mut automatic = AutoDrawing::new(AutoDrawingKind::TrendLine, 5.0, 3);
        automatic.calculate(&data);
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

        // Zoom between anchors: the construction still supplies the independent scale's range.
        automatic.drawings[0].visible = true;
        let mut study = ConfiguredStudy::new(Box::new(automatic));
        study.set_scale(crate::studies::StudyScale::Independent);
        let zoomed = ChartMapping::new(rect, 400.0, 6, 7, 0.0, price);
        let mapped = crate::chart::renderers::study_mapping(&study, &zoomed);
        for value in [113.0, 118.0] {
            assert!((0.0..=300.0).contains(&mapped.price_to_y(value)));
        }

        // Projected levels must fit even when none of the swing anchors are in view.
        let mut extension = AutoDrawing::new(AutoDrawingKind::FibonacciExtension, 5.0, 3);
        extension.calculate(&data);
        let prices: Vec<_> = crate::studies::plots::drawing_prices(&extension, 10..12).collect();
        // C + 2.618*(B-A) = 118 + 2.618*(100-113) = 83.966.
        assert!(prices.iter().any(|p| (p - 83.966).abs() < 1e-4));
        let mut state = crate::studies::ChartState::new(crate::model::BarData::from_bars(data));
        state.time_scale_mut().set_width(400.0);
        let transform = state.fit_price_scale([&extension as &dyn Indicator]);
        let range = state.price_scale().range();
        assert!(transform.to_price(range.min) <= 83.966);
        assert!(transform.to_price(range.max) >= 118.0);
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
        for (visible, reaches) in [
            (0..3, false),
            (3..5, true),
            (5..6, true),
            (6..9, false),
            (9..10, false),
        ] {
            assert_eq!(
                crate::studies::plots::drawing_prices(&extension, visible.clone())
                    .next()
                    .is_some(),
                reaches,
                "{visible:?}"
            );
        }
    }
}
