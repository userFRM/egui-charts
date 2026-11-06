//! Typed alerts, as a host or a server stores and evaluates them.

use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

use super::{Bar, Timeframe, TimezoneId};

/// The host's identifier of an alert.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId(
    /// The host's own identifier, unique among its alerts.
    pub String,
);

/// An alert as the host stores and evaluates it. The library creates and edits alerts through
/// [`Alerts::save`](crate::services::Alerts::save) and shows the ones the host pushes. It evaluates
/// them only through the host's service; `AlertEngine` (feature `alert-engine`) is one.
///
/// It is serialisable so that a server can store and evaluate the same type. Fields added later
/// will carry `#[serde(default)]`, so alerts stored by an earlier version keep loading. A
/// condition, frequency or status added later does not load in an earlier version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Alert {
    /// `None` for an alert the user is creating; the host assigns the id.
    #[serde(default)]
    pub id: Option<AlertId>,
    /// Creation time assigned by the host. `None` means an older record has no known time.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// Most recent firing time, including firings while the application was closed.
    /// Hosts retain it on every trigger, even for alerts that remain active.
    #[serde(default)]
    pub last_triggered_at: Option<DateTime<Utc>>,
    /// Draw the threshold across the pane in addition to its marker. Enabled by default.
    #[serde(default = "alert_display_enabled")]
    pub extend_line: bool,
    /// Blink the marker for a minute after a firing. Enabled by default; independent of the line
    /// extension.
    #[serde(default = "alert_display_enabled")]
    pub blink_on_trigger: bool,
    /// The symbol whose bars supply the sources, unless `watchlist` names the subject.
    pub symbol: String,
    /// User-facing name, independent of the notification message. The alert dialog takes at most
    /// [`Alert::MAX_NAME_CHARS`] characters.
    #[serde(default)]
    pub name: String,
    /// Host watchlist identifier. When present, evaluate independently on each member symbol.
    #[serde(default)]
    pub watchlist: Option<String>,
    /// Calculation interval of the first condition. `None` until selected;
    /// `AlertEngine` selects it from primary-series input. An explicit
    /// additional interval does not select the first condition's interval.
    #[serde(default)]
    pub timeframe: Option<Timeframe>,
    /// Independent creation inputs, retained through edits, copies and storage. Legacy alerts
    /// without these inputs cannot recover a study from a chart-local identifier.
    #[serde(default)]
    pub inputs: Option<Box<AlertInputs>>,
    /// Value tested by the first condition.
    #[serde(default)]
    pub source: AlertSource,
    /// Additional conditions, all required at the same event time as the first condition.
    /// An alert accepts at most five conditions including the first. Independent intervals need
    /// time-based bars; conditions on one execution-based interval remain supported.
    #[serde(default)]
    pub additional_conditions: Vec<AlertRule>,
    /// Optional interval overrides in `additional_conditions` order. Missing entries and `None`
    /// follow `timeframe`. Keep entries aligned when removing or reordering additional rules.
    #[serde(default)]
    pub additional_timeframes: Vec<Option<Timeframe>>,
    /// When the alert fires.
    pub condition: AlertCondition,
    /// Whether the alert fires once or on every occurrence.
    #[serde(default)]
    pub frequency: AlertFrequency,
    /// The alert starts monitoring at this time; `None` monitors from the moment it is saved.
    /// Before it, evaluation ignores the alert, so its first evaluation after the start compares
    /// with the value one bar earlier, as for a new alert. Earlier than `expires` when both are set.
    #[serde(default)]
    pub starts: Option<DateTime<Utc>>,
    /// The alert expires at this time; `None` never expires.
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
    /// When the alert may trigger; `None` allows any time. Outside the schedule, evaluation keeps
    /// observing values for crossings without firing or consuming an occurrence.
    #[serde(default)]
    pub schedule: Option<Box<NotificationSchedule>>,
    /// Where a firing is delivered.
    #[serde(default)]
    pub delivery: AlertDelivery,
    /// Text the host shows or sends when the alert fires. The alert dialog takes at most
    /// [`Alert::MAX_MESSAGE_CHARS`] characters. [`Alert::expand_message`] fills in the placeholders
    /// `{{symbol}}`, `{{close}}` and so on when it fires.
    #[serde(default)]
    pub message: String,
    /// Where the alert stands, as the host reports it.
    #[serde(default)]
    pub status: AlertStatus,
}

fn alert_display_enabled() -> bool {
    true
}

/// Where a firing is delivered. The alerts panel honours `popup` and `sound`; the host sends the
/// others itself, reading them from the saved alert. The trigger log records every firing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct AlertDelivery {
    /// Show a popup. Enabled by default.
    pub popup: bool,
    /// Play the sound chosen in the alert settings, if they turn sound on. Enabled by default.
    pub sound: bool,
    /// Ask the host to send an email.
    pub email: bool,
    /// Ask the host to send a push notification to the user's mobile app.
    pub push: bool,
    /// Ask the host to post to [`webhook_url`](Self::webhook_url).
    pub webhook: bool,
    /// Where the host posts, as an `http` or `https` URL.
    pub webhook_url: String,
}

impl Default for AlertDelivery {
    fn default() -> Self {
        Self {
            popup: true,
            sound: true,
            email: false,
            push: false,
            webhook: false,
            webhook_url: String::new(),
        }
    }
}

impl AlertDelivery {
    /// Whether an enabled webhook has an `http` or `https` URL.
    pub fn is_valid(&self) -> bool {
        let url = self.webhook_url.trim();
        !self.webhook
            || url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .is_some_and(|rest| !rest.is_empty())
    }
}

