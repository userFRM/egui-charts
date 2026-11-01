use super::*;
use crate::data::inbox::Inbox;
use crate::model::{Bar, BarClock, SymbolType, Timeframe};
use chrono::{DateTime, Utc};
use serde_json::json;

fn request() -> HistoryRequest {
    HistoryRequest {
        symbol: "A&B/é".into(),
        timeframe: Timeframe::Min15,
        clock: BarClock::new(Timeframe::Min15),
        before: None,
        since: None,
        count: 2,
        extended_hours: false,
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
    for bad in [
        json!({"s":"no_data", "nextTime":60}),
        json!({"s":"ok", "t":[i64::MAX], "o":[1], "h":[1], "l":[1], "c":[1]}),
        json!({"s":"ok", "t":[0], "o":[2], "h":[1], "l":[1], "c":[1]}),
    ] {
        let mut request = request();
        request.before = Some(at(60));
        assert!(matches!(
            serde_json::from_value::<Columns>(bad)
                .unwrap()
                .into_page(&request),
            Err(FeedError::InvalidData(_))
        ));
    }
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
            r#"[{"symbol":"X:ABC","description":"ABC Inc","type":"stock"}]"#,
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
    feed.search_symbols(
        "A & B".into(),
        Some(SymbolType::Stock),
        Some("X/Y".into()),
        7,
        10,
        inbox.reply(2),
    );
    let found = (*take()
        .downcast::<Result<Vec<Instrument>, FeedError>>()
        .unwrap())
    .unwrap();
    assert_eq!(found[0].symbol, "X:ABC");
    feed.history(request, inbox.reply(3));
    let page = (*take().downcast::<Result<HistoryPage, FeedError>>().unwrap()).unwrap();
    assert_eq!(page.continue_before, DateTime::from_timestamp(-59, 0));
    let targets = server.join().unwrap();
    assert_eq!(
        &targets[..3],
        [
            "/data/config",
            "/data/symbols?symbol=X%3AABC",
            "/data/search?query=A%20%26%20B&type=stock&exchange=X%2FY&offset=7&limit=10"
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
