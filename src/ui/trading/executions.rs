//! Account and host fills on the chart, through `Trading::show` and [`render_executions`].
//! Each side and bar shares a stack of up to three triangles, below buys and above sells.

use std::sync::Arc;

use crate::localization as l10n;
use egui::{Color32, FontId, Galley, Rect, Sense, Shape, Stroke, Ui, pos2};

use crate::model::{Execution, OrderSide};
use crate::theme::ThemeExt;
use crate::widget::{Chart, settings::TradingSettings};

/// The most triangles one mark draws; a "+N" line counts the rest.
const MAX_STACK: usize = 3;
/// The most rows crowded labels stack in; a label with no room left is left out.
const MAX_LABEL_ROWS: usize = 3;
/// The most fills a mark's hover lists before "and N more".
const MAX_HOVER_FILLS: usize = 20;
/// Half the width of a triangle's base.
const HALF_WIDTH: f32 = 7.0;
/// The height of a triangle, from its tip to its base.
const HEIGHT: f32 = 17.0;
/// The space between stacked triangles.
const STEP: f32 = 2.0;
/// The space between a bar and its marks, and between a mark and its text.
const GAP: f32 = 4.0;

use super::account_panel::quantity_decimals;
use super::format::{Numbers, TradingFormatter};
use super::widgets::{side_color, side_name};

/// A borrowed fill to draw with [`render_executions`], optionally with host colour and text.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ExecutionMark<'a> {
    /// The fill to draw.
    pub execution: &'a Execution,
    /// Triangle colour, or the chart's side colour. Only the first three fills in a stack
    /// draw triangles; the others appear in the count and hover.
    pub color: Option<Color32>,
    /// This fill's hover line, shown verbatim instead of the built-in side, quantity, price
    /// and time. Newlines are preserved.
    pub hover: Option<&'a str>,
}

impl<'a> ExecutionMark<'a> {
    /// Uses the side colour and the built-in hover line for `execution`.
    pub fn new(execution: &'a Execution) -> Self {
        Self {
            execution,
            color: None,
            hover: None,
        }
    }

    /// Replaces this fill's triangle colour.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Replaces this fill's hover line with untranslated host text.
    pub fn with_hover(mut self, hover: &'a str) -> Self {
        self.hover = Some(hover);
        self
    }
}