/// The source view and study definitions captured when an alert is created. The alert's
/// symbol and condition intervals select their bars; auxiliary requests follow these saved studies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertInputs {
    /// Calendar zone; `None` follows the system's local calendar.
    pub timezone: Option<TimezoneId>,
    /// Exchange calendar of the creation symbol. Other symbols resolve their own calendars.
    pub calendar: Option<AlertCalendar>,
    /// Source choices shown when editing, independent of the currently displayed chart.
    pub sources: Vec<(AlertSource, String)>,
    /// Include extended trading sessions.
    pub extended_hours: bool,
    /// Host currency or unit conversion.
    pub conversion: Option<String>,
    /// Initial main-series history size when no start time was available.
    pub count: usize,
    /// Oldest main bar used at creation. Later evaluations retain this seed.
    pub start: Option<DateTime<Utc>>,
    /// Creation window for studies whose calculation depends on visible bars, inclusive.
    pub visible: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Drawing levels interpolate logarithmically on a logarithmic source scale.
    pub logarithmic: bool,
    /// Independent calculation definitions, including study dependencies.
    pub studies: Vec<AlertStudy>,
}

impl Default for AlertInputs {
    fn default() -> Self {
        Self {
            timezone: Some(TimezoneId::utc()),
            calendar: None,
            sources: Vec::new(),
            extended_hours: false,
            conversion: None,
            count: 1000,
            start: None,
            visible: None,
            logarithmic: false,
            studies: Vec::new(),
        }
    }
}

impl AlertInputs {
    /// Offer newly available study sources during an explicit edit. Existing identifiers keep
    /// their original definitions, including when another chart reuses an identifier.
    pub fn add_sources_from(&mut self, other: &Self) {
        for study in &other.studies {
            if !self.studies.iter().any(|saved| saved.id == study.id) {
                self.studies.push(study.clone());
            }
        }
        for (source, name) in &other.sources {
            if !self.sources.iter().any(|(saved, _)| saved == source) {
                self.sources.push((source.clone(), name.clone()));
            }
        }
    }
}

/// The creation symbol's exchange calendar, including an explicitly continuous session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertCalendar {
    /// Symbol whose calendar was captured.
    pub symbol: String,
    /// Exchange zone, when supplied by the host.
    pub timezone: Option<TimezoneId>,
    /// Trading windows; `None` selects a continuous session.
    pub session: Option<super::Session>,
}

/// Serializable calculation inputs of a study, without computed values or shared stream state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertStudy {
    /// Identifier used by the alert sources and by dependent studies.
    pub id: usize,
    /// Name resolved by the study factory.
    pub name: String,
    /// Typed parameters, including auxiliary symbols and intervals.
    pub inputs: Vec<(String, super::InputValue)>,
}

impl Alert {
    /// An active alert on `symbol` with no id, no expiry and no message. Frequency is `Once`
    /// for bar conditions and `EveryTime` for strategy fills.
    pub fn new(symbol: impl Into<String>, condition: AlertCondition) -> Self {
        Self {
            id: None,
            created_at: None,
            last_triggered_at: None,
            extend_line: true,
            blink_on_trigger: true,
            symbol: symbol.into(),
            name: String::new(),
            watchlist: None,
            timeframe: None,
            inputs: None,
            source: AlertSource::Price,
            additional_conditions: Vec::new(),
            additional_timeframes: Vec::new(),
            frequency: if matches!(condition, AlertCondition::StrategyFills(_)) {
                AlertFrequency::EveryTime
            } else {
                AlertFrequency::default()
            },
            condition,
            starts: None,
            expires: None,
            schedule: None,
            delivery: AlertDelivery::default(),
            message: String::new(),
            status: AlertStatus::default(),
        }
    }
}

impl Alert {
    #[cfg(any(feature = "ui", feature = "alert-engine"))]
    pub(crate) fn same_monitoring(&self, other: &Self) -> bool {
        let mut copy = self.clone();
        copy.created_at = other.created_at;
        copy.last_triggered_at = other.last_triggered_at;
        copy.extend_line = other.extend_line;
        copy.blink_on_trigger = other.blink_on_trigger;
        copy.delivery.clone_from(&other.delivery);
        copy == *other
    }

    /// Distinct calculation intervals, shortest nominal duration first, preserving condition
    /// order for ties. An unset first interval uses one minute
    /// until the engine fixes it to its first input. Additional unset intervals follow it.
    pub fn timeframes(&self) -> Vec<Timeframe> {
        let mut intervals = Vec::new();
        for (_, _, interval) in self.rules() {
            if !intervals.contains(&interval) {
                intervals.push(interval);
            }
        }
        intervals.sort_by_key(Timeframe::duration_ms);
        intervals
    }

    pub(crate) fn rules(&self) -> impl Iterator<Item = (&AlertSource, &AlertCondition, Timeframe)> {
        let first = self.timeframe.unwrap_or_default();
        std::iter::once((&self.source, &self.condition, first)).chain(
            self.additional_conditions
                .iter()
                .enumerate()
                .map(move |(index, rule)| {
                    let interval = self
                        .additional_timeframes
                        .get(index)
                        .copied()
                        .flatten()
                        .unwrap_or(first);
                    (&rule.source, &rule.condition, interval)
                }),
        )
    }

