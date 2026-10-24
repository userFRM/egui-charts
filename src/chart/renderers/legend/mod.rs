//! In-pane series readout and supplementary rows.

use super::{price_decimals, semibold};
use crate::chart::renderers::StyleColors;
use crate::model::{Bar, Instrument, Level1Quote, percent_change};
use crate::studies::Indicator;
use crate::styles::typography;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, ImageSource, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;

/// A host-supplied status mark. Its text and colours carry no built-in market meaning.
pub struct LegendStatus<'a> {
    /// Short text shown beside the instrument title.
    pub text: &'a str,
    /// Status text colour.
    pub color: Color32,
    /// Status background colour.
    pub background: Color32,
}

/// The quote control clicked by the user. The host decides how to handle the intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteSide {
    /// Sell at the displayed bid.
    Sell,
    /// Buy at the displayed ask.
    Buy,
}

/// An action on the zero-based study index in a legend's study slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudyAction {
    /// Open the study's settings in the host.
    Settings(usize),
    /// Toggle study visibility.
    ToggleVisibility(usize),
    /// Remove the study.
    Remove(usize),
}

/// An in-pane legend. Historical readout data and current quotes are supplied independently.
pub struct Legend<'a> {
    /// Fallback title when the instrument has no description.
    pub symbol: &'a str,
    /// Display interval.
    pub timeframe: &'a str,
    /// Description, exchange and price precision reported by the host.
    pub instrument: Option<&'a Instrument>,
    /// The bar selected by the crosshair, or the latest bar when none is selected.
    pub bar: &'a Bar,
    /// Close of the bar preceding `bar`, including when it is outside the viewport.
    pub previous_close: Option<f64>,
    /// Current host quote, independent of `bar`. Invalid or crossed quotes are omitted.
    pub quote: Option<&'a Level1Quote>,
    /// Optional host-owned instrument image.
    pub badge: Option<ImageSource<'a>>,
    /// Optional host-owned status marks.
    pub statuses: &'a [LegendStatus<'a>],
    /// Studies on the chart. Overlay studies retain a row while hidden so they can be shown again.
    pub studies: &'a [Box<dyn Indicator>],
    /// Display inputs supplied by the host, in `studies` order. Missing entries are omitted.
    pub study_inputs: &'a [&'a str],
    /// Global index of `bar` in each study's values.
    pub bar_index: usize,
    /// Whether this series has volume.
    pub show_volume: bool,
    /// Display the instrument title and its host status marks.
    pub show_title: bool,
    /// Prefer the host description over the ticker.
    pub title_as_description: bool,
    /// Display OHLC fields.
    pub show_ohlc: bool,
    /// Display absolute bar change.
    pub show_change: bool,
    /// Display percentage bar change.
    pub show_change_percent: bool,
    /// Display study names.
    pub show_indicator_titles: bool,
    /// Display supplied study inputs.
    pub show_indicator_inputs: bool,
    /// Display study outputs.
    pub show_indicator_values: bool,
}

impl<'a> Legend<'a> {
    /// A series readout with volume and no optional host metadata or quotes.
    pub fn new(symbol: &'a str, timeframe: &'a str, bar: &'a Bar) -> Self {
        Self {
            symbol,
            timeframe,
            instrument: None,
            bar,
            previous_close: None,
            quote: None,
            badge: None,
            statuses: &[],
            studies: &[],
            study_inputs: &[],
            bar_index: 0,
            show_volume: true,
            show_title: true,
            title_as_description: true,
            show_ohlc: true,
            show_change: true,
            show_change_percent: true,
            show_indicator_titles: true,
            show_indicator_inputs: true,
            show_indicator_values: true,
        }
    }

    /// Paints inside `plot`, 8 points from its edges, and reports a quote intent.
    /// Supplementary volume and study rows collapse together; quotes remain visible.
    pub fn show(&self, ui: &mut Ui, plot: Rect) -> Option<QuoteSide> {
        self.show_with_study_actions(ui, plot).0
    }

