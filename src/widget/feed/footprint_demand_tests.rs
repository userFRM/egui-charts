use super::*;

fn chart_and_feed() -> (Chart, Arc<Script>) {
    let feed = Arc::new(Script::default());
    feed.prepared_footprints.store(true, Ordering::Relaxed);
    let mut chart = fed(&feed, Some("A"));
    chart.set_chart_type(ChartType::VolumeFootprint);
    chart.set_visible_bars(12);
    chart.set_footprint_history_by_view(true);
    chart.set_trade_history_on_demand(true);
    (chart, feed)
}

fn resolve(feed: &Script) {
    feed.resolve()
        .send(Ok(Instrument::new("A").with_tick_size(1.0)));
}

fn request(feed: &Script) -> FootprintRequest {
    let requests = footprint_requests(feed);
    assert_eq!(requests.len(), 1, "one prepared page: {requests:?}");
    requests.into_iter().next().unwrap()
}

// Serve exactly the suffix requested, up to the source's own execution cutoff.
fn answer(feed: &Script, req: &FootprintRequest, history: &[Bar], cutoff: DateTime<Utc>) {
    let end = req.before.unwrap_or(cutoff);
    let first = history.partition_point(|bar| req.since.is_some_and(|since| bar.time < since));
    let past = history.partition_point(|bar| bar.time < end);
    let start = first.max(past.saturating_sub(req.count));
    let tape: Vec<_> = history[start..past]
        .iter()
        .map(|bar| {
            Tick::new(
                bar.time,
                bar.close,
                23.0,
                Some(crate::model::OrderSide::Buy),
            )
        })
        .collect();
    feed.footprints
        .lock()
        .unwrap()
        .remove(0)
        .send(Ok(prepared_page(&tape, end, start == 0)));
}

#[test]
fn bounded_latest_page_preserves_the_live_join_and_loads_an_older_view() {
    let (mut chart, feed) = chart_and_feed();
    let history = bars(400);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(history.clone(), true));
    resolve(&feed);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let latest = request(&feed);
    assert_eq!((latest.before, latest.since), (None, None));
    assert!((1..100).contains(&latest.count), "{latest:?}");
    assert_eq!(chart.bars(SeriesId::MAIN).len(), 400);
    let last = &history[399];
    let cutoff = last.time + TimeDelta::seconds(30);
    answer(&feed, &latest, &history, cutoff);
    feed.sink().push_tick(Tick::new(
        cutoff,
        last.close,
        4.0,
        Some(crate::model::OrderSide::Sell),
    ));
    frames.run(&mut chart);
    let joined = prepared_view(&chart).bar_at(last.time).unwrap();
    assert_eq!((joined.ask, joined.bid), (23.0, 4.0));
    assert!(prepared_view(&chart).bar_at(history[40].time).is_none());
    assert!(feed.trades.lock().unwrap().is_empty());

    assert!(
        chart.set_visible_time_range(crate::widget::VisibleTimeRange {
            from: history[38].time,
            to: history[43].time,
        })
    );
    frames.run(&mut chart);
    let older = request(&feed);
    assert!(older.since.is_some_and(|t| t <= history[40].time));
    assert!(
        older
            .before
            .is_some_and(|t| history[40].time < t && t < history[60].time)
    );
    answer(&feed, &older, &history, cutoff);
    frames.run(&mut chart);
    let view = prepared_view(&chart);
    assert_eq!(view.bar_at(history[40].time).unwrap().ask, 23.0);
    assert_eq!(view.bar_at(last.time).unwrap().volume(), 27.0);
    assert!(!view.span_is_complete(history[40].time, history[350].time));
    chart.chart_type_params.footprint_modes = vec![crate::chart::overlays::FootprintMode::Volume];
    let painted = shapes(&frames.ctx, &mut chart, frames.time, vec![]);
    assert!(
        painted
            .iter()
            .any(|shape| matches!(shape, egui::Shape::Text(t) if t.galley.text() == "23")),
        "the older viewport paints the supplied volume"
    );
}

fn loaded_latest(
    history: &[Bar],
    cutoff: DateTime<Utc>,
) -> (Chart, Arc<Script>, Frames, FootprintRequest) {
    let (mut chart, feed) = chart_and_feed();
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(history.to_vec(), true));
    resolve(&feed);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let latest = request(&feed);
    answer(&feed, &latest, history, cutoff);
    frames.run(&mut chart);
    // A second layout pass could otherwise pump the new range before this frame returns.
    frames
        .ctx
        .options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
    (chart, feed, frames, latest)
}

