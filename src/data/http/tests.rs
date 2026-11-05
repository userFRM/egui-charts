use super::*;
use crate::data::inbox::Inbox;
use crate::model::{Bar, BarClock, SymbolType, Timeframe};
#[cfg(test)]
use crate::services::{SymbolQuery, SymbolSearch};
use chrono::{DateTime, Utc};
use serde_json::json;

#[cfg(all(target_arch = "wasm32", not(feature = "widget")))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

fn request() -> HistoryRequest {
    HistoryRequest {
        symbol: "A&B/é".into(),
        timeframe: Timeframe::Min15,
        clock: BarClock::new(Timeframe::Min15),
        before: None,
        since: None,
        count: 2,
        extended_hours: false,
        adjusted: false,
        conversion: None,
    }
}

fn response(status: u16, body: &str, retry: Option<&str>) -> ehttp::Response {
    ehttp::Response {
        url: String::new(),
        ok: status == 200,
        status,
        status_text: String::new(),
        headers: ehttp::Headers::new(
            &retry
                .map(|value| ("Retry-After", value))
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        bytes: body.as_bytes().to_vec(),
    }
}

#[test]
fn column_history_fixtures_preserve_rows_gaps_and_errors() {
    let at = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
    let body = json!({"s":"ok", "t":[0,60], "o":[10,11], "h":[12,13],
        "l":[9,10], "c":[11,12], "v":[100,200]});
    let bars = vec![
        Bar::new(at(0), 10.0, 12.0, 9.0, 11.0, 100.0),
        Bar::new(at(60), 11.0, 13.0, 10.0, 12.0, 200.0),
    ];
    let mut cases = vec![
        (
            200,
            body.to_string(),
            None,
            Ok(HistoryPage::new(bars.clone(), false)),
        ),
        (
            // A row closing above its high reaches the chart, which drops and counts it; a row
            // whose timestamp is out of range is dropped. Neither fails the other rows.
            200,
            json!({"s":"ok", "t":[0,60,i64::MAX], "o":[10,11,12], "h":[12,13,14],
                "l":[9,10,11], "c":[11,13.5,12], "v":[100,200,300]})
            .to_string(),
            None,
            Ok(HistoryPage::new(
                vec![
                    bars[0].clone(),
                    Bar::new(at(60), 11.0, 13.0, 10.0, 13.5, 200.0),
                ],
                false,
            )),
        ),
        (
            // Nor does an out-of-range row stop the rows after it.
            200,
            json!({"s":"ok", "t":[i64::MAX,0,60], "o":[12,10,11], "h":[14,12,13],
                "l":[11,9,10], "c":[12,11,12], "v":[300,100,200]})
            .to_string(),
            None,
            Ok(HistoryPage::new(bars.clone(), false)),
        ),
        (
            200,
            r#"{"s":"no_data"}"#.into(),
            None,
            Ok(HistoryPage::new(vec![], true)),
        ),
        (
            200,
            r#"{"s":"no_data","nextTime":-60}"#.into(),
            None,
            Ok(HistoryPage::new(vec![], false)
                .with_continue_before(DateTime::from_timestamp(-59, 0).unwrap())),
        ),
        (
            200,
            r#"{"s":"ok","t":[],"o":[],"h":[],"l":[],"c":[]}"#.into(),
            None,
            Ok(HistoryPage::new(vec![], false)),
        ),
        (
            429,
            String::new(),
            Some(" 2"),
            Err(FeedError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
            }),
        ),
        (
            503,
            String::new(),
            None,
            Err(FeedError::Network("HTTP 503".into())),
        ),
        (
            408,
            String::new(),
            None,
            Err(FeedError::Network("HTTP 408".into())),
        ),
        (
            404,
            String::new(),
            None,
            Err(FeedError::SymbolNotFound(request().symbol)),
        ),
        (
            400,
            String::new(),
            None,
            Err(FeedError::InvalidData("HTTP 400".into())),
        ),
        (
            200,
            r#"{"s":"error","errmsg":"bad interval"}"#.into(),
            None,
            Err(FeedError::InvalidData("bad interval".into())),
        ),
    ];
    for column in ["t", "o", "h", "l", "c", "v"] {
        let mut mismatch = body.clone();
        mismatch[column].as_array_mut().unwrap().pop();
        cases.push((
            200,
            mismatch.to_string(),
            None,
            Err(FeedError::InvalidData(
                "history columns have different lengths or contradict no_data".into(),
            )),
        ));
    }
    for (status, body, retry, expected) in cases {
        let result = decode::<Columns>(&request().symbol, response(status, &body, retry))
            .and_then(|columns| columns.into_page(&request()));
        assert_eq!(result, expected, "HTTP {status}, {body}");
    }
    for (since, before, count, expected, end) in [
        (None, None, 1, &bars[1..], false),
        (None, Some(at(60)), 2, &bars[..1], true),
        (Some(at(60)), None, 2, &bars[1..], false),
        (Some(at(1)), Some(at(60)), 2, &bars[0..0], false),
    ] {
        let mut request = request();
        request.since = since;
        request.before = before;
        request.count = count;
        let mut complete = body.clone();
        complete["end_of_history"] = json!(true);
        let page = serde_json::from_value::<Columns>(complete)
            .unwrap()
            .into_page(&request)
            .unwrap();
        assert_eq!(page.bars, expected);
        assert_eq!(page.end_of_history, end);
    }
    let mut adjacent = request();
    adjacent.before = Some(at(-59));
    let continued = serde_json::from_value::<Columns>(json!({"s":"no_data","nextTime":-60}))
        .unwrap()
        .into_page(&adjacent)
        .unwrap();
    assert_eq!(
        continued.continue_before,
        DateTime::from_timestamp(-60, 500_000_000)
    );
    let mut request = request();
    request.before = Some(at(60));
    assert!(matches!(
        serde_json::from_value::<Columns>(json!({"s":"no_data", "nextTime":60}))
            .unwrap()
            .into_page(&request),
        Err(FeedError::InvalidData(_))
    ));
}