    /// Paint the legend and report quote and study intents independently.
    /// Each study intent identifies its zero-based index in [`studies`](Self::studies).
    pub fn show_with_study_actions(
        &self,
        ui: &mut Ui,
        plot: Rect,
    ) -> (Option<QuoteSide>, Option<StudyAction>) {
        let theme = ui.active_theme();
        let colors = StyleColors {
            bullish: theme.chart.candle_up,
            bearish: theme.chart.candle_down,
            text: theme.ui.text,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        };
        self.show_with_colors(ui, plot, &colors)
    }

    pub(crate) fn show_with_colors(
        &self,
        ui: &mut Ui,
        plot: Rect,
        colors: &StyleColors,
    ) -> (Option<QuoteSide>, Option<StudyAction>) {
        let theme = ui.active_theme();
        let bounds = plot.shrink(8.0);
        if !bounds.is_positive() {
            return (None, None);
        }
        let id = ui.next_auto_id().with(("series_legend", self.symbol));
        ui.skip_ahead_auto_ids(1);
        let painter = ui.painter().with_clip_rect(bounds);
        let mut layout = Layout::new(&painter, bounds, colors);
        if self.show_title {
            if let Some(badge) = &self.badge {
                let rect = layout.reserve(Vec2::splat(18.0));
                let mut badge_ui = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
                badge_ui.set_clip_rect(painter.clip_rect());
                egui::Image::new(badge.clone()).paint_at(&badge_ui, rect);
            }
            let title = self.title();
            let status_width = self
                .statuses
                .iter()
                .map(|status| {
                    painter
                        .layout_no_wrap(
                            status.text.to_owned(),
                            FontId::proportional(typography::SM),
                            status.color,
                        )
                        .size()
                        .x
                        + 16.0
                })
                .sum::<f32>();
            let title_start = layout.pos;
            let title_width = (bounds.right() - layout.pos.x - status_width - 48.0).max(0.0);
            layout.text(title, typography::XXL, colors.text, title_width);
            for status in self.statuses {
                let galley = painter.layout_no_wrap(
                    status.text.to_owned(),
                    FontId::proportional(typography::SM),
                    status.color,
                );
                let rect = layout.reserve(Vec2::new(galley.size().x + 8.0, 18.0));
                painter.rect_filled(rect, 4.0, status.background);
                painter.galley(rect.center() - galley.size() / 2.0, galley, status.color);
            }
            #[cfg(feature = "icons")]
            {
                let title_rect = Rect::from_min_max(
                    title_start,
                    Pos2::new(
                        (layout.pos.x + 48.0).min(bounds.right()),
                        title_start.y + 24.0,
                    ),
                );
                let hovered = ui.rect_contains_pointer(title_rect);
                let flagged = ui
                    .ctx()
                    .data(|data| data.get_temp::<bool>(id.with("flag")).unwrap_or(false));
                if hovered
                    || flagged
                    || egui::Popup::is_id_open(ui.ctx(), id.with("more").with("popup"))
                {
                    let flag = layout.reserve(Vec2::splat(20.0)).intersect(bounds);
                    let more = layout.reserve(Vec2::splat(20.0)).intersect(bounds);
                    painter.rect_stroke(
                        title_rect,
                        3,
                        Stroke::new(1.0, theme.ui.border_control),
                        egui::StrokeKind::Inside,
                    );
                    let flag_response = ui.interact(flag, id.with("flag"), Sense::click());
                    crate::icons::icons::FLAG_MARK
                        .as_image_tinted(
                            Vec2::splat(16.0),
                            if flagged {
                                theme.ui.accent
                            } else {
                                theme.ui.text
                            },
                        )
                        .paint_at(ui, flag.shrink(2.0));
                    if flag_response.on_hover_text("Flag instrument").clicked() {
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(id.with("flag"), !flagged));
                    }
                    let more_response = ui
                        .interact(more, id.with("more"), Sense::click())
                        .on_hover_text("Series actions");
                    crate::icons::icons::UI_MORE_HORIZONTAL
                        .as_image_tinted(Vec2::splat(16.0), theme.ui.text)
                        .paint_at(ui, more.shrink(2.0));
                    egui::Popup::menu(&more_response).show(|ui| {
                        if ui.button("Copy symbol").clicked() {
                            ui.ctx().copy_text(self.symbol.to_owned());
                            ui.close();
                        }
                        if ui.button("Copy values").clicked() {
                            ui.ctx().copy_text(format!(
                                "Open {} High {} Low {} Close {} Vol {}",
                                self.bar.open,
                                self.bar.high,
                                self.bar.low,
                                self.bar.close,
                                self.bar.volume
                            ));
                            ui.close();
                        }
                    });
                }
            }
            #[cfg(not(feature = "icons"))]
            let _ = title_start;
        }
        let precision = self.price_precision(self.bar.close);
        if self.show_ohlc {
            layout.ohlc(self.bar, precision);
        }
        if self.show_change || self.show_change_percent {
            let reference = self.previous_close.unwrap_or(self.bar.open);
            let change = self.bar.close - reference;
            let pct = percent_change(reference, self.bar.close);
            let text = match (self.show_change, self.show_change_percent) {
                (true, true) => format!("{change:+.precision$} ({pct:+.2}%)"),
                (true, false) => format!("{change:+.precision$}"),
                _ => format!("{pct:+.2}%"),
            };
            layout.text(
                text,
                typography::MD,
                colors.bar_color(change >= 0.0),
                bounds.width(),
            );
        }
        if self.show_title || self.show_ohlc || self.show_change || self.show_change_percent {
            layout.new_row();
        }

