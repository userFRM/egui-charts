//! Named drawing folders and their descendant controls.

use egui::Ui;

use crate::icons::icons;
use crate::localization as l10n;
use crate::ui_kit::{IconButton, buttons::ButtonSize};

use super::item_row::render_item_row;
use super::source_tree::matches_filter;
use super::{ObjectTreeAction, ObjectTreeConfig, ObjectTreeState, SourceItem, SourceType};

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
            && !state.has_filter()
        {
            continue;
        }
        if !matches_folder_filter(&path, sources, state, filter) {
            continue;
        }
        let collapsed = state.collapsed_folders.contains(&path) && !state.has_filter();
        let members: Vec<_> = sources
            .iter()
            .filter(|s| s.is_drawing() && s.folder.starts_with(&path))
            .collect();
        let visible = members.iter().any(|s| s.visible);
        let locked = !members.is_empty() && members.iter().all(|s| s.locked);
        ui.push_id(&path, |ui| {
            ui.horizontal(|ui| {
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
                                egui::Label::new(
                                    egui::RichText::new(path.last().unwrap()).strong(),
                                )
                                .sense(egui::Sense::click())
                                .truncate(),
                            )
                        },
                    )
                    .inner;
                label.clone().on_hover_text(path.join(" / "));
                if config.enable_context_menu {
                    label.context_menu(|ui| {
                        if ui
                            .button(l10n::text(ui.ctx(), "New subfolder with selected drawings"))
                            .clicked()
                        {
                            state.creating_folder = Some(path.clone());
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
    action
}
