//! Command palette widget

use super::command::Command;
use super::fuzzy::{FuzzyMatch, fuzzy_match};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Context, Key, Order, Rect, RichText, ScrollArea, Sense, TextEdit, Vec2};

/// Command palette widget for quick command access
pub struct CommandPalette {
    /// All registered commands
    commands: Vec<Command>,
    /// Current search query
    search_query: String,
    /// Currently selected index in the filtered results
    selected_index: usize,
    /// Cached filtered results (command index, match)
    filtered: Vec<(usize, FuzzyMatch)>,
}

impl CommandPalette {
    /// Create a new command palette with the given commands
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            search_query: String::new(),
            selected_index: 0,
            filtered: Vec::new(),
        }
    }

    /// Add a command to the palette
    pub fn add_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Remove a command by ID
    pub fn remove_command(&mut self, id: &str) {
        self.commands.retain(|c| c.id != id);
    }

    /// Get the current search query
    pub fn query(&self) -> &str {
        &self.search_query
    }

    /// Clear the search query and reset selection
    pub fn clear(&mut self) {
        self.search_query.clear();
        self.selected_index = 0;
        self.filtered.clear();
    }

    /// Update filtered results based on current query
    fn update_filter(&mut self) {
        self.filtered.clear();

        if self.search_query.is_empty() {
            // Show all commands when query is empty
            for (idx, _) in self.commands.iter().enumerate() {
                self.filtered.push((
                    idx,
                    FuzzyMatch {
                        score: 0,
                        matched_indices: Vec::new(),
                    },
                ));
            }
        } else {
            // Fuzzy match against all commands
            for (idx, cmd) in self.commands.iter().enumerate() {
                if let Some(m) = fuzzy_match(&self.search_query, &cmd.searchable_text()) {
                    self.filtered.push((idx, m));
                }
            }

            // Sort by score (descending)
            self.filtered.sort_by_key(|m| std::cmp::Reverse(m.1.score));
        }

        // Reset selection if out of bounds
        if self.selected_index >= self.filtered.len() {
            self.selected_index = 0;
        }
    }

    /// Show the command palette modal
    ///
    /// Returns the ID of the executed command if any
    pub fn show(&mut self, ctx: &Context, open: &mut bool) -> Option<String> {
        if !*open {
            return None;
        }

        let mut executed_command = None;

        // Handle keyboard navigation
        let (move_up, move_down, execute, close) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });

        if close {
            *open = false;
            self.clear();
            return None;
        }

        if move_up && self.selected_index > 0 {
            self.selected_index -= 1;
        }
        if move_down && self.selected_index + 1 < self.filtered.len() {
            self.selected_index += 1;
        }

        if execute && !self.filtered.is_empty() {
            let (cmd_idx, _) = self.filtered[self.selected_index];
            executed_command = Some(self.commands[cmd_idx].id.clone());
            *open = false;
            self.clear();
            return executed_command;
        }

        // Render the modal
        let screen_rect = ctx.content_rect();
        let modal_width = DESIGN_TOKENS
            .sizing
            .command_palette
            .width
            .min(screen_rect.width() - DESIGN_TOKENS.sizing.command_palette.margin_x);
        let modal_max_height = DESIGN_TOKENS
            .sizing
            .command_palette
            .height
            .min(screen_rect.height() - DESIGN_TOKENS.sizing.command_palette.margin_y);

        // Modal window
        egui::Area::new(egui::Id::unique("command_palette"))
            .order(Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                Vec2::new(0.0, DESIGN_TOKENS.sizing.command_palette.margin_y),
            )
            .show(ctx, |ui| {
                // The scrim goes in the palette's own layer before its frame, so it dims only
                // what lies below the palette.
                ui.ctx().layer_painter(ui.layer_id()).rect_filled(
                    screen_rect,
                    0.0,
                    DESIGN_TOKENS.semantic.modal.overlay_bg,
                );
                egui::Frame::new()
                    .fill(ui.visuals().window_fill)
                    .stroke(ui.visuals().window_stroke)
                    .corner_radius(ui.visuals().window_corner_radius)
                    .shadow(ui.visuals().window_shadow)
                    .show(ui, |ui| {
                        ui.set_width(modal_width);
                        ui.set_max_height(modal_max_height);

                        // Search input
                        ui.add_space(DESIGN_TOKENS.spacing.md);
                        ui.horizontal(|ui| {
                            ui.add_space(DESIGN_TOKENS.spacing.md);

                            // Search icon
                            let tint = ui.visuals().widgets.noninteractive.fg_stroke.color;
                            ui.add(
                                icons::QUICK_SEARCH.as_image_tinted(
                                    Vec2::splat(DESIGN_TOKENS.sizing.icon_md),
                                    tint,
                                ),
                            );
                            ui.add_space(DESIGN_TOKENS.spacing.sm);

                            // Search input
                            let response = ui.add(
                                TextEdit::singleline(&mut self.search_query)
                                    .hint_text("Search commands...")
                                    // No background, with egui's default text margin.
                                    .frame(
                                        egui::Frame::new()
                                            .inner_margin(egui::Margin::symmetric(4, 2)),
                                    )
                                    .desired_width(
                                        modal_width
                                            - DESIGN_TOKENS.sizing.command_palette.input_padding,
                                    )
                                    .font(egui::TextStyle::Body),
                            );

                            // Auto-focus the input
                            response.request_focus();

                            // Update filter when query changes
                            if response.changed() {
                                self.update_filter();
                            }

                            ui.add_space(DESIGN_TOKENS.spacing.md);
                        });

                        ui.add_space(DESIGN_TOKENS.spacing.sm);
                        ui.separator();

                        // Initial filter if empty
                        if self.filtered.is_empty() && self.search_query.is_empty() {
                            self.update_filter();
                        }

                        // Results list
                        if self.filtered.is_empty() {
                            ui.add_space(DESIGN_TOKENS.spacing.xl);
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No commands found")
                                        .color(ui.visuals().weak_text_color()),
                                );
                            });
                            ui.add_space(DESIGN_TOKENS.spacing.xl);
                        } else {
                            ScrollArea::vertical()
                                .max_height(
                                    modal_max_height
                                        - DESIGN_TOKENS.sizing.command_palette.scroll_padding,
                                )
                                .show(ui, |ui| {
                                    ui.add_space(DESIGN_TOKENS.spacing.xs);

                                    for (list_idx, (cmd_idx, _)) in self.filtered.iter().enumerate()
                                    {
                                        let cmd = &self.commands[*cmd_idx];
                                        if command_row(ui, cmd, list_idx == self.selected_index)
                                            .clicked()
                                        {
                                            executed_command = Some(cmd.id.clone());
                                        }
                                    }

                                    ui.add_space(DESIGN_TOKENS.spacing.xs);
                                });
                        }
                    });
            });

        // Close on click outside
        if ctx.input(|i| i.pointer.any_pressed()) {
            let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
            if let Some(pos) = pointer_pos {
                let modal_rect = Rect::from_center_size(
                    egui::pos2(
                        screen_rect.center().x,
                        DESIGN_TOKENS.sizing.command_palette.margin_y + modal_max_height / 2.0,
                    ),
                    Vec2::new(modal_width, modal_max_height),
                );
                if !modal_rect.contains(pos) {
                    *open = false;
                    self.clear();
                }
            }
        }

        if executed_command.is_some() {
            *open = false;
            self.clear();
        }

        executed_command
    }
}

