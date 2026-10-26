//! Session-only example host: alert storage, presets and evaluation on the synthetic chart feed.

use std::sync::Mutex;

use egui_charts::data::Reply;
use egui_charts::model::{Alert, AlertId};
use egui_charts::services::{AlertEvent, Alerts, Events};

/// Session-only storage used by the demo alerts panel.
#[derive(Default)]
pub struct DemoAlerts(Mutex<State>);

#[derive(Default)]
struct State {
    alerts: Vec<Alert>,
    events: Option<Events<AlertEvent>>,
    next_id: u64,
    evaluation: std::collections::HashMap<AlertId, Evaluation>,
    documents: std::collections::HashMap<(egui_charts::services::StoreKind, String), String>,
}

impl Alerts for DemoAlerts {
    fn watch(&self, events: Events<AlertEvent>) {
        let mut state = self.0.lock().unwrap();
        for alert in &state.alerts {
            events.push(AlertEvent::Upsert(alert.clone()));
        }
        state.events = Some(events);
    }

    fn save(&self, mut alert: Alert, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        if alert.id.is_none() {
            state.next_id += 1;
            alert.id = Some(AlertId(state.next_id.to_string()));
        }
        if let Some(id) = &alert.id {
            state.evaluation.remove(id);
        }
        state.alerts.retain(|a| a.id != alert.id);
        state.alerts.push(alert.clone());
        if let Some(events) = &state.events {
            events.push(AlertEvent::Upsert(alert));
        }
        reply.send(Ok(()));
    }

    fn remove(&self, id: AlertId, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        state.evaluation.remove(&id);
        state.alerts.retain(|a| a.id.as_ref() != Some(&id));
        if let Some(events) = &state.events {
            events.push(AlertEvent::Remove(id));
        }
        reply.send(Ok(()));
    }
}

impl egui_charts::services::Storage for DemoAlerts {
    fn list(&self, kind: egui_charts::services::StoreKind, reply: Reply<Vec<String>>) {
        let state = self.0.lock().unwrap();
        reply.send(Ok(state
            .documents
            .keys()
            .filter(|(k, _)| *k == kind)
            .map(|(_, name)| name.clone())
            .collect()));
    }
    fn load(
        &self,
        kind: egui_charts::services::StoreKind,
        name: &str,
        reply: Reply<Option<String>>,
    ) {
        reply.send(Ok(self
            .0
            .lock()
            .unwrap()
            .documents
            .get(&(kind, name.into()))
            .cloned()));
    }
    fn save(
        &self,
        kind: egui_charts::services::StoreKind,
        name: &str,
        document: String,
        reply: Reply<()>,
    ) {
        self.0
            .lock()
            .unwrap()
            .documents
            .insert((kind, name.into()), document);
        reply.send(Ok(()));
    }
    fn delete(&self, kind: egui_charts::services::StoreKind, name: &str, reply: Reply<()>) {
        self.0
            .lock()
            .unwrap()
            .documents
            .remove(&(kind, name.into()));
        reply.send(Ok(()));
    }
}

#[derive(Default)]
struct Evaluation {
    sample: Option<(chrono::DateTime<chrono::Utc>, u64, u64)>,
    previous: Vec<(Option<f64>, Option<f64>)>,
    triggered_bar: Option<chrono::DateTime<chrono::Utc>>,
}

