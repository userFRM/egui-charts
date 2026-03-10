//! UiExt - Extension trait for egui::Ui
//!
//! Provides 60+ convenience methods for common UI patterns following rerun's approach.
//!
//! # Categories
//!
//! - **Icons**: small_icon, medium_icon, large_icon, icon_with_size
//! - **Labels**: error_label, warning_label, success_label, info_label, muted_label, subdued_label, strong_label, monospace_label, code_label, label_with_icon, clickable_label
//! - **Buttons**: primary_button, secondary_button, danger_button, icon_button, icon_button_with_tooltip, toggle_button
//! - **Selection**: selectable_label_with_icon, selectable_value, radio_value
//! - **Layout**: panel_content, section_header, subsection, list, separator_with_margin, themed_separator, full_width
//! - **Spacing**: space_xs, space_sm, space_md, space_lg, space_xl
//! - **Inputs**: text_edit_singleline, text_edit_multiline, number_edit, search_input
//! - **Drag Values**: drag_value, drag_value_with_range, drag_angle
//! - **Grid/Table**: grid_cell, grid_row, property_row
//! - **Visibility**: visible_rect, is_visible
//! - **Theming**: current_theme, is_dark_mode, is_light_mode
//! - **Utility**: is_touch_mode, min_touch_target

use egui::{Color32, Rect, Response, RichText, Sense, TextEdit, Ui, Vec2, Widget};

#[cfg(feature = "icons")]
use crate::icons::Icon;
#[cfg(feature = "icons")]
use crate::styles::icons;
use crate::tokens::DESIGN_TOKENS;

/// Extension trait for `egui::Ui` providing 60+ convenience methods
pub trait UiExt {
    // ==========================================================================
    // Icon Methods (require "icons" feature)
    // ==========================================================================

    /// Render a small icon (16px)
    #[cfg(feature = "icons")]
    fn small_icon(&mut self, icon: &Icon) -> Response;

    /// Render a medium icon (20px)
    #[cfg(feature = "icons")]
    fn medium_icon(&mut self, icon: &Icon) -> Response;

    /// Render a large icon (24px)
    #[cfg(feature = "icons")]
    fn large_icon(&mut self, icon: &Icon) -> Response;

    /// Render an icon at a specific size
    #[cfg(feature = "icons")]
    fn icon_with_size(&mut self, icon: &Icon, size: f32) -> Response;

    // ==========================================================================
    // Label Methods
    // ==========================================================================

    /// Render an error label (red text)
    fn error_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a warning label (orange text)
    fn warning_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a success label (green text)
    fn success_label(&mut self, text: impl Into<String>) -> Response;

    /// Render an info label (blue text)
    fn info_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a muted/secondary label (subdued text)
    fn muted_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a subdued label (alias for muted_label)
    fn subdued_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a hint label (small + weak, for column headers and auxiliary info)
    fn hint_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a strong/bold label
    fn strong_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a monospace label
    fn monospace_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a code-style label (monospace with background)
    fn code_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a label with an icon prefix
    #[cfg(feature = "icons")]
    fn label_with_icon(&mut self, icon: &Icon, text: impl Into<String>) -> Response;

    /// Render a clickable label (looks like link)
    fn clickable_label(&mut self, text: impl Into<String>) -> Response;

    // ==========================================================================
    // Button Methods
    // ==========================================================================

    /// Render a primary button (high emphasis)
    fn primary_button(&mut self, text: impl Into<String>) -> Response;

    /// Render a secondary button (medium emphasis)
    fn secondary_button(&mut self, text: impl Into<String>) -> Response;

    /// Render a danger button (destructive action)
    fn danger_button(&mut self, text: impl Into<String>) -> Response;

    /// Render an icon-only button
    #[cfg(feature = "icons")]
    fn icon_button(&mut self, icon: &Icon) -> Response;

