//! Selection Service - manages drawing selection state
//!
//! Handles single and multi-select for chart drawings.
//!
//! # Features
//!
//! - Single click selects, deselects others
//! - Ctrl/Cmd+click toggles in multi-select mode
//! - Click on empty area deselects all
//! - Selected drawings show handles

use egui::{
    Context, Pos2, Rect,
    epaint::{ClippedShape, Primitive},
};
use std::collections::HashSet;

/// Pointer-only state, never part of a saved drawing or undo history.
#[derive(Debug, Clone)]
pub(crate) struct SelectionArea {
    pub origin: Pos2,
    pub pane: Option<usize>,
    pub release_frame: Option<u64>,
    pub cancelled: bool,
    initial: HashSet<usize>,
}

/// Selection service for managing drawing selection state
#[derive(Debug, Clone, Default)]
pub struct SelectionService {
    /// Currently selected drawing IDs
    selected: HashSet<usize>,
    /// Primary selection (most recently clicked)
    primary: Option<usize>,
    /// Whether multi-select mode is active
    multi_select_mode: bool,
    pub(crate) area: Option<SelectionArea>,
    /// The frame and area intent of the last primary press, before panning moves geometry.
    pub(crate) area_press: Option<(u64, bool)>,
}

impl SelectionService {
    /// Create a new selection service
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a single drawing (deselects all others unless multi-select is active)
    ///
    /// Returns true if selection changed.
    pub fn select(&mut self, id: usize) -> bool {
        if self.multi_select_mode {
            // Add to selection
            let changed = self.selected.insert(id);
            self.primary = Some(id);
            changed
        } else {
            // Single select mode - replace selection
            let was_selected = self.selected.contains(&id) && self.selected.len() == 1;
            self.selected.clear();
            self.selected.insert(id);
            self.primary = Some(id);
            !was_selected
        }
    }

    /// Toggle selection of a drawing (for Ctrl+click behavior)
    ///
    /// Returns true if selection changed.
    pub fn toggle(&mut self, id: usize) -> bool {
        if self.selected.contains(&id) {
            self.selected.remove(&id);
            if self.primary == Some(id) {
                self.primary = self.selected.iter().copied().next();
            }
            true
        } else {
            self.selected.insert(id);
            self.primary = Some(id);
            true
        }
    }

    /// Deselect a specific drawing
    ///
    /// Returns true if the drawing was selected.
    pub fn deselect(&mut self, id: usize) -> bool {
        let removed = self.selected.remove(&id);
        if self.primary == Some(id) {
            self.primary = self.selected.iter().copied().next();
        }
        removed
    }

    /// Deselect all drawings
    ///
    /// Returns true if any drawings were selected.
    pub fn deselect_all(&mut self) -> bool {
        self.cancel_area();
        let had_selection = !self.selected.is_empty();
        self.selected.clear();
        self.primary = None;
        had_selection
    }

    /// Starts an area gesture that adds to the current selection.
    pub(crate) fn begin_area(&mut self, origin: Pos2, pane: Option<usize>) {
        self.area = Some(SelectionArea {
            origin,
            pane,
            release_frame: None,
            cancelled: false,
            initial: self.selected.clone(),
        });
    }

    /// Drops the press intent and marks a live area cancelled until the button is released.
    pub(crate) fn cancel_area(&mut self) {
        if let Some((_, intent)) = &mut self.area_press {
            *intent = false;
        }
        if let Some(area) = &mut self.area {
            area.cancelled = true;
        }
    }

    /// Replaces the selection with the one the area began with plus `ids`.
    pub(crate) fn update_area(&mut self, ids: impl IntoIterator<Item = usize>) {
        if let Some(area) = &self.area {
            self.selected = area.initial.clone();
            self.select_all(ids);
            if !self.primary.is_some_and(|id| self.selected.contains(&id)) {
                self.primary = self.selected.iter().copied().min();
            }
        }
    }

    /// Tests the clipped paint triangles, so curves, labels and custom renderers use the same
    /// geometry as the chart. Bounding rectangles alone would select empty space around them.
    pub(crate) fn intersects_painted(ctx: &Context, shapes: &[ClippedShape], area: Rect) -> bool {
        let candidates = shapes
            .iter()
            .filter(|s| {
                s.clip_rect.intersects(area)
                    && (s.transform_after_tessellation != egui::emath::TSTransform::IDENTITY
                        || s.shape.visual_bounding_rect().intersects(area))
            })
            .cloned()
            .collect();
        ctx.tessellate(candidates, ctx.pixels_per_point())
            .iter()
            .any(|primitive| {
                let area = area.intersect(primitive.clip_rect);
                if !area.is_positive() {
                    return false;
                }
                match &primitive.primitive {
                    Primitive::Mesh(mesh) => mesh.triangles().any(|indices| {
                        let vertices = indices.map(|i| mesh.vertices[i as usize]);
                        vertices.iter().any(|v| v.color.a() != 0)
                            && triangle_intersects_rect(vertices.map(|v| v.pos), area)
                    }),
                    Primitive::Callback(callback) => callback.rect.intersects(area),
                }
            })
    }

