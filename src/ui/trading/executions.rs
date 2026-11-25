//! Account and host fills on the chart, through `Trading::show` and [`render_executions`].
//! Each side and bar shares a stack of up to three triangles, below buys and above sells.
//! A host fill can carry the size, spacing and label font of its mark.

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
/// The most a host length draws at, so that sums of lengths stay finite.
const MAX_LENGTH: f32 = 1_000.0;

/// A host length in points: `value` up to [`MAX_LENGTH`], or `default` when it is negative or
/// not finite.
fn host_length(value: f32, default: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value.min(MAX_LENGTH)
    } else {
        default
    }
}

use super::account_panel::quantity_decimals;
use super::format::{Numbers, TradingFormatter};
use super::widgets::{side_color, side_name};

/// A borrowed fill to draw with [`render_executions`], optionally with host colour, hover text,
/// label and style.
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
    /// The size, spacing and label font of the fill's mark. A mark takes the style of the
    /// first of its fills, in the list's order, that has one; without any it keeps the
    /// default look.
    pub style: Option<&'a ExecutionMarkStyle>,
    /// Text replacing the side and quantity line beside the mark, and its average price with
    /// [`TradingSettings::show_execution_prices`], such as "Entry" or an account name. Shown
    /// verbatim. A mark takes the label of the first of its fills, in the list's order, that
    /// has one; the "+N" count stays, [`TradingSettings::show_execution_labels`] still hides
    /// it, and empty text draws no label line.
    pub label: Option<&'a str>,
    /// The colour of the mark's count and label, replacing the chart's text colour. A mark
    /// takes it from the first of its fills that has one, independently of the label text.
    pub label_color: Option<Color32>,
}

/// The size, spacing and label font of a fill mark, for [`ExecutionMark::with_style`]. The
/// default is the look of marks without a style. A length that is negative or not finite
/// counts as its default, and one above 1,000 points as 1,000.
///
/// ```
/// use egui_charts::ui::trading::ExecutionMarkStyle;
///
/// let mut style = ExecutionMarkStyle::default();
/// style.arrow_width = 10.0;
/// style.arrow_height = 12.0;
/// style.label_font = Some(egui::FontId::monospace(11.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ExecutionMarkStyle {
    /// The width of a triangle's base, in points. 14 by default. Zero draws no triangles and
    /// keeps the text and hover.
    pub arrow_width: f32,
    /// The height of a triangle, from its tip to its base. 17 by default. Zero draws no
    /// triangles and keeps the text and hover.
    pub arrow_height: f32,
    /// The space between the stacked triangles of one mark. 2 by default.
    pub stack_gap: f32,
    /// The space between the bar and the mark and between the triangles and their text. Lines
    /// of text keep half of it between them. 4 by default.
    pub gap: f32,
    /// The font of the mark's count and label, or the proportional font at the theme's label
    /// size. A family that is not installed draws as the proportional one, and the size is
    /// kept within the drawable font sizes.
    pub label_font: Option<FontId>,
}

/// The look of marks without a style.
static DEFAULT_STYLE: ExecutionMarkStyle = ExecutionMarkStyle {
    arrow_width: 2.0 * HALF_WIDTH,
    arrow_height: HEIGHT,
    stack_gap: STEP,
    gap: GAP,
    label_font: None,
};

impl Default for ExecutionMarkStyle {
    fn default() -> Self {
        DEFAULT_STYLE.clone()
    }
}

