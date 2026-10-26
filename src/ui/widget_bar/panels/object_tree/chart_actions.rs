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
    /// Bulk actions apply to every selected drawing and study, and visibility includes the main series.
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
            | ObjectTreeAction::SelectAll => {
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
                if !source.is_drawing() && !source.is_indicator() {
                    return false;
                }
                delete(chart, source);
            }
            ObjectTreeAction::DeleteSelected => {
                for source in selected {
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
            ObjectTreeAction::BringToFront(id)
            | ObjectTreeAction::SendToBack(id)
            | ObjectTreeAction::MoveUp(id)
            | ObjectTreeAction::MoveDown(id) => {
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
                let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
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
    if source.is_indicator() {
        if let Some(id) = source.series_id {
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
                        chart.drawings_mut().add_synced_drawing(drawing);
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
}
