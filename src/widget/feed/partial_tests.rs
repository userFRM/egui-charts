use super::*;
use crate::data::{FootprintBars, FootprintLevel};

fn at(minute: i64) -> DateTime<Utc> {
    bars(1)[0].time + TimeDelta::minutes(minute)
}

fn recorded(range: std::ops::Range<i64>) -> Vec<Tick> {
    range
        .map(|minute| Tick::new(at(minute), 100.0, (minute * 2 + 3) as f64, None))
        .collect()
}

fn partial_page(shared: bool, range: std::ops::Range<i64>, slots: &[i64]) -> FootprintPage {
    let end = at(range.end);
    let FootprintBars::Owned(mut bars) = prepared_page(&recorded(range), end, true).bars else {
        unreachable!()
    };
    let bars = if shared {
        FootprintBars::Shared(Arc::new(bars))
    } else {
        bars.reverse(); // The public owned payload still permits arbitrary order.
        FootprintBars::Owned(bars)
    };
    FootprintPage::partial(bars, end, true, slots.iter().copied().map(at).collect())
}

fn deliver(feed: &Script, chart: &mut Chart, frames: &mut Frames, page: FootprintPage) {
    feed.footprints.lock().unwrap().remove(0).send(Ok(page));
    frames.run(chart);
    frames.run(chart);
}

#[test]
fn explicit_partial_slots_preserve_rows_but_exclude_only_marked_executions() {
    for shared in [false, true] {
        for slots in [&[1][..], &[0, 2][..]] {
            let (feed, mut chart, mut frames) = prepared_chart();
            #[cfg(feature = "studies")]
            chart.add_indicator(
                crate::studies::IndicatorFactory::new()
                    .create("Vol Profile FR")
                    .unwrap(),
            );
            frames.run(&mut chart);
            let page = partial_page(shared, 0..3, slots);
            assert!(!page.is_available());
            assert_eq!(
                page.partial_slots(),
                Some(slots.iter().copied().map(at).collect::<Vec<_>>().as_slice())
            );
            let rows: Vec<_> = page
                .bars
                .iter()
                .map(|bar| {
                    (
                        bar.time,
                        bar.levels.get(&100).unwrap() as *const FootprintLevel,
                    )
                })
                .collect();
            deliver(&feed, &mut chart, &mut frames, page);
            let view = prepared_view(&chart);
            for (time, row) in rows {
                let bar = view.bar_at(time).unwrap();
                assert_eq!(bar.levels.get(&100).unwrap() as *const FootprintLevel, row);
                assert_eq!((bar.bid, bar.ask), (0.0, 0.0));
                assert_eq!(bar.levels.get(&100).unwrap().unknown_trades, 1);
                assert_eq!(
                    view.complete_at(time),
                    !slots.iter().any(|minute| at(*minute) == time)
                );
            }
            let expected: Vec<_> = (0..3)
                .filter(|minute| !slots.contains(minute))
                .map(|minute| (at(minute), (minute * 2 + 3) as f64))
                .collect();
            assert_eq!(
                view.complete_bars(..)
                    .map(|bar| (bar.time, bar.unknown))
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                view.profile(at(0), at(2)).unknown,
                expected.iter().map(|(_, volume)| volume).sum::<f64>()
            );
            assert!(!view.span_is_complete(at(0), at(2)));
            for minute in 0..3 {
                assert_eq!(
                    view.span_is_complete(at(minute), at(minute)),
                    !slots.contains(&minute)
                );
            }
            assert!(
                footprint_requests(&feed).len() == 1,
                "sealed partial slots must not request repairs"
            );
            #[cfg(feature = "studies")]
            {
                let read = missing_history_input(&chart).footprint_snapshot(at(0));
                assert_eq!(
                    read.bars(..)
                        .map(|bar| (bar.time, bar.unknown))
                        .collect::<Vec<_>>(),
                    expected
                );
                assert!(!read.span_is_complete(at(0), at(2)));
            }
        }
    }
}