impl DemoAlerts {
    /// The example host evaluates only its displayed synthetic series, once per changed sample.
    /// A deployed host supplies its own durable evaluator and feeds, including watchlist members.
    pub fn evaluate(&self, chart: &egui_charts::Chart, now: chrono::DateTime<chrono::Utc>) {
        use egui_charts::model::{AlertFrequency, AlertStatus};
        let mut state = self.0.lock().unwrap();
        let State {
            alerts,
            evaluation,
            events,
            ..
        } = &mut *state;
        let bars = chart.bars(egui_charts::chart::series::SeriesId::MAIN);
        for alert in alerts
            .iter_mut()
            .filter(|a| a.status == AlertStatus::Active)
        {
            if alert.expires.is_some_and(|expiry| expiry <= now) {
                alert.status = AlertStatus::Expired;
                if let Some(events) = events {
                    events.push(AlertEvent::Upsert(alert.clone()));
                }
                continue;
            }
            if alert.symbol != chart.symbol() || alert.watchlist.is_some() || !alert.is_valid() {
                continue;
            }
            let Some(id) = &alert.id else { continue };
            let closed = alert.frequency == AlertFrequency::OncePerBarClose;
            let Some(index) = bars.len().checked_sub(if closed { 2 } else { 1 }) else {
                continue;
            };
            let bar = &bars[index];
            let run = evaluation.entry(id.clone()).or_default();
            let sample = (bar.time, bar.close.to_bits(), bar.volume.to_bits());
            if closed && run.sample.is_some_and(|s| s.0 == bar.time) {
                continue;
            }
            let rules = std::iter::once((&alert.source, &alert.condition)).chain(
                alert
                    .additional_conditions
                    .iter()
                    .map(|r| (&r.source, &r.condition)),
            );
            let mut satisfied = true;
            let mut inputs = Vec::new();
            for (rule_index, (source, condition)) in rules.enumerate() {
                let current = source_value(chart, source, index);
                let other = match condition {
                    egui_charts::model::AlertCondition::Compare { other, .. } => {
                        source_value(chart, other, index)
                    }
                    _ => None,
                };
                let previous = run.previous.get(rule_index).copied().unwrap_or_else(|| {
                    (
                        index
                            .checked_sub(1)
                            .and_then(|i| source_value(chart, source, i)),
                        match condition {
                            egui_charts::model::AlertCondition::Compare { other, .. } => index
                                .checked_sub(1)
                                .and_then(|i| source_value(chart, other, i)),
                            _ => None,
                        },
                    )
                });
                satisfied &= current.is_some_and(|value| {
                    condition_holds(
                        condition,
                        value,
                        previous.0,
                        other,
                        previous.1,
                        |lookback| {
                            index
                                .checked_sub(lookback as usize)
                                .and_then(|i| source_value(chart, source, i))
                        },
                        &bars[..=index],
                    )
                });
                inputs.push((current, other));
            }
            if run.sample == Some(sample) && run.previous == inputs {
                continue;
            }
            let fire = satisfied
                && match alert.frequency {
                    AlertFrequency::Once | AlertFrequency::EveryTime => true,
                    AlertFrequency::OncePerBar | AlertFrequency::OncePerBarClose => {
                        run.triggered_bar != Some(bar.time)
                    }
                    _ => false,
                };
            run.previous = inputs;
            run.sample = Some(sample);
            if fire {
                run.triggered_bar = Some(bar.time);
                if alert.frequency == AlertFrequency::Once {
                    alert.status = AlertStatus::Triggered;
                }
                if let Some(events) = events {
                    events.push(AlertEvent::Upsert(alert.clone()));
                    events.push(AlertEvent::Triggered {
                        id: id.clone(),
                        time: now,
                        price: Some(bar.close),
                    });
                }
            }
        }
    }
}

fn source_value(
    chart: &egui_charts::Chart,
    source: &egui_charts::model::AlertSource,
    index: usize,
) -> Option<f64> {
    use egui_charts::model::AlertSource;
    let bars = chart.bars(egui_charts::chart::series::SeriesId::MAIN);
    let value = match source {
        AlertSource::Price => bars.get(index)?.close,
        AlertSource::Volume => bars.get(index)?.volume,
        AlertSource::Study { id, output } => chart
            .indicators()
            .find(|(key, _)| key.0.to_string() == *id)?
            .1
            .plot_value(*output, index)?,
        AlertSource::Drawing(id) => {
            use egui_charts::drawings::DrawingToolType;
            let drawing = chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id.to_string() == *id)?;
            let first = drawing.chart_points.first()?;
            match drawing.tool_type {
                DrawingToolType::HorizontalLine | DrawingToolType::HorizontalRay => first.price,
                DrawingToolType::TrendLine
                | DrawingToolType::Ray
                | DrawingToolType::ExtendedLine => {
                    let second = drawing.chart_points.get(1)?;
                    let span = f64::from(second.bar_idx - first.bar_idx);
                    if span == 0.0 {
                        return None;
                    }
                    first.price
                        + (index as f64 - f64::from(first.bar_idx)) * (second.price - first.price)
                            / span
                }
                _ => return None,
            }
        }
    };
    value.is_finite().then_some(value)
}

