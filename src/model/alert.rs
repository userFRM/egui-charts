//! Typed alerts, as a host or a server stores and evaluates them.

use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{Bar, Timeframe, TimezoneId};

/// The host's identifier of an alert.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlertId(
    /// The host's own identifier, unique among its alerts.
    pub String,
);

/// One trigger as the alert log keeps it, also after its alert is removed. `AlertEngine` keeps
/// the newest ones for the host to store beside the alerts.
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use egui_charts::model::{AlertId, AlertTrigger};
///
/// let time = Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap();
/// let mut trigger = AlertTrigger::new(AlertId("1".into()), "AAPL", time);
/// trigger.name = "Breakout".into();
/// trigger.price = Some(101.0);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AlertTrigger {
    /// Host alert identifier.
    pub id: AlertId,
    /// The alert's [`display_name`](Alert::display_name) when the trigger arrived, empty if its
    /// alert was not yet supplied.
    #[serde(default)]
    pub name: String,
    /// Symbol as known when the trigger arrived, empty if its alert was not yet supplied.
    #[serde(default)]
    pub symbol: String,
    /// Host trigger time, including triggers missed while the application was closed.
    pub time: DateTime<Utc>,
    /// Finite trigger price, if supplied.
    #[serde(default)]
    pub price: Option<f64>,
    /// The alert's message with its placeholders filled in, empty if there is none.
    #[serde(default)]
    pub message: String,
    /// The trigger fell outside the alert's notification schedule: logged without a popup.
    #[serde(default)]
    pub silent: bool,
}

impl AlertTrigger {
    /// A trigger of `id` on `symbol` at `time`, without a name, price or message, and not silent.
    pub fn new(id: AlertId, symbol: impl Into<String>, time: DateTime<Utc>) -> Self {
        Self {
            id,
            name: String::new(),
            symbol: symbol.into(),
            time,
            price: None,
            message: String::new(),
            silent: false,
        }
    }
}

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
    /// Hosts retain it on every trigger, even for alerts that remain active. `AlertEngine`
    /// counts it as a restored alert's last firing.
    #[serde(default)]
    pub last_triggered_at: Option<DateTime<Utc>>,
    /// Most recent firing time of each member of [`watchlist`](Self::watchlist) that fired, by
    /// symbol; empty for an alert on one symbol. Hosts keep it with the alert, as they keep
    /// `last_triggered_at`. `AlertEngine` fills it, and after a restart does not fire a member
    /// again in the bar or minute of its time: with frequency [`Once`](AlertFrequency::Once)
    /// a member it holds never fires again, and the alert becomes
    /// [`Triggered`](AlertStatus::Triggered) once it holds every member of the list. The engine
    /// clears it when a save changes what the alert watches or its status.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub member_triggered_at: BTreeMap<String, DateTime<Utc>>,
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
    /// Before it, evaluation ignores the alert, so its first evaluation after the start is its
    /// first one: for a new or edited alert a crossing then needs a value observed from the start
    /// on. Earlier than `expires` when both are set.
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
/// others itself, reading them from the saved alert, and
/// [`AlertNotice`](crate::services::AlertNotice) gives it what to send. The trigger log records
/// every firing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct AlertDelivery {
    /// Show a popup. Enabled by default.
    pub popup: bool,
    /// Play the sound chosen in the alert settings. Enabled by default.
    pub sound: bool,
    /// Ask the host to send an email to the account's address.
    pub email: bool,
    /// Ask the host to send a plain-text email, without formatting, as pagers and mail-to-text
    /// gateways need, to [`plain_text_address`](Self::plain_text_address). Independent of
    /// `email`, so one alert can send both.
    pub plain_text: bool,
    /// Where the host sends the plain-text email, such as a pager or a mail-to-text gateway;
    /// empty sends it to the alternative address the user's account holds. Send to it without
    /// surrounding spaces. Presets leave it out.
    pub plain_text_address: String,
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
            plain_text: false,
            plain_text_address: String::new(),
            push: false,
            webhook: false,
            webhook_url: String::new(),
        }
    }
}

impl AlertDelivery {
    /// Whether an enabled plain-text email's address, when given, is one email address and an
    /// enabled webhook has an `http` or `https` URL.
    pub fn is_valid(&self) -> bool {
        self.plain_text_is_valid() && self.webhook_is_valid()
    }

    /// Whether an enabled plain-text email's address is empty or one `local@domain` address: a
    /// domain of two or more labels, none empty, and no spaces, control characters or
    /// `@,;:<>()[]\"` in either part. Surrounding spaces do not count.
    pub(crate) fn plain_text_is_valid(&self) -> bool {
        let address = self.plain_text_address.trim();
        let forbidden =
            |c: char| c.is_whitespace() || c.is_control() || "@,;:<>()[]\\\"".contains(c);
        !self.plain_text
            || address.is_empty()
            || address.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty()
                    && domain.split('.').count() > 1
                    && domain.split('.').all(|label| !label.is_empty())
                    && !local.contains(forbidden)
                    && !domain.contains(forbidden)
            })
    }

    /// Whether an enabled webhook has an `http` or `https` URL.
    pub(crate) fn webhook_is_valid(&self) -> bool {
        let url = self.webhook_url.trim();
        !self.webhook
            || url.split_once("://").is_some_and(|(scheme, rest)| {
                (scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
                    && !rest.is_empty()
            })
    }
}

