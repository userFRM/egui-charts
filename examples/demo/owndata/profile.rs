//! The saved description of a visitor's data source: where history and the stream come from
//! and how their JSON maps to bars and trades. The mapped feed, the stream session and the
//! dialog all read it; it is also the exported file.

use std::fmt::Write as _;

use egui_charts::model::{Timeframe, TimezoneId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version written into every profile. A file with a higher version is refused on import.
pub(crate) const VERSION: u32 = 1;

/// The names every template knows. A key cannot take one of them.
pub(crate) const PLACEHOLDERS: [&str; 5] = ["symbol", "interval", "from", "to", "limit"];

/// One step of a path into a JSON value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Segment {
    /// A member of an object.
    Key(String),
    /// An element of an array.
    Index(usize),
}

/// The way from a starting value to a field, saved as a JSON array such as `["data", "candles"]`
/// or `[0]`. An empty path is the starting value itself; for an optional field it means that
/// the field is not mapped.
pub(crate) type FieldPath = Vec<Segment>;

/// One saved connection. It names the keys it needs and never holds their values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Profile {
    /// [`VERSION`] when written.
    pub(crate) version: u32,
    /// Name in the profile list, unique among the saved profiles.
    pub(crate) name: String,
    /// Symbols the symbol search offers, spelled as the service expects them. The chart opens
    /// on the first.
    pub(crate) symbols: Vec<String>,
    /// Zone of every time that names none: a date with a time of day, ISO 8601 text without an
    /// offset, and `{from}` and `{to}` printed as dates. UTC by default.
    pub(crate) time_zone: TimezoneId,
    /// The interval table, in the order the interval picker offers. The chart opens on the
    /// first row.
    pub(crate) intervals: Vec<Interval>,
    /// Names of the keys the templates use as `{name}`, such as `key`: lower-case letters,
    /// digits and `_`, and none of [`PLACEHOLDERS`].
    pub(crate) keys: Vec<String>,
    /// REST history. `None` charts the stream alone.
    pub(crate) history: Option<History>,
    /// WebSocket stream. `None` charts history alone.
    pub(crate) stream: Option<Stream>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            version: VERSION,
            name: String::new(),
            symbols: Vec::new(),
            time_zone: TimezoneId::utc(),
            intervals: Vec::new(),
            keys: Vec::new(),
            history: None,
            stream: None,
        }
    }
}

/// One row of the interval table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Interval {
    /// A chart interval with a duration. Intervals counted in trades, volume or range are not
    /// offered.
    pub(crate) chart: Timeframe,
    /// What `{interval}` becomes for it, such as `1m`, `60` or `60000`. Sent as typed and never
    /// parsed.
    pub(crate) service: String,
}

/// REST history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct History {
    /// URL template. `{symbol}`, `{interval}`, `{from}`, `{to}`, `{limit}` and the key names are
    /// replaced, each value percent-encoded.
    pub(crate) url: String,
    /// Request headers as a name and a value template. Values are sent as typed.
    pub(crate) headers: Vec<(String, String)>,
    /// Body template. Empty sends `GET`. Anything else is sent with `POST`, as
    /// `application/json` unless a header names another content type.
    pub(crate) body: String,
    /// How `{from}` and `{to}` are printed.
    pub(crate) range: RangeFormat,
    /// The most bars asked for in one request, the cap of `{limit}`. 1000 by default.
    pub(crate) page_size: usize,
    /// Where the rows of a response are and how they are laid out.
    pub(crate) rows: Rows,
    /// The fields of one row.
    pub(crate) bar: BarFields,
}

impl Default for History {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: Vec::new(),
            body: String::new(),
            range: RangeFormat::default(),
            page_size: 1000,
            rows: Rows::default(),
            bar: BarFields::default(),
        }
    }
}

/// How an instant is printed into `{from}` and `{to}`. `{from}` is the first instant asked for
/// and `{to}` the first one not asked for, except for the two date forms.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RangeFormat {
    /// Unix time in whole seconds. The default.
    #[default]
    Seconds,
    /// Unix time in milliseconds.
    Milliseconds,
    /// Unix time in microseconds.
    Microseconds,
    /// Unix time in nanoseconds.
    Nanoseconds,
    /// `2026-11-11T14:30:00Z`.
    Iso8601,
    /// `2026-11-11`. `{to}` is the last day asked for, itself included.
    Date,
    /// `20261111`. `{to}` is the last day asked for, itself included.
    CompactDate,
}

/// Where the rows of a history response are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Rows {
    /// From the root of the response to the rows. Empty when the root holds them.
    pub(crate) path: FieldPath,
    /// How the rows are laid out there.
    pub(crate) layout: Layout,
}

/// The layout of the rows of a history response.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layout {
    /// A list of objects, one per bar. Fields are keys such as `["t"]`. The default.
    #[default]
    Objects,
    /// A list of arrays, one per bar. Without `header`, fields are column numbers such as `[0]`.
    /// With it, the array of column names found at that path from the root of the response
    /// names the columns, and fields are those names.
    Arrays { header: Option<FieldPath> },
    /// An object of arrays of equal length, one per field. Bar `i` is element `i` of each, and
    /// fields are the keys of those arrays.
    Columns,
}