#[test]
fn invalid_partial_declarations_leave_admitted_rows_and_coverage_unchanged() {
    for shared in [false, true] {
        for case in 0..12 {
            let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
            resume_missing_history(&feed, &mut chart, &mut frames);
            let mut answer = partial_page(shared, 1..3, &[1]);
            match case {
                0 => answer.partial_slots = Some(vec![]),
                1 => answer.partial_slots = Some(vec![at(2), at(1)]),
                2 => answer.partial_slots = Some(vec![at(1), at(1)]),
                3 => answer.partial_slots = Some(vec![at(1) + TimeDelta::seconds(1)]),
                4 => answer.partial_slots = Some(vec![at(0)]),
                5 => answer.bars = FootprintBars::Owned(vec![]),
                6 => answer.complete_before += TimeDelta::seconds(1),
                7 => answer.bars = partial_page(shared, 0..3, &[1]).bars,
                8 => answer.bars = partial_page(shared, 2..3, &[2]).bars,
                9 => {
                    let FootprintBars::Owned(mut bars) = partial_page(false, 1..3, &[1]).bars
                    else {
                        unreachable!()
                    };
                    bars[0].unknown = f64::NAN;
                    answer.bars = if shared {
                        bars.reverse();
                        FootprintBars::Shared(Arc::new(bars))
                    } else {
                        FootprintBars::Owned(bars)
                    };
                }
                10 => answer.partial_slots = Some(vec![at(0), at(1), at(2)]),
                _ => answer.complete_before = at(2),
            }
            deliver(&feed, &mut chart, &mut frames, answer);
            assert!(
                matches!(chart.feed.last_error, Some(FeedError::InvalidData(_))),
                "shared={shared}, case={case}"
            );
            assert_missing_history_rows(&chart, owners, false);
        }
    }
}

#[test]
fn initial_partial_cutoff_must_close_every_supplied_slot() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let mut page = partial_page(true, 0..3, &[2]);
    page.complete_before = at(2) + TimeDelta::seconds(30);
    deliver(&feed, &mut chart, &mut frames, page);
    assert!(matches!(
        chart.feed.last_error,
        Some(FeedError::InvalidData(_))
    ));
    assert!(prepared_view(&chart).bar_at(at(0)).is_none());
}

#[test]
fn internal_partial_fill_keeps_other_gaps_requestable_and_cannot_be_promoted() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames, owners) = missing_history_chart(shared);
        resume_missing_history(&feed, &mut chart, &mut frames);
        let mut answer = partial_page(shared, 2..3, &[2]);
        answer.end_of_history = false;
        deliver(&feed, &mut chart, &mut frames, answer);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(
            (requests[0].since, requests[0].before),
            (Some(at(1)), Some(at(2)))
        );
        deliver(
            &feed,
            &mut chart,
            &mut frames,
            missing_history_page(shared, at(1), at(2), 11.0, false).0,
        );
        let view = prepared_view(&chart);
        assert_eq!(
            view.complete_bars(..)
                .map(|bar| bar.unknown)
                .collect::<Vec<_>>(),
            [3.0, 11.0, 7.0]
        );
        assert_eq!(view.bar_at(at(2)).unwrap().unknown, 7.0);
        assert!(!view.span_is_complete(at(0), at(3)));
        for (time, owner) in [at(0), at(3)].into_iter().zip(owners) {
            assert_eq!(
                view.bar_at(time).unwrap().levels.get(&100).unwrap() as *const FootprintLevel,
                owner
            );
        }
        assert_eq!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .unwrap()
                .live
                .prepared_before(),
            Some(at(4))
        );
        assert!(footprint_requests(&feed).is_empty());
        // Even a confirmed-empty overlapping response cannot certify the terminal slot.
        let mut link = chart.feed_link.take().unwrap();
        let mut request = requests[0].clone();
        request.since = Some(at(2));
        request.before = Some(at(3));
        chart.apply_footprints(
            &mut link,
            &request,
            FootprintPage::new(vec![], at(3), false).with_continue_before(at(2)),
        );
        chart.feed_link = Some(link);
        assert!(matches!(
            chart.feed.last_error,
            Some(FeedError::InvalidData(_))
        ));
        assert!(prepared_view(&chart).partial_at(at(2)));
        assert!(!prepared_view(&chart).complete_at(at(2)));
        #[cfg(feature = "studies")]
        assert!(
            !missing_history_input(&chart)
                .footprint_snapshot(at(0))
                .span_is_complete(at(0), at(3))
        );
    }
}

