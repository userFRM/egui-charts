//! Host-defined tools and availability of built-in tools.

use super::rendering::ChartView;
use super::{Drawing, DrawingToolType};
use egui::{Painter, Pos2, Rect, Ui};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Rendering, picking and settings for a host-defined drawing tool. Anchors are projected
/// by the manager into `drawing.points`; tool-specific data lives in `drawing.custom_settings`.
/// Callbacks must tolerate incomplete drawings while the pointer previews their construction.
pub trait DrawingRenderer: Send + Sync {
    /// Paints inside the supplied pane clip, using current projected anchors and market data.
    fn render(&self, drawing: &Drawing, painter: &Painter, rect: Rect, view: &ChartView);
    /// Tests the tool's geometry in screen points. This result is authoritative, including
    /// after painting; returning false keeps custom decorative areas out of selection.
    fn hit_test(&self, drawing: &Drawing, point: Pos2, tolerance: f32) -> bool;
    /// Edits the tool's settings and returns whether they changed. The manager retains undo,
    /// lock checks and persistence notifications. Tools without settings keep the default.
    fn settings(&self, _ui: &mut Ui, _settings: &mut serde_json::Value) -> bool {
        false
    }
}

/// Whether a built-in tool appears in a picker and can create new drawings. Existing drawings
/// remain visible and editable, so hiding a tool never makes a saved annotation disappear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolAvailability {
    /// Include the tool in picker lists.
    pub visible: bool,
    /// Permit new drawings and interactive placement.
    pub enabled: bool,
}

impl Default for ToolAvailability {
    fn default() -> Self {
        Self {
            visible: true,
            enabled: true,
        }
    }
}

/// A registered tool's host-facing name, anchor count and implementation.
#[derive(Clone)]
pub struct RegisteredDrawingTool {
    /// Picker label supplied by the host.
    pub name: String,
    /// Exact number of clicked anchors required to complete the construction.
    pub anchors: usize,
    /// Shared implementation, separate from each drawing's saved settings.
    pub implementation: Arc<dyn DrawingRenderer>,
}

/// Per-manager catalogue. Register stable host-owned string ids before loading drawings.
/// Unknown custom ids stay in storage but are not rendered or hit until registered.
/// Clone the catalogue into a host picker to share implementations and availability.
#[derive(Clone, Default)]
pub struct DrawingToolRegistry {
    custom: BTreeMap<String, RegisteredDrawingTool>,
    builtins: HashMap<DrawingToolType, ToolAvailability>,
}