/// The fields of a bar, each a path inside one history row or one stream record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BarFields {
    /// The bar's time: its open or any instant inside it.
    pub(crate) time: TimeSpec,
    pub(crate) open: FieldPath,
    pub(crate) high: FieldPath,
    pub(crate) low: FieldPath,
    pub(crate) close: FieldPath,
    /// Empty when the service sends no volume. Bars then carry zero.
    pub(crate) volume: FieldPath,
}

/// How a row or record tells its time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TimeSpec {
    /// One field that holds an instant. The default, with an empty field and `Seconds`.
    Instant { field: FieldPath, unit: TimeUnit },
    /// A calendar date and, optionally, a time of day on the wall clock of the profile's zone.
    /// Without a time of day the row is a bar dated on that day.
    Wall {
        date: FieldPath,
        format: DateFormat,
        clock: Option<Clock>,
    },
}

impl Default for TimeSpec {
    fn default() -> Self {
        Self::Instant {
            field: FieldPath::new(),
            unit: TimeUnit::Seconds,
        }
    }
}

/// The form of an instant field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TimeUnit {
    /// Unix time in seconds; a fraction is allowed. The default.
    #[default]
    Seconds,
    /// Unix time in milliseconds; a fraction is allowed.
    Milliseconds,
    /// Unix time in microseconds.
    Microseconds,
    /// Unix time in nanoseconds.
    Nanoseconds,
    /// ISO 8601 text. With an offset or `Z` it is an instant; without one it is a wall-clock
    /// time in the profile's zone; a bare date is a bar dated on that day.
    Iso8601,
}

/// The form of a date field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DateFormat {
    /// `20261111`, as a number or as text. The default.
    #[default]
    Compact,
    /// `2026-11-11`.
    Dashed,
}

/// A time-of-day field.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Clock {
    pub(crate) field: FieldPath,
    pub(crate) unit: ClockUnit,
}

/// The form of a time-of-day field. All three are readings of the wall clock: 09:30:00 is the
/// same value on every day of the year, also on a day with a daylight saving change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClockUnit {
    /// Milliseconds since midnight: 09:30:00 is 34,200,000. The default.
    #[default]
    Milliseconds,
    /// Seconds since midnight: 09:30:00 is 34,200.
    Seconds,
    /// `HH:MM`, `HH:MM:SS` or `HH:MM:SS.fff`.
    Text,
}

/// The WebSocket stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Stream {
    /// `ws://` or `wss://` URL template. `{symbol}`, `{interval}` and the key names are replaced,
    /// each value percent-encoded.
    pub(crate) url: String,
    /// Subprotocols offered in the handshake, as templates. A browser cannot send headers when
    /// it opens a socket, so a key travels in the URL, in a subprotocol or in a connect message.
    pub(crate) protocols: Vec<String>,
    /// Messages sent in this order every time the socket opens: the login first, then the
    /// subscribe message. Templates with `{symbol}`, `{interval}` and the key names, sent as
    /// typed.
    pub(crate) connect: Vec<String>,
    /// Keep-alive message, sent as typed. Empty sends none.
    pub(crate) keep_alive: String,
    /// Seconds between keep-alive messages, at least 1. 30 by default.
    pub(crate) keep_alive_every: u32,
    /// From the root of a message to its records. Empty takes the root. An array there is one
    /// record per element; an object is one record.
    pub(crate) records: FieldPath,
    /// The field whose value picks the route, looked up in the record and, when the record has
    /// none, at the root of the message. Empty sends every record to the first route.
    pub(crate) route_by: FieldPath,
    /// The routes, tried in order.
    pub(crate) routes: Vec<Route>,
}

impl Default for Stream {
    fn default() -> Self {
        Self {
            url: String::new(),
            protocols: Vec::new(),
            connect: Vec::new(),
            keep_alive: String::new(),
            keep_alive_every: 30,
            records: FieldPath::new(),
            route_by: FieldPath::new(),
            routes: Vec::new(),
        }
    }
}

/// What to do with the records whose routing field has one value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Route {
    /// The value, compared as text: a string as it is, a number or a boolean as JSON prints it.
    pub(crate) value: String,
    pub(crate) kind: RouteKind,
}

/// What a route's records are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteKind {
    /// Status, keep-alive and other messages that carry no data. The default.
    #[default]
    Ignore,
    /// Bars of the chart's interval, merged into the bar with the same open time.
    Bars(BarFields),
    /// Trades, aggregated into bars of the chart's interval.
    Trades(TradeFields),
}

/// The fields of a trade, each a path inside one stream record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TradeFields {
    pub(crate) time: TimeSpec,
    pub(crate) price: FieldPath,
    pub(crate) size: FieldPath,
    /// Empty when trades carry no condition. The field holds one value or a list of values.
    pub(crate) condition: FieldPath,
    /// The rule of each condition value, compared as text. A value that is not listed counts
    /// price and volume.
    pub(crate) conditions: Vec<(String, ConditionRule)>,
}