/// The source view and study definitions captured when an alert is created. The alert's
/// symbol and condition intervals select their bars; auxiliary requests follow these saved studies.
///
/// ```
/// use egui_charts::model::{AlertCalendar, AlertInputs, AlertStudy};
///
/// let mut inputs = AlertInputs::default();
/// inputs.calendar = Some(AlertCalendar::new("BTCUSD", None, None));
/// inputs.studies = vec![AlertStudy::new(0, "RSI", Vec::new())];
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AlertInputs {
    /// Calendar zone; `None` follows the system's local calendar.
    pub timezone: Option<TimezoneId>,
    /// Exchange calendar of the creation symbol. Other symbols resolve their own calendars.
    pub calendar: Option<AlertCalendar>,
    /// Source choices shown when editing, independent of the currently displayed chart.
    pub sources: Vec<(AlertSource, AlertSourceName)>,
    /// Include extended trading sessions.
    pub extended_hours: bool,
    /// Whether overnight hours are requested together with extended hours. False in older saves.
    #[serde(default)]
    pub overnight_hours: bool,
    /// Whether bars are requested adjusted for dividends, as the source chart requested them.
    /// False in older saves, which are evaluated on prices as traded.
    #[serde(default)]
    pub dividend_adjusted: bool,
    /// Host currency or unit conversion.
    pub conversion: Option<String>,
    /// Initial main-series history size when no start time was available.
    pub count: usize,
    /// Oldest main bar used at creation. Later evaluations retain this seed.
    pub start: Option<DateTime<Utc>>,
    /// Creation window for studies whose calculation depends on visible bars, inclusive.
    pub visible: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Drawing levels interpolate logarithmically: the scale of the pane that paints the drawing
    /// the first condition names, or the main series' scale.
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
            overnight_hours: false,
            dividend_adjusted: false,
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
    /// their original definitions, including when another chart reuses an identifier. A saved
    /// name whose English text equals the offered name takes the offered one, so a name saved
    /// as finished text is translated from then on.
    pub fn add_sources_from(&mut self, other: &Self) {
        for study in &other.studies {
            if !self.studies.iter().any(|saved| saved.id == study.id) {
                self.studies.push(study.clone());
            }
        }
        for (source, name) in &other.sources {
            match self.sources.iter_mut().find(|(saved, _)| saved == source) {
                Some((_, saved)) if *saved == *name => *saved = name.clone(),
                Some(_) => {}
                None => self.sources.push((source.clone(), name.clone())),
            }
        }
    }
}

/// The creation symbol's exchange calendar, including an explicitly continuous session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AlertCalendar {
    /// Symbol whose calendar was captured.
    pub symbol: String,
    /// Exchange zone, when supplied by the host.
    pub timezone: Option<TimezoneId>,
    /// Trading windows; `None` selects a continuous session.
    pub session: Option<super::Session>,
}

impl AlertCalendar {
    /// The calendar of `symbol`: its exchange `timezone` and trading `session`, continuous when
    /// `session` is `None`.
    ///
    /// ```
    /// use egui_charts::model::AlertCalendar;
    ///
    /// let calendar = AlertCalendar::new("BTCUSD", None, None);
    /// assert_eq!(calendar.session, None);
    /// ```
    pub fn new(
        symbol: impl Into<String>,
        timezone: Option<TimezoneId>,
        session: Option<super::Session>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            timezone,
            session,
        }
    }
}

/// Serializable calculation inputs of a study, without computed values or shared stream state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AlertStudy {
    /// Identifier used by the alert sources and by dependent studies.
    pub id: usize,
    /// Name resolved by the study factory.
    pub name: String,
    /// Typed parameters, including auxiliary symbols and intervals.
    pub inputs: Vec<(String, super::InputValue)>,
}

impl AlertStudy {
    /// The study `name`, known to the alert sources as `id`, calculated with `inputs`.
    ///
    /// ```
    /// use egui_charts::model::{AlertStudy, InputValue};
    ///
    /// let study = AlertStudy::new(0, "RSI", vec![("length".into(), InputValue::Length(14))]);
    /// assert_eq!(study.name, "RSI");
    /// ```
    pub fn new(
        id: usize,
        name: impl Into<String>,
        inputs: Vec<(String, super::InputValue)>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            inputs,
        }
    }
}