#[test]
fn partial_initial_history_allows_older_loading_and_survives_resume() {
    for shared in [false, true] {
        let (feed, mut chart, mut frames) = prepared_chart();
        footprint_requests(&feed);
        let mut answer = partial_page(shared, 2..4, &[2, 3]);
        answer.end_of_history = false;
        deliver(&feed, &mut chart, &mut frames, answer);
        let requests = footprint_requests(&feed);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].before, Some(at(2)));
        deliver(
            &feed,
            &mut chart,
            &mut frames,
            prepared_page(&recorded(0..2), at(2), true),
        );
        chart.set_feed_paused(true);
        frames.run(&mut chart);
        chart.set_feed_paused(false);
        frames.run(&mut chart);
        feed.resolve()
            .send(Ok(Instrument::new("A").with_tick_size(1.0)));
        feed.page().send(page(bars(5), true));
        frames.run(&mut chart);
        frames.run(&mut chart);
        assert!(footprint_requests(&feed).is_empty());
        assert!(prepared_view(&chart).partial_at(at(2)));
        assert!(prepared_view(&chart).partial_at(at(3)));
        assert!(prepared_view(&chart).span_is_complete(at(0), at(1)));
    }
}

#[test]
fn partial_rows_survive_live_dedup_raw_history_replay_and_return_to_live() {
    for shared in [false, true] {
        for page_first in [false, true] {
            let (feed, mut chart, mut frames) = prepared_chart();
            #[cfg(feature = "studies")]
            chart.add_indicator(
                crate::studies::IndicatorFactory::new()
                    .create("Vol Profile FR")
                    .unwrap(),
            );
            frames.run(&mut chart);
            let sink = feed.sink();
            let live = [
                Tick::new(at(3), 100.0, 11.0, None).with_id(19),
                Tick::new(at(3), 100.0, 13.0, None).with_id(2),
            ];
            if page_first {
                deliver(
                    &feed,
                    &mut chart,
                    &mut frames,
                    partial_page(shared, 0..3, &[1]),
                );
            }
            for trade in &live {
                sink.push_tick(trade.clone());
            }
            frames.run(&mut chart);
            if !page_first {
                deliver(
                    &feed,
                    &mut chart,
                    &mut frames,
                    partial_page(shared, 0..3, &[1]),
                );
            }
            let mut raw = recorded(0..3);
            raw[1].size = 999.0;
            raw.extend(live.iter().cloned());
            feed.trades
                .lock()
                .unwrap()
                .remove(0)
                .send(Ok(TradePage::new(raw, true)));
            sink.push_tick(live[0].clone());
            frames.run(&mut chart);
            let view = prepared_view(&chart);
            assert_eq!(view.bar_at(at(1)).unwrap().unknown, 5.0);
            assert_eq!(view.complete_bar_at(at(3)).unwrap().unknown, 24.0);
            for cutoff in [
                at(0) + TimeDelta::seconds(30),
                at(1) + TimeDelta::seconds(30),
                at(3),
            ] {
                chart.set_replay_time(Some(cutoff));
                frames.run(&mut chart);
                let link = chart.feed_link.as_ref().unwrap();
                let replay = link
                    .footprint(Some(cutoff), chart.bars(SeriesId::MAIN))
                    .unwrap();
                let replay = FootprintView::from(&replay);
                assert!(replay.bar_at(at(1)).is_none());
                assert!(!replay.span_is_complete(at(0), at(2)));
                #[cfg(feature = "studies")]
                {
                    let read = missing_history_input(&chart).footprint_snapshot(at(0));
                    assert!(read.bars(..).all(|bar| bar.time != at(1)));
                    assert!(!read.span_is_complete(at(0), at(2)));
                }
            }
            chart.set_replay_time(None);
            frames.run(&mut chart);
            assert!(prepared_view(&chart).partial_at(at(1)));
            assert_eq!(prepared_view(&chart).bar_at(at(1)).unwrap().unknown, 5.0);
        }
    }
}