/// What a trade with a given condition does to the bar it falls in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConditionRule {
    /// The price moves high, low and close, and the size is added to the volume. The default.
    #[default]
    PriceAndVolume,
    /// The size is added to the volume; open, high, low and close stay as they are.
    VolumeOnly,
    /// The bar stays as it is.
    Ignore,
}

impl Profile {
    /// The first reason the profile cannot connect, in words for the dialog, or `None` when it
    /// can: an empty name, no symbol, no interval, an interval listed twice or one without a
    /// duration, neither history nor a stream, a URL that does not start with `http://`,
    /// `https://`, `ws://` or `wss://` as its kind needs, a page size or keep-alive period of
    /// zero, or a key name that is empty, has other characters than lower-case letters, digits
    /// and `_`, or is one of [`PLACEHOLDERS`].
    pub(crate) fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("Give the profile a name.".to_owned());
        }
        if self.symbols.is_empty() {
            return Some("List at least one symbol.".to_owned());
        }
        if self.intervals.is_empty() {
            return Some("Add at least one interval.".to_owned());
        }
        for (index, interval) in self.intervals.iter().enumerate() {
            let chart = interval.chart;
            let above = &self.intervals[..index];
            if above.iter().any(|row| row.chart == chart) {
                return Some(format!("The interval {chart} is listed twice."));
            }
            if chart.is_trade_based() {
                return Some(format!("The interval {chart} has no duration."));
            }
        }
        if self.history.is_none() && self.stream.is_none() {
            return Some("Use REST history, a WebSocket stream or both.".to_owned());
        }
        if let Some(history) = &self.history
            && !(history.url.starts_with("http://") || history.url.starts_with("https://"))
        {
            return Some("The history URL must start with http:// or https://.".to_owned());
        }
        if let Some(stream) = &self.stream
            && !(stream.url.starts_with("ws://") || stream.url.starts_with("wss://"))
        {
            return Some("The stream URL must start with ws:// or wss://.".to_owned());
        }
        let page_size = self.history.as_ref().map(|history| history.page_size);
        if page_size == Some(0) {
            return Some("The page size must be at least 1.".to_owned());
        }
        let period = self.stream.as_ref().map(|stream| stream.keep_alive_every);
        if period == Some(0) {
            return Some("The keep-alive period must be at least 1 second.".to_owned());
        }
        for key in &self.keys {
            if key.is_empty() {
                return Some("Every key needs a name.".to_owned());
            }
            let plain = |byte: u8| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_');
            if !key.bytes().all(plain) {
                return Some(format!(
                    "The key name \"{key}\" may hold only lower-case letters, digits and _."
                ));
            }
            if PLACEHOLDERS.contains(&key.as_str()) {
                return Some(format!(
                    "The key name \"{key}\" is the name of a placeholder."
                ));
            }
        }
        None
    }

    /// What `{interval}` becomes for `timeframe`, or `None` when the table has no row for it.
    pub(crate) fn service_interval(&self, timeframe: Timeframe) -> Option<&str> {
        let row = self.intervals.iter().find(|row| row.chart == timeframe)?;
        Some(&row.service)
    }
}

/// `template` with every `{name}` of `values` replaced by its value, in one pass. Braces around
/// any other text stay as typed, so a JSON body or message needs no escaping, and a value is
/// never expanded again.
pub(crate) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    expand(template, values, |out, value| out.push_str(value))
}

/// As [`fill`], for a URL: every byte of a value is percent-encoded except ASCII letters,
/// digits, `-`, `.`, `_` and `~`.
pub(crate) fn fill_url(template: &str, values: &[(&str, &str)]) -> String {
    expand(template, values, |out, value| {
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                out.push(char::from(byte));
            } else {
                // Writing into a `String` cannot fail.
                let _ = write!(out, "%{byte:02X}");
            }
        }
    })
}

/// `template` with every `{name}` of `values` replaced by what `write` appends for its value.
fn expand(template: &str, values: &[(&str, &str)], write: impl Fn(&mut String, &str)) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let named = values.iter().find_map(|&(name, value)| {
            let tail = after.strip_prefix(name)?.strip_prefix('}')?;
            // `{}` is an empty JSON object, never a name.
            (!name.is_empty()).then_some((value, tail))
        });
        match named {
            Some((value, tail)) => {
                write(&mut out, value);
                rest = tail;
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The value at `path` below `value`: a key steps into an object and an index into an array.
/// `None` when a step does not exist.
pub(crate) fn at<'a>(value: &'a Value, path: &[Segment]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, segment| match segment {
        Segment::Key(key) => value.get(key),
        Segment::Index(index) => value.get(index),
    })
}

/// A finite number: a JSON number, or text that holds one, as services that send numbers as
/// strings do. Spaces around the text are allowed. `NaN`, infinities, empty text and every
/// other JSON type give `None`.
pub(crate) fn number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

/// The text a routing field or a trade condition is compared by: a string as it is, a number
/// or a boolean as JSON prints it. `None` for null, arrays and objects.
pub(crate) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        _ => None,
    }
}
