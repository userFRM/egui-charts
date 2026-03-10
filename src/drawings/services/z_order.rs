//! Z-Order Service - manages drawing layer ordering
//!
//! Controls the rendering order of drawings on the chart.
//! Higher z_order values are drawn on top of lower values.

use crate::drawings::domain::Drawing;

/// Service for managing drawing z-order (render layering)
pub struct ZOrderService;

impl ZOrderService {
    /// Bring a drawing to the front (highest z-order among all drawings)
    pub fn bring_to_front(drawings: &mut [Drawing], id: usize) {
        let max_z = drawings.iter().map(|d| d.z_order()).max().unwrap_or(0);
        if let Some(drawing) = drawings.iter_mut().find(|d| d.id == id) {
            drawing.set_z_order(max_z + 1);
        }
    }

    /// Send a drawing to the back (lowest z-order among all drawings)
    pub fn send_to_back(drawings: &mut [Drawing], id: usize) {
        let min_z = drawings.iter().map(|d| d.z_order()).min().unwrap_or(0);
        if let Some(drawing) = drawings.iter_mut().find(|d| d.id == id) {
            drawing.set_z_order(min_z - 1);
        }
    }

    /// Move a drawing one step forward in z-order
    ///
    /// Swaps z-order with the next drawing above it.
    pub fn bring_forward(drawings: &mut [Drawing], id: usize) {
        let target_idx = match drawings.iter().position(|d| d.id == id) {
            Some(i) => i,
            None => return,
        };
        let current_z = drawings[target_idx].z_order();

        // Find the drawing with the next higher z-order
        let swap_idx = drawings
            .iter()
            .enumerate()
            .filter(|(i, d)| *i != target_idx && d.z_order() > current_z)
            .min_by_key(|(_, d)| d.z_order())
            .map(|(i, _)| i);

        match swap_idx {
            Some(si) => {
                let above_z = drawings[si].z_order();
                drawings[target_idx].set_z_order(above_z);
                drawings[si].set_z_order(current_z);
            }
            None => {
                // Already at the top, bump up by 1
                drawings[target_idx].set_z_order(current_z + 1);
            }
        }
    }

    /// Move a drawing one step backward in z-order
    ///
    /// Swaps z-order with the next drawing below it.
    pub fn send_backward(drawings: &mut [Drawing], id: usize) {
        let target_idx = match drawings.iter().position(|d| d.id == id) {
            Some(i) => i,
            None => return,
        };
        let current_z = drawings[target_idx].z_order();

        // Find the drawing with the next lower z-order
        let swap_idx = drawings
            .iter()
            .enumerate()
            .filter(|(i, d)| *i != target_idx && d.z_order() < current_z)
            .max_by_key(|(_, d)| d.z_order())
            .map(|(i, _)| i);

        match swap_idx {
            Some(si) => {
                let below_z = drawings[si].z_order();
                drawings[target_idx].set_z_order(below_z);
                drawings[si].set_z_order(current_z);
            }
            None => {
                // Already at the bottom, push down by 1
                drawings[target_idx].set_z_order(current_z - 1);
            }
        }
    }

    /// Sort drawings by z-order (ascending: lowest z-order first, drawn first)
    pub fn sort_by_z_order(drawings: &mut [Drawing]) {
        drawings.sort_by_key(|d| d.z_order());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;

    fn make_drawing(id: usize, z: i32) -> Drawing {
        let mut d = Drawing::new(id, DrawingToolType::TrendLine);
        d.set_z_order(z);
        d
    }

    #[test]
    fn test_bring_to_front() {
        let mut drawings = vec![make_drawing(1, 0), make_drawing(2, 5), make_drawing(3, 3)];
        ZOrderService::bring_to_front(&mut drawings, 1);
        assert_eq!(drawings[0].z_order(), 6);
    }

    #[test]
    fn test_send_to_back() {
        let mut drawings = vec![make_drawing(1, 0), make_drawing(2, 5), make_drawing(3, 3)];
        ZOrderService::send_to_back(&mut drawings, 2);
        assert_eq!(drawings[1].z_order(), -1);
    }

    #[test]
    fn test_bring_forward() {
        let mut drawings = vec![make_drawing(1, 0), make_drawing(2, 5), make_drawing(3, 3)];
        ZOrderService::bring_forward(&mut drawings, 3);
        // Drawing 3 (z=3) should swap with drawing 2 (z=5)
        assert_eq!(drawings[2].z_order(), 5);
        assert_eq!(drawings[1].z_order(), 3);
    }

    #[test]
    fn test_send_backward() {
        let mut drawings = vec![make_drawing(1, 0), make_drawing(2, 5), make_drawing(3, 3)];
        ZOrderService::send_backward(&mut drawings, 3);
        // Drawing 3 (z=3) should swap with drawing 1 (z=0)
        assert_eq!(drawings[2].z_order(), 0);
        assert_eq!(drawings[0].z_order(), 3);
    }

    #[test]
    fn test_sort_by_z_order() {
        let mut drawings = vec![make_drawing(1, 5), make_drawing(2, 0), make_drawing(3, 3)];
        ZOrderService::sort_by_z_order(&mut drawings);
        assert_eq!(drawings[0].z_order(), 0);
        assert_eq!(drawings[1].z_order(), 3);
        assert_eq!(drawings[2].z_order(), 5);
    }
}