impl DrawingToolRegistry {
    /// Registers a tool. Empty ids, zero anchors and duplicate ids leave the catalogue intact.
    pub fn register(
        &mut self,
        id: impl Into<String>,
        tool: RegisteredDrawingTool,
    ) -> Result<(), &'static str> {
        let id = id.into();
        if id.trim().is_empty() || tool.anchors == 0 {
            return Err("A drawing tool needs an id and at least one anchor");
        }
        if self.custom.contains_key(&id) {
            return Err("Drawing tool id is already registered");
        }
        self.custom.insert(id, tool);
        Ok(())
    }

    /// Looks up a custom tool by its saved id.
    pub fn get(&self, id: &str) -> Option<&RegisteredDrawingTool> {
        self.custom.get(id)
    }

    /// Custom tool ids and metadata in stable id order for host pickers.
    pub fn custom_tools(&self) -> impl Iterator<Item = (&str, &RegisteredDrawingTool)> {
        self.custom.iter().map(|(id, tool)| (id.as_str(), tool))
    }

    /// Hides or disables a built-in tool. Hidden tools may still be created by the host when
    /// enabled; disabling rejects new creation through the manager and chart APIs.
    pub fn set_availability(&mut self, tool: DrawingToolType, availability: ToolAvailability) {
        self.builtins.insert(tool, availability);
    }

    /// Availability of a built-in tool; all tools are visible and enabled by default.
    pub fn availability(&self, tool: DrawingToolType) -> ToolAvailability {
        self.builtins.get(&tool).copied().unwrap_or_default()
    }

    /// Built-in picker entries, excluding hidden tools and including disabled entries.
    pub fn builtin_tools(&self) -> impl Iterator<Item = (DrawingToolType, ToolAvailability)> + '_ {
        super::categories::get_all_tools()
            .into_iter()
            .filter_map(|tool| {
                let availability = self.availability(tool);
                availability.visible.then_some((tool, availability))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingManager};

    struct Midpoint;
    impl DrawingRenderer for Midpoint {
        fn render(&self, drawing: &Drawing, painter: &Painter, _rect: Rect, _view: &ChartView) {
            if let [a, b] = drawing.points.as_slice() {
                painter.circle_filled(a.lerp(*b, 0.5), 10.0, egui::Color32::RED);
            }
        }
        fn hit_test(&self, drawing: &Drawing, point: Pos2, tolerance: f32) -> bool {
            match drawing.points.as_slice() {
                [a, b] => a.lerp(*b, 0.5).distance(point) <= 10.0 + tolerance,
                _ => false,
            }
        }
        fn settings(&self, _ui: &mut Ui, settings: &mut serde_json::Value) -> bool {
            *settings = serde_json::json!({"radius": 10});
            true
        }
    }

    #[test]
    fn registered_tools_use_host_geometry_and_retain_settings_through_history_and_storage() {
        let mut manager = DrawingManager::new();
        manager
            .registry
            .register(
                "midpoint",
                RegisteredDrawingTool {
                    name: "Midpoint".into(),
                    anchors: 2,
                    implementation: Arc::new(Midpoint),
                },
            )
            .unwrap();
        manager.set_custom_tool("midpoint").unwrap();
        manager.start_drawing_with_coords(
            DrawingToolType::Polyline,
            egui::pos2(100., 100.),
            |x| x,
            f64::from,
        );
        manager.add_point_with_coords(egui::pos2(200., 100.), |x| x, f64::from);
        let id = manager.drawings[0].id;
        assert!(manager.curr_drawing.is_none());
        assert_eq!(manager.drawings[0].custom_tool.as_deref(), Some("midpoint"));
        assert_eq!(manager.hit_test(egui::pos2(150., 100.)), Some(id));
        assert_eq!(manager.hit_test(egui::pos2(110., 100.)), None);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            manager.render_all(
                ui.painter(),
                ui.max_rect(),
                &ChartView {
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|x| x,
                    price_to_y: &|y| y as f32,
                },
            );
            assert!(manager.show_custom_settings(ui, id));
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Circle(c) if c.center == egui::pos2(150.,100.) && c.radius == 10.)));
        output.drop_without_applying_deltas();
        assert_eq!(manager.hit_test(egui::pos2(110., 100.)), None);
        assert!(manager.undo());
        assert_eq!(manager.drawings[0].custom_settings, serde_json::Value::Null);
        assert!(manager.redo());
        let stored = crate::drawings::persistence::drawing_to_stored(&manager.drawings[0], |i| {
            i as i64 * 1000
        });
        let restored =
            crate::drawings::persistence::stored_to_drawing(&stored, |t| t as f32 / 1000., 7);
        assert_eq!(restored.custom_tool.as_deref(), Some("midpoint"));
        assert_eq!(restored.custom_settings, serde_json::json!({"radius": 10}));
        let mut invalid = Drawing::custom("midpoint");
        invalid.chart_points = vec![ChartPoint::new(1., 1.)];
        assert!(manager.add_drawing(invalid).is_err());
        manager.registry.set_availability(
            DrawingToolType::TrendLine,
            ToolAvailability {
                visible: false,
                enabled: false,
            },
        );
        assert!(
            !manager
                .registry
                .builtin_tools()
                .any(|(tool, _)| tool == DrawingToolType::TrendLine)
        );
        manager.set_active_tool(Some(DrawingToolType::TrendLine));
        assert_eq!(manager.active_tool, None);
        manager.start_drawing_with_coords(DrawingToolType::TrendLine, Pos2::ZERO, |x| x, f64::from);
        assert!(manager.curr_drawing.is_none());
        let mut disabled = Drawing::new(0, DrawingToolType::TrendLine);
        disabled.chart_points = vec![ChartPoint::new(1., 1.), ChartPoint::new(2., 2.)];
        assert!(manager.add_drawing(disabled).is_err());
    }
}
