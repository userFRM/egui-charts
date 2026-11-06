//! Apply object tree operations to the attached chart.

use crate::Chart;
use crate::widget::SeriesId;

use super::{ObjectTreeAction, ObjectTreePanel, SourceItem, SourceType};

impl ObjectTreePanel {
    /// Apply a tree action to a chart. Drawing rows use drawing ids; study rows bind their
    /// chart series through [`SourceItem::with_series_id`]. Call this after [`Self::show`].
    ///
    /// Bind the main data source with `with_series_id(SeriesId::MAIN)` to control its visibility.
    /// Returns `false` for host-owned property dialogs, colour gestures, templates, main-series
    /// removal and renaming non-drawing rows. Drawing names are saved and undone with the drawing.
    /// Bulk actions apply to drawings, studies and added bar series. Visibility includes the main
    /// series; bar-series ordering stays within its pane.
    pub fn apply_to_chart(
        &mut self,
        chart: &mut Chart,
        action: &ObjectTreeAction,
        sources: &[SourceItem],
    ) -> bool {
        let selected: Vec<_> = sources
            .iter()
            .filter(|s| self.state().is_selected(s.id))
            .collect();
        match action {
            ObjectTreeAction::None => {}
            ObjectTreeAction::CreateFolder(parent, name) => {
                let name = name.trim();
                if name.is_empty() || (!parent.is_empty() && !self.state().folders.contains(parent))
                {
                    return false;
                }
                let mut path = parent.clone();
                path.push(name.to_owned());
                let ids: Vec<_> = selected
                    .iter()
                    .filter(|s| s.is_drawing())
                    .map(|s| s.id)
                    .collect();
                let folders = super::folders::selected_folders(self.state(), sources, &path);
                if ids.is_empty()
                    || self.state().folders.contains(&path)
                    || !chart.drawings_mut().move_to_folder(&ids, &folders, &path)
                {
                    return false;
                }
                self.state_mut().folders.insert(path.clone());
                self.state_mut().expanded_groups.insert("Drawings".into());
                for depth in 1..=path.len() {
                    self.state_mut().collapsed_folders.remove(&path[..depth]);
                }
            }
            ObjectTreeAction::MoveToFolder(ids, path) => {
                if !path.is_empty() && !self.state().folders.contains(path) {
                    return false;
                }
                let ids: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| {
                        ids.contains(&d.id)
                            && d.folder != *path
                            && sources.iter().any(|s| s.id == d.id && s.is_drawing())
                    })
                    .map(|d| d.id)
                    .collect();
                chart.drawings_mut().move_to_folder(&ids, &[], path);
            }
            ObjectTreeAction::MoveFolder(folder, parent) => {
                if !self.state().folders.contains(folder)
                    || (!parent.is_empty() && !self.state().folders.contains(parent))
                {
                    return false;
                }
                let moved =
                    chart
                        .drawings_mut()
                        .move_to_folder(&[], std::slice::from_ref(folder), parent);
                if moved {
                    let target = [parent.as_slice(), &folder[folder.len() - 1..]].concat();
                    self.state_mut()
                        .collapsed_folders
                        .retain(|p| !p.starts_with(folder) && !p.starts_with(&target));
                }
                return moved;
            }
            ObjectTreeAction::DeleteFolder(path) => {
                if !self.state().folders.contains(path) {
                    return false;
                }
                let ids: Vec<_> = sources
                    .iter()
                    .filter(|s| s.is_drawing() && s.folder.starts_with(path))
                    .map(|s| s.id)
                    .collect();
                chart.drawings_mut().delete_drawings(&ids);
                self.state_mut().selected_ids.retain(|id| !ids.contains(id));
                // A folder kept by its locked drawings stays collapsed.
                self.state_mut().collapsed_folders.retain(|p| {
                    !p.starts_with(path)
                        || chart
                            .drawings()
                            .drawings
                            .iter()
                            .any(|d| d.folder.starts_with(p))
                });
            }
            ObjectTreeAction::SetFolderSync(path, sync) => {
                return chart.drawings_mut().set_folder_sync(path, *sync);
            }
            ObjectTreeAction::SetFolderVisible(path, value)
            | ObjectTreeAction::SetFolderLocked(path, value) => {
                if path.is_empty() || !self.state().folders.contains(path) {
                    return false;
                }
                let ids: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| {
                        d.folder.starts_with(path)
                            && match action {
                                ObjectTreeAction::SetFolderVisible(_, _) => d.visible != *value,
                                _ => d.locked != *value,
                            }
                    })
                    .map(|d| d.id)
                    .collect();
                chart.drawings_mut().edit_drawings(&ids, |d| match action {
                    ObjectTreeAction::SetFolderVisible(_, _) => d.visible = *value,
                    _ => d.locked = *value,
                });
            }
            ObjectTreeAction::Rename(id, name) => {
                if !sources.iter().any(|s| s.id == *id && s.is_drawing()) {
                    return false;
                }
                if !name.trim().is_empty() {
                    chart
                        .drawings_mut()
                        .edit_drawing(*id, |d| d.label = Some(name.clone()));
                }
            }
            ObjectTreeAction::Select(id) => {
                chart.clear_selection();
                if sources.iter().any(|s| s.id == *id && s.is_drawing()) {
                    chart.drawings_mut().select(*id);
                    chart.reveal_drawing(*id);
                } else {
                    chart.drawings_mut().deselect();
                }
            }
            ObjectTreeAction::ClearSelection => {
                chart.drawings_mut().deselect();
                chart.clear_selection();
            }
            ObjectTreeAction::AddToSelection(_)
            | ObjectTreeAction::RangeSelect(_)
            | ObjectTreeAction::SelectAll
            | ObjectTreeAction::SelectFolder(_)
            | ObjectTreeAction::AddFolderToSelection(_) => {
                chart.clear_selection();
                chart
                    .drawings_mut()
                    .select_drawings(selected.iter().filter(|s| s.is_drawing()).map(|s| s.id));
            }
            ObjectTreeAction::ShowAll | ObjectTreeAction::HideAll => {
                let visible = matches!(action, ObjectTreeAction::ShowAll);
                for source in sources {
                    set_visible(chart, source, visible);
                }
            }
            ObjectTreeAction::HideSelected => {
                for source in selected {
                    set_visible(chart, source, false);
                }
            }
            ObjectTreeAction::ToggleVisibility(id) => {
                let Some(source) = sources.iter().find(|s| s.id == *id) else {
                    return false;
                };
                if !set_visible(chart, source, !source.visible) {
                    return false;
                }
            }
            ObjectTreeAction::ToggleLock(id) => {
                chart
                    .drawings_mut()
                    .edit_drawing(*id, |d| d.locked = !d.locked);
            }
            ObjectTreeAction::LockAll | ObjectTreeAction::UnlockAll => {
                let locked = matches!(action, ObjectTreeAction::LockAll);
                let ids: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| d.locked != locked)
                    .map(|d| d.id)
                    .collect();
                for id in ids {
                    chart.drawings_mut().edit_drawing(id, |d| d.locked = locked);
                }
            }
            ObjectTreeAction::Delete(id) | ObjectTreeAction::RemoveIndicator(id) => {
                let Some(source) = sources.iter().find(|s| s.id == *id) else {
                    return false;
                };
                if !source.is_drawing() && !source.is_indicator() && !source.is_added_series() {
                    return false;
                }
                delete(chart, source);
            }
            ObjectTreeAction::DeleteSelected => {
                let (drawings, others): (Vec<_>, Vec<_>) =
                    selected.into_iter().partition(|s| s.is_drawing());
                let ids: Vec<_> = drawings.iter().map(|s| s.id).collect();
                chart.drawings_mut().delete_drawings(&ids);
                for source in others {
                    delete(chart, source);
                }
                self.state_mut().clear_selection();
                chart.drawings_mut().deselect();
            }
            ObjectTreeAction::Duplicate(id) => {
                let Some(source) = sources.iter().find(|s| s.id == *id) else {
                    return false;
                };
                if !source.is_drawing() && !source.is_indicator() {
                    return false;
                }
                duplicate(chart, source);
            }
            ObjectTreeAction::DuplicateSelected => {
                for source in selected {
                    duplicate(chart, source);
                }
            }
            ObjectTreeAction::MoveToPane(id, pane) => {
                if !sources.iter().any(|s| s.id == *id && s.is_drawing())
                    || !chart.move_drawing_to_pane(*id, *pane)
                {
                    return false;
                }
            }
            ObjectTreeAction::BringToFront(id)
            | ObjectTreeAction::SendToBack(id)
            | ObjectTreeAction::MoveUp(id)
            | ObjectTreeAction::MoveDown(id) => {
                if let Some(series) = sources
                    .iter()
                    .find(|s| s.id == *id && s.source_type == SourceType::DataSource)
                    .and_then(|s| s.series_id)
                {
                    let Some(order) = chart.series_order(series) else {
                        return false;
                    };
                    let order = match action {
                        ObjectTreeAction::BringToFront(_) => usize::MAX,
                        ObjectTreeAction::SendToBack(_) => 0,
                        ObjectTreeAction::MoveUp(_) => order.saturating_add(1),
                        _ => order.saturating_sub(1),
                    };
                    return chart.set_series_order(series, order);
                }
                let mut order: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| (d.id, d.z_order))
                    .collect();
                order.sort_by_key(|(_, z)| *z);
                let Some(index) = order.iter().position(|(candidate, _)| candidate == id) else {
                    return false;
                };
                let other = match action {
                    ObjectTreeAction::BringToFront(_) => Some(order.len() - 1),
                    ObjectTreeAction::SendToBack(_) => Some(0),
                    ObjectTreeAction::MoveUp(_) => {
                        index.checked_add(1).filter(|next| *next < order.len())
                    }
                    _ => index.checked_sub(1),
                };
                if let Some(other) = other.filter(|other| *other != index) {
                    let target = order.remove(index);
                    order.insert(other, target);
                    // Normalise tied layers so exactly one neighbour is crossed.
                    let layers: Vec<_> = order
                        .into_iter()
                        .enumerate()
                        .filter_map(|(z, (id, _))| i32::try_from(z).ok().map(|z| (id, z)))
                        .collect();
                    chart.drawings_mut().reorder_drawings(*id, &layers);
                }
            }
            ObjectTreeAction::ZoomTo(id) => {
                if let Some(series) = sources
                    .iter()
                    .find(|s| s.id == *id && s.is_added_series())
                    .and_then(|s| s.series_id)
                {
                    if let Some(time) = chart.bars(series).last().map(|bar| bar.time) {
                        chart.scroll_to_time(time);
                        if let Some(scale) = chart.series_price_scale_mut(series) {
                            scale.reset();
                        }
                    }
                    return true;
                }
                if !sources.iter().any(|s| s.id == *id && s.is_drawing())
                    || !chart.drawings().drawings.iter().any(|d| d.id == *id)
                {
                    return false;
                }
                chart.clear_selection();
                chart.drawings_mut().select(*id);
                chart.reveal_drawing(*id);
            }
            ObjectTreeAction::RemoveAllDrawings => chart.drawings_mut().clear_all(),
            ObjectTreeAction::RemoveAllIndicators => {
                let ids: Vec<SeriesId> = chart
                    .indicators()
                    .filter(|(_, study)| study.user_edit_enabled())
                    .map(|(id, _)| id)
                    .collect();
                for id in ids {
                    chart.remove_series(id);
                }
            }
            _ => return false,
        }
        true
    }
}

