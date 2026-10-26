//! Command palette widget

use super::command::Command;
use super::fuzzy::{FuzzyMatch, fuzzy_match};
use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, Context, Key, Rect, RichText, ScrollArea, Sense, TextEdit, Vec2};

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
    focus_search: bool,
    context: Option<Context>,
}

impl CommandPalette {
    /// Create a new command palette with the given commands
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            search_query: String::new(),
            selected_index: 0,
            filtered: Vec::new(),
            focus_search: true,
            context: None,
        }
    }

    /// Add a command to the palette
    pub fn add_command(&mut self, command: Command) {
        self.edit_commands(|commands| commands.push(command));
    }

    /// Remove a command by ID
    pub fn remove_command(&mut self, id: &str) {
        self.edit_commands(|commands| commands.retain(|c| c.id != id));
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
        self.focus_search = true;
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
                let mut searchable = cmd.searchable_text();
                if let Some(ctx) = &self.context {
                    for text in std::iter::once(&cmd.label)
                        .chain(cmd.description.iter())
                        .chain(cmd.category.iter())
                    {
                        searchable.push(' ');
                        searchable.push_str(&l10n::text(ctx, text));
                    }
                }
                if let Some(m) = fuzzy_match(&self.search_query, &searchable) {
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

    /// Applies `change` to the commands and rebuilds the rows, keeping the highlight on the command
    /// it was on while that command is still listed.
    fn edit_commands(&mut self, change: impl FnOnce(&mut Vec<Command>)) {
        let highlighted = self
            .filtered
            .get(self.selected_index)
            .map(|&(i, _)| self.commands[i].id.clone());
        change(&mut self.commands);
        self.update_filter();
        if let Some(row) = highlighted.and_then(|id| {
            self.filtered
                .iter()
                .position(|&(i, _)| self.commands[i].id == id)
        }) {
            self.selected_index = row;
        }
    }

    /// Show the command palette modal
    ///
    /// Returns the ID of the executed command if any. Keyboard activation consumes its key so
    /// the host can open another dialog in the same frame.
    pub fn show(&mut self, ctx: &Context, open: &mut bool) -> Option<String> {
        if !*open {
            return None;
        }
        self.context = Some(ctx.clone());
        self.update_filter();

        let mut executed_command = None;

        let search_id = egui::Id::unique("command_palette_search");
        let focus_search = std::mem::take(&mut self.focus_search);
        let search_focused = ctx.memory(|m| m.has_focus(search_id));
        // Handle keyboard navigation
        let (move_up, move_down, execute, close) = ctx.input(|i| {
            (
                search_focused && i.key_pressed(Key::ArrowUp),
                search_focused && i.key_pressed(Key::ArrowDown),
                search_focused && i.key_pressed(Key::Enter),
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
            ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Enter));
            let (cmd_idx, _) = self.filtered[self.selected_index];
            executed_command = Some(self.commands[cmd_idx].id.clone());
            *open = false;
            self.clear();
            return executed_command;
        }

        crate::ui_kit::modal::Modal::new("command_palette", "Search actions")
            .width(480.0)
            .height(680.0)
            .rounding(ctx.active_theme().radius.lg)
            .show(ctx, open, |ui| {
                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(
                        icons::QUICK_SEARCH
                            .as_image_tinted(Vec2::splat(20.0), ui.visuals().text_color()),
                    );
                    if focus_search {
                        ui.memory_mut(|memory| memory.request_focus(search_id));
                    }
                    let response = ui
                        .add(
                            TextEdit::singleline(&mut self.search_query)
                                .id(search_id)
                                .hint_text(l10n::text(ui.ctx(), "Search actions"))
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width())
                                .font(egui::FontId::proportional(14.0)),
                        )
                        .accessible_name(l10n::text(ui.ctx(), "Search actions"));
                    // Sizing passes clear invisible widgets' focus while building the field.
                    if focus_search {
                        response.request_focus();
                    }
                    if response.changed() {
                        self.update_filter();
                    }
                });
                ui.separator();
                ui.add_space(12.0);
                if self.filtered.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), "No matching actions")).size(14.0),
                        );
                    });
                } else {
                    ScrollArea::vertical()
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            for (list_idx, (cmd_idx, _)) in self.filtered.iter().enumerate() {
                                let cmd = &self.commands[*cmd_idx];
                                if command_row(ui, cmd, list_idx == self.selected_index).clicked() {
                                    executed_command = Some(cmd.id.clone());
                                }
                            }
                        });
                }
            });

        if executed_command.is_some() || !*open {
            ctx.input_mut(|i| {
                i.consume_key(i.modifiers, Key::Enter);
                i.consume_key(i.modifiers, Key::Space);
            });
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
    let translated_label = l10n::text(ui.ctx(), &cmd.label);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            ui.is_enabled(),
            is_selected,
            &translated_label,
        )
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
    painter.rect_filled(
        rect.shrink2(Vec2::new(theme.spacing.sm, 0.0)),
        ui.active_theme().radius.sm,
        fill,
    );
    crate::styles::focus::draw_focus_ring(ui, &response, ui.active_theme().radius.sm);

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
    // Shortcut at the right edge, the category to its left.
    let mut right = rect.max.x - theme.spacing.md;
    // Leave at least half the text space for the command and its description.
    let metadata_left = (text_x + right) / 2.0;
    let shortcut = cmd
        .shortcut
        .map(|shortcut| ui.ctx().format_shortcut(&shortcut));
    for label in [shortcut.as_ref(), cmd.category.as_ref()]
        .into_iter()
        .flatten()
    {
        if right <= metadata_left {
            break;
        }
        let mut job = LayoutJob::simple_singleline(
            l10n::text(ui.ctx(), label.clone()),
            small.clone(),
            secondary,
        );
        job.wrap = TextWrapping::truncate_at_width(right - metadata_left);
        let galley = painter.layout_job(job);
        let pos = egui::pos2(
            right - galley.size().x,
            rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(pos, galley, secondary);
        right = pos.x - theme.spacing.lg;
    }

    // A description goes under the label; both meet at the row's middle.
    for (label, font, color, align) in [
        (
            Some(&cmd.label),
            egui::FontId::proportional(14.0),
            text,
            if cmd.description.is_some() { 1.0 } else { 0.5 },
        ),
        (cmd.description.as_ref(), small, secondary, 0.0),
    ] {
        if let Some(label) = label {
            let mut job =
                LayoutJob::simple_singleline(l10n::text(ui.ctx(), label.clone()), font, color);
            job.wrap = TextWrapping::truncate_at_width((right - text_x).max(0.0));
            let galley = painter.layout_job(job);
            let pos = egui::pos2(text_x, rect.center().y - galley.size().y * align);
            painter.galley(pos, galley, color);
        }
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

        self.show(ctx)
    }

    /// Show the palette when a host dispatcher owns its opening shortcut.
    pub fn show(&mut self, ctx: &Context) -> Option<String> {
        self.palette.show(ctx, &mut self.open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, RawInput};

    fn frame(ctx: &Context, palette: &mut CommandPalette, key: Option<Key>) -> Option<String> {
        let events = key
            .map(|key| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .into_iter()
            .collect();
        let (mut open, mut executed) = (true, None);
        ctx.run_ui(
            RawInput {
                events,
                ..Default::default()
            },
            |ui| executed = palette.show(ui.ctx(), &mut open),
        )
        .drop_without_applying_deltas();
        executed
    }

    #[test]
    fn search_and_focused_rows_consume_their_activation_key() {
        for (_, theme) in crate::theme::presets() {
            for (focus_row, key) in [(false, Key::Enter), (true, Key::Enter), (true, Key::Space)] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut palette = CommandPalette::new(vec![
                    Command::new("a", "First"),
                    Command::new("b", "Second"),
                ]);
                frame(&ctx, &mut palette, None);
                frame(&ctx, &mut palette, None);
                if focus_row {
                    let search = ctx.memory(|m| m.focused());
                    frame(&ctx, &mut palette, Some(Key::Tab));
                    assert_ne!(ctx.memory(|m| m.focused()), search);
                    frame(&ctx, &mut palette, Some(Key::Tab));
                }
                assert_eq!(
                    frame(&ctx, &mut palette, Some(key)).as_deref(),
                    Some(if focus_row { "b" } else { "a" })
                );
                assert!(!ctx.input(|i| i.key_pressed(key)));
            }
        }
    }

    #[test]
    fn row_text_stays_inside_its_columns_after_narrow_resizing() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            egui_extras::install_image_loaders(&ctx);
            crate::theme::apply_to_egui(&ctx, &theme);
            let commands = [
                Command::new("study", "Add Moving Average Convergence Divergence")
                    .description("Add this study to the current chart in a separate pane")
                    .icon(&icons::RAY)
                    .category("Studies")
                    .shortcut(egui::KeyboardShortcut::new(Modifiers::CTRL, Key::I)),
                Command::new("settings", "Open settings")
                    .category("Chart appearance and scale settings"),
            ];
            for width in [480.0, 238.0, 278.0, 480.0] {
                for selected in [false, true] {
                    for command in &commands {
                        let mut row = Rect::NOTHING;
                        let output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    Vec2::new(width, 200.0),
                                )),
                                ..Default::default()
                            },
                            |ui| row = command_row(ui, command, selected).rect,
                        );
                        let text: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| {
                                if let egui::Shape::Text(text) = &shape.shape {
                                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        output.drop_without_applying_deltas();
                        assert_eq!(
                            text.len(),
                            if command.description.is_some() { 4 } else { 2 }
                        );
                        for (i, rect) in text.iter().enumerate() {
                            assert!(
                                row.contains_rect(*rect),
                                "text outside row: {rect:?} {row:?}"
                            );
                            for other in &text[..i] {
                                let overlap = rect.intersect(*other);
                                assert!(
                                    overlap.width() <= 0.0 || overlap.height() <= 0.0,
                                    "text columns overlap at width {width}: {rect:?} {other:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn changing_the_commands_keeps_the_rows_on_listed_commands() {
        let palette = || {
            CommandPalette::new(
                ["a", "b", "c"]
                    .into_iter()
                    .map(|id| Command::new(id, id))
                    .collect(),
            )
        };

        // A removal leaves no row past the end of the list.
        let (ctx, mut p) = (Context::default(), palette());
        frame(&ctx, &mut p, None);
        p.remove_command("b");
        assert_eq!(frame(&ctx, &mut p, None), None);

        // A removal above the highlighted command keeps the highlight on it.
        let (ctx, mut p) = (Context::default(), palette());
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        p.remove_command("a");
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("b"));

        let (ctx, mut p) = (Context::default(), palette());
        l10n::set_translator(&ctx, |english| (english == "b").then(|| "zweite".into()));
        for (query, expected) in [("zweite", vec!["b"]), ("b", vec!["b"])] {
            p.search_query = query.into();
            frame(&ctx, &mut p, None);
            let ids: Vec<_> = p
                .filtered
                .iter()
                .map(|(i, _)| p.commands[*i].id.as_str())
                .collect();
            assert_eq!(ids, expected);
        }
        p.search_query = "zweite".into();
        l10n::clear_translator(&ctx);
        frame(&ctx, &mut p, None);
        assert!(p.filtered.is_empty());

        let (ctx, mut p) = (Context::default(), palette());
        l10n::set_translator(&ctx, |english| Some(format!("Aktion {english}")));
        p.search_query = "Aktion".into();
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        assert_eq!(p.selected_index, 1);
        p.remove_command("c");
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("b"));
    }
}