fn comparison_holds(
    comparison: egui_charts::model::AlertComparison,
    value: f64,
    previous: Option<f64>,
    level: f64,
    previous_level: Option<f64>,
) -> bool {
    use egui_charts::model::AlertComparison::*;
    let up = previous
        .zip(previous_level)
        .is_some_and(|(p, l)| p < l && value >= level);
    let down = previous
        .zip(previous_level)
        .is_some_and(|(p, l)| p > l && value <= level);
    match comparison {
        Crossing => up || down,
        CrossingUp => up,
        CrossingDown => down,
        Above => value > level,
        Below => value < level,
    }
}

fn condition_holds(
    condition: &egui_charts::model::AlertCondition,
    value: f64,
    previous: Option<f64>,
    other: Option<f64>,
    previous_other: Option<f64>,
    lookback: impl Fn(u32) -> Option<f64>,
    bars: &[egui_charts::model::Bar],
) -> bool {
    use egui_charts::model::{AlertComparison as C, AlertCondition::*};
    match condition {
        Crossing(level) => comparison_holds(C::Crossing, value, previous, *level, Some(*level)),
        CrossingUp(level) => comparison_holds(C::CrossingUp, value, previous, *level, Some(*level)),
        CrossingDown(level) => {
            comparison_holds(C::CrossingDown, value, previous, *level, Some(*level))
        }
        Above(level) => value > *level,
        Below(level) => value < *level,
        EnteringChannel { lower, upper } => previous
            .is_some_and(|p| (p < *lower || p > *upper) && value >= *lower && value <= *upper),
        ExitingChannel { lower, upper } => previous
            .is_some_and(|p| p >= *lower && p <= *upper && (value < *lower || value > *upper)),
        MovingBy {
            value: change,
            bars,
        } => lookback(*bars).is_some_and(|p| reaches(value - p, *change)),
        MovingByPercent { percent, bars } => lookback(*bars)
            .is_some_and(|p| p != 0.0 && reaches(100.0 * (value - p) / p.abs(), *percent)),
        Compare { comparison, .. } => other.is_some_and(|level| {
            comparison_holds(*comparison, value, previous, level, previous_other)
        }),
        VolumeAboveAverage {
            multiplier,
            bars: count,
        } => {
            let n = *count as usize;
            if n == 0 || bars.len() <= n {
                return false;
            }
            let mean = bars[bars.len() - n - 1..bars.len() - 1]
                .iter()
                .map(|b| b.volume / n as f64)
                .sum::<f64>();
            bars.last()
                .is_some_and(|b| mean.is_finite() && b.volume > multiplier * mean)
        }
        _ => false,
    }
}

