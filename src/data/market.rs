//! What a feed answers for the market panels beside the chart: news and highlights.

use chrono::{DateTime, Utc};

/// The stories a news or highlights panel asks [`Datafeed::news`](super::Datafeed::news) for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewsQuery {
    /// Stories about this symbol, or about the whole market when `None`.
    pub symbol: Option<String>,
    /// Short summaries of what moves the market or the symbol, instead of headlines.
    pub highlights: bool,
    /// Stories wanted at most.
    pub limit: usize,
}

/// One story, already formatted for display.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NewsStory {
    /// Identifies the story among the stories of one answer.
    pub id: String,
    /// When the story was published; the panel offers to show the chart at this time.
    pub time: DateTime<Utc>,
    pub headline: String,
    /// The publisher or provider, shown beside the time. May be empty.
    pub source: String,
    /// The story's text, shown when it is opened, and under the headline of a highlight. May be
    /// empty.
    pub body: String,
    /// The symbols the story is about.
    pub symbols: Vec<String>,
    /// A link to the full story, which the host opens.
    pub url: Option<String>,
}

impl NewsStory {
    /// A story with a headline and nothing else: set the other fields as known.
    pub fn new(id: impl Into<String>, time: DateTime<Utc>, headline: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            time,
            headline: headline.into(),
            source: String::new(),
            body: String::new(),
            symbols: Vec::new(),
            url: None,
        }
    }
}