    /// Render an icon button with tooltip
    #[cfg(feature = "icons")]
    fn icon_button_with_tooltip(&mut self, icon: &Icon, tooltip: impl Into<String>) -> Response;

    /// Render a toggle button
    fn toggle_button(&mut self, selected: &mut bool, text: impl Into<String>) -> Response;

    // ==========================================================================
    // Selection Methods
    // ==========================================================================

    /// Render a selectable label with an icon
    #[cfg(feature = "icons")]
    fn selectable_label_with_icon(
        &mut self,
        selected: bool,
        icon: &Icon,
        text: impl Into<String>,
    ) -> Response;

    /// Render a selectable value (like radio button without the circle)
    fn selectable_value_ui<T: PartialEq + Clone>(
        &mut self,
        current: &mut T,
        value: T,
        text: impl Into<String>,
    ) -> Response;

    /// Render a radio-style value
    fn radio_value_ui<T: PartialEq + Clone>(
        &mut self,
        current: &mut T,
        value: T,
        text: impl Into<String>,
    ) -> Response;

    /// Combo box for selecting from typed options.
    ///
    /// Replaces the 11-line `ComboBox::from_id_salt → .show_ui → for … selectable_value` pattern.
    /// Returns `true` if the value changed.
    ///
    /// ```ignore
    /// ui.combo_select("theme", &mut settings.theme, ThemePreset::all().copied(), |t| t.display_name().to_string());
    /// ```
    fn combo_select<T: PartialEq + Clone>(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut T,
        options: impl IntoIterator<Item = T>,
        display: impl Fn(&T) -> String,
    ) -> bool;

    /// Combo box for selecting from typed options with explicit width.
    fn combo_select_width<T: PartialEq + Clone>(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut T,
        options: impl IntoIterator<Item = T>,
        display: impl Fn(&T) -> String,
        width: f32,
    ) -> bool;

    /// Combo box for selecting a `String` value from `&str` options.
    ///
    /// ```ignore
    /// ui.combo_str_select("condition", &mut form.condition, &["Crossing", "Greater Than", "Less Than"]);
    /// ```
    fn combo_str_select(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut String,
        options: &[&str],
    ) -> bool;

    /// Combo box for selecting a `String` value from `&str` options with explicit width.
    fn combo_str_select_width(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut String,
        options: &[&str],
        width: f32,
    ) -> bool;

    // ==========================================================================
    // Layout Methods
    // ==========================================================================