    /// Check if a specific drawing is selected
    #[inline]
    pub fn is_selected(&self, id: usize) -> bool {
        self.selected.contains(&id)
    }

    /// Check if any drawings are selected
    #[inline]
    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Get the primary (most recently selected) drawing ID
    #[inline]
    pub fn primary(&self) -> Option<usize> {
        self.primary
    }

    /// Get all selected drawing IDs
    #[inline]
    pub fn selected_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected.iter().copied()
    }

    /// Get count of selected drawings
    #[inline]
    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Enable/disable multi-select mode
    pub fn set_multi_select_mode(&mut self, enabled: bool) {
        self.multi_select_mode = enabled;
    }

    /// Check if multi-select mode is active
    #[inline]
    pub fn is_multi_select_mode(&self) -> bool {
        self.multi_select_mode
    }

    /// Select all drawings from a list of IDs
    pub fn select_all(&mut self, ids: impl IntoIterator<Item = usize>) {
        for id in ids {
            self.selected.insert(id);
            if self.primary.is_none() {
                self.primary = Some(id);
            }
        }
    }

    /// Remove selection for drawings that no longer exist
    ///
    /// Call this after deleting drawings to clean up stale selections.
    pub fn prune(&mut self, existing_ids: &HashSet<usize>) {
        self.selected.retain(|id| existing_ids.contains(id));
        if let Some(p) = self.primary
            && !existing_ids.contains(&p)
        {
            self.primary = self.selected.iter().copied().next();
        }
    }

    /// Get single selected drawing ID (legacy compatibility)
    ///
    /// Returns None if multiple drawings are selected.
    pub fn single_selection(&self) -> Option<usize> {
        if self.selected.len() == 1 {
            self.selected.iter().copied().next()
        } else {
            None
        }
    }
}

/// Separating-axis test: the rectangle's axes and each triangle edge normal must overlap.
fn triangle_intersects_rect(triangle: [Pos2; 3], rect: Rect) -> bool {
    if !Rect::from_points(&triangle).intersects(rect) {
        return false;
    }
    let center = rect.center();
    let half = rect.size() * 0.5;
    (0..3).all(|i| {
        let edge = triangle[(i + 1) % 3] - triangle[i];
        let normal = egui::vec2(-edge.y, edge.x);
        let radius = normal.x.abs() * half.x + normal.y.abs() * half.y;
        let distances = triangle.map(|p| (p - center).dot(normal));
        distances.iter().any(|&d| d <= radius) && distances.iter().any(|&d| d >= -radius)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `select` replaces the selection in single mode and extends it in multi
    /// mode. Either way the last selected drawing becomes primary, and the
    /// return value reports whether the selected set changed.
    #[test]
    fn select_replaces_or_extends_by_mode() {
        let cases = [
            (false, [true, true, true, true, false], vec![2]),
            (true, [true, true, true, false, false], vec![1, 2, 3]),
        ];
        for (multi, changed, selected) in cases {
            let mut sel = SelectionService::new();
            sel.set_multi_select_mode(multi);

            let returned = [1, 2, 3, 2, 2].map(|id| sel.select(id));

            assert_eq!(returned, changed, "multi={multi}");
            let mut ids: Vec<_> = sel.selected_ids().collect();
            ids.sort_unstable();
            assert_eq!(ids, selected, "multi={multi}");
            assert_eq!(sel.primary(), Some(2), "multi={multi}");
        }
    }

    #[test]
    fn test_toggle() {
        let mut sel = SelectionService::new();

        sel.toggle(1);
        assert!(sel.is_selected(1));

        sel.toggle(1);
        assert!(!sel.is_selected(1));
    }

    #[test]
    fn test_deselect_all() {
        let mut sel = SelectionService::new();
        sel.set_multi_select_mode(true);
        sel.select(1);
        sel.select(2);

        assert!(sel.deselect_all());
        assert!(!sel.has_selection());
        assert_eq!(sel.primary(), None);
    }

    #[test]
    fn test_prune() {
        let mut sel = SelectionService::new();
        sel.set_multi_select_mode(true);
        sel.select(1);
        sel.select(2);
        sel.select(3);

        // Drawing 2 was deleted
        let existing: HashSet<usize> = [1, 3].into_iter().collect();
        sel.prune(&existing);

        assert!(sel.is_selected(1));
        assert!(!sel.is_selected(2));
        assert!(sel.is_selected(3));
        assert_eq!(sel.selection_count(), 2);
    }

    #[test]
    fn test_select_all() {
        let mut sel = SelectionService::new();
        sel.select_all([1, 2, 3]);

        let mut ids: Vec<_> = sel.selected_ids().collect();
        ids.sort_unstable();
        assert_eq!(ids, [1, 2, 3]);
        assert_eq!(sel.primary(), Some(1));
    }

    #[test]
    fn test_single_selection_legacy() {
        let mut sel = SelectionService::new();

        // No selection
        assert_eq!(sel.single_selection(), None);

        // Single selection
        sel.select(5);
        assert_eq!(sel.single_selection(), Some(5));

        // Multiple selection
        sel.set_multi_select_mode(true);
        sel.select(6);
        assert_eq!(sel.single_selection(), None);
    }
}
