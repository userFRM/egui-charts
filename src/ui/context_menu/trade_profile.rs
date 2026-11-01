//! Merge and split chart-owned and host-owned execution profiles.

use super::{ContextMenuAction, MenuEntry, MenuItem};
use crate::localization as l10n;
use crate::model::{BarClock, ChartType};
use crate::studies::TradeProfileRanges;
use crate::widget::Chart;
use chrono::{DateTime, Utc};
use egui::Pos2;
use std::ops::Range;

/// Shows merge/split commands for `index` inside a host menu. `split_at` is a bar boundary
/// selected by the user, such as the hovered bar. Returns whether the ranges changed; rebuild
/// the profiles with [`crate::studies::trade_profiles`] after a change.
pub fn trade_profile_menu(
    ui: &mut egui::Ui,
    ranges: &mut TradeProfileRanges,
    index: usize,
    split_at: Option<DateTime<Utc>>,
) -> bool {
    let merge = index < ranges.ranges().len().saturating_sub(1);
    if ui
        .add_enabled(
            merge,
            egui::Button::new(l10n::text(ui.ctx(), "Merge with next profile")),
        )
        .clicked()
    {
        ui.close();
        return ranges.merge_next(index);
    }
    let split = split_at.filter(|time| {
        ranges
            .ranges()
            .get(index)
            .is_some_and(|r| r.start < *time && *time < r.end)
    });
    if ui
        .add_enabled(
            split.is_some(),
            egui::Button::new(l10n::text(ui.ctx(), "Split profile at selected bar")),
        )
        .clicked()
    {
        ui.close();
        return split.is_some_and(|time| ranges.split(index, time));
    }
    false
}

pub(super) struct TradeProfileMenu {
    ranges: Vec<Range<DateTime<Utc>>>,
    index: usize,
    time: DateTime<Utc>,
    symbol: String,
    clock: BarClock,
}

impl TradeProfileMenu {
    pub(super) fn open(chart: &Chart, pos: Pos2, time: Option<DateTime<Utc>>) -> Option<Self> {
        if chart.chart_type() != ChartType::SessionVolumeProfile
            || !chart.is_main_series_visible()
            || !chart.get_rendered_price_rect().contains(pos)
        {
            return None;
        }
        let time = chart.profile_clock().bar_open(time?);
        let ranges = chart.trade_profile_ranges()?.ranges().to_vec();
        let index = ranges.iter().position(|r| r.contains(&time))?;
        Some(Self {
            ranges,
            index,
            time,
            symbol: chart.symbol().to_owned(),
            clock: chart.profile_clock(),
        })
    }

    pub(super) fn entries(&self) -> Vec<MenuEntry<ContextMenuAction>> {
        [
            (
                "Merge with previous profile",
                ContextMenuAction::MergeTradeProfilePrevious,
                self.index > 0,
            ),
            (
                "Merge with next profile",
                ContextMenuAction::MergeTradeProfileNext,
                self.index + 1 < self.ranges.len(),
            ),
            (
                "Split profile at selected bar",
                ContextMenuAction::SplitTradeProfile,
                self.time > self.ranges[self.index].start,
            ),
        ]
        .into_iter()
        .map(|(label, action, enabled)| {
            let mut item = MenuItem::new(label, action);
            item.enabled = enabled;
            MenuEntry::Item(item)
        })
        .collect()
    }

