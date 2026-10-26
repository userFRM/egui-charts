//! Study scale placement.

/// Value scale in the pane selected by [`super::Indicator::is_overlay`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StudyScale {
    /// Share the pane's main price scale and participate in its auto-range.
    Main,
    /// Fit this study's visible outputs independently and show its own axis.
    Independent,
    /// Fit independently without an axis or value tags.
    None,
}
