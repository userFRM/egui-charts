//! Color picker popup
//!
//! A color swatch picker with grayscale row, main colors row, and color variations.

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Id, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, epaint::StrokeKind};

// ============================================================================
// COLOR PALETTE - exact colors
// ============================================================================

/// Grayscale colors (row 1)
pub const GRAYSCALE: [Color32; 10] = [
    Color32::from_rgb(255, 255, 255), // white
    Color32::from_rgb(219, 219, 219),
    Color32::from_rgb(184, 184, 184),
    Color32::from_rgb(156, 156, 156),
    Color32::from_rgb(128, 128, 128),
    Color32::from_rgb(99, 99, 99),
    Color32::from_rgb(74, 74, 74),
    Color32::from_rgb(46, 46, 46),
    Color32::from_rgb(15, 15, 15),
    Color32::from_rgb(0, 0, 0), // black
];

/// Main colors (row 2)
pub const MAIN_COLORS: [Color32; 10] = [
    Color32::from_rgb(242, 54, 69),  // red
    Color32::from_rgb(255, 152, 0),  // orange
    Color32::from_rgb(255, 235, 59), // yellow
    Color32::from_rgb(76, 175, 80),  // green
    Color32::from_rgb(8, 153, 129),  // teal
    Color32::from_rgb(0, 188, 212),  // cyan
    Color32::from_rgb(41, 98, 255),  // blue
    Color32::from_rgb(103, 58, 183), // purple
    Color32::from_rgb(156, 39, 176), // magenta
    Color32::from_rgb(233, 30, 99),  // pink
];

/// Color variations - 6 rows from light to dark for each main color
pub const COLOR_VARIATIONS: [[Color32; 10]; 6] = [
    // Row 1 - Lightest
    [
        Color32::from_rgb(252, 203, 205),
        Color32::from_rgb(255, 224, 178),
        Color32::from_rgb(255, 249, 196),
        Color32::from_rgb(200, 230, 201),
        Color32::from_rgb(172, 229, 220),
        Color32::from_rgb(178, 235, 242),
        Color32::from_rgb(187, 217, 251),
        Color32::from_rgb(209, 196, 233),
        Color32::from_rgb(225, 190, 231),
        Color32::from_rgb(248, 187, 208),
    ],
    // Row 2
    [
        Color32::from_rgb(250, 161, 164),
        Color32::from_rgb(255, 204, 128),
        Color32::from_rgb(255, 245, 157),
        Color32::from_rgb(165, 214, 167),
        Color32::from_rgb(112, 204, 189),
        Color32::from_rgb(128, 222, 234),
        Color32::from_rgb(144, 191, 249),
        Color32::from_rgb(179, 157, 219),
        Color32::from_rgb(206, 147, 216),
        Color32::from_rgb(244, 143, 177),
    ],
    // Row 3
    [
        Color32::from_rgb(247, 124, 128),
        Color32::from_rgb(255, 183, 77),
        Color32::from_rgb(255, 241, 118),
        Color32::from_rgb(129, 199, 132),
        Color32::from_rgb(66, 189, 168),
        Color32::from_rgb(77, 208, 225),
        Color32::from_rgb(91, 156, 246),
        Color32::from_rgb(149, 117, 205),
        Color32::from_rgb(186, 104, 200),
        Color32::from_rgb(240, 98, 146),
    ],
    // Row 4
    [
        Color32::from_rgb(247, 82, 95),
        Color32::from_rgb(255, 167, 38),
        Color32::from_rgb(255, 238, 88),
        Color32::from_rgb(102, 187, 106),
        Color32::from_rgb(34, 171, 148),
        Color32::from_rgb(38, 198, 218),
        Color32::from_rgb(49, 121, 245),
        Color32::from_rgb(126, 87, 194),
        Color32::from_rgb(171, 71, 188),
        Color32::from_rgb(236, 64, 122),
    ],
    // Row 5
    [
        Color32::from_rgb(178, 40, 51),
        Color32::from_rgb(245, 124, 0),
        Color32::from_rgb(251, 192, 45),
        Color32::from_rgb(56, 142, 60),
        Color32::from_rgb(5, 102, 86),
        Color32::from_rgb(0, 151, 167),
        Color32::from_rgb(24, 72, 204),
        Color32::from_rgb(81, 45, 168),
        Color32::from_rgb(123, 31, 162),
        Color32::from_rgb(194, 24, 91),
    ],
    // Row 6 - Darkest
    [
        Color32::from_rgb(128, 25, 34),
        Color32::from_rgb(230, 81, 0),
        Color32::from_rgb(245, 127, 23),
        Color32::from_rgb(27, 94, 32),
        Color32::from_rgb(0, 51, 42),
        Color32::from_rgb(0, 96, 100),
        Color32::from_rgb(12, 50, 153),
        Color32::from_rgb(49, 27, 146),
        Color32::from_rgb(74, 20, 140),
        Color32::from_rgb(136, 14, 79),
    ],
];

