//! Instrument metadata reported by a feed.

/// What a feed reports about a symbol. Build it with [`Instrument::new`]; fields will be added.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Instrument {
    /// The symbol as the feed knows it.
    pub symbol: String,
}

impl Instrument {
    /// `symbol`, with everything else unknown.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
        }
    }
}