    /// Every input tested by the alert, including comparison outputs and additional rules.
    pub fn sources(&self) -> impl Iterator<Item = &AlertSource> {
        std::iter::once((&self.source, &self.condition))
            .chain(
                self.additional_conditions
                    .iter()
                    .map(|rule| (&rule.source, &rule.condition)),
            )
            .flat_map(|(source, condition)| {
                std::iter::once(source).chain(condition.partners().into_iter().flatten())
            })
    }

    /// The most characters, counted as Unicode scalar values, an edited name may have.
    pub const MAX_NAME_CHARS: usize = 300;
    /// The most characters, counted as Unicode scalar values, an edited message may have.
    pub const MAX_MESSAGE_CHARS: usize = 4000;

    /// The message with its placeholders filled in from the trigger: `{{symbol}}` and
    /// `{{timeframe}}` (also `{{interval}}`) as given, `{{exchange}}` the `exchange` the feed
    /// reports for the symbol ([`Instrument::exchange`](super::Instrument::exchange)), or without
    /// one the text before the first `:` of the symbol, and `{{ticker}}` the text after that `:`
    /// (the whole symbol without one),
    /// `{{time}}` the open time of `bar` and `{{trigger_time}}` the instant `trigger` the alert
    /// fired, both in RFC 3339, `{{open}}`, `{{high}}`, `{{low}}`, `{{close}}` and `{{volume}}` of
    /// `bar`, `{{value}}` the value of the alert's source, `{{output_N}}` the value of output `N`
    /// of the study the source names (empty if the source is not a study), `{{plot_ID_N}}` the
    /// value of output `N` of the study `ID` whatever the source is, and `{{level}}` the label
    /// that [`AlertInputs::sources`] gives the drawing level the first condition tests: the source
    /// if it is a drawing, otherwise the drawing it compares with or those bounding its channel,
    /// joined by `, `, and empty if it tests no drawing. `value` gives the value of a source at
    /// the bar. A value that is missing or not finite becomes `NaN`, a number is written in the
    /// shortest form that reads back the same, and a placeholder the alert does not know stays as
    /// written.
    ///
    /// ```
    /// use egui_charts::model::{Alert, AlertCondition, Bar, Timeframe};
    ///
    /// let mut alert = Alert::new("NYSE:IBM", AlertCondition::Above(100.0));
    /// alert.message = "{{ticker}} on {{exchange}} closed at {{close}} on {{interval}}".into();
    /// let time = chrono::DateTime::from_timestamp(0, 0).unwrap();
    /// let bar = Bar::new(time, 99.0, 102.0, 98.0, 101.5, 1000.0);
    /// assert_eq!(
    ///     alert.expand_message("NYSE:IBM", None, Timeframe::Min5, &bar, time, |_| None),
    ///     "IBM on NYSE closed at 101.5 on 5min"
    /// );
    /// ```
    pub fn expand_message(
        &self,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        bar: &Bar,
        trigger: DateTime<Utc>,
        value: impl Fn(&AlertSource) -> Option<f64>,
    ) -> String {
        let number = |number: Option<f64>| {
            number
                .filter(|number| number.is_finite())
                .map_or_else(|| "NaN".to_owned(), |number| number.to_string())
        };
        let (prefix, ticker) = symbol.split_once(':').unwrap_or(("", symbol));
        let exchange = exchange.filter(|name| !name.is_empty()).unwrap_or(prefix);
        let study = |id: &str, output| {
            number(value(&AlertSource::Study {
                id: id.to_owned(),
                output,
            }))
        };
        let field = |name: &str| -> Option<String> {
            Some(match name {
                "symbol" => symbol.to_owned(),
                "exchange" => exchange.to_owned(),
                "ticker" => ticker.to_owned(),
                "timeframe" | "interval" => timeframe.to_string(),
                "time" => bar.time.to_rfc3339(),
                "trigger_time" => trigger.to_rfc3339(),
                "open" => number(Some(bar.open)),
                "high" => number(Some(bar.high)),
                "low" => number(Some(bar.low)),
                "close" => number(Some(bar.close)),
                "volume" => number(Some(bar.volume)),
                "value" => number(value(&self.source)),
                "level" => {
                    let tested = if matches!(self.source, AlertSource::Drawing(_)) {
                        [Some(&self.source), None]
                    } else {
                        self.condition.partners()
                    };
                    let saved = self.inputs.as_ref().map_or(&[][..], |i| &i.sources);
                    let labels: Vec<_> = tested
                        .into_iter()
                        .flatten()
                        .filter(|source| matches!(source, AlertSource::Drawing(_)))
                        .filter_map(|source| saved.iter().find(|(s, _)| s == source))
                        .map(|(_, label)| label.as_str())
                        .collect();
                    labels.join(", ")
                }
                _ => {
                    if let Some((id, output)) = plot_placeholder(name) {
                        return Some(study(id, output));
                    }
                    let output = name.strip_prefix("output_")?.parse().ok()?;
                    match &self.source {
                        AlertSource::Study { id, .. } => study(id, output),
                        _ => String::new(),
                    }
                }
            })
        };
        fill_placeholders(&self.message, field)
    }

    /// The study identifiers that `{{plot_ID_N}}` placeholders of the message name.
    #[cfg(all(feature = "alert-engine", feature = "studies"))]
    pub(crate) fn plot_studies(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        fill_placeholders(&self.message, |name| {
            ids.extend(plot_placeholder(name).and_then(|(id, _)| id.parse::<usize>().ok()));
            None
        });
        ids
    }
}

/// The study identifier and output number of the placeholder `plot_ID_N`.
fn plot_placeholder(name: &str) -> Option<(&str, usize)> {
    let (id, output) = name.strip_prefix("plot_")?.rsplit_once('_')?;
    Some((id, output.parse().ok()?))
}

