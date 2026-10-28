//! Colour swatch popup of the drawing toolbar.

use super::DrawingToolbar;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::ext::UiExt;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

impl DrawingToolbar {
    /// Draw color picker popup
    pub(super) fn draw_color_picker_popup(
        &mut self,
        ui: &mut Ui,
        sidebar_rect: Rect,
    ) -> Option<[u8; 4]> {
        let popup_pos = Pos2::new(
            sidebar_rect.right() + ui.active_theme().spacing.sm,
            sidebar_rect.center().y - DESIGN_TOKENS.sizing.dialog.color_picker_height / 2.0,
        );
        let current_color = self.state.drawing_color;

        let (area_res, sel_color) = self.render_color_picker_area(ui, popup_pos, current_color);

        self.handle_color_picker_close(ui, &area_res, sidebar_rect, sel_color.is_some());

        sel_color
    }

    /// Render the color picker area and return selected color if any
    fn render_color_picker_area(
        &self,
        ui: &mut Ui,
        popup_pos: Pos2,
        current_color: [u8; 4],
    ) -> (egui::InnerResponse<()>, Option<[u8; 4]>) {
        let mut sel_color = None;

        let area_res = egui::Area::new(egui::Id::unique("color_picker_popup"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                sel_color = self.render_color_picker_frame(ui, current_color);
            });

        (area_res, sel_color)
    }

    /// Render the color picker frame content
    fn render_color_picker_frame(&self, ui: &mut Ui, current_color: [u8; 4]) -> Option<[u8; 4]> {
        let popup_width = DESIGN_TOKENS.sizing.dialog.color_picker_width;
        let popup_height = DESIGN_TOKENS.sizing.dialog.color_picker_height;
        let mut sel_color = None;

        egui::Frame::popup(ui.style())
            .fill(ui.style().visuals.window_fill())
            .stroke(ui.style().visuals.window_stroke)
            .corner_radius(ui.active_theme().radius.lg)
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(popup_width, popup_height));
                ui.space_lg();
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), "Drawing Color"))
                        .size(ui.active_theme().text.small),
                );
                ui.space_md();

                sel_color = self.render_color_swatches(ui, current_color);
            });

        sel_color
    }

    /// Render the color swatch grid and return selected color if clicked
    fn render_color_swatches(&self, ui: &mut Ui, current_color: [u8; 4]) -> Option<[u8; 4]> {
        let colors = Self::color_palette();
        let swatch_size = DESIGN_TOKENS.sizing.button_sm;
        let swatch_spacing = ui.active_theme().spacing.sm;
        let columns = 5;
        let mut sel_color = None;

        for row in 0..4 {
            ui.horizontal(|ui| {
                ui.space_lg();
                sel_color = sel_color.or(self.render_swatch_row(
                    ui,
                    &colors,
                    row,
                    columns,
                    swatch_size,
                    swatch_spacing,
                    current_color,
                ));
            });
            ui.add_space(swatch_spacing);
        }

        sel_color
    }

    /// Render a single row of color swatches
    fn render_swatch_row(
        &self,
        ui: &mut Ui,
        colors: &[([u8; 4], &str)],
        row: usize,
        columns: usize,
        swatch_size: f32,
        swatch_spacing: f32,
        current_color: [u8; 4],
    ) -> Option<[u8; 4]> {
        let mut sel_color = None;

        for col in 0..columns {
            let idx = row * columns + col;
            if idx >= colors.len() {
                break;
            }

            let (color_val, name) = &colors[idx];
            if let Some(clicked) =
                self.render_single_swatch(ui, *color_val, name, swatch_size, current_color)
            {
                sel_color = Some(clicked);
            }
            ui.add_space(swatch_spacing);
        }

        sel_color
    }

    /// Render a single color swatch and return its color if clicked
    fn render_single_swatch(
        &self,
        ui: &mut Ui,
        color_val: [u8; 4],
        name: &str,
        swatch_size: f32,
        current_color: [u8; 4],
    ) -> Option<[u8; 4]> {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(swatch_size), Sense::click());
        let translated_label = l10n::text(ui.ctx(), name);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                response.enabled(),
                current_color == color_val,
                &translated_label,
            )
        });
        crate::styles::focus::draw_focus_ring(ui, &response, ui.active_theme().radius.sm);

        let c =
            Color32::from_rgba_unmultiplied(color_val[0], color_val[1], color_val[2], color_val[3]);
        ui.painter()
            .rect_filled(rect, ui.active_theme().radius.sm, c);

        self.render_swatch_border(ui, rect, &response, current_color == color_val);
        let clicked = response.clicked();
        response.on_hover_text(l10n::text(ui.ctx(), name));

        if clicked { Some(color_val) } else { None }
    }

    /// Render border for a color swatch based on selection/hover state
    fn render_swatch_border(
        &self,
        ui: &mut Ui,
        rect: Rect,
        response: &egui::Response,
        is_selected: bool,
    ) {
        if is_selected {
            ui.painter().rect_stroke(
                rect,
                ui.active_theme().radius.sm,
                Stroke::new(DESIGN_TOKENS.stroke.thick, ui.active_theme().ui.accent),
                egui::StrokeKind::Outside,
            );
        } else if response.hovered() {
            let hover_color = ui.style().visuals.widgets.hovered.fg_stroke.color;
            ui.painter().rect_stroke(
                rect,
                ui.active_theme().radius.sm,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, hover_color),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Handle closing the color picker on outside click or selection
    fn handle_color_picker_close(
        &mut self,
        ui: &mut Ui,
        area_res: &egui::InnerResponse<()>,
        sidebar_rect: Rect,
        color_selected: bool,
    ) {
        let clicked_outside = ui.input(|i| i.pointer.hover_pos()).is_some_and(|pos| {
            ui.input(|i| i.pointer.primary_clicked())
                && !area_res.response.rect.contains(pos)
                && !sidebar_rect.contains(pos)
        });

        if clicked_outside || color_selected {
            self.state.show_color_picker = false;
        }
    }

    /// Get the color palette for the color picker
    fn color_palette() -> Vec<([u8; 4], &'static str)> {
        vec![
            ([242, 54, 69, 255], "Red"),
            ([255, 82, 82, 255], "Light Red"),
            ([255, 109, 0, 255], "Orange"),
            ([255, 152, 0, 255], "Amber"),
            ([255, 235, 59, 255], "Yellow"),
            ([38, 166, 154, 255], "Teal"),
            ([76, 175, 80, 255], "Green"),
            ([139, 195, 74, 255], "Light Green"),
            ([205, 220, 57, 255], "Lime"),
            ([0, 150, 136, 255], "Cyan"),
            ([33, 150, 243, 255], "Blue"),
            ([41, 98, 255, 255], "Indigo"),
            ([103, 58, 183, 255], "Deep Purple"),
            ([156, 39, 176, 255], "Purple"),
            ([233, 30, 99, 255], "Pink"),
            ([255, 255, 255, 255], "White"),
            ([178, 181, 190, 255], "Gray"),
            ([120, 123, 134, 255], "Dark Gray"),
            ([66, 66, 66, 255], "Charcoal"),
            ([0, 0, 0, 255], "Black"),
        ]
    }
}