/// A bar of a day or longer is dated by the UTC date of its timestamp and lands in the period of
/// the chart's clock holding that date, in history, in live refresh and in the bounds sent.
#[cfg(feature = "timezones")]
#[test]
fn calendar_bars_keep_their_utc_trading_date_in_every_chart_time_zone() {
    use crate::model::TimezoneId;
    use chrono::TimeZone as _;
    const NEW_YORK: &str = "America/New_York";
    const TOKYO: &str = "Asia/Tokyo";
    let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
    let local = |year, month, day| {
        chrono::Local
            .with_ymd_and_hms(year, month, day, 0, 0, 0)
            .earliest()
            .unwrap()
            .to_utc()
    };
    // New York is on EDT (UTC-4) through October 2026; Tokyo is UTC+9 all year. 2026-10-19 is a
    // Monday; the two-week periods counted from Monday 1970-01-05 begin on 2026-10-12 and 10-26.
    for (timeframe, zone, stamp, open) in [
        (
            "1D",
            "UTC",
            "2026-10-19T00:00:00Z",
            at("2026-10-19T00:00:00Z"),
        ),
        (
            "1D",
            NEW_YORK,
            "2026-10-19T00:00:00Z",
            at("2026-10-19T04:00:00Z"),
        ),
        (
            "1W",
            NEW_YORK,
            "2026-10-19T00:00:00Z",
            at("2026-10-19T04:00:00Z"),
        ),
        (
            "1M",
            NEW_YORK,
            "2026-10-01T00:00:00Z",
            at("2026-10-01T04:00:00Z"),
        ),
        (
            "2W",
            NEW_YORK,
            "2026-10-26T00:00:00Z",
            at("2026-10-26T04:00:00Z"),
        ),
        (
            "3M",
            NEW_YORK,
            "2026-10-01T00:00:00Z",
            at("2026-10-01T04:00:00Z"),
        ),
        // A session open inside the trading date stays on it.
        (
            "1D",
            NEW_YORK,
            "2026-10-19T13:30:00Z",
            at("2026-10-19T04:00:00Z"),
        ),
        (
            "1D",
            TOKYO,
            "2026-10-19T00:00:00Z",
            at("2026-10-18T15:00:00Z"),
        ),
        (
            "1W",
            TOKYO,
            "2026-10-19T00:00:00Z",
            at("2026-10-18T15:00:00Z"),
        ),
        (
            "1M",
            TOKYO,
            "2026-10-01T00:00:00Z",
            at("2026-09-30T15:00:00Z"),
        ),
        (
            "1D",
            TOKYO,
            "2026-10-19T13:30:00Z",
            at("2026-10-18T15:00:00Z"),
        ),
        ("1D", "local", "2026-10-19T00:00:00Z", local(2026, 10, 19)),
        ("1W", "local", "2026-10-19T00:00:00Z", local(2026, 10, 19)),
        ("1M", "local", "2026-10-01T00:00:00Z", local(2026, 10, 1)),
    ] {
        let case = format!("{timeframe} {zone} {stamp}");
        let mut request = request();
        request.timeframe = timeframe.parse().unwrap();
        request.clock = match zone {
            "local" => BarClock::new(request.timeframe).with_local_timezone(),
            zone => {
                BarClock::new(request.timeframe).with_timezone(TimezoneId::named(zone).unwrap())
            }
        };
        let stamp = at(stamp);
        let body = json!({"s":"ok", "t":[stamp.timestamp()], "o":[1], "h":[1], "l":[1], "c":[1]});
        let page = serde_json::from_value::<Columns>(body)
            .unwrap()
            .into_page(&request)
            .unwrap();
        let time = page.bars[0].time;
        assert_eq!(request.clock.bar_open(time), open, "{case}");
        assert_eq!(time.date_naive(), stamp.date_naive(), "{case}");
        // The live refresh asks from the bar it holds, scroll-back for the dates before it.
        let midnight = stamp
            .date_naive()
            .and_time(chrono::NaiveTime::MIN)
            .and_utc();
        request.since = Some(time);
        request.before = Some(time);
        let target = history_url("https://example.test", &request).unwrap();
        let bound = midnight.timestamp();
        assert!(
            target.contains(&format!("&to={bound}&from={bound}")),
            "{case}: {target}"
        );
        // A continuation that names the bar asks for its date and the dates before it.
        request.since = None;
        request.before = Some(at("2027-01-01T00:00:00Z"));
        let continued =
            serde_json::from_value::<Columns>(json!({"s":"no_data", "nextTime":stamp.timestamp()}))
                .unwrap()
                .into_page(&request)
                .unwrap();
        request.before = continued.continue_before;
        let target = history_url("https://example.test", &request).unwrap();
        assert!(
            target.ends_with(&format!("&to={}", bound + 86_400)),
            "{case}: {target}"
        );
    }
}