        let action = self.quote.and_then(|quote| {
            let (bid, ask) = (quote.bid, quote.ask);
            if !bid.is_finite() || !ask.is_finite() || bid > ask || !quote.spread().is_finite() {
                return None;
            }
            let precision = self.price_precision(bid.abs().max(ask.abs()));
            let action = paint_quotes(ui, &mut layout, bid, ask, precision, id);
            layout.new_row();
            layout.pos.y += 4.0;
            action
        });

        let mut study_settings = None;
        let collapsed = ui
            .ctx()
            .data_mut(|data| data.get_temp::<bool>(id).unwrap_or(false));
        let has_rows = self.show_volume || self.studies.iter().any(|study| study.is_overlay());
        if !collapsed {
            if self.show_volume
                && !self
                    .studies
                    .iter()
                    .any(|study| study.is_volume() && study.is_overlay())
            {
                layout.field(
                    "Vol ",
                    self.instrument
                        .and_then(|instrument| instrument.volume_decimals)
                        .map_or_else(
                            || format_volume(self.bar.volume),
                            |decimals| {
                                crate::scales::PriceFormatter::format(
                                    &crate::scales::VolumeFormatter {
                                        precision: usize::from(decimals),
                                    },
                                    self.bar.volume,
                                )
                            },
                        ),
                    layout.bar_color(self.bar),
                );
                layout.new_row();
                layout.pos.y += 4.0;
            }
            for (index, study) in self
                .studies
                .iter()
                .enumerate()
                .filter(|(_, study)| study.is_overlay())
            {
                if study.is_volume() && !self.show_volume {
                    continue;
                }
                let row_start = layout.pos;
                if self.show_indicator_titles {
                    layout.text(
                        study.name().to_owned(),
                        typography::MD,
                        colors.text,
                        bounds.width(),
                    );
                }
                if study.is_volume() {
                    if self.show_indicator_inputs && study.plot_style(2).visible {
                        layout.text(
                            study.line_names().get(2).cloned().unwrap_or_default(),
                            typography::MD,
                            Color32::from_gray(112),
                            bounds.width(),
                        );
                    }
                    if self.show_indicator_values && study.is_visible() {
                        let up = study.values().value(0, self.bar_index);
                        if let Some(value) = up.or_else(|| study.values().value(1, self.bar_index))
                            && study.plot_style(usize::from(up.is_none())).visible
                        {
                            let label = crate::scales::PriceFormatter::format(
                                &crate::scales::VolumeFormatter {
                                    precision: study.precision().unwrap_or(3),
                                },
                                value,
                            );
                            layout.text(
                                label,
                                typography::MD,
                                colors.bar_color(up.is_some()),
                                bounds.width(),
                            );
                        }
                    }
                } else {
                    if self.show_indicator_inputs
                        && let Some(inputs) = self
                            .study_inputs
                            .get(index)
                            .filter(|inputs| !inputs.is_empty())
                    {
                        layout.text(
                            (*inputs).to_owned(),
                            typography::MD,
                            theme.ui.text_muted,
                            bounds.width(),
                        );
                    }
                    let study_colors = study.colors();
                    let precision = study.precision().unwrap_or(precision);
                    for index in (0..study.line_cnt()).filter(|index| {
                        self.show_indicator_values
                            && study.is_visible()
                            && study.plot_style(*index).visible
                    }) {
                        let value = study.plot_value(index, self.bar_index).unwrap_or(f64::NAN);
                        let text = if value.is_finite() {
                            format!("{value:.precision$}")
                        } else {
                            "∅".to_owned()
                        };
                        let color = study_colors.get(index).copied().unwrap_or(colors.text);
                        layout.text(text, typography::MD, color, bounds.width());
                    }
                }
                let controls = layout.reserve(Vec2::new(60.0, 20.0)).intersect(bounds);
                let row = Rect::from_min_max(row_start, controls.right_bottom()).intersect(bounds);
                let responses = study_controls(
                    ui,
                    row,
                    controls,
                    id.with(("study", index)),
                    study.is_visible(),
                );
                for (response, intent) in responses.iter().zip([
                    StudyAction::ToggleVisibility(index),
                    StudyAction::Settings(index),
                    StudyAction::Remove(index),
                ]) {
                    if response.clicked() {
                        study_settings = Some(intent);
                    }
                }
                layout.new_row();
            }
        }
        if has_rows {
            let rect = layout.reserve(Vec2::new(29.0, 21.0));
            let response = ui.interact(rect.intersect(bounds), id, Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if collapsed {
                        "Expand legend"
                    } else {
                        "Collapse legend"
                    },
                )
            });
            painter.rect(
                rect,
                3.0,
                painter.ctx().active_theme().chart.bg,
                Stroke::new(1.0, painter.ctx().active_theme().ui.border),
                egui::StrokeKind::Inside,
            );
            let c = rect.center();
            let dy = if collapsed { -2.0 } else { 2.0 };
            painter.add(egui::Shape::line(
                vec![
                    c + Vec2::new(-4.0, dy),
                    c + Vec2::new(0.0, -dy),
                    c + Vec2::new(4.0, dy),
                ],
                Stroke::new(1.5, colors.text),
            ));
            if response.has_focus() {
                painter.rect_stroke(
                    rect,
                    3.0,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                ui.ctx().data_mut(|data| data.insert_temp(id, !collapsed));
            }
        }
        (action, study_settings)
    }

    fn title(&self) -> String {
        let instrument = self.instrument;
        [
            if self.title_as_description {
                instrument
                    .and_then(|i| i.description.as_deref())
                    .unwrap_or(self.symbol)
            } else {
                instrument.map_or(self.symbol, |i| i.symbol.as_str())
            },
            self.timeframe,
            instrument
                .and_then(|i| i.exchange.as_deref())
                .unwrap_or_default(),
        ]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
    }

    fn price_precision(&self, price: f64) -> usize {
        self.instrument
            .and_then(crate::scales::instrument_price_precision)
            .map_or_else(|| price_decimals(price), usize::from)
    }
}

