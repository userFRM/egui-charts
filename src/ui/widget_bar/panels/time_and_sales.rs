//! Host-fed time and sales: the latest trades with aggressor colours, size filters and sound
//! alert intents. The host feeds trades from its stream and plays any sound.

use crate::localization as l10n;
use std::{collections::VecDeque, ops::Range};

use egui::{Align, Align2, Label, Layout, RichText, Sense, Stroke, TextStyle, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::format_price;
use crate::data::combine_trades;
use crate::icons::icons;
use crate::model::{OrderSide, Tick, TimezoneId};
use crate::scales::StudyFormat;
use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, IconButton, PanelHeader};

/// Trades retained, the oldest making room for new ones.
const RETAINED: usize = 10_000;
const ROW_HEIGHT: f32 = 20.0;

/// A size filter of the time and sales panel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TradeSizeFilter {
    /// Smallest matching size, inclusive.
    pub min_size: f64,
    /// Report matching trades from [`TimeAndSalesPanel::push`] so the host plays a sound.
    pub sound: bool,
}

/// Formatting of the time and sales panel.
#[derive(Clone, Debug, Default)]
pub struct TimeAndSalesConfig {
    /// Decimals of prices, as `Instrument::price_decimals`. `None` chooses them from each price.
    pub price_decimals: Option<u8>,
    /// Decimals of sizes, as `Instrument::volume_decimals`.
    pub size_decimals: Option<u8>,
    /// Time zone of the time column, such as the chart's.
    pub timezone: TimezoneId,
}

/// The latest trades of one symbol, newest first, fed by the host.
///
/// Push each trade of the stream once with [`push`](Self::push), and [`clear`](Self::clear) when
/// the symbol changes. Prices and sizes take the aggressor colour: buys rising, sells falling, and
/// trades of unknown side neutral. The user adds, edits and removes size [`filters`](Self::filters)
/// in the panel: with any filter, only trades matching one are listed, and each filter can ask
/// for a sound.
pub struct TimeAndSalesPanel {
    /// Display and layout options for this component.
    pub config: TimeAndSalesConfig,
    /// A row is listed when its size reaches the `min_size` of any filter, or always when there
    /// are none.
    pub filters: Vec<TradeSizeFilter>,
    /// Combine consecutive fills with the same explicit aggressor id, or, without ids,
    /// the same exchange timestamp and known side. Off by default; raw fills stay retained.
    /// Size filters then apply to the combined row.
    pub combine_fills: bool,
    trades: VecDeque<Tick>,
    /// Retained size of the current consecutive run.
    run_size: f64,
    /// Highest retained size reached by the run, so eviction cannot repeat a sound.
    run_max_size: f64,
}

struct TapeRow {
    fills: Range<usize>,
    size: f64,
    side: Option<OrderSide>,
}

/// Explicit ids take precedence; a timestamp alone cannot identify an unknown aggressor.
fn same_run(a: &Tick, b: &Tick) -> bool {
    match (a.aggressor_order_id, b.aggressor_order_id) {
        (Some(a), Some(b)) => a == b,
        (None, None) => a.ts == b.ts && a.side.is_some() && a.side == b.side,
        _ => false,
    }
}

impl Default for TimeAndSalesPanel {
    fn default() -> Self {
        Self::new(TimeAndSalesConfig::default())
    }
}

impl TimeAndSalesPanel {
    /// A panel without trades or filters.
    pub fn new(config: TimeAndSalesConfig) -> Self {
        Self {
            config,
            filters: Vec::new(),
            combine_fills: false,
            trades: VecDeque::new(),
            run_size: 0.0,
            run_max_size: 0.0,
        }
    }