impl<'a> ExecutionMark<'a> {
    /// Uses the side colour and the built-in hover line for `execution`.
    pub fn new(execution: &'a Execution) -> Self {
        Self {
            execution,
            color: None,
            hover: None,
            style: None,
            label: None,
            label_color: None,
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

    /// Gives this fill's mark its size, spacing and label font.
    pub fn with_style(mut self, style: &'a ExecutionMarkStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Replaces the side and quantity line beside this fill's mark with untranslated host text.
    pub fn with_label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Colours the text beside this fill's mark, transparency included.
    pub fn with_label_color(mut self, color: Color32) -> Self {
        self.label_color = Some(color);
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
/// A mark takes its size, spacing and label font from the [`ExecutionMarkStyle`] of the first
/// of its fills that has one, and its label text and label colour from the first of its fills
/// that has each.
///
/// All supplied fills are treated as belonging to the chart's symbol. Filter a mixed list
/// with [`Execution::symbol`] first. Call after [`Chart::show`], once per chart with the full
/// list: separate calls place overlapping marks independently. The chart's theme applies.
/// Prices and quantities use `formatter` for the chart symbol when it returns text, otherwise
/// prices use the instrument's decimals (or two), and quantities their own decimals up to eight.
///
/// ```
/// use egui_charts::model::{Execution, OrderId, OrderSide};
/// use egui_charts::ui::trading::{ExecutionMark, ExecutionMarkStyle, render_executions};
/// use egui_charts::widget::{Chart, settings::TradingSettings};
///
/// fn draw(ui: &mut egui::Ui, chart: &mut Chart, imported: &[(Execution, String)]) {
///     chart.show(ui);
///     let mut style = ExecutionMarkStyle::default();
///     style.arrow_height = 12.0;
///     let marks: Vec<_> = imported.iter()
///         .filter(|(fill, _)| fill.symbol.as_deref() == Some(chart.symbol()))
///         .map(|(fill, note)| ExecutionMark::new(fill)
///             .with_color(egui::Color32::GOLD).with_hover(note).with_style(&style)
///             .with_label("Imported").with_label_color(egui::Color32::GOLD))
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
/// A mark's sizes and font come from its first fill with a style, else the defaults.
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
        let style = run
            .iter()
            .find_map(|(_, mark)| mark.style)
            .unwrap_or(&DEFAULT_STYLE);
        let half_width = host_length(style.arrow_width, 2.0 * HALF_WIDTH) / 2.0;
        let height = host_length(style.arrow_height, HEIGHT);
        let step = host_length(style.stack_gap, STEP);
        let gap = host_length(style.gap, GAP);
        let Some(x) = chart.index_to_x(index) else {
            continue;
        };
        if x < mapping.rect.left() - half_width || x > mapping.rect.right() + half_width {
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
            OrderSide::Buy => (1.0, low_y.max(high_y) + gap),
            OrderSide::Sell => (-1.0, low_y.min(high_y) - gap),
        };
        if !edge.is_finite() {
            continue;
        }
        let at = |dx: f32, dy: f32| pos2(x + dx, edge + down * dy);
        let drawn = run.len().min(MAX_STACK);
        if half_width > 0.0 && height > 0.0 {
            for (triangle, (_, mark)) in run.iter().take(MAX_STACK).enumerate() {
                let dy = triangle as f32 * (height + step);
                painter.add(Shape::convex_polygon(
                    vec![
                        at(0.0, dy),
                        at(half_width, dy + height),
                        at(-half_width, dy + height),
                    ],
                    mark.color.unwrap_or(color),
                    Stroke::NONE,
                ));
            }
        }
        let length = drawn as f32 * height + (drawn - 1) as f32 * step;
        let mut rect = Rect::from_two_pos(at(-half_width, 0.0), at(half_width, length));
        let more = (run.len() > MAX_STACK).then(|| format!("+{}", run.len() - MAX_STACK));
        let own_label = run.iter().find_map(|(_, mark)| mark.label);
        let text_color = run
            .iter()
            .find_map(|(_, mark)| mark.label_color)
            .unwrap_or(chart.config().text_color);
        let label = display.show_execution_labels.then(|| {
            if let Some(text) = own_label {
                return text.to_owned();
            }
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
        let label = label.filter(|text| !text.is_empty());
        let font = match &style.label_font {
            Some(font) => {
                let installed = (ui.ctx())
                    .fonts(|fonts| fonts.definitions().families.contains_key(&font.family));
                FontId::new(
                    crate::styles::typography::font_size(font.size),
                    if installed {
                        font.family.clone()
                    } else {
                        egui::FontFamily::Proportional
                    },
                )
            }
            None => FontId::proportional(ui.active_theme().text.label),
        };
        let lines: Vec<_> = more
            .into_iter()
            .chain(label)
            .map(|line| painter.layout_no_wrap(line, font.clone(), text_color))
            .collect();
        if !lines.is_empty() {
            let height: f32 = lines.iter().map(|line| line.size().y).sum::<f32>()
                + (lines.len() - 1) as f32 * gap / 2.0;
            let width = lines.iter().map(|line| line.size().x).fold(0.0, f32::max);
            let size = egui::vec2(width, height);
            let top = if down > 0.0 {
                rect.bottom() + gap
            } else {
                rect.top() - gap - size.y
            };
            let block = (0..MAX_LABEL_ROWS)
                .map(|row| {
                    Rect::from_min_size(pos2(x - size.x / 2.0, top), size)
                        .translate(egui::vec2(0.0, down * row as f32 * (size.y + gap)))
                })
                .find(|block| {
                    // The nearest labels, the likeliest to overlap, were placed last.
                    !text_rects
                        .iter()
                        .rev()
                        .any(|other| other.expand(gap / 2.0).intersects(*block))
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
                    y += down * (size.y + gap / 2.0);
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
    /// hide them through the shared display switch. A mark takes its label text and colour from
    /// the first fill with each; the labels switch still hides them. Invalid host fills never
    /// reach a mark.
    #[test]
    fn host_fills_draw_with_their_own_colour_and_hover() {
        let teal = Color32::from_rgba_unmultiplied(0, 150, 136, 160);
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            // The labels of fills a and b, whether b has its own label colour, and the text
            // expected with whether it is in that colour.
            let plain = (None, None, false);
            for (visible, labels, prices, (a_label, b_label, b_color), shown) in [
                (true, true, false, plain, Some(("Buy 2", false))),
                (false, true, false, plain, None),
                (true, true, true, plain, Some(("Buy 2 @ 101.25", false))),
                (true, false, true, plain, None),
                (
                    true,
                    true,
                    false,
                    (Some("Entry"), Some("Exit"), true),
                    Some(("Entry", true)),
                ),
                (
                    true,
                    true,
                    true,
                    (Some("Entry"), None, false),
                    Some(("Entry", false)),
                ),
                (true, false, false, (Some("Entry"), None, true), None),
                (true, true, false, (Some(""), None, false), None),
            ] {
                let label = shown.map(|(text, _)| text);
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
                let mut a = ExecutionMark::new(&a)
                    .with_color(gold)
                    .with_hover("Imported");
                if let Some(text) = a_label {
                    a = a.with_label(text);
                }
                let mut b = ExecutionMark::new(&b);
                if let Some(text) = b_label {
                    b = b.with_label(text);
                }
                if b_color {
                    b = b.with_label_color(teal);
                }
                let mut marks = vec![a, b];
                marks.extend(invalid.iter().map(ExecutionMark::new));
                let display = TradingSettings {
                    show_executions: visible,
                    show_execution_labels: labels,
                    show_execution_prices: prices,
                    ..TradingSettings::default()
                };
                let (mut triangles, mut hovered, mut labelled) = (Vec::new(), false, None);
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
                    for shape in &output.shapes {
                        // An empty label leaves no backing either.
                        if let Shape::Rect(backing) = &shape.shape {
                            assert!(
                                backing.fill != theme.chart.bg || backing.rect.width() > 3.0,
                                "backing of an empty label"
                            );
                        }
                        let Shape::Text(text) = &shape.shape else {
                            continue;
                        };
                        let written = text.galley.text();
                        if Some(written) == label {
                            labelled = Some(text.galley.job.sections[0].format.color);
                        } else {
                            assert!(
                                !["Buy 2", "Buy 2 @ 101.25", "Entry", "Exit"].contains(&written),
                                "{written} drawn instead of {label:?}"
                            );
                        }
                    }
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
                let text_color = if shown.is_some_and(|(_, host)| host) {
                    teal
                } else {
                    chart.config().text_color
                };
                assert_eq!(labelled, label.map(|_| text_color));
            }
        }
    }

    /// A stack uses its first supplied style, keeps the old look without one, and sanitizes
    /// host lengths and fonts before drawing. Coordinates below are calculated from the bar's
    /// low, two triangle heights and their intervening space, independently of the renderer.
    #[test]
    fn fill_marks_take_their_style_from_their_fills() {
        let style = ExecutionMarkStyle {
            arrow_width: 10.0,
            arrow_height: 12.0,
            stack_gap: 3.0,
            gap: 6.0,
            label_font: Some(FontId::monospace(11.0)),
        };
        let other = ExecutionMarkStyle {
            arrow_height: 30.0,
            ..ExecutionMarkStyle::default()
        };
        let bad = ExecutionMarkStyle {
            arrow_width: f32::NAN,
            arrow_height: -1.0,
            stack_gap: f32::INFINITY,
            gap: f32::NEG_INFINITY,
            label_font: Some(FontId::new(
                f32::NAN,
                egui::FontFamily::Name("Not Installed".into()),
            )),
        };
        let zero = ExecutionMarkStyle {
            arrow_width: 0.0,
            ..ExecutionMarkStyle::default()
        };
        let huge = ExecutionMarkStyle {
            arrow_height: f32::MAX,
            ..ExecutionMarkStyle::default()
        };
        let defaults = [
            [pos2(0.0, 4.0), pos2(7.0, 21.0), pos2(-7.0, 21.0)],
            [pos2(0.0, 23.0), pos2(7.0, 40.0), pos2(-7.0, 40.0)],
        ];
        let styled = [
            [pos2(0.0, 6.0), pos2(5.0, 18.0), pos2(-5.0, 18.0)],
            [pos2(0.0, 21.0), pos2(5.0, 33.0), pos2(-5.0, 33.0)],
        ];
        let capped = [
            [pos2(0.0, 4.0), pos2(7.0, 1004.0), pos2(-7.0, 1004.0)],
            [pos2(0.0, 1006.0), pos2(7.0, 2006.0), pos2(-7.0, 2006.0)],
        ];
        let theme = crate::theme::Theme::dark();
        let default_font = FontId::proportional(theme.text.label);
        type Row<'a> = (
            [Option<&'a ExecutionMarkStyle>; 2],
            &'a [[egui::Pos2; 3]],
            Option<(FontId, f32)>,
        );
        let rows: [Row<'_>; 7] = [
            ([None, None], &defaults, Some((default_font.clone(), 44.0))),
            (
                [Some(&style), None],
                &styled,
                Some((FontId::monospace(11.0), 39.0)),
            ),
            (
                [None, Some(&style)],
                &styled,
                Some((FontId::monospace(11.0), 39.0)),
            ),
            (
                [Some(&style), Some(&other)],
                &styled,
                Some((FontId::monospace(11.0), 39.0)),
            ),
            (
                [Some(&bad), None],
                &defaults,
                Some((FontId::proportional(1.0), 44.0)),
            ),
            ([Some(&zero), None], &[], Some((default_font, 44.0))),
            ([Some(&huge), None], &capped, None),
        ];
        for (row, (styles, expected, label)) in rows.into_iter().enumerate() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = test_frames::chart(&[90.0], &[110.0]);
            chart.set_theme_override(Some(theme.clone()));
            let time =
                chart.bars(crate::widget::SeriesId::MAIN)[95].time + chrono::Duration::hours(12);
            let a = Execution::new(OrderId::new("a"), time, OrderSide::Buy, 101.0, 1.0);
            let b = Execution::new(OrderId::new("b"), time, OrderSide::Buy, 101.5, 1.0);
            let fills = [a, b];
            let marks = fills
                .iter()
                .zip(styles)
                .map(|(fill, style)| {
                    let mut mark = ExecutionMark::new(fill);
                    mark.style = style;
                    mark
                })
                .collect::<Vec<_>>();
            let (mut triangles, mut text) = (Vec::new(), None);
            for frame in 0..4 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        time: Some(frame as f64),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        render_executions(ui, &chart, &marks, &TradingSettings::default(), None);
                    },
                );
                triangles = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Path(path)
                            if path.closed
                                && path.points.len() == 3
                                && path.fill == theme.ui.accent =>
                        {
                            Some([path.points[0], path.points[1], path.points[2]])
                        }
                        _ => None,
                    })
                    .collect();
                text = output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == "Buy 2" => Some((
                        text.galley.job.sections[0].format.font_id.clone(),
                        text.pos.y,
                    )),
                    _ => None,
                });
                output.drop_without_applying_deltas();
            }
            let origin = pos2(
                chart.index_to_x(95.0).unwrap(),
                chart.get_chart_mapping().price_to_y(90.0),
            );
            assert_eq!(triangles.len(), expected.len(), "triangles in row {row}");
            for (actual, expected) in triangles.iter().zip(expected) {
                for (actual, expected) in actual.iter().zip(expected) {
                    assert!(
                        (*actual - origin - expected.to_vec2()).length() < 0.01,
                        "row {row}: {actual:?}, offset {expected:?} from {origin:?}"
                    );
                }
            }
            if let Some((font, top)) = label {
                let (actual_font, actual_top) = text.expect("stack label");
                assert_eq!(actual_font, font, "font in row {row}");
                assert!(
                    (actual_top - origin.y - top).abs() < 0.6,
                    "label in row {row}: {actual_top} from {} + {top}",
                    origin.y
                );
            }
        }
    }
}
