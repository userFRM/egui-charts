//! Host-owned replay checkpoints. No bars or wall-clock timer are persisted.

use super::{ReplayAction, ReplayController};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A resumable position in host-supplied history. Store it with the symbol's source interval
/// and fetch that same source before restoring it. Restoring always pauses playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplaySession {
    /// Source symbol, checked before restoration.
    pub symbol: String,
    /// Opening time of the last revealed source bar.
    pub time: DateTime<Utc>,
    /// Playback speed multiplier, validated when restoring.
    pub speed: f32,
}

impl ReplayController {
    /// Seeks to the last source bar completed by an exclusive cutoff. Returns `false` without
    /// changes when the source has no completed bar there. Useful when changing source interval.
    pub fn seek_cutoff(&mut self, time: DateTime<Utc>, clock: &crate::model::BarClock) -> bool {
        let end = self.ts.partition_point(|&open| {
            clock
                .next_open(clock.bar_open(open))
                .map_or(open < time, |close| close <= time)
        });
        if end == 0 {
            return false;
        }
        self.handle_action(ReplayAction::JumpToBar(end - 1));
        true
    }

    /// The exclusive cutoff after the current source bar, suitable for `Chart::set_replay_time`.
    /// Pass the source interval's clock, including its calendar timezone.
    pub fn cutoff(&self, clock: &crate::model::BarClock) -> Option<DateTime<Utc>> {
        if !self.state.is_active() {
            return None;
        }
        let time = *self.curr_ts()?;
        clock
            .next_open(clock.bar_open(time))
            .or_else(|| time.checked_add_signed(chrono::TimeDelta::nanoseconds(1)))
    }

    /// Saves the active cursor for host storage, excluding bars and transient UI state.
    pub fn session(&self) -> Option<ReplaySession> {
        self.state.is_active().then_some(ReplaySession {
            symbol: self.state.symbol.clone().unwrap_or_default(),
            time: *self.curr_ts()?,
            speed: self.state.speed.multiplier(),
        })
    }

    /// Restores an exact source timestamp, paused, or returns `false` without changes when
    /// history, symbol or speed is incompatible. Missing history must be fetched by the host;
    /// a missing timestamp is never silently replaced with a different session position.
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
    use crate::model::{Bar, BarClock, Timeframe};

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
        let saved = replay.session().unwrap();
        let saved: ReplaySession =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        replay.handle_action(ReplayAction::JumpToEnd);
        assert!(replay.restore_session(&saved));
        assert_eq!(replay.state.curr_bar, 4);
        assert!(!replay.state.is_playing());
        assert_eq!(replay.state.speed.multiplier(), 3.0);
        let clock = BarClock::new(Timeframe::Min1);
        assert_eq!(replay.cutoff(&clock), Some(bars[5].time));
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