// ============================================================================
// COLOR PICKER CONFIG
// ============================================================================

/// Configuration for the color picker
#[derive(Clone, Debug)]
pub struct ColorPickerConfig {
    /// Size of each color swatch
    pub swatch_size: f32,
    /// Spacing between swatches
    pub swatch_spacing: f32,
    /// Spacing between rows
    pub row_spacing: f32,
    /// Corner radius for swatches
    pub swatch_rounding: f32,
    /// Popup background color
    pub bg_color: Color32,
    /// Border color for popup
    pub border_color: Color32,
    /// Border color for selected swatch
    pub sel_border_color: Color32,
    /// Popup padding
    pub padding: f32,
}

impl Default for ColorPickerConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            swatch_size: 20.0,
            swatch_spacing: 4.0,
            row_spacing: 4.0,
            swatch_rounding: 3.0,
            bg_color: Color32::TRANSPARENT,
            border_color: Color32::TRANSPARENT,
            sel_border_color: Color32::TRANSPARENT,
            padding: 12.0,
        }
    }
}

// ============================================================================
// COLOR PICKER STATE
// ============================================================================

/// State for a color picker popup
#[derive(Clone, Debug, Default)]
pub struct ColorPickerState {
    /// Whether the popup is open
    pub is_open: bool,
    /// Pos of the popup
    pub popup_pos: Option<Pos2>,
    /// Currently selected color
    pub sel_color: Color32,
    /// Whether user is dragging to select custom color
    pub custom_mode: bool,
    /// Custom color being edited
    pub custom_color: Color32,
    /// Whether popup was just opened this frame (skip close detection)
    pub just_opened: bool,
}

impl ColorPickerState {
    pub fn new(initial_color: Color32) -> Self {
        Self {
            is_open: false,
            popup_pos: None,
            sel_color: initial_color,
            custom_mode: false,
            custom_color: initial_color,
            just_opened: false,
        }
    }
}

// ============================================================================
// COLOR PICKER WIDGET
// ============================================================================

/// Color swatch picker
pub struct ColorPicker<'a> {
    config: ColorPickerConfig,
    state: &'a mut ColorPickerState,
    id: Id,
}

impl<'a> ColorPicker<'a> {
    pub fn new(state: &'a mut ColorPickerState, id_src: impl std::hash::Hash) -> Self {
        Self {
            config: ColorPickerConfig::default(),
            state,
            id: Id::new(id_src),
        }
    }

    pub fn with_config(mut self, config: ColorPickerConfig) -> Self {
        self.config = config;
        self
    }

    /// Show a color swatch button that opens the picker when clicked
    /// Returns the selected color if changed
    pub fn show(mut self, ui: &mut Ui) -> Option<Color32> {
        let mut color_changed = None;

        // Get colors from theme
        let visuals = ui.style().visuals.clone();
        let bg_color = visuals.faint_bg_color;
        let normal_border = visuals.widgets.noninteractive.bg_stroke.color;
        let active_border = visuals.widgets.active.fg_stroke.color;

        // Draw the color swatch button (size: icon_xxl x icon_xxl)
        let btn_size = Vec2::splat(DESIGN_TOKENS.sizing.icon_xxl);
        let (btn_rect, btn_res) = ui.allocate_exact_size(btn_size, Sense::click());

        // Draw dark background then blend color on top
        // This avoids the "checkered pattern" look for semi-transparent colors
        ui.painter()
            .rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, bg_color);

