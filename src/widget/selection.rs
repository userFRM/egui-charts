//! Selection API, main-chart hit testing and selection handles.

use super::{Chart, RightClickTarget};
use crate::chart::indicators::{
    SelectionDotConfig, hit_test_indicator, render_indicator_selection_dots,
};
use crate::chart::renderers::ChartMapping;
use crate::chart::selection::{ChartElementId, SeriesId};
use crate::chart::series::{
    SelectionHandleConfig, calculate_dot_interval, hit_test_candles, hit_test_volume,
    render_candle_selection_dots,
};
use crate::studies::IndicatorRegistry;
use egui::Pos2;

impl Chart {
    // =========================================================================
    // Selection API
    // =========================================================================

    /// Returns the chart element the user has currently selected, if any.
    ///
    /// Selection is driven by clicking a series, an overlay indicator line, or a
    /// separate-pane indicator line. Host apps read this to open a settings
    /// dialog for, or delete, the selected object. Returns `None` when nothing
    /// is selected (for example after a click on empty chart area).
    pub fn selected_element(&self) -> Option<ChartElementId> {
        self.selection.selected_id()
    }

    /// Returns the index at which the current selection was made, if any.
    /// On price-based types it counts elements, otherwise source bars.
    ///
    /// This is the data-space index of the segment that was clicked, useful for
    /// anchoring tooltips or context menus near the click.
    pub fn selected_bar(&self) -> Option<usize> {
        self.selection.selected_bar()
    }

    /// Clears any current selection.
    ///
    /// Equivalent to clicking empty chart area. Call this after the host app has
    /// finished acting on a selection (e.g. closing a settings dialog).
    pub fn clear_selection(&mut self) {
        self.selection.deselect();
    }

    /// Drains the right-click target captured during the last frame, if any.
    ///
    /// Returns `Some` exactly once per right-click: the chart hit-tests the
    /// cursor against drawings, indicators, and series (same priority as
    /// left-click selection) and records where the click landed. The hit object
    /// is also selected, so selection handles and any context menu stay
    /// consistent.
    ///
    /// Hosts that build their own context menus read this each frame and open
    /// the appropriate menu at [`RightClickTarget`]`::pos`. With the `ui`
    /// feature, `ui::context_menu::ChartMenus` consumes it for you.
    ///
    /// Carries no `ui`-feature types, so it is available without the `ui`
    /// feature.
    pub fn take_right_click(&mut self) -> Option<RightClickTarget> {
        self.right_click.take()
    }

    /// Draws selection handles for the element currently selected on the main
    /// chart (an overlay indicator line or a price/volume series).
    ///
    /// Handles of a study in a pane are drawn with its pane, which lies outside
    /// the main chart rect.
    pub(super) fn render_main_chart_selection(
        &self,
        painter: &egui::Painter,
        coords: &ChartMapping,
        visible_data: &[crate::model::Bar],
        visible_range: std::ops::Range<usize>,
    ) {
        let Some(selected) = self.selection.selected_id() else {
            return;
        };

        match selected {
            ChartElementId::Indicator(id) => {
                let Some((_, indicator)) = self.indicators().find(|(study, _)| *study == id) else {
                    return;
                };
                if !indicator.is_visible()
                    || !indicator.is_overlay()
                    || (indicator.is_volume() && !self.shows_volume())
                {
                    return;
                }
                let volume_coords = indicator.is_volume().then(|| {
                    super::volume::mapping(
                        coords,
                        super::volume::overlay_rect(coords.rect, &self.config),
                        indicator,
                        &visible_range,
                    )
                });
                let coords = volume_coords.as_ref().unwrap_or(coords);
                let config = SelectionDotConfig {
                    dot_interval: calculate_dot_interval(coords.bar_spacing),
                    ..Default::default()
                };
                // Multi-line indicators highlight every line so the whole study
                // reads as selected.
                for line_idx in 0..indicator.line_cnt() {
                    render_indicator_selection_dots(
                        painter,
                        indicator,
                        line_idx,
                        visible_range.clone(),
                        coords,
                        |price| coords.price_to_y(price),
                        &config,
                    );
                }
            }
            ChartElementId::Series(series_id) => {
                let config = SelectionHandleConfig {
                    dot_interval: calculate_dot_interval(coords.bar_spacing),
                    ..Default::default()
                };
                let values: Vec<f64> = visible_data
                    .iter()
                    .map(|b| {
                        if series_id == SeriesId::VOLUME {
                            b.volume
                        } else {
                            b.close
                        }
                    })
                    .collect();
                match series_id {
                    SeriesId::VOLUME => {
                        // Volume handles sit on the volume rect; render on the
                        // bar tops at the configured interval.
                        let volume_coords = coords.with_rect(self.last_rendered_volume_rect);
                        let max_volume = visible_data
                            .iter()
                            .map(|b| b.volume)
                            .fold(0.0_f64, f64::max);
                        if max_volume <= 0.0 {
                            return;
                        }
                        render_candle_selection_dots(
                            painter,
                            visible_range,
                            &volume_coords,
                            &values,
                            |volume| {
                                let norm = (volume / max_volume) as f32;
                                volume_coords.rect.bottom() - norm * volume_coords.rect.height()
                            },
                            &config,
                        );
                    }
                    _ => {
                        render_candle_selection_dots(
                            painter,
                            visible_range,
                            coords,
                            &values,
                            |price| coords.price_to_y(price),
                            &config,
                        );
                    }
                }
            }
        }
    }
}