impl Alert {
    /// An active alert on `symbol` with no id, no expiry and no message. Frequency is `Once`
    /// for bar conditions and `EveryTime` for strategy fills, detector signals and custom events.
    pub fn new(symbol: impl Into<String>, condition: AlertCondition) -> Self {
        Self {
            id: None,
            created_at: None,
            last_triggered_at: None,
            member_triggered_at: BTreeMap::new(),
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
            frequency: if condition.is_event() {
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
        copy.member_triggered_at
            .clone_from(&other.member_triggered_at);
        copy.extend_line = other.extend_line;
        copy.blink_on_trigger = other.blink_on_trigger;
        copy.delivery.clone_from(&other.delivery);
        // A position's progress is what evaluation reports, not what it watches.
        if let (
            AlertCondition::Position { progress, .. },
            AlertCondition::Position {
                progress: reported, ..
            },
        ) = (&mut copy.condition, &other.condition)
        {
            *progress = *reported;
        }
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
    /// value of output `N` of the study `ID` whatever the source is, and `{{level}}` the English
    /// name that [`AlertInputs::sources`] gives the drawing or TPO level the first condition tests:
    /// the source if it is one, otherwise the level it compares with or the drawings bounding its
    /// channel, joined by `, `, for a position the level reached last (the entry until its exit),
    /// and empty if it tests none. `value` gives the value of a source at
    /// the bar. A value that is missing or not finite becomes `NaN`, a number is written in the
    /// shortest form that reads back the same. A strategy fill alert also fills `{{order_id}}`,
    /// `{{side}}`, `{{quantity}}`, `{{position}}`, `{{comment}}` and `{{alert_message}}`, as
    /// `AlertEngine::strategy_fills` describes.
    /// A placeholder the alert does not know stays as written.
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
        self.expand_message_with(symbol, exchange, timeframe, bar, trigger, value, &[])
    }

    /// [`expand_message`](Self::expand_message) with occurrence fields; ordinary fields take precedence.
    pub(crate) fn expand_message_with(
        &self,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        bar: &Bar,
        trigger: DateTime<Utc>,
        value: impl Fn(&AlertSource) -> Option<f64>,
        extra: &[(&str, String)],
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
                    let tested = match (&self.source, &self.condition) {
                        (_, AlertCondition::Position { levels, progress }) => [
                            Some(match progress {
                                AlertPositionProgress::TargetReached => &levels[1],
                                AlertPositionProgress::StopReached => &levels[2],
                                _ => &levels[0],
                            }),
                            None,
                            None,
                        ],
                        (AlertSource::Drawing(_) | AlertSource::TPOLevel(_), _) => {
                            [Some(&self.source), None, None]
                        }
                        _ => self.condition.partners(),
                    };
                    let saved = self.inputs.as_ref().map_or(&[][..], |i| &i.sources);
                    let labels: Vec<_> = tested
                        .into_iter()
                        .flatten()
                        .filter(|source| {
                            matches!(source, AlertSource::Drawing(_) | AlertSource::TPOLevel(_))
                        })
                        .filter_map(|source| saved.iter().find(|(s, _)| s == source))
                        .map(|(_, label)| label.to_string())
                        .collect();
                    labels.join(", ")
                }
                _ => {
                    if let Some((_, text)) = extra.iter().find(|(key, _)| *key == name) {
                        return Some(text.clone());
                    }
                    if let Some((id, output)) = plot_placeholder(name) {
                        return Some(study(id, output));
                    }
                    let output = name.strip_prefix("output_")?.parse().ok()?;
                    match &self.source {
                        AlertSource::Study { id, .. } | AlertSource::StudyCondition { id, .. } => {
                            study(id, output)
                        }
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
///
/// On the price, the crossing, above, below and channel rules, also against a drawing in
/// [`Compare`](Self::Compare) and between two drawings in [`Channel`](Self::Channel), read every
/// price known to have traded since the alert's previous evaluation: the close, and the high and
/// low of the bars received where they moved past what that evaluation saw. A level or boundary
/// reached and left between two evaluations therefore counts, and a channel can then be both
/// inside and outside on one evaluation. The first evaluation, and an alert at bar close, read
/// the close alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertCondition {
    /// Each fill of the host's uniquely named strategy run. Feed `Backtest::update` results to
    /// `AlertEngine::strategy_fills` with the same run id. Uses the price source, no additional
    /// rules, and frequency `EveryTime` (the constructor default) or `Once`. No bars evaluate it.
    StrategyFills(String),
    /// Each new formation or target of the host's detector named here: every
    /// [`Signal`](super::Signal) of the symbol whose category is this name, once per signal id.
    /// `AlertEngine::detector_signals` takes the signals, and `AlertEvaluator` feeds it the
    /// signals of the charts it is lent. Same source, rules and frequencies as `StrategyFills`.
    DetectorSignals(String),
    /// Each [`CustomAlertEvent`] the host's event source named here raises, from its own logic,
    /// a Rust custom study or a strategy's alert call. Feed them to `AlertEngine::custom_events`
    /// with the same source name; `AlertEngine` saves the alert only once the host connects that
    /// source. Same source, rules and frequencies as `StrategyFills`.
    CustomEvents(String),
    /// The previous value was strictly below or above the level and a value since reaches or
    /// passes it, in either direction.
    Crossing(f64),
    /// The previous price was below the level and a price since is at or above it.
    CrossingUp(f64),
    /// The previous price was above the level and a price since is at or below it.
    CrossingDown(f64),
    /// A price since the previous evaluation is above the level.
    Above(f64),
    /// A price since the previous evaluation is below the level.
    Below(f64),
    /// The previous value was outside the inclusive channel and a value since is inside it.
    EnteringChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// The previous value was inside the inclusive channel and a value since is outside it.
    ExitingChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// A value since the previous evaluation is inside the inclusive channel,
    /// `lower <= value <= upper`, whatever the previous value was.
    InsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// A value since the previous evaluation is outside the inclusive channel, below `lower` or
    /// above `upper`, whatever the previous value was.
    OutsideChannel {
        /// Inclusive lower boundary, strictly below `upper`.
        lower: f64,
        /// Inclusive upper boundary.
        upper: f64,
    },
    /// Signed change from `bars` bars ago reaches `value`: at least for positive, at most for negative.
    /// The dialog offers positive values as Moving up and negative values as Moving down.
    MovingBy {
        /// Signed, nonzero change in source units.
        value: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Signed percentage change, 100 * (current - previous) / abs(previous), over `bars` bars.
    /// A zero previous value cannot satisfy this condition. The dialog offers positive percentages
    /// as Moving up % and negative percentages as Moving down %.
    MovingByPercent {
        /// Signed, nonzero percentage, with 1 meaning 1%.
        percent: f64,
        /// Number of bars back, at least one.
        bars: u32,
    },
    /// Compare against another output on the same bar, using its previous value for crossings.
    /// Against a drawing, the price reads every price traded since the previous evaluation, as
    /// the crossing rules do.
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
    /// previous evaluation, just as a crossing does. Between two drawings the price reads every
    /// price traded since the previous evaluation; a study boundary is known only at the close,
    /// so a channel with one reads the close.
    Channel {
        /// Stable sources of the two boundary lines, retained through drawing edits and storage.
        boundaries: Box<[AlertSource; 2]>,
        /// State or transition to test.
        comparison: AlertChannelComparison,
    },
    /// A long or short position: the value reaching the entry, then the first of the target and
    /// the stop. The entry is reached by crossing it, as [`Compare`](Self::Compare) with
    /// [`AlertComparison::Crossing`] crosses a moving level, and an exit reached before the entry
    /// counts for nothing. Once entered, a target above the entry makes the position long and one
    /// below it short, read from the current levels on each evaluation. A long position reaches
    /// its target at or above it and its stop at or below it, a short one the reverse, whatever
    /// the previous value was; the stop counts first when both are reached, and no exit is
    /// reached while the target equals the entry. The alert fires on the entry and on the exit,
    /// both on one evaluation that passes through the entry to an exit, and ends after the exit
    /// with status `Triggered`. Each trigger moves `progress` on, also outside the alert's
    /// schedule, where it is only logged. It takes frequency `Once`, no additional rules and no
    /// watchlist.
    Position {
        /// The entry, the target and the stop, in this order, such as levels 0, 1 and 2 of a
        /// position drawing's `Drawing::alert_source`.
        levels: Box<[AlertSource; 3]>,
        /// How far the alert has come. The service saves it with the alert on each trigger,
        /// so a restored alert resumes there.
        #[serde(default)]
        progress: AlertPositionProgress,
    },
    /// Current volume exceeds `multiplier` times the mean of the preceding `bars` completed volumes.
    VolumeAboveAverage {
        /// Positive, finite multiple of the preceding mean volume.
        multiplier: f64,
        /// Number of preceding bars in the mean, at least one.
        bars: u32,
    },
    /// The [`AlertSource::StudyCondition`] named by the source holds on this bar.
    /// It tests only that source, which takes no other condition. An undefined value does not hold.
    Holds,
}

/// A host-resolved value on a bar. Identifiers remain meaningful to the host across sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// A study's declared condition, indexed by
    /// [`Indicator::alert_conditions`](crate::studies::Indicator::alert_conditions).
    /// Reads 1 where it holds, 0 where it does not, and no value where undefined;
    /// `{{value}}` writes it as `1` or `0`. Only [`AlertCondition::Holds`] tests it.
    StudyCondition {
        /// Host's stable identifier for the study instance.
        id: String,
        /// Zero-based condition within the study.
        condition: usize,
    },
    /// A drawing's price at the bar's time, resolved by the host.
    Drawing(String),
    /// The bar's volume.
    Volume,
    /// A level of a completed TPO profile at the price it had when chosen; nothing resolves it.
    TPOLevel(TPOLevelSource),
}

/// A level of a completed TPO profile, kept at the price it had when the alert was made.
/// Merging, splitting or resetting profiles later, or a restart without the chart, leaves the
/// alert on that price. `Chart::alert_sources` offers one for each extended level that no later
/// bar has reached; with automatic rows the price is that of the rows drawn at the time.
///
/// ```
/// use egui_charts::model::{AlertSource, TPOLevel, TPOLevelSource};
///
/// let profile = "2024-01-03T10:00:00Z".parse().unwrap();
/// let poc = TPOLevelSource::new(profile, TPOLevel::PointOfControl, 112.0);
/// assert_eq!(AlertSource::TPOLevel(poc), AlertSource::TPOLevel(poc));
/// assert_eq!(poc.price, 112.0);
/// ```
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TPOLevelSource {
    /// Open time of the profile's first bar, its
    /// [`TPOProfile::session_date`](super::TPOProfile::session_date).
    pub profile: DateTime<Utc>,
    /// The level of that profile.
    pub level: super::TPOLevel,
    /// The level's price as its caption writes it: the lowest tick of its row, or the profile's
    /// high or low for a poor high or low.
    pub price: f64,
}

impl TPOLevelSource {
    /// The level `level` of the profile opening at `profile`, at `price`.
    pub fn new(profile: DateTime<Utc>, level: super::TPOLevel, price: f64) -> Self {
        Self {
            profile,
            level,
            price,
        }
    }
}

// Prices compare by their bits, so a stored source equals the one it was saved from.
impl PartialEq for TPOLevelSource {
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.level == other.level
            && self.price.to_bits() == other.price.to_bits()
    }
}

impl Eq for TPOLevelSource {}

/// The name an alert source is shown with: English text, translated only when shown.
///
/// `Chart::alert_sources` builds each name from an English template such as
/// `"{tool} {id}: {level}"`, whose placeholders take values shown as given, such as a study's
/// name or a drawing's number, and labels translated whole, such as a drawing tool, a level or a
/// study output, and keeps the English text they make. A host's own name converts from a string
/// and is translated whole. A name is stored and compared by its English text, which
/// [`Display`](std::fmt::Display) gives, so names saved as finished text, as alerts and presets
/// stored every name before names kept their parts, still match the chart's names.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlertSourceName(SourceName);

/// A name with its parts, or finished text: a host's own name, or one that alerts and presets
/// saved before names kept their parts hold.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum SourceName {
    /// `template` with each `{name}` taken from `values` as given or from `labels` translated,
    /// and `text`, the English it makes.
    Parts {
        text: String,
        template: String,
        values: BTreeMap<String, String>,
        labels: BTreeMap<String, String>,
    },
    /// Text translated whole.
    Text(String),
}

impl AlertSourceName {
    /// A name built from `template`, whose placeholders take `values` as given and `labels`
    /// translated when shown.
    #[cfg(feature = "drawings")]
    pub(crate) fn new(template: &str, values: &[(&str, &str)], labels: &[(&str, &str)]) -> Self {
        let owned = |parts: &[(&str, &str)]| {
            parts
                .iter()
                .map(|&(name, part)| (name.to_owned(), part.to_owned()))
                .collect()
        };
        Self(SourceName::Parts {
            text: crate::localization::substitute(template, &[values, labels].concat()),
            template: template.to_owned(),
            values: owned(values),
            labels: owned(labels),
        })
    }

    /// The name in the language of `ctx`'s translator: each label looked up whole, then the
    /// template looked up and filled with the values and the translated labels, as
    /// [`format_text`](crate::localization::format_text) does. A name without parts is looked up
    /// whole. Without a translator this is the English text.
    #[cfg(feature = "widget")]
    #[cfg_attr(docsrs, doc(cfg(feature = "widget")))]
    pub fn text(&self, ctx: &egui::Context) -> String {
        use crate::localization as l10n;
        match &self.0 {
            SourceName::Text(text) => l10n::text(ctx, text),
            SourceName::Parts {
                template,
                values,
                labels,
                ..
            } => {
                let labels: Vec<_> = labels
                    .iter()
                    .map(|(name, label)| (name.as_str(), l10n::text(ctx, label)))
                    .collect();
                let parts: Vec<(&str, &str)> = values
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .chain(labels.iter().map(|(name, label)| (*name, label.as_str())))
                    .collect();
                l10n::format_text(ctx, template, &parts)
            }
        }
    }

    /// The English label `part` of a name built from parts.
    #[cfg(feature = "ui")]
    pub(crate) fn label(&self, part: &str) -> Option<&str> {
        match &self.0 {
            SourceName::Parts { labels, .. } => labels.get(part).map(String::as_str),
            SourceName::Text(_) => None,
        }
    }

    /// The English text.
    fn english(&self) -> &str {
        match &self.0 {
            SourceName::Parts { text, .. } | SourceName::Text(text) => text,
        }
    }
}

impl From<String> for AlertSourceName {
    fn from(text: String) -> Self {
        Self(SourceName::Text(text))
    }
}

impl From<&str> for AlertSourceName {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

impl std::fmt::Display for AlertSourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.english())
    }
}

