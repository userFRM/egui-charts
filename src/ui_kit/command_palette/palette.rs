//! Command palette widget

use super::command::Command;
use super::fuzzy::{FuzzyMatch, rank};
use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, Context, Key, Rect, RichText, ScrollArea, Sense, TextEdit, Vec2};

/// How many run commands the palette remembers.
const RECENT_LIMIT: usize = 5;

/// Command palette widget for quick command access
#[derive(Debug)]
pub struct CommandPalette {
    /// All registered commands
    commands: Vec<Command>,
    /// Current search query
    search_query: String,
    /// Highlighted row of `filtered`; `None` until the user types a matching query
    /// or presses an arrow key.
    selected_index: Option<usize>,
    /// Ids of the commands run from the palette, most recent first, at most [`RECENT_LIMIT`].
    recent: Vec<String>,
    /// Command ids whose star was pressed, in order, until the host drains them.
    star_toggles: Vec<String>,
    /// Cached filtered results (command index, match)
    filtered: Vec<(usize, FuzzyMatch)>,
    /// The query `filtered` was built for; a different query puts the highlight back on row 0.
    filtered_query: String,
    focus_search: bool,
    context: Option<Context>,
}

impl CommandPalette {
    /// Create a new command palette with the given commands
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            search_query: String::new(),
            selected_index: None,
            recent: Vec::new(),
            star_toggles: Vec::new(),
            filtered: Vec::new(),
            filtered_query: String::new(),
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

    /// The ids of the commands last run from the palette, most recent first, at most five.
    /// While the query is empty the palette lists those still registered under a Recent heading.
    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    /// Restores ids read from [`Self::recent`], most recent first. A repeated id keeps its first
    /// place and ids past the fifth are left out. An id with no registered command stays in the
    /// list but is not shown until a command with that id is added.
    pub fn set_recent(&mut self, ids: impl IntoIterator<Item = impl Into<String>>) {
        self.recent.clear();
        for id in ids {
            let id = id.into();
            if self.recent.len() == RECENT_LIMIT {
                break;
            }
            if !self.recent.contains(&id) {
                self.recent.push(id);
            }
        }
    }

    /// The ids of the commands whose star was pressed since the last call, in order. The palette
    /// has already flipped their [`Command::starred`]; apply each change to what the star marks.
    pub fn take_star_toggles(&mut self) -> Vec<String> {
        std::mem::take(&mut self.star_toggles)
    }

    /// Puts `id` first among the recent entries.
    fn record_recent(&mut self, id: &str) {
        self.recent.retain(|recent| recent != id);
        self.recent.insert(0, id.to_owned());
        self.recent.truncate(RECENT_LIMIT);
    }

    /// Clear the search query and reset selection
    pub fn clear(&mut self) {
        self.search_query.clear();
        self.selected_index = None;
        self.filtered.clear();
        self.filtered_query.clear();
        self.focus_search = true;
    }

    /// Update filtered results based on current query
    fn update_filter(&mut self) {
        self.filtered.clear();

        // An empty query lists the recent commands still registered, in their order.
        if self.search_query.is_empty() {
            for id in &self.recent {
                if let Some(idx) = self.commands.iter().position(|cmd| &cmd.id == id) {
                    self.filtered.push((
                        idx,
                        FuzzyMatch {
                            score: 0,
                            matched_indices: Vec::new(),
                        },
                    ));
                }
            }
        } else {
            for (idx, cmd) in self.commands.iter().enumerate() {
                // The row paints the translated text, but the English text matches too.
                let mut labels = vec![cmd.label.clone()];
                let mut other: Vec<String> = cmd
                    .description
                    .iter()
                    .chain(cmd.category.iter())
                    .chain(cmd.keywords.iter())
                    .cloned()
                    .collect();
                if let Some(ctx) = &self.context {
                    labels.insert(0, l10n::text(ctx, &cmd.label));
                    for text in cmd.description.iter().chain(cmd.category.iter()) {
                        other.push(l10n::text(ctx, text));
                    }
                }
                let other = other.join(" ");
                let best = labels
                    .iter()
                    .filter_map(|label| rank(&self.search_query, label, &other))
                    .reduce(|a, b| if b.score > a.score { b } else { a });
                if let Some(m) = best {
                    self.filtered.push((idx, m));
                }
            }

            // Sort by score (descending)
            self.filtered.sort_by_key(|m| std::cmp::Reverse(m.1.score));
        }

        // A new query highlights its best match; an empty query waits for an arrow key.
        if self.filtered_query != self.search_query
            || self
                .selected_index
                .is_some_and(|row| row >= self.filtered.len())
        {
            self.selected_index =
                (!self.search_query.is_empty() && !self.filtered.is_empty()).then_some(0);
            self.filtered_query.clone_from(&self.search_query);
        }
    }