/// Height of a row that shows a description under its label.
const ROW_HEIGHT_WITH_DESCRIPTION: f32 = 40.0;

/// Paint one result row: icon slot, label with optional description, then category and
/// shortcut at the right.
fn command_row(ui: &mut egui::Ui, cmd: &Command, is_selected: bool) -> egui::Response {
    let theme = ui.active_theme();
    let height = if cmd.description.is_some() {
        ROW_HEIGHT_WITH_DESCRIPTION
    } else {
        DESIGN_TOKENS.sizing.list_item_height
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), is_selected, &cmd.label)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let visuals = ui.visuals();
    let (fill, text, icon_tint, secondary) = if is_selected {
        let fg = visuals.selection.stroke.color;
        (visuals.selection.bg_fill, fg, fg, fg)
    } else {
        let fill = if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let tint = visuals.widgets.noninteractive.fg_stroke.color;
        (fill, visuals.text_color(), tint, visuals.weak_text_color())
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);

    // Every row reserves the icon slot, so labels line up.
    let icon_size = DESIGN_TOKENS.sizing.icon_sm;
    let icon_x = rect.min.x + theme.spacing.md;
    if let Some(icon) = cmd.icon {
        let icon_rect = Rect::from_min_size(
            egui::pos2(icon_x, rect.center().y - icon_size / 2.0),
            Vec2::splat(icon_size),
        );
        icon.as_image_tinted(icon_rect.size(), icon_tint)
            .paint_at(ui, icon_rect);
    }

    let text_x = icon_x + icon_size + theme.spacing.lg;
    let small = egui::FontId::proportional(theme.text.small);
    // A description goes under the label; both meet at the row's middle.
    let label_align = if cmd.description.is_some() {
        egui::Align2::LEFT_BOTTOM
    } else {
        egui::Align2::LEFT_CENTER
    };
    let text_pos = egui::pos2(text_x, rect.center().y);
    let body = egui::FontId::proportional(theme.text.body);
    painter.text(text_pos, label_align, &cmd.label, body, text);
    if let Some(description) = &cmd.description {
        painter.text(
            text_pos,
            egui::Align2::LEFT_TOP,
            description,
            small.clone(),
            secondary,
        );
    }

    // Shortcut at the right edge, the category to its left.
    let mut right = rect.max.x - theme.spacing.md;
    let shortcut = cmd
        .shortcut
        .map(|shortcut| ui.ctx().format_shortcut(&shortcut));
    for label in [shortcut.as_ref(), cmd.category.as_ref()]
        .into_iter()
        .flatten()
    {
        let drawn = painter.text(
            egui::pos2(right, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            label,
            small.clone(),
            secondary,
        );
        right = drawn.min.x - theme.spacing.lg;
    }

    response
}