pub(crate) fn study_controls(
    ui: &mut Ui,
    row: Rect,
    controls: Rect,
    id: egui::Id,
    visible: bool,
) -> [Response; 3] {
    let theme = ui.active_theme();
    if ui.rect_contains_pointer(row) {
        ui.painter().rect_stroke(
            row,
            3.0,
            Stroke::new(1.0, theme.ui.border_control),
            egui::StrokeKind::Inside,
        );
    }
    std::array::from_fn(|index| {
        let rect = Rect::from_min_size(
            controls.min + Vec2::new(index as f32 * 20.0, 0.0),
            Vec2::splat(20.0),
        )
        .intersect(controls);
        let label = match index {
            0 if visible => "Hide study",
            0 => "Show study",
            1 => "Study settings",
            _ => "Remove study",
        };
        let response = ui
            .interact(rect, id.with(index), Sense::click())
            .on_hover_text(label);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        if response.hovered() {
            ui.painter().rect_filled(rect, 3.0, theme.ui.btn_bg_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        #[cfg(feature = "icons")]
        {
            let icon = match index {
                0 if visible => &crate::icons::icons::EYE,
                0 => &crate::icons::icons::EYE_HIDE,
                1 => &crate::icons::icons::SETTINGS,
                _ => &crate::icons::icons::CLOSE,
            };
            icon.as_image_tinted(Vec2::splat(14.0), theme.ui.text_secondary)
                .paint_at(ui, rect.shrink(3.0));
        }
        #[cfg(not(feature = "icons"))]
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            [if visible { "−" } else { "+" }, "...", "×"][index],
            FontId::proportional(14.0),
            theme.ui.text_secondary,
        );
        response
    })
}

struct Layout<'a> {
    painter: &'a egui::Painter,
    bounds: Rect,
    pos: Pos2,
    height: f32,
    background: egui::layers::ShapeIdx,
    colors: &'a StyleColors,
}

