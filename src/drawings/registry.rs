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
    /// Edits the tool's settings and returns whether they changed. The properties dialog shows
    /// this below the rows [`RegisteredDrawingTool::style`] keeps and applies all edits together.
    /// `DrawingManager::show_custom_settings` shows it alone as one undoable edit. Tools without
    /// settings keep the default.
    fn settings(&self, _ui: &mut Ui, _settings: &mut serde_json::Value) -> bool {
        false
    }
}

/// Whether a built-in tool appears in a picker and can create new drawings. Existing drawings
/// remain visible and editable, so hiding a tool never makes a saved annotation disappear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

/// Built-in style rows offered for a registered tool. Turn off values the renderer does not
/// read; every row is on by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StyleControls {
    /// The line colour.
    pub color: bool,
    /// The line width.
    pub line_width: bool,
    /// The line pattern.
    pub line_style: bool,
    /// The start and end styles.
    pub line_ends: bool,
    /// The fill switch and colour.
    pub fill: bool,
}

impl Default for StyleControls {
    fn default() -> Self {
        Self {
            color: true,
            line_width: true,
            line_style: true,
            line_ends: true,
            fill: true,
        }
    }
}

/// A registered tool's name, anchor count, implementation and style controls. Build with
/// [`Self::new`] and assign the optional presentation fields.
///
/// ```
/// use egui_charts::drawings::{ChartView, Drawing, DrawingRenderer, DrawingToolRegistry, RegisteredDrawingTool};
/// use std::sync::Arc;
///
/// struct Dot;
/// impl DrawingRenderer for Dot {
///     fn render(&self, _: &Drawing, _: &egui::Painter, _: egui::Rect, _: &ChartView) {}
///     fn hit_test(&self, _: &Drawing, _: egui::Pos2, _: f32) -> bool { false }
/// }
/// let mut tool = RegisteredDrawingTool::new("Dot", 1, Arc::new(Dot));
/// tool.style.fill = false;
/// let mut registry = DrawingToolRegistry::default();
/// registry.register("dot", tool).unwrap();
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct RegisteredDrawingTool {
    /// Picker label supplied by the host.
    pub name: String,
    /// Exact number of clicked anchors required to complete the construction.
    pub anchors: usize,
    /// Shared implementation, separate from each drawing's saved settings.
    pub implementation: Arc<dyn DrawingRenderer>,
    /// Built-in style rows the properties dialog offers; all by default.
    pub style: StyleControls,
}

impl RegisteredDrawingTool {
    /// A tool named `name` that completes after `anchors` clicks, with all style rows enabled.
    pub fn new(
        name: impl Into<String>,
        anchors: usize,
        implementation: Arc<dyn DrawingRenderer>,
    ) -> Self {
        Self {
            name: name.into(),
            anchors,
            implementation,
            style: StyleControls::default(),
        }
    }
}

impl std::fmt::Debug for RegisteredDrawingTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisteredDrawingTool")
            .field("name", &self.name)
            .field("anchors", &self.anchors)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Per-manager catalogue. Register stable host-owned string ids before loading drawings.