/// Handler for managing command palette state
pub struct CommandPaletteHandler {
    palette: CommandPalette,
    open: bool,
}

impl CommandPaletteHandler {
    /// Create a new command palette handler
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            palette: CommandPalette::new(commands),
            open: false,
        }
    }

    /// Open the command palette
    pub fn open(&mut self) {
        self.open = true;
        self.palette.clear();
    }

    /// Close the command palette
    pub fn close(&mut self) {
        self.open = false;
        self.palette.clear();
    }

    /// Check if the palette is open
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Add a command
    pub fn add_command(&mut self, command: Command) {
        self.palette.add_command(command);
    }

    /// Remove a command by ID
    pub fn remove_command(&mut self, id: &str) {
        self.palette.remove_command(id);
    }

    /// Update the command palette each frame
    ///
    /// Handles Ctrl+K / Cmd+K to open, and returns the executed command ID if any
    pub fn update(&mut self, ctx: &Context) -> Option<String> {
        // Check for Ctrl+K / Cmd+K to open
        let should_open = ctx.input(|i| {
            let ctrl_or_cmd = if cfg!(target_os = "macos") {
                i.modifiers.mac_cmd
            } else {
                i.modifiers.ctrl
            };
            ctrl_or_cmd && i.key_pressed(Key::K)
        });

        if should_open && !self.open {
            self.open();
        }

        // Show the palette if open
        self.palette.show(ctx, &mut self.open)
    }
}
