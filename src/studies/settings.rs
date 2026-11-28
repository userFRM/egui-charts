//! Applied study presentation and transactional input edits.

use super::{Indicator, IndicatorLineStyle, IndicatorValues, InputValue};
use crate::model::LineStyle;
use crate::model::{Bar, Instrument, Timeframe, TimeframeVisibility};
use egui::Color32;

/// Geometry for one output line.
///
/// Line, step line, step line with diamonds and area join the defined values on either side of
/// missing ones; the types with breaks leave a gap there instead.
///
/// Saved settings name the joined line, step line and area `JoinedLine`, `JoinedStepLine` and
/// `JoinedArea`. `Line`, `StepLine` and `Area` name the types with breaks, the drawing every
/// line, step line and area had before joined types existed, so earlier saves draw as they did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum PlotType {
    /// Connected values, joined across missing bars.
    #[default]
    #[serde(rename = "JoinedLine")]
    Line,
    /// Connected values, broken at missing bars.
    #[serde(rename = "Line")]
    LineWithBreaks,
    /// Bars from zero to the output value.
    Histogram,
    /// A circle at each defined value.
    Circles,
    /// Horizontal segments followed by a vertical change at the next defined value, joined
    /// across missing bars.
    #[serde(rename = "JoinedStepLine")]
    StepLine,
    /// Horizontal segments followed by a vertical change at the next bar, broken at missing
    /// bars.
    #[serde(rename = "StepLine")]
    StepLineWithBreaks,
    /// A step line joined across missing bars, with a diamond at its first value and at each
    /// change of value.
    StepLineWithDiamonds,
    /// Columns from zero, at 80% of the bar pitch.
    Columns,
    /// A plus at each defined value.
    Crosses,
    /// A short horizontal dash centred on the bar at each defined value, in the plot's width.
    Dashes,
    /// A line along the values over a fill of its colour at 20% of its opacity down to zero,
    /// joined across missing bars.
    #[serde(rename = "JoinedArea")]
    Area,
    /// A line along the values over a fill of its colour at 20% of its opacity down to zero,
    /// broken at missing bars.
    #[serde(rename = "Area")]
    AreaWithBreaks,
    /// An upward triangle on each defined value, centred on it or beside it as
    /// [`Indicator::plot_shape_side`](super::Indicator::plot_shape_side) chooses.
    TriangleUp,
    /// A downward triangle on each defined value, centred on it or beside it as
    /// [`Indicator::plot_shape_side`](super::Indicator::plot_shape_side) chooses.
    TriangleDown,
    /// Repaint the source price bars using this plot's per-bar colour.
    BarColor,
    /// Colour the full pane height at each defined bar.
    BackgroundColor,
    /// A character centred at each defined value in the plot colour, drawn at twice the sum of
    /// the stroke width and 5 logical pixels: 12 at the default width of 1. Saved as
    /// `{"Character":"d"}`.
    Character(char),
}

impl PlotType {
    /// Every plot type any plot can take, in the order a style editor lists them. A
    /// [`Character`](Self::Character) plot is offered for a plot whose study declares one, as
    /// [`Indicator::plot_character`](super::Indicator::plot_character) reports.
    pub const ALL: [Self; 16] = [
        Self::Line,
        Self::LineWithBreaks,
        Self::StepLine,
        Self::StepLineWithBreaks,
        Self::StepLineWithDiamonds,
        Self::Histogram,
        Self::Columns,
        Self::Area,
        Self::AreaWithBreaks,
        Self::Circles,
        Self::Crosses,
        Self::Dashes,
        Self::TriangleUp,
        Self::TriangleDown,
        Self::BarColor,
        Self::BackgroundColor,
    ];

    /// User-facing display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::LineWithBreaks => "Line with breaks",
            Self::StepLine => "Step line",
            Self::StepLineWithBreaks => "Step line with breaks",
            Self::StepLineWithDiamonds => "Step line with diamonds",
            Self::Histogram => "Histogram",
            Self::Columns => "Columns",
            Self::Area => "Area",
            Self::AreaWithBreaks => "Area with breaks",
            Self::Circles => "Circles",
            Self::Crosses => "Crosses",
            Self::Dashes => "Dashes",
            Self::TriangleUp => "Triangle up",
            Self::TriangleDown => "Triangle down",
            Self::BarColor => "Bar colour",
            Self::BackgroundColor => "Background colour",
            Self::Character(_) => "Character",
        }
    }

    /// Why a plot of this type cannot be drawn for a study, or `None` when it can.
    /// `overlay` is whether the study is drawn on the price chart, and `price_scale` whether it
    /// shares the price scale there, which a volume study does not.
    pub fn incompatibility(self, overlay: bool, price_scale: bool) -> Option<&'static str> {
        match self {
            Self::BarColor if !overlay => {
                Some("Bar colour repaints the price bars, so the study must be on the price chart.")
            }
            Self::Histogram | Self::Columns | Self::Area | Self::AreaWithBreaks
                if overlay && price_scale =>
            {
                Some(
                    "This type draws from zero, which is far from the prices on the price scale. \
                     Give the study its own scale first.",
                )
            }
            _ => None,
        }
    }

    /// Whether the plot joins the defined values on either side of missing ones.
    #[cfg(feature = "widget")]
    pub(crate) fn joins_gaps(self) -> bool {
        matches!(
            self,
            Self::Line | Self::StepLine | Self::StepLineWithDiamonds | Self::Area
        )
    }
}

/// Which side of its value a triangle plot draws each triangle on.
///
/// A study that marks bars at their high or low draws its triangles beyond that price, so a
/// mark never covers the bar it marks. Above and below are in prices: on an inverted scale a
/// triangle above its value is drawn under it on screen. See
/// [`Indicator::plot_shape_side`](super::Indicator::plot_shape_side).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShapeSide {
    /// Centred on the value.
    #[default]
    Centred,
    /// Wholly above the value, toward higher prices, for a mark on a bar's high.
    Above,
    /// Wholly below the value, toward lower prices, for a mark on a bar's low.
    Below,
}

/// Editable presentation of one output line.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PlotStyle {
    /// Plot colour.
    pub color: Color32,
    /// Stroke width in logical pixels, from 0.5 to 10.
    pub width: f32,
    /// Stroke pattern for line plots.
    pub pattern: LineStyle,
    /// Plot geometry.
    pub plot_type: PlotType,
    /// Whether the plot participates in rendering, hit testing and scaling.
    pub visible: bool,
}

/// Colour and thickness for an alternate state of a plot, such as decreasing bars.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct PlotVariant {
    /// Caption in the style editor.
    pub name: String,
    /// Colour in this state.
    pub color: Color32,
    /// Stroke width in logical pixels, from 0.5 to 10.
    pub width: f32,
}

impl PlotVariant {
    /// An alternate state with a one-pixel stroke.
    pub fn new(name: impl Into<String>, color: Color32) -> Self {
        Self {
            name: name.into(),
            color,
            width: 1.0,
        }
    }
}

/// Presentation of one declared band, independent of its boundary plots.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FillStyle {
    /// Colour when the first boundary is at least the second.
    pub color: Color32,
    /// Optional colour when the first boundary is below the second.
    pub inverted_color: Option<Color32>,
    /// Whether the band is drawn.
    pub visible: bool,
    /// Whether `visible` alone decides that the band is drawn. Styles saved by earlier releases
    /// lack it: their band was also hidden with a boundary plot, which applying them carries over
    /// to `visible`.
    #[serde(default)]
    pub(super) independent: bool,
}

impl FillStyle {
    /// A visible band with the same colour on either side of a crossing.
    ///
    /// ```
    /// use ecolor::Color32;
    /// use egui_charts::studies::FillStyle;
    ///
    /// let mut band = FillStyle::new(Color32::from_rgba_unmultiplied(33, 150, 243, 13));
    /// band.inverted_color = Some(Color32::from_rgba_unmultiplied(242, 54, 69, 13));
    /// band.visible = false;
    /// assert_eq!(band.color, Color32::from_rgba_unmultiplied(33, 150, 243, 13));
    /// ```
    pub fn new(color: Color32) -> Self {
        Self {
            color,
            inverted_color: None,
            visible: true,
            independent: true,
        }
    }
}

/// Independent colours and visibility of a declared vertical gradient.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct GradientStyle {
    /// Colours at the two values in the declared ramp, including their opacity.
    pub colors: [Color32; 2],
    /// Whether the fill is drawn, independently of its plot and guides.
    pub visible: bool,
}

/// Editable presentation of one reference level in a study's pane, such as 70 for RSI.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct LevelStyle {
    /// Value the level is drawn at, or `None` for the value the study declares, which can
    /// follow its inputs.
    pub value: Option<f64>,
    /// Line colour.
    pub color: Color32,
    /// Stroke pattern.
    pub pattern: LineStyle,
    /// Stroke width in logical pixels, from 0.5 to 10. Older settings use one pixel.
    #[serde(default = "default_level_width")]
    pub width: f32,
    /// Whether the guide stroke is drawn. A hidden boundary still bounds the fill.
    pub visible: bool,
    /// Draw a dotted value line independently of guide visibility. Off in older settings.
    #[serde(default)]
    pub price_line: bool,
}

fn default_level_width() -> f32 {
    1.0
}

/// The levels and fill a study declares, as the defaults of its level settings.
pub(super) fn declared_levels<S: Indicator + ?Sized>(
    study: &S,
) -> (Vec<LevelStyle>, bool, Color32) {
    let pane = study.pane();
    let levels = pane
        .levels
        .iter()
        .enumerate()
        .map(|(index, &(value, color, _))| {
            let (pattern, width) = study.level_stroke(index, value);
            LevelStyle {
                value: None,
                color,
                pattern,
                width,
                visible: true,
                price_line: study.level_price_line(index).is_some(),
            }
        })
        .collect();
    (levels, pane.fill_between_levels, study.level_fill_color())
}

/// The presentation a study declares, as the defaults of its settings.
pub(super) fn declared_settings<S: Indicator + ?Sized>(study: &S) -> StudySettings {
    let (levels, fill_between_levels, level_fill_color) = declared_levels(study);
    StudySettings {
        show_legend_values: true,
        user_edit_enabled: Some(study.user_edit_enabled()),
        help_url: study.help_url().map(str::to_owned),
        plots: (0..study.line_count())
            .map(|line| study.plot_style(line))
            .collect(),
        precision: study.precision(),
        hidden_intervals: Vec::new(),
        timeframe_visibility: TimeframeVisibility::all(),
        levels,
        fill_between_levels,
        level_fill_color,
        mfi_auto_range: false,
        tsi_unit_range: false,
        untagged_plots: (0..study.line_count())
            .filter(|&line| !study.plot_tagged(line))
            .collect(),
        price_line_plots: (0..study.line_count())
            .filter(|&line| study.plot_price_line(line))
            .collect(),
        plot_variants: (0..study.line_count())
            .map(|line| study.plot_variants(line))
            .collect(),
        gradients: study
            .gradients()
            .iter()
            .map(|fill| GradientStyle {
                colors: fill.colors,
                visible: true,
            })
            .collect(),
        background: study.background(),
        background_visible: true,
        fills: study
            .fills()
            .iter()
            .map(|fill| FillStyle {
                inverted_color: fill.inverted_color,
                ..FillStyle::new(fill.color)
            })
            .collect(),
    }
}

/// Missing style lists inherit the study's declared presentation.
fn match_declared_styles(study: &dyn Indicator, settings: &mut StudySettings) {
    let (levels, fill, color) = declared_levels(study);
    // The built-in Williams %R, saved with two levels or none before its middle level. Its values
    // are compared so that a host study under the same name keeps its own levels.
    let williams_r = study.name() == "Williams %R"
        && study
            .pane()
            .levels
            .iter()
            .map(|level| level.0)
            .eq([-20.0, -80.0, -50.0]);
    // The built-in Trend Strength Index, saved with its zero guide alone (or none) before it
    // drew guides at 1 and -1, shading and two decimals.
    let older_tsi = study.name() == "Trend Strength Index"
        && study
            .pane()
            .levels
            .iter()
            .map(|level| level.0)
            .eq([0.0, 1.0, -1.0])
        && settings.levels.len() < 2;
    // The built-in Choppiness Index, saved with its two bands before its middle line.
    let chop = study.name() == "CHOP"
        && study
            .pane()
            .levels
            .iter()
            .map(|level| level.0)
            .eq([61.8, 38.2, 50.0]);
    if (study.name() == "Stochastic"
        || (study.name() == "MFI" && guides_since(study))
        || williams_r
        || chop)
        && settings.levels.len() == 2
        && levels.len() == 3
    {
        // Keep the two saved guides and the old appearance when adding the middle guide.
        let mut middle = levels[2];
        middle.visible = false;
        settings.levels.push(middle);
    } else if williams_r && settings.levels.is_empty() {
        // Saved before its levels were editable, it drew both bands and their fill.
        settings.levels = levels;
        settings.levels[2].visible = false;
        settings.fill_between_levels = fill;
        settings.level_fill_color = color;
    } else if matches!(
        study.name(),
        "KST"
            | "CMF"
            | "Force Index"
            | "Advance/Decline Ratio (Bars)"
            | "Volume Delta"
            | "Cumulative Volume Delta"
            | "PVO"
    ) && settings.levels.is_empty()
        && levels.len() == 1
    {
        // Saved before it had a guide, the study keeps drawing none until the guide is shown.
        let mut guide = levels[0];
        guide.visible = false;
        settings.levels.push(guide);
    } else if study.name() == "Aroon Oscillator" && settings.levels.len() < 2 && guides_since(study)
    {
        // Keep the saved zero style at its new position and add the outer guides hidden.
        let zero = settings.levels.first().copied().unwrap_or(levels[1]);
        settings.levels = vec![
            LevelStyle {
                visible: false,
                ..levels[0]
            },
            zero,
            LevelStyle {
                visible: false,
                ..levels[2]
            },
        ];
        settings.fill_between_levels = false;
    } else if older_tsi {
        // Keep the saved zero style, add the band guides hidden and keep reading three decimals.
        let zero = settings.levels.first().copied().unwrap_or(levels[0]);
        settings.levels = vec![
            zero,
            LevelStyle {
                visible: false,
                ..levels[1]
            },
            LevelStyle {
                visible: false,
                ..levels[2]
            },
        ];
        settings.precision.get_or_insert(3);
        settings.tsi_unit_range = true;
    } else if settings.levels.is_empty() && guides_since(study) {
        // Older saves drew no guides or band; showing them later uses the declared styles.
        settings.levels = levels
            .into_iter()
            .map(|level| LevelStyle {
                visible: false,
                ..level
            })
            .collect();
        settings.fill_between_levels = false;
        settings.level_fill_color = color;
        settings.mfi_auto_range = study.name() == "MFI";
    } else if settings.levels.len() != levels.len()
        && former_guides(study, settings.levels.len()).is_none()
    {
        settings.levels = levels;
        settings.fill_between_levels = fill;
        settings.level_fill_color = color;
    }
    settings
        .plot_variants
        .resize_with(study.line_count(), Vec::new);
    for (line, variants) in settings.plot_variants.iter_mut().enumerate() {
        let declared = study.plot_variants(line);
        if variants.len() != declared.len() {
            *variants = declared;
            // Saved before this plot had alternate states, all its bars used the plot's width.
            if let Some(plot) = settings.plots.get(line) {
                for variant in variants.iter_mut() {
                    variant.width = plot.width;
                    // These older single-colour styles drew every value in the plot colour.
                    let one_colour = match study.name() {
                        "Trend Strength Index" => true,
                        "Aroon Oscillator" => plot.color != super::palette::GREEN,
                        _ => false,
                    };
                    if one_colour {
                        variant.color = plot.color;
                    }
                }
            }
        }
    }
    // Saved band styles are kept whatever the study declares so far, since a study can declare
    // a band only once it has data. Bands beyond them start with the study's own style.
    let gradients = study.gradients();
    let saved = settings.gradients.len().min(gradients.len());
    settings
        .gradients
        .extend(gradients[saved..].iter().map(|fill| GradientStyle {
            colors: fill.colors,
            // Older Trend Strength Index saves drew no shading.
            visible: !older_tsi,
        }));
    let fills = study.fills();
    let saved = settings.fills.len().min(fills.len());
    settings
        .fills
        .extend(fills[saved..].iter().map(|fill| FillStyle {
            inverted_color: fill.inverted_color,
            ..FillStyle::new(fill.color)
        }));
}

/// Guides as [`StudyPane::levels`](super::StudyPane::levels) declares them.
type Guides = Vec<(f64, Color32, &'static str)>;

/// The guides a built-in study declared before it opened with fewer, and the levels its band
/// filled between, for settings saved with that many levels. A host study under the same name,
/// which declares other guides, keeps its own.
fn former_guides(study: &dyn Indicator, saved: usize) -> Option<(Guides, Option<[usize; 2]>)> {
    use super::palette::{GRAY, MUTED};
    // The guides each built-in study declares now, written out because the built-in studies are
    // compiled only with the `studies` feature.
    let (current, former, fill) = match (study.name(), saved) {
        ("Connors RSI", 3) => (
            vec![(70.0, MUTED, "Upper guide"), (30.0, MUTED, "Lower guide")],
            vec![
                (70.0, MUTED, "Upper guide"),
                (50.0, MUTED.gamma_multiply_u8(128), "Middle guide"),
                (30.0, MUTED, "Lower guide"),
            ],
            Some([0, 2]),
        ),
        ("Correlation Coefficient", 3) => (
            vec![(0.0, GRAY.gamma_multiply_u8(128), "Zero")],
            vec![
                (1.0, GRAY, "Upper guide"),
                (-1.0, GRAY, "Lower guide"),
                (0.0, GRAY.gamma_multiply_u8(128), "Zero"),
            ],
            None,
        ),
        _ => return None,
    };
    (study.pane().levels == current).then_some((former, fill))
}

/// Whether a built-in gained guides after settings were saved without them. Compare its
/// declared values so a host study under the same name keeps its own guide handling.
fn guides_since(study: &dyn Indicator) -> bool {
    // Built-in types are available only with the `studies` feature.
    let since: &[f64] = match study.name() {
        "%B" => &[1.0, 0.0, 0.5],
        "CCI" => &[100.0, -100.0, 0.0],
        "MFI" => &[80.0, 20.0, 50.0],
        "Aroon Oscillator" => &[90.0, 0.0, -90.0],
        _ => return false,
    };
    study
        .pane()
        .levels
        .iter()
        .map(|level| level.0)
        .eq(since.iter().copied())
}

/// Reads [`StudySettings::timeframe_visibility`], or the interval range that settings saved
/// before it hold: every interval, the saved interval only, or also those above or below it.
fn saved_timeframe_visibility<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TimeframeVisibility, D::Error> {
    #[derive(serde::Deserialize)]
    enum SavedIntervalRange {
        All,
        AndAbove(Timeframe),
        AndBelow(Timeframe),
        Only(Timeframe),
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum SavedVisibility {
        Range(SavedIntervalRange),
        Filter(TimeframeVisibility),
    }
    Ok(
        match <SavedVisibility as serde::Deserialize>::deserialize(deserializer)? {
            SavedVisibility::Range(SavedIntervalRange::All) => TimeframeVisibility::all(),
            SavedVisibility::Range(SavedIntervalRange::AndAbove(interval)) => {
                TimeframeVisibility::higher_intervals(interval)
            }
            SavedVisibility::Range(SavedIntervalRange::AndBelow(interval)) => {
                TimeframeVisibility::lower_intervals(interval)
            }
            SavedVisibility::Range(SavedIntervalRange::Only(interval)) => {
                TimeframeVisibility::current_interval(interval)
            }
            SavedVisibility::Filter(filter) => filter,
        },
    )
}

/// Host-serializable presentation settings of an applied study.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[non_exhaustive]
pub struct StudySettings {
    /// Show values in the status line when the chart enables study values. Defaults to true.
    #[serde(default = "enabled_by_default")]
    pub show_legend_values: bool,
    /// Allow user edits through settings, legend and context controls. Host APIs remain writable.
    /// `None` keeps the permission declared by the study.
    /// Read the effective permission through [`Indicator::user_edit_enabled`].
    #[serde(default)]
    pub user_edit_enabled: Option<bool>,
    /// Help article requested through the host. None uses the underlying study's metadata.
    #[serde(default)]
    pub help_url: Option<String>,
    /// One style per declared output line.
    pub plots: Vec<PlotStyle>,
    /// Decimal places, or the chart's default precision.
    pub precision: Option<usize>,
    /// Exact intervals on which the study is also hidden, such as those of settings saved
    /// before [`timeframe_visibility`](Self::timeframe_visibility). The study menu's visibility
    /// choices clear them.
    pub hidden_intervals: Vec<Timeframe>,
    /// The intervals the study is drawn on, as for a drawing: every interval, or exact interval
    /// labels and a range of values per unit. The study menu's visibility choices and the
    /// Visibility tab write it. Settings saved before it restore their interval range as the
    /// matching ranges, or every interval.
    #[serde(
        default = "TimeframeVisibility::all",
        alias = "interval_range",
        deserialize_with = "saved_timeframe_visibility"
    )]
    pub timeframe_visibility: TimeframeVisibility,
    /// One style per level of the study's pane, in the order of
    /// [`StudyPane::levels`](super::StudyPane::levels). Settings saved before levels were
    /// editable, and settings with another number of levels, take the study's own levels and fill.
    /// Only the guide declared since is added, hidden, to a Stochastic or Money Flow Index saved
    /// with two levels or a Know Sure Thing, Chaikin Money Flow, Force Index or Advance/Decline
    /// Ratio (Bars) saved with none. A Bollinger Bands %b, Commodity Channel Index or Money Flow
    /// Index saved with none restores its guides hidden and its band unfilled, and an Aroon
    /// Oscillator saved with its zero guide alone, or with none, keeps that guide and adds those
    /// at 90 and -90 hidden.
    /// A Connors RSI or a Correlation Coefficient with three levels keeps the guides it
    /// opened with before: 70, 50 and 30 with the band between 70 and 30, or 1, -1 and 0.
    #[serde(default)]
    pub levels: Vec<LevelStyle>,
    /// Whether the band between the boundary levels is filled.
    /// [`Indicator::level_fill_indices`] selects them; otherwise the outer levels are used.
    #[serde(default)]
    pub fill_between_levels: bool,
    /// Colour of the band between the boundary levels.
    #[serde(default)]
    pub level_fill_color: Color32,
    /// Keep automatic fitting for an MFI saved before it declared a fixed range.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) mfi_auto_range: bool,
    /// Keep the -1 to 1 range of a Trend Strength Index saved before its band guides while
    /// both stay hidden.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) tsi_unit_range: bool,
    /// Plots, as indices into [`plots`](Self::plots), whose last value has no tag on the price
    /// scale while the chart shows study values; the legend still shows it. A study starts
    /// with the plots its [`Indicator::plot_tagged`] leaves out, every plot of a study with more
    /// than four. An empty list, as in settings saved before this field, tags every plot.
    /// Applying settings drops indices without a plot and sorts the rest.
    #[serde(default)]
    pub untagged_plots: Vec<usize>,
    /// Outputs with a horizontal line at their latest value, independent of tags and displacement.
    /// Missing latest values remove the line. Empty, including older settings, disables all.
    #[serde(default)]
    pub price_line_plots: Vec<usize>,
    /// Alternate colour and width states per output. Missing states take declared defaults.
    #[serde(default)]
    pub plot_variants: Vec<Vec<PlotVariant>>,
    /// Optional tint across the study's plot area, independent of its plots and bands.
    #[serde(default)]
    pub background: Option<Color32>,
    /// Whether the optional pane tint is drawn. Hiding it retains its colour.
    #[serde(default = "enabled_by_default")]
    pub background_visible: bool,
    /// One independent style per band returned by [`Indicator::fills`].
    /// Missing styles take the study's declared colours and visibility. Styles of bands the study
    /// does not declare yet are kept for when it does. A style saved before bands were drawn
    /// whatever the visibility of their boundary plots restores hidden while one of those plots
    /// is hidden, as it was drawn then.
    #[serde(default)]
    pub fills: Vec<FillStyle>,
    /// Independent styles for [`Indicator::gradients`], in declaration order.
    /// Missing styles take the study's declared ramps, shown. A saved study template of an SMI
    /// from before it declared its shading restores the ramps hidden.
    #[serde(default)]
    pub gradients: Vec<GradientStyle>,
}