#[test]
fn wheel_pan_requests_the_same_missing_rows_one_frame_earlier_when_opted_in() {
    fn pan(chart: &mut Chart, frames: &mut Frames, bars: f32) {
        let pos = chart.last_rendered_price_rect.center();
        let dx = bars * chart.get_time_scale_state().0;
        frames.time += 1.0 / 60.0;
        frame(
            &frames.ctx,
            chart,
            frames.time,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(dx, 0.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }

    let history = bars(400);
    let last = history.last().unwrap().time;
    let cutoff = last + TimeDelta::minutes(1);
    let [deferred, immediate] = [false, true].map(|by_view| {
        // Start both gestures with identical prepared coverage.
        let (mut chart, feed, mut frames, latest) = loaded_latest(&history, cutoff);
        chart.set_footprint_history_by_view(by_view);
        let distance = latest.count as f32 + 12.0;
        let original = chart.visible_time_range().unwrap();
        pan(&mut chart, &mut frames, distance);
        let moved = chart.visible_time_range().unwrap();
        assert!(moved.to < original.from);
        let target = history.iter().find(|b| b.time >= moved.from).unwrap().time;
        assert!(target <= moved.to);
        assert!(!prepared_view(&chart).span_is_complete(target, target));
        if !by_view {
            assert!(footprint_requests(&feed).is_empty());
            frames.run(&mut chart);
        }
        let [older]: [_; 1] = footprint_requests(&feed)
            .try_into()
            .unwrap_or_else(|pages| {
                panic!("expected one missing page (by_view={by_view}, target={target}): {pages:?}")
            });
        assert!(older.since.is_some_and(|from| from <= target));
        assert!(older.before.is_some_and(|end| target < end));

        // Reverse with the request in flight. Its real reply can fill the old
        // view offscreen, and revisiting it must not start a second request.
        pan(&mut chart, &mut frames, -distance);
        assert!(chart.visible_time_range().unwrap().from > moved.to);
        assert_eq!(prepared_view(&chart).bar_at(last).unwrap().ask, 23.0);
        assert!(prepared_view(&chart).bar_at(target).is_none());
        assert!(footprint_requests(&feed).is_empty());
        answer(&feed, &older, &history, cutoff);
        frames.run(&mut chart);
        pan(&mut chart, &mut frames, distance);
        let revisit = chart.visible_time_range().unwrap();
        assert!(revisit.from <= target && target <= revisit.to);
        assert_eq!(prepared_view(&chart).bar_at(target).unwrap().ask, 23.0);
        assert!(footprint_requests(&feed).is_empty());
        older
    });
    assert_eq!(immediate, deferred);
}

#[test]
fn restoring_a_view_and_new_clock_in_the_pane_waits_for_rebinding() {
    let history = bars(400);
    let cutoff = history.last().unwrap().time + TimeDelta::minutes(1);
    let (mut chart, feed, mut frames, latest) = loaded_latest(&history, cutoff);
    let target = history[40].time;
    assert!(prepared_view(&chart).bar_at(target).is_none());

    // Record a view and timezone change, then redo it by keyboard *after* the frame's pump.
    let new_clock = chart.user_action(|chart| {
        let mut options = chart.chart_options().clone();
        options.time_scale.timezone =
            crate::scales::TimezoneMode::Timezone("+05:45".parse().unwrap());
        chart.set_chart_options(options);
        assert!(
            chart.set_visible_time_range(crate::widget::VisibleTimeRange {
                from: history[38].time,
                to: history[43].time,
            })
        );
        chart.bar_clock()
    });
    assert_ne!(new_clock, latest.clock);
    assert!(chart.undo());
    assert_eq!(chart.bar_clock(), latest.clock);
    assert!(chart.visible_time_range().unwrap().from > target);

    let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::CTRL;
    frames.ctx.set_os(egui::os::OperatingSystem::Nix);
    frames.time += 1.0 / 60.0;
    frame(
        &frames.ctx,
        &mut chart,
        frames.time,
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key: egui::Key::Y,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ],
    );
    assert_eq!(chart.bar_clock(), new_clock);
    let restored = chart.visible_time_range().unwrap();
    assert!(restored.from <= target && target <= restored.to);
    assert!(footprint_requests(&feed).is_empty(), "stale clock paged");

    frames.run(&mut chart);
    resolve(&feed);
    feed.page().send(page(history, true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let rebound = request(&feed);
    assert_eq!(rebound.clock, new_clock);
    assert_eq!((rebound.before, rebound.since), (None, None));
}

#[test]
fn delayed_candles_without_a_layout_do_not_stall_or_certify_missing_rows() {
    let (mut chart, feed) = chart_and_feed();
    let mut frames = Frames::new();
    chart.pump(&frames.ctx);
    resolve(&feed);
    chart.pump(&frames.ctx);
    chart.pump(&frames.ctx);
    let latest = request(&feed);
    assert!((1..100).contains(&latest.count));
    let history = bars(400);
    let cutoff = history[399].time + TimeDelta::minutes(1);
    answer(&feed, &latest, &history, cutoff);
    chart.pump(&frames.ctx);
    assert!(!prepared_view(&chart).span_is_complete(history[0].time, history[399].time));
    feed.page().send(page(history.clone(), true));
    frames.run(&mut chart);
    chart.scroll_to_time(history[40].time);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let pending = request(&feed);
    let next = Arc::new(Script::default());
    next.prepared_footprints.store(true, Ordering::Relaxed);
    chart.set_feed(next.clone());
    answer(&feed, &pending, &history, cutoff);
    frames.run(&mut chart);
    resolve(&next);
    next.page().send(page(history.clone(), true));
    frames.run(&mut chart);
    frames.run(&mut chart);
    let rebound = request(&next);
    assert!(rebound.count < 100 && rebound.before.is_none());
    answer(&next, &rebound, &history, cutoff);
    frames.run(&mut chart);
    assert!(
        prepared_view(&chart).bar_at(history[40].time).is_none(),
        "old binding's reply leaked"
    );
}

#[cfg(feature = "drawings")]
#[test]
fn visible_profile_anchor_widens_the_first_page() {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    let (mut chart, feed) = chart_and_feed();
    let history = bars(400);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(history.clone(), true));
    frames.run(&mut chart);
    let mut drawing = Drawing::new(1, DrawingToolType::AnchoredVolumeProfile);
    drawing.chart_points = vec![ChartPoint::new(70.0, 170.0)];
    chart.drawings_mut().drawings.push(drawing);
    resolve(&feed);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let latest = request(&feed);
    assert!((330..400).contains(&latest.count), "{latest:?}");
    answer(
        &feed,
        &latest,
        &history,
        history[399].time + TimeDelta::minutes(1),
    );
    frames.run(&mut chart);
    assert_eq!(
        prepared_view(&chart)
            .profile(history[70].time, history[399].time)
            .volume(),
        330.0 * 23.0
    );

    chart.set_chart_type(ChartType::Candles);
    chart.drawings_mut().drawings[0].chart_points[0] = ChartPoint::new(12.0, 112.0);
    frames.run(&mut chart);
    let older = request(&feed);
    assert_eq!(older.since, Some(history[12].time));
    answer(
        &feed,
        &older,
        &history,
        history[399].time + TimeDelta::minutes(1),
    );
    frames.run(&mut chart);
    assert_eq!(
        prepared_view(&chart)
            .profile(history[12].time, history[399].time)
            .volume(),
        388.0 * 23.0
    );
}

#[cfg(feature = "studies")]
#[test]
fn fixed_study_profile_keeps_its_offscreen_execution_volume() {
    let (mut chart, feed) = chart_and_feed();
    let history = bars(400);
    let mut frames = Frames::new();
    frames.run(&mut chart);
    feed.page().send(page(history.clone(), true));
    let study =
        crate::studies::VolProfileFixed::new(4).with_range(history[75].time, history[399].time);
    let id = chart.add_indicator(Box::new(study));
    frames.run(&mut chart);
    resolve(&feed);
    frames.run(&mut chart);
    frames.run(&mut chart);
    let latest = request(&feed);
    assert!(latest.count >= 325 && latest.count < 1000, "{latest:?}");
    answer(
        &feed,
        &latest,
        &history,
        history[399].time + TimeDelta::minutes(1),
    );
    for _ in 0..3 {
        frames.time += 1.0;
        frames.run(&mut chart);
    }
    let study = chart.indicators().find(|(seen, _)| *seen == id).unwrap().1;
    let profiles = study.volume_profiles(395..400);
    assert_eq!(profiles.len(), 1);
    assert_eq!(
        profiles[0].profile.volume().iter().sum::<f64>(),
        325.0 * 23.0
    );
}