        // Draw the color on top - alpha will blend naturally over dark bg
        ui.painter()
            .rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, self.state.sel_color);

        // Draw border
        let border_color = if btn_res.hovered() || self.state.is_open {
            active_border
        } else {
            normal_border
        };
        ui.painter().rect_stroke(
            btn_rect,
            4.0,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Handle button click
        if btn_res.clicked() {
            self.state.is_open = !self.state.is_open;
            self.state.popup_pos = Some(btn_rect.left_bottom() + Vec2::new(0.0, 4.0));
            if self.state.is_open {
                // Mark as just opened to skip close detection this frame
                self.state.just_opened = true;
            }
        }

        // Show popup if open
        if self.state.is_open {
            color_changed = self.show_popup(ui);
        }

        color_changed
    }

    fn show_popup(&mut self, ui: &mut Ui) -> Option<Color32> {
        let mut color_changed = None;
        let popup_pos = self.state.popup_pos.unwrap_or(ui.cursor().min);

        // Get colors from theme (fall back to config if set, otherwise use visuals)
        let visuals = ui.style().visuals.clone();
        let popup_bg = if self.config.bg_color == Color32::TRANSPARENT {
            visuals.window_fill
        } else {
            self.config.bg_color
        };
        let popup_border = if self.config.border_color == Color32::TRANSPARENT {
            visuals.widgets.noninteractive.bg_stroke.color
        } else {
            self.config.border_color
        };
        let separator_color = visuals.widgets.noninteractive.bg_stroke.color;

        // Calculate popup size
        let cols = 10;
        let total_rows = 1 + 1 + 6 + 1; // grayscale + main + variations + custom button
        let popup_width = self.config.padding * 2.0
            + (self.config.swatch_size * cols as f32)
            + (self.config.swatch_spacing * (cols - 1) as f32);
        let popup_height = self.config.padding * 2.0
            + (self.config.swatch_size * (total_rows - 1) as f32) // -1 for separators
            + (self.config.row_spacing * total_rows as f32)
            + 16.0 // extra for separators and custom button
            + 32.0; // custom button

        let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_width, popup_height));

        // Check if clicked outside popup (but not on the frame it was opened)
        if self.state.just_opened {
            // Clear the just_opened flag for next frame
            self.state.just_opened = false;
        } else {
            // Check for clicks outside the popup to close it
            let pointer_pos = ui.input(|i| i.pointer.interact_pos());
            if let Some(pos) = pointer_pos
                && ui.input(|i| i.pointer.any_released())
                && !popup_rect.contains(pos)
            {
                self.state.is_open = false;
                return None; // Don't process popup if closing
            }
        }

        // Draw popup using an egui Area
        egui::Area::new(self.id.with("popup"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(popup_bg)
                    .stroke(Stroke::new(DESIGN_TOKENS.stroke.hairline, popup_border))
                    .corner_radius(DESIGN_TOKENS.rounding.xl)
                    .inner_margin(self.config.padding)
                    .show(ui, |ui| {
                        // Grayscale row
                        if let Some(color) = self.draw_color_row(ui, &GRAYSCALE) {
                            color_changed = Some(color);
                        }

                        // Main colors row
                        if let Some(color) = self.draw_color_row(ui, &MAIN_COLORS) {
                            color_changed = Some(color);
                        }

                        ui.add_space(DESIGN_TOKENS.spacing.lg);

                        // Separator
                        let sep_rect = ui.available_rect_before_wrap();
                        ui.painter().hline(
                            sep_rect.x_range(),
                            sep_rect.min.y,
                            Stroke::new(DESIGN_TOKENS.stroke.hairline, separator_color),
                        );
                        ui.add_space(DESIGN_TOKENS.spacing.lg);

                        // Color variations
                        for row in &COLOR_VARIATIONS {
                            if let Some(color) = self.draw_color_row(ui, row) {
                                color_changed = Some(color);
                            }
                        }

                        ui.add_space(DESIGN_TOKENS.spacing.lg);

                        // Separator
                        let sep_rect = ui.available_rect_before_wrap();
                        ui.painter().hline(
                            sep_rect.x_range(),
                            sep_rect.min.y,
                            Stroke::new(DESIGN_TOKENS.stroke.hairline, separator_color),
                        );
                        ui.add_space(DESIGN_TOKENS.spacing.lg);

                        // Custom color button
                        if self.draw_custom_color_btn(ui) {
                            // Custom color mode toggled
                        }

                        // If custom mode is active, show color editor
                        if self.state.custom_mode {
                            ui.add_space(DESIGN_TOKENS.spacing.lg);
                            let mut custom = self.state.custom_color;
                            if ui.color_edit_button_srgba(&mut custom).changed() {
                                self.state.custom_color = custom;
                                color_changed = Some(custom);
                            }
                            ui.add_space(DESIGN_TOKENS.spacing.sm);
                            if ui.small_button("Apply").clicked() {
                                color_changed = Some(self.state.custom_color);
                                self.state.is_open = false;
                            }
                        }
                    });
            });

        // If color changed, update state and close popup
        if let Some(color) = color_changed {
            self.state.sel_color = color;
            if !self.state.custom_mode {
                self.state.is_open = false;
            }
        }

        color_changed
    }

    fn draw_color_row(&self, ui: &mut Ui, colors: &[Color32]) -> Option<Color32> {
        let mut selected = None;

        // Get colors from theme
        let visuals = ui.style().visuals.clone();
        let sel_border = if self.config.sel_border_color == Color32::TRANSPARENT {
            visuals.widgets.active.fg_stroke.color
        } else {
            self.config.sel_border_color
        };
        let muted_border = visuals.widgets.noninteractive.fg_stroke.color;
        let hover_border = visuals.widgets.hovered.fg_stroke.color;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = self.config.swatch_spacing;

            for color in colors {
                let size = Vec2::splat(self.config.swatch_size);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());

                // Draw swatch
                let rounding = self.config.swatch_rounding;
                ui.painter().rect_filled(rect, rounding, *color);

                // Draw border
                let is_sel = *color == self.state.sel_color;
                let is_white = color.r() > 240 && color.g() > 240 && color.b() > 240;

                let border_color = if is_sel {
                    sel_border
                } else if is_white {
                    muted_border
                } else if response.hovered() {
                    hover_border
                } else {
                    Color32::TRANSPARENT
                };

                if border_color != Color32::TRANSPARENT {
                    ui.painter().rect_stroke(
                        rect,
                        rounding,
                        Stroke::new(DESIGN_TOKENS.stroke.thick, border_color),
                        StrokeKind::Outside,
                    );
                }

                if response.clicked() {
                    selected = Some(*color);
                }
            }
        });

        ui.add_space(self.config.row_spacing);

        selected
    }

    fn draw_custom_color_btn(&mut self, ui: &mut Ui) -> bool {
        // Get colors from theme
        let visuals = ui.style().visuals.clone();
        let faint_bg = visuals.faint_bg_color;
        let border_normal = visuals.widgets.noninteractive.bg_stroke.color;
        let border_hover = visuals.widgets.hovered.bg_stroke.color;
        let text_color = visuals.widgets.noninteractive.fg_stroke.color;

        let btn_height = DESIGN_TOKENS.sizing.button_md;
        let btn_width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(btn_width, btn_height), Sense::click());

        // Draw checkerboard pattern for transparency indication
        let checker_size = 6.0;
        // Derive checker colors from theme
        let dark_checker = faint_bg;
        let light_checker = faint_bg.gamma_multiply(1.3);

        for row in 0..((rect.height() / checker_size) as usize) {
            for col in 0..((rect.width() / checker_size) as usize) {
                let checker_rect = Rect::from_min_size(
                    rect.min + Vec2::new(col as f32 * checker_size, row as f32 * checker_size),
                    Vec2::splat(checker_size),
                );
                let is_dark = (row + col) % 2 == 0;
                let color = if is_dark { dark_checker } else { light_checker };
                ui.painter()
                    .rect_filled(checker_rect.intersect(rect), 0.0, color);
            }
        }

        // Draw border
        let border_color = if response.hovered() {
            border_hover
        } else {
            border_normal
        };
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Draw plus icon and text
        let center = rect.center();
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "+",
            egui::FontId::proportional(typography::XXL),
            text_color,
        );

        if response.clicked() {
            self.state.custom_mode = !self.state.custom_mode;
            return true;
        }

        false
    }
}

// ============================================================================
// HELPER FUNCTION FOR INLINE COLOR PICKER
// ============================================================================

/// Show a color picker button that returns the new color if changed
pub fn color_picker_btn(
    ui: &mut Ui,
    color: &mut Color32,
    id_src: impl std::hash::Hash,
) -> Response {
    // Create temporary state
    let id = Id::new(id_src);
    let mut state = ui.ctx().data_mut(|d| {
        d.get_temp::<ColorPickerState>(id)
            .unwrap_or_else(|| ColorPickerState::new(*color))
    });

    // Sync the selected color with the input
    state.sel_color = *color;

    // Show the picker
    let picker = ColorPicker::new(&mut state, id);
    if let Some(new_color) = picker.show(ui) {
        *color = new_color;
    }

    // Store state
    ui.ctx().data_mut(|d| d.insert_temp(id, state));

    // Return a dummy response for the button area
    ui.allocate_response(Vec2::ZERO, Sense::hover())
}