    /// Applies `change` to the commands and rebuilds the rows, keeping the highlight on the command
    /// it was on while that command is still listed.
    fn edit_commands(&mut self, change: impl FnOnce(&mut Vec<Command>)) {
        let highlighted = self
            .selected_index
            .and_then(|row| self.filtered.get(row))
            .map(|&(i, _)| self.commands[i].id.clone());
        change(&mut self.commands);
        self.update_filter();
        if let Some(row) = highlighted.and_then(|id| {
            self.filtered
                .iter()
                .position(|&(i, _)| self.commands[i].id == id)
        }) {
            self.selected_index = Some(row);
        }
    }

    /// Show the command palette modal
    ///
    /// With an empty query it lists the commands run most recently under a Recent heading,
    /// without a highlight until an arrow key moves onto them, or an instruction to type when
    /// there are none; a query lists its matches with the best one highlighted. Returns the ID
    /// of the executed command if any. Keyboard activation consumes its key so the host can
    /// open another dialog in the same frame.
    pub fn show(&mut self, ctx: &Context, open: &mut bool) -> Option<String> {
        if !*open {
            return None;
        }
        self.context = Some(ctx.clone());
        self.update_filter();

        let mut executed_command = None;
        let mut toggled_star = None;
        let star_slot = self.commands.iter().any(|cmd| cmd.starred.is_some());

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

        let before = self.selected_index;
        if move_up && let Some(row) = self.selected_index.filter(|row| *row > 0) {
            self.selected_index = Some(row - 1);
        }
        if move_down {
            self.selected_index = match self.selected_index {
                None if !self.filtered.is_empty() => Some(0),
                Some(row) if row + 1 < self.filtered.len() => Some(row + 1),
                row => row,
            };
        }
        // Only a key or a new query scrolls to the highlight, so the wheel still scrolls freely.
        let mut reveal_selected = self.selected_index != before;

        if execute && let Some(row) = self.selected_index {
            ctx.input_mut(|i| i.consume_key(i.modifiers, Key::Enter));
            let (cmd_idx, _) = self.filtered[row];
            let id = self.commands[cmd_idx].id.clone();
            self.record_recent(&id);
            *open = false;
            self.clear();
            return Some(id);
        }

        crate::ui_kit::modal::Modal::new("command_palette", "Search actions")
            .dim_background(false)
            .width(480.0)
            .height(680.0)
            .rounding(ctx.active_theme().radius.lg)
            .show(ctx, open, |ui| {
                ui.add_space(12.0);
                ui.separator();
                // The separators and item spacing add 14 px to the 41 px search row.
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 27.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let theme = ui.active_theme();
                        ui.add(icons::QUICK_SEARCH.as_image_tinted(
                            Vec2::splat(18.0),
                            theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.2),
                        ));
                        if focus_search {
                            ui.memory_mut(|memory| memory.request_focus(search_id));
                        }
                        let response = ui
                            .add(
                                TextEdit::singleline(&mut self.search_query)
                                    .id(search_id)
                                    // Enter runs a match; without one the query stays focused.
                                    .return_key(None)
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
                            reveal_selected = true;
                        }
                    },
                );
                ui.separator();
                ui.add_space(12.0);
                if self.filtered.is_empty() {
                    let message = if self.search_query.is_empty() {
                        "Type to search for actions"
                    } else {
                        "No matching actions"
                    };
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), message))
                                .size(ui.active_theme().text.heading),
                        );
                    });
                } else {
                    if self.search_query.is_empty() {
                        let theme = ui.active_theme();
                        ui.horizontal(|ui| {
                            ui.add_space(theme.spacing.md);
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), "Recent"))
                                    .size(theme.text.small)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        });
                    }
                    ScrollArea::vertical()
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            for (list_idx, (cmd_idx, _)) in self.filtered.iter().enumerate() {
                                let cmd = &self.commands[*cmd_idx];
                                let selected = Some(list_idx) == self.selected_index;
                                let (row, star) = command_row(ui, cmd, selected, star_slot);
                                if selected && reveal_selected {
                                    row.scroll_to_me(None);
                                }
                                if let Some(star) = star.filter(egui::Response::clicked) {
                                    toggled_star = Some((
                                        *cmd_idx,
                                        star.clicked_by(egui::PointerButton::Primary),
                                    ));
                                } else if row.clicked() {
                                    executed_command = Some(cmd.id.clone());
                                }
                            }
                        });
                }
            });

        if let Some((idx, by_pointer)) = toggled_star {
            let command = &mut self.commands[idx];
            command.starred = command.starred.map(|starred| !starred);
            self.star_toggles.push(command.id.clone());
            // A pointer press leaves the field, so typing and Enter would reach nothing.
            self.focus_search |= by_pointer;
            ctx.input_mut(|i| {
                i.consume_key(i.modifiers, Key::Enter);
                i.consume_key(i.modifiers, Key::Space);
            });
        }
        if let Some(id) = &executed_command {
            self.record_recent(id);
        }
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
fn command_row(
    ui: &mut egui::Ui,
    cmd: &Command,
    is_selected: bool,
    star_slot: bool,
) -> (egui::Response, Option<egui::Response>) {
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
    let slot = star_slot.then(|| {
        Rect::from_min_max(
            egui::pos2(
                rect.max.x - theme.spacing.md - DESIGN_TOKENS.sizing.button_sm,
                rect.min.y,
            ),
            egui::pos2(rect.max.x - theme.spacing.md, rect.max.y),
        )
    });
    // The star answers its own clicks, so a click on it never runs the command.
    let star = cmd.starred.zip(slot).map(|(starred, slot)| {
        let star = ui.interact(slot, response.id.with("star"), Sense::click());
        let name = l10n::format_text(ui.ctx(), "Favourite {0}", &[("0", &translated_label)]);
        star.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, star.enabled(), starred, &name)
        });
        star
    });
    let hovered = response.hovered() || star.as_ref().is_some_and(egui::Response::hovered);
    if !ui.is_rect_visible(rect) {
        return (response, star);
    }

    let visuals = ui.visuals();
    let (fill, text, icon_tint, secondary) = if is_selected {
        let fg = visuals.selection.stroke.color;
        (visuals.selection.bg_fill, fg, fg, fg)
    } else {
        let fill = if hovered {
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
    let mut right = slot.map_or(rect.max.x - theme.spacing.md, |slot| {
        slot.min.x - theme.spacing.sm
    });
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

    if let (Some(starred), Some(star)) = (cmd.starred, &star) {
        let active = hovered || is_selected || response.has_focus() || star.has_focus();
        if starred || active {
            let (icon, tint) = match (starred, is_selected) {
                (true, false) => (&icons::STAR_FILLED, theme.ui.warning),
                (true, true) => (&icons::STAR_FILLED, secondary),
                (false, _) => (&icons::STAR_EMPTY, secondary),
            };
            let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_sm);
            icon.as_image_tinted(size, tint)
                .paint_at(ui, Rect::from_center_size(star.rect.center(), size));
        }
        crate::styles::focus::draw_focus_ring(ui, star, theme.radius.sm);
    }

    (response, star)
}