/// Unknown custom ids stay in storage but are not rendered or hit until registered.
/// Clone the catalogue into a host picker to share implementations and availability.
#[derive(Debug, Clone, Default)]
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

    /// The tool name for a drawing: its registered name, or its saved id if the tool is
    /// missing or unnamed, otherwise the built-in name. Lists may put the drawing's label first.
    pub fn tool_name<'a>(&'a self, drawing: &'a Drawing) -> &'a str {
        match drawing.custom_tool.as_deref() {
            Some(id) if !id.trim().is_empty() => self.custom_name(id),
            _ => drawing.tool_type.as_str(),
        }
    }

    /// A registered tool's picker name, or its id when missing or unnamed.
    pub(crate) fn custom_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.get(id)
            .map(|tool| tool.name.as_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(id)
    }

    /// Hides or disables a built-in tool. Hidden tools may still be created by the host when
    /// enabled; disabling rejects new creation through the manager and chart APIs. Duplicates,
    /// pastes and copies of its drawings are left out; restoring and syncing them is not affected.
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
pub(crate) mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingManager};

    pub(crate) struct Midpoint;

    pub(crate) fn midpoint() -> RegisteredDrawingTool {
        RegisteredDrawingTool::new("Midpoint", 2, Arc::new(Midpoint))
    }
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
        fn settings(&self, ui: &mut Ui, settings: &mut serde_json::Value) -> bool {
            ui.label("Radius");
            *settings = serde_json::json!({"radius": 10});
            true
        }
    }

    /// Registered names reach the dialog and inventory; labels still come first in lists.
    #[cfg(feature = "ui")]
    #[test]
    fn registered_tool_drawings_carry_the_registered_name() {
        let mut registry = DrawingToolRegistry::default();
        registry.register("midpoint", midpoint()).unwrap();
        let mut chart = crate::ChartBuilder::new()
            .with_drawing_registry(registry)
            .build();
        chart.set_symbol("AAPL");
        chart.set_data(
            crate::widget::SeriesId::MAIN,
            crate::widget::test_frames::chart(&[90.0], &[110.0])
                .bars(crate::widget::SeriesId::MAIN)
                .to_vec(),
        );
        let mut drawing = Drawing::custom("midpoint");
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(0.0, 105.0)];
        let id = chart.drawings_mut().add_drawing(drawing).unwrap();
        let mut dialog = crate::ui::dialogs::drawing_properties::DrawingPropertiesDialog::new();
        assert!(dialog.open_for_chart(
            &chart,
            crate::ui::dialogs::drawing_properties::DrawingId(id)
        ));
        assert_eq!(dialog.drawing_name, "Midpoint");
        let inventory = crate::widget::ChartLayout::default().drawing_inventory(&[&mut chart]);
        assert_eq!(inventory[0].drawings[0].name(), "Midpoint");
        use crate::ui::widget_bar::panels::object_tree::SourceItem;
        let d = &chart.drawings().drawings[0];
        assert_eq!(
            SourceItem::for_drawing(&chart, d).display_name(),
            "Midpoint"
        );
        chart.drawings_mut().drawings[0].label = Some("Mine".into());
        let d = &chart.drawings().drawings[0];
        assert_eq!(SourceItem::for_drawing(&chart, d).display_name(), "Mine");
        for (id, name) in [("gone", "gone"), ("", "Polyline"), ("  ", "Polyline")] {
            assert_eq!(
                chart.drawings().registry.tool_name(&Drawing::custom(id)),
                name
            );
        }
        let mut blank = midpoint();
        blank.name = "  ".into();
        chart
            .drawings_mut()
            .registry
            .register("blank", blank)
            .unwrap();
        assert_eq!(
            chart
                .drawings()
                .registry
                .tool_name(&Drawing::custom("blank")),
            "blank"
        );
    }

    /// A saved polyline default leaves registered tools in their own starting style.
    #[test]
    fn registered_tools_keep_their_own_default_style() {
        let mut manager = DrawingManager::new();
        manager.registry.register("midpoint", midpoint()).unwrap();
        let mut polyline = Drawing::new(0, DrawingToolType::Polyline);
        polyline.stroke_width = 7.0;
        polyline.color = [1, 2, 3, 255];
        manager.tool_defaults.insert(
            DrawingToolType::Polyline,
            crate::drawings::DrawingTemplate::from_drawing(&polyline),
        );
        let place = |manager: &mut DrawingManager| {
            manager.set_custom_tool("midpoint").unwrap();
            manager.start_drawing_with_coords(
                DrawingToolType::Polyline,
                egui::pos2(100., 100.),
                |x| x,
                f64::from,
            );
            manager.add_point_with_coords(egui::pos2(200., 100.), |x| x, f64::from);
            manager.drawings.last().unwrap().clone()
        };
        let first = place(&mut manager);
        assert_eq!(
            (first.color, first.stroke_width, first.fill_color),
            ([0, 188, 212, 255], 2.0, Some([0, 188, 212, 51]))
        );
        let mut own = first.clone();
        own.stroke_width = 4.0;
        own.custom_settings = serde_json::json!({"radius": 3});
        manager.save_tool_default(crate::drawings::DrawingTemplate::from_drawing(&own));
        let second = place(&mut manager);
        assert_eq!(
            (second.stroke_width, second.custom_settings),
            (4.0, serde_json::json!({"radius": 3}))
        );
        assert_eq!(
            manager
                .new_drawing(0, DrawingToolType::Polyline)
                .stroke_width,
            7.0
        );
        // Templates are checked against their identity even if the host used the wrong key.
        manager.custom_tool_defaults.insert(
            "other".into(),
            crate::drawings::DrawingTemplate::from_drawing(&own),
        );
        assert_eq!(manager.new_custom_drawing(0, "other").stroke_width, 2.0);
        let mut chart = crate::ChartBuilder::new()
            .with_drawing_registry(manager.registry.clone())
            .build();
        chart.drawings_mut().custom_tool_defaults = manager.custom_tool_defaults.clone();
        chart.set_symbol("A");
        chart.set_symbol("B");
        chart.load_drawings(Vec::new());
        assert_eq!(
            chart
                .drawings()
                .new_custom_drawing(0, "midpoint")
                .stroke_width,
            4.0
        );
    }

    #[test]
    fn registered_tools_use_host_geometry_and_retain_settings_through_history_and_storage() {
        let mut manager = DrawingManager::new();
        manager.registry.register("midpoint", midpoint()).unwrap();
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
                    price_formatter: None,
                    thousands_sep: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                    bar_to_x: &|x| x,
                    bar_spacing: 1.0,
                    price_to_y: &|y| y as f32,
                    y_to_price: &f64::from,
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
        let mut chart = crate::ChartBuilder::new()
            .with_drawing_registry(manager.registry.clone())
            .build();
        chart.set_data(
            crate::widget::SeriesId::MAIN,
            crate::widget::test_frames::chart(&[90.0], &[110.0])
                .bars(crate::widget::SeriesId::MAIN)
                .to_vec(),
        );
        let mut exported = Drawing::custom("midpoint");
        exported.chart_points = vec![ChartPoint::new(60.0, 100.0), ChartPoint::new(80.0, 100.0)];
        chart.drawings_mut().add_drawing(exported).unwrap();
        let mut image = chart.export_image(&ctx, &Default::default()).unwrap();
        image.textures_delta.clear();
        assert!(
            image.primitives.iter().any(|primitive| matches!(
                &primitive.primitive, egui::epaint::Primitive::Mesh(mesh)
                    if mesh.vertices.iter().any(|vertex| vertex.color == egui::Color32::RED)
            )),
            "custom drawing missing from export"
        );
        chart.set_symbol("A");
        chart.drawings_mut().set_custom_tool("midpoint").unwrap();
        chart.set_symbol("B");
        chart.drawings_mut().set_active_tool(None);
        chart.set_symbol("A");
        assert_eq!(chart.drawings().active_custom_tool(), None);
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
        assert!(!manager.set_active_tool(Some(DrawingToolType::TrendLine)));
        assert_eq!(manager.active_tool, None);
        manager.start_drawing_with_coords(DrawingToolType::TrendLine, Pos2::ZERO, |x| x, f64::from);
        assert!(manager.curr_drawing.is_none());
        let mut disabled = Drawing::new(0, DrawingToolType::TrendLine);
        disabled.chart_points = vec![ChartPoint::new(1., 1.), ChartPoint::new(2., 2.)];
        assert!(manager.add_drawing(disabled).is_err());
    }
}