/// Hit-test a click on the main chart area, applying selection priority.
///
/// Overlay indicator lines take priority over the series beneath them, matching
/// the visual stacking order. `study_ids` holds the id of each study in
/// `studies`. Returns the hit element and the bar index where the hit occurred,
/// or `None` when the click landed on empty chart area.
pub(super) fn hit_test_main_chart(
    click_pos: Pos2,
    studies: &IndicatorRegistry,
    study_ids: &[SeriesId],
    coords: &ChartMapping,
    volume_coords: Option<&ChartMapping>,
    bars: &[crate::model::Bar],
    max_volume: f64,
    visible_range: std::ops::Range<usize>,
) -> Option<(ChartElementId, usize)> {
    if bars.is_empty() {
        return None;
    }

    // 1. Overlay indicators (drawn on top of the series).
    for (idx, indicator) in studies.indicators().iter().enumerate() {
        let volume_mapping = if indicator.is_volume() {
            if !indicator.is_overlay() || !indicator.is_visible() {
                continue;
            }
            let Some(volume) = volume_coords else {
                continue;
            };
            Some(super::volume::mapping(
                coords,
                volume.rect,
                indicator.as_ref(),
                &visible_range,
            ))
        } else {
            None
        };
        let coords = volume_mapping.as_ref().unwrap_or(coords);
        if let Some(hit) = hit_test_indicator(
            click_pos,
            indicator.as_ref(),
            idx,
            visible_range.clone(),
            coords,
            |price| coords.price_to_y(price),
        ) {
            return Some((
                ChartElementId::Indicator(study_ids[hit.indicator_idx]),
                hit.bar_idx,
            ));
        }
    }

    // 2. Volume is painted over the price series.
    if !studies
        .indicators()
        .iter()
        .any(|study| study.is_volume() && study.is_overlay())
        && let Some(volume_coords) = volume_coords
        && let Some(hit) = hit_test_volume(
            click_pos,
            bars,
            visible_range.clone(),
            volume_coords,
            max_volume,
        )
    {
        return Some((ChartElementId::Series(hit.series_id), hit.bar_idx));
    }

    // 3. Main price series (candles/bars/line).
    if let Some(hit) = hit_test_candles(
        click_pos,
        bars,
        visible_range.clone(),
        coords,
        |price| coords.price_to_y(price),
        &crate::chart::series::HitTestConfig::default(),
    ) {
        return Some((ChartElementId::Series(hit.series_id), hit.bar_idx));
    }

    None
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::model::Bar;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::{CustomIndicator, IndicatorValue};
    use chrono::{TimeZone, Utc};
    use egui::{Pos2, Rect, Vec2};

    /// Build a deterministic mapping plus a small candle series.
    ///
    /// The series is flat OHLC at the integer prices `[10, 11, 12, 13, 14]` so
    /// every candle has a visible body and an exact price-to-screen mapping.
    fn fixture() -> (ChartMapping, Vec<Bar>) {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(500.0, 400.0));
        let bars: Vec<Bar> = (0..5)
            .map(|i| {
                let close = 10.0 + i as f64;
                Bar::new(
                    Utc.timestamp_opt(i as i64, 0).unwrap(),
                    close - 0.4,
                    close + 0.5,
                    close - 0.5,
                    close + 0.4,
                    100.0,
                )
            })
            .collect();
        // base_idx = last bar, no scroll offset, price window covers the data.
        let range = PriceRange::new(8.0, 16.0);
        let price = PriceMap::new(
            PriceTransform::default(),
            range,
            rect.min.y,
            rect.max.y,
            false,
        );
        let mapping = ChartMapping::new(rect, 40.0, 0, bars.len() - 1, 0.0, price);
        (mapping, bars)
    }

    fn overlay_at(values: Vec<IndicatorValue>) -> IndicatorRegistry {
        let mut registry = IndicatorRegistry::new();
        let captured = values.clone();
        registry.add(Box::new(
            CustomIndicator::new("TestLine", move |_| captured.clone()).with_overlay(true),
        ));
        // Prime the cached values without needing real bar input.
        registry.calculate_all(&[]);
        registry
    }

    #[test]
    fn click_on_empty_area_returns_none() {
        let (mapping, bars) = fixture();
        // A point near the top of the chart, above every candle body and on no
        // indicator line.
        let pos = Pos2::new(mapping.idx_to_x(2), mapping.rect.min.y + 1.0);
        let hit = hit_test_main_chart(
            pos,
            &IndicatorRegistry::new(),
            &[],
            &mapping,
            None,
            &bars,
            100.0,
            0..bars.len(),
        );
        assert!(hit.is_none());
    }

    #[test]
    fn click_on_candle_selects_main_series() {
        let (mapping, bars) = fixture();
        // Click the center of bar 2's body (close = 12.4, open = 11.6).
        let bar = &bars[2];
        let mid_price = (bar.open + bar.close) / 2.0;
        let pos = Pos2::new(mapping.idx_to_x(2), mapping.price_to_y(mid_price));
        let hit = hit_test_main_chart(
            pos,
            &IndicatorRegistry::new(),
            &[],
            &mapping,
            None,
            &bars,
            100.0,
            0..bars.len(),
        );
        assert_eq!(hit, Some((ChartElementId::Series(SeriesId::MAIN), 2)));
    }

    #[test]
    fn selection_follows_overlay_volume_and_price_paint_order() {
        let (mapping, bars) = fixture();
        // Place an indicator value at bar 2 and bar 3 exactly on the candle's
        // close price so its line passes through the body. A click there must
        // resolve to the overlay, not the series beneath it.
        let mut values = vec![IndicatorValue::None; bars.len()];
        values[2] = IndicatorValue::Single(bars[2].close);
        values[3] = IndicatorValue::Single(bars[3].close);
        let registry = overlay_at(values);

        let pos = Pos2::new(mapping.idx_to_x(2), mapping.price_to_y(bars[2].close));
        let volume = mapping.with_rect(egui::Rect::from_min_max(
            Pos2::new(mapping.rect.left(), pos.y - 10.0),
            Pos2::new(mapping.rect.right(), pos.y + 10.0),
        ));
        let empty = IndicatorRegistry::new();
        for (studies, volume, expected) in [
            (
                &registry,
                Some(&volume),
                ChartElementId::Indicator(SeriesId(2)),
            ),
            (
                &empty,
                Some(&volume),
                ChartElementId::Series(SeriesId::VOLUME),
            ),
            (&empty, None, ChartElementId::Series(SeriesId::MAIN)),
        ] {
            let hit = hit_test_main_chart(
                pos,
                studies,
                &[SeriesId(2)],
                &mapping,
                volume,
                &bars,
                100.0,
                0..bars.len(),
            );
            assert_eq!(hit, Some((expected, 2)));
        }
    }

    #[test]
    fn hidden_or_pane_indicator_does_not_steal_overlay_priority() {
        let (mapping, bars) = fixture();
        // A non-overlay (pane) indicator must be ignored by the main-chart hit
        // test even though its values sit on the candle body.
        let mut values = vec![IndicatorValue::None; bars.len()];
        values[2] = IndicatorValue::Single(bars[2].close);
        let mut registry = IndicatorRegistry::new();
        let captured = values.clone();
        registry.add(Box::new(
            CustomIndicator::new("Pane", move |_| captured.clone()).with_overlay(false),
        ));
        registry.calculate_all(&[]);

        let bar = &bars[2];
        let mid_price = (bar.open + bar.close) / 2.0;
        let pos = Pos2::new(mapping.idx_to_x(2), mapping.price_to_y(mid_price));
        let hit = hit_test_main_chart(
            pos,
            &registry,
            &[SeriesId(2)],
            &mapping,
            None,
            &bars,
            100.0,
            0..bars.len(),
        );
        // Falls through to the series, since the pane indicator is not an overlay.
        assert_eq!(hit, Some((ChartElementId::Series(SeriesId::MAIN), 2)));
    }

    #[test]
    fn empty_data_returns_none() {
        let (mapping, _) = fixture();
        let pos = mapping.rect.center();
        let hit = hit_test_main_chart(
            pos,
            &IndicatorRegistry::new(),
            &[],
            &mapping,
            None,
            &[],
            100.0,
            0..0,
        );
        assert!(hit.is_none());
    }

    /// Guards stable study ids: with ids taken from positions, removing a study moved the
    /// selection to the study after it, and removing the selected study must clear the
    /// selection instead of leaving it on a study that is gone.
    #[test]
    fn removing_a_study_keeps_the_selection_on_its_id() {
        use crate::model::BarData;

        let mut chart = Chart::new(BarData::default());
        let ids: Vec<SeriesId> = ["A", "B", "C"]
            .into_iter()
            .map(|name| {
                let study =
                    CustomIndicator::new(name, |bars| vec![IndicatorValue::None; bars.len()]);
                chart.add_indicator(Box::new(study.with_overlay(false)))
            })
            .collect();
        chart
            .selection
            .select(ChartElementId::Indicator(ids[2]), None);
        assert!(chart.remove_series(ids[0]));
        assert_eq!(
            chart.selected_element(),
            Some(ChartElementId::Indicator(ids[2]))
        );
        assert!(chart.remove_series(ids[2]));
        assert_eq!(chart.selected_element(), None);
    }
}