/// Handler for managing command palette state
#[derive(Debug)]
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

    /// The ids of the commands last run, most recent first; see [`CommandPalette::recent`].
    pub fn recent(&self) -> &[String] {
        self.palette.recent()
    }

    /// Restores ids read from [`Self::recent`]; see [`CommandPalette::set_recent`].
    pub fn set_recent(&mut self, ids: impl IntoIterator<Item = impl Into<String>>) {
        self.palette.set_recent(ids);
    }

    /// The ids whose star was pressed; see [`CommandPalette::take_star_toggles`].
    pub fn take_star_toggles(&mut self) -> Vec<String> {
        self.palette.take_star_toggles()
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
    fn run_entries_are_listed_most_recent_first_on_reopening() {
        fn texts(ctx: &Context, palette: &mut CommandPalette) -> Vec<(String, f32)> {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                palette.show(ui.ctx(), &mut true);
            });
            let texts = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        Some((text.galley.text().to_owned(), text.pos.y))
                    } else {
                        None
                    }
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        }
        let commands = || {
            vec![
                Command::new("a", "Alpha"),
                Command::new("b", "Beta"),
                Command::new("c", "Gamma"),
                Command::new("d", "Delta"),
            ]
        };
        let ctx = Context::default();
        let mut p = CommandPalette::new(commands());
        for (query, id) in [("alpha", "a"), ("beta", "b")] {
            p.search_query = query.into();
            frame(&ctx, &mut p, None);
            assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some(id));
        }
        assert_eq!(p.recent(), ["b", "a"]);
        frame(&ctx, &mut p, None);
        let rows = texts(&ctx, &mut p);
        let y = |label| rows.iter().find(|(text, _)| text == label).unwrap().1;
        assert!(rows.iter().any(|(text, _)| text == "Recent"));
        assert!(y("Beta") < y("Alpha"));
        assert!(
            !rows
                .iter()
                .any(|(text, _)| text == "Gamma" || text == "Type to search for actions")
        );
        assert_eq!(p.selected_index, None);
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)), None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        frame(&ctx, &mut p, Some(Key::ArrowUp));
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("b"));
        assert_eq!(p.recent(), ["b", "a"]);
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("a"));
        assert_eq!(p.recent(), ["a", "b"]);

        let mut p = CommandPalette::new(commands());
        p.set_recent(["c", "zzz", "a", "c", "b", "x", "y"]);
        assert_eq!(p.recent(), ["c", "zzz", "a", "b", "x"]);
        frame(&ctx, &mut p, None);
        let rows = texts(&ctx, &mut p);
        let y = |label| rows.iter().find(|(text, _)| text == label).unwrap().1;
        assert!(y("Gamma") < y("Alpha") && y("Alpha") < y("Beta"));
        assert!(!rows.iter().any(|(text, _)| text == "zzz" || text == "x"));
        p.search_query = "delta".into();
        frame(&ctx, &mut p, None);
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("d"));
        assert_eq!(p.recent(), ["d", "c", "zzz", "a", "b"]);
        p.remove_command("c");
        frame(&ctx, &mut p, None);
        assert!(!texts(&ctx, &mut p).iter().any(|(text, _)| text == "Gamma"));
        p.add_command(Command::new("c", "Gamma"));
        assert!(texts(&ctx, &mut p).iter().any(|(text, _)| text == "Gamma"));
        p.search_query = "alpha".into();
        frame(&ctx, &mut p, None);
        p.search_query.clear();
        frame(&ctx, &mut p, None);
        assert_eq!(p.selected_index, None);
        frame(&ctx, &mut p, Some(Key::Escape));
        assert_eq!(p.recent(), ["d", "c", "zzz", "a", "b"]);
    }

    #[test]
    fn stars_toggle_by_pointer_and_keyboard_without_closing() {
        fn run(
            ctx: &Context,
            palette: &mut CommandPalette,
            events: Vec<Event>,
        ) -> (Option<String>, egui::FullOutput) {
            let (mut open, mut executed) = (true, None);
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| executed = palette.show(ui.ctx(), &mut open),
            );
            assert_eq!(open, executed.is_none());
            (executed, output)
        }
        let palette = || {
            let mut p = CommandPalette::new(vec![
                Command::new("t", "Trend Line").starred(false),
                Command::new("p", "Pitchfork"),
            ]);
            p.search_query = "trend".into();
            p
        };
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            ctx.enable_accesskit();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut p = palette();
            run(&ctx, &mut p, vec![]).1.drop_without_applying_deltas();
            let (_, output) = run(&ctx, &mut p, vec![]);
            let node = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some("Favourite Trend Line"))
                .unwrap()
                .1;
            assert_eq!(node.role(), egui::accesskit::Role::Button);
            assert_eq!(node.toggled(), Some(egui::accesskit::Toggled::False));
            let bounds = node.bounds().unwrap();
            let pos = egui::pos2(
                (bounds.x0 + bounds.x1) as f32 * 0.5,
                (bounds.y0 + bounds.y1) as f32 * 0.5,
            );
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                let (id, output) = run(
                    &ctx,
                    &mut p,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                assert_eq!(id, None);
                output.drop_without_applying_deltas();
            }
            assert_eq!(p.query(), "trend");
            assert_eq!(p.take_star_toggles(), ["t"]);
            assert!(p.take_star_toggles().is_empty());
            assert_eq!(p.commands[0].starred, Some(true));
            assert!(
                p.recent().is_empty(),
                "starring alone does not run the command"
            );
            run(&ctx, &mut p, vec![]).1.drop_without_applying_deltas();
            let (id, output) = run(&ctx, &mut p, vec![key(Key::Enter)]);
            assert_eq!(
                id.as_deref(),
                Some("t"),
                "a pointer star press returns focus to search"
            );
            assert_eq!(p.recent(), ["t"]);
            output.drop_without_applying_deltas();

            for activation in [Key::Space, Key::Enter] {
                let ctx = Context::default();
                ctx.enable_accesskit();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut p = palette();
                for events in [vec![], vec![], vec![key(Key::Tab)], vec![key(Key::Tab)]] {
                    run(&ctx, &mut p, events).1.drop_without_applying_deltas();
                }
                let focused = ctx.memory(|m| m.focused());
                for starred in [true, false] {
                    let (id, output) = run(&ctx, &mut p, vec![key(activation)]);
                    assert_eq!(id, None);
                    assert!(!ctx.input(|i| i.key_pressed(activation)));
                    assert_eq!(p.commands[0].starred, Some(starred));
                    assert_eq!(p.query(), "trend");
                    assert_eq!(ctx.memory(|m| m.focused()), focused);
                    output.drop_without_applying_deltas();
                    let (_, output) = run(&ctx, &mut p, vec![]);
                    let node = &output
                        .platform_output
                        .accesskit_update
                        .as_ref()
                        .unwrap()
                        .nodes
                        .iter()
                        .find(|(_, node)| node.label() == Some("Favourite Trend Line"))
                        .unwrap()
                        .1;
                    assert_eq!(
                        node.toggled(),
                        Some(if starred {
                            egui::accesskit::Toggled::True
                        } else {
                            egui::accesskit::Toggled::False
                        })
                    );
                    output.drop_without_applying_deltas();
                }
                assert_eq!(p.take_star_toggles(), ["t", "t"]);
                assert!(p.recent().is_empty());
            }
        }
    }

    #[test]
    fn empty_palette_has_an_undimmed_backdrop_and_a_spacious_search_row() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut palette = CommandPalette::new(vec![]);
            frame(&ctx, &mut palette, None);
            let mut open = true;
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    palette.show(ui.ctx(), &mut open);
                },
            );
            let rules: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { points, .. } if points[0].y == points[1].y => {
                        Some(points[0].y)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(rules.len(), 2, "{rules:?}");
            assert_eq!(rules[1] - rules[0], 41.0);
            for shape in &output.shapes {
                match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.contains(egui::pos2(10.0, 10.0)) => {
                        assert_eq!(
                            rect.fill.a(),
                            0,
                            "the backdrop must leave the plot undimmed"
                        );
                    }
                    egui::Shape::Text(text)
                        if text.galley.text() == "Type to search for actions" =>
                    {
                        assert_eq!(
                            text.galley.job.sections[0].format.font_id.size,
                            theme.text.heading
                        );
                    }
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn search_and_focused_rows_consume_their_activation_key() {
        for (_, theme) in crate::theme::presets() {
            for (focus_row, key) in [(false, Key::Enter), (true, Key::Enter), (true, Key::Space)] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut palette = CommandPalette::new(vec![
                    Command::new("a", "Action one"),
                    Command::new("b", "Action two"),
                ]);
                frame(&ctx, &mut palette, None);
                assert_eq!(
                    frame(&ctx, &mut palette, Some(key)),
                    None,
                    "nothing is listed or run before a query"
                );
                palette.search_query = "action".into();
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
                    .shortcut(egui::KeyboardShortcut::new(Modifiers::CTRL, Key::I))
                    .starred(true),
                Command::new("settings", "Open settings")
                    .category("Chart appearance and scale settings"),
            ];
            for width in [480.0, 238.0, 278.0, 480.0] {
                for selected in [false, true] {
                    for command in &commands {
                        let mut row = Rect::NOTHING;
                        let mut star = None;
                        let output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    Vec2::new(width, 200.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                let (response, star_response) =
                                    command_row(ui, command, selected, true);
                                row = response.rect;
                                star = star_response.map(|response| response.rect);
                            },
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
                        if let Some(star) = star {
                            assert!(row.contains_rect(star));
                        }
                        for (i, rect) in text.iter().enumerate() {
                            if let Some(star) = star {
                                assert!(
                                    !rect.intersects(star),
                                    "text overlaps star at width {width}: {rect:?} {star:?}"
                                );
                            }
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
                    .map(|id| Command::new(id, id).category("Actions"))
                    .collect(),
            )
        };

        // A removal leaves no row past the end of the list.
        let (ctx, mut p) = (Context::default(), palette());
        p.search_query = "actions".into();
        frame(&ctx, &mut p, None);
        p.remove_command("b");
        assert_eq!(frame(&ctx, &mut p, None), None);

        // A removal above the highlighted command keeps the highlight on it.
        let (ctx, mut p) = (Context::default(), palette());
        p.search_query = "actions".into();
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        p.remove_command("a");
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("b"));

        for (query, clear_translation, expected) in [
            ("zweite", false, Some("b")),
            ("b", false, Some("b")),
            ("zweite", true, None),
        ] {
            let (ctx, mut p) = (Context::default(), palette());
            l10n::set_translator(&ctx, |english| (english == "b").then(|| "zweite".into()));
            p.search_query = query.into();
            frame(&ctx, &mut p, None);
            if clear_translation {
                l10n::clear_translator(&ctx);
            }
            assert_eq!(
                frame(&ctx, &mut p, Some(Key::Enter)).as_deref(),
                expected,
                "{query}, clear translation: {clear_translation}"
            );
        }

        let (ctx, mut p) = (Context::default(), palette());
        l10n::set_translator(&ctx, |english| Some(format!("Aktion {english}")));
        p.search_query = "Aktion".into();
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        p.remove_command("c");
        assert_eq!(frame(&ctx, &mut p, Some(Key::Enter)).as_deref(), Some("b"));
    }

    #[test]
    fn typing_returns_the_highlight_to_the_first_result() {
        let (ctx, mut p) = (
            Context::default(),
            CommandPalette::new(vec![
                Command::new("sto", "Add Stochastic RSI"),
                Command::new("con", "Add Connors RSI"),
                Command::new("rsi", "Add RSI"),
                Command::new("ao", "Add AO"),
            ]),
        );
        p.search_query = "a".into();
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        frame(&ctx, &mut p, Some(Key::ArrowDown));
        p.search_query = "add rsi".into();
        assert_eq!(
            frame(&ctx, &mut p, Some(Key::Enter)).as_deref(),
            Some("rsi")
        );
    }

    #[test]
    fn arrow_keys_keep_the_highlighted_row_in_view() {
        let (ctx, mut p) = (
            Context::default(),
            CommandPalette::new(
                (0..60)
                    .map(|i| Command::new(format!("a{i}"), format!("Action {i}")))
                    .collect(),
            ),
        );
        p.search_query = "ction".into();
        frame(&ctx, &mut p, None);
        frame(&ctx, &mut p, None);
        let selected = ctx.global_style().visuals.selection.bg_fill;
        // Runs one frame, a second after the last so the scroll animation completes, and counts
        // the highlight rows painted inside the list.
        let mut time = 0.0;
        let mut run = |event: Option<Event>| {
            let mut open = true;
            time += 1.0;
            let output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    events: event.into_iter().collect(),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    p.show(ui.ctx(), &mut open);
                },
            );
            let visible = output
                .shapes
                .iter()
                .filter(|clipped| {
                    matches!(&clipped.shape, egui::Shape::Rect(r)
                        if r.fill == selected && clipped.clip_rect.contains_rect(r.rect))
                })
                .count();
            output.drop_without_applying_deltas();
            visible
        };
        let key = |key| {
            Some(Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
        };
        // Down and Up past the rows that fit, then settle.
        let mut visible_highlights = 0;
        for event in std::iter::repeat_n(key(Key::ArrowDown), 35)
            .chain(std::iter::repeat_n(key(Key::ArrowUp), 3))
            .chain([None, None])
        {
            visible_highlights = run(event);
        }
        assert_eq!(
            visible_highlights, 1,
            "the highlighted row is painted in view"
        );
        // More text (typed at the start, where the field's cursor begins) puts the highlight on the
        // first row, which must come into view as well.
        for event in [Some(Event::Text("a".into())), None, None] {
            visible_highlights = run(event);
        }
        assert_eq!(visible_highlights, 1, "the first result is painted in view");
        assert_eq!(
            (p.search_query.as_str(), p.selected_index),
            ("action", Some(0))
        );
    }
}
