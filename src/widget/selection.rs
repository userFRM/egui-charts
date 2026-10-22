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

    /// Returns the bar index at which the current selection was made, if any.
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
    /// the appropriate menu at [`RightClickTarget`]`::pos`. The turnkey menu in
    /// [`TradingChart`](crate::TradingChart) consumes it for you.
    ///
    /// Carries no `ui`-feature types, so it is available with
    /// `--no-default-features`.
    pub fn take_right_click(&mut self) -> Option<RightClickTarget> {
        self.right_click.take()
    }

    /// Drains a pane-indicator remove request captured during the last frame.
    ///
    /// Returns `Some(index)` exactly once when the user clicks the close "x" on
    /// an indicator pane's legend, where `index` is the indicator's position in
    /// the registry passed to [`Chart::show_with_indicators`]. The chart does
    /// not own the registry, so the host performs the actual removal (e.g.
    /// [`IndicatorRegistry::remove_indicator`](crate::studies::IndicatorRegistry::remove_indicator)
    /// followed by a recompute); the pane layout reflows on the next frame.
    ///
    /// Carries no `ui`-feature types, so it is available with
    /// `--no-default-features`.
    pub fn take_indicator_remove(&mut self) -> Option<usize> {
        self.indicator_remove.take()
    }

    /// Draws selection handles for the element currently selected on the main
    /// chart (an overlay indicator line or a price/volume series).
    ///
    /// Pane-indicator handles are drawn separately by `show_with_indicators`
    /// since their panes live outside the main chart rect.
    pub(super) fn render_main_chart_selection(
        &self,
        painter: &egui::Painter,
        indicators: Option<&IndicatorRegistry>,
        coords: &ChartMapping,
        visible_data: &[crate::model::Bar],
        visible_range: std::ops::Range<usize>,
    ) {
        let Some(selected) = self.selection.selected_id() else {
            return;
        };

        match selected {
            ChartElementId::OverlayIndicator(idx) => {
                let Some(registry) = indicators else { return };
                let Some(indicator) = registry.indicators().get(idx) else {
                    return;
                };
                if !indicator.is_visible() || !indicator.is_overlay() {
                    return;
                }
                let config = SelectionDotConfig {
                    dot_interval: calculate_dot_interval(coords.bar_spacing),
                    ..Default::default()
                };
                // Multi-line indicators highlight every line so the whole study
                // reads as selected.
                for line_idx in 0..indicator.line_cnt() {
                    render_indicator_selection_dots(
                        painter,
                        indicator.as_ref(),
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
                let closes: Vec<f64> = visible_data.iter().map(|b| b.close).collect();
                match series_id {
                    SeriesId::VOLUME => {
                        // Volume handles sit on the volume rect; render on the
                        // bar tops at the configured interval.
                        let volume_coords = coords.with_rect(self.last_rendered_volume_rect);
                        let max_volume = visible_data
                            .iter()
                            .map(|b| b.volume)
                            .fold(0.0_f64, f64::max)
                            .max(1.0);
                        render_candle_selection_dots(
                            painter,
                            visible_range,
                            &volume_coords,
                            &closes,
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
                            &closes,
                            |price| coords.price_to_y(price),
                            &config,
                        );
                    }
                }
            }
            // Pane indicators are handled where their panes are rendered.
            ChartElementId::PaneIndicator(_) => {}
        }
    }
}

/// Hit-test a click on the main chart area, applying selection priority.
///
/// Overlay indicator lines take priority over the series beneath them, matching
/// the visual stacking order. Returns the hit element and the bar index where
/// the hit occurred, or `None` when the click landed on empty chart area.
pub(super) fn hit_test_main_chart(
    click_pos: Pos2,
    indicators: Option<&IndicatorRegistry>,
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
    if let Some(registry) = indicators {
        for (idx, indicator) in registry.indicators().iter().enumerate() {
            if let Some(hit) = hit_test_indicator(
                click_pos,
                indicator.as_ref(),
                idx,
                visible_range.clone(),
                coords,
                |price| coords.price_to_y(price),
            ) {
                return Some((
                    ChartElementId::OverlayIndicator(hit.indicator_idx),
                    hit.bar_idx,
                ));
            }
        }
    }

    // 2. Main price series (candles/bars/line).
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

    // 3. Volume series (only when a volume pane was rendered).
    if let Some(volume_coords) = volume_coords
        && let Some(hit) =
            hit_test_volume(click_pos, bars, visible_range, volume_coords, max_volume)
    {
        return Some((ChartElementId::Series(hit.series_id), hit.bar_idx));
    }

    None
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::model::Bar;
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
        let mapping = ChartMapping::new(rect, 40.0, 0, bars.len() - 1, 0.0, 8.0, 16.0);
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
        let hit = hit_test_main_chart(pos, None, &mapping, None, &bars, 100.0, 0..bars.len());
        assert!(hit.is_none());
    }

    #[test]
    fn click_on_candle_selects_main_series() {
        let (mapping, bars) = fixture();
        // Click the center of bar 2's body (close = 12.4, open = 11.6).
        let bar = &bars[2];
        let mid_price = (bar.open + bar.close) / 2.0;
        let pos = Pos2::new(mapping.idx_to_x(2), mapping.price_to_y(mid_price));
        let hit = hit_test_main_chart(pos, None, &mapping, None, &bars, 100.0, 0..bars.len());
        assert_eq!(hit, Some((ChartElementId::Series(SeriesId::MAIN), 2)));
    }

    #[test]
    fn overlay_indicator_wins_over_series_at_same_point() {
        let (mapping, bars) = fixture();
        // Place an indicator value at bar 2 and bar 3 exactly on the candle's
        // close price so its line passes through the body. A click there must
        // resolve to the overlay, not the series beneath it.
        let mut values = vec![IndicatorValue::None; bars.len()];
        values[2] = IndicatorValue::Single(bars[2].close);
        values[3] = IndicatorValue::Single(bars[3].close);
        let registry = overlay_at(values);

        let pos = Pos2::new(mapping.idx_to_x(2), mapping.price_to_y(bars[2].close));
        let hit = hit_test_main_chart(
            pos,
            Some(&registry),
            &mapping,
            None,
            &bars,
            100.0,
            0..bars.len(),
        );
        assert_eq!(hit, Some((ChartElementId::OverlayIndicator(0), 2)));
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
            Some(&registry),
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
        let hit = hit_test_main_chart(pos, None, &mapping, None, &[], 100.0, 0..0);
        assert!(hit.is_none());
    }

    #[test]
    fn take_indicator_remove_drains_once() {
        use crate::model::BarData;

        let mut chart = Chart::new(BarData::default());
        // No request pending after construction.
        assert_eq!(chart.take_indicator_remove(), None);

        // A pane legend "x" click records the indicator's registry index.
        chart.indicator_remove = Some(2);
        assert_eq!(chart.take_indicator_remove(), Some(2));
        // The request is consumed exactly once, mirroring `take_right_click`.
        assert_eq!(chart.take_indicator_remove(), None);
    }
}