/// The periods of an interval that is not whole days, such as 36 hours, do not open at midnight,
/// so its bars and bounds keep their timestamps.
#[test]
fn bars_of_partial_day_intervals_keep_their_timestamps() {
    let at = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
    let mut request = request();
    request.timeframe = "36h".parse().unwrap();
    request.clock = BarClock::new(request.timeframe);
    // Periods of 36 hours from 1970-01-01 open at 00:00 and 12:00 UTC in turn.
    let page = serde_json::from_value::<Columns>(json!({"s":"ok", "t":[129_600, 259_200],
        "o":[1,1], "h":[1,1], "l":[1,1], "c":[1,1]}))
    .unwrap()
    .into_page(&request)
    .unwrap();
    assert_eq!(
        page.bars.iter().map(|bar| bar.time).collect::<Vec<_>>(),
        [at(129_600), at(259_200)]
    );
    request.since = Some(at(129_600));
    request.before = Some(at(388_800));
    let target = history_url("https://example.test", &request).unwrap();
    assert!(target.ends_with("&to=388800&from=129600"), "{target}");
}

#[test]
fn endpoint_requests_preserve_interval_bounds_and_search_filters() {
    let mut request = request();
    request.before = DateTime::from_timestamp(90, 500_000_000);
    request.since = DateTime::from_timestamp(-1, 500_000_000);
    request.extended_hours = true;
    request.conversion = Some("USD & EUR".into());
    assert_eq!(
        history_url("https://example.test/data", &request).unwrap(),
        "https://example.test/data/history?symbol=A%26B%2F%C3%A9&resolution=15&countback=2&extended_hours=true&to=90.500000000&from=-0.500000000&conversion=USD%20%26%20EUR"
    );
    for (timeframe, expected) in [
        (Timeframe::Sec1, "1S"),
        (Timeframe::Min1, "1"),
        (Timeframe::Hour4, "240"),
        (Timeframe::Day1, "1D"),
        (Timeframe::Week1, "1W"),
        (Timeframe::Month1, "1M"),
        ("45min".parse().unwrap(), "45"),
        ("3M".parse().unwrap(), "3M"),
    ] {
        request.timeframe = timeframe;
        assert!(
            history_url("https://example.test", &request)
                .unwrap()
                .contains(&format!("&resolution={expected}&"))
        );
    }

    for (hours, expected_days) in [
        (
            "0930-1600",
            vec![
                chrono::Weekday::Mon,
                chrono::Weekday::Tue,
                chrono::Weekday::Wed,
                chrono::Weekday::Thu,
                chrono::Weekday::Fri,
            ],
        ),
        ("0000-0000:2", vec![chrono::Weekday::Tue]),
        ("0930-2400:2", vec![chrono::Weekday::Tue]),
        ("1700-0930:2", vec![chrono::Weekday::Mon]),
        ("0900-1200,1300-1700:2", vec![chrono::Weekday::Mon]),
    ] {
        let info = serde_json::from_value::<Symbol>(json!({"name":"ABC", "session":hours}))
            .unwrap()
            .into_instrument()
            .unwrap();
        for window in info.session.unwrap().windows {
            assert_eq!(window.days, expected_days, "{hours}");
        }
    }
    for invalid in [
        json!({"name":"ABC", "pricescale":0}),
        json!({"name":"ABC", "minmov":-1, "pricescale":100}),
        json!({"name":"ABC", "session":"0930-1600:8"}),
        json!({"name":"ABC", "session":"0930-1600F"}),
    ] {
        assert!(matches!(
            serde_json::from_value::<Symbol>(invalid)
                .unwrap()
                .into_instrument(),
            Err(FeedError::InvalidData(_))
        ));
    }

    // Exercises real asynchronous HTTP and Reply delivery, including a root path prefix.
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/data/", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut targets = Vec::new();
        for body in [
            r#"{"supported_resolutions":["1","15","60","240","1D"],"exchanges":[{"value":"X"}],"symbols_types":[{"value":"stock"}]}"#,
            r#"{"name":"ABC","ticker":"X:ABC","description":"ABC Inc","exchange":"X","type":"stock","minmov":5,"pricescale":100,"currency_code":"USD","volume_precision":2,"timezone":"UTC","session":"0930-1600:23456","supported_resolutions":["15"],"data_status":"streaming"}"#,
            r#"[{"symbol":"X:BAD","timezone":"Mars/Olympus_Mons"},{"symbol":"X:ABC","description":"ABC Inc","type":"stock","exchange":"X","country":"GB","sector":"Energy","source":"Provider/A","root":"ABC","logo":"bytes://company","exchange_logo":"bytes://venue"}]"#,
            r#"{"s":"no_data","nextTime":-60}"#,
        ] {
            let (mut connection, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&connection);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            targets.push(line.split_whitespace().nth(1).unwrap().to_owned());
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
            }
            write!(
                connection,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        targets
    });
    let feed = HttpDatafeed::new(base);
    let inbox = Arc::new(Inbox::default());
    let (sender, receiver) = std::sync::mpsc::channel();
    inbox.set_waker(move || {
        let _ = sender.send(());
    });
    let take = || {
        receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        inbox.drain().answers.pop().unwrap().1
    };
    feed.capabilities(inbox.reply(0));
    let capabilities = *take()
        .downcast::<Result<FeedCapabilities, FeedError>>()
        .unwrap();
    assert_eq!(
        capabilities.unwrap(),
        FeedCapabilities::new()
            .with_timeframes([
                Timeframe::Min1,
                Timeframe::Min15,
                Timeframe::Hour1,
                Timeframe::Hour4,
                Timeframe::Day1
            ])
            .with_exchanges(["X".into()])
            .with_symbol_types([SymbolType::Stock])
            .with_services([])
    );
    feed.resolve("X:ABC", inbox.reply(1));
    let instrument = (*take().downcast::<Result<Instrument, FeedError>>().unwrap()).unwrap();
    assert_eq!(instrument.symbol, "X:ABC");
    assert_eq!(instrument.tick_size, Some(0.05)); // Five hundredths per price tick.
    assert_eq!(instrument.currency.as_deref(), Some("USD"));
    assert_eq!(instrument.volume_decimals, Some(2));
    assert_eq!(
        instrument.supported_timeframes,
        Some(vec![Timeframe::Min15])
    );
    let session = instrument.session.unwrap();
    assert_eq!(
        session.windows[0].days,
        [
            chrono::Weekday::Mon,
            chrono::Weekday::Tue,
            chrono::Weekday::Wed,
            chrono::Weekday::Thu,
            chrono::Weekday::Fri
        ]
    );
    assert_eq!(
        session.windows[0].open,
        chrono::NaiveTime::from_hms_opt(9, 30, 0).unwrap()
    );
    let mut query = SymbolQuery::new("X:A & B");
    query.symbol_type = Some(SymbolType::Stock);
    query.exchange = Some("X/Y".into());
    query.country = Some("GB".into());
    query.sector = Some("Energy & utilities".into());
    query.source = Some("Provider/A".into());
    query.offset = 7;
    query.limit = 10;
    feed.search(query, inbox.reply(2));
    let found = (*take()
        .downcast::<Result<Vec<Instrument>, FeedError>>()
        .unwrap())
    .unwrap();
    // A result with a time zone this build does not know is left out.
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].symbol, "X:ABC");
    assert_eq!(found[0].country.as_deref(), Some("GB"));
    assert_eq!(found[0].sector.as_deref(), Some("Energy"));
    assert_eq!(found[0].source.as_deref(), Some("Provider/A"));
    assert_eq!(found[0].root.as_deref(), Some("ABC"));
    assert_eq!(found[0].logo.as_deref(), Some("bytes://company"));
    assert_eq!(found[0].exchange_logo.as_deref(), Some("bytes://venue"));
    feed.history(request, inbox.reply(3));
    let page = (*take().downcast::<Result<HistoryPage, FeedError>>().unwrap()).unwrap();
    assert_eq!(page.continue_before, DateTime::from_timestamp(-59, 0));
    let targets = server.join().unwrap();
    assert_eq!(
        &targets[..3],
        [
            "/data/config",
            "/data/symbols?symbol=X%3AABC",
            "/data/search?query=X%3AA%20%26%20B&type=stock&exchange=X%2FY&offset=7&limit=10&country=GB&sector=Energy%20%26%20utilities&source=Provider%2FA"
        ]
    );
    assert!(targets[3].contains("&resolution=3M&countback=2&"));
}

