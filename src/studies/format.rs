//! Study scale placement.

/// Value scale in the pane selected by [`super::Indicator::is_overlay`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum StudyScale {
    /// Share the pane's main price scale and participate in its auto-range.
    Main,
    /// Share a named bar-series scale. Right and left share with other members assigned
    /// there; `Separate(id)` creates or joins an additional scale. Assignments are scoped
    /// to the study's pane. `Overlay(id)` shares a scale without an axis.
    Shared(crate::model::SeriesScale),
    /// Fit this study's visible outputs independently and show its own axis.
    Independent,
    /// Fit independently without an axis or value tags.
    None,
}
