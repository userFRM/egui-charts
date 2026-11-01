use super::{FeedCapabilities, FeedError, HistoryPage, HistoryRequest};
use crate::model::{
    Bar, DataStatus, Instrument, Session, SessionWindow, SymbolType, Timeframe, TimezoneId,
};
use chrono::{DateTime, NaiveTime, TimeDelta, Utc, Weekday};
use serde::Deserialize;
use std::fmt::Write as _;

fn invalid(message: impl ToString) -> FeedError {
    FeedError::InvalidData(message.to_string())
}

pub(super) fn url(base: &str, path: &str, params: &[(&str, String)]) -> String {
    let mut url = format!("{base}/{path}");
    for (index, (key, value)) in params.iter().enumerate() {
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(key);
        url.push('=');
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                url.push(char::from(byte));
            } else {
                write!(url, "%{byte:02X}").expect("writing to a String");
            }
        }
    }
    url
}

fn resolution(timeframe: Timeframe) -> Result<String, FeedError> {
    let text = timeframe.as_str();
    if let Some(minutes) = text.strip_suffix("min") {
        Ok(minutes.to_owned())
    } else if let Some(hours) = text.strip_suffix('h') {
        Ok((hours.parse::<u64>().map_err(invalid)? * 60).to_string())
    } else if text.ends_with("ms") || timeframe.is_trade_based() {
        Err(FeedError::NotSupported)
    } else {
        Ok(text.to_uppercase())
    }
}

fn timeframe(text: &str) -> Result<Timeframe, FeedError> {
    if let Some(timeframe) = Timeframe::from_resolution(text) {
        Ok(timeframe)
    } else {
        text.parse().map_err(invalid)
    }
}

fn time_param(time: DateTime<Utc>) -> String {
    let seconds = time.timestamp();
    let nanos = time.timestamp_subsec_nanos();
    if nanos == 0 {
        seconds.to_string()
    } else if seconds < 0 {
        format!("-{}.{:09}", -(seconds + 1), 1_000_000_000 - nanos)
    } else {
        format!("{seconds}.{nanos:09}")
    }
}

pub(super) fn history_url(base: &str, request: &HistoryRequest) -> Result<String, FeedError> {
    let mut params = vec![
        ("symbol", request.symbol.clone()),
        ("resolution", resolution(request.timeframe)?),
        ("countback", request.count.to_string()),
        ("extended_hours", request.extended_hours.to_string()),
    ];
    if let Some(before) = request.before {
        params.push(("to", time_param(before)));
    }
    if let Some(since) = request.since {
        params.push(("from", time_param(since)));
    }
    if let Some(conversion) = &request.conversion {
        params.push(("conversion", conversion.clone()));
    }
    Ok(url(base, "history", &params))
}

#[derive(Deserialize)]
pub(super) struct Config {
    supported_resolutions: Option<Vec<String>>,
    exchanges: Option<Vec<FilterValue>>,
    symbols_types: Option<Vec<FilterValue>>,
}

#[derive(Deserialize)]
struct FilterValue {
    value: String,
}

impl Config {
    pub(super) fn into_capabilities(self) -> Result<FeedCapabilities, FeedError> {
        let mut capabilities = FeedCapabilities::new().with_services([]);
        capabilities.timeframes = self
            .supported_resolutions
            .map(|values| values.iter().map(|text| timeframe(text)).collect())
            .transpose()?;
        capabilities.exchanges = self.exchanges.map(|values| {
            values
                .into_iter()
                .map(|exchange| exchange.value)
                .filter(|value| !value.is_empty())
                .collect()
        });
        capabilities.symbol_types = self.symbols_types.map(|values| {
            values
                .into_iter()
                .filter_map(|kind| symbol_type(&kind.value))
                .collect()
        });
        Ok(capabilities)
    }
}

#[derive(Deserialize)]
pub(super) struct Symbol {
    #[serde(alias = "symbol")]
    name: String,
    ticker: Option<String>,
    description: Option<String>,
    exchange: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    minmov: Option<f64>,
    pricescale: Option<f64>,
    volume_precision: Option<u8>,
    currency_code: Option<String>,
    timezone: Option<TimezoneId>,
    session: Option<String>,
    supported_resolutions: Option<Vec<String>>,
    data_status: Option<String>,
}

impl Symbol {
    pub(super) fn into_instrument(self) -> Result<Instrument, FeedError> {
        let mut instrument = Instrument::new(self.ticker.unwrap_or(self.name));
        if instrument.symbol.is_empty() {
            return Err(invalid("empty symbol"));
        }
        instrument.description = self.description;
        instrument.exchange = self.exchange;
        instrument.symbol_type = self.kind.as_deref().and_then(symbol_type);
        if let Some(scale) = self.pricescale {
            let movement = self.minmov.unwrap_or(1.0);
            let tick = movement / scale;
            if !scale.is_finite() || scale <= 0.0 || !tick.is_finite() || tick <= 0.0 {
                return Err(invalid("minmov and pricescale must be positive and finite"));
            }
            instrument.tick_size = Some(tick);
        }
        instrument.volume_decimals = self.volume_precision;
        instrument.currency = self.currency_code;
        instrument.time_zone = self.timezone;
        instrument.session = self.session.as_deref().map(session).transpose()?;
        instrument.supported_timeframes = self
            .supported_resolutions
            .map(|values| values.iter().map(|text| timeframe(text)).collect())
            .transpose()?;
        instrument.data_status = match self.data_status.as_deref() {
            Some("streaming") => Some(DataStatus::Realtime),
            Some("delayed_streaming") => Some(DataStatus::Delayed),
            Some("endofday") => Some(DataStatus::EndOfDay),
            _ => None,
        };
        Ok(instrument)
    }
}