fn reaches(change: f64, target: f64) -> bool {
    if target > 0.0 {
        change >= target
    } else {
        change <= target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, Utc};
    use egui_charts::model::{AlertCondition as C, AlertFrequency, AlertStatus, Bar, BarData};
    use egui_charts::ui::widget_bar::AlertsPanel;
    use egui_charts::ui_kit::notifications::Toasts;
    use std::sync::Arc;

    #[test]
    fn sample_conditions_use_strict_crossings_signed_moves_and_prior_volume() {
        let now = Utc::now();
        let bars: Vec<_> = [10.0, 20.0, 30.0, 50.0]
            .into_iter()
            .enumerate()
            .map(|(i, volume)| {
                Bar::new(
                    now + TimeDelta::minutes(i as i64),
                    100.0,
                    110.0,
                    99.0,
                    110.0,
                    volume,
                )
            })
            .collect();
        for (condition, current, previous, expected) in [
            (C::Crossing(100.0), 100.0, 99.0, true),
            (C::Crossing(100.0), 100.0, 101.0, true),
            (C::CrossingUp(100.0), 101.0, 100.0, false),
            (C::CrossingDown(100.0), 99.0, 101.0, true),
            (C::Above(100.0), 100.0, 99.0, false),
            (
                C::EnteringChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                100.0,
                99.0,
                true,
            ),
            (
                C::ExitingChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                111.0,
                110.0,
                true,
            ),
            (
                C::MovingBy {
                    value: 10.0,
                    bars: 1,
                },
                110.0,
                100.0,
                true,
            ),
            (
                C::MovingBy {
                    value: -10.0,
                    bars: 1,
                },
                90.0,
                100.0,
                true,
            ),
            (
                C::MovingByPercent {
                    percent: 10.0,
                    bars: 1,
                },
                110.0,
                100.0,
                true,
            ),
            (
                C::MovingByPercent {
                    percent: -10.0,
                    bars: 1,
                },
                -110.0,
                -100.0,
                true,
            ),
            (
                C::MovingByPercent {
                    percent: 10.0,
                    bars: 1,
                },
                110.0,
                0.0,
                false,
            ),
            (
                C::VolumeAboveAverage {
                    multiplier: 2.0,
                    bars: 3,
                },
                50.0,
                30.0,
                true,
            ),
            (
                C::VolumeAboveAverage {
                    multiplier: 2.5,
                    bars: 3,
                },
                50.0,
                30.0,
                false,
            ),
        ] {
            assert_eq!(
                condition_holds(
                    &condition,
                    current,
                    Some(previous),
                    None,
                    None,
                    |_| Some(previous),
                    &bars
                ),
                expected,
                "{condition:?}"
            );
        }
    }

    #[test]
    fn host_evaluation_preserves_frequency_expiry_and_nonprice_changes() {
        let now = Utc::now();
        let host = Arc::new(DemoAlerts::default());
        let mut panel = AlertsPanel::default();
        panel.set_service(host.clone());
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        let mut chart = egui_charts::Chart::new(BarData::from_bars(vec![
            Bar::new(now - TimeDelta::minutes(1), 99.0, 99.0, 99.0, 99.0, 10.0),
            Bar::new(now, 101.0, 101.0, 101.0, 101.0, 20.0),
        ]));
        chart.set_symbol("SAMPLE");
        let mut alert = Alert::new(chart.symbol(), C::Crossing(100.0));
        panel.save(alert.clone());
        panel.poll(&ctx, &mut toasts);
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.alerts()[0].status, AlertStatus::Triggered);
        assert_eq!(panel.log().len(), 1);
        assert_eq!(toasts.len(), 1);
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 1);
        alert.expires = Some(now - TimeDelta::seconds(1));
        panel.save(alert);
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.alerts()[1].status, AlertStatus::Expired);
        let mut alert = Alert::new(chart.symbol(), C::Above(100.0));
        alert.frequency = AlertFrequency::OncePerBar;
        panel.save(alert);
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 2);
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 2);
        chart.set_data(
            egui_charts::chart::series::SeriesId::MAIN,
            vec![
                Bar::new(now, 101.0, 101.0, 101.0, 101.0, 20.0),
                Bar::new(
                    now + TimeDelta::minutes(1),
                    102.0,
                    102.0,
                    102.0,
                    102.0,
                    30.0,
                ),
            ],
        );
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 3);
        assert_eq!(panel.alerts()[2].status, AlertStatus::Active);

        // Every changed sample that crosses is a new occurrence, even on the same bar.
        let mut alert = Alert::new(chart.symbol(), C::Crossing(100.0));
        alert.frequency = AlertFrequency::EveryTime;
        panel.save(alert);
        for (price, expected) in [(99.0, 4), (101.0, 5), (99.0, 6), (99.0, 6)] {
            chart.set_data(
                egui_charts::chart::series::SeriesId::MAIN,
                vec![
                    Bar::new(now, 101.0, 101.0, 101.0, 101.0, 20.0),
                    Bar::new(
                        now + TimeDelta::minutes(1),
                        price,
                        price,
                        price,
                        price,
                        30.0,
                    ),
                ],
            );
            host.evaluate(&chart, now);
            panel.poll(&ctx, &mut toasts);
            assert_eq!(panel.log().len(), expected, "crossing at {price}");
        }

        // Moving a comparison drawing changes the input even while the feed is idle.
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        use egui_charts::model::{AlertComparison, AlertSource};
        let mut drawing = Drawing::new(7, DrawingToolType::HorizontalLine);
        drawing.chart_points.push(ChartPoint::new(0.0, 110.0));
        chart.drawings_mut().drawings.push(drawing);
        panel.save(Alert::new(
            chart.symbol(),
            C::Compare {
                other: AlertSource::Drawing("7".into()),
                comparison: AlertComparison::Crossing,
            },
        ));
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 6);
        chart.drawings_mut().drawings[0].chart_points[0].price = 90.0;
        host.evaluate(&chart, now);
        panel.poll(&ctx, &mut toasts);
        assert_eq!(panel.log().len(), 7);
    }
}