    /// Add content within a panel-style container
    fn panel_content(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Render a section header
    fn section_header(&mut self, text: impl Into<String>);

    /// Render a small uppercase section label (e.g. "BALANCE", "MARGIN").
    fn section_label(&mut self, text: impl Into<String>) -> Response;

    /// Render a subsection with collapsible header
    fn subsection(&mut self, text: impl Into<String>, add_contents: impl FnOnce(&mut Ui));

    /// Render content in a list-style container
    fn list(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Right-align content within the current horizontal layout.
    ///
    /// Replaces `ui.with_layout(Layout::right_to_left(Align::Center), |ui| { ... })`.
    fn right_aligned(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Vertical scroll area that fills available space.
    ///
    /// Replaces `egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, ...)`.
    fn scroll_vertical(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Add a separator with margin
    fn separator_with_margin(&mut self, margin: f32);

    /// Add a separator with small spacing above and below — the most common section divider.
    fn spaced_separator(&mut self);

    /// Add a horizontal separator with theme-appropriate color
    fn themed_separator(&mut self);

    /// Vertical separator for top toolbar
    /// 1px width, 20px height, 12px padding each side
    fn toolbar_separator(&mut self);

    /// Get full available width
    fn full_width(&self) -> f32;

    // ==========================================================================
    // Spacing Methods
    // ==========================================================================

    /// Add extra small spacing (2px)
    fn space_xs(&mut self);

    /// Add small spacing (4px)
    fn space_sm(&mut self);

    /// Add medium spacing (6px)
    fn space_md(&mut self);

    /// Add large spacing (8px)
    fn space_lg(&mut self);

    /// Add extra large spacing (12px)
    fn space_xl(&mut self);

    /// Add extra extra large spacing (16px)
    fn space_xxl(&mut self);

    // Legacy aliases
    /// Add small spacing (4px) - alias for space_sm
    fn small_space(&mut self);

    /// Add medium spacing (8px) - alias for space_lg
    fn medium_space(&mut self);

    /// Add large spacing (12px) - alias for space_xl
    fn large_space(&mut self);

    // ==========================================================================
    // Input Methods
    // ==========================================================================

    /// Single-line text edit
    fn text_edit_singleline_ui(&mut self, text: &mut String) -> Response;

    /// Multi-line text edit
    fn text_edit_multiline_ui(&mut self, text: &mut String) -> Response;

    /// Number edit field
    fn number_edit(&mut self, value: &mut f64) -> Response;

    /// Search input with icon
    fn search_input(&mut self, query: &mut String) -> Response;

    // ==========================================================================
    // Drag Value Methods
    // ==========================================================================

    /// Drag value for f32
    fn drag_value_f32(&mut self, value: &mut f32) -> Response;

    /// Drag value with range
    fn drag_value_with_range(
        &mut self,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> Response;

    /// Drag angle (radians, displayed as degrees)
    fn drag_angle(&mut self, radians: &mut f32) -> Response;

    // ==========================================================================
    // Grid/Table Methods
    // ==========================================================================

    /// Render a property row (label: value)
    fn property_row(&mut self, label: impl Into<String>, add_value: impl FnOnce(&mut Ui));

    // ==========================================================================
    // Visibility Methods
    // ==========================================================================

    /// Get the visible rect
    fn visible_rect(&self) -> Rect;

    /// Check if the UI is visible
    fn is_visible(&self) -> bool;

    // ==========================================================================
    // Theming Methods
    // ==========================================================================

    /// Get the current theme
    fn current_theme(&self) -> egui::Theme;

    /// Check if dark mode is active
    fn is_dark_mode(&self) -> bool;

    /// Check if light mode is active
    fn is_light_mode(&self) -> bool;

    // ==========================================================================
    // Utility Methods
    // ==========================================================================

    /// Check if touch input mode is active
    fn is_touch_mode(&self) -> bool;

    /// Get minimum touch target size for current input mode
    fn min_touch_target(&self) -> f32;

    // ==========================================================================
    // Trading Color Methods
    // ==========================================================================

    /// Get P&L color: bullish for positive, bearish for negative, neutral for zero
    fn pnl_color(&self, value: f64) -> Color32;
}

impl UiExt for Ui {
    // ==========================================================================
    // Icon Methods (require "icons" feature)
    // ==========================================================================

    #[cfg(feature = "icons")]
    fn small_icon(&mut self, icon: &Icon) -> Response {
        self.icon_with_size(icon, icons::SMALL)
    }

    #[cfg(feature = "icons")]
    fn medium_icon(&mut self, icon: &Icon) -> Response {
        self.icon_with_size(icon, icons::MEDIUM)
    }

    #[cfg(feature = "icons")]
    fn large_icon(&mut self, icon: &Icon) -> Response {
        self.icon_with_size(icon, icons::LARGE)
    }

    #[cfg(feature = "icons")]
    fn icon_with_size(&mut self, icon: &Icon, size: f32) -> Response {
        let tint = self.style().visuals.widgets.noninteractive.fg_stroke.color;
        let image = icon.as_image_tinted(Vec2::splat(size), tint);
        self.add(image)
    }

    // ==========================================================================
    // Label Methods
    // ==========================================================================

    fn error_label(&mut self, text: impl Into<String>) -> Response {
        let color = DESIGN_TOKENS.semantic.extended.error;
        self.colored_label(color, text.into())
    }

    fn warning_label(&mut self, text: impl Into<String>) -> Response {
        let color = DESIGN_TOKENS.semantic.extended.warning;
        self.colored_label(color, text.into())
    }

    fn success_label(&mut self, text: impl Into<String>) -> Response {
        let color = DESIGN_TOKENS.semantic.extended.success;
        self.colored_label(color, text.into())
    }

    fn info_label(&mut self, text: impl Into<String>) -> Response {
        let color = DESIGN_TOKENS.semantic.extended.info;
        self.colored_label(color, text.into())
    }

    fn muted_label(&mut self, text: impl Into<String>) -> Response {
        let color = self.style().visuals.widgets.noninteractive.fg_stroke.color;
        self.colored_label(color, text.into())
    }

    fn subdued_label(&mut self, text: impl Into<String>) -> Response {
        self.muted_label(text)
    }

    fn hint_label(&mut self, text: impl Into<String>) -> Response {
        self.label(RichText::new(text.into()).small().weak())
    }

    fn strong_label(&mut self, text: impl Into<String>) -> Response {
        self.label(RichText::new(text.into()).strong())
    }

    fn monospace_label(&mut self, text: impl Into<String>) -> Response {
        self.label(RichText::new(text.into()).monospace())
    }

    fn code_label(&mut self, text: impl Into<String>) -> Response {
        self.code(text.into())
    }

    #[cfg(feature = "icons")]
    fn label_with_icon(&mut self, icon: &Icon, text: impl Into<String>) -> Response {
        let text = text.into();
        let icon_size = icons::SMALL;

        self.horizontal(|ui| {
            let tint = ui.style().visuals.widgets.noninteractive.fg_stroke.color;
            ui.add(icon.as_image_tinted(Vec2::splat(icon_size), tint));
            ui.label(text);
        })
        .response
    }

    fn clickable_label(&mut self, text: impl Into<String>) -> Response {
        let text = text.into();
        let color = DESIGN_TOKENS.semantic.brand.accent;
        self.add(egui::Label::new(RichText::new(text).color(color)).sense(Sense::click()))
    }

    // ==========================================================================
    // Button Methods
    // ==========================================================================

    fn primary_button(&mut self, text: impl Into<String>) -> Response {
        let tokens = &DESIGN_TOKENS.semantic.buttons;
        let text = text.into();

        let button = egui::Button::new(RichText::new(text).color(tokens.primary_fg))
            .fill(tokens.primary_bg)
            .corner_radius(DESIGN_TOKENS.rounding.button);

        self.add(button)
    }

    fn secondary_button(&mut self, text: impl Into<String>) -> Response {
        let tokens = &DESIGN_TOKENS.semantic.buttons;
        let text = text.into();

        let button = egui::Button::new(RichText::new(text).color(tokens.secondary_fg))
            .fill(tokens.secondary_bg)
            .corner_radius(DESIGN_TOKENS.rounding.button);

        self.add(button)
    }

    fn danger_button(&mut self, text: impl Into<String>) -> Response {
        let tokens = &DESIGN_TOKENS.semantic.buttons;
        let text = text.into();

        let button = egui::Button::new(RichText::new(text).color(tokens.danger_fg))
            .fill(tokens.danger_bg)
            .corner_radius(DESIGN_TOKENS.rounding.button);

        self.add(button)
    }

    #[cfg(feature = "icons")]
    fn icon_button(&mut self, icon: &Icon) -> Response {
        let size = DESIGN_TOKENS.sizing.button_md;
        let icon_size = icons::MEDIUM;
        let (rect, response) = self.allocate_exact_size(Vec2::splat(size), Sense::click());

        if self.is_rect_visible(rect) {
            let visuals = &self.style().visuals;
            let bg_color = if response.is_pointer_button_down_on() {
                visuals.widgets.active.bg_fill
            } else if response.hovered() {
                visuals.widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };

            self.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.button, bg_color);

            let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
            let icon_color = if response.hovered() {
                visuals.widgets.hovered.fg_stroke.color
            } else {
                visuals.widgets.noninteractive.fg_stroke.color
            };
            icon.as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(self, icon_rect);
        }

        response
    }

    #[cfg(feature = "icons")]
    fn icon_button_with_tooltip(&mut self, icon: &Icon, tooltip: impl Into<String>) -> Response {
        let response = self.icon_button(icon);
        response.on_hover_text(tooltip.into())
    }

    fn toggle_button(&mut self, selected: &mut bool, text: impl Into<String>) -> Response {
        let text = text.into();
        let visuals = &self.style().visuals;

        let (bg, fg) = if *selected {
            (visuals.selection.bg_fill, visuals.selection.stroke.color)
        } else {
            (
                Color32::TRANSPARENT,
                visuals.widgets.inactive.fg_stroke.color,
            )
        };

        let button = egui::Button::new(RichText::new(text).color(fg))
            .fill(bg)
            .corner_radius(DESIGN_TOKENS.rounding.button);

        let response = self.add(button);
        if response.clicked() {
            *selected = !*selected;
        }
        response
    }

    // ==========================================================================
    // Selection Methods
    // ==========================================================================

    #[cfg(feature = "icons")]
    fn selectable_label_with_icon(
        &mut self,
        selected: bool,
        icon: &Icon,
        text: impl Into<String>,
    ) -> Response {
        let text = text.into();
        let icon_size = icons::SMALL;
        let total_width = self.available_width();

        let (rect, response) = self.allocate_exact_size(
            Vec2::new(total_width, DESIGN_TOKENS.sizing.button_md),
            Sense::click(),
        );

        if self.is_rect_visible(rect) {
            let visuals = self.style().visuals.clone();

            // Background
            let bg_color = if selected {
                visuals.selection.bg_fill
            } else if response.hovered() {
                visuals.widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };

            self.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.button, bg_color);

            // Icon
            let icon_rect = Rect::from_min_size(
                rect.min + Vec2::new(DESIGN_TOKENS.spacing.sm, (rect.height() - icon_size) / 2.0),
                Vec2::splat(icon_size),
            );
            let icon_color = if selected {
                visuals.selection.stroke.color
            } else {
                visuals.widgets.noninteractive.fg_stroke.color
            };
            icon.as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(self, icon_rect);

            // Text
            let text_pos = egui::pos2(
                icon_rect.right() + DESIGN_TOKENS.spacing.sm,
                rect.center().y - self.text_style_height(&egui::TextStyle::Body) / 2.0,
            );
            let text_color = if selected {
                visuals.selection.stroke.color
            } else {
                visuals.widgets.noninteractive.fg_stroke.color
            };
            self.painter().text(
                text_pos,
                egui::Align2::LEFT_TOP,
                text,
                egui::TextStyle::Body.resolve(self.style()),
                text_color,
            );
        }

        response
    }

    fn selectable_value_ui<T: PartialEq + Clone>(
        &mut self,
        current: &mut T,
        value: T,
        text: impl Into<String>,
    ) -> Response {
        let selected = *current == value;
        let response = self.selectable_label(selected, text.into());
        if response.clicked() {
            *current = value;
        }
        response
    }

    fn radio_value_ui<T: PartialEq + Clone>(
        &mut self,
        current: &mut T,
        value: T,
        text: impl Into<String>,
    ) -> Response {
        let selected = *current == value;
        let response = self.radio(selected, text.into());
        if response.clicked() {
            *current = value;
        }
        response
    }

    fn combo_select<T: PartialEq + Clone>(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut T,
        options: impl IntoIterator<Item = T>,
        display: impl Fn(&T) -> String,
    ) -> bool {
        let selected_text = display(value);
        let mut changed = false;
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .show_ui(self, |ui| {
                for option in options {
                    let label = display(&option);
                    if ui.selectable_value(value, option, label).changed() {
                        changed = true;
                    }
                }
            });
        changed
    }

    fn combo_select_width<T: PartialEq + Clone>(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut T,
        options: impl IntoIterator<Item = T>,
        display: impl Fn(&T) -> String,
        width: f32,
    ) -> bool {
        let selected_text = display(value);
        let mut changed = false;
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .width(width)
            .show_ui(self, |ui| {
                for option in options {
                    let label = display(&option);
                    if ui.selectable_value(value, option, label).changed() {
                        changed = true;
                    }
                }
            });
        changed
    }

    fn combo_str_select(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut String,
        options: &[&str],
    ) -> bool {
        let mut changed = false;
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(value.as_str())
            .show_ui(self, |ui| {
                for &option in options {
                    if ui
                        .selectable_value(value, option.to_string(), option)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        changed
    }

    fn combo_str_select_width(
        &mut self,
        id_salt: impl std::hash::Hash,
        value: &mut String,
        options: &[&str],
        width: f32,
    ) -> bool {
        let mut changed = false;
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(value.as_str())
            .width(width)
            .show_ui(self, |ui| {
                for &option in options {
                    if ui
                        .selectable_value(value, option.to_string(), option)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        changed
    }

    // ==========================================================================
    // Layout Methods
    // ==========================================================================

    fn panel_content(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        egui::Frame::new()
            .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.md as i8))
            .show(self, |ui| {
                add_contents(ui);
            });
    }

    fn section_header(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.add_space(DESIGN_TOKENS.spacing.lg);
        self.label(
            RichText::new(text)
                .strong()
                .size(DESIGN_TOKENS.typography.md),
        );
        self.add_space(DESIGN_TOKENS.spacing.sm);
    }

    fn section_label(&mut self, text: impl Into<String>) -> Response {
        use crate::ext::HasDesignTokens;
        use crate::styles::typography;
        self.label(
            RichText::new(text.into())
                .size(typography::TINY)
                .strong()
                .color(self.text_secondary()),
        )
    }

    fn subsection(&mut self, text: impl Into<String>, add_contents: impl FnOnce(&mut Ui)) {
        egui::CollapsingHeader::new(text.into())
            .default_open(true)
            .show(self, |ui| {
                add_contents(ui);
            });
    }

    fn right_aligned(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        self.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            add_contents,
        );
    }

    fn scroll_vertical(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(self, |ui| {
                add_contents(ui);
            });
    }

    fn list(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        self.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = DESIGN_TOKENS.spacing.xs;
            add_contents(ui);
        });
    }

    fn separator_with_margin(&mut self, margin: f32) {
        self.add_space(margin);
        self.separator();
        self.add_space(margin);
    }

    fn spaced_separator(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.sm);
        self.separator();
        self.add_space(DESIGN_TOKENS.spacing.sm);
    }

    fn themed_separator(&mut self) {
        let stroke_color = self.style().visuals.widgets.noninteractive.bg_stroke.color;
        let rect = self.available_rect_before_wrap();
        let y = rect.top();
        self.painter().hline(
            rect.x_range(),
            y,
            egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, stroke_color),
        );
        self.add_space(DESIGN_TOKENS.spacing.hairline);
    }

    fn toolbar_separator(&mut self) {
        use crate::theming;

        let padding = DESIGN_TOKENS.spacing.xl; // 12px
        let sep_width = DESIGN_TOKENS.sizing.toolbar.separator_width;
        let sep_height = DESIGN_TOKENS.sizing.toolbar.separator_height;
        self.add_space(padding);
        let (rect, _) = self.allocate_exact_size(Vec2::new(sep_width, sep_height), Sense::hover());
        if self.is_rect_visible(rect) {
            self.painter()
                .rect_filled(rect, 0.0, theming::separator_color(self));
        }
        self.add_space(padding);
    }

    fn full_width(&self) -> f32 {
        self.available_width()
    }

    // ==========================================================================
    // Spacing Methods
    // ==========================================================================

    fn space_xs(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.xs);
    }

    fn space_sm(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.sm);
    }