#[test]
fn old_subscription_cannot_publish_or_schedule_after_cancellation() {
    let feed = HttpDatafeed::new("https://example.test");
    let inbox = Arc::new(Inbox::default());
    let poll = || {
        let clock = BarClock::new(Timeframe::Min1);
        let sink = inbox.subscribe("ABC".into(), clock.clone(), 10, None);
        let cancelled = Arc::new(AtomicBool::new(false));
        feed.subscriptions
            .lock()
            .unwrap()
            .insert(sink.id, cancelled.clone());
        Poll {
            base: feed.base.clone(),
            subscription: Subscription {
                id: sink.id,
                symbol: "ABC".into(),
                timeframe: Timeframe::Min1,
                clock,
                extended_hours: false,
                conversion: None,
                sink,
            },
            cancelled,
            since: None,
            retry: REFRESH,
            live: false,
        }
    };
    let page = || HistoryPage::new(vec![Bar::new(Utc::now(), 1.0, 2.0, 0.0, 1.5, 10.0)], false);
    let old = poll();
    let current = poll();
    feed.unsubscribe(old.subscription.id);
    assert!(old.received(Ok(page())).is_none());
    let (current, delay) = current.received(Ok(page())).unwrap();
    assert_eq!(delay, REFRESH);
    let updates = inbox.drain().updates;
    assert_eq!(
        updates
            .iter()
            .map(|(_, update)| update.bars.len())
            .sum::<usize>(),
        1
    );
    let (current, delay) = current
        .received(Err(FeedError::RateLimited {
            retry_after: Some(Duration::from_secs(12)),
        }))
        .unwrap();
    assert_eq!(delay, Duration::from_secs(12));
    let (current, delay) = current
        .received(Err(FeedError::Network("offline".into())))
        .unwrap();
    assert_eq!(delay, Duration::from_secs(10));
    assert!(current.received(Err(FeedError::NotSupported)).is_none());
    let mut overflow = poll();
    overflow.since = Some(DateTime::from_timestamp(0, 0).unwrap());
    let bars = (1..=500)
        .map(|seconds| {
            Bar::new(
                DateTime::from_timestamp(seconds, 0).unwrap(),
                1.0,
                1.0,
                1.0,
                1.0,
                1.0,
            )
        })
        .collect();
    assert!(
        overflow
            .received(Ok(HistoryPage::new(bars, false)))
            .is_none()
    );
    assert!(inbox.drain().updates.iter().any(|(_, update)| matches!(&update.error,
        Some(FeedError::InvalidData(message)) if message == "live history window exceeded; reload history")));
    // A long Retry-After must not keep an unsubscribed native worker alive.
    struct Released(std::sync::mpsc::Sender<bool>);
    impl Drop for Released {
        fn drop(&mut self) {
            let _ = self.0.send(false);
        }
    }
    let (released, stopped) = std::sync::mpsc::channel();
    let release = Released(released);
    let waiting = poll();
    later(
        Duration::from_secs(300),
        waiting.cancelled.clone(),
        Box::new(move || {
            let _ = release.0.send(true);
        }),
    );
    feed.unsubscribe(waiting.subscription.id);
    assert!(!stopped.recv_timeout(Duration::from_secs(5)).unwrap());
    let dropped = poll();
    drop(feed);
    assert!(dropped.received(Ok(page())).is_none());
}