#[test]
fn frozen_partial_coverage_is_independent_of_rebinding_the_same_row_owner() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let FootprintBars::Shared(owner) = partial_page(true, 0..3, &[1]).bars else {
        unreachable!()
    };
    let weak = Arc::downgrade(&owner);
    deliver(
        &feed,
        &mut chart,
        &mut frames,
        FootprintPage::partial(
            FootprintBars::Shared(owner.clone()),
            at(3),
            true,
            vec![at(1)],
        ),
    );
    let row = prepared_view(&chart)
        .bar_at(at(1))
        .unwrap()
        .levels
        .get(&100)
        .unwrap() as *const FootprintLevel;
    let mut frozen = chart.export_snapshot(&frames.ctx, &Default::default());
    let old_sink = feed.sink();
    let next = Arc::new(Script::default());
    next.prepared_footprints.store(true, Ordering::Relaxed);
    chart.set_feed(next.clone());
    frames.run(&mut chart);
    next.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
    next.page().send(page(bars(5), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    old_sink.push_tick(Tick::new(at(3), 100.0, 999.0, None));
    deliver(
        &next,
        &mut chart,
        &mut frames,
        FootprintPage::partial(FootprintBars::Shared(owner), at(3), true, vec![at(0)]),
    );
    frames.run(&mut frozen);
    assert_eq!(prepared_view(&chart).profile(at(0), at(2)).unknown, 12.0);
    assert_eq!(prepared_view(&frozen).profile(at(0), at(2)).unknown, 10.0);
    assert!(prepared_view(&frozen).partial_at(at(1)));
    assert!(prepared_view(&chart).partial_at(at(0)));
    assert!(!old_sink.is_active());
    assert_eq!(
        prepared_view(&frozen)
            .bar_at(at(1))
            .unwrap()
            .levels
            .get(&100)
            .unwrap() as *const FootprintLevel,
        row
    );
    drop(chart);
    assert!(weak.upgrade().is_some());
    drop(frozen);
    assert!(weak.upgrade().is_none());
}

#[test]
fn stale_partial_replies_cannot_cross_reload_rebind_or_interval_changes() {
    for change in 0..3 {
        let (feed, mut chart, mut frames) = prepared_chart();
        let stale = feed.footprints.lock().unwrap().remove(0);
        let sink = feed.sink();
        match change {
            0 => chart.reload_feed(),
            1 => chart.set_feed(Arc::new(Script::default())),
            _ => chart.set_timeframe(Timeframe::Min5),
        }
        frames.run(&mut chart);
        stale.send(Ok(partial_page(true, 0..3, &[1])));
        sink.push_tick(Tick::new(at(0), 100.0, 999.0, None));
        frames.run(&mut chart);
        assert!(!sink.is_active());
        assert!(
            chart
                .feed_link
                .as_ref()
                .unwrap()
                .footprint
                .as_ref()
                .is_none_or(|flow| flow.view().bar_at(at(1)).is_none())
        );
        assert!(chart.feed.last_error.is_none());
    }
}

#[test]
fn painter_displays_only_explicit_partial_rows_not_ordinary_loss_fragments() {
    use crate::chart::overlays::{FootprintConfig, FootprintMode, render_footprint_read};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::widget::ChartMapping;
    let (feed, mut chart, mut frames) = prepared_chart();
    deliver(
        &feed,
        &mut chart,
        &mut frames,
        partial_page(true, 0..3, &[1]),
    );
    chart
        .feed_link
        .as_mut()
        .unwrap()
        .footprint
        .as_mut()
        .unwrap()
        .live
        .add_missing(at(0), at(1));
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 240.0));
    let mapping = ChartMapping::new(
        rect,
        120.0,
        0,
        2,
        0.0,
        PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(98.0, 104.0),
            0.0,
            240.0,
            false,
        ),
    );
    let config = FootprintConfig {
        modes: vec![FootprintMode::Volume],
        auto_row_height: None,
        value_area: None,
        show_poc: false,
        ..Default::default()
    };
    let ohlc: Vec<_> = (0..3)
        .map(|minute| Bar::new(at(minute), 100.0, 100.0, 100.0, 100.0, 0.0))
        .collect();
    let mut output = frames.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ui| {
            render_footprint_read(
                ui.painter(),
                &mapping,
                &ohlc,
                prepared_view(&chart),
                &config,
            );
        },
    );
    output.textures_delta.clear();
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"5"),
        "explicit partial cells must remain visible"
    );
    assert!(
        texts.contains(&"7"),
        "complete neighbors must remain visible"
    );
    assert!(
        !texts.contains(&"3"),
        "ordinary loss fragments must stay hidden"
    );
    assert_eq!(prepared_view(&chart).profile(at(0), at(2)).unknown, 7.0);
}

#[test]
fn chart_paints_an_entirely_partial_page_and_omits_its_statistics() {
    let (feed, mut chart, mut frames) = prepared_chart();
    let page = prepared_page(&[Tick::new(at(4), 104.0, 777.0, None)], at(5), true);
    deliver(
        &feed,
        &mut chart,
        &mut frames,
        FootprintPage::partial(page.bars, at(5), true, vec![at(4)]),
    );
    chart.set_chart_type_params(crate::model::ChartTypeParams {
        footprint_modes: vec![crate::model::FootprintMode::Volume],
        footprint_row_ticks: 1,
        footprint_poc: false,
        ..Default::default()
    });
    chart.apply_synced_time_scale(160.0, 1.0);
    let mut output = frames.ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            chart.show(ui);
        },
    );
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "777")));
    assert_eq!(prepared_view(&chart).profile(at(0), at(4)).unknown, 0.0);
}
