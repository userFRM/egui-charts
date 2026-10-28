//! Host-fed time and sales: the latest trades with aggressor colours, size filters and sound
//! alert intents. The host feeds trades from its stream and plays any sound.

use crate::localization as l10n;
use std::collections::VecDeque;

use egui::{Align, Align2, Label, Layout, RichText, Sense, Stroke, TextStyle, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::object_tree::format_price;
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
    /// A trade is listed when its size reaches the `min_size` of any filter, or always when there
    /// are none.
    pub filters: Vec<TradeSizeFilter>,
    trades: VecDeque<Tick>,
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
            trades: VecDeque::new(),
        }
    }

    /// Adds the next trade of the stream, keeping the latest 10,000. A trade without a finite
    /// price and a finite, non-negative size is ignored. Returns the index of the matching filter
    /// with sound that has the largest `min_size`, for the host to play that filter's sound.
    pub fn push(&mut self, trade: Tick) -> Option<usize> {
        if !trade.price.is_finite() || !trade.size.is_finite() || trade.size < 0.0 {
            return None;
        }
        let sound = self
            .filters
            .iter()
            .enumerate()
            .filter(|(_, filter)| filter.sound && trade.size >= filter.min_size)
            .max_by(|(_, a), (_, b)| a.min_size.total_cmp(&b.min_size))
            .map(|(index, _)| index);
        if self.trades.len() == RETAINED {
            self.trades.pop_front();
        }
        self.trades.push_back(trade);
        sound
    }

    /// Removes every trade, such as when the symbol changes.
    pub fn clear(&mut self) {
        self.trades.clear();
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
        });
        let mut removed = None;
        egui::ScrollArea::vertical()
            .id_salt("size_filters")
            .max_height(ui.available_height() * 0.4)
            .show(ui, |ui| {
                for (index, filter) in self.filters.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(l10n::text(ui.ctx(), "Size ≥"));
                            ui.add(
                                egui::DragValue::new(&mut filter.min_size)
                                    .range(0.0..=f64::MAX)
                                    .speed(1.0),
                            );
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
        let columns = |rect: egui::Rect| {
            let right = rect.right() - 8.0;
            let width = (rect.width() - 16.0).max(0.0);
            let edges = [
                rect.left() + 8.0,
                rect.left() + 8.0 + width * 0.34,
                right - width * 0.3,
                right,
            ];
            std::array::from_fn::<_, 3, _>(|i| {
                egui::Rect::from_min_max(
                    pos2(edges[i], rect.top()),
                    pos2((edges[i + 1] - 4.0).max(edges[i]), rect.bottom()),
                )
            })
        };
        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let cells = columns(header);
        for (index, label) in ["Time", "Price", "Size"].into_iter().enumerate() {
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

        let smallest = self.filters.iter().map(|f| f.min_size).reduce(f64::min);
        let listed: Vec<&Tick> = self
            .trades
            .iter()
            .rev()
            .filter(|trade| smallest.is_none_or(|min| trade.size >= min))
            .collect();
        if listed.is_empty() {
            EmptyState::new("No trades yet").fill_height().show(ui);
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, listed.len(), |ui, rows| {
                for trade in &listed[rows] {
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::hover(),
                    );
                    let cells = columns(rect);
                    let color = match trade.side {
                        Some(OrderSide::Buy) => theme.chart.candle_up,
                        Some(OrderSide::Sell) => theme.chart.candle_down,
                        None => theme.ui.text,
                    };
                    let time = trade.ts.with_timezone(&self.config.timezone);
                    for (column, text, color) in [
                        (
                            0,
                            time.format("%H:%M:%S").to_string(),
                            theme.ui.text_secondary,
                        ),
                        (
                            1,
                            format_price(trade.price, self.config.price_decimals),
                            color,
                        ),
                        (
                            2,
                            StudyFormat::Volume
                                .format(trade.size, self.config.size_decimals.map(usize::from)),
                            color,
                        ),
                    ] {
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
                    if let Some(id) = trade.aggressor_order_id {
                        response.on_hover_ui(|ui| self.show_breakdown(ui, id));
                    }
                }
            });
    }

    /// The resting orders that the listed fills of aggressor order `id` consumed.
    fn show_breakdown(&self, ui: &mut Ui, id: u64) {
        let Some(order) = combine_trades(&self.trades)
            .into_iter()
            .find(|order| order.aggressor_order_id == Some(id))
        else {
            return;
        };
        let size =
            |size| StudyFormat::Volume.format(size, self.config.size_decimals.map(usize::from));
        ui.label(l10n::format_text(
            ui.ctx(),
            "Order {0}: {1} in {2} fills",
            &[
                ("0", id.to_string()),
                ("1", size(order.size)),
                ("2", order.fills.to_string()),
            ],
        ));
        for consumed in &order.consumed {
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
        if !order.complete {
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
}