/// Draws host fills on the price pane, using the same marks as
/// [`Trading::show`](super::Trading::show). Fills on the same side and bar share up to three
/// stacked triangles and a count of the rest.
/// Buys point up below the bar's low; sells point down above its high. Hover lists the fills,
/// and [`TradingSettings::show_execution_labels`] adds their side and total quantity.
/// [`TradingSettings::show_execution_prices`] adds their quantity-weighted average fill price.
/// [`TradingSettings::show_executions`] hides everything when off. Fills with non-finite
/// prices or quantities, or quantities at or below zero, are omitted.
///
/// All supplied fills are treated as belonging to the chart's symbol. Filter a mixed list
/// with [`Execution::symbol`] first. Call after [`Chart::show`], once per chart with the full
/// list: separate calls place overlapping marks independently. The chart's theme applies.
/// Prices and quantities use `formatter` for the chart symbol when it returns text, otherwise
/// prices use the instrument's decimals (or two), and quantities their own decimals up to eight.
///
/// ```
/// use egui_charts::model::{Execution, OrderId, OrderSide};
/// use egui_charts::ui::trading::{ExecutionMark, render_executions};
/// use egui_charts::widget::{Chart, settings::TradingSettings};
///
/// fn draw(ui: &mut egui::Ui, chart: &mut Chart, imported: &[(Execution, String)]) {
///     chart.show(ui);
///     let marks: Vec<_> = imported.iter()
///         .filter(|(fill, _)| fill.symbol.as_deref() == Some(chart.symbol()))
///         .map(|(fill, note)| ExecutionMark::new(fill)
///             .with_color(egui::Color32::GOLD).with_hover(note))
///         .collect();
///     render_executions(ui, chart, &marks, &TradingSettings::default(), None);
/// }
///
/// let mut fill = Execution::new(OrderId::new("import-1"), chrono::Utc::now(),
///     OrderSide::Buy, 101.25, 2.0);
/// fill.symbol = Some("AAPL".into());
/// let imported = vec![(fill, "Imported statement fill".to_owned())];
/// # let _ = (draw, imported);
/// ```
pub fn render_executions(
    ui: &mut Ui,
    chart: &Chart,
    marks: &[ExecutionMark<'_>],
    display: &TradingSettings,
    formatter: Option<&dyn TradingFormatter>,
) {
    if !display.show_executions {
        return;
    }
    let _theme = chart
        .theme_override
        .as_ref()
        .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
    let decimals = chart
        .instrument()
        .and_then(crate::scales::instrument_price_precision)
        .map_or(2, usize::from);
    let numbers = Numbers {
        symbol: chart.symbol(),
        decimals,
        formatter,
    };
    show(ui, chart, marks, display, numbers);
}

/// Draws `marks` on the price pane of `chart`. The fills of one side in one bar form one mark: up
/// to [`MAX_STACK`] triangles stacked outward, pointing up in the buy colour just under the bar's
/// low, or down in the sell colour just over its high, and beyond them a "+N" line for the fills
/// not drawn. With labels enabled, a line beyond the stack gives the side and total quantity. Text
/// that would overlap newer text moves outward, up to [`MAX_LABEL_ROWS`] rows, or is left out.
/// Hovering a mark lists its fills with their side, quantity, price and time, up to
/// [`MAX_HOVER_FILLS`].
pub(super) fn show(
    ui: &mut Ui,
    chart: &Chart,
    marks: &[ExecutionMark<'_>],
    display: &TradingSettings,
    numbers: Numbers<'_>,
) {
    // A bar spans from its time to the next bar's, so a fill belongs to the bar at or before it.
    let bars = chart.state.shown();
    let mut placed: Vec<(f64, &ExecutionMark<'_>)> = marks
        .iter()
        .filter_map(|mark| {
            let fill = mark.execution;
            if !fill.price.is_finite() || !fill.quantity.is_finite() || fill.quantity <= 0.0 {
                return None;
            }
            let index = if let Some(cutoff) = chart.replay_time() {
                if fill.time >= cutoff {
                    return None;
                }
                bars.partition_point(|bar| bar.time <= fill.time)
                    .checked_sub(1)
                    .map(|index| index as f64)
            } else {
                chart.time_to_index(fill.time).map(f64::floor)
            };
            Some((index?, mark))
        })
        .collect();
    // Stable, so that a mark keeps its fills in the list's order.
    let is_sell = |mark: &ExecutionMark<'_>| matches!(mark.execution.side, OrderSide::Sell);
    placed.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| is_sell(a.1).cmp(&is_sell(b.1)))
    });

    let mapping = chart.get_chart_mapping();
    let theme = ui.ctx().active_theme();
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let (mut text_rects, mut texts): (Vec<Rect>, Vec<(Rect, Arc<Galley>)>) = Default::default();
    // Newest first, so that where labels crowd each other the newer ones keep their text.
    for run in placed
        .chunk_by(|a, b| a.0 == b.0 && a.1.execution.side == b.1.execution.side)
        .rev()
    {
        let (index, side) = (run[0].0, run[0].1.execution.side);
        let Some(x) = chart.index_to_x(index) else {
            continue;
        };
        if x < mapping.rect.left() - HALF_WIDTH || x > mapping.rect.right() + HALF_WIDTH {
            continue;
        }
        let quantity: f64 = run.iter().map(|(_, mark)| mark.execution.quantity).sum();
        let price = run
            .iter()
            .map(|(_, mark)| mark.execution.price * mark.execution.quantity)
            .sum::<f64>()
            / quantity;
        // A fill timed after the last bar waits at its price for its bar.
        let (low, high) = usize::try_from(index as i64)
            .ok()
            .and_then(|index| bars.get(index))
            .map_or((price, price), |bar| (bar.low, bar.high));
        let (low_y, high_y) = (mapping.price_to_y(low), mapping.price_to_y(high));
        let color = side_color(&theme, side);
        let (down, edge) = match side {
            OrderSide::Buy => (1.0, low_y.max(high_y) + GAP),
            OrderSide::Sell => (-1.0, low_y.min(high_y) - GAP),
        };
        if !edge.is_finite() {
            continue;
        }
        let at = |dx: f32, dy: f32| pos2(x + dx, edge + down * dy);
        let drawn = run.len().min(MAX_STACK);
        for (triangle, (_, mark)) in run.iter().take(MAX_STACK).enumerate() {
            let dy = triangle as f32 * (HEIGHT + STEP);
            painter.add(Shape::convex_polygon(
                vec![
                    at(0.0, dy),
                    at(HALF_WIDTH, dy + HEIGHT),
                    at(-HALF_WIDTH, dy + HEIGHT),
                ],
                mark.color.unwrap_or(color),
                Stroke::NONE,
            ));
        }
        let length = drawn as f32 * HEIGHT + (drawn - 1) as f32 * STEP;
        let mut rect = Rect::from_two_pos(at(-HALF_WIDTH, 0.0), at(HALF_WIDTH, length));
        let more = (run.len() > MAX_STACK).then(|| format!("+{}", run.len() - MAX_STACK));
        let label = display.show_execution_labels.then(|| {
            let quantity = numbers.quantity(quantity, quantity_decimals(quantity));
            let price =
                (display.show_execution_prices && price.is_finite()).then(|| numbers.price(price));
            let template = match (side, price.is_some()) {
                (OrderSide::Buy, false) => "Buy {quantity}",
                (OrderSide::Sell, false) => "Sell {quantity}",
                (OrderSide::Buy, true) => "Buy {quantity} @ {price}",
                (OrderSide::Sell, true) => "Sell {quantity} @ {price}",
            };
            l10n::format_text(
                ui.ctx(),
                template,
                &[
                    ("quantity", quantity.as_str()),
                    ("price", price.as_deref().unwrap_or("")),
                ],
            )
        });
        let lines: Vec<_> = more
            .into_iter()
            .chain(label)
            .map(|line| {
                painter.layout_no_wrap(
                    line,
                    FontId::proportional(ui.active_theme().text.label),
                    chart.config().text_color,
                )
            })
            .collect();
        if !lines.is_empty() {
            let height: f32 = lines.iter().map(|line| line.size().y).sum::<f32>()
                + (lines.len() - 1) as f32 * GAP / 2.0;
            let width = lines.iter().map(|line| line.size().x).fold(0.0, f32::max);
            let size = egui::vec2(width, height);
            let top = if down > 0.0 {
                rect.bottom() + GAP
            } else {
                rect.top() - GAP - size.y
            };
            let block = (0..MAX_LABEL_ROWS)
                .map(|row| {
                    Rect::from_min_size(pos2(x - size.x / 2.0, top), size)
                        .translate(egui::vec2(0.0, down * row as f32 * (size.y + GAP)))
                })
                .find(|block| {
                    // The nearest labels, the likeliest to overlap, were placed last.
                    !text_rects
                        .iter()
                        .rev()
                        .any(|other| other.expand(GAP / 2.0).intersects(*block))
                });
            // Without room among the newer labels the mark keeps its triangles and hover only.
            if let Some(block) = block {
                text_rects.push(block);
                let mut y = if down > 0.0 {
                    block.top()
                } else {
                    block.bottom()
                };
                for line in lines {
                    let size = line.size();
                    let top = if down > 0.0 { y } else { y - size.y };
                    texts.push((Rect::from_min_size(pos2(x - size.x / 2.0, top), size), line));
                    y += down * (size.y + GAP / 2.0);
                }
                rect = rect.union(block);
            }
        }
        let hover = rect.expand(2.0).intersect(mapping.rect);
        if hover.is_positive() {
            ui.interact(
                hover,
                ui.scope_id()
                    .with(("execution", side_name(side), index.to_bits())),
                Sense::hover(),
            )
            // Written only while it shows: a mark can hold every fill of the list.
            .on_hover_ui(|ui| {
                let time = chart.time_formatter_in(chart.display_timezone());
                let mut lines: Vec<_> = run
                    .iter()
                    .take(MAX_HOVER_FILLS)
                    .map(|(_, mark)| {
                        if let Some(hover) = mark.hover {
                            return hover.to_owned();
                        }
                        let fill = mark.execution;
                        l10n::format_text(
                            ui.ctx(),
                            "{side} {quantity} at {price}, {time}",
                            &[
                                ("side", l10n::text(ui.ctx(), side_name(fill.side)).as_str()),
                                (
                                    "quantity",
                                    numbers
                                        .quantity(fill.quantity, quantity_decimals(fill.quantity))
                                        .as_str(),
                                ),
                                ("price", numbers.price(fill.price).as_str()),
                                // With seconds, also on charts of daily bars.
                                ("time", time.format_crosshair(fill.time, 1).as_str()),
                            ],
                        )
                    })
                    .collect();
                let more = run.len().saturating_sub(MAX_HOVER_FILLS);
                if more > 0 {
                    lines.push(l10n::format_text(
                        ui.ctx(),
                        "and {0} more",
                        &[("0", more.to_string())],
                    ));
                }
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.label(lines.join("\n"));
            });
        }
    }
    // Over every triangle, so that a neighbouring stack never hides a label.
    for (rect, line) in texts {
        // The chart's background keeps the text readable over the bars beside its own.
        painter.rect_filled(rect.expand(1.0), 2.0, theme.chart.bg);
        painter.galley(rect.min, line, chart.config().text_color);
    }
}