fn symbol_type(text: &str) -> Option<SymbolType> {
    // The model's serialized names capitalize only the first letter.
    let mut text = text.to_ascii_lowercase();
    text.get_mut(..1)?.make_ascii_uppercase();
    serde_json::from_value(serde_json::Value::String(text)).ok()
}

fn session(text: &str) -> Result<Session, FeedError> {
    if text == "24x7" {
        return Ok(Session::new([]));
    }
    let time = |value: &str| {
        if value == "2400" {
            Ok(NaiveTime::MIN)
        } else {
            NaiveTime::parse_from_str(value, "%H%M").map_err(invalid)
        }
    };
    let (hours, days) = text.rsplit_once(':').unwrap_or((text, "23456"));
    let days = days
        .bytes()
        .map(|day| match day {
            b'1' => Ok(Weekday::Sun),
            b'2' => Ok(Weekday::Mon),
            b'3' => Ok(Weekday::Tue),
            b'4' => Ok(Weekday::Wed),
            b'5' => Ok(Weekday::Thu),
            b'6' => Ok(Weekday::Fri),
            b'7' => Ok(Weekday::Sat),
            _ => Err(invalid("invalid session day")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if days.is_empty() {
        return Err(invalid("empty session days"));
    }
    let mut windows = Vec::new();
    for window in hours.split(',') {
        let (open, close) = window
            .split_once('-')
            .ok_or_else(|| invalid("invalid session hours"))?;
        let mut days = days.clone();
        // A midnight end closes on the day after this format's trading day.
        if close == "2400" || (open == "0000" && close == "0000") {
            days.iter_mut().for_each(|day| *day = day.succ());
        }
        windows.push(SessionWindow::new(days, time(open)?, time(close)?));
    }
    Ok(Session::new(windows))
}

#[derive(Deserialize)]
pub(super) struct Columns {
    s: String,
    #[serde(default)]
    t: Vec<i64>,
    #[serde(default)]
    o: Vec<f64>,
    #[serde(default)]
    h: Vec<f64>,
    #[serde(default)]
    l: Vec<f64>,
    #[serde(default)]
    c: Vec<f64>,
    v: Option<Vec<f64>>,
    #[serde(rename = "nextTime")]
    next_time: Option<i64>,
    #[serde(default)]
    end_of_history: bool,
    errmsg: Option<String>,
}

impl Columns {
    pub(super) fn into_page(self, request: &HistoryRequest) -> Result<HistoryPage, FeedError> {
        if !matches!(self.s.as_str(), "ok" | "no_data") {
            return Err(invalid(self.errmsg.unwrap_or(self.s)));
        }
        let count = self.t.len();
        if [self.o.len(), self.h.len(), self.l.len(), self.c.len()]
            .iter()
            .any(|&length| length != count)
            || self.v.as_ref().is_some_and(|values| values.len() != count)
            || (self.s == "no_data" && count != 0)
        {
            return Err(invalid(
                "history columns have different lengths or contradict no_data",
            ));
        }
        let mut bars = Vec::with_capacity(count.min(request.count));
        let mut end_of_history = self.end_of_history;
        for (index, stamp) in self.t.into_iter().enumerate() {
            let time = DateTime::from_timestamp(stamp, 0)
                .ok_or_else(|| invalid("invalid history timestamp"))?;
            let bar = Bar::new(
                time,
                self.o[index],
                self.h[index],
                self.l[index],
                self.c[index],
                self.v.as_ref().map_or(0.0, |values| values[index]),
            );
            if ![bar.open, bar.high, bar.low, bar.close, bar.volume]
                .iter()
                .all(|value| value.is_finite())
                || bar.low > bar.open.min(bar.close)
                || bar.high < bar.open.max(bar.close)
                || bar.volume < 0.0
            {
                return Err(invalid("invalid history bar"));
            }
            if request.since.is_some_and(|since| time < since) {
                end_of_history = false;
            } else if request.before.is_none_or(|before| time < before) {
                bars.push(bar);
            }
        }
        bars.sort_by_key(|bar| bar.time);
        bars.dedup_by_key(|bar| bar.time);
        if bars.len() > request.count {
            end_of_history = false;
            bars.drain(..bars.len() - request.count);
        }
        let continuation = self
            .next_time
            .map(|stamp| {
                let next = DateTime::from_timestamp(stamp, 0)
                    .ok_or_else(|| invalid("invalid history continuation"))?;
                // Include the named second without rounding it away in a server's numeric parser.
                let step = request
                    .before
                    .filter(|_| bars.is_empty())
                    .map_or(TimeDelta::seconds(1), |before| {
                        TimeDelta::seconds(1).min((before - next) / 2)
                    });
                if step <= TimeDelta::zero() {
                    return Err(invalid(
                        "history continuation must precede the requested end",
                    ));
                }
                next.checked_add_signed(step)
                    .ok_or_else(|| invalid("invalid history continuation"))
            })
            .transpose()?;
        let mut page = HistoryPage::new(
            bars,
            end_of_history || (self.s == "no_data" && continuation.is_none()),
        );
        page.continue_before = continuation;
        Ok(page)
    }
}