/// Names are equal when their English text is, whether or not they keep their parts.
impl PartialEq for AlertSourceName {
    fn eq(&self, other: &Self) -> bool {
        self.english() == other.english()
    }
}

impl Eq for AlertSourceName {}

/// Comparison of two outputs. Crossings require a strict previous side and include equality now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum AlertChannelComparison {
    /// Previously outside, and a value since is inside.
    Entering,
    /// Previously inside, and a value since is outside.
    Exiting,
    /// A value since the previous evaluation is inside, regardless of the previous value.
    Inside,
    /// A value since the previous evaluation is outside, regardless of the previous value.
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

/// How far a [`Position`](AlertCondition::Position) alert has come.
///
/// ```
/// use egui_charts::model::{Alert, AlertCondition, AlertPositionProgress, AlertSource};
///
/// let level = |id: &str| AlertSource::Drawing(id.into());
/// let alert = Alert::new(
///     "AAPL",
///     AlertCondition::Position {
///         levels: Box::new([level("7"), level("7#1"), level("7#2")]),
///         progress: AlertPositionProgress::AwaitingEntry,
///     },
/// );
/// assert!(alert.is_valid());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AlertPositionProgress {
    /// Waiting for the value to reach the entry, also after an ended alert is made active again.
    #[default]
    AwaitingEntry,
    /// The entry was reached; waiting for the target or the stop.
    Entered,
    /// The target was reached first, which ended the alert.
    TargetReached,
    /// The stop was reached first, which ended the alert.
    StopReached,
}