    /// Adds the next trade of the stream, keeping the latest 10,000. A trade without a finite
    /// price and a finite, non-negative size is ignored. Returns the index of the matching filter
    /// with sound that has the largest `min_size`, for the host to play that filter's sound.
    /// With [`combine_fills`](Self::combine_fills), reports only thresholds first reached by
    /// this fill in its consecutive run; later fills and toggling the option do not replay them.
    pub fn push(&mut self, trade: Tick) -> Option<usize> {
        if !trade.price.is_finite() || !trade.size.is_finite() || trade.size < 0.0 {
            return None;
        }
        let previous = self
            .trades
            .back()
            .filter(|last| same_run(last, &trade))
            .map(|_| self.run_max_size);
        if previous.is_none() {
            self.run_size = 0.0;
        }
        if self.trades.len() == RETAINED
            && let Some(evicted) = self.trades.pop_front()
            && previous.is_some()
            && same_run(&evicted, &trade)
        {
            // Re-sum instead of subtracting: a saturated sum has lost the evicted contribution.
            let start = self
                .trades
                .iter()
                .rposition(|fill| !same_run(fill, &trade))
                .map_or(0, |index| index + 1);
            self.run_size = self
                .trades
                .range(start..)
                .fold(0.0_f64, |size, fill| (size + fill.size).min(f64::MAX));
        }
        self.run_size = (self.run_size + trade.size).min(f64::MAX);
        self.run_max_size = previous.unwrap_or(0.0).max(self.run_size);
        let size = if self.combine_fills {
            self.run_size
        } else {
            trade.size
        };
        let sound = self
            .filters
            .iter()
            .enumerate()
            .filter(|(_, filter)| {
                filter.sound
                    && size >= filter.min_size
                    && (!self.combine_fills || previous.is_none_or(|size| size < filter.min_size))
            })
            .max_by(|(_, a), (_, b)| a.min_size.total_cmp(&b.min_size))
            .map(|(index, _)| index);
        self.trades.push_back(trade);
        sound
    }

    /// Removes every trade, such as when the symbol changes.
    pub fn clear(&mut self) {
        self.trades.clear();
        self.run_size = 0.0;
        self.run_max_size = 0.0;
    }

