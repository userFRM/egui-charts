//! Analyst estimates supplied by the host's data provider.

use chrono::NaiveDate;

/// Consensus price targets for one symbol, in its price units. The chart accepts finite prices
/// with `low <= average <= high`, a nonzero analyst count and a horizon after publication.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceTargets {
    /// Highest estimate.
    pub high: f64,
    /// Average estimate.
    pub average: f64,
    /// Lowest estimate.
    pub low: f64,
    /// Analysts represented by these estimates.
    pub analysts: u32,
    /// Publication date of this consensus.
    pub published: NaiveDate,
    /// Date the estimates refer to, on the chart's UTC daily time axis.
    pub horizon: NaiveDate,
}
