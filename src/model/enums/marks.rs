//! Mark-related enums
//!
//! Types for controlling chart marks (annotations on time axis and bars).

use std::fmt;

/// Mode for clearing marks from the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClearMarksMode {
    /// Clear all marks (bar marks + timescale marks)
    #[default]
    All,
    /// Clear only bar marks (markers on candles/bars)
    BarMarks,
    /// Clear only timescale marks (markers on the time axis)
    TimescaleMarks,
}

impl ClearMarksMode {
    pub fn all() -> &'static [Self] {
        &[Self::All, Self::BarMarks, Self::TimescaleMarks]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::BarMarks => "bar_marks",
            Self::TimescaleMarks => "timescale_marks",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::All => "All Marks",
            Self::BarMarks => "Bar Marks",
            Self::TimescaleMarks => "Timescale Marks",
        }
    }
}

impl fmt::Display for ClearMarksMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Mark Location - Position for mark placement
///
/// Discriminant values:
/// AboveBar=0, BelowBar=1, Top=2, Bottom=3, Right=4, AbsoluteBar=5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum MarkLocation {
    /// Above the bar
    #[default]
    AboveBar = 0,
    /// Below the bar
    BelowBar = 1,
    /// Top of the chart area
    Top = 2,
    /// Bottom of the chart area
    Bottom = 3,
    /// Right side of the chart
    Right = 4,
    /// Absolute position on the bar
    AbsoluteBar = 5,
}

impl MarkLocation {
    pub fn all() -> &'static [Self] {
        &[
            Self::AboveBar,
            Self::BelowBar,
            Self::Top,
            Self::Bottom,
            Self::Right,
            Self::AbsoluteBar,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::AboveBar => "AboveBar",
            Self::BelowBar => "BelowBar",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Right => "Right",
            Self::AbsoluteBar => "AbsoluteBar",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AboveBar => "Above Bar",
            Self::BelowBar => "Below Bar",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Right => "Right",
            Self::AbsoluteBar => "Absolute Bar",
        }
    }
}

impl fmt::Display for MarkLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_marks_mode() {
        assert_eq!(ClearMarksMode::all().len(), 3);
        assert_eq!(ClearMarksMode::All.name(), "all");
    }

    #[test]
    fn test_mark_location() {
        assert_eq!(MarkLocation::all().len(), 6);
        assert_eq!(MarkLocation::AboveBar.name(), "AboveBar");
        assert_eq!(MarkLocation::Top.display_name(), "Top");
    }
}