/// `message` with each `{{name}}` replaced by what `field` gives for it, or left as written if
/// `field` gives nothing.
fn fill_placeholders(message: &str, mut field: impl FnMut(&str) -> Option<String>) -> String {
    let mut text = String::new();
    let mut rest = message;
    while let Some(start) = rest.find("{{")
        && let Some(length) = rest[start + 2..].find("}}")
    {
        let end = start + 2 + length;
        text.push_str(&rest[..start]);
        match field(&rest[start + 2..end]) {
            Some(filled) => text.push_str(&filled),
            None => text.push_str(&rest[start..end + 2]),
        }
        rest = &rest[end + 2..];
    }
    text.push_str(rest);
    text
}

/// When an alert fires, judged on its source for each subject symbol.
/// Each variant states its rule so that a host and a server evaluate it alike. Levels are finite
/// and in the symbol's price units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertCondition {
    /// Each fill of the host's uniquely named strategy run. Feed `Backtest::update` results to
    /// `AlertEngine::strategy_fills` with the same run id. Uses the price source, no additional
    /// rules, and frequency `EveryTime` (the constructor default) or `Once`. No bars evaluate it.
    StrategyFills(String),
    /// The previous value was strictly below or above the level and the current value reaches
    /// or passes it, in either direction.
    Crossing(f64),
    /// The previous price was below the level and the current price is at or above it.
    CrossingUp(f64),
    /// The previous price was above the level and the current price is at or below it.
    CrossingDown(f64),
    /// The price is above the level.
    Above(f64),
    /// The price is below the level.
    Below(f64),
    /// Previous value was outside the inclusive channel, current value is inside it.
    EnteringChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// Previous value was inside the inclusive channel, current value is outside it.
    ExitingChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// The current value is inside the inclusive channel, `lower <= value <= upper`, whatever the
    /// previous value was.
    InsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// The current value is outside the inclusive channel, below `lower` or above `upper`,
    /// whatever the previous value was.
    OutsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// Signed change from `bars` bars ago reaches `value`: at least for positive, at most for negative.
    MovingBy {
        /// Signed, nonzero change in source units.
        value: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Signed percentage change, 100 * (current - previous) / abs(previous), over `bars` bars.
    /// A zero previous value cannot satisfy this condition.
    MovingByPercent {
        /// Signed, nonzero percentage, with 1 meaning 1%.
        percent: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Compare against another output on the same bar, using its previous value for crossings.
    Compare {
        /// The second output on the same symbol and bar.
        other: AlertSource,
        /// The required relationship between the outputs.
        comparison: AlertComparison,
    },
    /// Test against two boundaries resolved on each evaluated bar. The boundaries may change
    /// order; their minimum and maximum define the inclusive channel. Missing, non-finite or
    /// coincident boundaries cannot satisfy the condition. On a new bar, transitions resolve the
    /// source and both boundaries at the preceding bar again. Within a bar they compare with the
    /// previous evaluation, just as a crossing does.
    Channel {
        /// Stable sources of the two boundary lines, retained through drawing edits and storage.
        boundaries: Box<[AlertSource; 2]>,
        /// State or transition to test.
        comparison: AlertChannelComparison,
    },
    /// Current volume exceeds `multiplier` times the mean of the preceding `bars` completed volumes.
    VolumeAboveAverage {
        /// Positive, finite multiple of the preceding mean volume.
        multiplier: f64,
        /// Number of preceding bars in the mean, at least one.
        bars: u32,
    },
}

/// A host-resolved value on a bar. Identifiers remain meaningful to the host across sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSource {
    /// The symbol's closing or latest price.
    #[default]
    Price,
    /// A study instance and its zero-based output index.
    Study {
        /// Host's stable identifier for the study instance.
        id: String,
        /// Zero-based output line within the study.
        output: usize,
    },
    /// A drawing's price at the bar's time, resolved by the host.
    Drawing(String),
    /// The bar's volume.
    Volume,
}

/// Comparison of two outputs. Crossings require a strict previous side and include equality now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertComparison {
    /// Cross in either direction.
    Crossing,
    /// Cross from below.
    CrossingUp,
    /// Cross from above.
    CrossingDown,
    /// Strictly greater.
    Above,
    /// Strictly less.
    Below,
}

/// Relationship of a value to two inclusive, independently resolved boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertChannelComparison {
    /// Previously outside, currently inside.
    Entering,
    /// Previously inside, currently outside.
    Exiting,
    /// Currently inside, regardless of the previous value.
    Inside,
    /// Currently outside, regardless of the previous value.
    Outside,
}

impl std::fmt::Display for AlertChannelComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Entering => "Entering channel",
            Self::Exiting => "Exiting channel",
            Self::Inside => "Inside channel",
            Self::Outside => "Outside channel",
        })
    }
}

/// One additional condition of an alert; all rules must hold at the same event time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    /// The value to test.
    pub source: AlertSource,
    /// The required relationship.
    pub condition: AlertCondition,
}

impl AlertSource {
    fn is_valid(&self) -> bool {
        match self {
            Self::Study { id, .. } | Self::Drawing(id) => !id.trim().is_empty(),
            _ => true,
        }
    }
}

impl AlertCondition {
    /// Other sources required on the same bar, in boundary order for a channel.
    pub(crate) fn partners(&self) -> [Option<&AlertSource>; 2] {
        match self {
            Self::Compare { other, .. } => [Some(other), None],
            Self::Channel { boundaries, .. } => [Some(&boundaries[0]), Some(&boundaries[1])],
            _ => [None, None],
        }
    }