    fn space_md(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.md);
    }

    fn space_lg(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.lg);
    }

    fn space_xl(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.xl);
    }

    fn space_xxl(&mut self) {
        self.add_space(DESIGN_TOKENS.spacing.xxl);
    }

    // Legacy aliases
    fn small_space(&mut self) {
        self.space_sm();
    }

    fn medium_space(&mut self) {
        self.space_lg();
    }

    fn large_space(&mut self) {
        self.space_xl();
    }

    // ==========================================================================
    // Input Methods
    // ==========================================================================

    fn text_edit_singleline_ui(&mut self, text: &mut String) -> Response {
        TextEdit::singleline(text)
            .desired_width(f32::INFINITY)
            .ui(self)
    }

    fn text_edit_multiline_ui(&mut self, text: &mut String) -> Response {
        TextEdit::multiline(text)
            .desired_width(f32::INFINITY)
            .ui(self)
    }

    fn number_edit(&mut self, value: &mut f64) -> Response {
        self.add(egui::DragValue::new(value).speed(0.1))
    }

    fn search_input(&mut self, query: &mut String) -> Response {
        let response = TextEdit::singleline(query)
            .hint_text("Search...")
            .desired_width(f32::INFINITY)
            .ui(self);

        // Clear button when not empty
        if !query.is_empty()
            && response.lost_focus()
            && self.input(|i| i.key_pressed(egui::Key::Escape))
        {
            query.clear();
        }

        response
    }

    // ==========================================================================
    // Drag Value Methods
    // ==========================================================================

    fn drag_value_f32(&mut self, value: &mut f32) -> Response {
        self.add(egui::DragValue::new(value).speed(0.1))
    }

    fn drag_value_with_range(
        &mut self,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> Response {
        self.add(egui::DragValue::new(value).speed(0.1).range(range))
    }

    fn drag_angle(&mut self, radians: &mut f32) -> Response {
        self.add(
            egui::DragValue::new(radians)
                .speed(0.01)
                .suffix("°")
                .custom_formatter(|n, _| format!("{:.1}", n.to_degrees()))
                .custom_parser(|s| s.parse::<f64>().ok().map(|d| d.to_radians())),
        )
    }

    // ==========================================================================
    // Grid/Table Methods
    // ==========================================================================

    fn property_row(&mut self, label: impl Into<String>, add_value: impl FnOnce(&mut Ui)) {
        self.horizontal(|ui| {
            ui.label(label.into());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                add_value(ui);
            });
        });
    }

    // ==========================================================================
    // Visibility Methods
    // ==========================================================================

    fn visible_rect(&self) -> Rect {
        self.clip_rect()
    }

    fn is_visible(&self) -> bool {
        let rect = self.clip_rect();
        rect.width() > 0.0 && rect.height() > 0.0
    }

    // ==========================================================================
    // Theming Methods
    // ==========================================================================

    fn current_theme(&self) -> egui::Theme {
        self.ctx().theme()
    }

    fn is_dark_mode(&self) -> bool {
        self.ctx().theme() == egui::Theme::Dark
    }

    fn is_light_mode(&self) -> bool {
        self.ctx().theme() == egui::Theme::Light
    }

    // ==========================================================================
    // Utility Methods
    // ==========================================================================

    fn is_touch_mode(&self) -> bool {
        self.ctx().input(|i| i.any_touches())
    }

    fn min_touch_target(&self) -> f32 {
        DESIGN_TOKENS.sizing.target_min
    }

    // ==========================================================================
    // Trading Color Methods
    // ==========================================================================

    fn pnl_color(&self, value: f64) -> Color32 {
        if value > 0.0 {
            DESIGN_TOKENS.semantic.extended.bullish
        } else if value < 0.0 {
            DESIGN_TOKENS.semantic.extended.bearish
        } else {
            self.style().visuals.text_color()
        }
    }
}
