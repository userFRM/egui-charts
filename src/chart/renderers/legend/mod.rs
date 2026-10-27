//! In-pane series readout and supplementary rows.

use super::{price_decimals, semibold};
use crate::chart::renderers::StyleColors;
use crate::model::{Bar, DataStatus, Instrument, Level1Quote, percent_change};
use crate::studies::Indicator;
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
    /// Open the order ticket without placing an order.
    Ticket,
}

/// An action on the zero-based study index in a legend's study slice.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum StudyAction {
    /// Open the study's settings in the host.
    Settings(usize),
    /// Toggle study visibility.
    ToggleVisibility(usize),
    /// Remove the study.
    Remove(usize),
    /// Open the study's menu at `pos`: below the more-actions button, or at the pointer after a
    /// secondary click on the row.
    Menu {
        /// Zero-based study index.
        index: usize,
        /// Screen position of the menu's corner.
        pos: Pos2,
    },
}

/// An in-pane legend. Historical readout data and current quotes are supplied independently.
pub struct Legend<'a> {
    /// Host number formatting for this series' readouts.
    pub formatters: super::super::ValueFormatters<'a>,
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
    /// Display OHLC fields, or the close alone in a compact plot.
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
    /// The order quantity shown in a box under the quote buttons' spread, which the user clicks
    /// to change it.
    pub(crate) quantity: Option<f64>,
}