impl std::fmt::Display for AlertPositionProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AwaitingEntry => "Waiting for position entry",
            Self::Entered => "Position entered",
            Self::TargetReached => "Position target reached",
            Self::StopReached => "Position stop reached",
        })
    }
}

/// One additional condition of an alert; all rules must hold at the same event time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AlertRule {
    /// The value to test.
    pub source: AlertSource,
    /// The required relationship.
    pub condition: AlertCondition,
}

impl AlertRule {
    /// The rule that `source` meets `condition`.
    pub fn new(source: AlertSource, condition: AlertCondition) -> Self {
        Self { source, condition }
    }
}

impl AlertSource {
    fn is_valid(&self) -> bool {
        match self {
            Self::Study { id, .. } | Self::StudyCondition { id, .. } | Self::Drawing(id) => {
                !id.trim().is_empty()
            }
            Self::TPOLevel(level) => level.price.is_finite(),
            _ => true,
        }
    }
}

impl AlertCondition {
    /// Other sources required on the same bar, in boundary order for a channel and as entry,
    /// target and stop for a position.
    pub(crate) fn partners(&self) -> [Option<&AlertSource>; 3] {
        match self {
            Self::Compare { other, .. } => [Some(other), None, None],
            Self::Channel { boundaries, .. } => [Some(&boundaries[0]), Some(&boundaries[1]), None],
            Self::Position { levels, .. } => [Some(&levels[0]), Some(&levels[1]), Some(&levels[2])],
            _ => [None; 3],
        }
    }