impl Default for StudySettings {
    /// No plot styles, the chart's default precision and no hidden intervals, with the status
    /// line values and the pane tint shown, as older saved settings restore them.
    fn default() -> Self {
        Self {
            show_legend_values: true,
            user_edit_enabled: None,
            help_url: None,
            plots: Vec::new(),
            precision: None,
            hidden_intervals: Vec::new(),
            timeframe_visibility: TimeframeVisibility::all(),
            levels: Vec::new(),
            fill_between_levels: false,
            level_fill_color: Color32::TRANSPARENT,
            mfi_auto_range: false,
            tsi_unit_range: false,
            untagged_plots: Vec::new(),
            price_line_plots: Vec::new(),
            plot_variants: Vec::new(),
            background: None,
            background_visible: true,
            fills: Vec::new(),
            gradients: Vec::new(),
        }
    }
}

fn enabled_by_default() -> bool {
    true
}

/// An owned settings and input snapshot without calculated study data.
/// Used by chart history; restoring it requires recalculation on the current bars.
#[derive(Debug, Clone)]
pub struct StudySettingsSnapshot {
    inputs: Vec<(String, InputValue)>,
    pub(crate) settings: StudySettings,
    timeframe: Option<Timeframe>,
    colors: Vec<Color32>,
    visible: bool,
    overlay: Option<bool>,
    scale: Option<super::StudyScale>,
    source: Option<super::StudySource>,
    inherited_overlay: bool,
    inherited_scale: super::StudyScale,
    inherited_source: Option<super::StudySource>,
    calculation_symbol: String,
    calculation_timeframe: Option<Timeframe>,
    wait_for_close: bool,
    offset: i32,
}

impl PartialEq for StudySettingsSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.inputs == other.inputs
            && self.settings == other.settings
            && self.colors == other.colors
            && self.visible == other.visible
            && self.overlay.unwrap_or(self.inherited_overlay)
                == other.overlay.unwrap_or(other.inherited_overlay)
            && self.scale.unwrap_or(self.inherited_scale)
                == other.scale.unwrap_or(other.inherited_scale)
            && self.source.or(self.inherited_source) == other.source.or(other.inherited_source)
            && self.calculation_symbol == other.calculation_symbol
            && self.calculation_timeframe == other.calculation_timeframe
            && self.wait_for_close == other.wait_for_close
            && self.offset == other.offset
    }
}

impl StudySettingsSnapshot {
    /// Capture declared inputs and presentation with the global visibility toggle.
    /// `visible` precedes any interval filtering performed by [`Indicator::is_visible`].
    pub fn capture(study: &(impl Indicator + ?Sized), visible: bool) -> Self {
        Self {
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            timeframe: None,
            colors: study.colors(),
            visible,
            overlay: Some(study.is_overlay()),
            scale: Some(study.scale()),
            source: None,
            inherited_overlay: study.is_overlay(),
            inherited_scale: study.scale(),
            inherited_source: study.source(),
            calculation_symbol: String::new(),
            calculation_timeframe: None,
            wait_for_close: true,
            offset: 0,
        }
    }

    pub(super) fn restore(
        &self,
        study: &mut (impl Indicator + ?Sized),
    ) -> Result<(), crate::studies::StudyError> {
        let current = study.inputs();
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .filter(|(id, value)| {
                !current
                    .iter()
                    .any(|input| input.id == *id && input.value == *value)
            })
            .cloned()
            .collect();
        if !inputs.is_empty() || study.settings() != self.settings {
            study.apply_settings(&inputs, self.settings.clone())?;
        }
        study.set_colors(self.colors.clone());
        // A palette recolours the bands of a wrapped study, and the saved band styles win.
        if study.settings().fills != self.settings.fills {
            study.apply_settings(&[], self.settings.clone())?;
        }
        study.set_visible(self.visible);
        if let Some(overlay) = self.overlay {
            study.set_overlay(overlay);
        }
        if let Some(scale) = self.scale {
            study.set_scale(scale);
        }
        Ok(())
    }
}

/// A study with editable presentation, used automatically by the chart.
/// Hosts composing their own chart can wrap built-in or custom studies the same way.
///
/// Calculated on another interval or symbol, a study's volume profiles, zones and candles span
/// the chart bars that share time with their bars. Each candle keeps the colour of its first
/// source bar. Its markers sit on the last such chart bar once the values on the chart reach
/// their bar, and its labels, price segments, drawings and projections are hidden. Its visible
/// range is the range of its own bars that share time with the chart bars in view.
pub struct ConfiguredStudy {
    study: Box<dyn Indicator>,
    /// The stable factory key, independent of the wrapped study's display name.
    registered: String,
    settings: StudySettings,
    timeframe: Option<Timeframe>,
    instrument_precision: Option<u8>,
    /// The instrument's volume decimals: the automatic precision of values in volume units.
    volume_decimals: Option<u8>,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    overlay: Option<bool>,
    scale: Option<super::StudyScale>,
    scale_mapping: Option<crate::scales::PriceMap>,
    source: Option<super::StudySource>,
    calculation_symbol: String,
    calculation_timeframe: Option<Timeframe>,
    wait_for_close: bool,
    offset: i32,
    clock: crate::model::BarClock,
    aligned: Option<IndicatorValues>,
    aligned_bars: Vec<Option<usize>>,
    /// For each chart bar, the bars of another interval or symbol that share its time, by which
    /// the study's profiles, zones, candles and markers are placed on chart bars.
    spans: Vec<std::ops::Range<usize>>,
    /// The chart bars in view, from `set_visible_range`.
    visible: Option<std::ops::Range<usize>>,
    /// The study's marks on chart bars while it runs on another interval or symbol.
    aligned_markers: Vec<super::IndicatorMarker>,
    aligned_zones: Vec<super::StudyZone>,
    aligned_candles: Vec<super::StudyCandle>,
    /// The original candle for each mapped candle, retaining its first source bar.
    aligned_candle_sources: Vec<usize>,
    /// The values kept on the bars that remain after the chart evicted the oldest bars, until
    /// the study is calculated again.
    kept: Option<Kept>,
    /// Defaults by input id that replace the declared ones, from `set_input_defaults`.
    input_defaults: Vec<(String, InputValue)>,
    /// Whether a bar replay cursor completed the chart's newest bar, which then reads closed
    /// bars as the chart's earlier bars do.
    newest_closed: bool,
    /// The time of the first bar of the calculation history that a bar replay cursor has not
    /// completed and so left out.
    replay_forming: Option<chrono::DateTime<chrono::Utc>>,
}

impl Clone for ConfiguredStudy {
    fn clone(&self) -> Self {
        self.clone_with(self.study.clone_box())
    }
}

/// The values a study keeps on the bars that remain after the chart evicted the oldest `shift`
/// bars since its last calculation.
#[derive(Clone)]
struct Kept {
    values: IndicatorValues,
    shift: usize,
}

impl std::fmt::Debug for ConfiguredStudy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfiguredStudy")
            .field("settings", &self.settings)
            .field("timeframe", &self.timeframe)
            .finish_non_exhaustive()
    }
}

impl ConfiguredStudy {
    /// The study before the wrapper adds chart controls and default overrides.
    #[cfg(all(feature = "widget", feature = "studies"))]
    pub(crate) fn declared_study(&self) -> &(dyn Indicator + 'static) {
        self.study.as_ref()
    }

    /// Only main-bar calculations can be regenerated from a trade checkpoint. In particular,
    /// an evicted prefix may have contributed to kept values that no longer follow from the
    /// retained bars alone.
    #[cfg(all(feature = "widget", feature = "studies"))]
    pub(crate) fn replay_calculation(&self) -> Option<(&(dyn Indicator + 'static), i32, bool)> {
        (self.calculation_symbol.is_empty()
            && self.calculation_timeframe.is_none()
            && self.aligned.is_none()
            && self.kept.is_none())
        .then_some((self.study.as_ref(), self.offset, self.wait_for_close))
    }

    /// The history the study is calculated on while it runs on another symbol or interval.
    #[cfg(feature = "widget")]
    pub(crate) fn calculation_history(&self) -> Option<super::StudyDataRequest> {
        (!self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some())
            .then(|| self.calculation_request())
    }

    /// Sets where a bar replay cut the calculation history and whether its cursor completed the
    /// chart's newest bar; returns whether either changed.
    #[cfg(feature = "widget")]
    pub(crate) fn set_replay(
        &mut self,
        newest_closed: bool,
        forming: Option<chrono::DateTime<chrono::Utc>>,
    ) -> bool {
        let changed = (self.newest_closed, self.replay_forming) != (newest_closed, forming);
        (self.newest_closed, self.replay_forming) = (newest_closed, forming);
        changed
    }

    // Select the inner clone once, without first cloning and discarding its calculated data.
    fn clone_with(&self, study: Box<dyn Indicator>) -> Self {
        Self {
            study,
            registered: self.registered.clone(),
            settings: self.settings.clone(),
            timeframe: self.timeframe,
            instrument_precision: self.instrument_precision,
            volume_decimals: self.volume_decimals,
            formatter: self.formatter.clone(),
            overlay: self.overlay,
            scale: self.scale,
            scale_mapping: self.scale_mapping,
            source: self.source,
            calculation_symbol: self.calculation_symbol.clone(),
            calculation_timeframe: self.calculation_timeframe,
            wait_for_close: self.wait_for_close,
            offset: self.offset,
            clock: self.clock.clone(),
            aligned: self.aligned.clone(),
            aligned_bars: self.aligned_bars.clone(),
            spans: self.spans.clone(),
            visible: self.visible.clone(),
            aligned_markers: self.aligned_markers.clone(),
            aligned_zones: self.aligned_zones.clone(),
            aligned_candles: self.aligned_candles.clone(),
            aligned_candle_sources: self.aligned_candle_sources.clone(),
            kept: self.kept.clone(),
            input_defaults: self.input_defaults.clone(),
            newest_closed: self.newest_closed,
            replay_forming: self.replay_forming,
        }
    }

    /// The study's pane, with the guides it declared before it opened with fewer when the
    /// settings hold that many levels.
    fn declared_pane(&self) -> super::StudyPane {
        let mut pane = self.study.pane();
        if let Some((levels, _)) = former_guides(self.study.as_ref(), self.settings.levels.len()) {
            pane.levels = levels;
        }
        if self.settings.mfi_auto_range
            && self.study.name() == "MFI"
            && guides_since(self.study.as_ref())
        {
            pane.range = None;
        }
        if self.settings.tsi_unit_range
            && self.study.name() == "Trend Strength Index"
            && self
                .settings
                .levels
                .get(1..3)
                .is_some_and(|bands| bands.iter().all(|band| !band.visible))
        {
            pane.range = Some((-1.0, 1.0));
        }
        pane
    }

    /// Retain the study's current colours, widths and calculation parameters.
    pub fn new(study: Box<dyn Indicator>) -> Self {
        let mut settings = study.settings();
        // The optional average input must not become a permanent style override.
        if study.is_volume()
            && let Some(plot) = settings.plots.get_mut(2)
        {
            plot.visible = true;
        }
        match_declared_styles(study.as_ref(), &mut settings);
        let registered = study.registered_name().to_owned();
        Self {
            study,
            registered,
            settings,
            timeframe: None,
            instrument_precision: None,
            volume_decimals: None,
            formatter: None,
            overlay: None,
            scale: None,
            scale_mapping: None,
            source: None,
            calculation_symbol: String::new(),
            calculation_timeframe: None,
            wait_for_close: true,
            offset: 0,
            clock: crate::model::BarClock::new(Timeframe::Min1),
            aligned: None,
            aligned_bars: Vec::new(),
            spans: Vec::new(),
            visible: None,
            aligned_markers: Vec::new(),
            aligned_zones: Vec::new(),
            aligned_candles: Vec::new(),
            aligned_candle_sources: Vec::new(),
            kept: None,
            input_defaults: Vec::new(),
            newest_closed: false,
            replay_forming: None,
        }
    }
    /// Attach a host-chosen help article to this study, retained in saved presentation.
    pub fn with_help_url(mut self, url: impl Into<String>) -> Self {
        self.settings.help_url = Some(url.into());
        self
    }

    /// Whether the study can be calculated on another interval: it reads one source, or the bars
    /// alone, without order flow it cannot do without and with no data of its own to request
    /// unless its requests apply on any interval or are price bars it reads on that interval as
    /// on a chart of it: other symbols' bars, and bars of the chart's interval, while the study
    /// has no interval input of its own. A study with an interval it picks below the chart's
    /// ([`InputKind::AutoTimeframe`](super::InputKind::AutoTimeframe)) never qualifies. An
    /// accepted interval stays available if calculating on it changes those requests. A study
    /// whose outputs cannot follow ([`Indicator::draws_on_other_bars`]) never qualifies, with or
    /// without an interval set.
    fn runs_on_other_bars(&self) -> bool {
        let requests = self.study.data_requests();
        let follows = |request: &super::StudyDataRequest| {
            request.metric.is_none() && (request.timeframe.is_none() || !request.symbol.is_empty())
        };
        self.study.draws_on_other_bars()
            && (self.calculation_timeframe.is_some()
                || self.study.source().is_some()
                || ((requests.is_empty()
                || self.study.requests_allow_other_intervals()
                || (requests.iter().all(follows)
                    && !self
                        .study
                        .inputs()
                        .iter()
                        .any(|input| matches!(input.kind, super::InputKind::Timeframe))))
                && !self.study.needs_order_flow()
                // An interval picked below the chart's follows the chart, whatever the study
                // requests on it.
                && !self
                    .study
                    .inputs()
                    .iter()
                    .any(|input| matches!(input.kind, super::InputKind::AutoTimeframe(_)))))
    }

    /// Whether the study has the Wait for interval close input: with the Interval input, or for
    /// histories of its own that it reads as closed ([`Indicator::offers_wait_for_close`]).
    fn offers_wait(&self) -> bool {
        self.runs_on_other_bars() || self.study.offers_wait_for_close()
    }

    /// Whether the study reads one source, or only the chart symbol's bars on an interval it
    /// can calculate on, without order flow, metrics or a Symbol input of its own. A study whose
    /// outputs cannot follow never qualifies. `own` holds the wrapped study's inputs. A chosen
    /// symbol stays available until it is cleared.
    fn runs_on_other_symbol(&self, own: &[super::StudyInput]) -> bool {
        self.study.draws_on_other_bars()
            && (!self.calculation_symbol.is_empty()
                || self.study.source().is_some()
                || (self.runs_on_other_bars()
                    && !self.study.needs_order_flow()
                    && self
                        .study
                        .data_requests()
                        .iter()
                        .all(|request| request.metric.is_none() && request.symbol.is_empty())
                    && !own
                        .iter()
                        .any(|input| matches!(input.kind, super::InputKind::Symbol))))
    }

    /// Whether the offset can move the study's plots, bands and fills together.
    fn offers_offset(&self) -> bool {
        self.study.source().is_some() || self.study.draws_only_plots()
    }

    /// The bars the study is calculated on while it runs on another symbol or interval.
    fn calculation_request(&self) -> super::StudyDataRequest {
        super::StudyDataRequest {
            metric: None,
            symbol: self.calculation_symbol.clone(),
            timeframe: self.calculation_timeframe,
        }
    }

    /// The study's own requests and the requests made for them on another symbol or interval.
    /// Chart-interval price bars follow the calculation interval; chart-symbol price bars
    /// follow the calculation symbol. Metrics and explicitly named symbols keep their identity.
    /// Stated intervals stay when only the symbol changes, inside the calculation interval,
    /// or for a study whose requests apply on any interval. No other histories are read.
    fn requests_on_other_bars(&self) -> Vec<(super::StudyDataRequest, super::StudyDataRequest)> {
        // Finer bars are the candles a chart of that interval would read.
        let inside = |timeframe: Timeframe| {
            !timeframe.is_trade_based()
                && self
                    .calculation_timeframe
                    .is_some_and(|interval| timeframe.duration_ms() < interval.duration_ms())
        };
        let all = self.study.requests_allow_other_intervals();
        self.study
            .data_requests()
            .into_iter()
            .filter_map(|request| {
                let mut made = if all
                    || request.metric.is_some()
                    || (!request.symbol.is_empty() && request.timeframe.is_some())
                    || (request.symbol.is_empty()
                        && request.timeframe.is_some_and(|timeframe| {
                            self.calculation_timeframe.is_none() || inside(timeframe)
                        })) {
                    request.clone()
                } else if request.timeframe.is_none() {
                    super::StudyDataRequest {
                        symbol: request.symbol.clone(),
                        timeframe: self.calculation_timeframe,
                        metric: None,
                    }
                } else {
                    return None;
                };
                // As on a chart of the calculation symbol, all its price histories follow it.
                if made.metric.is_none() && made.symbol.is_empty() {
                    made.symbol = self.calculation_symbol.clone();
                }
                Some((request, made))
            })
            .collect()
    }

    /// The bars evicted since the study was last calculated, by which the bars of its kept values
    /// trail those of the study.
    fn shift(&self) -> usize {
        self.kept.as_ref().map_or(0, |kept| kept.shift)
    }

    /// The inner study's bar for a bar this study is asked about, through its shift and its
    /// alignment to another interval.
    fn inner_bar(&self, bar: usize) -> Option<usize> {
        let bar = bar.checked_add(self.shift())?;
        if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()
        } else {
            Some(bar)
        }
    }

    fn update_price(&mut self, data: &[Bar], from: usize) {
        self.aligned = None;
        self.kept = None;
        match self.source() {
            Some(super::StudySource::Price(source)) if self.source() != self.study.source() => {
                let values: Vec<_> = data
                    .iter()
                    .map(|bar| source.compute(bar.open, bar.high, bar.low, bar.close))
                    .collect();
                self.study.update_source(data, from, &values);
            }
            Some(super::StudySource::Study { .. }) => self.study.update_source(data, from, &[]),
            _ => self.study.update(data, from),
        }
    }
}

/// Ids of the inputs [`ConfiguredStudy::inputs`] adds to a study itself, refused by
/// `CustomIndicator::with_inputs`. Keep in step with the inputs listed there, with the
/// calculation inputs after `plot_offset`, which restores of studies that cannot follow another
/// symbol or interval drop.
pub(crate) const CHART_INPUT_IDS: [&str; 4] = [
    "plot_offset",
    "calculation_symbol",
    "calculation_timeframe",
    "wait_for_close",
];

impl Indicator for ConfiguredStudy {
    fn projections(&self) -> &[super::StudyProjection] {
        // Fractional and future source anchors have no chart position in the span map.
        if self.aligned.is_some() {
            &[]
        } else {
            self.study.projections()
        }
    }

    fn table(&self) -> Option<&super::StudyTable> {
        self.study.table()
    }

    fn has_settings(&self) -> bool {
        true
    }