impl<'a> Layout<'a> {
    fn new(painter: &'a egui::Painter, bounds: Rect, colors: &'a StyleColors) -> Self {
        Self {
            painter,
            bounds,
            pos: bounds.min,
            height: 24.0,
            background: painter.add(egui::Shape::Noop),
            colors,
        }
    }

    fn new_row(&mut self) {
        self.pos = Pos2::new(self.bounds.left(), self.pos.y + self.height);
        self.height = 22.0;
        self.background = self.painter.add(egui::Shape::Noop);
    }

    fn reserve(&mut self, size: Vec2) -> Rect {
        if self.pos.x > self.bounds.left() && self.pos.x + size.x > self.bounds.right() {
            self.new_row();
        }
        self.height = self.height.max(size.y);
        let rect = Rect::from_min_size(
            Pos2::new(self.pos.x, self.pos.y + (self.height - size.y) / 2.0),
            size,
        );
        self.painter.set(
            self.background,
            egui::Shape::rect_filled(
                Rect::from_min_max(
                    Pos2::new(self.bounds.left(), self.pos.y),
                    Pos2::new(rect.right() + 4.0, self.pos.y + self.height),
                )
                .intersect(self.bounds),
                0.0,
                self.painter.ctx().active_theme().chart.bg,
            ),
        );
        self.pos.x += size.x + 8.0;
        rect
    }

    fn galley(&mut self, galley: Arc<egui::Galley>) {
        let rect = self.reserve(galley.size());
        self.painter.galley(rect.min, galley, self.colors.text);
    }

    fn text(&mut self, text: String, size: f32, color: Color32, max_width: f32) {
        let mut job =
            egui::text::LayoutJob::simple_singleline(text, FontId::proportional(size), color);
        job.wrap.max_width = max_width;
        job.wrap.max_rows = 1;
        self.galley(self.painter.layout_job(job));
    }

    fn field(&mut self, label: &str, value: String, color: Color32) {
        let mut job = egui::text::LayoutJob::default();
        let font = FontId::proportional(typography::MD);
        job.append(
            label,
            0.0,
            egui::text::TextFormat::simple(
                font.clone(),
                self.painter.ctx().active_theme().ui.text_secondary,
            ),
        );
        job.append(&value, 0.0, egui::text::TextFormat::simple(font, color));
        job.wrap.max_width = self.bounds.width();
        job.wrap.max_rows = 1;
        self.galley(self.painter.layout_job(job));
    }

    fn bar_color(&self, bar: &Bar) -> Color32 {
        self.colors.bar_color(bar.close >= bar.open)
    }