    /// Whether levels are finite and ordered, lookbacks positive, and referenced ids nonempty.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::StrategyFills(run) => !run.trim().is_empty(),
            Self::Crossing(v)
            | Self::CrossingUp(v)
            | Self::CrossingDown(v)
            | Self::Above(v)
            | Self::Below(v) => v.is_finite(),
            Self::EnteringChannel { lower, upper }
            | Self::ExitingChannel { lower, upper }
            | Self::InsideChannel { lower, upper }
            | Self::OutsideChannel { lower, upper } => {
                lower.is_finite() && upper.is_finite() && lower < upper
            }
            Self::MovingBy { value, bars } => value.is_finite() && *value != 0.0 && *bars > 0,
            Self::MovingByPercent { percent, bars } => {
                percent.is_finite() && *percent != 0.0 && *bars > 0
            }
            Self::Compare { other, .. } => other.is_valid(),
            Self::Channel { boundaries, .. } => {
                boundaries[0] != boundaries[1] && boundaries.iter().all(AlertSource::is_valid)
            }
            Self::VolumeAboveAverage { multiplier, bars } => {
                multiplier.is_finite() && *multiplier > 0.0 && *bars > 0
            }
        }
    }

    /// The single horizontal threshold, if the condition has one.
    pub fn level(&self) -> Option<f64> {
        match self {
            Self::Crossing(v)
            | Self::CrossingUp(v)
            | Self::CrossingDown(v)
            | Self::Above(v)
            | Self::Below(v) => Some(*v),
            _ => None,
        }
    }
}

impl std::fmt::Display for AlertCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StrategyFills(run) => write!(f, "Fills of strategy {run}"),
            Self::Crossing(v) => write!(f, "Crossing {v}"),
            Self::CrossingUp(v) => write!(f, "Crossing up {v}"),
            Self::CrossingDown(v) => write!(f, "Crossing down {v}"),
            Self::Above(v) => write!(f, "Greater than {v}"),
            Self::Below(v) => write!(f, "Less than {v}"),
            Self::EnteringChannel { lower, upper } => write!(f, "Entering {lower} to {upper}"),
            Self::ExitingChannel { lower, upper } => write!(f, "Exiting {lower} to {upper}"),
            Self::InsideChannel { lower, upper } => write!(f, "Inside {lower} to {upper}"),
            Self::OutsideChannel { lower, upper } => write!(f, "Outside {lower} to {upper}"),
            Self::MovingBy { value, bars } => write!(f, "Moving by {value} over {bars} bars"),
            Self::MovingByPercent { percent, bars } => {
                write!(f, "Moving by {percent}% over {bars} bars")
            }
            Self::Compare { comparison, .. } => write!(f, "{} another output", comparison),
            Self::Channel { comparison, .. } => write!(f, "{comparison}"),
            Self::VolumeAboveAverage { multiplier, bars } => write!(
                f,
                "Volume above {multiplier} times the preceding {bars}-bar mean"
            ),
        }
    }
}

impl Alert {
    /// Validate user input before handing it to host storage or evaluation. The length limits of
    /// the name and the message are not part of it: they bind whoever edits the text, as the
    /// alert dialog does, and an alert stored with a longer text keeps working.
    pub fn is_valid(&self) -> bool {
        (!self.symbol.trim().is_empty()
            || self
                .watchlist
                .as_ref()
                .is_some_and(|id| !id.trim().is_empty()))
            && self
                .watchlist
                .as_ref()
                .is_none_or(|id| !id.trim().is_empty())
            && (!matches!(self.condition, AlertCondition::StrategyFills(_))
                || (self.source == AlertSource::Price
                    && self.additional_conditions.is_empty()
                    && matches!(
                        self.frequency,
                        AlertFrequency::EveryTime | AlertFrequency::Once
                    )))
            && self
                .additional_conditions
                .iter()
                .all(|rule| !matches!(rule.condition, AlertCondition::StrategyFills(_)))
            && self.additional_conditions.len() < 5
            && self.additional_timeframes.len() <= self.additional_conditions.len()
            && {
                let intervals = self.timeframes();
                intervals.len() == 1 || intervals.iter().all(|interval| !interval.is_trade_based())
            }
            && self.source.is_valid()
            && self.condition.is_valid()
            && self
                .additional_conditions
                .iter()
                .all(|rule| rule.source.is_valid() && rule.condition.is_valid())
            && self
                .schedule
                .as_ref()
                .is_none_or(|schedule| schedule.is_valid())
            && self.delivery.is_valid()
            && self
                .starts
                .zip(self.expires)
                .is_none_or(|(starts, expires)| starts < expires)
    }
}

/// When an alert's notifications may be delivered: on the selected weekdays, inside the time
/// windows, both read on the clock of `timezone`, so a daylight-saving change moves the windows
/// with the local clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationSchedule {
    /// The zone whose weekday and clock decide.
    pub timezone: TimezoneId,
    /// Weekdays on which a window starts, at least one.
    pub days: Vec<Weekday>,
    /// Delivery windows, at least one.
    pub windows: Vec<NotificationWindow>,
}

/// A window of the day, from `start` up to but excluding `end`, in the schedule's zone. A window
/// whose `end` is before `start` crosses midnight and belongs to the weekday it starts on. A
/// window whose `end` equals `start` covers the 24 hours from `start`, ending at `start` of the
/// next day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationWindow {
    /// First clock time inside the window.
    pub start: NaiveTime,
    /// First clock time after the window; equal to `start` for a full day.
    pub end: NaiveTime,
}

