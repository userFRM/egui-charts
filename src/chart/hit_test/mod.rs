//! Shared hit-test utilities for chart elements.
//!
//! The segment distance and tolerance live in `painting`, so that drawings use the same ones
//! without depending on the chart.

pub(crate) use crate::painting::{HIT_TOLERANCE, point_to_segment_distance};
