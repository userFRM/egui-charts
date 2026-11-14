//! Named drawing folders and their descendant controls.

use egui::Ui;

use crate::drawings::FolderSync;
use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::{IconButton, buttons::ButtonSize};

use super::item_row::render_item_row;
use super::source_tree::matches_filter;
use super::{ObjectTreeAction, ObjectTreeConfig, ObjectTreeState, SourceItem, SourceType};

/// The drag payload of a folder row: the folder's path.
struct FolderDrag(Vec<String>);

/// The outermost folders whose drawings are all selected, leaving out those that contain
/// `target`, which stay; their fully selected subfolders are taken instead.
pub(super) fn selected_folders(
    state: &ObjectTreeState,
    sources: &[SourceItem],
    target: &[String],
) -> Vec<Vec<String>> {
    let mut selected: Vec<Vec<String>> = Vec::new();
    // Tree order puts a folder ahead of its descendants.
    for path in &state.folders {
        if target.starts_with(path) || selected.iter().any(|outer| path.starts_with(outer)) {
            continue;
        }
        let mut members = sources
            .iter()
            .filter(|s| s.is_drawing() && s.folder.starts_with(path))
            .peekable();
        if members.peek().is_some() && members.all(|s| state.is_selected(s.id)) {
            selected.push(path.clone());
        }
    }
    selected
}

/// The folders `path` can move under: not itself, a descendant or its current parent.
fn move_targets<'a>(
    state: &'a ObjectTreeState,
    path: &'a [String],
) -> impl Iterator<Item = &'a Vec<String>> {
    state.folders.iter().filter(move |candidate| {
        !candidate.starts_with(path) && candidate.as_slice() != &path[..path.len() - 1]
    })
}

pub(super) fn matches_folder_filter(
    path: &[String],
    sources: &[SourceItem],
    state: &ObjectTreeState,
    filter: &str,
) -> bool {
    !path.is_empty()
        && state
            .filter_type
            .is_none_or(|kind| kind == SourceType::Drawing)
        && (filter.is_empty()
            || state.folders.iter().any(|candidate| {
                candidate.starts_with(path)
                    && candidate
                        .iter()
                        .any(|name| name.to_lowercase().contains(filter))
            })
            || sources.iter().any(|s| {
                s.is_drawing() && s.folder.starts_with(path) && matches_filter(s, filter, state)
            }))
}