    fn user_edit_enabled(&self) -> bool {
        self.settings
            .user_edit_enabled
            .unwrap_or_else(|| self.study.user_edit_enabled())
    }
    fn help_url(&self) -> Option<&str> {
        self.settings
            .help_url
            .as_deref()
            .or_else(|| self.study.help_url())
            .filter(|url| !url.trim().is_empty())
    }
    fn name(&self) -> &str {
        self.study.name()
    }
    fn registered_name(&self) -> &str {
        &self.registered
    }
    fn set_registered_name(&mut self, name: &str) -> bool {
        self.registered = name.to_owned();
        true
    }
    fn set_input_defaults(&mut self, defaults: &[(String, InputValue)]) {
        self.input_defaults = defaults.to_vec();
    }
    fn metric(&self) -> Option<&crate::data::MetricSeries> {
        self.study.metric()
    }
    fn desc(&self) -> &str {
        self.study.desc()
    }
    fn source(&self) -> Option<super::StudySource> {
        self.source.or_else(|| self.study.source())
    }
    fn offset_range(&self) -> std::ops::RangeInclusive<i32> {
        self.study.offset_range()
    }
    fn inputs(&self) -> Vec<super::StudyInput> {
        let mut inputs = self.study.inputs();
        if let Some(default) = self.study.source() {
            let own = inputs.iter().find(|input| input.id == "source");
            // A study that names its source, where only part of it reads the source, keeps that name.
            let label = own.map_or_else(|| "Source".into(), |input| input.label.clone());
            let initial = own
                .and_then(|input| match input.default {
                    InputValue::Choice(index) => crate::model::PriceSource::all()
                        .get(index)
                        .copied()
                        .map(super::StudySource::Price),
                    InputValue::Source(value) => Some(value),
                    _ => None,
                })
                .unwrap_or(default);
            inputs.retain(|input| input.id != "source");
            inputs.push(
                super::StudyInput::new(
                    "source",
                    label,
                    InputValue::Source(self.source.unwrap_or(default)),
                    super::InputKind::Source,
                )
                .with_default(InputValue::Source(initial)),
            );
        }
        use super::{InputKind, StudyInput};
        if self.offers_offset() {
            let range = self.study.offset_range();
            inputs.push(super::inputs::holding(
                StudyInput::new(
                    "plot_offset",
                    "Offset",
                    InputValue::Integer(self.offset),
                    InputKind::Integer(*range.start(), *range.end()),
                )
                .with_default(InputValue::Integer(0)),
            ));
        }
        if self.runs_on_other_symbol(&inputs) {
            inputs.push(
                StudyInput::new(
                    "calculation_symbol",
                    "Symbol",
                    InputValue::Symbol(self.calculation_symbol.clone()),
                    InputKind::Symbol,
                )
                .with_default(InputValue::Symbol(String::new())),
            );
        }
        if self.runs_on_other_bars() {
            inputs.push(
                StudyInput::new(
                    "calculation_timeframe",
                    "Interval",
                    InputValue::Timeframe(self.calculation_timeframe),
                    InputKind::Timeframe,
                )
                .with_default(InputValue::Timeframe(None)),
            );
        }
        if self.offers_wait() {
            inputs.push(
                StudyInput::new(
                    "wait_for_close",
                    "Wait for interval close",
                    InputValue::Toggle(self.wait_for_close),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(true)),
            );
        }
        for input in &mut inputs {
            if let Some((_, value)) = self.input_defaults.iter().find(|(id, _)| *id == input.id)
                && input.accepts(value)
            {
                input.default = value.clone();
            }
        }
        inputs
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        // Offset 0 is no offset: a list read before an input withdrew Offset still applies.
        if id == "plot_offset" && (self.offers_offset() || value == InputValue::Integer(0)) {
            let InputValue::Integer(offset) = value else {
                return Err(crate::studies::StudyError::invalid_input(id));
            };
            // Only edits are bounded: the current offset and one restored as saved stay.
            if offset != self.offset
                && !self.study.offset_range().contains(&offset)
                && !super::inputs::restores(id, &InputValue::Integer(offset))
            {
                return Err(crate::studies::StudyError::invalid_input(id));
            }
            self.offset = offset;
            self.aligned = None;
            self.kept = None;
            self.study.calculate(&[]);
            return Ok(());
        }
        if match id {
            "calculation_symbol" => self.runs_on_other_symbol(&self.study.inputs()),
            "calculation_timeframe" => self.runs_on_other_bars(),
            "wait_for_close" => self.offers_wait(),
            _ => false,
        } {
            let schema = self
                .inputs()
                .into_iter()
                .rev() // The chart's own input follows a study input of the same id.
                .find(|input| input.id == id)
                .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
            if !schema.accepts(&value) {
                return Err(crate::studies::StudyError::invalid_input(id));
            }
            if matches!(self.source(), Some(super::StudySource::Study { .. }))
                && !matches!(&value, InputValue::Symbol(s) if s.is_empty())
                && value != InputValue::Timeframe(None)
                && id != "wait_for_close"
            {
                return Err(super::StudyError::InvalidInput {
                    id: id.into(),
                    reason: "must follow the chart while the source is another study's output"
                        .into(),
                });
            }
            match value {
                InputValue::Symbol(value) => self.calculation_symbol = value,
                InputValue::Timeframe(value) => self.calculation_timeframe = value,
                InputValue::Toggle(value) => self.wait_for_close = value,
                _ => unreachable!(),
            }
            // The study missed the chart's interval, clock and bars in view while it ran on
            // other bars.
            if id != "wait_for_close"
                && self.calculation_symbol.is_empty()
                && self.calculation_timeframe.is_none()
            {
                if let Some(timeframe) = self.timeframe {
                    self.study.set_timeframe(timeframe);
                    self.study.set_bar_clock(&self.clock);
                }
                if let Some(visible) = &self.visible {
                    self.study.set_visible_range(visible.clone());
                }
            }
            self.aligned = None;
            self.kept = None;
            self.study.calculate(&[]);
            return Ok(());
        }
        if id == "source" && self.study.source().is_some() {
            if matches!(value, InputValue::Choice(_)) {
                self.study.set_input(id, value)?;
                self.source = None;
                self.aligned = None;
                self.kept = None;
                return Ok(());
            }
            let InputValue::Source(source) = value else {
                return Err(crate::studies::StudyError::invalid_input(id));
            };
            if matches!(source, super::StudySource::Study { .. })
                && (!self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some())
            {
                return Err(super::StudyError::InvalidInput {
                    id: id.into(),
                    reason: "cannot be another study's output while the study runs on another symbol or interval".into(),
                });
            }
            self.source = Some(source);
            self.aligned = None;
            self.kept = None;
            self.study.calculate(&[]);
            Ok(())
        } else {
            // An offset moves plots only; inputs adding other outputs require Offset 0.
            if self.offset != 0 && self.study.source().is_none() {
                let mut probe = self.study.clone_box();
                probe.set_input(id, value.clone())?;
                if !probe.draws_only_plots() {
                    return Err(super::StudyError::InvalidInput {
                        id: id.into(),
                        reason: "needs Offset 0, as the study would draw outputs the offset does not move".into(),
                    });
                }
            }
            let format = self.study.value_format();
            self.study.set_input(id, value)?;
            if self.study.value_format() != format {
                self.formatter = None;
            }
            self.aligned = None;
            self.kept = None;
            Ok(())
        }
    }
    fn data_requests(&self) -> Vec<super::StudyDataRequest> {
        if !self.calculation_symbol.is_empty() || self.calculation_timeframe.is_some() {
            // The bars come first: `update_data` reads them from the head. The requests the study
            // reads on them follow, each once.
            let mut requests = vec![self.calculation_request()];
            for (_, made) in self.requests_on_other_bars() {
                if !requests.contains(&made) {
                    requests.push(made);
                }
            }
            requests
        } else {
            self.study.data_requests()
        }
    }
    fn data_start(
        &self,
        request: &super::StudyDataRequest,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            return self.study.data_start(request);
        }
        // Own histories retain their starts under their original request. Calculation bars
        // load to the chart's first bar, even when a table slot names that same request.
        if *request == self.calculation_request()
            || !(self.study.requests_allow_other_intervals()
                || self.calculation_timeframe.is_none())
        {
            return None;
        }
        self.requests_on_other_bars()
            .into_iter()
            .find(|(_, made)| made == request)
            .and_then(|(own, _)| self.study.data_start(&own))
    }
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[super::StudyData<'_>]) {
        self.kept = None;
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            if self.study.data_requests().is_empty() {
                self.update(data, from);
            } else {
                let chart = super::StudyChart {
                    clock: self.clock.clone(),
                    wait_for_close: self.wait_for_close,
                };
                let sources: Vec<_> = sources
                    .iter()
                    .map(|source| source.read_on(&chart))
                    .collect();
                self.study.update_data(data, from, &sources);
            }
            return;
        }
        let request = &self.data_requests()[0];
        let Some(source) = sources.iter().find(|source| &source.request == request) else {
            self.aligned = Some(IndicatorValues::missing(data.len()));
            self.aligned_bars.clear();
            self.spans.clear();
            self.aligned_markers.clear();
            self.aligned_zones.clear();
            self.aligned_candles.clear();
            self.aligned_candle_sources.clear();
            return;
        };
        self.instrument_precision = source
            .instrument
            .and_then(crate::scales::instrument_price_precision);
        self.volume_decimals = source
            .instrument
            .and_then(|instrument| instrument.volume_decimals);
        self.study.set_instrument(source.instrument);
        self.study.set_timeframe(source.timeframe);
        self.study.set_bar_clock(&source.clock);
        // The study's own histories are read on the calculation bars, whose latest is forming.
        let chart = super::StudyChart {
            clock: source.clock.clone(),
            wait_for_close: self.wait_for_close,
        };
        self.spans = spans_of(data, &self.clock, source);
        if let Some(visible) = &self.visible {
            self.study
                .set_visible_range(source_bars(&self.spans, visible));
        }
        let read = self.requests_on_other_bars();
        if read.is_empty() {
            self.update_price(source.bars, 0);
        } else if read.iter().all(|(own, made)| own == made) {
            let sources: Vec<_> = sources
                .iter()
                .map(|history| history.read_on(&chart))
                .collect();
            self.study.update_data(source.bars, 0, &sources);
        } else {
            // The study finds each history under its own request.
            let histories: Vec<_> = read
                .into_iter()
                .filter_map(|(own, made)| {
                    let history = sources.iter().find(|history| history.request == made)?;
                    Some(super::StudyData {
                        request: own,
                        bars: history.bars,
                        clock: history.clock.clone(),
                        instrument: history.instrument,
                        timeframe: history.timeframe,
                        chart: Some(chart.clone()),
                    })
                })
                .collect();
            self.study.update_data(source.bars, 0, &histories);
        }
        let mut aligned = IndicatorValues::new();
        self.aligned_bars.clear();
        let longer = (self.clock.timeframe())
            .is_some_and(|chart| source.timeframe.duration_ms() > chart.duration_ms());
        // A replay leaves out the longer bar its cursor is inside; the chart bars it holds read
        // the bars closed before it, as they did live, and are no gap.
        let forming = (self.replay_forming)
            .filter(|_| longer)
            .map(|time| source.clock.bar_open(time));
        for (at, bar) in data.iter().enumerate() {
            let held = source.bar_on(&self.clock, bar.time).is_some()
                || forming.is_some_and(|open| source.slot_on(&self.clock, bar.time) == open);
            let output = held
                .then(|| {
                    source.closed_on(
                        &self.clock,
                        self.wait_for_close,
                        bar.time,
                        at + 1 == data.len() && !self.newest_closed,
                    )
                })
                .flatten();
            self.aligned_bars.push(output);
            let values: Vec<_> = (0..self.study.line_count())
                .map(|line| {
                    output
                        .and_then(|at| self.study.values().value(line, at))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            aligned.push(super::IndicatorValue::Multiple(values));
        }
        self.aligned = Some(aligned);
        // A marker appears once the values on the chart reach its bar (Wait for interval close).
        let shown = self.aligned_bars.iter().flatten().max().copied();
        self.aligned_markers = self
            .study
            .markers()
            .iter()
            .filter(|marker| shown >= Some(marker.bar))
            .filter_map(|marker| {
                let bars = chart_bars(&self.spans, &(marker.bar..marker.bar.saturating_add(1)))?;
                Some(super::IndicatorMarker {
                    bar: bars.end - 1,
                    ..*marker
                })
            })
            .collect();
        self.aligned_zones = self
            .study
            .zones()
            .iter()
            .filter_map(|zone| {
                Some(super::StudyZone {
                    bars: chart_bars(&self.spans, &zone.bars)?,
                    ..zone.clone()
                })
            })
            .collect();
        (self.aligned_candles, self.aligned_candle_sources) = self
            .study
            .candles()
            .iter()
            .enumerate()
            .filter_map(|(index, candle)| {
                Some((
                    super::StudyCandle {
                        bars: chart_bars(&self.spans, &candle.bars)?,
                        ..candle.clone()
                    },
                    index,
                ))
            })
            .unzip();
    }
    fn order_flow_inputs(&self) -> &[crate::data::OrderFlowInput] {
        self.study.order_flow_inputs()
    }
    fn requests_allow_other_intervals(&self) -> bool {
        self.study.requests_allow_other_intervals()
    }
    fn needs_order_flow(&self) -> bool {
        self.study.needs_order_flow()
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        if !self.data_requests().is_empty() {
            self.update_data(data, from, &[]);
        } else {
            self.update_price(data, from);
        }
    }
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        self.kept = None;
        self.study.update_source(data, from, source);
    }
    fn is_incremental(&self) -> bool {
        self.data_requests().is_empty()
            && self.source() == self.study.source()
            && self.study.is_incremental()
    }
    fn calculation(&self) -> Option<super::Calculation> {
        self.study.calculation()
    }
    fn remove_oldest_bars(&mut self, count: usize) -> bool {
        // These outputs index the evicted bars too.
        let indexed = !self.candles().is_empty()
            || !self.zones().is_empty()
            || !self.markers().is_empty()
            || !self.labels().is_empty()
            || !self.price_segments().is_empty()
            || !self.projections().is_empty()
            || (0..self.line_count()).any(|line| self.provisional_after(line).is_some());
        #[cfg(feature = "drawings")]
        let indexed = indexed || !self.drawings().is_empty();
        if indexed || self.values().len() <= count {
            return false;
        }
        let shift = self.shift() + count;
        let values = self.values().without_first(count);
        self.kept = Some(Kept { values, shift });
        true
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.instrument_precision =
                instrument.and_then(crate::scales::instrument_price_precision);
            self.volume_decimals = instrument.and_then(|instrument| instrument.volume_decimals);
            self.study.set_instrument(instrument)
        } else {
            false
        }
    }
    fn set_chart_symbol(&mut self, symbol: &str) -> bool {
        self.study
            .set_chart_symbol(if self.calculation_symbol.is_empty() {
                symbol
            } else {
                &self.calculation_symbol
            })
    }
    fn set_time_scale_events(&mut self, events: &[crate::model::TimeScaleEvent]) -> bool {
        // A study calculated on another symbol has none of the chart's events.
        self.study
            .set_time_scale_events(if self.calculation_symbol.is_empty() {
                events
            } else {
                &[]
            })
    }
    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        self.clock = crate::model::BarClock::new(timeframe);
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.set_timeframe(timeframe);
        }
    }
    fn set_visible_range(&mut self, range: std::ops::Range<usize>) -> bool {
        self.visible = Some(range.clone());
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            return self.study.set_visible_range(range);
        }
        // Mapped by the spans of the last calculation; `update_data` maps it again on its bars.
        // Without spans there is nothing to recalculate until it does.
        self.study
            .set_visible_range(source_bars(&self.spans, &range))
            && !self.spans.is_empty()
    }
    fn set_visible_logical_range(&mut self, range: crate::model::LogicalRange) {
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.set_visible_logical_range(range);
        }
    }
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        self.clock = clock.clone();
        if self.calculation_symbol.is_empty() && self.calculation_timeframe.is_none() {
            self.study.set_bar_clock(clock)
        } else {
            false
        }
    }
    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        // Source bars of a study on another interval or symbol are not chart bars.
        if self.aligned.is_some() {
            &[]
        } else {
            self.study.drawings()
        }
    }
    #[cfg(feature = "drawings")]
    fn drawings_use_plot_style(&self) -> bool {
        self.study.drawings_use_plot_style()
    }
    #[cfg(feature = "drawings")]
    fn drawing_plot(&self, index: usize) -> Option<usize> {
        self.study.drawing_plot(index)
    }
    fn candles(&self) -> &[super::StudyCandle] {
        if self.aligned.is_some() {
            &self.aligned_candles
        } else {
            self.study.candles()
        }
    }
    fn candle_color(&self, index: usize) -> Option<Color32> {
        if self.aligned.is_none() {
            let candle = self.candles().get(index)?;
            return self.plot_color(candle.plot, candle.bars.start);
        }
        let index = *self.aligned_candle_sources.get(index)?;
        let candle = self.study.candles().get(index)?;
        self.study
            .plot_variant(candle.plot, candle.bars.start)
            .and_then(|variant| self.settings.plot_variants.get(candle.plot)?.get(variant))
            .map(|variant| variant.color)
            .or_else(|| self.study.candle_color(index))
    }
    fn solid_candles(&self) -> bool {
        self.study.solid_candles()
    }
    fn tpo(&self) -> Option<&crate::model::TPOConfig> {
        self.study.tpo()
    }
    fn values(&self) -> &IndicatorValues {
        self.kept
            .as_ref()
            .map(|kept| &kept.values)
            .or(self.aligned.as_ref())
            .unwrap_or_else(|| self.study.values())
    }
    fn colors(&self) -> Vec<Color32> {
        self.settings.plots.iter().map(|plot| plot.color).collect()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        self.study.set_colors(colors);
        for (plot, color) in self.settings.plots.iter_mut().zip(self.study.colors()) {
            plot.color = color;
        }
        for (line, variants) in self.settings.plot_variants.iter_mut().enumerate() {
            for (variant, declared) in variants.iter_mut().zip(self.study.plot_variants(line)) {
                variant.color = declared.color;
            }
        }
        for (style, declared) in self.settings.fills.iter_mut().zip(self.study.fills()) {
            style.color = declared.color;
            style.inverted_color = declared.inverted_color;
        }
    }
    fn set_color(&mut self, line: usize, color: Color32) {
        if let Some(plot) = self.settings.plots.get_mut(line) {
            self.study.set_color(line, color);
            plot.color = self.study.plot_style(line).color;
        }
    }
    fn line_style(&self, line: usize) -> IndicatorLineStyle {
        let mut style = self.study.line_style(line);
        if let Some(plot) = self.settings.plots.get(line) {
            style.width = plot.width;
        }
        style
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        let mut plot = self.settings.plots.get(line).copied().unwrap_or_else(|| {
            let mut plot = self.study.plot_style(line);
            let output = self.settings.plots.get(self.study.plot_output(line));
            plot.visible &= output.is_none_or(|output| output.visible);
            plot
        });
        if self.study.is_volume() {
            plot.visible &= self.study.plot_style(line).visible;
        }
        // A readout shows in the legend, never as a line.
        if self.study.readout_only(line) {
            plot.visible = false;
        }
        plot
    }
    fn plot_tagged(&self, line: usize) -> bool {
        if line < self.settings.plots.len() {
            !self.settings.untagged_plots.contains(&line)
        } else {
            self.study.plot_tagged(line)
        }
    }
    fn plot_price_line(&self, line: usize) -> bool {
        self.settings.price_line_plots.contains(&line)
    }
    fn plot_variants(&self, line: usize) -> Vec<PlotVariant> {
        self.settings
            .plot_variants
            .get(line)
            .cloned()
            .unwrap_or_default()
    }
    fn plot_variant(&self, line: usize, bar: usize) -> Option<usize> {
        let bar = bar.checked_add(self.shift())?;
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_variant(line, bar)
    }
    fn plot_width(&self, line: usize, bar: usize) -> f32 {
        self.plot_variant(line, bar)
            .and_then(|variant| self.settings.plot_variants.get(line)?.get(variant))
            .map_or_else(|| self.plot_style(line).width, |variant| variant.width)
    }
    fn background(&self) -> Option<Color32> {
        self.settings
            .background
            .filter(|_| self.settings.background_visible)
    }
    fn plot_offset(&self, line: usize) -> isize {
        self.study
            .plot_offset(line)
            .saturating_add(self.offset as isize)
    }
    fn draws_only_plots(&self) -> bool {
        self.study.draws_only_plots()
    }
    fn draws_on_other_bars(&self) -> bool {
        self.study.draws_on_other_bars()
    }
    fn alert_conditions(&self) -> Vec<String> {
        self.study.alert_conditions()
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        self.study.alert_condition(condition, self.inner_bar(bar)?)
    }

    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        let kept = self.kept.as_ref().map(|kept| &kept.values);
        if let Some(values) = kept.or(self.aligned.as_ref()) {
            values.value(
                line,
                bar.checked_add_signed(self.plot_offset(line).checked_neg()?)?,
            )
        } else {
            self.study.plot_value(
                line,
                bar.checked_add_signed((self.offset as isize).checked_neg()?)?,
            )
        }
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        if let Some(variant) = self
            .plot_variant(line, bar)
            .and_then(|variant| self.settings.plot_variants.get(line)?.get(variant))
        {
            return Some(variant.color);
        }
        let bar = bar.checked_add(self.shift())?;
        let bar = if self.aligned.is_some() {
            self.aligned_bars.get(bar).copied().flatten()?
        } else {
            bar
        };
        self.study.plot_color(line, bar)
    }
    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        self.study.plot_text(line, self.inner_bar(bar)?)
    }
    fn plot_shape_side(&self, line: usize) -> ShapeSide {
        self.study.plot_shape_side(line)
    }
    fn plot_tooltip(&self, line: usize, bar: usize) -> Option<String> {
        self.study.plot_tooltip(line, self.inner_bar(bar)?)
    }
    fn plot_character(&self, line: usize) -> Option<char> {
        self.study.plot_character(line)
    }
    fn fills(&self) -> Vec<super::StudyFill> {
        self.study
            .fills()
            .into_iter()
            .enumerate()
            .filter_map(|(index, mut fill)| {
                if let Some(style) = self.settings.fills.get(index) {
                    if !style.visible {
                        return None;
                    }
                    fill.color = style.color;
                    fill.inverted_color = style.inverted_color;
                }
                Some(fill)
            })
            .collect()
    }
    fn gradients(&self) -> Vec<super::StudyGradient> {
        self.study
            .gradients()
            .into_iter()
            .enumerate()
            .map(|(index, mut fill)| {
                if let Some(style) = self.settings.gradients.get(index) {
                    fill.colors = if style.visible {
                        style.colors
                    } else {
                        [Color32::TRANSPARENT; 2]
                    };
                }
                fill
            })
            .collect()
    }
    fn snapshot_settings(&self) -> StudySettingsSnapshot {
        let mut snapshot =
            StudySettingsSnapshot::capture(self.study.as_ref(), self.study.is_visible());
        snapshot.settings = self.settings.clone();
        // As `settings` returns it, so applying unchanged settings adds no history step.
        snapshot.settings.user_edit_enabled = Some(self.user_edit_enabled());
        snapshot.timeframe = self.timeframe;
        snapshot.overlay = self.overlay;
        snapshot.scale = self.scale;
        snapshot.source = self.source;
        snapshot.calculation_symbol = self.calculation_symbol.clone();
        snapshot.calculation_timeframe = self.calculation_timeframe;
        snapshot.wait_for_close = self.wait_for_close;
        snapshot.offset = self.offset;
        snapshot
    }
    fn restore_settings(
        &mut self,
        snapshot: &StudySettingsSnapshot,
    ) -> Result<(), crate::studies::StudyError> {
        let mut inputs: Vec<_> = snapshot.inputs.iter().collect();
        // The study held these values, a restored one outside its input's range included.
        super::inputs::restoring(snapshot.inputs.clone(), || {
            while !inputs.is_empty() {
                let remaining = inputs.len();
                let mut error = None;
                // An input can change another input's accepted range.
                inputs.retain(|(id, value)| {
                    if self
                        .study
                        .inputs()
                        .iter()
                        .any(|input| input.id == *id && input.value == *value)
                    {
                        return false;
                    }
                    if let Err(message) = self.study.set_input(id, value.clone()) {
                        error = Some(message);
                        true
                    } else {
                        false
                    }
                });
                if inputs.len() == remaining {
                    return Err(error.expect("a retained input failed to apply"));
                }
            }
            Ok(())
        })?;
        self.study.set_colors(snapshot.colors.clone());
        self.study.set_visible(snapshot.visible);
        self.study.set_label_precision(snapshot.settings.precision);
        self.settings = snapshot.settings.clone();
        self.timeframe = snapshot.timeframe;
        self.overlay = snapshot.overlay;
        self.scale = snapshot.scale;
        self.scale_mapping = None;
        self.source = snapshot.source;
        self.calculation_symbol = snapshot.calculation_symbol.clone();
        self.calculation_timeframe = snapshot.calculation_timeframe;
        self.wait_for_close = snapshot.wait_for_close;
        self.offset = snapshot.offset;
        self.aligned = None;
        self.kept = None;
        self.aligned_bars.clear();
        Ok(())
    }
    fn settings(&self) -> StudySettings {
        let mut settings = self.settings.clone();
        settings.user_edit_enabled = Some(self.user_edit_enabled());
        // A study can declare a band after its settings were last applied.
        match_declared_styles(self.study.as_ref(), &mut settings);
        settings
    }
    fn apply_settings(
        &mut self,
        inputs: &[(String, InputValue)],
        mut settings: StudySettings,
    ) -> Result<(), crate::studies::StudyError> {
        if let Some(precision) = settings.precision.filter(|precision| *precision > 12) {
            return Err(super::StudyError::InvalidPrecision(precision));
        }
        if let Some((plot, width)) = settings
            .plots
            .iter()
            .enumerate()
            .map(|(plot, style)| (plot, style.width))
            .chain(
                settings
                    .plot_variants
                    .iter()
                    .enumerate()
                    .flat_map(|(plot, variants)| {
                        variants.iter().map(move |variant| (plot, variant.width))
                    }),
            )
            .find(|(_, width)| !width.is_finite() || !(0.5..=10.0).contains(width))
        {
            return Err(super::StudyError::InvalidPlotWidth { plot, width });
        }
        if let Some((level, style)) = settings
            .levels
            .iter()
            .enumerate()
            .find(|(_, style)| !style.width.is_finite() || !(0.5..=10.0).contains(&style.width))
        {
            return Err(super::StudyError::InvalidLevelWidth {
                level,
                width: style.width,
            });
        }
        if let Some((level, value)) =
            settings
                .levels
                .iter()
                .enumerate()
                .find_map(|(level, style)| {
                    style
                        .value
                        .filter(|value| !value.is_finite())
                        .map(|value| (level, value))
                })
        {
            return Err(super::StudyError::InvalidLevelValue { level, value });
        }
        let mut edited = self.clone();
        let mut inputs: Vec<_> = inputs.iter().collect();
        // Close-wait precedes clearing its interval. Symbol waits for own inputs that enable
        // it. Clear an available Offset before own inputs; otherwise apply it after them.
        if (edited.study.source().is_none() && edited.runs_on_other_bars())
            || edited.calculation_timeframe.is_some()
            || inputs.iter().any(|(id, value)| {
                id == "source"
                    && matches!(value, InputValue::Source(super::StudySource::Study { .. }))
            })
        {
            let offers_offset = edited.offers_offset();
            let offers_symbol = edited.runs_on_other_symbol(&edited.study.inputs());
            inputs.sort_by_key(|(id, value)| match (id.as_str(), value) {
                ("wait_for_close", _) => 0,
                ("calculation_symbol", _) if !offers_symbol => 3,
                ("calculation_symbol" | "calculation_timeframe", _) => 1,
                ("plot_offset", InputValue::Integer(0)) if offers_offset => 1,
                ("plot_offset", _) => 3,
                _ => 2,
            });
        }
        for (id, value) in inputs {
            edited.set_input(id, value.clone())?;
        }
        // Styles and tags of the plots before the inputs changed their number keep their place,
        // and plots they add start with the study's own style and tag. Plots that keep their
        // names at the end, behind plots the inputs add or remove, keep theirs under those names.
        if settings.plots.len() == self.line_count() && edited.line_count() != self.line_count() {
            let defaults = edited.study.settings();
            let (old, new) = (settings.plots.len(), defaults.plots.len());
            let (before, after) = (self.line_names(), edited.line_names());
            let moved = if (before.len(), after.len()) == (old, new) {
                let same = |(a, b): &(&String, &String)| a == b;
                let head = before.iter().zip(&after).take_while(same).count();
                let (before, after) = (before[head..].iter().rev(), after[head..].iter().rev());
                before.zip(after).take_while(same).count()
            } else {
                0
            };
            // The plot of the study before the edit that plot `line` continues.
            let was = |line: usize| {
                if line + moved >= new {
                    Some(old - (new - line))
                } else {
                    (line + moved < old).then_some(line)
                }
            };
            let flagged = |flags: &[usize], declared: &[usize]| -> Vec<usize> {
                (0..new)
                    .filter(|&line| match was(line) {
                        Some(from) => flags.contains(&from),
                        None => declared.contains(&line),
                    })
                    .collect()
            };
            settings.untagged_plots = flagged(&settings.untagged_plots, &defaults.untagged_plots);
            settings.price_line_plots = flagged(&settings.price_line_plots, &[]);
            settings.plots = (0..new)
                .map(|line| was(line).map_or(defaults.plots[line], |from| settings.plots[from]))
                .collect();
        }
        if settings.plots.len() != edited.line_count() {
            return Err(super::StudyError::PlotCount {
                expected: edited.line_count(),
                found: settings.plots.len(),
            });
        }
        settings
            .untagged_plots
            .retain(|&line| line < settings.plots.len());
        settings.untagged_plots.sort_unstable();
        settings.untagged_plots.dedup();
        settings
            .price_line_plots
            .retain(|&line| line < settings.plots.len());
        settings.price_line_plots.sort_unstable();
        settings.price_line_plots.dedup();
        // Unchanged plot colours must retain any per-bar palette stored by the study.
        for (line, plot) in settings.plots.iter().enumerate() {
            if plot.color != edited.study.plot_style(line).color {
                edited.study.set_color(line, plot.color);
            }
        }
        // Bands these inputs remove take their styles with them. A study left with none keeps
        // them, since it may declare its bands only once it has data and these inputs can
        // recalculate it without bars.
        let bands = edited.study.fills().len();
        if bands > 0 && bands < self.study.fills().len() {
            settings.fills.truncate(bands);
        }
        // A band saved before its style alone decided whether it is drawn was not drawn while
        // a boundary plot was hidden, so it keeps that look with its own switch off.
        let declared = edited.study.fills();
        let hidden = |boundary| match boundary {
            super::FillBoundary::Plot(line) => {
                settings.plots.get(line).is_some_and(|plot| !plot.visible)
            }
            super::FillBoundary::Level(_) => false,
        };
        for (band, style) in settings.fills.iter_mut().enumerate() {
            if !style.independent
                && declared
                    .get(band)
                    .is_some_and(|fill| hidden(fill.first) || hidden(fill.second))
            {
                style.visible = false;
            }
            style.independent = true;
        }
        match_declared_styles(edited.study.as_ref(), &mut settings);
        if self.study.value_format() != edited.study.value_format() {
            self.formatter = None;
        }
        self.study = edited.study;
        self.study.set_label_precision(settings.precision);
        self.source = edited.source;
        self.calculation_symbol = edited.calculation_symbol;
        self.calculation_timeframe = edited.calculation_timeframe;
        self.wait_for_close = edited.wait_for_close;
        self.offset = edited.offset;
        self.aligned = None;
        self.kept = None;
        self.settings = settings;
        Ok(())
    }
    fn scale_mapping(&self) -> Option<crate::scales::PriceMap> {
        self.scale_mapping
    }
    fn set_scale_mapping(&mut self, mapping: Option<crate::scales::PriceMap>) {
        self.scale_mapping = mapping;
    }
    fn set_value_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) -> bool {
        self.formatter = formatter;
        true
    }
    fn format_value(&self, value: f64) -> String {
        self.formatter
            .as_ref()
            .filter(|_| value.is_finite())
            .map_or_else(
                || {
                    if self.precision() == self.study.precision() {
                        self.study.format_value(value)
                    } else if let Some(precision) = self.precision().filter(|_| {
                        self.settings.precision.is_none()
                            && self.value_format() == crate::scales::StudyFormat::Volume
                            && value.is_finite()
                    }) {
                        // Instrument volume decimals describe the whole number; units keep two.
                        crate::scales::VolumeFormatter { precision }
                            .format_with_min_unit_decimals(value, 2)
                    } else {
                        self.value_format().format(value, self.precision())
                    }
                },
                |formatter| formatter.format(value),
            )
    }
    fn format_plot_value(&self, line: usize, value: f64) -> String {
        if self.formatter.is_some() || self.precision() != self.study.precision() {
            self.format_value(value)
        } else {
            self.study.format_plot_value(line, value)
        }
    }
    fn legend_lines(&self) -> Vec<usize> {
        self.study.legend_lines()
    }
    fn legend_visible(&self, line: usize) -> bool {
        // The style tab shows or hides a readout that the chart never draws.
        if self.study.readout_only(line) {
            return self.settings.show_legend_values
                && self
                    .settings
                    .plots
                    .get(line)
                    .is_none_or(|plot| plot.visible)
                && self.study.legend_visible(line);
        }
        let (shown, own) = (
            self.study.legend_visible(line),
            self.study.plot_style(line).visible,
        );
        // The study may add a readout for a hidden plot or withhold one for a drawn plot.
        self.settings.show_legend_values
            && (self.plot_style(line).visible || (shown && !own))
            && (shown || !own)
    }
    fn value_format(&self) -> crate::scales::StudyFormat {
        self.study.value_format()
    }
    fn scale(&self) -> super::StudyScale {
        // A pane whose plots stay on the price chart has no value scale of its own.
        if self.study.plots_on_price() && !self.is_overlay() {
            return super::StudyScale::None;
        }
        self.scale.unwrap_or_else(|| self.study.scale())
    }
    fn precision(&self) -> Option<usize> {
        self.settings
            .precision
            .or_else(|| self.study.precision())
            .or_else(|| match self.value_format() {
                crate::scales::StudyFormat::Price => self.instrument_precision.map(usize::from),
                // Capped as `StudyFormat` caps them, so exported columns match the legend.
                crate::scales::StudyFormat::Volume => self
                    .volume_decimals
                    .map(|decimals| usize::from(decimals).min(12)),
                _ => None,
            })
    }
    fn line_count(&self) -> usize {
        self.study.line_count()
    }
    fn line_names(&self) -> Vec<String> {
        self.study.line_names()
    }
    fn alert_outputs(&self) -> Vec<(usize, String)> {
        let mut outputs = self.study.alert_outputs();
        // Values kept after eviction or aligned to another interval hold only plotted lines.
        if self.kept.is_some() || self.aligned.is_some() {
            let plots = self.study.line_count();
            outputs.retain(|(output, _)| *output < plots);
        }
        outputs
    }
    fn projection_fill_color(&self, projection: &super::StudyProjection) -> Color32 {
        self.study.projection_fill_color(projection)
    }
    fn plot_label_colors(&self, line: usize) -> Option<(Color32, Color32)> {
        self.study
            .plot_label_colors(line)
            .map(|(_, text)| (self.plot_style(line).color, text))
    }
    fn marker_style(&self) -> Option<PlotStyle> {
        self.study.marker_style()
    }
    fn markers(&self) -> &[super::IndicatorMarker] {
        if self.aligned.is_some() {
            &self.aligned_markers
        } else {
            self.study.markers()
        }
    }
    fn volume_profiles(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::StudyProfile> {
        // Kept profiles would index the evicted bars too.
        if self.kept.is_some() {
            return Vec::new();
        }
        if self.aligned.is_none() {
            return self.study.volume_profiles(visible);
        }
        // Source bars of a study on another interval or symbol are not chart bars.
        self.study
            .volume_profiles(source_bars(&self.spans, &visible))
            .into_iter()
            .filter_map(|mut plot| {
                plot.bars = chart_bars(&self.spans, &plot.bars)?;
                Some(plot)
            })
            .collect()
    }
    fn profile_levels(&self, visible: std::ops::Range<usize>) -> Vec<crate::data::ProfileLevel> {
        // Kept levels would index the evicted bars too.
        if self.kept.is_some() {
            return Vec::new();
        }
        if self.aligned.is_none() {
            return self.study.profile_levels(visible);
        }
        // Source bars of a study on another interval or symbol are not chart bars.
        self.study
            .profile_levels(source_bars(&self.spans, &visible))
            .into_iter()
            .filter_map(|mut level| {
                level.bars = chart_bars(&self.spans, &level.bars)?;
                Some(level)
            })
            .collect()
    }
    fn zones(&self) -> &[super::StudyZone] {
        if self.aligned.is_some() {
            &self.aligned_zones
        } else {
            self.study.zones()
        }
    }
    fn zones_cover_whole_bars(&self) -> bool {
        self.study.zones_cover_whole_bars()
    }
    fn labels(&self) -> &[super::StudyLabel] {
        // Source bars of a study on another interval or symbol are not chart bars.
        if self.aligned.is_some() {
            &[]
        } else {
            self.study.labels()
        }
    }
    fn labels_use_plot_background(&self) -> bool {
        self.study.labels_use_plot_background()
    }

    fn labels_on_price(&self) -> bool {
        self.study.labels_on_price()
    }
    fn plots_on_price(&self) -> bool {
        self.study.plots_on_price()
    }
    fn price_segments(&self) -> &[super::StudySegment] {
        // Source bars of a study on another interval or symbol are not chart bars.
        if self.aligned.is_some() {
            &[]
        } else {
            self.study.price_segments()
        }
    }
    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.study.set_label_precision(precision);
    }
    fn provisional_after(&self, line: usize) -> Option<usize> {
        self.aligned
            .is_none()
            .then(|| self.study.provisional_after(line))
            .flatten()
    }
    fn extend_right(&self) -> bool {
        self.study.extend_right()
    }
    fn plot_label(&self) -> Option<(usize, String)> {
        self.study.plot_label()
    }
    fn is_volume(&self) -> bool {
        self.study.is_volume()
    }
    fn readout_only(&self, line: usize) -> bool {
        self.study.readout_only(line)
    }
    fn plot_holds_values(&self, line: usize) -> bool {
        self.study.plot_holds_values(line)
    }
    fn plot_color_only(&self, line: usize) -> bool {
        self.study.plot_color_only(line)
    }
    fn level_style(&self, value: f64) -> LineStyle {
        // Levels moved onto one value draw with the pattern of the first visible one.
        self.declared_pane()
            .levels
            .iter()
            .zip(&self.settings.levels)
            .find(|((declared, ..), style)| {
                style.visible && style.value.unwrap_or(*declared) == value
            })
            .map_or_else(|| self.study.level_style(value), |(_, style)| style.pattern)
    }
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        former_guides(self.study.as_ref(), self.settings.levels.len())
            .map_or_else(|| self.study.level_fill_indices(), |(_, fill)| fill)
    }
    fn level_price_line(&self, index: usize) -> Option<Color32> {
        self.settings
            .levels
            .get(index)
            .filter(|style| style.price_line)
            .map(|style| style.color)
    }
    fn level_stroke(&self, index: usize, value: f64) -> (LineStyle, f32) {
        self.settings
            .levels
            .get(index)
            .filter(|style| {
                style
                    .value
                    .or_else(|| self.declared_pane().levels.get(index).map(|level| level.0))
                    == Some(value)
            })
            .map_or_else(
                || (self.level_style(value), self.level_width(value)),
                |style| (style.pattern, style.width),
            )
    }
    fn level_fill_color(&self) -> Color32 {
        self.settings.level_fill_color
    }
    fn level_width(&self, value: f64) -> f32 {
        self.declared_pane()
            .levels
            .iter()
            .zip(&self.settings.levels)
            .find(|((declared, ..), style)| {
                style.visible && style.value.unwrap_or(*declared) == value
            })
            .map_or_else(|| self.study.level_width(value), |(_, style)| style.width)
    }
    fn pane(&self) -> super::StudyPane {
        let mut pane = self.declared_pane();
        for ((value, color, _), style) in pane.levels.iter_mut().zip(&self.settings.levels) {
            *value = style.value.unwrap_or(*value);
            // A hidden level still bounds the fill; its transparent line is not drawn.
            *color = if style.visible {
                style.color
            } else {
                Color32::TRANSPARENT
            };
        }
        pane.fill_between_levels = self.settings.fill_between_levels;
        pane
    }
    fn is_overlay(&self) -> bool {
        self.overlay.unwrap_or_else(|| self.study.is_overlay())
    }
    fn set_overlay(&mut self, overlay: bool) -> bool {
        self.overlay = Some(overlay);
        true
    }
    fn set_scale(&mut self, scale: super::StudyScale) -> bool {
        if self.study.plots_on_price() && !self.is_overlay() {
            return false;
        }
        self.scale_mapping = None;
        self.scale = Some(scale);
        true
    }
    fn is_visible(&self) -> bool {
        let intervals = &self.settings.timeframe_visibility;
        self.study.is_visible()
            && self.timeframe.is_none_or(|tf| {
                !self.settings.hidden_intervals.contains(&tf)
                    // The label is only built for a study limited to some intervals.
                    && (intervals.show_on_all || intervals.is_visible_on(&tf.as_str()))
            })
    }
    fn set_visible(&mut self, visible: bool) {
        self.study.set_visible(visible);
    }
    fn user_visible(&self) -> bool {
        self.study.user_visible()
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
    fn clone_for_export(&self) -> Box<dyn Indicator> {
        Box::new(self.clone_with(self.study.clone_for_export()))
    }
}