#[cfg(test)]
mod tests {
    use super::super::TradingFormatter;
    use super::*;
    use crate::model::OrderId;
    use crate::widget::test_frames;

    /// Labels and hover use the same host quantity text, with grouped and rounded fallback;
    /// hover prices retain the symbol's formatter.
    #[test]
    fn execution_hover_uses_symbol_price_formatter() {
        struct Host;
        impl TradingFormatter for Host {
            fn price(&self, symbol: &str, price: f64) -> Option<String> {
                (symbol == "BOND").then(|| format!("{price:.3} u"))
            }
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "BOND").then(|| format!("{quantity} lots"))
            }
        }
        type Row<'a> = (
            &'a str,
            Option<&'a dyn TradingFormatter>,
            f64,
            &'a str,
            &'a str,
        );
        let rows: [Row<'_>; 4] = [
            (
                "BOND",
                Some(&Host),
                2.0,
                "Buy 2 lots",
                "Buy 2 lots at 1001.125 u, ",
            ),
            ("BOND", None, 2.0, "Buy 2", "Buy 2 at 1,001.12, "),
            ("OTHER", Some(&Host), 2.0, "Buy 2", "Buy 2 at 1,001.12, "),
            ("OTHER", None, 0.1 + 0.2, "Buy 0.3", "Buy 0.3 at 1,001.12, "),
        ];
        for (symbol, formatter, quantity, label, hover) in rows {
            let ctx = egui::Context::default();
            let mut chart = test_frames::chart(&[990.0], &[1010.0]);
            let fill = Execution::new(
                OrderId("fill".into()),
                chart.bars(crate::widget::SeriesId::MAIN)[95].time,
                OrderSide::Buy,
                1001.125,
                quantity,
            );
            let (mut hovered, mut labelled) = (false, false);
            for frame in 0..4 {
                let mapping = chart.get_chart_mapping();
                let events = chart
                    .index_to_x(95.0)
                    .filter(|_| frame == 1)
                    .map(|x| egui::Event::PointerMoved(pos2(x, mapping.price_to_y(990.0) + 10.0)));
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        time: Some(frame as f64),
                        events: events.into_iter().collect(),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        show(
                            ui,
                            &chart,
                            &[ExecutionMark::new(&fill)],
                            &TradingSettings::default(),
                            Numbers {
                                symbol,
                                decimals: 2,
                                formatter,
                            },
                        );
                    },
                );
                hovered |= output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, Shape::Text(text)
                        if text.galley.text().starts_with(hover))
                });
                labelled |= output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, Shape::Text(text) if text.galley.text() == label)
                });
                output.drop_without_applying_deltas();
            }
            assert!(labelled, "execution label: {symbol} {label}");
            assert!(hovered, "execution hover: {symbol} {hover}");
        }
    }

    /// Hosts can draw fills without a broker, keep their colour and literal hover text, and
    /// hide them through the shared display switch. Invalid host fills never reach a mark.
    #[test]
    fn host_fills_draw_with_their_own_colour_and_hover() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (visible, labels, prices, label) in [
                (true, true, false, Some("Buy 2")),
                (false, true, false, None),
                (true, true, true, Some("Buy 2 @ 101.25")),
                (true, false, true, None),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = test_frames::chart(&[90.0], &[110.0]);
                chart.set_theme_override(Some(theme.clone()));
                let time = chart.bars(crate::widget::SeriesId::MAIN)[95].time
                    + chrono::Duration::hours(12);
                let a = Execution::new(OrderId::new("a"), time, OrderSide::Buy, 101.0, 1.0);
                let b = Execution::new(OrderId::new("b"), time, OrderSide::Buy, 101.5, 1.0);
                let invalid: Vec<_> = [
                    (f64::NAN, 1.0),
                    (f64::INFINITY, 1.0),
                    (101.0, f64::NAN),
                    (101.0, f64::INFINITY),
                    (101.0, 0.0),
                    (101.0, -1.0),
                ]
                .map(|(price, quantity)| {
                    Execution::new(
                        OrderId::new("invalid"),
                        time,
                        OrderSide::Buy,
                        price,
                        quantity,
                    )
                })
                .into();
                let gold = Color32::from_rgb(0xe0, 0xa0, 0x20);
                let mut marks = vec![
                    ExecutionMark::new(&a)
                        .with_color(gold)
                        .with_hover("Imported"),
                    ExecutionMark::new(&b),
                ];
                marks.extend(invalid.iter().map(ExecutionMark::new));
                let display = TradingSettings {
                    show_executions: visible,
                    show_execution_labels: labels,
                    show_execution_prices: prices,
                    ..TradingSettings::default()
                };
                let (mut triangles, mut hovered, mut labelled) = (Vec::new(), false, false);
                for frame in 0..4 {
                    let mapping = chart.get_chart_mapping();
                    let events = chart.index_to_x(95.0).filter(|_| frame == 1).map(|x| {
                        egui::Event::PointerMoved(pos2(x, mapping.price_to_y(90.0) + 10.0))
                    });
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(800.0, 600.0),
                            )),
                            time: Some(frame as f64),
                            events: events.into_iter().collect(),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                            render_executions(ui, &chart, &marks, &display, None);
                        },
                    );
                    triangles = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Path(path)
                                if path.closed
                                    && path.points.len() == 3
                                    && (path.fill == gold || path.fill == theme.ui.accent) =>
                            {
                                Some((path.fill, path.points[0]))
                            }
                            _ => None,
                        })
                        .collect();
                    let hover = "Imported\nBuy 1 at 101.50, Sun 18 Feb '24 10:13:20";
                    hovered |= output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, Shape::Text(text) if text.galley.text() == hover)
                    });
                    labelled |= output.shapes.iter().any(|shape| {
                        matches!(&shape.shape,
                            Shape::Text(text) if Some(text.galley.text()) == label)
                    });
                    output.drop_without_applying_deltas();
                }
                let x = chart.index_to_x(95.0).unwrap();
                let low = chart.get_chart_mapping().price_to_y(90.0);
                let expected = if visible {
                    vec![
                        (gold, pos2(x, low + 4.0)),
                        (theme.ui.accent, pos2(x, low + 23.0)),
                    ]
                } else {
                    vec![]
                };
                assert_eq!(triangles, expected);
                assert_eq!(hovered, visible);
                assert_eq!(labelled, label.is_some());
            }
        }
    }
}
