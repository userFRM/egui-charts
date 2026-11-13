//! Why a study, its inputs or its settings were refused.

use super::Indicator;
#[cfg(feature = "studies")]
use super::StudyInput;
use std::fmt;
use std::sync::{Mutex, PoisonError};

/// Why a study was not built, edited, given a source or added.
#[derive(Debug)]
#[non_exhaustive]
pub enum StudyError {
    /// The factory knows no study of this name.
    UnknownStudy(String),
    /// No study on the chart has this id.
    UnknownStudyId(usize),
    /// The study has no input with this id.
    UnknownInput(String),
    /// The study refused the value of an input.
    InvalidInput {
        /// The id of the input.
        id: String,
        /// Why, completing the sentence "study input `id` ...".
        reason: String,
    },
    /// A declared input has an empty id.
    EmptyInputId {
        /// The position of the input in the declared list, counted from 0.
        index: usize,
    },
    /// The study edits its presentation only inside a `ConfiguredStudy`.
    NotConfigured,
    /// The precision is above 12 decimals.
    InvalidPrecision(usize),
    /// A plot width is not finite or not between 0.5 and 10.
    InvalidPlotWidth {
        /// The plot, counted from 0.
        plot: usize,
        /// The refused width.
        width: f32,
    },
    /// A reference level width is not finite or not between 0.5 and 10.
    InvalidLevelWidth {
        /// The level, counted from 0.
        level: usize,
        /// The refused width.
        width: f32,
    },
    /// A horizontal level value is NaN or infinite.
    InvalidLevelValue {
        /// The level, counted from 0.
        level: usize,
        /// The refused value.
        value: f64,
    },
    /// The settings hold another number of plot styles than the study draws.
    PlotCount {
        /// The plots the study draws.
        expected: usize,
        /// The plot styles given.
        found: usize,
    },
    /// The source names a study the chart does not have.
    UnknownSourceStudy(usize),
    /// The source names a plot its study does not draw.
    UnknownSourcePlot {
        /// The id of the source study.
        id: usize,
        /// The plot, counted from 0.
        plot: usize,
    },
    /// The study would read its own output, directly or through other studies.
    CyclicSource,
    /// The chart holds as many studies as its study cap allows. The refused study comes back
    /// unchanged.
    StudyLimitReached(RefusedStudy),
    /// The text is not study clipboard text.
    NotClipboard,
    /// The copied study cannot be read.
    Clipboard(serde_json::Error),
}

impl StudyError {
    /// The error for a value an input's schema refuses.
    pub(crate) fn invalid_input(id: &str) -> Self {
        Self::InvalidInput {
            id: id.to_owned(),
            reason: "is outside its accepted values".into(),
        }
    }

    /// `UnknownInput` when `inputs` has no input `id`, otherwise [`Self::invalid_input`].
    #[cfg(feature = "studies")]
    pub(crate) fn rejected(inputs: &[StudyInput], id: &str) -> Self {
        if inputs.iter().any(|input| input.id == id) {
            Self::invalid_input(id)
        } else {
            Self::UnknownInput(id.to_owned())
        }
    }
}

/// A study the chart refused at its study cap, handed back unchanged by
/// [`into_inner`](Self::into_inner).
// The lock only makes the error `Sync`, which `Indicator` is not; nothing else locks it.
pub struct RefusedStudy(Mutex<Box<dyn Indicator>>);

impl RefusedStudy {
    #[cfg(feature = "widget")]
    pub(crate) fn new(study: Box<dyn Indicator>) -> Self {
        Self(Mutex::new(study))
    }

    /// The refused study.
    pub fn into_inner(self) -> Box<dyn Indicator> {
        self.0.into_inner().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for RefusedStudy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let study = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_tuple("RefusedStudy").field(&study.name()).finish()
    }
}

impl fmt::Display for StudyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStudy(name) => write!(f, "unknown study {name:?}"),
            Self::UnknownStudyId(id) => write!(f, "no study has id {id}"),
            Self::UnknownInput(id) => write!(f, "unknown study input {id:?}"),
            Self::InvalidInput { id, reason } => write!(f, "study input {id:?} {reason}"),
            Self::EmptyInputId { index } => write!(f, "study input {index} has an empty id"),
            Self::NotConfigured => {
                f.write_str("wrap the study in ConfiguredStudy to edit its presentation")
            }
            Self::InvalidPrecision(p) => {
                write!(f, "study precision must be at most 12 decimals, got {p}")
            }
            Self::InvalidPlotWidth { plot, width } => write!(
                f,
                "plot {plot} width must be between 0.5 and 10, got {width:?}"
            ),
            Self::InvalidLevelValue { level, value } => {
                write!(f, "level {level} value must be finite, got {value:?}")
            }
            Self::InvalidLevelWidth { level, width } => write!(
                f,
                "level {level} width must be between 0.5 and 10, got {width:?}"
            ),
            Self::PlotCount { expected, found } => write!(
                f,
                "the study draws {expected} plots, got {found} plot styles"
            ),
            Self::UnknownSourceStudy(id) => write!(f, "no study has id {id} to read as a source"),
            Self::UnknownSourcePlot { id, plot } => write!(f, "study {id} has no plot {plot}"),
            Self::CyclicSource => f.write_str("the study would read its own output"),
            Self::StudyLimitReached(_) => f.write_str("study limit reached"),
            Self::NotClipboard => f.write_str("not a study clipboard"),
            Self::Clipboard(error) => write!(f, "cannot read the copied study: {error}"),
        }
    }
}
impl std::error::Error for StudyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Clipboard(error) => Some(error),
            _ => None,
        }
    }
}
