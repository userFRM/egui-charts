//! A drawing tool the demo registers as a host crate would: a round pin with one setting.

use egui_charts::drawings::rendering::ChartView;
use egui_charts::drawings::{Drawing, DrawingRenderer, DrawingToolRegistry, RegisteredDrawingTool};
use std::sync::Arc;

/// The saved id of the pin's drawings.
pub(super) const PIN: &str = "demo-pin";

/// The toolbar picture, drawn like the built-in icons: white strokes the toolbar tints.
const PIN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" color="#fff" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="9" r="4.5"/><path d="M12 13.5V21"/></svg>"##;

struct Pin;

impl Pin {
    /// The radius in points, doubled by the drawing's "large" setting.
    fn radius(drawing: &Drawing) -> f32 {
        let large = drawing
            .custom_settings
            .get("large")
            .and_then(serde_json::Value::as_bool);
        if large == Some(true) { 12.0 } else { 6.0 }
    }
}

impl DrawingRenderer for Pin {
    fn render(&self, drawing: &Drawing, painter: &egui::Painter, _: egui::Rect, _: &ChartView) {
        if let Some(&at) = drawing.points.first() {
            let [r, g, b, a] = drawing.color;
            painter.circle_filled(
                at,
                Self::radius(drawing),
                egui::Color32::from_rgba_unmultiplied(r, g, b, a),
            );
        }
    }

    fn hit_test(&self, drawing: &Drawing, point: egui::Pos2, tolerance: f32) -> bool {
        drawing
            .points
            .first()
            .is_some_and(|at| at.distance(point) <= Self::radius(drawing) + tolerance)
    }

    fn settings(&self, ui: &mut egui::Ui, settings: &mut serde_json::Value) -> bool {
        let mut large = settings
            .get("large")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let changed = ui.checkbox(&mut large, "Large").changed();
        if changed {
            // The whole value: indexing into a saved value that is not an object would panic.
            *settings = serde_json::json!({ "large": large });
        }
        changed
    }
}

/// The demo's registered tools: the pin, in the Text/Annotations flyout with its own picture.
pub(super) fn registry() -> DrawingToolRegistry {
    let mut tool = RegisteredDrawingTool::new("Pin marker", 1, Arc::new(Pin));
    tool.category = Some("Text/Annotations".into());
    tool.icon = Some(egui_charts::icons::Icon::new("demo/pin.svg", PIN_SVG));
    // The pin reads only its colour.
    tool.style.line_width = false;
    tool.style.line_style = false;
    tool.style.line_ends = false;
    tool.style.fill = false;
    let mut registry = DrawingToolRegistry::default();
    registry.register(PIN, tool).expect("the pin's id is free");
    registry
}

#[cfg(test)]
mod tests {
    use super::super::tests::{AppStorage, settle};
    use super::super::{DemoApp, TopToolbarAction};
    use super::PIN;
    use eframe::App as _;
    use egui::accesskit::Role;