impl<'a> Legend<'a> {
    /// A series readout with volume and no optional host metadata or quotes.
    pub fn new(symbol: &'a str, timeframe: &'a str, bar: &'a Bar) -> Self {
        Self {
            formatters: Default::default(),
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
            quantity: None,
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
        let (quote, study, _, _) = self.show_with_colors(ui, plot, &colors);
        (quote, study)
    }

    /// Also returns the bottom of the legend and the quantity box, when the user clicked it.
    pub(crate) fn show_with_colors(
        &self,
        ui: &mut Ui,
        plot: Rect,
        colors: &StyleColors,
    ) -> (Option<QuoteSide>, Option<StudyAction>, f32, Option<Rect>) {
        let theme = ui.active_theme();
        let bounds = plot.shrink(theme.spacing.md);
        if !bounds.is_positive() {
            return (None, None, plot.top(), None);
        }
        let id = ui.next_auto_id().with(("series_legend", self.symbol));
        ui.skip_ahead_auto_ids(1);
        let painter = ui.painter().with_clip_rect(bounds);
        let mut layout = Layout::new(&painter, bounds, colors);
        #[cfg(feature = "icons")]
        let title_start = layout.pos;
        if self.show_title {
            if let Some(badge) = &self.badge {
                let rect = layout.reserve(Vec2::splat(18.0));
                let mut badge_ui = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
                badge_ui.set_clip_rect(painter.clip_rect());
                egui::Image::new(badge.clone()).paint_at(&badge_ui, rect);
            }
            let title = self.title();
            // Delayed and end-of-day data are marked ahead of the host's own marks.
            let data_status = match self
                .instrument
                .and_then(|instrument| instrument.data_status)
            {
                Some(DataStatus::Delayed) => Some(("D", "Delayed data")),
                Some(DataStatus::EndOfDay) => Some(("E", "End-of-day data")),
                _ => None,
            }
            .map(|(text, hint)| {
                let color = theme.ui.warning;
                let background = color.gamma_multiply(0.15);
                (
                    LegendStatus {
                        text,
                        color,
                        background,
                    },
                    hint,
                )
            });
            let statuses: Vec<_> = data_status
                .iter()
                .map(|(status, hint)| (status, Some(*hint)))
                .chain(self.statuses.iter().map(|status| (status, None)))
                .collect();
            let status_width = statuses
                .iter()
                .map(|(status, _)| {
                    painter
                        .layout_no_wrap(
                            status.text.to_owned(),
                            FontId::proportional(theme.text.small),
                            status.color,
                        )
                        .size()
                        .x
                        + 16.0
                })
                .sum::<f32>();
            let title_width = (bounds.right() - layout.pos.x - status_width - 64.0).max(0.0);
            layout.text(title, theme.text.chart_title, colors.text, title_width);
            for (status, hint) in statuses {
                let galley = painter.layout_no_wrap(
                    status.text.to_owned(),
                    FontId::proportional(theme.text.small),
                    status.color,
                );
                let rect = layout.reserve(Vec2::new(galley.size().x + 8.0, 18.0));
                painter.rect_filled(rect, theme.radius.sm, status.background);
                painter.galley(rect.center() - galley.size() / 2.0, galley, status.color);
                if let Some(hint) = hint {
                    let rect = rect.intersect(bounds);
                    ui.interact(rect, id.with("data_status"), Sense::hover())
                        .on_hover_text(hint);
                }
            }
        }
        let precision = self.price_precision(self.bar.close);
        if self.show_ohlc {
            if plot.width() < 600.0 {
                layout.field(
                    "C ",
                    self.formatters.price.map_or_else(
                        || format!("{:.precision$}", self.bar.close),
                        |formatter| formatter.format(self.bar.close),
                    ),
                    layout.bar_color(self.bar),
                );
            } else {
                layout.ohlc(self.bar, precision, self.formatters.price);
            }
        }
        if self.show_change || self.show_change_percent {
            let reference = self.previous_close.unwrap_or(self.bar.open);
            let change = self.bar.close - reference;
            let pct = percent_change(reference, self.bar.close);
            let absolute = self.formatters.price.map_or_else(
                || format!("{change:+.precision$}"),
                |formatter| formatter.format(change),
            );
            let percent = self
                .formatters
                .percentage
                .map_or_else(|| format!("{pct:+.2}%"), |formatter| formatter.format(pct));
            let text = match (self.show_change, self.show_change_percent) {
                (true, true) => format!("{absolute} ({percent})"),
                (true, false) => absolute,
                _ => percent,
            };
            layout.text(
                text,
                theme.text.ohlc,
                colors.bar_color(change >= 0.0),
                bounds.width(),
            );
        }
        // Values follow the title directly; the controls take the space after them on hover.
        #[cfg(feature = "icons")]
        if self.show_title {
            let size = Vec2::new(48.0, 20.0);
            // On a full row the controls cover its end instead of opening an empty row.
            let controls = if layout.pos.x + size.x > bounds.right() {
                Rect::from_min_size(
                    Pos2::new(
                        bounds.right() - size.x,
                        layout.pos.y + (layout.height - size.y) / 2.0,
                    ),
                    size,
                )
            } else {
                layout.reserve(size)
            };
            let row_right = if layout.pos.y > title_start.y {
                bounds.right()
            } else {
                controls.right().min(bounds.right())
            };
            let title_rect =
                Rect::from_min_max(title_start, Pos2::new(row_right, controls.bottom()));
            let hovered = ui.rect_contains_pointer(title_rect);
            let flagged = ui
                .ctx()
                .data(|data| data.get_temp::<bool>(id.with("flag")).unwrap_or(false));
            if hovered
                || flagged
                || egui::Popup::is_id_open(ui.ctx(), id.with("more").with("popup"))
            {
                let flag = Rect::from_min_size(controls.min, Vec2::splat(20.0)).intersect(bounds);
                let more =
                    Rect::from_min_size(controls.min + Vec2::new(28.0, 0.0), Vec2::splat(20.0))
                        .intersect(bounds);
                painter.rect_stroke(
                    title_rect,
                    theme.radius.sm,
                    Stroke::new(1.0, theme.ui.border_control),
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(controls, 0.0, theme.chart.bg);
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
        if self.show_title || self.show_ohlc || self.show_change || self.show_change_percent {
            layout.new_row();
        }

        let (action, quantity) = self.quote.map_or((None, None), |quote| {
            let (bid, ask) = (quote.bid, quote.ask);
            if !bid.is_finite() || !ask.is_finite() || bid > ask || !quote.spread().is_finite() {
                return (None, None);
            }
            let precision = self.price_precision(bid.abs().max(ask.abs()));
            let action = paint_quotes(ui, &mut layout, bid, ask, precision, self.quantity, id);
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
                    self.formatters
                        .volume
                        .map(|formatter| formatter.format(self.bar.volume))
                        .unwrap_or_else(|| {
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
                                )
                        }),
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
                        if study.is_volume() {
                            "Vol"
                        } else {
                            study.name()
                        }
                        .to_owned(),
                        theme.text.ohlc,
                        colors.text,
                        bounds.width(),
                    );
                }
                if study.is_volume() {
                    if self.show_indicator_inputs && study.plot_style(2).visible {
                        layout.text(
                            study.line_names().get(2).cloned().unwrap_or_default(),
                            theme.text.ohlc,
                            Color32::from_gray(112),
                            bounds.width(),
                        );
                    }
                    if self.show_indicator_values && study.is_visible() {
                        let up = study.values().value(0, self.bar_index);
                        let line = usize::from(up.is_none());
                        if let Some(value) = up.or_else(|| study.values().value(1, self.bar_index))
                            && study.plot_style(line).visible
                        {
                            // The column colour without its transparency.
                            let [r, g, b, _] = study
                                .colors()
                                .get(line)
                                .map_or(colors.bar_color(up.is_some()), |color| *color)
                                .to_srgba_unmultiplied();
                            layout.text(
                                study.format_value(value),
                                theme.text.ohlc,
                                Color32::from_rgb(r, g, b),
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
                            theme.text.ohlc,
                            theme.ui.text_muted,
                            bounds.width(),
                        );
                    }
                    let study_colors = study.colors();
                    let precision = study.precision().or_else(|| {
                        (study.value_format() == crate::scales::StudyFormat::Price)
                            .then_some(precision)
                    });
                    for index in (0..study.line_count()).filter(|index| {
                        self.show_indicator_values
                            && study.is_visible()
                            && study.plot_style(*index).visible
                    }) {
                        let value = study.plot_value(index, self.bar_index).unwrap_or(f64::NAN);
                        let text = if value.is_finite() {
                            if study.value_format() == crate::scales::StudyFormat::Price {
                                study.value_format().format(value, precision)
                            } else {
                                study.format_value(value)
                            }
                        } else {
                            "∅".to_owned()
                        };
                        let color = study_colors.get(index).copied().unwrap_or(colors.text);
                        layout.text(text, theme.text.ohlc, color, bounds.width());
                    }
                }
                let controls = layout.reserve(Vec2::new(80.0, 20.0)).intersect(bounds);
                let row_right = if layout.pos.y > row_start.y {
                    bounds.right()
                } else {
                    controls.right()
                };
                let row = Rect::from_min_max(row_start, Pos2::new(row_right, controls.bottom()))
                    .intersect(bounds);
                let (responses, menu) = study_controls(
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
                if let Some(pos) = menu {
                    study_settings = Some(StudyAction::Menu { index, pos });
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
                    theme.radius.sm,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                ui.ctx().data_mut(|data| data.insert_temp(id, !collapsed));
            }
        }
        (
            action,
            study_settings,
            layout.pos.y + layout.height,
            quantity,
        )
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

/// The row outline and its hide, settings, remove and more-actions buttons. Returns the first
/// three buttons and, when the more-actions button is clicked or the row secondary-clicked, the
/// corner of the study menu.
pub(crate) fn study_controls(
    ui: &mut Ui,
    row: Rect,
    controls: Rect,
    id: egui::Id,
    visible: bool,
) -> ([Response; 3], Option<Pos2>) {
    let theme = ui.active_theme();
    let touch_id = egui::Id::unique("legend_touch_input");
    let input_touch = ui.input(|input| {
        if input.any_touches()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Touch { .. }))
        {
            Some(true)
        } else if input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        {
            Some(false)
        } else {
            None
        }
    });
    let touch = ui.ctx().data_mut(|data| {
        let touch = input_touch.unwrap_or_else(|| data.get_temp::<bool>(touch_id).unwrap_or(false));
        data.insert_temp(touch_id, touch);
        touch
    });
    let row_response = ui.interact(row, id.with("row"), Sense::click());
    row_response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Study controls"));
    let mut menu = row_response
        .secondary_clicked()
        .then(|| row_response.interact_pointer_pos())
        .flatten();
    let clicked_elsewhere = row_response.clicked_elsewhere();
    let clicked = row_response.clicked();
    let selected = ui.ctx().data_mut(|data| {
        let mut selected = data.get_temp::<bool>(id).unwrap_or(false);
        if clicked_elsewhere || !touch {
            selected = false;
        } else if clicked {
            selected = true;
        }
        data.insert_temp(id, selected);
        selected
    });
    // Keep controls focusable while Tab transfers focus from their row.
    let focused = row_response.has_focus()
        || row_response.lost_focus()
        || (0..4_usize).any(|index| ui.memory(|memory| memory.has_focus(id.with(index))));
    let hovered = !touch
        && ui.input(|input| input.pointer.hover_pos().is_some())
        && ui.rect_contains_pointer(row);
    let revealed = selected || focused || hovered;
    if revealed {
        ui.painter().rect_stroke(
            row,
            theme.radius.sm,
            Stroke::new(1.0, theme.ui.border_control),
            egui::StrokeKind::Inside,
        );
    }
    let buttons: [Response; 4] = std::array::from_fn(|index| {
        let rect = Rect::from_min_size(
            controls.min + Vec2::new(index as f32 * 20.0, 0.0),
            Vec2::splat(20.0),
        )
        .intersect(controls);
        let label = match index {
            0 if visible => "Hide study",
            0 => "Show study",
            1 => "Study settings",
            2 => "Remove study",
            _ => "More actions",
        };
        let response = ui.interact(
            rect,
            id.with(index),
            if revealed {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if !revealed {
            return response;
        }
        let response = response.on_hover_text(label);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, theme.radius.sm, theme.ui.btn_bg_hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        #[cfg(feature = "icons")]
        {
            let icon = match index {
                0 if visible => &crate::icons::icons::EYE,
                0 => &crate::icons::icons::EYE_HIDE,
                1 => &crate::icons::icons::SETTINGS,
                2 => &crate::icons::icons::TRASH,
                _ => &crate::icons::icons::UI_MORE_HORIZONTAL,
            };
            icon.as_image_tinted(Vec2::splat(14.0), theme.ui.text_secondary)
                .paint_at(ui, rect.shrink(3.0));
        }
        #[cfg(not(feature = "icons"))]
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            [if visible { "−" } else { "+" }, "...", "×", "•••"][index],
            FontId::proportional(14.0),
            theme.ui.text_secondary,
        );
        response
    });
    let [hide, settings, remove, more] = buttons;
    if more.clicked() {
        menu = Some(more.rect.left_bottom());
    }
    ([hide, settings, remove], menu)
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
        let font = FontId::proportional(self.painter.ctx().active_theme().text.ohlc);
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

    fn ohlc(
        &mut self,
        bar: &Bar,
        precision: usize,
        formatter: Option<&dyn crate::scales::PriceFormatter>,
    ) {
        let color = self.bar_color(bar);
        for (label, value) in [
            ("O ", bar.open),
            ("H ", bar.high),
            ("L ", bar.low),
            ("C ", bar.close),
        ] {
            self.field(
                label,
                formatter.map_or_else(
                    || format!("{value:.precision$}"),
                    |formatter| formatter.format(value),
                ),
                color,
            );
        }
    }
}

/// The Sell and Buy buttons with the spread between them and, under it, `quantity` in a box.
/// Returns the button clicked and the quantity box, when clicked.
fn paint_quotes(
    ui: &mut Ui,
    layout: &mut Layout<'_>,
    bid: f64,
    ask: f64,
    precision: usize,
    quantity: Option<f64>,
    id: egui::Id,
) -> (Option<QuoteSide>, Option<Rect>) {
    let spread = format!("{:.precision$}", ask - bid);
    let theme = ui.active_theme();
    let spread_galley = layout.painter.layout_no_wrap(
        spread,
        FontId::proportional(theme.text.tiny),
        layout.colors.text,
    );
    let gap = (spread_galley.size().x + 12.0).max(36.0);
    let quote = |side: QuoteSide, price: f64, color: Color32| {
        let price = layout
            .painter
            .layout_job(egui::text::LayoutJob::single_section(
                format!("{price:.precision$}"),
                semibold(FontId::proportional(theme.text.ohlc), color),
            ));
        let label = layout.painter.layout_no_wrap(
            if side == QuoteSide::Sell {
                "SELL"
            } else {
                "BUY"
            }
            .to_owned(),
            FontId::proportional(theme.text.small),
            color,
        );
        (price, label)
    };
    let sell = quote(QuoteSide::Sell, bid, theme.chart.candle_down);
    let buy = quote(QuoteSide::Buy, ask, theme.ui.accent);
    let width = 59.0_f32
        .max(sell.0.size().x + 12.0)
        .max(buy.0.size().x + 12.0);
    let size = Vec2::new(width, 34.0);
    let sell_rect = layout.reserve(size);
    let sell_response = quote_box(
        ui,
        layout.painter,
        sell_rect,
        sell,
        theme.chart.candle_down,
        "Sell at bid",
        id.with("sell"),
    );
    let spread_rect = Rect::from_min_size(
        Pos2::new(sell_rect.right(), sell_rect.top()),
        Vec2::new(gap, 34.0),
    );
    layout.painter.rect_filled(spread_rect, 0.0, theme.chart.bg);
    let quantity = quantity.map(|quantity| {
        let text = layout.painter.layout_no_wrap(
            compact_quantity(quantity),
            FontId::proportional(theme.text.small),
            layout.colors.text,
        );
        let rect = Rect::from_center_size(
            Pos2::new(spread_rect.center().x, spread_rect.bottom() - 9.0),
            Vec2::new((text.size().x + 10.0).max(gap - 8.0), 18.0),
        );
        let response = ui
            .interact(
                rect.intersect(layout.painter.clip_rect()),
                id.with("quantity"),
                Sense::click(),
            )
            .on_hover_text("Order quantity");
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Order quantity"));
        let fill = if response.hovered() {
            theme.ui.btn_bg_active
        } else {
            theme.ui.btn_bg_hover
        };
        layout.painter.rect_filled(rect, theme.radius.sm, fill);
        layout
            .painter
            .galley(rect.center() - text.size() / 2.0, text, layout.colors.text);
        (rect, response.clicked())
    });
    // With the quantity box under it, the spread sits at the top.
    let spread_y = match quantity {
        Some((rect, _)) => (spread_rect.top() + rect.top() - spread_galley.size().y) / 2.0,
        None => spread_rect.center().y - spread_galley.size().y / 2.0,
    };
    layout.painter.galley(
        Pos2::new(
            spread_rect.center().x - spread_galley.size().x / 2.0,
            spread_y,
        ),
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
    layout.pos.x = buy_rect.right() + 8.0;
    let dropdown = layout.reserve(Vec2::new(28.0, size.y));
    layout.painter.rect(
        dropdown,
        4.0,
        theme.chart.bg,
        Stroke::new(1.0, theme.ui.border),
        egui::StrokeKind::Inside,
    );
    let c = dropdown.center();
    let points = [
        c + egui::vec2(-4.0, -2.0),
        c + egui::vec2(0.0, 1.5),
        c + egui::vec2(4.0, -2.0),
    ];
    for pair in points.windows(2) {
        layout
            .painter
            .line_segment([pair[0], pair[1]], Stroke::new(1.5, theme.ui.text));
    }
    for point in points {
        layout.painter.circle_filled(point, 0.75, theme.ui.text);
    }
    let dropdown = ui
        .interact(dropdown, id.with("ticket"), Sense::click())
        .on_hover_text("Order ticket");
    dropdown.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Order ticket"));
    let action = if dropdown.clicked() {
        Some(QuoteSide::Ticket)
    } else if sell_response.clicked() {
        Some(QuoteSide::Sell)
    } else if buy_response.clicked() {
        Some(QuoteSide::Buy)
    } else {
        None
    };
    let clicked = quantity.and_then(|(rect, clicked)| clicked.then_some(rect));
    (action, clicked)
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
    // The chart background inside an outline in the side's colour, which also colours the text.
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

fn compact_quantity(quantity: f64) -> String {
    for (divisor, suffix) in [(1_000_000_000.0, "B"), (1_000_000.0, "M"), (1_000.0, "K")] {
        if quantity.abs() >= divisor {
            let number = format!("{:.2}", quantity / divisor);
            return format!(
                "{}{suffix}",
                number.trim_end_matches('0').trim_end_matches('.')
            );
        }
    }
    quantity.to_string()
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
    fn study_controls_reveal_on_touch_and_dismiss_outside() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let row = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(220.0, 20.0));
            let controls = Rect::from_min_size(Pos2::new(150.0, 10.0), Vec2::new(80.0, 20.0));
            let settings = controls.min + Vec2::new(30.0, 10.0);
            let frame = |events| {
                let mut clicked = [false; 3];
                let mut menu = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let (buttons, opened) =
                            study_controls(ui, row, controls, egui::Id::unique("study"), true);
                        clicked = buttons.map(|response| response.clicked());
                        menu = opened;
                    },
                );
                let painted = output.shapes.iter().any(|s| match &s.shape {
                    Shape::Rect(r) => r.rect == row,
                    _ => false,
                });
                let focused_label = output.platform_output.events.iter().find_map(|event| {
                    if let egui::output::OutputEvent::FocusGained(info) = event {
                        info.label.clone()
                    } else {
                        None
                    }
                });
                output.drop_without_applying_deltas();
                (painted, clicked, focused_label, menu)
            };
            let touch = |pos, phase| Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(0),
                phase,
                pos,
                force: None,
            };
            let tap = |pos| {
                frame(vec![
                    touch(pos, egui::TouchPhase::Start),
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ]);
                frame(vec![
                    touch(pos, egui::TouchPhase::End),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                    Event::PointerGone,
                ])
            };
            assert!(!frame(vec![Event::PointerGone]).0);
            let first = tap(settings);
            assert_eq!(first.1, [false; 3], "the first tap only reveals controls");
            let second = tap(settings);
            assert_eq!(
                second.1,
                [false, true, false],
                "the second tap opens settings"
            );
            tap(row.left_center() + Vec2::new(10.0, 0.0));
            assert!(frame(vec![]).0, "touch selection must survive pointer exit");
            tap(Pos2::new(250.0, 100.0));
            assert!(!frame(vec![]).0, "outside tap must dismiss controls");
            assert!(
                frame(vec![Event::PointerMoved(row.center())]).0,
                "mouse hover reveals controls"
            );
            // A secondary click on the row opens the menu at the pointer.
            let pos = row.left_center() + Vec2::new(20.0, 0.0);
            frame(vec![Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                let clicked = frame(vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                }]);
                assert_eq!(clicked.3, (!pressed).then_some(pos));
            }
            assert!(!frame(vec![Event::PointerGone]).0);
            for (index, label) in [
                "Study controls",
                "Hide study",
                "Study settings",
                "Remove study",
                "More actions",
            ]
            .into_iter()
            .enumerate()
            {
                let focused = frame(vec![Event::Key {
                    key: Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]);
                assert!(focused.0, "keyboard focus must reveal {label}");
                assert_eq!(focused.2.as_deref(), Some(label));
                assert!(frame(vec![]).0, "focused controls must stay visible");
                let activated = frame(vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]);
                assert_eq!(
                    activated.1,
                    std::array::from_fn(|button| index == button + 1)
                );
                // The menu opens below the more-actions button.
                assert_eq!(
                    activated.3,
                    (index == 4).then(|| controls.min + Vec2::new(60.0, 20.0))
                );
            }

            // A wrapped row is selectable over its text, even when the controls wrap below it.
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let bar = Bar::new(
                Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                100.0,
                110.0,
                90.0,
                105.0,
                1000.0,
            );
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(
                crate::studies::CustomIndicator::new("A long overlay study title", |_| Vec::new())
                    .with_overlay(true),
            )];
            let pos = Pos2::new(130.0, 16.0);
            let frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let mut legend = Legend::new("SAMPLE", "1h", &bar);
                        legend.show_title = false;
                        legend.show_ohlc = false;
                        legend.show_change = false;
                        legend.show_change_percent = false;
                        legend.show_volume = false;
                        legend.studies = &studies;
                        legend.show(ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(160.0, 400.0)));
                    },
                );
                let selected = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape,
                    Shape::Rect(rect) if rect.stroke.width > 0.0 && rect.rect.contains(pos))
                });
                output.drop_without_applying_deltas();
                selected
            };
            frame(vec![]);
            frame(vec![
                touch(pos, egui::TouchPhase::Start),
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            frame(vec![
                touch(pos, egui::TouchPhase::End),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
                Event::PointerGone,
            ]);
            assert!(
                frame(vec![]),
                "the full wrapped study row must respond to a tap"
            );
        }
    }

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
            assert_eq!(buttons.len(), 8);
            for (index, side) in [
                (4, QuoteSide::Sell),
                (5, QuoteSide::Buy),
                (6, QuoteSide::Ticket),
            ] {
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
            for (index, side) in [
                (4, Some(QuoteSide::Sell)),
                (5, Some(QuoteSide::Buy)),
                (6, Some(QuoteSide::Ticket)),
                (7, None),
            ] {
                let mut reached = false;
                for _ in 0..24 {
                    frame(1, vec![key(Key::Tab, true)]);
                    frame(1, vec![key(Key::Tab, false)]);
                    reached = ctx
                        .memory(|memory| memory.focused())
                        .and_then(|id| ctx.read_response(id))
                        .is_some_and(|response| response.rect == buttons[index]);
                    if reached {
                        break;
                    }
                }
                assert!(reached, "legend control must remain keyboard reachable");
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
