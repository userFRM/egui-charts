//! Command palette widget

use super::command::Command;
use super::fuzzy::{FuzzyMatch, fuzzy_match};
use crate::icons::icons;
use crate::styles::typography;
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
            self.filtered.sort_by(|a, b| b.1.score.cmp(&a.1.score));
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

        // Dark overlay
        let layer_id = egui::LayerId::new(Order::Foreground, egui::Id::new("command_palette_bg"));
        let painter = ctx.layer_painter(layer_id);
        painter.rect_filled(screen_rect, 0.0, DESIGN_TOKENS.semantic.modal.overlay_bg);

        // Modal window
        egui::Area::new(egui::Id::new("command_palette"))
            .order(Order::Foreground)
            .anchor(
                egui::Align2::CENTER_TOP,
                Vec2::new(0.0, DESIGN_TOKENS.sizing.command_palette.margin_y),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(DESIGN_TOKENS.semantic.modal.panel_bg)
                    .stroke(egui::Stroke::new(
                        1.0,
                        DESIGN_TOKENS.semantic.modal.panel_border,
                    ))
                    .corner_radius(DESIGN_TOKENS.rounding.lg)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 8,
                        color: Color32::from_black_alpha(100),
                    })
                    .show(ui, |ui| {
                        ui.set_width(modal_width);
                        ui.set_max_height(modal_max_height);

                        // Search input
                        ui.add_space(DESIGN_TOKENS.spacing.md);
                        ui.horizontal(|ui| {
                            ui.add_space(DESIGN_TOKENS.spacing.md);

                            // Search icon
                            ui.add(
                                icons::QUICK_SEARCH
                                    .as_image(Vec2::splat(DESIGN_TOKENS.sizing.icon_md)),
                            );
                            ui.add_space(DESIGN_TOKENS.spacing.sm);

                            // Search input
                            let response = ui.add(
                                TextEdit::singleline(&mut self.search_query)
                                    .hint_text("Search commands...")
                                    .frame(false)
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
                                        .color(DESIGN_TOKENS.semantic.ui.text_muted_dark),
                                );
                            });
                            ui.add_space(DESIGN_TOKENS.spacing.xl);
                        } else {
                            // Collect commands we need to render
                            let items_to_render: Vec<_> = self
                                .filtered
                                .iter()
                                .enumerate()
                                .map(|(list_idx, (cmd_idx, _))| {
                                    let cmd = &self.commands[*cmd_idx];
                                    (
                                        list_idx,
                                        cmd.id.clone(),
                                        cmd.label.clone(),
                                        cmd.category.clone(),
                                    )
                                })
                                .collect();

                            ScrollArea::vertical()
                                .max_height(
                                    modal_max_height
                                        - DESIGN_TOKENS.sizing.command_palette.scroll_padding,
                                )
                                .show(ui, |ui| {
                                    ui.add_space(DESIGN_TOKENS.spacing.xs);

                                    for (list_idx, cmd_id, label, category) in items_to_render {
                                        let is_selected = list_idx == self.selected_index;

                                        let bg_color = if is_selected {
                                            DESIGN_TOKENS.semantic.command_palette.item_selected
                                        } else {
                                            Color32::TRANSPARENT
                                        };

                                        let (rect, response) = ui.allocate_exact_size(
                                            Vec2::new(
                                                ui.available_width(),
                                                DESIGN_TOKENS.sizing.list_item_height,
                                            ),
                                            Sense::click(),
                                        );

                                        if ui.is_rect_visible(rect) {
                                            let painter = ui.painter();

                                            // Background
                                            let hover_color = if response.hovered() && !is_selected
                                            {
                                                DESIGN_TOKENS.semantic.command_palette.item_hover
                                            } else {
                                                bg_color
                                            };
                                            painter.rect_filled(rect, 0.0, hover_color);

                                            // Label
                                            let text_color = if is_selected {
                                                DESIGN_TOKENS.semantic.ui.text_dark
                                            } else {
                                                DESIGN_TOKENS.semantic.ui.text_secondary_dark
                                            };

                                            let x = rect.min.x + DESIGN_TOKENS.spacing.md;
                                            painter.text(
                                                egui::pos2(x, rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                                &label,
                                                egui::FontId::proportional(typography::LG),
                                                text_color,
                                            );

                                            // Category (if any)
                                            if let Some(ref cat) = category {
                                                let cat_x = rect.max.x - DESIGN_TOKENS.spacing.md;
                                                painter.text(
                                                    egui::pos2(cat_x, rect.center().y),
                                                    egui::Align2::RIGHT_CENTER,
                                                    cat,
                                                    egui::FontId::proportional(typography::SM),
                                                    DESIGN_TOKENS
                                                        .semantic
                                                        .command_palette
                                                        .shortcut_text,
                                                );
                                            }
                                        }

                                        if response.clicked() {
                                            executed_command = Some(cmd_id);
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