/// For each of the chart's bars `data`, opened by `clock`, the bars of `source` that share its
/// time: those that open before it closes and close after it opens. A bar of an interval not
/// measured in time lasts until the next one opens, and the last one without end.
fn spans_of(
    data: &[Bar],
    clock: &crate::model::BarClock,
    source: &super::StudyData<'_>,
) -> Vec<std::ops::Range<usize>> {
    let close = |by: &crate::model::BarClock, bars: &[Bar], at: usize| {
        by.next_open(by.bar_open(bars[at].time))
            .or_else(|| bars.get(at + 1).map(|bar| bar.time))
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC)
    };
    let (mut first, mut end) = (0, 0);
    data.iter()
        .enumerate()
        .map(|(at, bar)| {
            let (open, until) = (clock.bar_open(bar.time), close(clock, data, at));
            while first < source.bars.len() && close(&source.clock, source.bars, first) <= open {
                first += 1;
            }
            end = end.max(first);
            while end < source.bars.len() && source.clock.bar_open(source.bars[end].time) < until {
                end += 1;
            }
            first..end
        })
        .collect()
}

/// The chart bars that share time with the source bars `bars`, from the `spans` of each chart
/// bar; `None` when no chart bar does.
fn chart_bars(
    spans: &[std::ops::Range<usize>],
    bars: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let start = spans.partition_point(|span| span.end <= bars.start);
    let end = spans.partition_point(|span| span.start < bars.end);
    (bars.start < bars.end && start < end).then_some(start..end)
}

