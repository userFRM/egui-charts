//! Host-owned replay checkpoints. No bars or wall-clock timer are persisted.

use super::{ReplayAction, ReplayController};
use crate::model::ReplaySession;
use chrono::{DateTime, Utc};

impl ReplayController {
    /// Seeks to the last source bar completed by an exclusive cutoff, each bar closing as
    /// [`Self::cutoff`] closes it. Returns `false` without changes when the source has no
    /// completed bar there. Useful when changing source interval.
    pub fn seek_cutoff(&mut self, time: DateTime<Utc>, clock: &crate::model::BarClock) -> bool {
        let end = self.ts.partition_point(|&open| {
            clock
                .bar_close(clock.bar_open(open))
                .map_or(open < time, |close| close <= time)
        });
        if end == 0 {
            return false;
        }
        self.handle_action(ReplayAction::JumpToBar(end - 1));
        true
    }

    /// The exclusive cutoff after the current source bar, suitable for `Chart::set_replay_time`.
    /// Pass the source interval's clock, including its calendar timezone. For the chart's own
    /// interval that is [`Chart::bar_clock`](crate::widget::Chart::bar_clock). A bar closes at
    /// its next open, and a daily, weekly or monthly bar of a clock with an exchange session when
    /// its last trading window closes, as bar replay completes it.
    pub fn cutoff(&self, clock: &crate::model::BarClock) -> Option<DateTime<Utc>> {
        if !self.state.is_active() {
            return None;
        }
        let time = *self.curr_ts()?;
        clock
            .bar_close(clock.bar_open(time))
            .or_else(|| time.checked_add_signed(chrono::TimeDelta::nanoseconds(1)))
    }

    /// Saves the active cursor and its scope for host storage, excluding bars and transient UI
    /// state. [`ReplaySession::charts`] is left empty for the host to fill from its layout.
    pub fn session(&self) -> Option<ReplaySession> {
        self.state.is_active().then_some(ReplaySession {
            symbol: self.state.symbol.clone().unwrap_or_default(),
            time: *self.curr_ts()?,
            speed: self.state.speed.multiplier(),
            scope: self.state.scope,
            charts: Vec::new(),
        })
    }

    /// Restores an exact source timestamp, paused, or returns `false` without changes when
    /// history, symbol or speed is incompatible. Missing history must be fetched by the host;
    /// a missing timestamp is never silently replaced with a different session position. Restores
    /// [`ReplayState::scope`](super::ReplayState::scope) for the host's
    /// `ChartLayout::set_replay_time`. It does not touch the charts: the host puts each of
    /// [`ReplaySession::charts`] back on its symbol and interval, and fetches their history, first.
    pub fn restore_session(&mut self, session: &ReplaySession) -> bool {
        if self.state.symbol.as_deref().unwrap_or_default() != session.symbol
            || !session.speed.is_finite()
            || !(0.1..=100.0).contains(&session.speed)
        {
            return false;
        }
        let Ok(index) = self.ts.binary_search(&session.time) else {
            return false;
        };
        self.state.scope = session.scope;
        self.handle_action(ReplayAction::EnterReplayMode);
        self.handle_action(ReplayAction::SetSpeed(session.speed));
        self.handle_action(ReplayAction::JumpToBar(index));
        self.handle_action(ReplayAction::Pause);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarClock, BarData, ReplayChart, ReplayScope, Timeframe};
    use crate::widget::{Chart, ChartLayout};

    #[test]
    fn checkpoints_require_matching_history_and_restore_paused() {
        let bars: Vec<_> = (0..10)
            .map(|minute| {
                Bar::new(
                    DateTime::from_timestamp(1_704_067_200 + minute * 60, 0).unwrap(),
                    10.0,
                    12.0,
                    9.0,
                    11.0,
                    1.0,
                )
            })
            .collect();
        let mut replay = ReplayController::new(bars.clone(), vec![]).with_symbol("TEST");
        replay.handle_action(ReplayAction::JumpToBar(4));
        replay.handle_action(ReplayAction::SetSpeed(3.0));
        replay.handle_action(ReplayAction::Play);
        replay.state.scope = ReplayScope::AllCharts;
        let mut saved = replay.session().unwrap();
        let charts = [
            ReplayChart::new(1, "TEST", Timeframe::Min1),
            ReplayChart::new(0, "OTHER", Timeframe::Day1),
        ];
        saved.charts = charts.to_vec();
        let saved: ReplaySession =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(saved.scope, ReplayScope::AllCharts);
        assert_eq!(saved.charts, charts);
        replay.handle_action(ReplayAction::JumpToEnd);
        replay.state.scope = ReplayScope::ThisChart;
        assert!(replay.restore_session(&saved));
        assert_eq!(replay.state.curr_bar, 4);
        assert_eq!(replay.state.scope, ReplayScope::AllCharts);
        assert!(!replay.state.is_playing());
        assert_eq!(replay.state.speed.multiplier(), 3.0);
        let clock = BarClock::new(Timeframe::Min1);
        assert_eq!(replay.cutoff(&clock), Some(bars[5].time));
        // The restored scope replays every chart at the saved cursor.
        let mut charts: Vec<_> = (0..2)
            .map(|_| {
                let mut chart = Chart::new(BarData::from_bars(bars.clone()));
                chart.set_timeframe(Timeframe::Min1);
                chart
            })
            .collect();
        ChartLayout::default().set_replay_time(
            &mut charts.iter_mut().collect::<Vec<_>>(),
            0,
            replay.state.scope,
            replay.cutoff(&clock),
        );
        assert!(
            charts
                .iter()
                .all(|chart| chart.replay_time() == Some(bars[5].time))
        );
        for invalid in [
            ReplaySession {
                symbol: "OTHER".into(),
                ..saved.clone()
            },
            ReplaySession {
                time: bars[0].time - chrono::TimeDelta::minutes(1),
                ..saved.clone()
            },
            ReplaySession {
                speed: f32::NAN,
                ..saved.clone()
            },
            ReplaySession {
                speed: 0.0,
                ..saved.clone()
            },
        ] {
            assert!(!replay.restore_session(&invalid));
            assert_eq!(replay.state.curr_bar, 4);
            assert_eq!(replay.state.scope, ReplayScope::AllCharts);
        }
        // Wire formats written before sessions carried a scope, and before they carried charts.
        for (json, scope) in [
            (
                r#"{"symbol":"TEST","time":"2024-01-01T00:04:00Z","speed":3.0}"#,
                ReplayScope::ThisChart,
            ),
            (
                r#"{"symbol":"TEST","time":"2024-01-01T00:04:00Z","speed":3.0,"scope":"AllCharts"}"#,
                ReplayScope::AllCharts,
            ),
        ] {
            let old: ReplaySession = serde_json::from_str(json).unwrap();
            assert_eq!((old.scope, old.charts.len()), (scope, 0));
            assert!(replay.restore_session(&old));
            assert_eq!((replay.state.curr_bar, replay.state.scope), (4, scope));
        }
        assert!(!replay.seek_cutoff(bars[0].time, &clock));
        assert!(replay.seek_cutoff(bars[8].time, &clock));
        assert_eq!(replay.state.curr_bar, 7);
        replay.handle_action(ReplayAction::ExitReplayMode);
        assert!(replay.cutoff(&clock).is_none());
        replay.load_data(vec![], vec![], "TEST".into());
        assert!(replay.session().is_none());
        assert!(!replay.state.is_active());
    }
}