impl NotificationSchedule {
    /// Whether at least one weekday and one window are selected.
    pub fn is_valid(&self) -> bool {
        !self.days.is_empty() && !self.windows.is_empty()
    }

    /// Whether a notification raised at `at` may be delivered.
    pub fn allows(&self, at: DateTime<Utc>) -> bool {
        let local = at.with_timezone(&self.timezone);
        let (day, time) = (local.weekday(), local.time());
        self.windows.iter().any(|w| {
            if w.start < w.end {
                self.days.contains(&day) && (w.start..w.end).contains(&time)
            } else {
                (self.days.contains(&day) && time >= w.start)
                    || (self.days.contains(&day.pred()) && time < w.end)
            }
        })
    }
}

/// How often an alert fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertFrequency {
    /// Fires once; the host then reports the alert with status `Triggered`.
    #[default]
    Once,
    /// Stays active and fires on every new occurrence; how often a lasting condition fires again
    /// is the host's choice.
    EveryTime,
    /// At most one trigger per bar, identified by its opening time.
    OncePerBar,
    /// Evaluate only completed bars, at most once per bar.
    OncePerBarClose,
    /// At most one trigger per clock minute of UTC, identified by the minute's start, whatever the
    /// chart's interval.
    OncePerMinute,
}

impl std::fmt::Display for AlertFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Once => "Only once",
            Self::EveryTime => "Every time",
            Self::OncePerBar => "Once per bar",
            Self::OncePerBarClose => "Once per bar close",
            Self::OncePerMinute => "Once per minute",
        })
    }
}

impl std::fmt::Display for AlertComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Crossing => "Crossing",
            Self::CrossingUp => "Crossing up",
            Self::CrossingDown => "Crossing down",
            Self::Above => "Greater than",
            Self::Below => "Less than",
        })
    }
}