/// A local server that accepts every connection and never answers, as a stalled server or a
/// connection lost without a reset does.
fn stalled_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for connection in listener.incoming() {
            held.push(connection);
        }
    });
    base
}

#[test]
fn stalled_requests_answer_a_network_error() {
    let feed = HttpDatafeed::new(stalled_server());
    let inbox = Arc::new(Inbox::default());
    let clock = BarClock::new(Timeframe::Min1);
    let sink = inbox.subscribe("ABC".into(), clock.clone(), 10, None);
    feed.subscribe(Subscription {
        id: sink.id,
        symbol: "ABC".into(),
        timeframe: Timeframe::Min1,
        clock,
        extended_hours: false,
        conversion: None,
        sink,
    });
    feed.history(request(), inbox.reply(0));
    let start = std::time::Instant::now();
    let (mut answer, mut live) = (None, None);
    while start.elapsed() < Duration::from_secs(30) && (answer.is_none() || live.is_none()) {
        std::thread::sleep(Duration::from_millis(100));
        let drained = inbox.drain();
        for (_, reply) in drained.answers {
            answer = Some(*reply.downcast::<Result<HistoryPage, FeedError>>().unwrap());
        }
        live = drained
            .updates
            .into_iter()
            .find_map(|(_, update)| update.error)
            .or(live);
    }
    assert!(
        matches!(answer, Some(Err(FeedError::Network(_))))
            && matches!(live, Some(FeedError::Network(_))),
        "after {:?}: history {answer:?}, live {live:?}",
        start.elapsed()
    );
}