    fn ohlc(&mut self, bar: &Bar, precision: usize) {
        let color = self.bar_color(bar);
        for (label, value) in [
            ("O ", bar.open),
            ("H ", bar.high),
            ("L ", bar.low),
            ("C ", bar.close),
        ] {
            self.field(label, format!("{value:.precision$}"), color);
        }
    }
}

fn paint_quotes(
    ui: &mut Ui,
    layout: &mut Layout<'_>,
    bid: f64,
    ask: f64,
    precision: usize,
    id: egui::Id,
) -> Option<QuoteSide> {
    let spread = format!("{:.precision$}", ask - bid);
    let theme = ui.active_theme();
    let spread_galley = layout.painter.layout_no_wrap(
        spread,
        FontId::proportional(typography::XS),
        layout.colors.text,
    );
    let gap = (spread_galley.size().x + 12.0).max(36.0);
    let quote = |side: QuoteSide, price: f64, color| {
        let price = layout
            .painter
            .layout_job(egui::text::LayoutJob::single_section(
                format!("{price:.precision$}"),
                semibold(FontId::proportional(typography::MD), color),
            ));
        let label = layout.painter.layout_no_wrap(
            if side == QuoteSide::Sell {
                "SELL"
            } else {
                "BUY"
            }
            .to_owned(),
            FontId::proportional(typography::SM),
            color,
        );
        (price, label)
    };
    let sell = quote(QuoteSide::Sell, bid, layout.colors.bearish);
    let buy = quote(QuoteSide::Buy, ask, theme.ui.accent);
    let width = 66.0_f32
        .max(sell.0.size().x + 12.0)
        .max(buy.0.size().x + 12.0);
    let size = Vec2::new(width, 34.0);
    let sell_rect = layout.reserve(size);
    let sell_response = quote_box(
        ui,
        layout.painter,
        sell_rect,
        sell,
        layout.colors.bearish,
        "Sell at bid",
        id.with("sell"),
    );
    let spread_rect = Rect::from_min_size(
        Pos2::new(sell_rect.right(), sell_rect.top()),
        Vec2::new(gap, 34.0),
    );
    layout.painter.rect_filled(spread_rect, 0.0, theme.chart.bg);
    layout.painter.galley(
        spread_rect.center() - spread_galley.size() / 2.0,
        spread_galley,
        layout.colors.text,
    );
    layout.pos.x = sell_rect.right() + gap;
    let buy_rect = layout.reserve(size);
    let buy_response = quote_box(
        ui,
        layout.painter,
        buy_rect,
        buy,
        theme.ui.accent,
        "Buy at ask",
        id.with("buy"),
    );
    if sell_response.clicked() {
        Some(QuoteSide::Sell)
    } else if buy_response.clicked() {
        Some(QuoteSide::Buy)
    } else {
        None
    }
}