    /// Render the filters and the listed trades, newest first.
    pub fn show(&mut self, ui: &mut Ui) {
        PanelHeader::new("Time and sales").show(ui, |ui| {
            if IconButton::new(&icons::PLUS)
                .tooltip("Add size filter")
                .show(ui)
                .clicked()
            {
                let min_size = self.filters.iter().map(|f| f.min_size).fold(0.0, f64::max);
                self.filters.push(TradeSizeFilter {
                    min_size: if min_size > 0.0 {
                        min_size * 10.0
                    } else {
                        100.0
                    },
                    sound: false,
                });
            }
            ui.checkbox(
                &mut self.combine_fills,
                l10n::text(ui.ctx(), "Combine fills"),
            );
        });
        let mut removed = None;
        egui::ScrollArea::vertical()
            .id_salt("size_filters")
            .max_height(ui.available_height() * 0.4)
            .show(ui, |ui| {
                for (index, filter) in self.filters.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let size_label = ui.label(l10n::text(ui.ctx(), "Size ≥"));
                            ui.add(
                                egui::DragValue::new(&mut filter.min_size)
                                    .range(0.0..=f64::MAX)
                                    .speed(1.0),
                            )
                            .labelled_by(size_label.id);
                            ui.checkbox(&mut filter.sound, l10n::text(ui.ctx(), "Sound"));
                            if IconButton::new(&icons::TRASH)
                                .tooltip("Remove filter")
                                .show(ui)
                                .clicked()
                            {
                                removed = Some(index);
                            }
                        });
                    });
                }
            });
        if let Some(index) = removed {
            self.filters.remove(index);
        }

        let theme = ui.active_theme();
        let font = TextStyle::Monospace.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        let column_count = if self.combine_fills { 4 } else { 3 };
        let columns = |rect: egui::Rect| {
            let right = rect.right() - 8.0;
            let width = (rect.width() - 16.0).max(0.0);
            let left = rect.left() + 8.0;
            let edges = if self.combine_fills {
                [
                    left,
                    left + width * 0.3,
                    left + width * 0.57,
                    left + width * 0.83,
                    right,
                ]
            } else {
                [left, left + width * 0.34, right - width * 0.3, right, right]
            };
            std::array::from_fn::<_, 4, _>(|i| {
                egui::Rect::from_min_max(
                    pos2(edges[i], rect.top()),
                    pos2((edges[i + 1] - 4.0).max(edges[i]), rect.bottom()),
                )
            })
        };
        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let cells = columns(header);
        for (index, label) in ["Time", "Price", "Size", "Fills"]
            .into_iter()
            .take(column_count)
            .enumerate()
        {
            ui.painter().text(
                egui::pos2(
                    if index == 0 {
                        cells[index].left()
                    } else {
                        cells[index].right()
                    },
                    header.center().y,
                ),
                if index == 0 {
                    Align2::LEFT_CENTER
                } else {
                    Align2::RIGHT_CENTER
                },
                l10n::text(ui.ctx(), label),
                small.clone(),
                theme.ui.text_secondary,
            );
        }
        ui.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0, theme.ui.border),
        );

        let mut listed: Vec<TapeRow> = Vec::new();
        for (index, trade) in self.trades.iter().enumerate() {
            if self.combine_fills
                && let Some(row) = listed.last_mut()
                && same_run(&self.trades[index - 1], trade)
            {
                row.fills.end = index + 1;
                row.size = (row.size + trade.size).min(f64::MAX);
                if row.side != trade.side {
                    row.side = None;
                }
            } else {
                listed.push(TapeRow {
                    fills: index..index + 1,
                    size: trade.size,
                    side: trade.side,
                });
            }
        }
        let smallest = self.filters.iter().map(|f| f.min_size).reduce(f64::min);
        listed.retain(|row| smallest.is_none_or(|min| row.size >= min));
        listed.reverse();
        if listed.is_empty() {
            EmptyState::new("No trades yet").fill_height().show(ui);
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, listed.len(), |ui, rows| {
                for row in &listed[rows] {
                    let first = &self.trades[row.fills.start];
                    let last = &self.trades[row.fills.end - 1];
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::hover(),
                    );
                    let cells = columns(rect);
                    let color = match row.side {
                        Some(OrderSide::Buy) => theme.chart.candle_up,
                        Some(OrderSide::Sell) => theme.chart.candle_down,
                        None => theme.ui.text,
                    };
                    let time = first.ts.with_timezone(&self.config.timezone);
                    for (column, text, color) in [
                        (
                            0,
                            time.format("%H:%M:%S").to_string(),
                            theme.ui.text_secondary,
                        ),
                        (
                            1,
                            format_price(last.price, self.config.price_decimals),
                            color,
                        ),
                        (
                            2,
                            StudyFormat::Volume
                                .format(row.size, self.config.size_decimals.map(usize::from)),
                            color,
                        ),
                        (3, row.fills.len().to_string(), theme.ui.text_secondary),
                    ]
                    .into_iter()
                    .take(column_count)
                    {
                        ui.new_child(egui::UiBuilder::new().max_rect(cells[column]).layout(
                            if column == 0 {
                                Layout::left_to_right(Align::Center)
                            } else {
                                Layout::right_to_left(Align::Center)
                            },
                        ))
                        .add(
                            Label::new(
                                RichText::new(l10n::text(ui.ctx(), text))
                                    .font(font.clone())
                                    .color(color),
                            )
                            .truncate()
                            .selectable(false),
                        );
                    }
                    if self.combine_fills || first.aggressor_order_id.is_some() {
                        response.on_hover_ui(|ui| self.show_breakdown(ui, row.fills.clone()));
                    }
                }
            });
    }

    /// The resting orders consumed by this row, or by its explicit order in individual mode.
    fn show_breakdown(&self, ui: &mut Ui, fills: Range<usize>) {
        let id = self.trades[fills.start].aggressor_order_id;
        let orders = if self.combine_fills {
            combine_trades(self.trades.range(fills))
        } else {
            combine_trades(
                self.trades
                    .iter()
                    .filter(|trade| trade.aggressor_order_id == id),
            )
        };
        let size =
            |size| StudyFormat::Volume.format(size, self.config.size_decimals.map(usize::from));
        let total = orders
            .iter()
            .fold(0.0_f64, |size, order| (size + order.size).min(f64::MAX));
        let count: usize = orders.iter().map(|order| order.fills).sum();
        ui.label(if let Some(id) = id {
            l10n::format_text(
                ui.ctx(),
                "Order {0}: {1} in {2} fills",
                &[
                    ("0", id.to_string()),
                    ("1", size(total)),
                    ("2", count.to_string()),
                ],
            )
        } else {
            l10n::format_text(
                ui.ctx(),
                "{0} in {1} fills",
                &[("0", size(total)), ("1", count.to_string())],
            )
        });
        for consumed in orders.iter().flat_map(|order| &order.consumed) {
            let name = consumed.resting_order_id.map_or_else(
                || l10n::text(ui.ctx(), "Unidentified"),
                |id| l10n::format_text(ui.ctx(), "Order {0}", &[("0", id.to_string())]),
            );
            ui.label(l10n::format_text(
                ui.ctx(),
                "{0}: {1} at {2}",
                &[
                    ("0", name),
                    ("1", size(consumed.size)),
                    (
                        "2",
                        format_price(consumed.price, self.config.price_decimals),
                    ),
                ],
            ));
        }
        if orders
            .iter()
            .flat_map(|order| &order.consumed)
            .any(|order| order.resting_order_id.is_none())
        {
            ui.weak(l10n::text(
                ui.ctx(),
                "Incomplete: some fills do not name their resting order",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn push_reports_the_largest_matching_sound_filter_and_drops_invalid_trades() {
        let mut panel = TimeAndSalesPanel {
            filters: vec![
                TradeSizeFilter {
                    min_size: 100.0,
                    sound: true,
                },
                TradeSizeFilter {
                    min_size: 1000.0,
                    sound: true,
                },
                TradeSizeFilter {
                    min_size: 5000.0,
                    sound: false,
                },
                TradeSizeFilter {
                    min_size: 0.0,
                    sound: false,
                },
            ],
            ..TimeAndSalesPanel::default()
        };
        let trade = |price, size| Tick::new(chrono::Utc::now(), price, size, None);
        for (price, size, expected) in [
            (10.0, 50.0, None),
            (10.0, 100.0, Some(0)),
            (10.0, 999.0, Some(0)),
            (10.0, 6000.0, Some(1)),
            (f64::NAN, 6000.0, None),
            (10.0, -1.0, None),
        ] {
            assert_eq!(panel.push(trade(price, size)), expected, "{price} {size}");
        }
        assert_eq!(panel.trades.len(), 4);
    }

    #[test]
    fn combine_fills_groups_only_consecutive_matches_and_restores_raw_fills() {
        use chrono::{DateTime, TimeDelta, Utc};
        let start = "2024-03-15T10:00:00.100Z".parse::<DateTime<Utc>>().unwrap();
        let buy = Some(OrderSide::Buy);
        let sell = Some(OrderSide::Sell);
        // (millisecond offset, size, side, aggressor id), expected rows in arrival order.
        #[rustfmt::skip]
        let cases = [
            ("explicit ids across prices and times",
                vec![(0, 2.0, buy, Some(7)), (1000, 3.0, buy, Some(7)), (2000, 4.0, buy, Some(8))],
                vec![(0, 1, 5.0, 2), (2, 2, 4.0, 1)]),
            ("different explicit ids at the same instant stay separate",
                vec![(0, 2.0, buy, Some(7)), (0, 3.0, buy, Some(8))],
                vec![(0, 0, 2.0, 1), (1, 1, 3.0, 1)]),
            ("timestamp and side without ids",
                vec![(0, 2.0, buy, None), (0, 3.0, buy, None), (0, 4.0, sell, None), (1, 1.0, buy, None)],
                vec![(0, 1, 5.0, 2), (2, 2, 4.0, 1), (3, 3, 1.0, 1)]),
            ("unknown sides need explicit ids",
                vec![(0, 2.0, None, None), (0, 3.0, None, None), (0, 4.0, None, Some(7)), (1, 1.0, None, Some(7))],
                vec![(0, 0, 2.0, 1), (1, 1, 3.0, 1), (2, 3, 5.0, 2)]),
            ("ids do not join unidentified fills or skip intervening rows",
                vec![(0, 2.0, buy, Some(7)), (0, 3.0, buy, None), (0, 4.0, buy, Some(7))],
                vec![(0, 0, 2.0, 1), (1, 1, 3.0, 1), (2, 2, 4.0, 1)]),
        ];
        for (name, inputs, expected) in cases {
            let mut panel = TimeAndSalesPanel::default();
            panel.config.price_decimals = Some(2);
            panel.config.size_decimals = Some(0);
            for (index, &(ms, size, side, id)) in inputs.iter().enumerate() {
                let mut trade = Tick::new(
                    start + TimeDelta::milliseconds(ms),
                    100.0 + index as f64 * 0.25,
                    size,
                    side,
                );
                trade.aggressor_order_id = id;
                trade.resting_order_id = Some(10 + index as u64);
                panel.push(trade);
            }
            let mut h = Harness::builder()
                .with_size(vec2(600.0, 500.0))
                .with_context_setup(crate::icons::install_loader)
                .build_ui_state(|ui, panel: &mut TimeAndSalesPanel| panel.show(ui), panel);
            h.run();
            let row_times = |h: &Harness<'_, TimeAndSalesPanel>| {
                ["10:00:00", "10:00:01", "10:00:02"]
                    .iter()
                    .map(|time| h.query_all_by_label(time).count())
                    .sum::<usize>()
            };
            assert_eq!(
                row_times(&h),
                inputs.len(),
                "{name}: default is individual fills"
            );
            h.get_by_label("Combine fills").click();
            h.run();
            assert_eq!(row_times(&h), expected.len(), "{name}: combined row count");
            for &(first, last, size, fills) in &expected {
                let time = (start + TimeDelta::milliseconds(inputs[first].0))
                    .format("%H:%M:%S")
                    .to_string();
                assert!(
                    h.query_all_by_label(&time).count() > 0,
                    "{name}: first time"
                );
                let price = format!("{:.2}", 100.0 + last as f64 * 0.25);
                h.get_by_label(&price);
                assert!(
                    h.query_all_by_label(&size.to_string()).next().is_some(),
                    "{name}: total size"
                );
                assert!(
                    h.query_all_by_label(&fills.to_string()).next().is_some(),
                    "{name}: fill count"
                );
            }
            if let Some(&(first, last, size, count)) = expected.iter().find(|row| row.3 > 1) {
                let price = format!("{:.2}", 100.0 + last as f64 * 0.25);
                let pos = h.get_by_label(&price).rect().center();
                h.hover_at(pos);
                h.run_steps(90);
                let summary = inputs[first].3.map_or_else(
                    || format!("{size} in {count} fills"),
                    |id| format!("Order {id}: {size} in {count} fills"),
                );
                h.get_by_label(&summary);
                for (index, input) in inputs.iter().enumerate().take(last + 1).skip(first) {
                    h.get_by_label(&format!(
                        "Order {}: {} at {:.2}",
                        10 + index,
                        input.1,
                        100.0 + index as f64 * 0.25
                    ));
                }
                h.remove_cursor();
                h.run();
            }
            h.get_by_label("Combine fills").click();
            h.run();
            assert_eq!(
                row_times(&h),
                inputs.len(),
                "{name}: toggle restores retained fills"
            );
            assert_eq!(
                h.state().trades.len(),
                inputs.len(),
                "{name}: raw retention"
            );
        }
    }

    #[test]
    fn combined_size_filters_sound_once_when_the_run_crosses_each_threshold() {
        let ts = "2024-03-15T10:00:00.100Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap();
        for order in [None, Some(7)] {
            for combine in [true, false] {
                let panel = TimeAndSalesPanel::default();
                let mut h = Harness::builder()
                    .with_size(vec2(600.0, 500.0))
                    .with_context_setup(crate::icons::install_loader)
                    .build_ui_state(|ui, panel: &mut TimeAndSalesPanel| panel.show(ui), panel);
                h.run();
                if combine {
                    h.get_by_label("Combine fills").click();
                    h.run();
                }
                h.state_mut().filters = vec![
                    TradeSizeFilter {
                        min_size: 5.0,
                        sound: true,
                    },
                    TradeSizeFilter {
                        min_size: 10.0,
                        sound: true,
                    },
                ];
                h.run();
                for (size, expected) in [
                    (2.0, None),
                    (3.0, Some(0)),
                    (1.0, None),
                    (6.0, Some(1)),
                    (1.0, None),
                ] {
                    let mut trade = Tick::new(ts, 100.0, size, Some(OrderSide::Buy));
                    trade.aggressor_order_id = order;
                    let expected = if combine {
                        expected
                    } else if size >= 5.0 {
                        Some(0)
                    } else {
                        None
                    };
                    assert_eq!(
                        h.state_mut().push(trade),
                        expected,
                        "{order:?}, combine={combine}, size={size}"
                    );
                    h.run();
                    if size == 3.0 {
                        assert_eq!(
                            h.query_all_by_label("10:00:00").count(),
                            usize::from(combine)
                        );
                    }
                }
                h.state_mut().clear();
                assert_eq!(
                    h.state_mut()
                        .push(Tick::new(ts, 100.0, 5.0, Some(OrderSide::Buy))),
                    Some(0),
                    "clear resets the run"
                );
                if combine && order.is_none() {
                    for _ in 0..RETAINED {
                        assert_eq!(
                            h.state_mut()
                                .push(Tick::new(ts, 100.0, 0.0, Some(OrderSide::Buy))),
                            None
                        );
                    }
                    assert_eq!(h.state().trades.len(), RETAINED);
                    assert_eq!(
                        h.state_mut()
                            .push(Tick::new(ts, 100.0, 5.0, Some(OrderSide::Buy))),
                        None,
                        "eviction does not repeat the threshold at 5 or sound 10 before the retained row reaches it"
                    );
                    assert_eq!(
                        h.state_mut()
                            .push(Tick::new(ts, 100.0, 5.0, Some(OrderSide::Buy))),
                        Some(1)
                    );
                }
            }
        }
    }
}