pub(super) fn render_folders(
    ui: &mut Ui,
    sources: &mut [SourceItem],
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    filter: &str,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;
    // Iterate paths in tree order without recursion, including for deeply nested host data.
    let paths: Vec<_> = state
        .folders
        .iter()
        .filter(|path| !path.is_empty())
        .cloned()
        .collect();
    for path in paths {
        if (1..path.len()).any(|depth| state.collapsed_folders.contains(&path[..depth]))
            && filter.is_empty()
        {
            continue;
        }
        if !matches_folder_filter(&path, sources, state, filter) {
            continue;
        }
        let collapsed = state.collapsed_folders.contains(&path) && filter.is_empty();
        let members: Vec<_> = sources
            .iter()
            .filter(|s| s.is_drawing() && s.folder.starts_with(&path))
            .collect();
        let visible = members.iter().any(|s| s.visible);
        let locked = !members.is_empty() && members.iter().all(|s| s.locked);
        let selected = !members.is_empty() && members.iter().all(|s| state.is_selected(s.id));
        let sync = members.first().and_then(|s| s.folder_sync);
        let can_group = sources
            .iter()
            .any(|s| s.is_drawing() && state.is_selected(s.id));
        ui.push_id(&path, |ui| {
            let background = ui.painter().add(egui::Shape::Noop);
            let mut highlight = None;
            let row = ui.horizontal(|ui| {
                ui.add_space(
                    ((path.len() - 1) as f32 * config.indent_width)
                        .min((ui.available_width() - 120.0).max(0.0)),
                );
                if IconButton::new(if collapsed {
                    &icons::CHEVRON_RIGHT
                } else {
                    &icons::CHEVRON_DOWN
                })
                .size(ButtonSize::SM)
                .tooltip("Toggle folder")
                .show(ui)
                .clicked()
                    && !state.collapsed_folders.remove(&path)
                {
                    state.collapsed_folders.insert(path.clone());
                }
                let label_size =
                    egui::vec2((ui.available_width() - 64.0).max(0.0), config.row_height);
                let label = ui
                    .allocate_ui_with_layout(
                        label_size,
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_size(label_size);
                            ui.add(
                                egui::Label::new({
                                    let name = egui::RichText::new(path.last().unwrap()).strong();
                                    if selected {
                                        name.color(ui.visuals().selection.stroke.color)
                                    } else {
                                        name
                                    }
                                })
                                .sense(if config.enable_drag_drop {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::click()
                                })
                                .selectable(false)
                                .truncate(),
                            )
                        },
                    )
                    .inner;
                label.clone().on_hover_text(path.join(" / "));
                if label.clicked() {
                    action = if ui.input(|i| i.modifiers.ctrl) && config.enable_multi_select {
                        ObjectTreeAction::AddFolderToSelection(path.clone())
                    } else {
                        ObjectTreeAction::SelectFolder(path.clone())
                    };
                }
                if config.enable_drag_drop {
                    label.dnd_set_drag_payload(FolderDrag(path.clone()));
                    let dragged = egui::DragAndDrop::payload::<FolderDrag>(ui.ctx());
                    if dragged.as_ref().is_some_and(|drag| drag.0 == path) {
                        highlight = Some(ui.visuals().widgets.noninteractive.fg_stroke.color);
                    } else if dragged.as_ref().is_some_and(|drag| {
                        move_targets(state, &drag.0).any(|target| *target == path)
                    }) && label.contains_pointer()
                    {
                        highlight = Some(ui.active_theme().ui.accent);
                    }
                    if let Some(drag) = label.dnd_release_payload::<FolderDrag>()
                        && move_targets(state, &drag.0).any(|target| *target == path)
                    {
                        action = ObjectTreeAction::MoveFolder(drag.0.clone(), path.clone());
                    }
                }
                if config.enable_context_menu {
                    label.context_menu(|ui| {
                        if ui
                            .add_enabled(
                                can_group,
                                egui::Button::new(l10n::text(
                                    ui.ctx(),
                                    "New subfolder with selected drawings",
                                )),
                            )
                            .clicked()
                        {
                            state.creating_folder = Some(path.clone());
                            ui.close();
                        }
                        ui.menu_button(l10n::text(ui.ctx(), "Move to folder"), |ui| {
                            let root = (path.len() > 1).then(Vec::new);
                            let targets: Vec<_> = root
                                .into_iter()
                                .chain(move_targets(state, &path).cloned())
                                .collect();
                            for target in targets {
                                let title = if target.is_empty() {
                                    l10n::text(ui.ctx(), "Root")
                                } else {
                                    target.join(" / ")
                                };
                                if ui.button(title).clicked() {
                                    action = ObjectTreeAction::MoveFolder(path.clone(), target);
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        // A nested folder shows the mode of its top-level folder, which it follows.
                        ui.add_enabled_ui(path.len() == 1, |ui| {
                            for (mode, title) in [
                                (None, "Follow layout setting"),
                                (Some(FolderSync::Chart), "This chart only"),
                                (Some(FolderSync::Layout), "All charts in layout"),
                            ] {
                                if ui
                                    .selectable_label(sync == mode, l10n::text(ui.ctx(), title))
                                    .clicked()
                                {
                                    action = ObjectTreeAction::SetFolderSync(path.clone(), mode);
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        if ui.button(l10n::text(ui.ctx(), "Delete folder")).clicked() {
                            action = ObjectTreeAction::DeleteFolder(path.clone());
                            ui.close();
                        }
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if config.show_lock_indicator
                        && IconButton::new(if locked { &icons::LOCK } else { &icons::UNLOCK })
                            .size(ButtonSize::SM)
                            .tooltip(if locked {
                                "Unlock folder"
                            } else {
                                "Lock folder"
                            })
                            .enabled(!members.is_empty())
                            .show(ui)
                            .clicked()
                    {
                        action = ObjectTreeAction::SetFolderLocked(path.clone(), !locked);
                    }
                    if IconButton::new(if visible {
                        &icons::EYE
                    } else {
                        &icons::EYE_HIDE
                    })
                    .size(ButtonSize::SM)
                    .tooltip(if visible {
                        "Hide folder"
                    } else {
                        "Show folder"
                    })
                    .enabled(!members.is_empty())
                    .show(ui)
                    .clicked()
                    {
                        action = ObjectTreeAction::SetFolderVisible(path.clone(), !visible);
                    }
                });
            });
            let mut rect = row.response.rect;
            rect.set_right(ui.max_rect().right());
            if selected {
                ui.painter().set(
                    background,
                    egui::Shape::rect_filled(rect, 0.0, ui.visuals().selection.bg_fill),
                );
            }
            if let Some(color) = highlight {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, color),
                    egui::StrokeKind::Inside,
                );
            }
        });
        if collapsed {
            continue;
        }
        for item in sources
            .iter_mut()
            .filter(|s| s.is_drawing() && s.folder == path)
        {
            if matches_filter(item, filter, state) {
                let next = render_item_row(ui, item, config, state, path.len());
                if !next.is_none() {
                    action = next;
                }
            }
        }
    }
    // Dropped here, a nested folder leaves its parents.
    if let Some(drag) = egui::DragAndDrop::payload::<FolderDrag>(ui.ctx())
        && drag.0.len() > 1
    {
        let (rect, zone) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), config.row_height),
            egui::Sense::hover(),
        );
        let title = l10n::text(ui.ctx(), "Move to root");
        zone.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &title));
        let color = if zone.contains_pointer() {
            ui.active_theme().ui.accent
        } else {
            ui.visuals().widgets.noninteractive.fg_stroke.color
        };
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, color),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(ui.active_theme().text.label),
            color,
        );
        if zone.dnd_release_payload::<FolderDrag>().is_some() {
            action = ObjectTreeAction::MoveFolder(drag.0.clone(), Vec::new());
        }
    }
    action
}