    /// Whether the host raises the condition's events, which no bars evaluate.
    pub(crate) fn is_event(&self) -> bool {
        matches!(
            self,
            Self::StrategyFills(_) | Self::DetectorSignals(_) | Self::CustomEvents(_)
        )
    }

    /// A declared study condition takes only `Holds`, which tests nothing else.
    pub(crate) fn pairs_with(&self, source: &AlertSource) -> bool {
        matches!(source, AlertSource::StudyCondition { .. }) == matches!(self, Self::Holds)
    }

    /// Whether levels are finite and ordered, lookbacks positive, and referenced ids nonempty.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Holds => true,
            Self::StrategyFills(name) | Self::DetectorSignals(name) | Self::CustomEvents(name) => {
                !name.trim().is_empty()
            }
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
            Self::Position { levels, .. } => {
                levels.iter().all(AlertSource::is_valid)
                    && levels[0] != levels[1]
                    && levels[1] != levels[2]
                    && levels[0] != levels[2]
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
            Self::Holds => f.write_str("Condition met"),
            Self::StrategyFills(run) => write!(f, "Fills of strategy {run}"),
            Self::DetectorSignals(detector) => write!(f, "Signals of detector {detector}"),
            Self::CustomEvents(source) => write!(f, "Events of {source}"),
            Self::Crossing(v) => write!(f, "Crossing {v}"),
            Self::CrossingUp(v) => write!(f, "Crossing up {v}"),
            Self::CrossingDown(v) => write!(f, "Crossing down {v}"),
            Self::Above(v) => write!(f, "Greater than {v}"),
            Self::Below(v) => write!(f, "Less than {v}"),
            Self::EnteringChannel { lower, upper } => write!(f, "Entering {lower} to {upper}"),
            Self::ExitingChannel { lower, upper } => write!(f, "Exiting {lower} to {upper}"),
            Self::InsideChannel { lower, upper } => write!(f, "Inside {lower} to {upper}"),
            Self::OutsideChannel { lower, upper } => write!(f, "Outside {lower} to {upper}"),
            Self::MovingBy { value, bars } => {
                let direction = if value.is_sign_negative() {
                    "down"
                } else {
                    "up"
                };
                write!(f, "Moving {direction} {} over {bars} bars", value.abs())
            }
            Self::MovingByPercent { percent, bars } => {
                let direction = if percent.is_sign_negative() {
                    "down"
                } else {
                    "up"
                };
                write!(f, "Moving {direction} {}% over {bars} bars", percent.abs())
            }
            Self::Compare { comparison, .. } => write!(f, "{} another output", comparison),
            Self::Channel { comparison, .. } => write!(f, "{comparison}"),
            Self::Position { progress, .. } => write!(f, "{progress}"),
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
            && (!self.condition.is_event()
                || (self.source == AlertSource::Price
                    && self.additional_conditions.is_empty()
                    && matches!(
                        self.frequency,
                        AlertFrequency::EveryTime | AlertFrequency::Once
                    )))
            && self.additional_conditions.iter().all(|rule| {
                !rule.condition.is_event()
                    && !matches!(rule.condition, AlertCondition::Position { .. })
            })
            && (!matches!(self.condition, AlertCondition::Position { .. })
                || (self.frequency == AlertFrequency::Once
                    && self.additional_conditions.is_empty()
                    && self.watchlist.is_none()))
            && self.additional_conditions.len() < 5
            && self.additional_timeframes.len() <= self.additional_conditions.len()
            && {
                let intervals = self.timeframes();
                intervals.len() == 1 || intervals.iter().all(|interval| !interval.is_trade_based())
            }
            && self.rules().all(|(source, condition, _)| {
                condition.pairs_with(source)
                    && !condition
                        .partners()
                        .into_iter()
                        .flatten()
                        .any(|source| matches!(source, AlertSource::StudyCondition { .. }))
            })
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

    /// The alert's name, or its condition when the name is blank, as the alert log and trigger
    /// popups name it.
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.condition.to_string()
        } else {
            self.name.clone()
        }
    }
}