/// Where an alert stands. The host sets it; the library only shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertStatus {
    /// Evaluated and waiting for its condition.
    #[default]
    Active,
    /// Fired and no longer evaluated, as an alert with frequency `Once` is after it fires.
    Triggered,
    /// Kept but not evaluated until the user enables it again.
    Disabled,
    /// Its expiry time passed before it fired.
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_named_compound_alert_round_trips() {
        let stored = serde_json::json!({
            "symbol": "AAPL", "name": "Momentum", "message": "Check momentum",
            "timeframe": "Hour1", "additional_timeframes": ["Min5"],
            "created_at": "2026-10-01T10:00:00Z", "last_triggered_at": "2026-10-02T11:30:00Z",
            "extend_line": true, "blink_on_trigger": false,
            "watchlist": "technology", "source": {"Study": {"id": "rsi", "output": 0}},
            "condition": {"Above": 70.0}, "frequency": "OncePerBarClose",
            "additional_conditions": [{"source": "Price", "condition": {"EnteringChannel": {"lower": 100.0, "upper": 110.0}}}]
        });
        let alert: Alert = serde_json::from_value(stored.clone()).unwrap();
        let saved = serde_json::to_value(alert).unwrap();
        for (key, value) in stored.as_object().unwrap() {
            assert_eq!(saved.get(key), Some(value), "{key}");
        }
    }

    #[test]
    fn messages_fill_placeholders_from_the_evaluated_bar() {
        let time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let bar = Bar::new(time, 99.0, 102.5, f64::NAN, 101.5, 1000.0);
        let study = AlertSource::Study {
            id: "3".into(),
            output: 0,
        };
        // Output 0 of the study is 70.25 and output 1 is 3; no other output has a value.
        let value = |source: &AlertSource| match source {
            AlertSource::Study { id, output: 0 } if id == "3" => Some(70.25),
            AlertSource::Study { id, output: 1 } if id == "3" => Some(3.0),
            AlertSource::Price => Some(101.5),
            _ => None,
        };
        let drawing = AlertSource::Drawing("d#1".into());
        let trigger = DateTime::from_timestamp(1_704_067_260, 0).unwrap();
        for (message, source, expected) in [
            (
                "{{symbol}} closed at {{close}}",
                AlertSource::Price,
                "NASDAQ:AAPL closed at 101.5",
            ),
            (
                "{{timeframe}}={{interval}} {{time}} {{open}} {{high}} [{{low}}] {{volume}}",
                AlertSource::Price,
                "5min=5min 2024-01-01T00:00:00+00:00 99 102.5 [NaN] 1000",
            ),
            (
                "{{ticker}} on {{exchange}} at {{trigger_time}}",
                AlertSource::Price,
                "AAPL on NASDAQ at 2024-01-01T00:01:00+00:00",
            ),
            ("{{value}}/{{output_1}}", study.clone(), "70.25/3"),
            ("{{output_2}}.", study.clone(), "NaN."),
            ("{{value}}{{output_1}}", AlertSource::Price, "101.5"),
            (
                "{{plot_3_1}} {{plot_3_0}} {{plot_3_2}} {{plot_9_0}} {{plot_3}}",
                AlertSource::Price,
                "3 70.25 NaN NaN {{plot_3}}",
            ),
            ("{{level}} at {{value}}", drawing, "Upper at NaN"),
            ("[{{level}}]", AlertSource::Price, "[]"),
            (
                "{{ close }} {{nope}} {{output_x}} {{close",
                AlertSource::Price,
                "{{ close }} {{nope}} {{output_x}} {{close",
            ),
            ("{{{{close}}}}", AlertSource::Price, "{{{{close}}}}"),
        ] {
            let mut alert = Alert::new("NASDAQ:AAPL", AlertCondition::Above(100.0));
            alert.source = source;
            alert.message = message.into();
            alert.inputs = Some(Box::new(AlertInputs {
                sources: vec![(AlertSource::Drawing("d#1".into()), "Upper".into())],
                ..AlertInputs::default()
            }));
            assert_eq!(
                alert.expand_message("NASDAQ:AAPL", None, Timeframe::Min5, &bar, trigger, value),
                expected,
                "{message}"
            );
        }
    }

    #[test]
    fn exchange_placeholder_prefers_the_reported_exchange_over_the_symbol_prefix() {
        let time = DateTime::from_timestamp(0, 0).unwrap();
        let bar = Bar::new(time, 1.0, 1.0, 1.0, 1.0, 1.0);
        let mut alert = Alert::new("SAMPLE", AlertCondition::Above(0.0));
        alert.message = "{{exchange}}/{{ticker}}".into();
        for (symbol, exchange, expected) in [
            ("NYSE:IBM", None, "NYSE/IBM"),
            ("NYSE:IBM", Some(""), "NYSE/IBM"),
            ("NYSE:IBM", Some("ARCA"), "ARCA/IBM"),
            ("AAPL", Some("NASDAQ"), "NASDAQ/AAPL"),
            ("AAPL", None, "/AAPL"),
        ] {
            assert_eq!(
                alert.expand_message(symbol, exchange, Timeframe::Min1, &bar, time, |_| None),
                expected
            );
        }
    }

    /// The drawing menu and the floating toolbar create the price crossing a drawing level
    /// (`DrawingContextMenu::open_on`), the dialog a channel between two levels, and
    /// `prepare_alert` stores each level's label in the inputs.
    #[test]
    fn level_names_the_drawing_level_a_price_alert_compares_with() {
        let drawing = |id: &str| AlertSource::Drawing(id.into());
        let time = DateTime::from_timestamp(0, 0).unwrap();
        let bar = Bar::new(time, 123.0, 124.0, 122.0, 123.6, 10.0);
        for (condition, expected) in [
            (
                AlertCondition::Compare {
                    other: drawing("fib#1"),
                    comparison: AlertComparison::Crossing,
                },
                "SAMPLE crossed Fib Retracement 7: 0.236",
            ),
            (
                AlertCondition::Channel {
                    boundaries: Box::new([drawing("fib#1"), drawing("fib#2")]),
                    comparison: AlertChannelComparison::Entering,
                },
                "SAMPLE crossed Fib Retracement 7: 0.236, Fib Retracement 7: 0.382",
            ),
            (
                AlertCondition::Compare {
                    other: AlertSource::Volume,
                    comparison: AlertComparison::Crossing,
                },
                "SAMPLE crossed ",
            ),
        ] {
            let mut alert = Alert::new("SAMPLE", condition);
            alert.inputs = Some(Box::new(AlertInputs {
                sources: vec![
                    (drawing("fib#1"), "Fib Retracement 7: 0.236".into()),
                    (drawing("fib#2"), "Fib Retracement 7: 0.382".into()),
                ],
                ..AlertInputs::default()
            }));
            alert.message = "{{ticker}} crossed {{level}}".into();
            assert_eq!(
                alert.expand_message("SAMPLE", None, Timeframe::Min1, &bar, time, |_| Some(123.6)),
                expected
            );
        }
    }

    #[test]
    fn saved_alerts_require_finite_levels_ordered_channels_and_positive_lookbacks() {
        type Edit = fn(&mut Alert);
        let rows: [(Edit, bool); 29] = [
            (
                |a| a.condition = AlertCondition::StrategyFills("run".into()),
                true,
            ),
            (
                |a| a.condition = AlertCondition::StrategyFills(" ".into()),
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::StrategyFills("run".into());
                    a.frequency = AlertFrequency::OncePerBar;
                },
                false,
            ),
            (
                |a| {
                    a.additional_conditions.push(AlertRule {
                        source: AlertSource::Price,
                        condition: AlertCondition::StrategyFills("run".into()),
                    });
                },
                false,
            ),
            (
                |a| {
                    let rule: AlertRule = serde_json::from_value(
                        serde_json::json!({"source":"Price", "condition":{"Above":1.0}}),
                    )
                    .unwrap();
                    a.additional_conditions = vec![rule; 4];
                },
                true,
            ),
            (
                |a| {
                    let rule: AlertRule = serde_json::from_value(
                        serde_json::json!({"source":"Price", "condition":{"Above":1.0}}),
                    )
                    .unwrap();
                    a.additional_conditions = vec![rule; 5];
                },
                false,
            ),
            (
                |a| {
                    *a = serde_json::from_value(serde_json::json!({
                    "symbol":"SAMPLE", "condition":{"Above":1.0}, "additional_timeframes":["Min5"]
                })).unwrap()
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::StrategyFills("run".into());
                    a.additional_conditions.push(AlertRule {
                        source: AlertSource::Price,
                        condition: AlertCondition::Above(1.0),
                    });
                },
                false,
            ),
            (
                |a| {
                    *a = serde_json::from_value(serde_json::json!({
                        "symbol":"SAMPLE", "timeframe":{"Ticks":2}, "condition":{"Above":1.0},
                        "additional_conditions":[{"source":"Volume","condition":{"Above":1.0}}]
                    }))
                    .unwrap()
                },
                true,
            ),
            (
                |a| {
                    *a = serde_json::from_value(serde_json::json!({
                        "symbol":"SAMPLE", "timeframe":"Min1", "condition":{"Above":1.0},
                        "additional_timeframes":[{"Ticks":2}],
                        "additional_conditions":[{"source":"Volume","condition":{"Above":1.0}}]
                    }))
                    .unwrap()
                },
                false,
            ),
            (|a| a.condition = AlertCondition::Crossing(f64::NAN), false),
            (
                |a| a.condition = AlertCondition::Above(f64::INFINITY),
                false,
            ),
            (|a| a.condition = AlertCondition::Below(-12.5), true),
            (
                |a| {
                    a.condition = AlertCondition::InsideChannel {
                        lower: 1.0,
                        upper: 2.0,
                    }
                },
                true,
            ),
            (
                |a| {
                    a.condition = AlertCondition::OutsideChannel {
                        lower: 2.0,
                        upper: 2.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: Vec::new(),
                        windows: vec![NotificationWindow {
                            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                            end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
                        }],
                    }))
                },
                false,
            ),
            (
                |a| {
                    a.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: vec![Weekday::Mon],
                        windows: vec![NotificationWindow {
                            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                            end: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                        }],
                    }))
                },
                true,
            ),
            (
                |a| {
                    let time = DateTime::from_timestamp(1_000, 0);
                    a.starts = time;
                    a.expires = time;
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::EnteringChannel {
                        lower: 2.0,
                        upper: 1.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::ExitingChannel {
                        lower: 1.0,
                        upper: 1.0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::MovingBy {
                        value: 1.0,
                        bars: 0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::MovingByPercent {
                        percent: 0.0,
                        bars: 1,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::VolumeAboveAverage {
                        multiplier: -2.0,
                        bars: 20,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::VolumeAboveAverage {
                        multiplier: 2.0,
                        bars: 0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.source = AlertSource::Study {
                        id: " ".into(),
                        output: 0,
                    }
                },
                false,
            ),
            (|a| a.symbol.clear(), false),
            (
                |a| {
                    a.symbol.clear();
                    a.watchlist = Some("technology".into());
                },
                true,
            ),
            (|a| a.watchlist = Some(" ".into()), false),
            (
                |a| {
                    a.additional_conditions.push(AlertRule {
                        source: AlertSource::Drawing(String::new()),
                        condition: AlertCondition::Above(1.0),
                    })
                },
                false,
            ),
        ];
        for (index, (edit, valid)) in rows.into_iter().enumerate() {
            let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(1.0));
            edit(&mut alert);
            assert_eq!(alert.is_valid(), valid, "row {index}");
        }
    }

    #[test]
    fn a_minimal_stored_alert_loads() {
        let stored = r#"{"symbol":"AAPL","condition":{"CrossingUp":101.5}}"#;
        let alert: Alert = serde_json::from_str(stored).unwrap();
        assert_eq!(
            alert,
            Alert {
                id: None,
                created_at: None,
                last_triggered_at: None,
                extend_line: true,
                blink_on_trigger: true,
                name: String::new(),
                source: AlertSource::Price,
                watchlist: None,
                timeframe: None,
                inputs: None,
                additional_conditions: Vec::new(),
                additional_timeframes: Vec::new(),
                symbol: "AAPL".into(),
                condition: AlertCondition::CrossingUp(101.5),
                frequency: AlertFrequency::Once,
                starts: None,
                expires: None,
                schedule: None,
                delivery: AlertDelivery::default(),
                message: String::new(),
                status: AlertStatus::Active,
            }
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn a_stored_schedule_round_trips_and_a_window_may_cross_midnight() {
        let stored = serde_json::json!({
            "symbol": "AAPL", "condition": {"InsideChannel": {"lower": 1.0, "upper": 2.0}},
            "frequency": "OncePerMinute",
            "schedule": {"timezone": "America/New_York", "days": ["Fri"],
                "windows": [{"start": "22:00:00", "end": "02:00:00"}]}
        });
        let alert: Alert = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&alert).unwrap()["schedule"],
            stored["schedule"]
        );
        let schedule = alert.schedule.unwrap();
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        // Friday 2026-03-06 22:30 EST is 03:30 UTC on Saturday: inside. Saturday 01:59 EST
        // still belongs to Friday's window; 02:00 does not.
        assert!(schedule.allows(at("2026-03-07T03:30:00Z")));
        assert!(schedule.allows(at("2026-03-07T06:59:00Z")));
        assert!(!schedule.allows(at("2026-03-07T07:00:00Z")));
        // Thursday 22:30 EST: its weekday is not selected.
        assert!(!schedule.allows(at("2026-03-06T03:30:00Z")));
        // After the clocks go forward on Sunday 2026-03-08, Friday 22:30 is 02:30 UTC.
        assert!(schedule.allows(at("2026-03-14T02:30:00Z")));
        assert!(schedule.allows(at("2026-03-14T05:59:00Z")));
        assert!(!schedule.allows(at("2026-03-14T06:00:00Z")));
        // Equal bounds cover 24 hours from the start: Friday 09:00 EST up to Saturday 09:00.
        let full_day = NotificationSchedule {
            windows: vec![NotificationWindow {
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            }],
            ..*schedule
        };
        assert!(full_day.allows(at("2026-03-06T14:00:00Z")));
        assert!(full_day.allows(at("2026-03-07T13:59:00Z")));
        assert!(!full_day.allows(at("2026-03-07T14:00:00Z")));
        assert!(!full_day.allows(at("2026-03-06T13:59:00Z")));
    }
}
