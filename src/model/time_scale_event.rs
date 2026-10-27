//! Host-supplied events displayed beside the time axis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Category of a host-supplied time-axis event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// Company earnings announcement.
    Earnings,
    /// Cash or stock dividend.
    Dividend,
    /// Stock split or reverse split.
    Split,
    /// Economic release or scheduled announcement.
    Economic,
    /// News headline.
    News,
    /// Trading idea published by the host.
    Idea,
}

impl EventKind {
    /// Short label drawn inside the event badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Earnings => "E",
            Self::Dividend => "D",
            Self::Split => "S",
            Self::Economic => "Ec",
            Self::News => "N",
            Self::Idea => "I",
        }
    }
}

/// An event supplied by the host, which owns its data and any external service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeScaleEvent {
    /// Stable host identity, returned by the chart when the badge is clicked.
    pub id: String,
    /// Scheduled or actual event time in UTC.
    pub time: DateTime<Utc>,
    /// Category used by the event visibility controls.
    pub kind: EventKind,
    /// Headline shown in the hover popup.
    pub title: String,
    /// Host-formatted details shown beneath the headline, including any units.
    pub details: String,
}

/// Visibility of each time-axis event category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventVisibility {
    /// Show dividend events.
    pub show_dividends: bool,
    /// Show stock splits.
    pub show_splits: bool,
    /// Show earnings announcements.
    pub show_earnings: bool,
    /// Draw a dashed vertical break at each earnings announcement.
    pub show_earnings_breaks: bool,
    /// Show economic releases.
    pub show_economic_events: bool,
    /// Show news headlines.
    pub show_news: bool,
    /// Ask the host to raise a notification for each news headline it supplies. The host owns
    /// delivery; the chart does not read this.
    pub show_news_notifications: bool,
    /// Show trading ideas the host supplies.
    pub show_ideas: bool,
}

impl Default for EventVisibility {
    fn default() -> Self {
        Self {
            show_dividends: true,
            show_splits: true,
            show_earnings: false,
            show_earnings_breaks: false,
            show_economic_events: false,
            show_news: true,
            show_news_notifications: false,
            show_ideas: true,
        }
    }
}

impl EventVisibility {
    /// Whether events in this category are displayed.
    pub fn shows(&self, kind: EventKind) -> bool {
        match kind {
            EventKind::Dividend => self.show_dividends,
            EventKind::Split => self.show_splits,
            EventKind::Earnings => self.show_earnings,
            EventKind::Economic => self.show_economic_events,
            EventKind::News => self.show_news,
            EventKind::Idea => self.show_ideas,
        }
    }
}