/// The source bars that share time with the chart bars `bars`: from the first source bar of the
/// first chart bar to the last of the last, from the `spans` of each chart bar.
fn source_bars(
    spans: &[std::ops::Range<usize>],
    bars: &std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let end = bars.end.min(spans.len());
    if bars.start >= end {
        return 0..0;
    }
    spans[bars.start].start..spans[end - 1].end
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::studies::BollingerBands;

    /// An edit of a chart input uses its schema even if a host study declares the same id.
    #[test]
    fn chart_inputs_check_edits_against_their_own_schema() {
        #[derive(Clone, Default)]
        struct Own {
            values: IndicatorValues,
        }
        impl Indicator for Own {
            fn name(&self) -> &str {
                "Own"
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.values = IndicatorValues::missing(data.len());
            }
            fn values(&self) -> &IndicatorValues {
                &self.values
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::WHITE]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn set_visible(&mut self, _: bool) {}
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
            fn inputs(&self) -> Vec<super::super::StudyInput> {
                vec![super::super::StudyInput::new(
                    "wait_for_close",
                    "Bars",
                    InputValue::Length(2),
                    super::super::InputKind::Length(1, 10),
                )]
            }
        }
        let mut study = ConfiguredStudy::new(Box::new(Own::default()));
        let error = study
            .set_input("wait_for_close", InputValue::Length(3))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "study input \"wait_for_close\" is outside its accepted values"
        );
        study
            .set_input("wait_for_close", InputValue::Toggle(false))
            .unwrap();
        assert_eq!(
            study
                .inputs()
                .last()
                .map(|input| (input.id.clone(), input.value.clone())),
            Some(("wait_for_close".into(), InputValue::Toggle(false)))
        );
    }

    /// Performance comparing no symbols, which requests the chart symbol's daily bars alone.
    fn performance_alone() -> super::super::Performance {
        let mut study = super::super::Performance::new();
        study
            .set_input("symbols", InputValue::Text(String::new()))
            .unwrap();
        study
    }

    /// Guards #2112: clearing the calculation interval hands the study the chart's interval,
    /// whether the chart passed it before the input was set, as a template does, or moved on
    /// while it was set, and whether the edit comes directly or from the settings dialog.
    #[test]
    fn clearing_the_calculation_interval_restores_the_chart_interval() {
        let daily = super::super::StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(Timeframe::Day1),
            metric: None,
        };
        for (added_on, dialog) in [
            (None, false),
            (None, true),
            (Some(Timeframe::Day1), false),
            (Some(Timeframe::Day1), true),
        ] {
            let mut study = ConfiguredStudy::new(Box::new(performance_alone()));
            if let Some(timeframe) = added_on {
                study.set_timeframe(timeframe);
            }
            study
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Day1)),
                )
                .unwrap();
            study.set_timeframe(Timeframe::Min1);
            let cleared = (
                "calculation_timeframe".to_owned(),
                InputValue::Timeframe(None),
            );
            if dialog {
                let settings = study.settings();
                study
                    .apply_settings(
                        &[cleared, ("wait_for_close".into(), InputValue::Toggle(true))],
                        settings,
                    )
                    .unwrap();
            } else {
                study.set_input(&cleared.0, cleared.1).unwrap();
            }
            assert_eq!(
                study.data_requests(),
                std::slice::from_ref(&daily),
                "added on {added_on:?}, dialog {dialog}"
            );
        }
    }

    /// A study on a daily chart keeps the Interval it accepted once it has run on those bars,
    /// though on them it requests data of its own: the input stays listed, a template saved from
    /// the study keeps it, and it can be cleared.
    #[test]
    fn calculation_interval_stays_listed_and_clears_after_a_run() {
        use crate::model::BarClock;
        let hour = |h: i64| chrono::DateTime::UNIX_EPOCH + chrono::Duration::hours(h);
        let flat = |time| Bar::new(time, 10.0, 10.0, 10.0, 10.0, 1.0);
        let days: Vec<_> = (0..21).map(|day| flat(hour(24 * day))).collect();
        // Each study, the Interval set on it, and the hours between the bars of that Interval.
        let cases: [(Box<dyn Indicator>, Timeframe, i64); 2] = [
            (Box::new(performance_alone()), Timeframe::Hour1, 1),
            (
                Box::new(super::super::Volume24h::new()),
                Timeframe::Week1,
                168,
            ),
        ];
        let (mut observed, mut expected) = (Vec::new(), Vec::new());
        for (inner, interval, step) in cases {
            let name = inner.name().to_owned();
            let mut study = ConfiguredStudy::new(inner);
            study.set_timeframe(Timeframe::Day1);
            study.set_bar_clock(&BarClock::new(Timeframe::Day1));
            let chart_requests = study.data_requests();
            let chosen = InputValue::Timeframe(Some(interval));
            study
                .set_input("calculation_timeframe", chosen.clone())
                .unwrap();
            let bars: Vec<_> = (0..21 * 24 / step)
                .map(|at| flat(hour(at * step)))
                .collect();
            let request = study.data_requests().pop().unwrap();
            study.update_data(
                &days,
                0,
                &[super::super::StudyData {
                    request,
                    bars: &bars,
                    clock: BarClock::new(interval),
                    instrument: None,
                    timeframe: interval,
                    chart: None,
                }],
            );
            let listed = study
                .inputs()
                .into_iter()
                .find(|input| input.id == "calculation_timeframe")
                .map(|input| input.value);
            let template = crate::studies::StudyTemplate::from_study(&study);
            // Saved own inputs can enable Symbol, such as an empty Performance Symbol list.
            for symbol in ["", "OTHER"] {
                for reverse in [false, true] {
                    let mut saved = template.clone();
                    saved
                        .inputs
                        .iter_mut()
                        .find(|(id, _)| id == "calculation_symbol")
                        .unwrap()
                        .1 = InputValue::Symbol(symbol.into());
                    if reverse {
                        saved.inputs.reverse();
                    }
                    let restored = saved.create().unwrap_or_else(|error| {
                        panic!("{name}, symbol {symbol:?}, reverse {reverse}: {error}")
                    });
                    let restored = crate::studies::StudyTemplate::from_study(restored.as_ref());
                    let mut expected = saved.inputs.clone();
                    let mut actual = restored.inputs;
                    expected.sort_by(|a, b| a.0.cmp(&b.0));
                    actual.sort_by(|a, b| a.0.cmp(&b.0));
                    assert_eq!(
                        actual, expected,
                        "{name}, symbol {symbol:?}, reverse {reverse}"
                    );
                    assert_eq!(restored.settings, saved.settings, "{name}");
                    assert_eq!(restored.visible, saved.visible, "{name}");
                    assert_eq!(restored.scale, saved.scale, "{name}");
                }
            }
            let saved = template
                .inputs
                .into_iter()
                .find(|(id, _)| id == "calculation_timeframe")
                .map(|(_, value)| value);
            let cleared = study
                .set_input("calculation_timeframe", InputValue::Timeframe(None))
                .map_err(|error| error.to_string());
            assert_eq!(study.data_requests(), chart_requests, "{name}");
            observed.push((name.clone(), listed, saved, cleared));
            expected.push((
                name,
                Some(chosen.clone()),
                Some(chosen),
                Ok::<(), String>(()),
            ));
        }
        assert_eq!(observed, expected);

        // Switching back to price first makes a separate calculation interval available.
        let mut average = ConfiguredStudy::new(Box::new(super::super::SMA::new(1)));
        average.set_timeframe(Timeframe::Day1);
        average
            .set_input(
                "source",
                InputValue::Source(super::super::StudySource::Study { id: 1, plot: 0 }),
            )
            .unwrap();
        let settings = average.settings();
        average
            .apply_settings(
                &[
                    (
                        "source".into(),
                        InputValue::Source(super::super::StudySource::Price(
                            crate::model::PriceSource::Close,
                        )),
                    ),
                    (
                        "calculation_timeframe".into(),
                        InputValue::Timeframe(Some(Timeframe::Hour1)),
                    ),
                ],
                settings,
            )
            .unwrap();
        assert_eq!(average.data_requests()[0].timeframe, Some(Timeframe::Hour1));
    }

    /// 24-hour Volume on another interval asks for its exchange rates next to that interval's
    /// bars and converts with them, and clearing the Interval or the target restores the
    /// requests of the chart's own interval.
    #[test]
    fn volume_24h_on_another_interval_converts_its_currency() {
        use crate::model::BarClock;
        let hour = |h: i64| chrono::DateTime::UNIX_EPOCH + chrono::Duration::hours(h);
        let flat = |h| Bar::new(hour(h), 10.0, 10.0, 10.0, 10.0, 2.0);
        let usd = Instrument::new("TEST").with_currency("USD");
        let eur = super::super::StudyDataRequest::exchange_rate("USD", "EUR");
        let mut study = ConfiguredStudy::new(Box::new(super::super::Volume24h::new()));
        study.set_timeframe(Timeframe::Hour1);
        study.set_bar_clock(&BarClock::new(Timeframe::Hour1));
        study.set_instrument(Some(&usd));
        // The dialog applies the Interval first, while it is still offered, then the units.
        let chosen = InputValue::Timeframe(Some(Timeframe::Hour4));
        study.set_input("calculation_timeframe", chosen).unwrap();
        study.set_input("units", InputValue::Choice(1)).unwrap();
        study.set_input("currency", InputValue::Choice(2)).unwrap();
        let requests = study.data_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].timeframe, Some(Timeframe::Hour4));
        assert_eq!(requests[1], eur);
        let (chart, bars4, rates) = (
            (0..=4).map(flat).collect::<Vec<_>>(),
            [flat(0), flat(4)],
            [Bar::from_value(hour(0), 0.5)],
        );
        let source = |request, bars, timeframe| super::super::StudyData {
            request,
            bars,
            clock: BarClock::new(timeframe),
            instrument: Some(&usd),
            timeframe,
            chart: None,
        };
        study.update_data(
            &chart,
            0,
            &[
                source(requests[0].clone(), &bars4, Timeframe::Hour4),
                source(eur.clone(), &rates, Timeframe::default()),
            ],
        );
        // The first 4h bar is 2 x 10 x 0.5 = 10 EUR, shown where it closes.
        assert_eq!(study.values().value(0, 4), Some(10.0));
        assert!(study.table().is_none());

        study.set_input("currency", InputValue::Choice(0)).unwrap();
        assert_eq!(study.data_requests().len(), 1);
        study.set_input("currency", InputValue::Choice(2)).unwrap();
        study
            .set_input("calculation_timeframe", InputValue::Timeframe(None))
            .unwrap();
        assert_eq!(study.data_requests(), [eur]);
    }

    /// 24-hour Volume on an Interval longer than a day asks for the lower-interval candles a
    /// chart of that interval asks for, after that interval's bars, and sums the last 24 hours
    /// of each of its bars from them.
    #[test]
    fn volume_24h_on_a_weekly_interval_sums_lower_candles() {
        use crate::model::BarClock;
        let at = |day: i64, hour: i64| {
            chrono::DateTime::UNIX_EPOCH
                + chrono::Duration::days(day)
                + chrono::Duration::hours(hour)
        };
        let flat = |time, volume| Bar::new(time, 10.0, 10.0, 10.0, 10.0, volume);
        let mut study = ConfiguredStudy::new(Box::new(super::super::Volume24h::new()));
        study.set_timeframe(Timeframe::Hour1);
        study.set_bar_clock(&BarClock::new(Timeframe::Hour1));
        let weekly = InputValue::Timeframe(Some(Timeframe::Week1));
        study.set_input("calculation_timeframe", weekly).unwrap();
        let bars_request = study.data_requests()[0].clone();
        let hourly = super::super::StudyDataRequest {
            metric: None,
            symbol: String::new(),
            timeframe: Some(Timeframe::Hour1),
        };
        // Weeks open on Monday 1970-01-05 (day 4) and Monday 1970-01-12 (day 11).
        let weeks = [flat(at(4, 0), 99.0), flat(at(11, 0), 99.0)];
        // The first week's last candle, Sunday 1970-01-11 23:00, closes at 1970-01-12 00:00, so
        // its window holds the candles opening from 1970-01-11 00:00: 2 + 3 = 5. The candle of
        // Saturday 1970-01-10 22:00 is older. The second week has no candles and no value.
        let candles = [
            flat(at(9, 22), 7.0),
            flat(at(10, 22), 2.0),
            flat(at(10, 23), 3.0),
        ];
        let source = |request, bars, timeframe| super::super::StudyData {
            request,
            bars,
            clock: BarClock::new(timeframe),
            instrument: None,
            timeframe,
            chart: None,
        };
        let chart = [flat(at(11, 0), 1.0)];
        study.update_data(
            &chart,
            0,
            &[
                source(bars_request.clone(), &weeks, Timeframe::Week1),
                source(hourly.clone(), &candles, Timeframe::Hour1),
            ],
        );
        // The chart bar at 1970-01-12 00:00 shows the first week, which closed then.
        assert_eq!(study.values().value(0, 0), Some(5.0));
        assert_eq!(study.data_requests(), [bars_request, hourly]);
    }

    #[test]
    fn value_tags_default_by_plot_count_and_persist() {
        use crate::studies::{
            CustomIndicator, IndicatorValue, MARibbon, StudyTemplate, VolumeWeightedAvgPrice,
        };
        for (lines, untagged) in [(4, vec![]), (5, vec![0, 1, 2, 3, 4])] {
            let study = ConfiguredStudy::new(Box::new(
                CustomIndicator::new("Lines", |bars| vec![IndicatorValue::None; bars.len()])
                    .with_line_count(lines),
            ));
            assert_eq!(study.settings().untagged_plots, untagged, "{lines} plots");
        }

        let mut ribbon = ConfiguredStudy::new(Box::new(MARibbon::new()));
        assert_eq!(
            ribbon.settings().untagged_plots,
            (0..12).collect::<Vec<_>>()
        );
        let mut settings = ribbon.settings();
        settings.untagged_plots = (1..11).collect();
        ribbon.apply_settings(&[], settings).unwrap();
        let template = StudyTemplate::from_study(&ribbon);
        let json = serde_json::to_string(&template).unwrap();
        let restored = serde_json::from_str::<StudyTemplate>(&json)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            restored.settings().untagged_plots,
            (1..11).collect::<Vec<_>>()
        );
        assert!(restored.plot_tagged(0) && restored.plot_tagged(11) && !restored.plot_tagged(5));
        // Settings saved before the field existed tag every plot, as they did.
        let mut old = serde_json::to_value(&template).unwrap();
        old["settings"]
            .as_object_mut()
            .unwrap()
            .remove("untagged_plots");
        let restored = serde_json::from_value::<StudyTemplate>(old)
            .unwrap()
            .create()
            .unwrap();
        assert!((0..12).all(|line| restored.plot_tagged(line)));

        // Two bands take VWAP from one plot to five: the kept VWAP line stays tagged.
        let mut vwap = ConfiguredStudy::new(Box::new(VolumeWeightedAvgPrice::new()));
        let settings = vwap.settings();
        vwap.apply_settings(&[("band_count".into(), InputValue::Length(2))], settings)
            .unwrap();
        assert_eq!(vwap.settings().untagged_plots, [1, 2, 3, 4]);
        let mut settings = vwap.settings();
        settings.untagged_plots = vec![3, 1, 3, 7];
        vwap.apply_settings(&[], settings).unwrap();
        assert_eq!(vwap.settings().untagged_plots, [1, 3]);
    }

    #[test]
    fn automatic_precision_tracks_metadata_and_preserves_study_overrides() {
        let mut study = ConfiguredStudy::new(Box::new(super::super::MACD::default()));
        let instrument = Instrument::new("FX").with_tick_size(0.00001);
        study.set_instrument(Some(&instrument));
        assert_eq!(study.precision(), Some(5));
        assert_eq!(study.settings().precision, None);
        let mut settings = study.settings();
        settings.precision = Some(3);
        study.apply_settings(&[], settings).unwrap();
        study.set_instrument(Some(&Instrument::new("INT").with_tick_size(1.0)));
        assert_eq!(study.precision(), Some(3));
        let mut settings = study.settings();
        settings.precision = None;
        study.apply_settings(&[], settings).unwrap();
        assert_eq!(study.precision(), Some(0));
        study.set_instrument(None);
        assert_eq!(study.precision(), None);
        for (bounded, precision) in [
            (
                Box::new(super::super::RSI::default()) as Box<dyn Indicator>,
                Some(2),
            ),
            (Box::new(super::super::Stochastic::default()), Some(2)),
            (Box::new(super::super::OnBalanceVolume::default()), None),
            (
                Box::new(super::super::DirectionalMovement::default()),
                Some(4),
            ),
            (
                Box::new(super::super::TrendStrengthIndex::default()),
                Some(2),
            ),
            (
                Box::new(
                    super::super::CustomIndicator::new("Small", |_| Vec::new())
                        .with_value_format(crate::scales::StudyFormat::Fixed(4)),
                ),
                Some(4),
            ),
        ] {
            let mut bounded = ConfiguredStudy::new(bounded);
            bounded.set_instrument(Some(&instrument));
            let mut settings = bounded.settings();
            settings.precision = None;
            bounded.apply_settings(&[], settings).unwrap();
            assert_eq!(bounded.precision(), precision);
        }
    }

    /// Automatic volume units follow the instrument, while a chosen precision wins. With
    /// three decimals, 0.375 stays fractional and 1234.5 / 1000 rounds to 1.234 in binary;
    /// without metadata the same values keep their former 0 and 1.23K text. At 0 volume
    /// decimals the units keep two decimals: 1.23K. 1,234,567,890 / 10^9 = 1.23456789 reads
    /// 1.23B, 1.235B at three decimals and 1.2B at a chosen one. Order flow studies in volume
    /// units follow the same rule on every line and axis mark: 1,234.5678 / 1,000 = 1.2345678
    /// reads 1.235K at three volume decimals and 1.23K at 0 or without them, 2,500,000 reads
    /// 2.500M and 2.50M.
    #[test]
    fn volume_units_follow_the_instrument_volume_decimals() {
        use crate::data::OrderFlowKind;
        use crate::studies::OrderFlowStudy;
        type Row = (Option<u8>, Option<usize>, Option<usize>, [&'static str; 3]);
        let rows: [Row; 4] = [
            (None, None, None, ["0", "1.23K", "1.23B"]),
            (Some(3), None, Some(3), ["0.375", "1.234K", "1.235B"]),
            (Some(3), Some(1), Some(1), ["0.4", "1.2K", "1.2B"]),
            (Some(0), None, Some(0), ["0", "1.23K", "1.23B"]),
        ];
        let mut wrong = Vec::new();
        for (decimals, chosen, precision, expected) in rows {
            for study in [
                Box::new(super::super::Volume::default()) as Box<dyn Indicator>,
                Box::new(super::super::OnBalanceVolume::default()),
                Box::new(super::super::CumulativeVolumeIndex::default()),
            ] {
                let name = study.name().to_owned();
                let mut study = ConfiguredStudy::new(study);
                let mut settings = study.settings();
                settings.precision = chosen;
                study.apply_settings(&[], settings).unwrap();
                let mut instrument = Instrument::new("COIN");
                instrument.volume_decimals = decimals;
                study.set_instrument(Some(&instrument));
                let values = [0.375, 1234.5, 1_234_567_890.0];
                let got = (
                    study.precision(),
                    values.map(|value| study.format_value(value)),
                    values.map(|value| study.format_plot_value(0, value)),
                );
                let want = (
                    precision,
                    expected.map(String::from),
                    expected.map(String::from),
                );
                if got != want {
                    wrong.push(format!(
                        "{name}, {decimals:?}, {chosen:?}: got {got:?}, want {want:?}"
                    ));
                }
            }
        }
        for (decimals, expected) in [
            (None, ["1.23K", "2.50M"]),
            (Some(0), ["1.23K", "2.50M"]),
            (Some(3), ["1.235K", "2.500M"]),
        ] {
            for study in [
                OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
                    min: 0.0,
                    max: f64::MAX,
                }),
                OrderFlowStudy::new(OrderFlowKind::SizeDelta {
                    ranges: [[0.0, 10.0]; 3],
                }),
                OrderFlowStudy::new(OrderFlowKind::BookSize {
                    levels: 10,
                    distance: 0.0,
                }),
                OrderFlowStudy::new(OrderFlowKind::FlowSum { bars: 2 }),
                OrderFlowStudy::execution_delta(false),
                OrderFlowStudy::execution_delta(true),
                OrderFlowStudy::new(OrderFlowKind::TradeRate {
                    volume: true,
                    seconds: 60,
                    instant: 5,
                }),
            ] {
                let name = study.name().to_owned();
                let mut study = ConfiguredStudy::new(Box::new(study));
                let mut instrument = Instrument::new("COIN");
                instrument.volume_decimals = decimals;
                study.set_instrument(Some(&instrument));
                for (value, expected) in [1234.5678, 2_500_000.0].into_iter().zip(expected) {
                    // The axis labels its marks with `format_value`.
                    let mut got: Vec<_> = (0..study.line_count().min(3))
                        .map(|line| study.format_plot_value(line, value))
                        .collect();
                    got.push(study.format_value(value));
                    if got.iter().any(|text| text != expected) {
                        wrong.push(format!(
                            "{name}, {decimals:?}: got {got:?}, want {expected}"
                        ));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// Net Volume starts as a line; PPO and PVO keep columns for their differences.
    /// A saved plot type replaces each default.
    #[test]
    fn histogram_defaults_yield_to_a_saved_plot_type() {
        use super::super::{NetVolume, PercentageVolumeOscillator, PriceOscillator};
        let (line, columns) = (PlotType::Line, PlotType::Columns);
        let studies: [(Box<dyn Indicator>, &[PlotType]); 3] = [
            (Box::new(NetVolume::default()), &[line]),
            (Box::new(PriceOscillator::default()), &[line, line, columns]),
            (
                Box::new(PercentageVolumeOscillator::default()),
                &[line, line, columns],
            ),
        ];
        for (study, expected) in studies {
            let name = study.name().to_owned();
            let mut study = ConfiguredStudy::new(study);
            let types = |study: &ConfiguredStudy| -> Vec<_> {
                study.settings().plots.iter().map(|p| p.plot_type).collect()
            };
            assert_eq!(types(&study), expected, "{name}");
            let mut settings = study.settings();
            settings.plots[0].plot_type = PlotType::Area;
            study.apply_settings(&[], settings).unwrap();
            assert_eq!(study.plot_style(0).plot_type, PlotType::Area, "{name}");
        }
    }

    /// Settings saved before joined types existed name the types with breaks `Line`, `StepLine`
    /// and `Area`, so they restore with the gaps they were drawn with; the joined types save
    /// under names of their own.
    #[test]
    fn plot_types_keep_their_saved_names() {
        for (saved, plot_type) in [
            ("Line", PlotType::LineWithBreaks),
            ("StepLine", PlotType::StepLineWithBreaks),
            ("Area", PlotType::AreaWithBreaks),
            ("JoinedLine", PlotType::Line),
            ("JoinedStepLine", PlotType::StepLine),
            ("JoinedArea", PlotType::Area),
            ("StepLineWithDiamonds", PlotType::StepLineWithDiamonds),
        ] {
            let json = format!("\"{saved}\"");
            assert_eq!(
                serde_json::from_str::<PlotType>(&json).unwrap(),
                plot_type,
                "{saved}"
            );
            assert_eq!(serde_json::to_string(&plot_type).unwrap(), json, "{saved}");
        }
    }

    /// VWAP and all six band values move together, including the filled first band pair.
    #[test]
    fn an_offset_moves_vwap_auto_anchored_with_its_bands() {
        use super::super::{AutoAnchor, FillBoundary, VwapAutoAnchored};
        let bars: Vec<_> = [(10.0, 100.0), (11.0, 10.0), (12.0, 10.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (close, volume))| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i as i64),
                    close,
                    close + 1.0,
                    close - 1.0,
                    close,
                    volume,
                )
            })
            .collect();
        let mut study = ConfiguredStudy::new(Box::new(
            VwapAutoAnchored::new()
                .with_anchor(AutoAnchor::HighestVolume)
                .with_length(3),
        ));
        study
            .set_input("plot_offset", InputValue::Integer(2))
            .unwrap();
        study.calculate(&bars);
        assert_eq!(study.line_count(), 7);
        for line in 0..7 {
            assert_eq!(study.plot_offset(line), 2);
            assert_eq!(study.plot_value(line, 0), None);
            assert_eq!(study.plot_value(line, 1), None);
            for bar in 0..3 {
                assert_eq!(
                    study.plot_value(line, bar + 2),
                    study.study.values().value(line, bar)
                );
            }
        }
        // Sum(v*p)=1230, sum(v)=120; E(p^2)=12650/120, so variance=17/48.
        assert_eq!(study.plot_value(0, 2), Some(10.0));
        assert_eq!(study.plot_value(0, 4), Some(10.25));
        assert_eq!(study.plot_value(1, 2), Some(10.0));
        for (line, sign) in [(1, 1.0), (2, -1.0)] {
            let expected = 10.25 + sign * (17.0f64 / 48.0).sqrt();
            assert!((study.plot_value(line, 4).unwrap() - expected).abs() < 1e-12);
        }
        let fills = study.fills();
        assert_eq!(fills.len(), 1);
        assert!(matches!(fills[0].first, FillBoundary::Plot(1)));
        assert!(matches!(fills[0].second, FillBoundary::Plot(2)));
    }

    /// Crossing markers remain on their bars: Offset and a crossing pair cannot coexist.
    /// One Apply can clear either of them before enabling the other, in either input order.
    #[test]
    fn an_offset_study_draws_no_markers_it_would_not_move() {
        use super::super::{IndicatorFactory, StudyError, test_support::make_bar};
        let factory = IndicatorFactory::new();
        let mut study = ConfiguredStudy::new(factory.create("Multi MA").unwrap());
        study
            .set_input("plot_offset", InputValue::Integer(2))
            .unwrap();
        study
            .set_input("cross_first", InputValue::Choice(1))
            .unwrap();
        assert!(
            matches!(study.set_input("cross_second", InputValue::Choice(2)),
            Err(StudyError::InvalidInput { id, .. }) if id == "cross_second")
        );
        assert_eq!(
            study
                .inputs()
                .into_iter()
                .find(|input| input.id == "cross_second")
                .unwrap()
                .value,
            InputValue::Choice(0)
        );
        let bars: Vec<_> = (0..60)
            .chain((1..=60).rev())
            .map(|close| make_bar(f64::from(close)))
            .collect();
        for reverse in [false, true] {
            let mut edited = study.clone();
            let mut inputs = vec![
                ("cross_first".into(), InputValue::Choice(1)),
                ("cross_second".into(), InputValue::Choice(2)),
                ("plot_offset".into(), InputValue::Integer(0)),
            ];
            if reverse {
                inputs.reverse();
            }
            edited.apply_settings(&inputs, edited.settings()).unwrap();
            assert!(
                !edited
                    .inputs()
                    .iter()
                    .any(|input| input.id == "plot_offset")
            );
            // A list read before the pair withdrew Offset still applies with its Offset 0.
            edited.apply_settings(&inputs, edited.settings()).unwrap();
            edited.calculate(&bars);
            assert!(!edited.markers().is_empty());
            assert!(
                edited
                    .set_input("plot_offset", InputValue::Integer(2))
                    .is_err()
            );

            let mut edited = ConfiguredStudy::new(factory.create("MA Cross").unwrap());
            let mut inputs = vec![
                ("plot_offset".into(), InputValue::Integer(2)),
                ("cross_first".into(), InputValue::Choice(0)),
            ];
            if reverse {
                inputs.reverse();
            }
            edited.apply_settings(&inputs, edited.settings()).unwrap();
            assert_eq!(edited.plot_offset(0), 2);
            assert!(
                edited
                    .inputs()
                    .iter()
                    .any(|input| input.id == "plot_offset")
            );
        }
    }

    /// The listed controls and accepted edits agree for bar-only and multi-output studies,
    /// and studies whose outputs cannot follow another symbol or interval offer neither.
    #[test]
    fn sourceless_studies_offer_symbol_and_offset_as_they_draw() {
        for (name, symbol, interval, offset) in [
            ("PSAR", true, true, true),
            ("MA Cross", true, true, false),
            ("VWAP Auto Anchored", true, true, true),
            ("Multi MA", true, true, true),
            ("Donchian", true, true, false),
            ("Volume", true, true, false),
            ("Ratio", false, true, false),
            ("Advance/Decline Ratio", false, false, false),
            ("Quoted Spread", false, false, false),
            // Drawn by the chart from its own bars or symbol.
            ("Time Price Opportunity", false, false, false),
            ("Analyst price forecast", false, false, false),
            // Labels, drawings, projections and segments sit on source bars.
            ("ZigZag", false, false, false),
            ("Trading Sessions", false, false, false),
            #[cfg(feature = "drawings")]
            ("Auto Trend Line", false, false, false),
            #[cfg(feature = "drawings")]
            ("Auto Fibonacci Retracement", false, false, false),
            #[cfg(feature = "drawings")]
            ("Auto Fibonacci Extension", false, false, false),
            #[cfg(feature = "drawings")]
            ("Auto Parallel Channel", false, false, false),
            #[cfg(feature = "drawings")]
            ("Auto Pitchfork", false, false, false),
            ("Double Top", false, false, false),
            ("Elliott Wave", false, false, false),
            ("Auto Key Levels", false, false, false),
            ("Ziv Ghost Pivots", false, false, false),
            ("Knoxville Divergence", false, false, false),
            ("Seasonality", false, false, true),
            ("Intraday Pivot Points", false, false, false),
            ("Pivot Points Standard", false, false, false),
            ("Projected Pivot Points", false, false, false),
        ] {
            let factory = super::super::IndicatorFactory::new();
            let study = ConfiguredStudy::new(factory.create(name).unwrap());
            let ids: Vec<_> = study.inputs().into_iter().map(|input| input.id).collect();
            assert_eq!(
                ids.iter().any(|id| id == "calculation_symbol"),
                symbol,
                "{name}"
            );
            // Breadth has its own close-wait input, independent of the chart's controls.
            let wait = interval || name == "Advance/Decline Ratio";
            for (id, offered) in [
                ("calculation_timeframe", interval),
                ("wait_for_close", wait),
            ] {
                assert_eq!(
                    ids.iter().any(|listed| listed == id),
                    offered,
                    "{name} {id}"
                );
            }
            assert_eq!(ids.iter().any(|id| id == "plot_offset"), offset, "{name}");
            for (id, value, accepted) in [
                ("calculation_symbol", InputValue::Symbol("X".into()), symbol),
                (
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Day1)),
                    interval,
                ),
                ("wait_for_close", InputValue::Toggle(false), wait),
                ("plot_offset", InputValue::Integer(1), offset),
            ] {
                let mut study = study.clone();
                assert_eq!(study.set_input(id, value).is_ok(), accepted, "{name} {id}");
            }
        }
    }

    /// Guards #3898: every catalogue study that lists Symbol draws, on a symbol whose history
    /// has the chart's bar times, every kind of output it draws on the chart's own bars.
    #[test]
    fn studies_listing_symbol_keep_every_output_on_it() {
        use crate::model::BarClock;
        use crate::studies::{IndicatorFactory, StudyData};
        let start: chrono::DateTime<chrono::Utc> = "2026-02-02T00:00:00Z".parse().unwrap();
        let close = |i: f64| 100.0 + 10.0 * (i / 7.0).sin() + 5.0 * (i / 23.0).sin() + 0.02 * i;
        let bars: Vec<_> = (0..600)
            .map(|i| {
                let price = close(f64::from(i));
                let open = if i == 0 {
                    100.0
                } else {
                    close(f64::from(i - 1))
                };
                Bar::new(
                    start + chrono::TimeDelta::hours(i.into()),
                    open,
                    open.max(price) + 1.0,
                    open.min(price) - 1.0,
                    price,
                    100.0 + f64::from(i),
                )
            })
            .collect();
        let kinds = |study: &ConfiguredStudy| {
            [
                #[cfg(feature = "drawings")]
                ("drawings", !study.drawings().is_empty()),
                ("labels", !study.labels().is_empty()),
                ("projections", !study.projections().is_empty()),
                ("price segments", !study.price_segments().is_empty()),
                ("markers", !study.markers().is_empty()),
                ("zones", !study.zones().is_empty()),
                ("candles", !study.candles().is_empty()),
                ("profiles", !study.volume_profiles(0..bars.len()).is_empty()),
                (
                    "profile levels",
                    !study.profile_levels(0..bars.len()).is_empty(),
                ),
            ]
            .into_iter()
            .filter_map(|(kind, shown)| shown.then_some(kind))
            .collect::<Vec<_>>()
        };
        let factory = IndicatorFactory::new();
        let mut names = factory.list();
        names.sort();
        let mut lost = Vec::new();
        for name in names {
            let mut study = ConfiguredStudy::new(factory.create(&name).unwrap());
            study.set_timeframe(Timeframe::Hour1);
            study.set_bar_clock(&BarClock::new(Timeframe::Hour1));
            if !study
                .inputs()
                .iter()
                .any(|input| input.id == "calculation_symbol")
            {
                continue;
            }
            study.update_data(&bars, 0, &[]);
            let own = kinds(&study);
            study
                .set_input("calculation_symbol", InputValue::Symbol("X".into()))
                .unwrap();
            let source = StudyData {
                request: study.data_requests().remove(0),
                bars: &bars,
                clock: BarClock::new(Timeframe::Hour1),
                instrument: None,
                timeframe: Timeframe::Hour1,
                chart: None,
            };
            study.update_data(&bars, 0, &[source]);
            let other = kinds(&study);
            let missing: Vec<_> = own
                .into_iter()
                .filter(|kind| !other.contains(kind))
                .collect();
            if !missing.is_empty() {
                lost.push((name, missing));
            }
        }
        assert!(
            lost.is_empty(),
            "output kinds lost on another symbol: {lost:?}"
        );
    }

    /// Studies that read the chart symbol's bars offer Symbol, Interval and close-wait, keep
    /// their own inputs, and calculate on the bars they are given. Studies reading other
    /// symbols, metrics or trades keep their existing inputs.
    #[test]
    fn ohlc_studies_offer_a_calculation_interval() {
        use super::super::{ADX, ATR, IchimokuCloud, OHLC4, StudyDataRequest};
        let ids = |study: &ConfiguredStudy| -> Vec<String> {
            study.inputs().into_iter().map(|input| input.id).collect()
        };
        for (inner, offset) in [
            (Box::new(ATR::new(3)) as Box<dyn Indicator>, true),
            (Box::new(ADX::new(3)), true),
            (Box::new(IchimokuCloud::default()), false),
            (Box::new(OHLC4::new()), true),
        ] {
            let mut study = ConfiguredStudy::new(inner);
            let own = study.study.inputs().len();
            let ids = ids(&study);
            let mut expected = vec![
                "calculation_symbol",
                "calculation_timeframe",
                "wait_for_close",
            ];
            if offset {
                expected.insert(0, "plot_offset");
            }
            assert_eq!(&ids[own..], expected, "{}", study.name());
            assert!(study.data_requests().is_empty());
            study
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Hour1)),
                )
                .unwrap();
            assert_eq!(study.data_requests().len(), 1);
            study
                .set_input("calculation_symbol", InputValue::Symbol("X".into()))
                .unwrap();
            assert_eq!(
                study.data_requests(),
                [StudyDataRequest {
                    symbol: "X".into(),
                    timeframe: Some(Timeframe::Hour1),
                    metric: None,
                }],
                "{}",
                study.name()
            );
        }
        let mut study = ConfiguredStudy::new(Box::new(super::super::TechnicalRatings::default()));
        study.set_timeframe(Timeframe::Min5);
        let before = study.snapshot_settings();
        study
            .set_input("table_intervals", InputValue::Toggle(true))
            .unwrap();
        let own = study.study.inputs().len();
        assert_eq!(
            &ids(&study)[own..],
            [
                "calculation_symbol",
                "calculation_timeframe",
                "wait_for_close"
            ]
        );
        study
            .set_input(
                "calculation_timeframe",
                InputValue::Timeframe(Some(Timeframe::Hour1)),
            )
            .unwrap();
        let requests = study.data_requests();
        assert_eq!(
            requests
                .iter()
                .map(|request| request.timeframe.unwrap())
                .collect::<Vec<_>>(),
            [
                Timeframe::Hour1,
                Timeframe::Hour4,
                Timeframe::Day1,
                Timeframe::Week1,
                Timeframe::Month1
            ]
        );
        let hourly = [10., 12.]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::hours(i as i64),
                    close,
                    close,
                    close,
                    close,
                    1.,
                )
            })
            .collect::<Vec<_>>();
        let chart = (0..12)
            .map(|i| {
                Bar::new(
                    hourly[1].time + chrono::TimeDelta::minutes(i * 5),
                    12.,
                    12.,
                    12.,
                    12.,
                    1.,
                )
            })
            .collect::<Vec<_>>();
        let source = |request: &super::super::StudyDataRequest, bars| super::super::StudyData {
            request: request.clone(),
            bars,
            clock: crate::model::BarClock::new(request.timeframe.unwrap()),
            instrument: None,
            timeframe: request.timeframe.unwrap(),
            chart: None,
        };
        study.update_data(
            &chart,
            0,
            &[
                source(&requests[0], &hourly[..]),
                source(&requests[1], &[][..]),
            ],
        );
        let row: Vec<_> = study.table().unwrap().rows[0]
            .iter()
            .map(|cell| match cell {
                super::super::StudyCell::Text(text) => text.as_str(),
                _ => panic!("text cell"),
            })
            .collect();
        assert_eq!(row, ["1h", "Strong Buy", "N/A", "Strong Buy", "23%"]);
        // The table rates the forming hour while the confirmed plot holds the previous hour.
        assert_eq!(study.values().value(2, chart.len() - 1), Some(0.0));
        study
            .set_input("wait_for_close", InputValue::Toggle(false))
            .unwrap();
        study.update_data(
            &chart,
            0,
            &[
                source(&requests[0], &hourly[..]),
                source(&requests[1], &[][..]),
            ],
        );
        assert_eq!(study.values().value(2, chart.len() - 1), Some(1.0));
        assert_eq!(study.data_start(&requests[0]), None);
        assert_eq!(
            study.data_start(&requests[1]),
            Some(hourly[1].time - chrono::TimeDelta::hours(8000))
        );
        study
            .set_input("calculation_symbol", InputValue::Symbol("X".into()))
            .unwrap();
        let other = study.data_requests();
        assert_eq!(
            other,
            requests
                .iter()
                .map(|request| super::super::StudyDataRequest {
                    symbol: "X".into(),
                    ..request.clone()
                })
                .collect::<Vec<_>>()
        );
        study.update_data(
            &chart,
            0,
            &[source(&other[0], &hourly[..]), source(&other[1], &[][..])],
        );
        assert_eq!(study.data_start(&other[0]), None);
        assert_eq!(
            study.data_start(&other[1]),
            Some(hourly[1].time - chrono::TimeDelta::hours(8000))
        );
        study
            .set_input("calculation_symbol", InputValue::Symbol(String::new()))
            .unwrap();
        study.restore_settings(&before).unwrap();
        let values = study.inputs();
        for (id, value) in [
            ("table_intervals", InputValue::Toggle(false)),
            ("calculation_timeframe", InputValue::Timeframe(None)),
            ("wait_for_close", InputValue::Toggle(true)),
        ] {
            assert_eq!(
                values.iter().find(|input| input.id == id).unwrap().value,
                value
            );
        }
        let breadth = ConfiguredStudy::new(Box::new(super::super::AdvanceDeclineRatio::default()));
        assert!(!ids(&breadth).contains(&"calculation_timeframe".to_owned()));
        let factory = super::super::IndicatorFactory::new();
        let mut excluded: Vec<_> = ["Correlation Coefficient", "Ratio", "Spread", "Performance"]
            .into_iter()
            .map(|name| factory.create_registered(name).unwrap())
            .collect();
        excluded.push(Box::new(super::super::MetricStudy::new(
            crate::data::MetricSeries::new(
                "oi",
                "Open interest",
                crate::scales::StudyFormat::Volume,
            ),
        )));
        for inner in excluded {
            let mut study = ConfiguredStudy::new(inner);
            assert!(
                !ids(&study).contains(&"calculation_symbol".to_owned()),
                "{}",
                study.name()
            );
            assert!(
                study
                    .set_input("calculation_symbol", InputValue::Symbol("X".into()))
                    .is_err(),
                "{}",
                study.name()
            );
        }
    }

    /// A bar study retains its calculation symbol through Cancel, saved settings and templates.
    #[test]
    fn bar_study_symbol_restores_from_settings_and_templates() {
        use super::super::{IchimokuCloud, StudyTemplate};
        let make = || ConfiguredStudy::new(Box::new(IchimokuCloud::default()));
        let symbol = |study: &dyn Indicator| {
            study
                .inputs()
                .into_iter()
                .find(|input| input.id == "calculation_symbol")
                .unwrap()
                .value
        };
        let mut study = make();
        let before = study.snapshot_settings();
        study
            .set_input("calculation_symbol", InputValue::Symbol("OTHER".into()))
            .unwrap();
        let edited = study.snapshot_settings();
        let mut restored = make();
        restored.restore_settings(&edited).unwrap();
        assert_eq!(symbol(&restored), InputValue::Symbol("OTHER".into()));
        assert_eq!(restored.data_requests()[0].symbol, "OTHER");
        restored.restore_settings(&before).unwrap();
        assert_eq!(symbol(&restored), InputValue::Symbol(String::new()));
        assert!(restored.data_requests().is_empty());
        let json = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
        let template: StudyTemplate = serde_json::from_str(&json).unwrap();
        let restored = template.create().unwrap();
        assert_eq!(
            symbol(restored.as_ref()),
            InputValue::Symbol("OTHER".into())
        );
        assert_eq!(restored.data_requests()[0].symbol, "OTHER");
    }

    /// Requests added after another symbol's bars arrive follow that symbol, including when a
    /// chart interval change makes Volume24h and Performance without comparisons
    /// need bars of a stated interval. Each must accept the symbol and request its own bars.
    #[test]
    fn bar_study_on_another_symbol_reads_its_own_bars_on_that_symbol() {
        use super::super::{StudyData, StudyDataRequest, Volume24h};
        use crate::model::BarClock;
        let cases: [(Box<dyn Indicator>, Timeframe, Timeframe); 2] = [
            (
                Box::new(Volume24h::new()),
                Timeframe::Week1,
                Timeframe::Hour1,
            ),
            (
                Box::new(performance_alone()),
                Timeframe::Min5,
                Timeframe::Day1,
            ),
        ];
        let request = |timeframe| StudyDataRequest {
            symbol: "OTHER".into(),
            timeframe,
            metric: None,
        };
        for (inner, interval, stated) in cases {
            let mut study = ConfiguredStudy::new(inner);
            study.set_timeframe(Timeframe::Day1);
            study.set_bar_clock(&BarClock::new(Timeframe::Day1));
            assert!(study.data_requests().is_empty(), "{}", study.name());
            study
                .set_input("calculation_symbol", InputValue::Symbol("OTHER".into()))
                .unwrap();
            study.set_timeframe(interval);
            study.set_bar_clock(&BarClock::new(interval));
            assert_eq!(study.data_requests()[0], request(None));
            let start: chrono::DateTime<chrono::Utc> = "2026-03-02T00:00:00Z".parse().unwrap();
            let bars: Vec<_> = (0..3)
                .map(|i| Bar::new(start + interval.duration() * i, 10.0, 10.0, 10.0, 10.0, 1.0))
                .collect();
            study.update_data(
                &bars,
                0,
                &[StudyData {
                    request: request(None),
                    bars: &bars,
                    clock: BarClock::new(interval),
                    instrument: None,
                    timeframe: interval,
                    chart: None,
                }],
            );
            assert_eq!(
                study.data_requests(),
                [request(None), request(Some(stated))],
                "{}",
                study.name()
            );
            assert!(
                study
                    .inputs()
                    .iter()
                    .any(|input| input.id == "calculation_symbol")
            );
        }
    }

    /// Volume profiles, zones, candles and markers calculated on a finer or a coarser interval
    /// lie on the chart bars that share time with their bars, a marker once the values on the chart
    /// reach its bar. Projections are hidden, and no marks remain when the history is missing.
    #[test]
    fn profiles_zones_and_markers_of_another_interval_lie_on_their_chart_bars() {
        use super::super::{
            CrossDirection, IndicatorMarker, StudyCandle, StudyData, StudyProjection, StudyZone,
            VolProfileSession, VolProfileVisible,
        };
        // Markers on the second and the last bar, and a zone from the one to the other.
        #[derive(Clone, Default)]
        struct Marks {
            values: IndicatorValues,
            markers: Vec<IndicatorMarker>,
            zones: Vec<StudyZone>,
            candles: Vec<StudyCandle>,
            projections: Vec<StudyProjection>,
            variants: bool,
        }
        impl Indicator for Marks {
            fn name(&self) -> &str {
                "Marks"
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.values = IndicatorValues::missing(data.len());
                self.candles = data
                    .iter()
                    .enumerate()
                    .map(|(i, bar)| StudyCandle {
                        bars: i..i + 1,
                        bar: bar.clone(),
                        plot: 0,
                    })
                    .collect();
                self.projections = vec![StudyProjection {
                    bars: 1.0..2.0,
                    top: 101.0,
                    bottom: 100.0,
                    plot: 0,
                }];
                let last = data.len().saturating_sub(1);
                let marker = |bar| IndicatorMarker {
                    bar,
                    value: 100.0,
                    direction: CrossDirection::Up,
                };
                let zone = StudyZone {
                    bars: 1..last + 1,
                    top: 101.0,
                    bottom: 100.0,
                    plot: 0,
                };
                (self.markers, self.zones) = if last > 1 {
                    (vec![marker(1), marker(last)], vec![zone])
                } else {
                    (Vec::new(), Vec::new())
                };
            }
            fn values(&self) -> &IndicatorValues {
                &self.values
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::WHITE]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn set_visible(&mut self, _: bool) {}
            fn plot_color(&self, _: usize, bar: usize) -> Option<Color32> {
                Some(Color32::from_gray((bar + 1) as u8))
            }
            fn plot_variants(&self, _: usize) -> Vec<PlotVariant> {
                ["Even", "Odd"]
                    .map(|name| PlotVariant {
                        name: name.into(),
                        color: Color32::WHITE,
                        width: 1.0,
                    })
                    .to_vec()
            }
            fn plot_variant(&self, _: usize, bar: usize) -> Option<usize> {
                self.variants.then_some(bar % 2)
            }
            fn markers(&self) -> &[IndicatorMarker] {
                &self.markers
            }
            fn zones(&self) -> &[StudyZone] {
                &self.zones
            }
            fn candles(&self) -> &[StudyCandle] {
                &self.candles
            }
            fn projections(&self) -> &[StudyProjection] {
                &self.projections
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
        }
        // Bar i opens i intervals after 23:50 UTC on 6 March, so midnight starts a second
        // session, and trades a volume of 1 from 100 + i to 101 + i.
        let start: chrono::DateTime<chrono::Utc> = "2026-03-06T23:50:00Z".parse().unwrap();
        let bars = |count: i32, interval: Timeframe| -> Vec<Bar> {
            (0..count)
                .map(|i| {
                    let low = 100.0 + f64::from(i);
                    let time = start + interval.duration() * i;
                    Bar::new(time, low, low + 1.0, low, low + 1.0, 1.0)
                })
                .collect()
        };
        let profiles = |study: &ConfiguredStudy, visible| -> Vec<_> {
            study
                .volume_profiles(visible)
                .into_iter()
                .map(|plot| {
                    let rows = &plot.profile;
                    (plot.bars, rows.low(), rows.high(), rows.volume().to_vec())
                })
                .collect()
        };
        for (name, [chart, other], sessions, (view, visible), markers, zones, levels, candles) in [
            (
                "finer",
                [(4, Timeframe::Min5), (20, Timeframe::Min1)],
                [
                    (0..2, 100.0, 110.0, vec![10.0]),
                    (2..4, 110.0, 120.0, vec![10.0]),
                ],
                (1..3, (1..3, 105.0, 115.0, vec![10.0])),
                vec![0, 3],
                0..4,
                (2..4, 105.0),
                (0..20).map(|i| i / 5..i / 5 + 1).collect::<Vec<_>>(),
            ),
            (
                "coarser",
                [(20, Timeframe::Min1), (4, Timeframe::Min5)],
                [
                    (0..10, 100.0, 102.0, vec![2.0]),
                    (10..20, 102.0, 104.0, vec![2.0]),
                ],
                (3..12, (0..15, 100.0, 103.0, vec![3.0])),
                vec![9],
                5..20,
                (10..20, 101.0),
                (0..4).map(|i| 5 * i..5 * i + 5).collect(),
            ),
        ] {
            let (interval, source_interval) = (chart.1, other.1);
            let (chart, source) = (bars(chart.0, interval), bars(other.0, source_interval));
            let calculated = |inner: Box<dyn Indicator>, wait| {
                let mut study = ConfiguredStudy::new(inner);
                study.set_timeframe(interval);
                let other = InputValue::Timeframe(Some(source_interval));
                study.set_input("calculation_timeframe", other).unwrap();
                study
                    .set_input("wait_for_close", InputValue::Toggle(wait))
                    .unwrap();
                let data = StudyData {
                    request: study.data_requests().remove(0),
                    bars: &source,
                    clock: crate::model::BarClock::new(source_interval),
                    instrument: None,
                    timeframe: source_interval,
                    chart: None,
                };
                study.update_data(&chart, 0, &[data]);
                study
            };
            let mut session = VolProfileSession::new(1);
            session
                .set_input("extend_poc", InputValue::Toggle(true))
                .unwrap();
            let mut studies = [
                calculated(Box::new(session), true),
                calculated(Box::new(VolProfileVisible::new(1)), true),
                calculated(Box::<Marks>::default(), true),
            ];
            assert_eq!(profiles(&studies[0], 0..chart.len()), sessions, "{name}");
            let extended: Vec<_> = studies[0]
                .profile_levels(0..chart.len())
                .into_iter()
                .map(|level| (level.bars, level.price))
                .collect();
            assert_eq!(extended, [levels], "{name}");
            assert_eq!(profiles(&studies[1], view), [visible], "{name}");
            let marked: Vec<_> = studies[2]
                .markers()
                .iter()
                .map(|marker| marker.bar)
                .collect();
            assert_eq!(marked, markers, "{name}");
            let zone = StudyZone {
                bars: zones,
                top: 101.0,
                bottom: 100.0,
                plot: 0,
            };
            assert_eq!(studies[2].zones(), [zone], "{name}");
            assert_eq!(
                studies[2]
                    .candles()
                    .iter()
                    .map(|candle| candle.bars.clone())
                    .collect::<Vec<_>>(),
                candles,
                "{name}"
            );
            assert!(studies[2].projections().is_empty(), "{name}");
            // The renderer must read each candle's own source bar, even when several share
            // a chart bar or its aligned line still holds the preceding source bar's value.
            #[cfg(feature = "widget")]
            for wait in [true, false] {
                for variants in [false, true] {
                    use crate::chart::{coords::ChartMapping, rendering::RenderContext};
                    use crate::scales::{PriceMap, PriceRange, PriceTransform};
                    let mut study = calculated(
                        Box::new(Marks {
                            variants,
                            ..Marks::default()
                        }),
                        wait,
                    );
                    let colors = [Color32::RED, Color32::BLUE];
                    let mut settings = study.settings();
                    for (variant, color) in settings.plot_variants[0].iter_mut().zip(colors) {
                        variant.color = color;
                    }
                    study.apply_settings(&[], settings).unwrap();
                    study.update_data(
                        &chart,
                        0,
                        &[StudyData {
                            request: study.data_requests().remove(0),
                            bars: &source,
                            clock: crate::model::BarClock::new(source_interval),
                            instrument: None,
                            timeframe: source_interval,
                            chart: None,
                        }],
                    );
                    let rect =
                        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(440.0, 300.0));
                    let price = PriceMap::new(
                        PriceTransform::default(),
                        PriceRange::new(90.0, 130.0),
                        rect.top(),
                        rect.bottom(),
                        false,
                    );
                    let coords = ChartMapping::new(rect, 20.0, 0, chart.len() - 1, 0.0, price);
                    let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                        crate::chart::indicators::plots::paint_candles(
                            &RenderContext::new(ui.painter(), rect),
                            &study,
                            &price,
                            &coords,
                        );
                    });
                    let painted: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Rect(body) => Some(body.stroke.color),
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    let expected: Vec<_> = (0..source.len())
                        .map(|bar| {
                            if variants {
                                colors[bar % 2]
                            } else {
                                Color32::from_gray((bar + 1) as u8)
                            }
                        })
                        .collect();
                    assert_eq!(
                        painted, expected,
                        "{name}, wait={wait}, variants={variants}"
                    );
                }
            }
            for study in &mut studies {
                study.update_data(&chart, 0, &[]);
            }
            for study in &studies {
                assert!(profiles(study, 0..chart.len()).is_empty(), "{name}");
                assert!(study.profile_levels(0..chart.len()).is_empty(), "{name}");
                assert!(
                    study.markers().is_empty()
                        && study.zones().is_empty()
                        && study.candles().is_empty(),
                    "{name}"
                );
            }
        }
    }

    /// Candles use chart bars sharing their time. Withdrawing the history leaves no stale geometry.
    #[test]
    fn geometry_of_another_history_never_uses_its_source_indices() {
        use super::super::{BollingerBars, HigherIntervalCandles, StudyData};
        use chrono::{TimeDelta, TimeZone, Utc};

        let start = Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap();
        // Hourly closes turning at the listed bars; the rows read the first 20.
        let turns = [
            (0, 100.0),
            (14, 100.0),
            (24, 110.0),
            (34, 100.0),
            (46, 112.0),
            (56, 102.0),
            (70, 116.0),
            (90, 96.0),
            (100, 106.0),
        ];
        let hourly: Vec<_> = (0..=100)
            .map(|i| {
                let pair = turns.windows(2).find(|pair| i <= pair[1].0).unwrap();
                let [(a, low), (b, high)] = [pair[0], pair[1]];
                let close = low + (high - low) * f64::from(i - a) / f64::from(b - a);
                Bar::new(
                    start + TimeDelta::hours(i.into()),
                    close,
                    close + 1.0,
                    close - 1.0,
                    close,
                    1.0,
                )
            })
            .collect();
        let chart = &hourly[10..20];
        let four_hourly: Vec<_> = hourly[..20].iter().step_by(4).cloned().collect();
        let bollinger = (0..10)
            .flat_map(|i| [(i..i + 1, i as i64 + 10), (i..i + 1, i as i64 + 10)])
            .collect::<Vec<_>>();
        let higher = vec![(0..2, 8), (2..6, 12), (6..10, 16)];
        let higher_bollinger = higher.iter().flat_map(|c| [c.clone(), c.clone()]).collect();
        for (inner, interval, history, expected) in [
            (
                Box::new(BollingerBars::new()) as Box<dyn Indicator>,
                None,
                &hourly[..20],
                bollinger,
            ),
            (
                Box::new(HigherIntervalCandles::new(Timeframe::Hour4)),
                None,
                &hourly[..20],
                higher,
            ),
            (
                Box::new(BollingerBars::new()),
                Some(Timeframe::Hour4),
                &four_hourly[..],
                higher_bollinger,
            ),
        ] {
            let mut study = ConfiguredStudy::new(inner);
            study.set_timeframe(Timeframe::Hour1);
            study.update_data(history, 0, &[]);
            let name = study.name().to_owned();
            let (id, value, cleared) = if let Some(interval) = interval {
                (
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(interval)),
                    InputValue::Timeframe(None),
                )
            } else {
                (
                    "calculation_symbol",
                    InputValue::Symbol("X".into()),
                    InputValue::Symbol(String::new()),
                )
            };
            study.set_input(id, value).unwrap();
            let source = StudyData {
                request: study.data_requests().remove(0),
                bars: history,
                clock: crate::model::BarClock::new(interval.unwrap_or(Timeframe::Hour1)),
                instrument: None,
                timeframe: interval.unwrap_or(Timeframe::Hour1),
                chart: None,
            };
            study.update_data(chart, 0, std::slice::from_ref(&source));
            let candles = |study: &ConfiguredStudy| {
                study
                    .candles()
                    .iter()
                    .map(|c| {
                        assert!(
                            c.bars.start < c.bars.end && c.bars.end <= chart.len(),
                            "{name}: {:?} exceeds {} chart bars",
                            c.bars,
                            chart.len()
                        );
                        (c.bars.clone(), (c.bar.time - start).num_hours())
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(candles(&study), expected, "{name}");
            assert_eq!(candles(&study.clone()), expected, "{name} clone");

            study.update_data(chart, 0, &[]);
            assert!(study.candles().is_empty(), "{name}");
            study.update_data(
                chart,
                0,
                &[StudyData {
                    bars: &[],
                    request: source.request.clone(),
                    clock: source.clock.clone(),
                    chart: None,
                    ..source
                }],
            );
            assert!(study.candles().is_empty(), "{name} empty history");
            study.update_data(chart, 0, &[source]);
            assert_eq!(candles(&study), expected, "{name} restored history");

            study.set_input(id, cleared).unwrap();
            study.update_data(history, 0, &[]);
            assert_eq!(
                study.candles().len(),
                study.study.candles().len(),
                "{name} follows chart"
            );
        }
    }

    /// A study calculated on other bars offers no output its aligned values cannot hold.
    #[test]
    fn studies_on_other_bars_offer_alerts_only_on_their_plots() {
        use crate::model::BarClock;
        use crate::studies::StudyData;
        #[derive(Clone, Default)]
        struct Zoned(IndicatorValues);
        impl Indicator for Zoned {
            fn name(&self) -> &str {
                "Zoned"
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.0 = IndicatorValues::missing(data.len());
            }
            fn values(&self) -> &IndicatorValues {
                &self.0
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::WHITE]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn set_visible(&mut self, _: bool) {}
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
            fn alert_outputs(&self) -> Vec<(usize, String)> {
                vec![(0, "Zoned".into()), (1, "Level".into())]
            }
        }
        let start: chrono::DateTime<chrono::Utc> = "2026-02-02T09:00:00Z".parse().unwrap();
        let bars = |count: i32, interval: Timeframe| -> Vec<Bar> {
            (0..count)
                .map(|i| {
                    let time = start + interval.duration() * i;
                    let price = 100.0 + f64::from(i);
                    Bar::new(time, price, price, price, price, 1.0)
                })
                .collect()
        };
        let chart = bars(10, Timeframe::Min1);
        let five = bars(2, Timeframe::Min5);
        let mut study = ConfiguredStudy::new(Box::new(Zoned::default()));
        study.set_timeframe(Timeframe::Min1);
        study.calculate(&chart);
        assert_eq!(
            study.alert_outputs(),
            [(0, "Zoned".into()), (1, "Level".into())]
        );
        study
            .set_input(
                "calculation_timeframe",
                InputValue::Timeframe(Some(Timeframe::Min5)),
            )
            .unwrap();
        let request = study.data_requests().remove(0);
        study.update_data(
            &chart,
            0,
            &[StudyData {
                request,
                bars: &five,
                clock: BarClock::new(Timeframe::Min5),
                instrument: None,
                timeframe: Timeframe::Min5,
                chart: None,
            }],
        );
        assert_eq!(study.alert_outputs(), [(0, "Zoned".into())]);
    }

    /// Guards #3494: a volume profile offers the Interval and Wait for interval close whatever
    /// finer bars it requests, from its Source interval or the host's suggestion, so either order
    /// of Source interval and Interval, by input or by Apply, gives the same study. It reads the
    /// finer bars inside the Interval's bars from the profile's first bar, and keeps the Interval
    /// listed once it is cleared.
    #[test]
    fn profiles_offer_the_interval_in_either_order() {
        use crate::model::{BarClock, ProfileResolution};
        use crate::studies::{
            StudyData, StudyDataRequest, VolProfileAutoAnchored, VolProfileVisible,
        };
        // 48 hourly bars from Monday 2 March 2026 00:00 UTC, two daily bars of volume 99, and one
        // minute bar on each day, of volume 2 and 3.
        let start: chrono::DateTime<chrono::Utc> = "2026-03-02T00:00:00Z".parse().unwrap();
        let hour = |h: i64| start + chrono::Duration::hours(h);
        let flat = |h, volume| Bar::new(hour(h), 100.0, 101.0, 100.0, 101.0, volume);
        let chart: Vec<_> = (0..48).map(|h| flat(h, 1.0)).collect();
        let days = [flat(0, 99.0), flat(24, 99.0)];
        let minutes = [
            Bar::new(hour(10), 100.0, 100.5, 100.0, 100.5, 2.0),
            Bar::new(hour(29), 100.5, 101.0, 100.5, 101.0, 3.0),
        ];
        let bars = |timeframe| StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(timeframe),
            metric: None,
        };
        let suggested = Instrument::new("TEST")
            .with_profile_resolutions([ProfileResolution::new(Timeframe::Min1, None)]);
        let interval = (
            "calculation_timeframe",
            InputValue::Timeframe(Some(Timeframe::Day1)),
        );
        let source = (
            "source_interval",
            InputValue::Timeframe(Some(Timeframe::Min1)),
        );
        let listed = |study: &ConfiguredStudy| -> Vec<_> {
            study
                .inputs()
                .into_iter()
                .filter(|input| {
                    matches!(
                        input.id.as_str(),
                        "calculation_timeframe" | "wait_for_close"
                    )
                })
                .map(|input| (input.id, input.value))
                .collect()
        };
        type Factory = fn() -> Box<dyn Indicator>;
        let studies: [(&str, Factory); 2] = [
            ("Vol Profile VR", || Box::new(VolProfileVisible::new(1))),
            ("Vol Profile Auto Anchored", || {
                Box::new(VolProfileAutoAnchored::new(1))
            }),
        ];
        let (mut observed, mut expected) = (Vec::new(), Vec::new());
        for (name, study) in studies {
            for (order, apply, host, edits) in [
                (
                    "source, then interval",
                    false,
                    false,
                    vec![vec![source.clone()], vec![interval.clone()]],
                ),
                (
                    "interval, then source",
                    false,
                    false,
                    vec![vec![interval.clone()], vec![source.clone()]],
                ),
                (
                    "source, then interval",
                    true,
                    false,
                    vec![vec![source.clone()], vec![interval.clone()]],
                ),
                (
                    "interval, then source",
                    true,
                    false,
                    vec![vec![interval.clone()], vec![source.clone()]],
                ),
                (
                    "one Apply",
                    true,
                    false,
                    vec![vec![source.clone(), interval.clone()]],
                ),
                (
                    "suggested, then interval",
                    false,
                    true,
                    vec![vec![interval.clone()]],
                ),
            ] {
                let case = format!("{name}: {order}, apply_settings {apply}");
                let rows = edits.len();
                let mut study = ConfiguredStudy::new(study());
                study.set_timeframe(Timeframe::Hour1);
                study.set_bar_clock(&BarClock::new(Timeframe::Hour1));
                study.set_instrument(host.then_some(&suggested));
                study.set_visible_range(0..chart.len());
                study.calculate(&chart);
                let results: Vec<_> = edits
                    .into_iter()
                    .map(|edit| {
                        if apply {
                            let inputs: Vec<_> = edit
                                .iter()
                                .map(|(id, value)| (id.to_string(), value.clone()))
                                .collect();
                            let settings = study.settings();
                            study.apply_settings(&inputs, settings)
                        } else {
                            edit.into_iter()
                                .try_for_each(|(id, value)| study.set_input(id, value))
                        }
                        .map_err(|error| error.to_string())
                    })
                    .collect();
                let reply = |request, bars, timeframe| StudyData {
                    request,
                    bars,
                    clock: BarClock::new(timeframe),
                    instrument: host.then_some(&suggested),
                    timeframe,
                    chart: None,
                };
                let replies = [
                    reply(bars(Timeframe::Day1), &days[..], Timeframe::Day1),
                    reply(bars(Timeframe::Min1), &minutes[..], Timeframe::Min1),
                ];
                // A suggestion follows the profile length, known once the daily bars arrived.
                for _ in 0..2 {
                    study.update_data(&chart, 0, &replies);
                }
                let profiles: Vec<_> = study
                    .volume_profiles(0..chart.len())
                    .into_iter()
                    .map(|plot| {
                        let rows = &plot.profile;
                        (plot.bars, rows.low(), rows.high(), rows.volume().to_vec())
                    })
                    .collect();
                let starts = [
                    study.data_start(&bars(Timeframe::Day1)),
                    study.data_start(&bars(Timeframe::Min1)),
                ];
                let on_interval = (listed(&study), study.data_requests(), starts, profiles);
                let cleared = study
                    .set_input("calculation_timeframe", InputValue::Timeframe(None))
                    .map_err(|error| error.to_string());
                study.calculate(&chart);
                observed.push((
                    case.clone(),
                    results,
                    on_interval,
                    cleared,
                    listed(&study),
                    study.data_requests(),
                ));
                expected.push((
                    case,
                    vec![Ok(()); rows],
                    (
                        vec![
                            (interval.0.to_owned(), interval.1.clone()),
                            ("wait_for_close".to_owned(), InputValue::Toggle(true)),
                        ],
                        vec![bars(Timeframe::Day1), bars(Timeframe::Min1)],
                        [None, Some(start)],
                        // Each daily bar holds a minute bar, so the one row sums 2 + 3.
                        vec![(0..48, 100.0, 101.0, vec![5.0])],
                    ),
                    Ok(()),
                    vec![
                        (interval.0.to_owned(), InputValue::Timeframe(None)),
                        ("wait_for_close".to_owned(), InputValue::Toggle(true)),
                    ],
                    vec![bars(Timeframe::Min1)],
                ));
            }
        }
        assert_eq!(observed, expected);
    }

    #[test]
    fn edits_are_atomic_and_interval_visibility_preserves_the_global_toggle() {
        let mut study = ConfiguredStudy::new(Box::new(BollingerBands::new(3, 2.0)));
        let mut settings = study.settings();
        settings.plots[0].visible = false;
        settings.plots[1].width = 3.0;
        settings.precision = Some(4);
        settings.hidden_intervals.push(Timeframe::Min1);
        assert!(
            study
                .apply_settings(
                    &[
                        ("period".into(), InputValue::Length(2)),
                        ("std_dev".into(), InputValue::Number(f64::NAN))
                    ],
                    settings.clone()
                )
                .is_err()
        );
        assert_eq!(study.inputs()[0].value, InputValue::Length(3));
        assert!(study.plot_style(0).visible);
        study
            .apply_settings(&[("period".into(), InputValue::Length(2))], settings)
            .unwrap();
        assert_eq!(study.inputs()[0].value, InputValue::Length(2));
        assert!(!study.plot_style(0).visible);
        assert_eq!(study.line_style(1).width, 3.0);
        assert_eq!(study.precision(), Some(4));
        study.set_timeframe(Timeframe::Min1);
        assert!(!study.is_visible());
        study.set_timeframe(Timeframe::Hour1);
        assert!(study.is_visible());
        study.set_visible(false);
        study.set_timeframe(Timeframe::Day1);
        assert!(!study.is_visible());
        // Settings saved with an interval range show on the intervals they showed.
        let n = |value| std::num::NonZeroU64::new(value).unwrap();
        for (range, interval, visible) in [
            (serde_json::json!({"Only": "Hour1"}), Timeframe::Hour1, true),
            (
                serde_json::json!({"Only": "Hour1"}),
                Timeframe::Custom(n(7200)),
                false,
            ),
            (
                serde_json::json!({"Only": "Hour1"}),
                Timeframe::Min30,
                false,
            ),
            (
                serde_json::json!({"AndAbove": "Min5"}),
                Timeframe::Custom(n(120)),
                false,
            ),
            (
                serde_json::json!({"AndAbove": "Min5"}),
                Timeframe::Custom(n(420)),
                true,
            ),
            (
                serde_json::json!({"AndBelow": {"Ticks": 100}}),
                Timeframe::Ticks(n(50)),
                true,
            ),
            (
                serde_json::json!({"AndBelow": {"Ticks": 100}}),
                Timeframe::Volume(n(50)),
                false,
            ),
        ] {
            let mut saved = serde_json::to_value(study.settings()).unwrap();
            let fields = saved.as_object_mut().unwrap();
            fields.remove("timeframe_visibility");
            fields.insert("interval_range".into(), range);
            study
                .apply_settings(&[], serde_json::from_value(saved).unwrap())
                .unwrap();
            study.set_visible(true);
            study.set_timeframe(interval);
            assert_eq!(study.is_visible(), visible, "{interval}");
            study.set_visible(false);
            assert!(!study.is_visible());
        }
        let saved = serde_json::to_string(&study.settings()).unwrap();
        let restored: StudySettings = serde_json::from_str(&saved).unwrap();
        assert!(!restored.plots[0].visible);
        assert_eq!(restored.hidden_intervals, [Timeframe::Min1]);
        assert_eq!(
            restored.timeframe_visibility,
            TimeframeVisibility {
                show_on_all: false,
                visible_timeframes: Default::default(),
                ranges: [(crate::model::TimeframeUnit::Tick, 1..=100)].into(),
            }
        );
    }

    #[test]
    fn rsi_levels_round_trip_and_older_settings_take_the_declared_levels() {
        use crate::studies::{AdxBreakout, RSI, palette};
        let values = |study: &ConfiguredStudy| -> Vec<f64> {
            study.pane().levels.iter().map(|level| level.0).collect()
        };
        let mut study = ConfiguredStudy::new(Box::new(RSI::default()));
        let declared = study.settings();
        assert_eq!(
            declared
                .levels
                .iter()
                .map(|l| (l.value, l.color, l.pattern, l.visible))
                .collect::<Vec<_>>(),
            [(None, palette::MUTED, LineStyle::Dashed, true); 3]
        );
        assert!(declared.fill_between_levels);
        assert_eq!(
            declared.level_fill_color,
            Color32::from_rgba_unmultiplied(126, 87, 194, 26)
        );
        let before = study.snapshot_settings();
        let mut edited = declared.clone();
        edited.levels[0].value = Some(80.0);
        edited.levels[0].width = 3.0;
        edited.levels[1].visible = false;
        study.apply_settings(&[], edited.clone()).unwrap();
        assert_eq!(values(&study), [80.0, 30.0, 50.0]);
        // Templates, copies and layouts rebuild a fresh study from the saved JSON.
        let saved = serde_json::to_value(&edited).unwrap();
        let mut loaded = ConfiguredStudy::new(Box::new(RSI::default()));
        loaded
            .apply_settings(&[], serde_json::from_value(saved.clone()).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), edited);
        let mut older = saved;
        for level in older["levels"].as_array_mut().unwrap() {
            level.as_object_mut().unwrap().remove("width");
        }
        loaded
            .apply_settings(&[], serde_json::from_value(older.clone()).unwrap())
            .unwrap();
        assert_eq!(loaded.settings().levels[0].width, 1.0);
        for key in ["levels", "fill_between_levels", "level_fill_color"] {
            older.as_object_mut().unwrap().remove(key);
        }
        loaded
            .apply_settings(&[], serde_json::from_value(older).unwrap())
            .unwrap();
        assert_eq!(loaded.settings(), declared);
        study.restore_settings(&before).unwrap();
        assert_eq!(values(&study), [70.0, 30.0, 50.0]);
        let mut invalid = declared.clone();
        invalid.levels[0].value = Some(f64::INFINITY);
        assert!(matches!(
            study.apply_settings(&[], invalid),
            Err(super::super::StudyError::InvalidLevelValue { level: 0, value }) if value == f64::INFINITY
        ));
        assert_eq!(study.settings(), declared);
        for width in [f32::NAN, f32::INFINITY, 0.0, 11.0] {
            let mut invalid = declared.clone();
            invalid.levels[0].width = width;
            assert!(matches!(
                study.apply_settings(&[], invalid),
                Err(super::super::StudyError::InvalidLevelWidth { level: 0, .. })
            ));
            assert_eq!(study.settings(), declared);
        }
        // Adding a middle guide keeps the two saved Stochastic guides and their band.
        for name in ["Stochastic", "StochRSI"] {
            let mut study = ConfiguredStudy::new(
                crate::studies::IndicatorFactory::new()
                    .create(name)
                    .unwrap(),
            );
            let defaults = study.settings();
            assert_eq!(values(&study), [80.0, 20.0, 50.0]);
            assert!(defaults.fill_between_levels);
            assert_eq!(study.level_fill_indices(), Some([0, 1]));
            assert_eq!(
                defaults.level_fill_color,
                palette::LIGHT_BLUE.gamma_multiply_u8(26)
            );
            let mut older = defaults.clone();
            older.levels.truncate(2);
            older.levels[0].value = Some(85.0);
            older.levels[0].width = 4.0;
            older.levels[1].visible = false;
            older.fill_between_levels = false;
            study.apply_settings(&[], older.clone()).unwrap();
            if name == "Stochastic" {
                assert_eq!(&study.settings().levels[..2], older.levels);
                assert!(!study.settings().levels[2].visible);
                assert!(!study.pane().fill_between_levels);
            }
            let mut absent = defaults.clone();
            absent.levels.clear();
            study.apply_settings(&[], absent).unwrap();
            assert_eq!(study.settings(), defaults);
        }
        // Saved before the lower interval named its automatic choice and before the zero level,
        // this template keeps its unset input, every plot style on the chart's interval and draws
        // no zero level.
        let saved = r#"{"name":"Volume Delta","inputs":[["lower_interval",{"Timeframe":null}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolumeDelta.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[8,153,129,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
        let mut older = template.create().unwrap();
        older.set_timeframe(Timeframe::Min5);
        let restored = crate::studies::StudyTemplate::from_study(older.as_ref());
        let mut inputs = template.inputs.clone();
        inputs.push(("custom_interval".to_owned(), InputValue::Toggle(false)));
        assert_eq!(restored.inputs, inputs);
        let mut expected = template.settings.clone();
        expected.levels = vec![LevelStyle {
            value: None,
            color: crate::studies::palette::GRAY,
            pattern: LineStyle::Dashed,
            width: 1.0,
            visible: false,
            price_line: false,
        }];
        assert_eq!(restored.settings, expected);
        assert_eq!(restored.visible, template.visible);
        assert_eq!(restored.scale, template.scale);
        // Profiles saved with finer bars keep their presentation and both saved interval inputs.
        // Saves that omitted those inputs keep their existing chart interval and close-wait defaults.
        for (saved, interval) in [
            (
                r#"{"name":"Vol Profile VR","inputs":[["row_count",{"Length":24}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":"Min1"}],["feed_source",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileVisible.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                None,
            ),
            (
                r#"{"name":"Vol Profile VR","inputs":[["row_count",{"Length":24}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":"Min1"}],["feed_source",{"Toggle":true}],["calculation_timeframe",{"Timeframe":"Day1"}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileVisible.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                Some(Timeframe::Day1),
            ),
            (
                r#"{"name":"Vol Profile Auto Anchored","inputs":[["row_count",{"Length":24}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":"Min1"}],["feed_source",{"Toggle":true}],["anchor",{"Choice":0}],["length",{"Length":100}],["width",{"Number":30.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileAutoAnchored.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                None,
            ),
            (
                r#"{"name":"Vol Profile Auto Anchored","inputs":[["row_count",{"Length":24}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":"Min1"}],["feed_source",{"Toggle":true}],["anchor",{"Choice":0}],["length",{"Length":100}],["width",{"Number":30.0}],["calculation_timeframe",{"Timeframe":"Day1"}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileAutoAnchored.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                Some(Timeframe::Day1),
            ),
        ] {
            let template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
            let mut older = template.create().unwrap();
            older.set_timeframe(Timeframe::Hour1);
            // The developing plots added since are hidden.
            let mut settings = older.settings();
            assert_eq!(settings.plots.len(), 3);
            assert!(settings.plots.iter().all(|plot| !plot.visible));
            settings.plots.clear();
            settings.plot_variants.clear();
            assert_eq!(settings, template.settings);
            assert_eq!(older.is_visible(), template.visible);
            assert_eq!(Some(older.scale()), template.scale);
            let mut expected = template.inputs.clone();
            let controls = expected
                .iter()
                .position(|(id, _)| id == "calculation_timeframe")
                .unwrap_or(expected.len());
            expected.insert(
                controls,
                (
                    "calculation_symbol".to_owned(),
                    InputValue::Symbol(String::new()),
                ),
            );
            for (id, value) in [
                ("calculation_symbol", InputValue::Symbol(String::new())),
                ("calculation_timeframe", InputValue::Timeframe(interval)),
                ("wait_for_close", InputValue::Toggle(true)),
            ] {
                if !expected.iter().any(|(saved, _)| saved == id) {
                    expected.push((id.to_owned(), value.clone()));
                }
                assert_eq!(
                    older
                        .inputs()
                        .iter()
                        .find(|input| input.id == id)
                        .unwrap()
                        .value,
                    value
                );
            }
            let inputs = crate::studies::StudyTemplate::from_study(older.as_ref()).inputs;
            for input in &expected {
                assert!(inputs.contains(input), "{input:?}");
            }
        }
        // A default Ichimoku saved before Symbol existed keeps the chart symbol and its styles.
        let saved = r#"{"name":"Ichimoku","inputs":[["tenkan_period",{"Length":9}],["kijun_period",{"Length":26}],["senkou_b_period",{"Length":52}],["displacement",{"Length":26}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.IchimokuCloud.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[183,28,28,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[165,214,167,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[239,154,154,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[67,160,71,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[0,1,2,3,4],"price_line_plots":[],"plot_variants":[[],[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[7,16,7,26],"inverted_color":[25,7,6,26],"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let template: crate::studies::StudyTemplate = serde_json::from_str(saved).unwrap();
        let older = template.create().unwrap();
        let fresh = ConfiguredStudy::new(Box::new(super::super::IchimokuCloud::default()));
        assert_eq!(older.settings(), template.settings);
        assert!(older.data_requests().is_empty());
        assert_eq!(
            serde_json::to_string(&crate::studies::StudyTemplate::from_study(older.as_ref()))
                .unwrap(),
            serde_json::to_string(&crate::studies::StudyTemplate::from_study(&fresh)).unwrap()
        );
        // A template saved before the size basis existed keeps fill-based filtering and
        // its exact presentation; the missing toggle takes its declared false default.
        let saved = r#"{"name":"Size-Filtered CVD","inputs":[["min",{"Number":100.0}],["max",{"Number":1.7976931348623157e+308}],["anchor",{"Choice":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.OrderFlowStudy.html","plots":[{"color":[38,166,154,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,10,10,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
        let older = template.create().unwrap();
        assert_eq!(older.settings(), template.settings);
        assert_eq!(
            older
                .inputs()
                .iter()
                .find(|input| input.id == "combine_fills")
                .unwrap()
                .value,
            InputValue::Toggle(false)
        );
        let mut expected = template.inputs.clone();
        expected.insert(2, ("combine_fills".to_owned(), InputValue::Toggle(false)));
        // Saved before interval resets: the Continuous choice keeps its index.
        expected.push(("reset_minutes".to_owned(), InputValue::Length(60)));
        assert_eq!(
            crate::studies::StudyTemplate::from_study(older.as_ref()).inputs,
            expected
        );
        // A KST opens with a dashed zero guide. Saved before it had one, it restores with the
        // guide hidden, and a guide shown since keeps its style through a template.
        let declared =
            ConfiguredStudy::new(Box::new(crate::studies::KnowSureThing::new())).settings();
        let zero = LevelStyle {
            value: None,
            color: palette::GRAY,
            pattern: LineStyle::Dashed,
            width: 1.0,
            visible: true,
            price_line: false,
        };
        assert_eq!(declared.levels, [zero]);
        let saved = r#"{"name":"KST","inputs":[["roc_periods [0]",{"Length":10}],["roc_periods [1]",{"Length":15}],["roc_periods [2]",{"Length":20}],["roc_periods [3]",{"Length":30}],["sma_periods [0]",{"Length":10}],["sma_periods [1]",{"Length":10}],["sma_periods [2]",{"Length":10}],["sma_periods [3]",{"Length":15}],["signal_period",{"Length":9}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.KnowSureThing.html","plots":[{"color":[0,150,136,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[244,67,54,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[0,9,9,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let mut older = serde_json::from_str::<crate::studies::StudyTemplate>(saved)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            older.settings().timeframe_visibility,
            TimeframeVisibility::all()
        );
        assert_eq!(
            older.settings().levels,
            [LevelStyle {
                visible: false,
                ..zero
            }]
        );
        assert_eq!(older.pane().levels[0].1, Color32::TRANSPARENT);
        let mut shown = older.settings();
        shown.levels[0].visible = true;
        shown.levels[0].width = 2.0;
        older.apply_settings(&[], shown.clone()).unwrap();
        let again = crate::studies::StudyTemplate::from_study(older.as_ref())
            .create()
            .unwrap();
        assert_eq!(again.settings().levels, shown.levels);
        assert_eq!(again.pane().levels[0].1, palette::GRAY);
        // A new ratio has an equality guide; this template was saved before it had one.
        let ratio =
            ConfiguredStudy::new(Box::new(crate::studies::AdvanceDeclineRatioBars::default()));
        assert_eq!(values(&ratio), [1.0]);
        assert_eq!(ratio.settings().levels, [zero]);
        let saved = r#"{"name":"Advance/Decline Ratio (Bars)","inputs":[["period",{"Length":9}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AdvanceDeclineRatioBars.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
        let older = template.create().unwrap();
        let mut expected = template.settings.clone();
        expected.levels.push(LevelStyle {
            visible: false,
            ..zero
        });
        assert_eq!(older.settings(), expected);
        assert_eq!(older.pane().levels[0].1, Color32::TRANSPARENT);
        // A literal Aroon Oscillator template saved on origin/main keeps its zero guide alone.
        let saved = r#"{"name":"Aroon Oscillator","inputs":[["period",{"Length":14}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AroonOscillator.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
        let older = template.create().unwrap();
        let mut expected = template.settings.clone();
        let hidden = LevelStyle {
            visible: false,
            ..zero
        };
        expected.levels = vec![hidden, expected.levels[0], hidden];
        expected.fills = vec![FillStyle {
            inverted_color: Some(palette::HISTOGRAM_RED.gamma_multiply_u8(26)),
            visible: false,
            ..FillStyle::new(palette::GREEN.gamma_multiply_u8(26))
        }];
        expected.plot_variants = vec![vec![PlotVariant::new("Below zero", palette::HISTOGRAM_RED)]];
        assert_eq!(older.settings(), expected);
        assert_eq!(
            older.pane().levels,
            [
                (90.0, Color32::TRANSPARENT, "Upper level"),
                (0.0, palette::GRAY, "Center"),
                (-90.0, Color32::TRANSPARENT, "Lower level"),
            ]
        );
        // An older MFI retains automatic fitting, including after saving its upgraded guides.
        let saved = r#"{"name":"MFI","inputs":[["period",{"Length":14}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MoneyFlowIndex.html","plots":[{"color":[126,87,194,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[8,5,12,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let mut template = serde_json::from_str::<crate::studies::StudyTemplate>(saved).unwrap();
        for _ in 0..2 {
            let older = template.create().unwrap();
            assert_eq!(older.pane().range, None);
            assert!(older.settings().levels.iter().all(|level| !level.visible));
            assert!(!older.pane().fill_between_levels);
            template = serde_json::from_str(
                &serde_json::to_string(&crate::studies::StudyTemplate::from_study(older.as_ref()))
                    .unwrap(),
            )
            .unwrap();
        }
        let mut adx = ConfiguredStudy::new(Box::new(AdxBreakout::default()));
        let settings = adx.settings();
        adx.apply_settings(&[("threshold".into(), InputValue::Length(25))], settings)
            .unwrap();
        assert_eq!(values(&adx), [25.0]);
    }

    /// The Aroon Oscillator, Bollinger Bands %b, CCI, Money Flow Index, Volume Delta, Cumulative
    /// Volume Delta, PVO, Choppiness Index and Trend Strength Index open with their usual guides
    /// and bands. Older saves keep their guide styles and leave new guides and fills hidden;
    /// a guide shown since keeps its style through a template. Awesome Oscillator opens as
    /// columns and keeps a saved histogram; Balance of Power keeps its saved smoothing. An older
    /// SuperTrend keeps its styles with its new body middle and background hidden.
    #[test]
    fn oscillator_guides_open_declared_and_older_saves_restore_as_drawn() {
        use crate::studies::{IndicatorFactory, PlotType, StudyTemplate, palette};
        type Row = (
            &'static str,
            Vec<(f64, Color32, &'static str, LineStyle)>,
            bool,
            Option<[usize; 2]>,
            Option<(f64, f64)>,
            Color32,
            Vec<(Color32, Option<Color32>)>,
        );
        let factory = IndicatorFactory::new();
        let (gray, faint) = (palette::MUTED, palette::MUTED.gamma_multiply_u8(128));
        let blue = Color32::from_rgba_unmultiplied(41, 98, 255, 26);
        let (dashed, solid) = (LineStyle::Dashed, LineStyle::Solid);
        let rows: [Row; 9] = [
            (
                "Aroon Oscillator",
                vec![
                    (90.0, gray, "Upper level", dashed),
                    (0.0, gray, "Center", dashed),
                    (-90.0, gray, "Lower level", dashed),
                ],
                false,
                None,
                Some((-100.0, 100.0)),
                Color32::TRANSPARENT,
                vec![(
                    palette::GREEN.gamma_multiply_u8(26),
                    Some(palette::HISTOGRAM_RED.gamma_multiply_u8(26)),
                )],
            ),
            (
                "%B",
                vec![
                    (
                        1.0,
                        palette::RED.gamma_multiply_u8(128),
                        "Overbought",
                        dashed,
                    ),
                    (
                        0.0,
                        palette::TEAL.gamma_multiply_u8(128),
                        "Oversold",
                        dashed,
                    ),
                    (
                        0.5,
                        palette::BLUE.gamma_multiply_u8(128),
                        "Middle Band",
                        solid,
                    ),
                ],
                true,
                Some([0, 1]),
                None,
                blue,
                vec![
                    (
                        palette::RED.gamma_multiply_u8(26),
                        Some(Color32::TRANSPARENT),
                    ),
                    (
                        Color32::TRANSPARENT,
                        Some(palette::TEAL.gamma_multiply_u8(26)),
                    ),
                ],
            ),
            (
                "CCI",
                vec![
                    (100.0, gray, "Upper Band", dashed),
                    (-100.0, gray, "Lower Band", dashed),
                    (0.0, faint, "Middle Band", dashed),
                ],
                true,
                Some([0, 1]),
                None,
                Color32::from_rgba_unmultiplied(33, 150, 243, 26),
                vec![],
            ),
            (
                "MFI",
                vec![
                    (80.0, gray, "Overbought", dashed),
                    (20.0, gray, "Oversold", dashed),
                    (50.0, faint, "Middle Band", dashed),
                ],
                true,
                Some([0, 1]),
                Some((0.0, 100.0)),
                Color32::from_rgba_unmultiplied(126, 87, 194, 26),
                vec![],
            ),
            (
                "Volume Delta",
                vec![(0.0, palette::GRAY, "Zero", dashed)],
                false,
                None,
                None,
                Color32::TRANSPARENT,
                vec![],
            ),
            (
                "Cumulative Volume Delta",
                vec![(0.0, palette::GRAY, "Zero", dashed)],
                false,
                None,
                None,
                Color32::TRANSPARENT,
                vec![],
            ),
            (
                "PVO",
                vec![(0.0, faint, "Zero Line", dashed)],
                false,
                None,
                None,
                Color32::TRANSPARENT,
                vec![],
            ),
            (
                "CHOP",
                vec![
                    (61.8, gray, "Upper Band", dashed),
                    (38.2, gray, "Lower Band", dashed),
                    (50.0, faint, "Middle Band", solid),
                ],
                true,
                Some([0, 1]),
                None,
                Color32::from_rgba_unmultiplied(33, 150, 243, 26),
                vec![],
            ),
            (
                "Trend Strength Index",
                vec![
                    (0.0, palette::GRAY.gamma_multiply_u8(128), "Zero", dashed),
                    (1.0, palette::UP, "Bullish Band", dashed),
                    (-1.0, palette::DOWN, "Bearish Band", dashed),
                ],
                false,
                None,
                Some((-1.1, 1.1)),
                Color32::TRANSPARENT,
                vec![],
            ),
        ];
        let hidden = |level: LevelStyle| LevelStyle {
            visible: false,
            ..level
        };
        for (name, guides, fill, indices, range, band, fills) in rows {
            let mut study = ConfiguredStudy::new(factory.create(name).unwrap());
            let pane = study.pane();
            let drawn: Vec<_> = pane
                .levels
                .iter()
                .enumerate()
                .map(|(i, level)| {
                    (
                        level.0,
                        level.1,
                        level.2,
                        study.settings().levels[i].pattern,
                    )
                })
                .collect();
            assert_eq!(drawn, guides, "{name}");
            assert_eq!(pane.range, range, "{name}");
            let defaults = study.settings();
            assert!(
                defaults
                    .levels
                    .iter()
                    .all(|level| level.value.is_none() && level.visible && level.width == 1.0),
                "{name}"
            );
            assert_eq!(defaults.fill_between_levels, fill, "{name}");
            assert_eq!(study.level_fill_indices(), indices, "{name}");
            if fill {
                assert_eq!(defaults.level_fill_color, band, "{name}");
            }
            assert_eq!(
                study
                    .fills()
                    .iter()
                    .map(|fill| (fill.color, fill.inverted_color))
                    .collect::<Vec<_>>(),
                fills,
                "{name}"
            );
            if !fills.is_empty() {
                let mut saved = StudyTemplate::from_study(&study);
                saved.settings.fills.clear();
                let older = saved.create().unwrap();
                assert!(older.fills().is_empty(), "{name}");
                assert_eq!(
                    older
                        .settings()
                        .fills
                        .iter()
                        .map(|fill| (fill.color, fill.inverted_color, fill.visible))
                        .collect::<Vec<_>>(),
                    fills
                        .iter()
                        .map(|&(color, inverted)| (color, inverted, false))
                        .collect::<Vec<_>>(),
                    "{name}"
                );
            }
            // The Aroon Oscillator and Trend Strength Index drew their zero guide before, the
            // Choppiness Index its outer bands; the others drew none.
            let declared = defaults.levels.clone();
            let older: Vec<(Vec<LevelStyle>, Vec<LevelStyle>)> = if name == "Trend Strength Index" {
                let moved = LevelStyle {
                    value: Some(0.25),
                    width: 2.0,
                    ..declared[0]
                };
                vec![
                    (
                        vec![moved],
                        vec![moved, hidden(declared[1]), hidden(declared[2])],
                    ),
                    (
                        Vec::new(),
                        vec![declared[0], hidden(declared[1]), hidden(declared[2])],
                    ),
                ]
            } else if name == "Aroon Oscillator" {
                let moved = LevelStyle {
                    value: Some(5.0),
                    width: 2.0,
                    ..declared[1]
                };
                vec![
                    (
                        vec![moved],
                        vec![hidden(declared[0]), moved, hidden(declared[2])],
                    ),
                    (
                        Vec::new(),
                        vec![hidden(declared[0]), declared[1], hidden(declared[2])],
                    ),
                ]
            } else if name == "MFI" {
                vec![
                    (
                        declared[..2].to_vec(),
                        vec![declared[0], declared[1], hidden(declared[2])],
                    ),
                    (Vec::new(), declared.iter().copied().map(hidden).collect()),
                ]
            } else if name == "CHOP" {
                vec![(
                    declared[..2].to_vec(),
                    vec![declared[0], declared[1], hidden(declared[2])],
                )]
            } else {
                vec![(Vec::new(), declared.iter().copied().map(hidden).collect())]
            };
            for (saved, restored) in older {
                let mut settings = defaults.clone();
                let empty = saved.is_empty();
                settings.levels = saved;
                settings.fill_between_levels = false;
                study.apply_settings(&[], settings).unwrap();
                assert_eq!(study.settings().levels, restored, "{name}");
                assert!(!study.pane().fill_between_levels, "{name}");
                assert_eq!(
                    study.pane().range,
                    match name {
                        "MFI" if empty => None,
                        // Older saves keep the range they drew with while the band guides stay hidden.
                        "Trend Strength Index" => Some((-1.0, 1.0)),
                        _ => range,
                    },
                    "{name}"
                );
                let shown: Vec<_> = study.pane().levels.iter().map(|level| level.1).collect();
                let expected: Vec<_> = restored
                    .iter()
                    .zip(&guides)
                    .map(|(level, &(_, color, _, _))| {
                        if level.visible {
                            color
                        } else {
                            Color32::TRANSPARENT
                        }
                    })
                    .collect();
                assert_eq!(shown, expected, "{name}");
            }
            // A guide shown since keeps its style, and the restore does not run again.
            let mut edited = study.settings();
            edited.levels[0].visible = true;
            edited.levels[0].width = 3.0;
            study.apply_settings(&[], edited.clone()).unwrap();
            let again = StudyTemplate::from_study(&study).create().unwrap();
            assert_eq!(again.settings().levels, edited.levels, "{name}");
            assert_eq!(again.pane().range, study.pane().range, "{name}");
        }
        // Literal baseline saves keep their styles, with newly declared guides and fills hidden.
        for text in [
            // SuperTrend saved on origin/main 9544f5b34 before its background and body middle.
            r#"{"name":"SuperTrend","inputs":[["atr_period",{"Length":10}],["multiplier",{"Number":3.0}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.SuperTrend.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"MFI","inputs":[["period",{"Length":14}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MoneyFlowIndex.html","plots":[{"color":[126,87,194,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":true,"level_fill_color":[13,9,20,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"Aroon Oscillator","inputs":[["period",{"Length":14}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AroonOscillator.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"AO","inputs":[["fast_period",{"Length":5}],["slow_period",{"Length":34}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AwesomeOscillator.html","plots":[{"color":[0,150,136,255],"width":1.0,"pattern":"Solid","plot_type":"Histogram","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[0,9,9,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[{"name":"Decreasing bars","color":[244,67,54,255],"width":1.0}]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"%B","inputs":[["period",{"Length":20}],["multiplier",{"Number":2.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.PercentB.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[60,62,67,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":true,"level_fill_color":[4,10,26,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"CCI","inputs":[["period",{"Length":20}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CCI.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[60,62,67,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":true,"level_fill_color":[4,10,26,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        ] {
            let saved: StudyTemplate = serde_json::from_str(text).unwrap();
            let older = saved.create().unwrap();
            let mut expected = saved.settings.clone();
            match saved.name.as_str() {
                "MFI" => expected.levels.push(LevelStyle {
                    color: faint,
                    visible: false,
                    ..expected.levels[0]
                }),
                "Aroon Oscillator" | "%B" | "SuperTrend" => {
                    expected.fills = factory
                        .create(&saved.name)
                        .unwrap()
                        .fills()
                        .iter()
                        .map(|fill| FillStyle {
                            inverted_color: fill.inverted_color,
                            visible: false,
                            ..FillStyle::new(fill.color)
                        })
                        .collect();
                    if saved.name == "SuperTrend" {
                        expected
                            .plots
                            .push(crate::studies::SuperTrend::default().plot_style(2));
                        expected.plot_variants.push(Vec::new());
                    }
                    if saved.name == "Aroon Oscillator" {
                        expected.plot_variants =
                            vec![vec![PlotVariant::new("Below zero", palette::HISTOGRAM_RED)]];
                    }
                }
                _ => {}
            }
            assert_eq!(older.line_count(), expected.plots.len(), "{}", saved.name);
            assert_eq!(older.settings(), expected, "{}", saved.name);
            for (id, value) in &saved.inputs {
                assert_eq!(
                    older
                        .inputs()
                        .iter()
                        .find(|input| input.id == *id)
                        .map(|input| &input.value),
                    Some(value),
                    "{} {id}",
                    saved.name
                );
            }
            let again = StudyTemplate::from_study(older.as_ref()).create().unwrap();
            assert_eq!(again.line_names(), older.line_names(), "{}", saved.name);
            assert_eq!(again.settings(), expected, "{}", saved.name);
        }
        // Saved as a histogram, an Awesome Oscillator stays a histogram; saved smoothed over 14 bars,
        // a Balance of Power keeps its smoothing.
        let mut ao = ConfiguredStudy::new(factory.create("AO").unwrap());
        assert_eq!(ao.plot_style(0).plot_type, PlotType::Columns);
        let mut histogram = ao.settings();
        histogram.plots[0].plot_type = PlotType::Histogram;
        ao.apply_settings(&[], histogram).unwrap();
        let again = StudyTemplate::from_study(&ao).create().unwrap();
        assert_eq!(again.plot_style(0).plot_type, PlotType::Histogram);
        let mut bop = ConfiguredStudy::new(factory.create("BoP").unwrap());
        let settings = bop.settings();
        bop.apply_settings(
            &[("smooth_period".into(), InputValue::Length(14))],
            settings,
        )
        .unwrap();
        let again = StudyTemplate::from_study(&bop).create().unwrap();
        assert_eq!(again.line_names(), ["BoP(14)"]);
    }

    /// A palette set on a chart study recolours its band, as it recolours its plots.
    #[test]
    fn set_colors_recolours_the_band() {
        let band = Color32::from_rgba_unmultiplied(1, 2, 3, 40);
        let palette = vec![Color32::RED, Color32::GREEN, Color32::BLUE, band];
        let mut own = BollingerBands::default();
        own.set_colors(palette.clone());
        // Bollinger Bands fill the band in the fourth palette colour.
        assert_eq!(own.fills()[0].color, band);
        let mut study = ConfiguredStudy::new(Box::new(BollingerBands::default()));
        study.set_colors(palette);
        assert_eq!(study.plot_style(0).color, Color32::RED);
        assert_eq!(study.fills()[0].color, band);

        // A hidden band stays hidden through a palette, and a template keeps its colour.
        let mut settings = study.settings();
        settings.fills[0].visible = false;
        study.apply_settings(&[], settings).unwrap();
        study.set_colors(vec![Color32::RED, Color32::GREEN, Color32::BLUE, band]);
        assert!(study.fills().is_empty());
        let restored = crate::studies::StudyTemplate::from_study(&study)
            .create()
            .unwrap();
        assert_eq!(restored.settings().fills[0].color, band);
        assert!(restored.fills().is_empty());

        // Restoring a snapshot through the trait reapplies the palette after the saved styles,
        // which keep a band colour of their own.
        let own = Color32::from_rgba_unmultiplied(9, 8, 7, 60);
        let mut settings = study.settings();
        settings.fills[0] = FillStyle::new(own);
        study.apply_settings(&[], settings).unwrap();
        study.snapshot_settings().restore(&mut study).unwrap();
        assert_eq!(study.fills()[0].color, own);
    }

    /// `StudySettings::default()`, where an `Indicator::settings` override starts, holds what
    /// settings saved with only the original fields restore, so it shows the status line values
    /// and the pane tint.
    #[test]
    fn default_settings_match_settings_saved_without_the_newer_fields() {
        let saved: StudySettings =
            serde_json::from_str(r#"{"plots":[],"precision":null,"hidden_intervals":[]}"#).unwrap();
        assert_eq!(StudySettings::default(), saved);
    }
}
