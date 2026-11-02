//! Serializable calculation inputs shared by studies and saved alerts.

use super::{PriceSource, Timeframe};
use egui::Color32;

/// A price series or an output of another study on the chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StudySource {
    /// A field or average of fields of the input bars.
    Price(PriceSource),
    /// A stable chart study id and its zero-based output column, without plot displacement.
    Study {
        /// Source series identifier assigned by the chart.
        id: usize,
        /// Zero-based plot index within the source study.
        plot: usize,
    },
}

/// A study input's current value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum InputValue {
    /// A bar count or nonnegative integer parameter.
    Length(usize),
    /// A signed integer parameter.
    Integer(i32),
    /// A finite numeric parameter.
    Number(f64),
    /// Index in the input's choices.
    Choice(usize),
    /// An enabled or disabled input.
    Toggle(bool),
    /// A price field or another study's output.
    Source(StudySource),
    /// An RGBA colour.
    Color(Color32),
    /// A calculation interval; `None` follows the chart.
    Timeframe(Option<Timeframe>),
    /// A host-resolved symbol; an empty string follows the chart.
    Symbol(String),
    /// Text the study parses itself, such as a date and time.
    Text(String),
}