    pub(super) fn apply(&self, chart: &mut Chart, action: &ContextMenuAction) {
        if chart.chart_type() != ChartType::SessionVolumeProfile
            || chart.symbol() != self.symbol
            || chart.profile_clock() != self.clock
            || chart
                .trade_profile_ranges()
                .is_none_or(|r| r.ranges() != self.ranges)
        {
            return;
        }
        match action {
            ContextMenuAction::MergeTradeProfilePrevious => {
                if let Some(previous) = self.index.checked_sub(1) {
                    chart.merge_trade_profile(previous);
                }
            }
            ContextMenuAction::MergeTradeProfileNext => {
                chart.merge_trade_profile(self.index);
            }
            ContextMenuAction::SplitTradeProfile => {
                chart.split_trade_profile(self.index, self.time);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::overlays::{FootprintMode, TradeProfileSettings};
    use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, TradePage, TradeRequest};
    use crate::model::{Bar, Instrument, LogicalRange, OrderSide, Tick, Timeframe};
    use crate::theme::{Theme, apply_to_egui};
    use crate::ui::context_menu::{ChartContextAction, ChartMenus};
    use crate::widget::{ChartBuilder, SeriesId};
    use chrono::TimeDelta;
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The runtime painter and the actual chart menu share execution ranges, including a
    /// selected later session, a composite scrolled partly out of view, evicted history and
    /// bars timestamped inside their slots.
    #[test]
    fn execution_profile_settings_and_menu_reach_the_displayed_rows() {
        struct Feed {
            bars: Vec<Bar>,
            ticks: Vec<Tick>,
            partial: AtomicBool,
        }
        impl Datafeed for Feed {
            fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
                reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
            }
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                reply.send(Ok(HistoryPage::new(self.bars.clone(), true)));
            }
            fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
                let partial = self.partial.load(Ordering::Relaxed);
                let ticks = self
                    .ticks
                    .iter()
                    .filter(|t| {
                        !partial
                            || t.ts >= BarClock::new(Timeframe::Min1).bar_open(self.bars[2].time)
                    })
                    .cloned()
                    .collect();
                reply.send(Ok(TradePage::new(ticks, !partial)));
            }
        }
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let time = |day, minute| start + TimeDelta::days(day) + TimeDelta::minutes(minute);
        let bars: Vec<_> = (0..3)
            .flat_map(|day| {
                (0..2).map(move |minute| Bar::new(time(day, minute), 99.0, 104.0, 97.0, 102.0, 0.0))
            })
            .collect();
        let ticks: Vec<_> = [
            (0, 0, 99.0, 2.0, Some(OrderSide::Sell)),
            (0, 0, 100.0, 8.0, Some(OrderSide::Buy)),
            (0, 0, 101.0, 1.0, None),
            (0, 0, 102.0, 5.0, Some(OrderSide::Sell)),
            (0, 1, 99.0, 1.0, Some(OrderSide::Sell)),
            (1, 0, 100.0, 4.0, Some(OrderSide::Buy)),
            (1, 0, 101.0, 1.0, Some(OrderSide::Sell)),
            (1, 1, 101.0, 6.0, Some(OrderSide::Buy)),
            (2, 0, 99.0, 2.0, Some(OrderSide::Sell)),
            (2, 0, 102.0, 3.0, Some(OrderSide::Buy)),
            (2, 1, 101.0, 2.0, Some(OrderSide::Buy)),
        ]
        .into_iter()
        .map(|(day, minute, price, size, side)| Tick::new(time(day, minute), price, size, side))
        .collect();
        for (theme, stamp) in [
            (Theme::dark(), TimeDelta::zero()),
            (Theme::light(), TimeDelta::zero()),
            (Theme::dark(), TimeDelta::seconds(1)),
            (Theme::light(), TimeDelta::seconds(1)),
        ] {
            let bars: Vec<_> = bars
                .iter()
                .cloned()
                .map(|mut bar| {
                    bar.time += stamp;
                    bar
                })
                .collect();
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            let feed = Arc::new(Feed {
                bars: bars.clone(),
                ticks: ticks.clone(),
                partial: AtomicBool::new(false),
            });
            let mut chart = ChartBuilder::new()
                .with_symbol("PROFILE")
                .with_timeframe(Timeframe::Min1)
                .with_chart_type(ChartType::SessionVolumeProfile)
                .with_feed(feed.clone())
                .build();
            chart.set_trade_profile_settings(TradeProfileSettings {
                mode: FootprintMode::Delta,
                developing_poc: true,
                nodes: Some(crate::data::NodeSensitivity::default()),
                value_area: Some(0.6),
                value_area_guides: true,
            });
            let mut menus = ChartMenus::default();
            let frame = |chart: &mut Chart, menus: &mut ChartMenus, events| {
                let mut action = None;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        action = menus.show(ui.ctx(), chart);
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                (action, shapes)
            };
            for _ in 0..6 {
                frame(&mut chart, &mut menus, vec![]);
            }
            assert!(chart.set_visible_logical_range(LogicalRange::new(-0.5, 6.5)));
            let (_, painted) = frame(&mut chart, &mut menus, vec![]);
            let map = chart.get_chart_mapping();
            let has_line = |painted: &[egui::epaint::ClippedShape], a: Pos2, b: Pos2| {
                painted.iter().any(|shape| {
                    matches!(&shape.shape,
                    Shape::LineSegment { points, .. } if *points == [a, b])
                })
            };
            // Session 1 totals [4, 7] at [100, 101]: its cumulative POC moves from
            // 100 (4 > 1) to 101 (7 > 4), independent of its deltas [+4, +5].
            assert!(has_line(
                &painted,
                Pos2::new(map.idx_to_x(2), map.price_to_y(100.0)),
                Pos2::new(map.idx_to_x(3), map.price_to_y(101.0))
            ));
            for label in ["HVN", "LVN"] {
                assert!(
                    painted
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == label))
                );
            }
            // Session 0 totals [3, 8, 1, 5]. POC 100 plus row 99 reaches 11/17,
            // exceeding 60%; its guides bound [98.5, 100.5].
            let left = map.idx_to_x(0) - map.bar_spacing * 0.45;
            let right = map.idx_to_x(1) + map.bar_spacing * 0.5;
            for edge in [98.5, 100.5] {
                assert!(has_line(
                    &painted,
                    Pos2::new(left, map.price_to_y(edge)),
                    Pos2::new(right, map.price_to_y(edge))
                ));
            }
            for nodes in [
                None,
                Some(crate::data::NodeSensitivity {
                    smoothing: 0,
                    prominence: 1.0,
                }),
            ] {
                let mut settings = chart.trade_profile_settings().clone();
                settings.mode = FootprintMode::Volume;
                settings.developing_poc = false;
                settings.nodes = nodes;
                settings.value_area_guides = false;
                chart.set_trade_profile_settings(settings);
                let (_, hidden) = frame(&mut chart, &mut menus, vec![]);
                assert!(!has_line(
                    &hidden,
                    Pos2::new(map.idx_to_x(2), map.price_to_y(100.0)),
                    Pos2::new(map.idx_to_x(3), map.price_to_y(101.0))
                ));
                assert!(!hidden.iter().any(|s| matches!(&s.shape, Shape::Text(t) if ["HVN", "LVN"].contains(&t.galley.text()))));
                assert!(!has_line(
                    &hidden,
                    Pos2::new(left, map.price_to_y(98.5)),
                    Pos2::new(right, map.price_to_y(98.5))
                ));
                // Volume mode draws all 4 contracts at 100 from the profile's left edge,
                // relative to the largest total row, 7 at 101.
                let left = map.idx_to_x(2) - map.bar_spacing * 0.45;
                let width = map.idx_to_x(3) + map.bar_spacing * 0.5 - left;
                assert!(
                    hidden
                        .iter()
                        .any(|shape| matches!(&shape.shape, Shape::Rect(r)
                    if (r.rect.left() - left).abs() < 0.001
                    && (r.rect.width() - width * 4.0 / 7.0).abs() < 0.001
                    && r.rect.contains(Pos2::new(r.rect.center().x, map.price_to_y(100.0)))))
                );
            }
            let mut settings = chart.trade_profile_settings().clone();
            settings.mode = FootprintMode::Delta;
            settings.developing_poc = true;
            settings.nodes = Some(Default::default());
            settings.value_area_guides = true;
            chart.set_trade_profile_settings(settings);
            let click = |chart: &mut Chart, menus: &mut ChartMenus, pos, button| {
                frame(chart, menus, vec![Event::PointerMoved(pos)]);
                let mut action = None;
                for pressed in [true, false] {
                    action = action.or(frame(
                        chart,
                        menus,
                        vec![Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    )
                    .0);
                }
                action
            };
            let pick = |chart: &mut Chart, menus: &mut ChartMenus, bar, label: &str| {
                frame(chart, menus, vec![]);
                let map = chart.get_chart_mapping();
                click(
                    chart,
                    menus,
                    Pos2::new(map.idx_to_x(bar), map.price_to_y(101.0)),
                    PointerButton::Secondary,
                );
                let (_, shapes) = frame(chart, menus, vec![]);
                let row = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(t) if t.galley.text() == label => {
                            Some(t.pos + t.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("no {label} row"));
                click(chart, menus, row, PointerButton::Primary)
            };
            assert_eq!(chart.trade_profile_ranges().unwrap().ranges().len(), 3);
            assert!(matches!(
                pick(&mut chart, &mut menus, 2, "Merge with next profile"),
                Some(ChartContextAction::Chart(
                    ContextMenuAction::MergeTradeProfileNext
                ))
            ));
            let ranges = chart.trade_profile_ranges().unwrap();
            assert_eq!(ranges.ranges().len(), 2);
            assert_eq!(ranges.ranges()[1].start, time(1, 0));
            chart
                .export_layout_svg(
                    &ctx,
                    &crate::widget::ImageExportOptions::default(),
                    |ui, snapshot| {
                        snapshot.show(ui);
                        assert_eq!(snapshot.trade_profile_settings().mode, FootprintMode::Delta);
                        assert_eq!(snapshot.trade_profile_ranges().unwrap().ranges().len(), 2);
                    },
                )
                .unwrap();
            let (_, merged) = frame(&mut chart, &mut menus, vec![]);
            let map = chart.get_chart_mapping();
            // Merged rows: bid/ask 2/0, 0/4, 1/8, 0/3. Delta -2, +4, +7, +3.
            // The maximum total row is 9. Each signed bar starts at the centre.
            let left = map.idx_to_x(2) - map.bar_spacing * 0.45;
            let width = map.idx_to_x(5) + map.bar_spacing * 0.5 - left;
            let centre = left + width * 0.5;
            for (price, delta) in [(99.0, -2.0_f32), (100.0, 4.0), (101.0, 7.0), (102.0, 3.0)] {
                let end = centre + width * 0.5 * delta / 9.0;
                assert!(
                    merged
                        .iter()
                        .any(|shape| matches!(&shape.shape, Shape::Rect(r)
                    if (r.rect.left() - centre.min(end)).abs() < 0.001
                    && (r.rect.right() - centre.max(end)).abs() < 0.001
                    && r.rect.contains(Pos2::new(r.rect.center().x, map.price_to_y(price))))),
                    "delta at {price}"
                );
            }
            assert!(chart.set_visible_logical_range(LogicalRange::new(2.5, 6.5)));
            assert!(matches!(
                pick(&mut chart, &mut menus, 4, "Split profile at selected bar"),
                Some(ChartContextAction::Chart(
                    ContextMenuAction::SplitTradeProfile
                ))
            ));
            assert_eq!(chart.trade_profile_ranges().unwrap().ranges().len(), 3);
            let (_, split) = frame(&mut chart, &mut menus, vec![]);
            let map = chart.get_chart_mapping();
            assert!(has_line(
                &split,
                Pos2::new(map.idx_to_x(2), map.price_to_y(100.0)),
                Pos2::new(map.idx_to_x(3), map.price_to_y(101.0))
            ));
            let left = (map.idx_to_x(2) - map.bar_spacing * 0.45).max(map.rect.left());
            let width = map.idx_to_x(3) + map.bar_spacing * 0.5 - left;
            let centre = left + width * 0.5;
            for (price, delta) in [(100.0, 4.0), (101.0, 5.0)] {
                assert!(
                    split
                        .iter()
                        .any(|shape| matches!(&shape.shape, Shape::Rect(r)
                    if (r.rect.left() - centre).abs() < 0.001
                    && (r.rect.width() - width * 0.5 * delta / 7.0).abs() < 0.001
                    && r.rect.contains(Pos2::new(r.rect.center().x, map.price_to_y(price)))))
                );
            }
            assert!(!chart.split_trade_profile(1, time(1, 0) + TimeDelta::seconds(30)));
            assert!(chart.merge_trade_profile(1));
            let mut extended = bars.clone();
            extended.push(Bar::new(time(3, 0), 99.0, 104.0, 97.0, 102.0, 0.0));
            chart.set_data(SeriesId::MAIN, extended);
            assert_eq!(
                chart.trade_profile_ranges().unwrap().ranges().len(),
                3,
                "new session stays separate"
            );
            let mut prepended = bars.clone();
            prepended.insert(2, Bar::new(time(1, -1), 99.0, 104.0, 97.0, 102.0, 0.0));
            chart.set_data(SeriesId::MAIN, prepended);
            assert_eq!(
                chart.trade_profile_ranges().unwrap().ranges()[1].start,
                time(1, -1)
            );
            chart.set_data(SeriesId::MAIN, bars.clone());
            chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(3));
            chart.set_visible_logical_range(LogicalRange::new(-0.5, 3.5));
            let (_, covered) = frame(&mut chart, &mut menus, vec![]);
            assert!(!covered.iter().any(
                |s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Partial executions")
            ));
            let map = chart.get_chart_mapping();
            assert!(!has_line(
                &covered,
                Pos2::new(map.idx_to_x(0), map.price_to_y(100.0)),
                Pos2::new(map.idx_to_x(0), map.price_to_y(101.0))
            ));
            feed.partial.store(true, Ordering::Relaxed);
            chart.reload_feed();
            for _ in 0..6 {
                frame(&mut chart, &mut menus, vec![]);
            }
            chart.set_visible_logical_range(LogicalRange::new(-0.5, 3.5));
            let (_, evicted) = frame(&mut chart, &mut menus, vec![]);
            let ranges = chart.trade_profile_ranges().unwrap();
            assert_eq!(ranges.ranges()[0].start, time(1, 0));
            assert!(evicted.iter().any(
                |s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Partial executions")
            ));
            let map = chart.get_chart_mapping();
            assert!(has_line(
                &evicted,
                Pos2::new(map.idx_to_x(0), map.price_to_y(101.0)),
                Pos2::new(map.idx_to_x(1), map.price_to_y(101.0))
            ));
            let left = (map.idx_to_x(0) - map.bar_spacing * 0.45).max(map.rect.left());
            let width = map.idx_to_x(2) + map.bar_spacing * 0.5 - left;
            let centre = left + width * 0.5;
            // The incomplete first bar is excluded: row 101 now holds eight buys, no sell.
            assert!(
                evicted
                    .iter()
                    .any(|shape| matches!(&shape.shape, Shape::Rect(r)
                if (r.rect.left() - centre).abs() < 0.001
                && (r.rect.width() - width * 0.5).abs() < 0.001
                && r.rect.contains(Pos2::new(r.rect.center().x, map.price_to_y(101.0)))))
            );
            chart.set_symbol("OTHER");
            assert!(!chart.merge_trade_profile(usize::MAX));
        }
    }
}