fn quote_box(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    (price, label): (Arc<egui::Galley>, Arc<egui::Galley>),
    color: Color32,
    name: &str,
    id: egui::Id,
) -> Response {
    let response = ui.interact(rect.intersect(painter.clip_rect()), id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    painter.rect(
        rect,
        5.0,
        ui.active_theme().chart.bg,
        Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        Pos2::new(rect.center().x - price.size().x / 2.0, rect.top() + 2.0),
        price,
        color,
    );
    painter.galley(
        Pos2::new(
            rect.center().x - label.size().x / 2.0,
            rect.bottom() - label.size().y - 2.0,
        ),
        label,
        color,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            5.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    response
}

fn format_volume(volume: f64) -> String {
    for (divisor, suffix) in [(1_000_000_000.0, "B"), (1_000_000.0, "M"), (1_000.0, "K")] {
        if volume.abs() >= divisor {
            return format!("{:.2}{suffix}", volume / divisor);
        }
    }
    format!("{volume:.0}")
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Key, PointerButton, Shape};

    #[test]
    fn quote_controls_and_collapse_belong_to_each_legend() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let bars = [
                Bar::new(
                    Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    100.0,
                    110.0,
                    90.0,
                    105.0,
                    1_000.0,
                ),
                Bar::new(
                    Utc.timestamp_opt(1_700_003_600, 0).unwrap(),
                    105.0,
                    110.0,
                    90.0,
                    95.0,
                    2_000.0,
                ),
            ];
            let instrument = Instrument::new("SAMPLE").with_price_decimals(2);
            let mut study = crate::studies::BollingerBands::new(2, 2.0);
            study.calculate(&bars);
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let quote = Level1Quote {
                ts: bars[1].time,
                symbol: "SAMPLE".into(),
                bid: 104.0,
                ask: 106.0,
                bid_size: 1.0,
                ask_size: 1.0,
            };
            let frame = |selected: usize, events| {
                let mut actions = [None; 2];
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        for (index, action) in actions.iter_mut().enumerate() {
                            let mut legend = Legend::new("SAMPLE", "1h", &bars[selected]);
                            legend.previous_close = selected.checked_sub(1).map(|i| bars[i].close);
                            legend.quote = Some(&quote);
                            legend.instrument = Some(&instrument);
                            legend.studies = &studies;
                            legend.study_inputs = &["2 2"];
                            legend.bar_index = selected;
                            *action = legend.show(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(index as f32 * 600.0, 0.0),
                                    Vec2::new(600.0, 400.0),
                                ),
                            );
                        }
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                (shapes, actions)
            };
            let texts = |shapes: &[egui::epaint::ClippedShape]| {
                shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let (first, _) = frame(0, vec![]);
            let (second, _) = frame(1, vec![]);
            let a = texts(&first);
            let b = texts(&second);
            for (text, values) in [
                ("C 105.00", &a),
                ("Vol 1.00K", &a),
                ("C 95.00", &b),
                ("Vol 2.00K", &b),
                ("-10.00 (-9.52%)", &b),
            ] {
                assert!(values.iter().any(|value| value == text), "{text}");
            }
            for text in ["BB", "2 2", "110.00", "100.00", "90.00"] {
                assert!(b.iter().any(|value| value == text), "{text}");
            }
            for text in ["104.00", "106.00", "2.00", "SELL", "BUY"] {
                assert!(a.iter().any(|value| value == text) && b.iter().any(|value| value == text));
            }
            let buttons: Vec<_> = second
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.stroke.width > 0.0 => Some(rect.rect),
                    _ => None,
                })
                .collect();
            assert_eq!(buttons.len(), 6);
            for (index, side) in [(3, QuoteSide::Sell), (4, QuoteSide::Buy)] {
                let pos = buttons[index].center();
                frame(1, vec![Event::PointerMoved(pos)]);
                frame(
                    1,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    }],
                );
                let (_, actions) = frame(
                    1,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                assert_eq!(actions, [None, Some(side)]);
            }
            let key = |key, pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            };
            // Tab through both legends, then activate each control in the second one.
            for (tabs, side) in [
                (7, Some(QuoteSide::Sell)),
                (1, Some(QuoteSide::Buy)),
                (4, None),
            ] {
                for _ in 0..tabs {
                    frame(1, vec![key(Key::Tab, true)]);
                    frame(1, vec![key(Key::Tab, false)]);
                }
                let (_, actions) = frame(1, vec![key(Key::Enter, true)]);
                assert_eq!(actions, [None, side]);
                frame(1, vec![key(Key::Enter, false)]);
            }
            let (collapsed, _) = frame(1, vec![]);
            let text = texts(&collapsed);
            assert_eq!(text.iter().filter(|text| *text == "Vol 2.00K").count(), 1);
            assert_eq!(text.iter().filter(|text| *text == "BB").count(), 1);
            assert_eq!(text.iter().filter(|text| *text == "104.00").count(), 2);
            frame(1, vec![key(Key::Enter, true)]);
            let (restored, _) = frame(1, vec![key(Key::Enter, false)]);
            assert_eq!(
                texts(&restored).iter().filter(|text| *text == "BB").count(),
                2
            );
        }
    }
}