    /// The registered pin is picked from quick search and from its toolbar flyout, drawn,
    /// edited in its properties dialog, undone, redone and opened again after a restart: #4162.
    #[test]
    fn registered_pin_is_picked_drawn_edited_undone_and_reopened() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = DemoApp::new_at(&cc, "2026-11-10T19:29:50Z".parse().unwrap(), true);
        settle(&mut app, &ctx);
        let frame = |app: &mut DemoApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
        };
        let press = |app: &mut DemoApp, pos| {
            frame(app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
            for pressed in [true, false] {
                let button = egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(app, vec![button]).drop_without_applying_deltas();
            }
        };
        // The control `role` labelled `label`, the leftmost when the label repeats.
        let click = |app: &mut DemoApp, role, label: &str| {
            let output = frame(app, vec![]);
            let bounds = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == role && node.label() == Some(label))
                .filter_map(|(_, node)| node.bounds())
                .min_by(|a, b| a.x0.total_cmp(&b.x0))
                .unwrap_or_else(|| panic!("missing control: {label}"));
            output.drop_without_applying_deltas();
            let center = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 * 0.5,
                (bounds.y0 + bounds.y1) as f32 * 0.5,
            );
            press(app, center);
        };
        let pins = |app: &DemoApp| -> Vec<egui_charts::drawings::Drawing> {
            app.chart
                .drawings()
                .drawings
                .iter()
                .filter(|drawing| drawing.custom_tool.as_deref() == Some(PIN))
                .cloned()
                .collect()
        };
        // The radius of the circle painted at the pin's anchor.
        let painted = |app: &mut DemoApp| -> Option<f32> {
            let at = pins(app)[0].points[0];
            let output = frame(app, vec![]);
            let radius = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) if circle.center.distance(at) <= 1.0 => {
                    Some(circle.radius)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            radius
        };
        let large = serde_json::json!({ "large": true });

        let commands = super::super::commands::quick_search_commands(
            app.chart.shortcuts(),
            &[],
            app.indicator_dialog.factory(),
            &[],
            &app.chart.drawings().registry,
        );
        let row = commands
            .iter()
            .find(|row| row.id == "custom-tool:demo-pin")
            .expect("quick search lists the pin");
        assert_eq!(row.label, "Pin marker");
        assert_eq!(row.category.as_deref(), Some("Drawing tools"));
        app.execute_search_command(&ctx, "custom-tool:demo-pin");
        assert_eq!(app.chart.drawings().active_custom_tool(), Some(PIN));
        app.chart.drawings_mut().set_active_tool(None);
        assert_eq!(app.chart.drawings().active_custom_tool(), None);

        click(&mut app, Role::Button, "Text");
        click(&mut app, Role::MenuItem, "Pin marker");
        assert_eq!(app.chart.drawings().active_custom_tool(), Some(PIN));

        // Past the double click interval, below the legend.
        for _ in 0..30 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        assert_eq!(app.chart.drawings().active_custom_tool(), Some(PIN));
        let rect = app.chart.get_rendered_price_rect();
        press(
            &mut app,
            rect.left_center() + egui::vec2(rect.width() / 4.0, 0.0),
        );
        let drawn = pins(&app);
        assert_eq!(
            drawn.len(),
            1,
            "{rect:?} {:?}",
            app.chart
                .drawings()
                .drawings
                .iter()
                .map(|drawing| (drawing.tool_type, drawing.custom_tool.clone()))
                .collect::<Vec<_>>()
        );
        assert!(drawn[0].completed);
        assert_ne!(drawn[0].custom_settings, large);
        assert_eq!(painted(&mut app), Some(6.0));

        app.open_drawing_props_for(drawn[0].id);
        frame(&mut app, vec![]).drop_without_applying_deltas();
        frame(&mut app, vec![]).drop_without_applying_deltas();
        click(&mut app, Role::CheckBox, "Large");
        click(&mut app, Role::Button, "Ok");
        assert_eq!(pins(&app)[0].custom_settings, large);
        assert_eq!(painted(&mut app), Some(12.0));

        app.handle_top_toolbar(&ctx, TopToolbarAction::Undo);
        assert_eq!(pins(&app).len(), 1);
        assert_ne!(pins(&app)[0].custom_settings, large);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Redo);
        assert_eq!(pins(&app)[0].custom_settings, large);

        assert!(app.chart.flush_stored_drawings());
        app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
        app.poll_storage(&ctx);
        let mut storage = AppStorage::default();
        app.save(&mut storage);
        drop(app);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&storage);
        let mut app = DemoApp::new(&cc);
        app.poll_storage(&ctx);
        settle(&mut app, &ctx);
        let reopened = pins(&app);
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened[0].custom_settings, large);
        assert_eq!(
            app.chart.drawings().registry.tool_name(&reopened[0]),
            "Pin marker"
        );
    }
}
