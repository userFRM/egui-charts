//! Drawing properties dialog - modal popup for editing drawing properties.
//!
//! Provides a properties dialog for drawings with tabs:
//! - Style: Color, line width, line style, fill options
//! - Inputs: Saved tool-specific levels, sizing and calculation settings
//! - Coordinates: Price and time points
//! - Visibility: Timeframe visibility settings
//! - Text: Labels and font settings
//!
//! Open with [`DrawingPropertiesDialog::open_for_chart`] and call
//! [`DrawingPropertiesDialog::show_and_apply`] each frame to apply validated edits in the library.
//! Hosts persist [`crate::drawings::DrawingManager::tool_defaults`] and may attach storage to
//! [`DrawingPropertiesDialog::templates`]. Delete and Clone are in the drawing context menu.

mod actions;
mod config;
mod dialog;
mod tabs;

pub use actions::{DrawingId, DrawingPropertiesAction, DrawingProps};
pub use config::DrawingPropertiesConfig;
pub use dialog::DrawingPropertiesDialog;
pub use tabs::{CoordinatePoint, CoordinatesState, PropertiesTab};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType as Tool};
    use crate::model::{Bar, BarData};
    use crate::theme::Theme;
    use crate::widget::{Chart, SeriesId};
    use chrono::{TimeZone, Utc};

    /// Runs a frame of the dialog and returns the text it laid out, with its bounds.
    fn frame(
        dialog: &mut DrawingPropertiesDialog,
        chart: &mut Chart,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                dialog.show_and_apply(ui.ctx(), chart);
            },
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.job.text.clone(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    fn click(
        dialog: &mut DrawingPropertiesDialog,
        chart: &mut Chart,
        ctx: &egui::Context,
        point: egui::Pos2,
    ) {
        frame(dialog, chart, ctx, vec![egui::Event::PointerMoved(point)]);
        let buttons = [true, false].map(|pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        frame(dialog, chart, ctx, buttons.to_vec());
    }

    fn enter(dialog: &mut DrawingPropertiesDialog, chart: &mut Chart, ctx: &egui::Context) {
        for pressed in [true, false] {
            let key = egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            frame(dialog, chart, ctx, vec![key]);
        }
    }

    /// Family pages fit both themes and narrow windows; coordinate drafts apply as one undoable edit.
    #[test]
    fn family_inputs_and_coordinates_apply_through_dialog() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::new());
        let mut dialog = DrawingPropertiesDialog::new();
        // Times 0, 60, 180 seconds: 120 is halfway between bars 1 and 2, so index 1.5.
        chart.set_data(
            SeriesId::MAIN,
            [0, 60, 180]
                .map(|seconds| {
                    Bar::new(
                        Utc.timestamp_opt(seconds, 0).unwrap(),
                        100.0,
                        120.0,
                        90.0,
                        110.0,
                        1.0,
                    )
                })
                .to_vec(),
        );
        for (tool, heading) in [
            (Tool::FibonacciRetracement, "Fibonacci levels"),
            (Tool::FibonacciExtension, "Fibonacci levels"),
            (Tool::FibonacciChannel, "Fibonacci levels"),
            (Tool::FibonacciWedge, "Fibonacci levels"),
            (Tool::FibonacciTimeZones, "Fibonacci levels"),
            (Tool::FibonacciSpeedFan, "Fibonacci levels"),
            (Tool::TrendBasedFibTime, "Fibonacci levels"),
            (Tool::FibonacciCircles, "Fibonacci levels"),
            (Tool::FibonacciSpeedResistanceArcs, "Fibonacci levels"),
            (Tool::Pitchfork, "Pitchfork levels"),
            (Tool::SchiffPitchfork, "Pitchfork levels"),
            (Tool::ModifiedSchiffPitchfork, "Pitchfork levels"),
            (Tool::InsidePitchfork, "Pitchfork levels"),
            (Tool::GannBox, "Price levels"),
            (Tool::GannSquare, "Price levels"),
            (Tool::GannFixed, "Price levels"),
            (Tool::GannFan, "Fan lines"),
            (Tool::LongPos, "Entry price"),
            (Tool::ShortPos, "Entry price"),
            (Tool::FixedRangeVolumeProfile, "Rows"),
            (Tool::AnchoredVolumeProfile, "Rows"),
            (Tool::RegressionTrend, "Deviations"),
            (Tool::ElliottImpulse, "Degree"),
            (Tool::ElliottCorrection, "Degree"),
            (Tool::ElliottTriangle, "Degree"),
            (Tool::ElliottDoubleCombo, "Degree"),
            (Tool::ElliottTripleCombo, "Degree"),
        ] {
            chart.drawings_mut().drawings.clear();
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(0.0, 100.0),
                ChartPoint::new(2.0, 120.0),
                ChartPoint::new(1.0, 95.0),
            ];
            let before = format!("{drawing:?}");
            chart.drawings_mut().drawings.push(drawing);
            dialog.open_for_chart(&chart, DrawingId(7));
            assert_eq!(
                dialog.coordinates.points.len(),
                3,
                "{tool:?}: every anchor must be editable"
            );
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
            let inputs = texts
                .iter()
                .find(|(text, _)| text == "Inputs")
                .unwrap_or_else(|| panic!("{tool:?}: tool-specific Inputs tab: {texts:?}"))
                .1
                .center();
            click(&mut dialog, &mut chart, &ctx, inputs);
            let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
            assert!(
                texts.iter().any(|(text, _)| text == heading),
                "{tool:?}: {texts:?}"
            );
            assert_eq!(
                format!("{:?}", chart.drawings().drawings[0]),
                before,
                "visiting a page must not change defaults"
            );
            for (theme, width) in [(Theme::light(), 320.0), (Theme::dark(), 360.0)] {
                let narrow = egui::Context::default();
                crate::theme::apply_to_egui(&narrow, &theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 640.0));
                let tabs = if tool == Tool::FibonacciRetracement {
                    PropertiesTab::all().to_vec()
                } else {
                    vec![PropertiesTab::Inputs]
                };
                for tab in tabs {
                    dialog.active_tab = tab;
                    for pass in 0..3 {
                        let mut output = narrow.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| {
                                dialog.show_and_apply(ui.ctx(), &mut chart);
                            },
                        );
                        output.textures_delta.clear();
                        if pass > 0 {
                            let area = narrow
                                .memory(|memory| {
                                    memory.area_rect(egui::Id::unique(Some("Drawing Properties")))
                                })
                                .unwrap();
                            assert!(
                                screen.contains_rect(area),
                                "{tool:?}, {tab:?}: dialog exceeds the window: {area:?}"
                            );
                        }
                        let mut footer = Vec::new();
                        for shape in &output.shapes {
                            if let egui::Shape::Text(text) = &shape.shape
                                && matches!(
                                    text.galley.job.text.as_str(),
                                    "Ok" | "Cancel" | "Template"
                                )
                            {
                                let bounds =
                                    egui::Rect::from_min_size(text.pos, text.galley.size());
                                if matches!(
                                    text.galley.job.text.as_str(),
                                    "Ok" | "Cancel" | "Template"
                                ) {
                                    footer.push(bounds);
                                }
                                assert!(
                                    screen.left() <= bounds.left()
                                        && screen.right() >= bounds.right(),
                                    "{tool:?}: control outside narrow window: {bounds:?}"
                                );
                                assert!(
                                    shape.clip_rect.left() <= bounds.left()
                                        && shape.clip_rect.right() >= bounds.right(),
                                    "{tool:?}: horizontally clipped control: {bounds:?}, clip: {:?}",
                                    shape.clip_rect
                                );
                            }
                        }
                        for (index, bounds) in footer.iter().enumerate() {
                            assert!(
                                footer[index + 1..]
                                    .iter()
                                    .all(|other| !bounds.intersects(*other)),
                                "{tool:?}: overlapping footer controls: {footer:?}"
                            );
                        }
                        output.drop_without_applying_deltas();
                    }
                }
            }
            dialog.close();
        }
        chart.drawings_mut().drawings[0].tool_type = Tool::TrendLine;
        dialog.open_for_chart(&chart, DrawingId(7));
        dialog.active_tab = PropertiesTab::Coordinates;
        assert_eq!(dialog.coordinates.points[1].time, "00:03:00");
        dialog.coordinates.points[1].price = "123.5".into();
        dialog.coordinates.points[1].time = "00:02".into();
        dialog.coordinates.points[0].price = "NaN".into();
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        enter(&mut dialog, &mut chart, &ctx);
        assert!(dialog.is_open, "invalid drafts remain editable");
        assert_eq!(chart.drawings().drawings[0].chart_points[1].price, 120.0);
        dialog.coordinates.points[0].price = "100".into();
        enter(&mut dialog, &mut chart, &ctx);
        assert!(
            !dialog.is_open,
            "{:?}",
            frame(&mut dialog, &mut chart, &ctx, vec![])
        );
        assert_eq!(
            chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(1.5, 123.5)
        );
        assert!(chart.undo());
        assert_eq!(
            chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(2.0, 120.0)
        );
        assert!(chart.redo());
        dialog.open_for_chart(&chart, DrawingId(7));
        dialog.coordinates.points[1].price = "124.5".into();
        // History can arrive while the draft is open. The timestamp stays at 120 seconds,
        // now halfway between indices 2 and 3; untouched anchors must move with history too.
        chart.set_data(
            SeriesId::MAIN,
            [-60, 0, 60, 180]
                .map(|seconds| {
                    Bar::new(
                        Utc.timestamp_opt(seconds, 0).unwrap(),
                        100.0,
                        120.0,
                        90.0,
                        110.0,
                        1.0,
                    )
                })
                .to_vec(),
        );
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        enter(&mut dialog, &mut chart, &ctx);
        assert_eq!(
            chart.drawings().drawings[0].chart_points[1],
            ChartPoint::new(2.5, 124.5)
        );
        assert_eq!(chart.drawings().drawings[0].chart_points[0].bar_idx, 1.0);
        chart.drawings_mut().drawings[0].visible = false;
        dialog.open_for_chart(&chart, DrawingId(7));
        dialog.props.line_width = 4.0;
        let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
        let menu = texts
            .iter()
            .find(|(text, _)| text == "Template")
            .unwrap()
            .1
            .center();
        click(&mut dialog, &mut chart, &ctx, menu);
        let texts = frame(&mut dialog, &mut chart, &ctx, vec![]);
        let save = texts
            .iter()
            .find(|(text, _)| text == "Save as default for this tool")
            .unwrap()
            .1
            .center();
        click(&mut dialog, &mut chart, &ctx, save);
        enter(&mut dialog, &mut chart, &ctx);
        chart.drawings_mut().start_drawing_with_coords(
            Tool::TrendLine,
            egui::pos2(0.0, 100.0),
            |x| x,
            |y| y as f64,
        );
        assert_eq!(
            chart.drawings().curr_drawing.as_ref().unwrap().stroke_width,
            4.0
        );
        assert_eq!(
            chart
                .drawings()
                .curr_drawing
                .as_ref()
                .unwrap()
                .chart_points
                .len(),
            1,
            "defaults cannot copy anchors"
        );
        assert!(
            chart.drawings().curr_drawing.as_ref().unwrap().visible,
            "a hidden source must not make new drawings invisible"
        );
        assert!(!chart.drawings().drawings[0].visible);
        chart.drawings_mut().cancel_curr_drawing();
        chart.drawings_mut().start_drawing_with_coords(
            Tool::HorizontalLine,
            egui::pos2(0.0, 100.0),
            |x| x,
            |y| y as f64,
        );
        assert_eq!(
            chart.drawings().drawings.last().unwrap().stroke_width,
            2.0,
            "defaults belong to one tool"
        );
    }
}
