//! Style tab - color, line width, line style.

use egui::{Color32, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo};

use crate::ext::UiExt;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::{DrawingLineStyle, DrawingProps};
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::{ColorPicker, ColorPickerState, FormGrid};

/// Line widths offered as swatches; the stepper next to them takes any width.
const LINE_WIDTHS: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

/// Style tab renderer
pub struct StyleTab;

impl StyleTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps, pickers: &mut [ColorPickerState; 2]) {
        let [color_picker, fill_picker] = pickers;
        ui.space_md();

        FormGrid::new("style_grid").show(ui, |ui| {
            ui.label("Color");
            if let Some(color) = color_swatch(ui, color_picker, props.color, "drawing_props_color")
            {
                props.color = color;
            }
            ui.end_row();

            ui.label("Line width");
            line_width_row(ui, &mut props.line_width);
            ui.end_row();

            ui.label("Line style");
            ui.combo_select(
                "line_style",
                &mut props.line_style,
                DrawingLineStyle::all(),
                |v| v.label().to_string(),
            );
            ui.end_row();

            ui.label("Fill");
            ui.checkbox(&mut props.fill_enabled, "Enable fill");
            ui.end_row();

            if props.fill_enabled {
                ui.label("Fill color");
                let fill = props
                    .fill_color
                    .unwrap_or(ui.active_theme().ui.accent.gamma_multiply(0.2));
                if let Some(picked) = color_swatch(ui, fill_picker, fill, "drawing_props_fill") {
                    // Palette swatches are opaque: keep the fill's opacity unless it was edited.
                    let [r, g, b, a] = picked.to_srgba_unmultiplied();
                    let a = if fill_picker.custom_mode { a } else { fill.a() };
                    props.fill_color = Some(Color32::from_rgba_unmultiplied(r, g, b, a));
                }
                ui.end_row();
            }

            ui.label("Extend");
            ui.horizontal(|ui| {
                ui.checkbox(&mut props.extend_left, "Left");
                ui.add_space(ui.active_theme().spacing.xxl - ui.spacing().item_spacing.x);
                ui.checkbox(&mut props.extend_right, "Right");
            });
            ui.end_row();

            ui.label("Labels");
            ui.checkbox(&mut props.show_price, "Show price");
            ui.end_row();
        });
    }
}

/// Kit colour swatch for `color`; returns the colour picked this frame.
fn color_swatch(
    ui: &mut Ui,
    state: &mut ColorPickerState,
    color: Color32,
    id: &str,
) -> Option<Color32> {
    if !state.is_open {
        // The custom editor starts from the current colour, not the state's transparent default.
        state.custom_color = color;
    }
    state.sel_color = color;
    ColorPicker::new(state, id).show(ui)
}

/// Swatches that draw each preset width, then a stepper for any width from 1 to 10 px.
fn line_width_row(ui: &mut Ui, width: &mut f32) {
    let theme = ui.active_theme();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing.xs;
        for preset in LINE_WIDTHS {
            let size = Vec2::splat(ButtonSize::MD.pixels());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            if response.clicked() {
                *width = preset;
            }
            let selected = *width == preset;
            response.widget_info(|| {
                WidgetInfo::selected(
                    Role::Button,
                    ui.is_enabled(),
                    selected,
                    format!("{preset} px"),
                )
            });

            let visuals = ui.visuals();
            let radius = DESIGN_TOKENS.rounding.md;
            if selected {
                ui.painter()
                    .rect_filled(rect, radius, visuals.selection.bg_fill);
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(rect, radius, visuals.widgets.hovered.bg_fill);
            }
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, theme.ui.border_control),
                StrokeKind::Inside,
            );
            crate::styles::focus::draw_focus_ring(ui, &response, radius);
            let line = if selected {
                visuals.selection.stroke.color
            } else {
                visuals.text_color()
            };
            ui.painter().hline(
                rect.x_range().shrink(theme.spacing.md),
                rect.center().y,
                Stroke::new(preset, line),
            );
        }
        ui.add_space(theme.spacing.lg - theme.spacing.xs);
        ui.add(
            egui::DragValue::new(width)
                .range(1.0..=10.0)
                .speed(0.5)
                .max_decimals(1)
                .custom_formatter(|v, _| v.to_string())
                .suffix(" px"),
        );
    });
}