#[test]
fn only_connection_attempts_report_connecting() {
    let base = stalled_server();
    let inbox = Arc::new(Inbox::default());
    let cancelled = Arc::new(AtomicBool::new(false));
    let poll = || {
        let clock = BarClock::new(Timeframe::Min1);
        let sink = inbox.subscribe("ABC".into(), clock.clone(), 10, None);
        Poll {
            base: base.clone(),
            subscription: Subscription {
                id: sink.id,
                symbol: "ABC".into(),
                timeframe: Timeframe::Min1,
                clock,
                extended_hours: false,
                conversion: None,
                sink,
            },
            cancelled: cancelled.clone(),
            since: None,
            retry: REFRESH,
            live: false,
        }
    };
    let page = HistoryPage::new(vec![Bar::new(Utc::now(), 1.0, 2.0, 0.5, 1.5, 10.0)], false);
    let refresh = poll().received(Ok(page)).unwrap().0;
    let retry = poll()
        .received(Err(FeedError::Network("offline".into())))
        .unwrap()
        .0;
    inbox.drain();
    let mut statuses = Vec::new();
    for poll in [poll(), refresh, retry] {
        poll.fetch();
        statuses.push(
            inbox
                .drain()
                .updates
                .into_iter()
                .find_map(|(_, update)| update.status),
        );
    }
    cancelled.store(true, Ordering::Release);
    // The first request and a retry connect; the refresh of a live subscription stays live.
    assert_eq!(
        statuses,
        [
            Some(FeedStatus::Connecting),
            None,
            Some(FeedStatus::Connecting)
        ]
    );
}

/// A cancelled browser retry releases its captured state without waiting for Retry-After.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
async fn cancelled_browser_retry_releases_its_callback() {
    use wasm_bindgen::{JsCast as _, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, js_sys::Promise};

    let cancelled = Arc::new(AtomicBool::new(false));
    let held = Arc::new(());
    let captured = held.clone();
    later(
        Duration::from_secs(300),
        cancelled.clone(),
        Box::new(move || {
            drop(captured);
            panic!("a cancelled retry must not run");
        }),
    );
    cancelled.store(true, Ordering::Release);
    let wait = Promise::new(&mut |resolve, _| {
        let done = Closure::once_into_js(move || {
            resolve.call0(&wasm_bindgen::JsValue::NULL).unwrap();
        });
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(done.unchecked_ref(), 1500)
            .unwrap();
    });
    JsFuture::from(wait).await.unwrap();
    assert_eq!(Arc::strong_count(&held), 1);
}
