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
                if !sources.iter().any(|s| s.id == *id && s.is_drawing()) {
                    return false;
                }
                let Some(drawing) = chart.drawings().drawings.iter().find(|d| d.id == *id) else {
                    return false;
                };
                let points: Vec<_> = drawing
                    .chart_points
                    .iter()
                    .filter(|p| p.bar_idx.is_finite() && p.price.is_finite())
                    .collect();
                if points.is_empty() {
                    return true;
                }
                let min_bar = points
                    .iter()
                    .map(|p| p.bar_idx)
                    .fold(f32::INFINITY, f32::min);
                let max_bar = points
                    .iter()
                    .map(|p| p.bar_idx)
                    .fold(f32::NEG_INFINITY, f32::max);
                let mut transform = chart.get_chart_mapping().price.transform();
                if matches!(
                    transform.mode,
                    crate::scales::PriceScaleMode::Percentage
                        | crate::scales::PriceScaleMode::IndexedTo100
                ) {
                    let start = (min_bar - ((max_bar - min_bar) * 0.1).max(2.0))
                        .floor()
                        .max(0.0) as usize;
                    if let Some(bar) = chart.state().shown().get(start) {
                        transform.base = bar.close;
                    }
                }
                let min_price = points
                    .iter()
                    .map(|p| transform.to_scale(p.price))
                    .fold(f64::INFINITY, f64::min);
                let max_price = points
                    .iter()
                    .map(|p| transform.to_scale(p.price))
                    .fold(f64::NEG_INFINITY, f64::max);
                let padding = ((max_bar - min_bar) * 0.1).max(2.0);
                chart.set_start_idx((min_bar - padding).floor().max(0.0) as usize);
                chart.set_visible_bars(
                    ((max_bar - min_bar + 2.0 * padding).ceil() as usize).saturating_add(1),
                );
                if min_price.is_finite() && max_price.is_finite() {
                    let pad = ((max_price - min_price) * 0.1).max(min_price.abs().max(1.0) * 0.001);
                    let low = min_price - pad;
                    let high = max_price + pad;
                    if low.is_finite() && high.is_finite() {
                        chart.price_scale_mut().set_range(low, high);
                    }
                }
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
            .map(|(_, i)| i.clone_box());
        if let Some(copy) = copy {
            chart.add_indicator(copy);
        }
    } else if source.is_drawing() {
        chart.drawings_mut().duplicate_drawing(source.id);
    }
}