/// When an alert's notifications may be delivered: on the selected weekdays, inside the time
/// windows, both read on the clock of `timezone`, so a daylight-saving change moves the windows
/// with the local clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct NotificationWindow {
    /// First clock time inside the window.
    pub start: NaiveTime,
    /// First clock time after the window; equal to `start` for a full day.
    pub end: NaiveTime,
}

impl NotificationWindow {
    /// The window from `start` up to but excluding `end`.
    pub fn new(start: NaiveTime, end: NaiveTime) -> Self {
        Self { start, end }
    }
}

impl NotificationSchedule {
    /// Delivery on `days` inside `windows`, both read on the clock of `timezone`;
    /// [`is_valid`](Self::is_valid) asks for at least one of each.
    ///
    /// ```
    /// use chrono::{NaiveTime, Weekday};
    /// use egui_charts::model::{NotificationSchedule, NotificationWindow, TimezoneId};
    ///
    /// let window = NotificationWindow::new(
    ///     NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
    ///     NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
    /// );
    /// let schedule = NotificationSchedule::new(TimezoneId::utc(), vec![Weekday::Mon], vec![window]);
    /// assert!(schedule.is_valid());
    /// ```
    pub fn new(timezone: TimezoneId, days: Vec<Weekday>, windows: Vec<NotificationWindow>) -> Self {
        Self {
            timezone,
            days,
            windows,
        }
    }

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
    /// Fires once; the host then reports the alert with status `Triggered`. On a watchlist it
    /// fires once for each member and becomes `Triggered` when every member has fired.
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

/// An event a host's event source raises for its
/// [`CustomEvents`](AlertCondition::CustomEvents) alerts: the host's own logic, a Rust custom
/// study or a strategy's alert call decides when.
///
/// ```
/// use egui_charts::model::CustomAlertEvent;
///
/// let time = chrono::DateTime::from_timestamp(0, 0).unwrap();
/// let mut event = CustomAlertEvent::new(1, time, "RSI divergence confirmed");
/// event.price = Some(101.5);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CustomAlertEvent {
    /// Increasing number within the source, from one. An event numbered at or below one an
    /// alert already received is a repeat and ignored.
    pub sequence: u64,
    /// When the event happened, which the trigger reports.
    pub time: DateTime<Utc>,
    /// The price the event refers to, finite, when it has one.
    pub price: Option<f64>,
    /// The text a trigger shows when the alert has no message of its own.
    pub message: String,
}