fn set_visible(chart: &mut Chart, source: &SourceItem, visible: bool) -> bool {
    if source.source_type == SourceType::DataSource && source.series_id == Some(SeriesId::MAIN) {
        chart.set_main_series_visible(visible);
        true
    } else if source.is_added_series() {
        source
            .series_id
            .is_some_and(|id| chart.set_series_visible(id, visible))
    } else if source.is_indicator() {
        source
            .series_id
            .is_some_and(|id| chart.set_indicator_visible(id, visible))
    } else if source.is_drawing() {
        chart
            .drawings_mut()
            .edit_drawing(source.id, |d| d.visible = visible);
        true
    } else {
        false
    }
}

fn delete(chart: &mut Chart, source: &SourceItem) {
    if source.is_indicator() || source.is_added_series() {
        if let Some(id) = source.series_id
            && (source.is_added_series() || chart.indicator_user_edit_enabled(id))
        {
            chart.remove_series(id);
        }
    } else if source.is_drawing() {
        chart.drawings_mut().delete_drawing(source.id);
    }
}

fn duplicate(chart: &mut Chart, source: &SourceItem) {
    if source.source_type == SourceType::Indicator {
        let copy = chart
            .indicators()
            .find(|(id, _)| Some(*id) == source.series_id)
            .map(|(id, i)| {
                (
                    i.clone_box(),
                    chart.indicator_inputs(id).unwrap_or_default().to_owned(),
                )
            });
        if let Some((copy, inputs)) = copy {
            let id = chart.add_indicator(copy);
            chart.set_indicator_inputs(id, inputs);
        }
    } else if source.is_drawing() {
        chart.drawings_mut().duplicate_drawing(source.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::LogicalRange;
    use crate::widget::test_frames::{chart, frame};

    #[test]
    fn tree_selection_reveals_drawings_without_refitting_scales() {
        for tool in [
            DrawingToolType::AnchoredVWAP,
            DrawingToolType::AnchoredVolumeProfile,
        ] {
            for zoom in [false, true] {
                for manual in [false, true] {
                    // A 100-point manual span pans by 25 to reveal 80 or 220 with 5 points of margin.
                    for (index, price, low, high) in [
                        (10.0, 150.0, 100.0, 200.0),
                        (70.0, 150.0, 100.0, 200.0),
                        (110.0, 150.0, 100.0, 200.0),
                        (70.0, 80.0, 75.0, 175.0),
                        (70.0, 220.0, 125.0, 225.0),
                    ] {
                        let ctx = egui::Context::default();
                        let mut chart = chart(&[100.0], &[200.0]);
                        frame(&ctx, &mut chart, 0.0, vec![]);
                        chart.set_visible_logical_range(LogicalRange::new(50.0, 89.0));
                        if manual {
                            chart.price_scale_mut().set_range(100.0, 200.0);
                        }
                        let mut drawing = Drawing::new(1, tool);
                        drawing.chart_points = vec![ChartPoint::new(index, price)];
                        drawing.completed = true;
                        chart.drawings_mut().add_synced_drawing(drawing).unwrap();
                        let id = chart.drawings().drawings[0].id;
                        let sources = [SourceItem::drawing(id, tool, egui::Color32::RED)];
                        let mut tree = ObjectTreePanel::new();
                        let action = if zoom {
                            ObjectTreeAction::ZoomTo(id)
                        } else {
                            ObjectTreeAction::Select(id)
                        };
                        let spacing = chart.get_time_scale_state().0;
                        assert!(tree.apply_to_chart(&mut chart, &action, &sources));
                        assert_eq!(chart.drawings().selected_ids().collect::<Vec<_>>(), [id]);
                        assert_eq!(chart.get_time_scale_state().0, spacing);
                        assert_eq!(chart.state().price_scale().is_manual(), manual);
                        let range = chart.visible_logical_range().unwrap();
                        assert!(
                            range.left <= index && index <= range.right,
                            "{action:?}: {range:?}, index {index}"
                        );
                        if index == 70.0 {
                            assert_eq!(range, LogicalRange::new(50.0, 89.0));
                        }
                        if manual {
                            assert_eq!(
                                chart.state().price_scale().range(),
                                crate::scales::PriceRange::new(low, high)
                            );
                        }
                    }
                }
            }
        }
    }

    /// The rows a host builds from the chart: the main series, the studies and the drawings.
    fn rows(chart: &Chart) -> Vec<SourceItem> {
        const STUDIES: usize = usize::MAX / 2;
        let mut main = SourceItem::data_source(usize::MAX, "AAPL").with_series_id(SeriesId::MAIN);
        main.visible = chart.is_main_series_visible();
        let mut rows = vec![main];
        for (id, study) in chart.indicators() {
            let item = SourceItem::indicator(STUDIES + id.0, study.name(), egui::Color32::GRAY)
                .with_series_id(id);
            rows.push(item);
        }
        for drawing in chart.drawings().listed_drawings() {
            let mut item = SourceItem::drawing(drawing.id, drawing.tool_type, drawing.color32());
            item.visible = drawing.visible;
            item.z_index = drawing.z_order;
            rows.push(item);
        }
        rows
    }

    /// Applies `action` as the host's handler does: one undoable step over the current rows.
    fn apply(tree: &mut ObjectTreePanel, chart: &mut Chart, action: ObjectTreeAction) {
        let rows = rows(chart);
        chart.user_action(|chart| tree.apply_to_chart(chart, &action, &rows));
    }

    #[test]
    fn deleting_added_series_preserves_read_only_studies() {
        for bulk in [false, true] {
            let mut chart = chart(&[100.0], &[200.0]);
            let study = chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
            chart.set_indicator_user_edit_enabled(study, false);
            let added = chart.add_series(crate::widget::SeriesOptions::default());
            let sources = [
                SourceItem::indicator(1, "SMA", egui::Color32::GRAY).with_series_id(study),
                SourceItem::data_source(2, "Comparison").with_series_id(added),
            ];
            let mut tree = ObjectTreePanel::new();
            if bulk {
                tree.state_mut().select(1);
                tree.state_mut().toggle_selection(2);
                tree.apply_to_chart(&mut chart, &ObjectTreeAction::DeleteSelected, &sources);
            } else {
                for id in [1, 2] {
                    tree.apply_to_chart(&mut chart, &ObjectTreeAction::Delete(id), &sources);
                }
            }
            assert!(chart.indicators().any(|(id, _)| id == study));
            assert!(chart.series().all(|(id, _)| id != added));
        }
    }

    #[test]
    fn layers_names_copies_and_visibility_apply_as_undoable_steps() {
        let mut chart = chart(&[100.0], &[200.0]);
        let mut tree = ObjectTreePanel::new();
        for id in 1..=3 {
            let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 200.0)];
            drawing.completed = true;
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        }
        let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
        let z_order = |chart: &Chart| {
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.z_order)
                .collect::<Vec<_>>()
        };
        let painted = |chart: &Chart| {
            let mut order: Vec<_> = chart
                .drawings()
                .drawings
                .iter()
                .map(|d| (d.z_order, d.id))
                .collect();
            order.sort_by_key(|(z, _)| *z);
            order.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
        };
        // The main series hides and shows again through its row.
        apply(
            &mut tree,
            &mut chart,
            ObjectTreeAction::ToggleVisibility(usize::MAX),
        );
        assert!(!chart.is_main_series_visible());
        assert!(chart.undo());
        assert!(chart.is_main_series_visible());
        apply(
            &mut tree,
            &mut chart,
            ObjectTreeAction::ToggleVisibility(ids[0]),
        );
        assert!(!chart.drawings().drawings[0].visible);
        assert!(chart.undo());
        assert!(chart.drawings().drawings[0].visible);
        // Tied layers still have a stable paint order. Each move crosses exactly one neighbour.
        for drawing in &mut chart.drawings_mut().drawings {
            drawing.z_order = 0;
        }
        for (action, expected) in [
            (
                ObjectTreeAction::MoveUp(ids[0]),
                vec![ids[1], ids[0], ids[2]],
            ),
            (ObjectTreeAction::MoveDown(ids[0]), ids.clone()),
            (
                ObjectTreeAction::BringToFront(ids[0]),
                vec![ids[1], ids[2], ids[0]],
            ),
            (ObjectTreeAction::SendToBack(ids[0]), ids.clone()),
        ] {
            let before = z_order(&chart);
            apply(&mut tree, &mut chart, action);
            let after = z_order(&chart);
            assert!(chart.undo());
            assert_eq!(z_order(&chart), before);
            assert!(chart.redo());
            assert_eq!(z_order(&chart), after);
            assert_eq!(painted(&chart), expected);
        }
        apply(
            &mut tree,
            &mut chart,
            ObjectTreeAction::Rename(ids[0], "Support".into()),
        );
        assert_eq!(
            chart.drawings().drawings[0].label.as_deref(),
            Some("Support")
        );
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].label, None);
        apply(&mut tree, &mut chart, ObjectTreeAction::Duplicate(ids[0]));
        assert_eq!(chart.drawings().drawings.len(), 4);
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings.len(), 3);
        // A copied study keeps its inputs.
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
        chart.set_indicator_inputs(study, "2 close");
        let before: Vec<_> = chart.indicators().map(|(id, _)| id).collect();
        apply(
            &mut tree,
            &mut chart,
            ObjectTreeAction::Duplicate(usize::MAX / 2 + study.0),
        );
        let copy = chart
            .indicators()
            .map(|(id, _)| id)
            .find(|id| !before.contains(id))
            .unwrap();
        assert_eq!(chart.indicator_inputs(copy), Some("2 close"));
    }

    /// A new subfolder made inside a fully selected folder takes that folder's subfolders whole.
    /// Folder A holds one drawing directly, one in A/B and one in A/C; clicking A selects all
    /// three. Expected by the contract that a new folder keeps the structure of every folder whose
    /// drawings are all selected, except the folders the new one is created in, in one undo step.
    #[test]
    fn new_subfolder_keeps_the_structure_of_a_selected_folder() {
        let folders = |chart: &Chart| {
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.folder.join("/"))
                .collect::<Vec<_>>()
        };
        let mut observed = Vec::new();
        for parent in [&["A"][..], &["A", "B"]] {
            let mut chart = crate::widget::ChartBuilder::new().build();
            for folder in [&["A"][..], &["A", "B"], &["A", "C"]] {
                let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
                drawing.folder = folder.iter().map(|name| (*name).to_owned()).collect();
                drawing.chart_points =
                    vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
                drawing.completed = true;
                chart.drawings_mut().add_synced_drawing(drawing).unwrap();
            }
            let mut rows: Vec<_> = chart
                .drawings()
                .drawings
                .iter()
                .map(|d| {
                    let mut row = SourceItem::drawing(d.id, d.tool_type, d.color32());
                    row.folder = d.folder.clone();
                    row
                })
                .collect();
            let mut tree = ObjectTreePanel::new();
            // The tree derives its folders from the rows as it shows them.
            egui::Context::default()
                .run_ui(egui::RawInput::default(), |ui| {
                    tree.show(ui, None, &mut rows);
                })
                .drop_without_applying_deltas();
            let ids: Vec<_> = rows.iter().map(|row| row.id).collect();
            tree.state_mut().select_all(&ids);
            let parent: Vec<String> = parent.iter().map(|name| (*name).to_owned()).collect();
            assert!(tree.apply_to_chart(
                &mut chart,
                &ObjectTreeAction::CreateFolder(parent, "Sub".into()),
                &rows,
            ));
            observed.push(folders(&chart));
            assert!(chart.drawings_mut().undo());
            assert_eq!(folders(&chart), ["A", "A/B", "A/C"]);
        }
        assert_eq!(
            observed,
            [
                vec!["A/Sub", "A/Sub/B", "A/Sub/C"],
                vec!["A/B/Sub", "A/B/Sub", "A/B/Sub/C"],
            ],
            "folders after creating Sub under A, then under A/B: {observed:?}"
        );
    }
}