impl CustomAlertEvent {
    /// The event numbered `sequence` at `time` with `message` and no price.
    pub fn new(sequence: u64, time: DateTime<Utc>, message: impl Into<String>) -> Self {
        Self {
            sequence,
            time,
            price: None,
            message: message.into(),
        }
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
            AlertSource::StudyCondition { id, condition: 0 } if id == "3" => Some(1.0),
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
            (
                "{{value}}/{{output_1}}",
                AlertSource::StudyCondition {
                    id: "3".into(),
                    condition: 0,
                },
                "1/3",
            ),
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
        let tpo = AlertSource::TPOLevel(TPOLevelSource::new(
            time,
            crate::model::TPOLevel::PointOfControl,
            123.5,
        ));
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
            (
                AlertCondition::Position {
                    levels: Box::new([drawing("fib#2"), drawing("fib#3"), drawing("fib#1")]),
                    progress: AlertPositionProgress::Entered,
                },
                "SAMPLE crossed Fib Retracement 7: 0.382",
            ),
            (
                AlertCondition::Position {
                    levels: Box::new([drawing("fib#2"), drawing("fib#3"), drawing("fib#1")]),
                    progress: AlertPositionProgress::StopReached,
                },
                "SAMPLE crossed Fib Retracement 7: 0.236",
            ),
            (
                AlertCondition::Compare {
                    other: tpo.clone(),
                    comparison: AlertComparison::Crossing,
                },
                "SAMPLE crossed TPO 1 Jan: Point of control 123.5",
            ),
        ] {
            let mut alert = Alert::new("SAMPLE", condition);
            alert.inputs = Some(Box::new(AlertInputs {
                sources: vec![
                    (drawing("fib#1"), "Fib Retracement 7: 0.236".into()),
                    (drawing("fib#2"), "Fib Retracement 7: 0.382".into()),
                    (tpo.clone(), "TPO 1 Jan: Point of control 123.5".into()),
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
        fn position() -> AlertCondition {
            AlertCondition::Position {
                levels: Box::new(["d", "d#1", "d#2"].map(|id| AlertSource::Drawing(id.into()))),
                progress: AlertPositionProgress::AwaitingEntry,
            }
        }
        fn plain_text(a: &mut Alert, address: &str) {
            a.delivery.plain_text = true;
            a.delivery.plain_text_address = address.into();
        }
        let rows: [(Edit, bool); 58] = [
            (
                |a| {
                    a.source = AlertSource::StudyCondition {
                        id: "3".into(),
                        condition: 0,
                    };
                    a.condition = AlertCondition::Holds;
                },
                true,
            ),
            (|a| a.condition = AlertCondition::Holds, false),
            (
                |a| {
                    a.source = AlertSource::StudyCondition {
                        id: "3".into(),
                        condition: 0,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::Compare {
                        other: AlertSource::StudyCondition {
                            id: "3".into(),
                            condition: 0,
                        },
                        comparison: AlertComparison::Above,
                    }
                },
                false,
            ),
            (
                |a| {
                    a.source = AlertSource::StudyCondition {
                        id: " ".into(),
                        condition: 0,
                    };
                    a.condition = AlertCondition::Holds;
                },
                false,
            ),
            (
                |a| {
                    a.additional_conditions.push(AlertRule::new(
                        AlertSource::StudyCondition {
                            id: "3".into(),
                            condition: 1,
                        },
                        AlertCondition::Holds,
                    ))
                },
                true,
            ),
            (|a| a.condition = position(), true),
            (
                |a| {
                    a.condition = position();
                    a.frequency = AlertFrequency::EveryTime;
                },
                false,
            ),
            (
                |a| {
                    a.condition = position();
                    a.watchlist = Some("technology".into());
                },
                false,
            ),
            (
                |a| {
                    a.condition = position();
                    a.additional_conditions.push(AlertRule::new(
                        AlertSource::Price,
                        AlertCondition::Above(1.0),
                    ));
                },
                false,
            ),
            (
                |a| {
                    a.additional_conditions
                        .push(AlertRule::new(AlertSource::Price, position()))
                },
                false,
            ),
            (
                |a| {
                    a.condition = position();
                    if let AlertCondition::Position { levels, .. } = &mut a.condition {
                        levels[2] = levels[0].clone();
                    }
                },
                false,
            ),
            (|a| plain_text(a, "pager@example.com"), true),
            (|a| plain_text(a, " pager@example.com "), true),
            (|a| plain_text(a, ""), true),
            (|a| plain_text(a, "José@exämple.com"), true),
            (
                |a| {
                    a.delivery.email = true;
                    a.delivery.plain_text_address = "pager".into();
                },
                true,
            ),
            (|a| plain_text(a, "pager@example"), false),
            (|a| plain_text(a, "@example.com"), false),
            (|a| plain_text(a, "pager@example..com"), false),
            (|a| plain_text(a, "a@example.com, b@example.com"), false),
            (|a| plain_text(a, "pager@example.com\r\nBcc: x@y.z"), false),
            (
                |a| {
                    a.delivery.webhook = true;
                    a.delivery.webhook_url = "HTTPS://hooks.example.com/alert".into();
                },
                true,
            ),
            (
                |a| {
                    a.delivery.webhook = true;
                    a.delivery.webhook_url = "Http://10.0.0.2/hook".into();
                },
                true,
            ),
            (
                |a| {
                    a.delivery.webhook = true;
                    a.delivery.webhook_url = "ftp://hooks.example.com".into();
                },
                false,
            ),
            (
                |a| {
                    a.delivery.webhook = true;
                    a.delivery.webhook_url = "https://".into();
                },
                false,
            ),
            (
                |a| a.condition = AlertCondition::StrategyFills("run".into()),
                true,
            ),
            (
                |a| a.condition = AlertCondition::StrategyFills(" ".into()),
                false,
            ),
            (
                |a| a.condition = AlertCondition::CustomEvents(" ".into()),
                false,
            ),
            (
                |a| {
                    a.condition = AlertCondition::DetectorSignals("Big trades".into());
                    a.frequency = AlertFrequency::OncePerBarClose;
                },
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
            (
                |a| {
                    a.condition = AlertCondition::Compare {
                        other: AlertSource::TPOLevel(TPOLevelSource::new(
                            DateTime::UNIX_EPOCH,
                            crate::model::TPOLevel::PointOfControl,
                            f64::NAN,
                        )),
                        comparison: AlertComparison::Crossing,
                    }
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
                member_triggered_at: BTreeMap::new(),
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
        // A stored alert on a strategy run's fills loads as saved and stays armed.
        let fills: Alert = serde_json::from_str(
            r#"{"symbol":"AAPL","condition":{"StrategyFills":"run-1"},"frequency":"EveryTime"}"#,
        )
        .unwrap();
        assert_eq!(
            fills.condition,
            AlertCondition::StrategyFills("run-1".into())
        );
        assert!(fills.is_valid());
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
