//! In-pane series readout and supplementary rows.

mod quotes;
mod study_controls;

pub(crate) use study_controls::{controls_width, study_controls};

use quotes::paint_quotes;

use crate::chart::rendering::StyleColors;
use crate::chart::rendering::axes::semibold;
use crate::localization as l10n;
use crate::model::session::MarketSession;
use crate::model::{Bar, DataStatus, Instrument, Level1Quote, percent_change};
use crate::painting::symbol_mark::paint_symbol_mark;
use crate::scales::price_decimals;
use crate::studies::Indicator;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, ImageSource, Pos2, Rect, Sense, Stroke, Ui, Vec2};
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

/// An action on the series a legend reads, from the controls of its title row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "icons"), allow(dead_code))]
pub(crate) enum SeriesAction {
    /// Open the series' settings in the host.
    Settings,
    /// Show or hide the series.
    ToggleVisibility,
    /// Remove the series.
    Remove,
}

/// An in-pane legend. Historical readout data and current quotes are supplied independently.
pub struct Legend<'a> {
    /// Host number formatting for this series' readouts.
    pub formatters: super::ValueFormatters<'a>,
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
    /// Image for the symbol mark before the title, in place of the instrument's logo.
    pub badge: Option<ImageSource<'a>>,
    /// Show the symbol mark before the title: [`badge`](Self::badge), else the instrument's
    /// [`logo`](Instrument::logo), else the symbol's first letter or digit on a neutral disc.
    pub show_logo: bool,
    /// Show the market status after the title: the session's state, when known, and delayed or
    /// end-of-day data.
    pub show_market_status: bool,
    /// State of the instrument's session at the chart's clock. Only a chart knows its clock, so a
    /// standalone legend shows the data status alone.
    pub(crate) session: Option<MarketSession>,
    /// Show the replay mark after the title in place of the market status, which describes the
    /// present rather than the replayed time.
    pub replaying: bool,
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
    /// Display the title row: the symbol mark, the instrument title and the status marks.
    pub show_title: bool,
    /// Whether the series is shown. A hidden series keeps its title and controls, dimmed, so it
    /// can be shown again.
    pub(crate) series_visible: bool,
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
    /// The previous session's close and the last price, shown after the bar change as the
    /// change between them.
    pub day_change: Option<(f64, f64)>,
    /// Opacity from 0 to 1 of the chart background behind the volume and study rows.
    /// Text and controls retain opaque backing so profiles cannot interfere with them.
    pub study_backdrop: f32,
    /// The order quantity shown in a box under the quote buttons' spread, which the user clicks
    /// to change it.
    pub(crate) quantity: Option<f64>,
    /// Groups the thousands of built-in price values; the chart sets it from
    /// `ChartConfig::thousands_sep`.
    pub(crate) thousands_sep: Option<char>,
    /// The rects the chart shades as extended hours under the legend, left to right, and their
    /// shading. The chart sets it.
    pub(crate) extended_hours: (&'a [Rect], Color32),
    /// The colour of the chart background at a point under the legend, which the backdrops take.
    /// The chart sets it; a legend shown on its own has the theme's chart background.
    pub(crate) background: Option<&'a dyn Fn(Pos2) -> Color32>,
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
            show_logo: true,
            show_market_status: true,
            session: None,
            replaying: false,
            statuses: &[],
            studies: &[],
            study_inputs: &[],
            bar_index: 0,
            show_volume: true,
            show_title: true,
            series_visible: true,
            title_as_description: true,
            show_ohlc: true,
            show_change: true,
            show_change_percent: true,
            show_indicator_titles: true,
            show_indicator_inputs: true,
            show_indicator_values: true,
            day_change: None,
            study_backdrop: 0.6,
            quantity: None,
            thousands_sep: None,
            extended_hours: (&[], Color32::TRANSPARENT),
            background: None,
        }
    }

    /// Paints inside `plot`, inset by the theme's large spacing from its sides and medium
    /// spacing from its top and bottom, and reports a quote intent.
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
        let (quote, study, ..) = self.show_with_colors(ui, plot, &colors, None, f32::INFINITY);
        (quote, study)
    }

    /// Also returns the bottom of the legend and the quantity box, when the user clicked it.
    /// The first row keeps its usual inset from `first_row_end`, where another control starts.
    pub(crate) fn show_with_colors(
        &self,
        ui: &mut Ui,
        plot: Rect,
        colors: &StyleColors,
        study_indices: Option<&[usize]>,
        first_row_end: f32,
    ) -> (
        Option<QuoteSide>,
        Option<StudyAction>,
        f32,
        Option<Rect>,
        Option<SeriesAction>,
    ) {
        let theme = ui.active_theme();
        let bounds = plot.shrink2(Vec2::new(theme.spacing.lg, theme.spacing.md));
        if !bounds.is_positive() {
            return (None, None, plot.top(), None, None);
        }
        #[cfg_attr(not(feature = "icons"), allow(unused_mut))]
        let mut series_action = None;
        let id = ui.next_auto_id().with(("series_legend", self.symbol));
        ui.skip_ahead_auto_ids(1);
        let painter = ui.painter().with_clip_rect(plot);
        let theme_background = |_: Pos2| theme.chart.bg;
        let mut layout = Layout::new(
            &painter,
            bounds,
            colors,
            (first_row_end - theme.spacing.lg).min(bounds.right()),
            self.background.unwrap_or(&theme_background),
            self.extended_hours,
        );
        #[cfg(feature = "icons")]
        let (title_start, title_end) = (layout.pos, layout.row_end);
        if self.show_title {
            let image = self.badge.clone().or_else(|| {
                self.instrument
                    .and_then(|instrument| instrument.logo.as_deref())
                    .map(ImageSource::from)
            });
            let mark_symbol = self
                .instrument
                .map_or(self.symbol, |instrument| instrument.symbol.as_str());
            if self.show_logo && (image.is_some() || mark_symbol.chars().any(char::is_alphanumeric))
            {
                let rect = layout.reserve(Vec2::splat(18.0));
                // The title follows 5 points after the mark, closer than the layout's gap.
                layout.pos.x = rect.right() + 5.0;
                paint_symbol_mark(&painter, rect, mark_symbol, image);
            }
            let title = self.title();
            let small = FontId::proportional(theme.text.small);
            // The market status pill: the session's state, then delayed or end-of-day data.
            let mut pill: Vec<(StatusGlyph, &'static str, Color32, f32)> = Vec::new();
            if self.replaying {
                let ink = theme.ui.accent_text;
                pill.push((StatusGlyph::Replay(ink), "Replay", theme.ui.accent, 20.0));
            } else if self.show_market_status {
                if let Some(session) = self.session {
                    let (label, color) = session_status(session, &theme);
                    pill.push((StatusGlyph::Session(session), label, color, 20.0));
                }
                let data = match self
                    .instrument
                    .and_then(|instrument| instrument.data_status)
                {
                    Some(DataStatus::Delayed) => Some(("D", "Delayed data")),
                    Some(DataStatus::EndOfDay) => Some(("E", "End-of-day data")),
                    _ => None,
                };
                if let Some((text, label)) = data {
                    let galley = painter.layout_no_wrap(
                        l10n::text(ui.ctx(), text),
                        small.clone(),
                        theme.ui.warning,
                    );
                    let width = (galley.size().x + 8.0).max(20.0);
                    pill.push((StatusGlyph::Text(galley), label, theme.ui.warning, width));
                }
            }
            let pill_width: f32 = pill.iter().map(|(.., width)| width).sum();
            let host_width = self
                .statuses
                .iter()
                .map(|status| {
                    painter
                        .layout_no_wrap(status.text.to_owned(), small.clone(), status.color)
                        .size()
                        .x
                        + 16.0
                })
                .sum::<f32>();
            let status_width = if pill.is_empty() {
                0.0
            } else {
                pill_width + 8.0
            } + host_width;
            let title_width = (layout.row_end - layout.pos.x - status_width - 64.0).max(0.0);
            layout.text(
                title,
                theme.text.chart_title,
                if self.series_visible {
                    theme.ui.text
                } else {
                    theme.ui.text_muted
                },
                title_width,
            );
            if !pill.is_empty() {
                let pill_rect = layout.reserve(Vec2::new(pill_width, 18.0));
                let (mut left, last) = (pill_rect.left(), pill.len() - 1);
                for (index, (glyph, label, color, width)) in pill.into_iter().enumerate() {
                    let rect = Rect::from_min_size(
                        Pos2::new(left, pill_rect.top()),
                        Vec2::new(width, 18.0),
                    );
                    left += width;
                    // Opaque, so each glyph's contrast is fixed whatever is drawn beneath.
                    let fill = theme.chart.bg.lerp_to_gamma(color, 0.15);
                    let mut corners = egui::CornerRadius::from(theme.radius.sm);
                    if index > 0 {
                        (corners.nw, corners.sw) = (0, 0);
                    }
                    if index < last {
                        (corners.ne, corners.se) = (0, 0);
                    }
                    // The replay mark is its own disc.
                    if !matches!(glyph, StatusGlyph::Replay(_)) {
                        painter.rect_filled(rect, corners, fill);
                    }
                    paint_status_glyph(&painter, rect.center(), glyph, color, fill);
                    let name = l10n::text(ui.ctx(), label);
                    let response = ui
                        .interact(
                            rect.intersect(bounds),
                            id.with(("market_status", index)),
                            Sense::hover(),
                        )
                        .on_hover_text(name.clone());
                    response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
                }
            }
            for status in self.statuses {
                let galley =
                    painter.layout_no_wrap(status.text.to_owned(), small.clone(), status.color);
                let rect = layout.reserve(Vec2::new(galley.size().x + 8.0, 18.0));
                painter.rect_filled(rect, theme.radius.sm, status.background);
                painter.galley(rect.center() - galley.size() / 2.0, galley, status.color);
            }
        }
        let precision = self.price_precision(self.bar.close);
        if self.show_ohlc {
            if plot.width() < 600.0 {
                layout.field(
                    "C ",
                    self.formatters.price.map_or_else(
                        || {
                            crate::scales::group_thousands(
                                format!("{:.precision$}", self.bar.close),
                                self.thousands_sep,
                            )
                        },
                        |formatter| formatter.format(self.bar.close),
                    ),
                    layout.bar_color(self.bar),
                );
            } else {
                layout.ohlc(
                    self.bar,
                    precision,
                    self.formatters.price,
                    self.thousands_sep,
                );
            }
        }
        // The change from `reference` to `value`, absolute and as a percentage.
        let change_text = |reference: f64, value: f64| {
            let change = value - reference;
            let pct = percent_change(reference, value);
            let absolute = self.formatters.price.map_or_else(
                || {
                    crate::scales::group_thousands(
                        format!("{change:+.precision$}"),
                        self.thousands_sep,
                    )
                },
                |formatter| formatter.format_change(change),
            );
            let percent = self
                .formatters
                .percentage
                .map_or_else(|| format!("{pct:+.2}%"), |formatter| formatter.format(pct));
            (change, absolute, percent)
        };
        if self.show_change || self.show_change_percent {
            let reference = self.previous_close.unwrap_or(self.bar.open);
            let (change, absolute, percent) = change_text(reference, self.bar.close);
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
        if let Some((reference, last)) = self.day_change {
            let (change, absolute, percent) = change_text(reference, last);
            layout.text(
                format!("{absolute} ({percent})"),
                theme.text.ohlc,
                colors.bar_color(change >= 0.0),
                bounds.width(),
            );
        }
        // Values follow the title directly; the controls take the space after them on hover.
        #[cfg(feature = "icons")]
        if self.show_title {
            let size = Vec2::new(116.0, 20.0);
            // On a full row the controls cover its end instead of opening an empty row. They and
            // the outline keep to the end of the title's first row on every row the title spans.
            let controls = if layout.pos.x + size.x > title_end {
                Rect::from_min_size(
                    Pos2::new(
                        (title_end - size.x).max(bounds.left()),
                        layout.pos.y + (layout.height - size.y) / 2.0,
                    ),
                    size,
                )
            } else {
                layout.reserve(size)
            };
            let row_right = if layout.pos.y > title_start.y {
                title_end
            } else {
                controls.right()
            };
            let title_rect =
                Rect::from_min_max(title_start, Pos2::new(row_right, controls.bottom()));
            let hovered = ui.rect_contains_pointer(title_rect);
            let flagged = ui
                .ctx()
                .data(|data| data.get_temp::<bool>(id.with("flag")).unwrap_or(false));
            // Tab reaches the controls through the title, which stays revealed while one of
            // them has focus. It takes focus only, so clicks on it still reach the chart.
            let title_response = ui.interact(
                title_rect,
                id.with("title"),
                Sense::focusable_noninteractive(),
            );
            let controls_label = l10n::text(ui.ctx(), "Series controls");
            title_response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, &controls_label)
            });
            let focused = title_response.has_focus()
                || title_response.lost_focus()
                || ui.memory(|memory| {
                    memory.has_focus(id.with("flag"))
                        || memory.has_focus(id.with("more"))
                        || (0..3_usize)
                            .any(|index| memory.has_focus(id.with(("series_control", index))))
                });
            if hovered
                || flagged
                || focused
                || egui::Popup::is_id_open(ui.ctx(), id.with("more").with("popup"))
            {
                let flag = Rect::from_min_size(controls.min, Vec2::splat(20.0)).intersect(bounds);
                let more =
                    Rect::from_min_size(controls.min + Vec2::new(28.0, 0.0), Vec2::splat(20.0))
                        .intersect(bounds);
                painter.rect_stroke(
                    title_rect.union(controls.expand2(Vec2::new(0.0, 2.0))),
                    theme.radius.sm,
                    Stroke::new(1.0, theme.ui.accent),
                    egui::StrokeKind::Inside,
                );
                painter.add(layout.fill(controls, 1.0));
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
                if flag_response
                    .on_hover_text(l10n::text(ui.ctx(), "Flag instrument"))
                    .clicked()
                {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(id.with("flag"), !flagged));
                }
                let more_response = ui
                    .interact(more, id.with("more"), Sense::click())
                    .on_hover_text(l10n::text(ui.ctx(), "Series actions"));
                crate::icons::icons::UI_MORE_HORIZONTAL
                    .as_image_tinted(Vec2::splat(16.0), theme.ui.text)
                    .paint_at(ui, more.shrink(2.0));
                egui::Popup::menu(&more_response).show(|ui| {
                    if ui.button(l10n::text(ui.ctx(), "Copy symbol")).clicked() {
                        ui.ctx().copy_text(self.symbol.to_owned());
                        ui.close();
                    }
                    if ui.button(l10n::text(ui.ctx(), "Copy values")).clicked() {
                        let values = [
                            self.bar.open,
                            self.bar.high,
                            self.bar.low,
                            self.bar.close,
                            self.bar.volume,
                        ];
                        ui.ctx().copy_text(l10n::format_text(
                            ui.ctx(),
                            "Open {0} High {1} Low {2} Close {3} Vol {4}",
                            &[
                                ("0", values[0].to_string()),
                                ("1", values[1].to_string()),
                                ("2", values[2].to_string()),
                                ("3", values[3].to_string()),
                                ("4", values[4].to_string()),
                            ],
                        ));
                        ui.close();
                    }
                });
                for (index, (action, icon, label)) in [
                    (
                        SeriesAction::ToggleVisibility,
                        if self.series_visible {
                            &crate::icons::icons::EYE
                        } else {
                            &crate::icons::icons::EYE_HIDE
                        },
                        if self.series_visible {
                            "Hide series"
                        } else {
                            "Show series"
                        },
                    ),
                    (
                        SeriesAction::Settings,
                        &crate::icons::icons::SETTINGS,
                        "Series settings",
                    ),
                    (
                        SeriesAction::Remove,
                        &crate::icons::icons::TRASH,
                        "Remove series",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let rect = Rect::from_min_size(
                        controls.min + Vec2::new(56.0 + index as f32 * 20.0, 0.0),
                        Vec2::splat(20.0),
                    )
                    .intersect(bounds);
                    let label = l10n::text(ui.ctx(), label);
                    let response = ui
                        .interact(rect, id.with(("series_control", index)), Sense::click())
                        .on_hover_text(&label);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &label)
                    });
                    if response.hovered() {
                        painter.rect_filled(rect, theme.radius.sm, theme.ui.btn_bg_hover);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    icon.as_image_tinted(Vec2::splat(14.0), theme.ui.text)
                        .paint_at(ui, rect.shrink(3.0));
                    if response.clicked() {
                        series_action = Some(action);
                    }
                }
            }
        }
        if self.show_title
            || self.show_ohlc
            || self.show_change
            || self.show_change_percent
            || self.day_change.is_some()
        {
            layout.new_row();
        }

        let (action, quantity) = self.quote.map_or((None, None), |quote| {
            let (bid, ask) = (quote.bid, quote.ask);
            if !bid.is_finite() || !ask.is_finite() || bid > ask || !quote.spread().is_finite() {
                return (None, None);
            }
            let precision = self.price_precision(bid.abs().max(ask.abs()));
            let action = paint_quotes(
                ui,
                &mut layout,
                bid,
                ask,
                precision,
                self.thousands_sep,
                self.quantity,
                id,
            );
            layout.new_row();
            layout.pos.y += 4.0;
            action
        });

        layout.backdrop = self.study_backdrop.clamp(0.0, 1.0);
        let mut study_settings = None;
        let collapsed = ui
            .ctx()
            .data_mut(|data| data.get_temp::<bool>(id).unwrap_or(false));
        let on_pane = |index: usize| {
            study_indices.map_or_else(
                || self.studies[index].is_overlay(),
                |indices| indices.contains(&index),
            )
        };
        let has_rows = self.show_volume || (0..self.studies.len()).any(on_pane);
        if !collapsed {
            if self.show_volume
                && !self
                    .studies
                    .iter()
                    .enumerate()
                    .any(|(idx, study)| study.is_volume() && on_pane(idx))
            {
                layout.field(
                    &format!("{} ", l10n::text(ui.ctx(), "Vol")),
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
                .filter(|(idx, _)| on_pane(*idx))
            {
                if study.is_volume() && !self.show_volume {
                    continue;
                }
                let (row_start, row_end) = (layout.pos, layout.row_end);
                if self.show_indicator_titles {
                    layout.text(
                        if study.is_volume() {
                            l10n::text(ui.ctx(), "Vol")
                        } else {
                            study.name().to_owned()
                        },
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
                            theme.ui.text_muted,
                            bounds.width(),
                        );
                    }
                    if self.show_indicator_values && study.is_visible() {
                        let up = study.values().value(0, self.bar_index);
                        let line = usize::from(up.is_none());
                        if let Some(value) = up.or_else(|| study.values().value(1, self.bar_index))
                            && study.legend_visible(line)
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
                    for index in study.legend_lines().into_iter().filter(|index| {
                        self.show_indicator_values
                            && study.is_visible()
                            && study.legend_visible(*index)
                    }) {
                        let value = study.plot_value(index, self.bar_index).unwrap_or(f64::NAN);
                        let text = if value.is_finite() {
                            if study.value_format() == crate::scales::StudyFormat::Price {
                                crate::scales::group_thousands(
                                    study.value_format().format(value, precision),
                                    self.thousands_sep,
                                )
                            } else {
                                study.format_plot_value(index, value)
                            }
                        } else {
                            "∅".to_owned()
                        };
                        // A fully transparent plot colour holds no RGB, so it has no opaque form.
                        let color = crate::chart::indicators::plots::plot_value_color(
                            study.as_ref(),
                            index,
                            Some(self.bar_index),
                        )
                        .filter(|color| color.a() != 0)
                        .or_else(|| study_colors.get(index).copied())
                        .filter(|color| color.a() != 0)
                        .map_or(colors.text, |color| color.to_opaque());
                        layout.text(text, theme.text.ohlc, color, bounds.width());
                    }
                }
                // The row and its outline keep to the end of the row it starts on, which is the
                // shortened first row when nothing comes before it. Controls past that end take
                // a row of their own.
                layout.row_end = layout.row_end.min(row_end);
                let controls = layout
                    .reserve(Vec2::new(controls_width(study.as_ref()), 20.0))
                    .intersect(bounds);
                let row_right = if layout.pos.y > row_start.y {
                    row_end
                } else {
                    controls.right()
                };
                let row = Rect::from_min_max(row_start, Pos2::new(row_right, controls.bottom()))
                    .intersect(bounds);
                // The next row's backdrop goes under this row's outline, which reaches into it.
                layout.new_row();
                let (responses, menu) = study_controls(
                    ui,
                    row,
                    controls,
                    None,
                    id.with(("study", index)),
                    study.as_ref(),
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
            }
        }
        if has_rows {
            let rect = layout.reserve(Vec2::new(29.0, 21.0));
            let response = ui.interact(rect.intersect(bounds), id, Sense::click());
            let label = l10n::text(
                ui.ctx(),
                if collapsed {
                    "Expand legend"
                } else {
                    "Collapse legend"
                },
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            painter.rect(
                rect,
                3.0,
                (layout.chart_background)(rect.center()),
                Stroke::new(
                    1.0,
                    if theme.dark {
                        theme.ui.border
                    } else {
                        theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.14)
                    },
                ),
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
            series_action,
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

/// A glyph of the market status pill.
enum StatusGlyph {
    Session(MarketSession),
    Text(Arc<egui::Galley>),
    /// A rewind mark in this ink on a disc of the segment's colour.
    Replay(Color32),
}

/// The tooltip and accessible name of a session state, and the colour of its glyph.
fn session_status(session: MarketSession, theme: &crate::theme::Theme) -> (&'static str, Color32) {
    match session {
        MarketSession::Open => ("Market open", theme.ui.success),
        MarketSession::PreMarket => ("Pre-market", theme.ui.warning),
        MarketSession::PostMarket => ("Post-market", theme.ui.info),
        MarketSession::Closed => ("Market closed", theme.ui.text_muted),
    }
}

/// Paints `glyph` centred on `center` in `color`, over a segment filled with `fill`.
fn paint_status_glyph(
    painter: &egui::Painter,
    center: Pos2,
    glyph: StatusGlyph,
    color: Color32,
    fill: Color32,
) {
    match glyph {
        StatusGlyph::Session(MarketSession::Open) => {
            painter.circle_filled(center, 4.0, color);
        }
        StatusGlyph::Session(MarketSession::PreMarket) => {
            // A rising sun: a core and eight short rays.
            painter.circle_filled(center, 2.0, color);
            for ray in 0..8 {
                let direction = Vec2::angled(ray as f32 * std::f32::consts::TAU / 8.0);
                painter.line_segment(
                    [center + direction * 3.5, center + direction * 5.0],
                    Stroke::new(1.2, color),
                );
            }
        }
        StatusGlyph::Session(MarketSession::PostMarket) => {
            // A crescent: a disc with an offset disc of the segment's fill cut out of it.
            painter.circle_filled(center, 4.0, color);
            painter.circle_filled(center + Vec2::new(2.0, -1.5), 3.5, fill);
        }
        StatusGlyph::Session(MarketSession::Closed) => {
            painter.circle_stroke(center, 3.5, Stroke::new(1.5, color));
        }
        StatusGlyph::Text(galley) => {
            painter.galley(center - galley.size() / 2.0, galley, color);
        }
        StatusGlyph::Replay(ink) => {
            painter.circle_filled(center, 8.0, color);
            for tip in [-4.5, 0.0] {
                let points = [(tip, 0.0), (tip + 4.5, -3.5), (tip + 4.5, 3.5)]
                    .map(|(x, y)| center + Vec2::new(x, y))
                    .to_vec();
                painter.add(egui::Shape::convex_polygon(points, ink, Stroke::NONE));
            }
        }
    }
}

struct Layout<'a> {
    painter: &'a egui::Painter,
    bounds: Rect,
    pos: Pos2,
    /// Where the current row ends: `bounds` less what the first row leaves to another control.
    row_end: f32,
    height: f32,
    background: egui::layers::ShapeIdx,
    /// The colour of the chart background at a point.
    chart_background: &'a dyn Fn(Pos2) -> Color32,
    /// Opacity of the current row's background.
    backdrop: f32,
    /// The rects shaded as extended hours and their shading.
    tint: (&'a [Rect], Color32),
    colors: &'a StyleColors,
}

impl<'a> Layout<'a> {
    fn new(
        painter: &'a egui::Painter,
        bounds: Rect,
        colors: &'a StyleColors,
        first_row_end: f32,
        chart_background: &'a dyn Fn(Pos2) -> Color32,
        tint: (&'a [Rect], Color32),
    ) -> Self {
        Self {
            painter,
            bounds,
            pos: bounds.min,
            row_end: first_row_end,
            height: 24.0,
            background: painter.add(egui::Shape::Noop),
            chart_background,
            backdrop: 1.0,
            tint,
            colors,
        }
    }

    /// `rect` in the chart background behind it at `opacity`, and over the extended hours shading
    /// in the shaded background at the same opacity, so each part matches the plot beneath.
    fn fill(&self, rect: Rect, opacity: f32) -> egui::Shape {
        let (shaded, shade) = self.tint;
        let part = |part: Rect, shaded: bool| {
            // shortcut: one colour per part, so a steep horizontal gradient shows a step at the
            // ends of a wide row; paint the part as a slice of the gradient if that is seen.
            let color = (self.chart_background)(part.center());
            let color = if shaded { color.blend(shade) } else { color };
            egui::Shape::rect_filled(part, 0.0, color.gamma_multiply(opacity))
        };
        let mut parts = Vec::new();
        let mut left = rect.left();
        // The parts are disjoint: a shaded part laid over a translucent fill would apply the
        // opacity twice.
        for shaded in shaded
            .iter()
            .map(|shaded| rect.intersect(*shaded))
            .filter(Rect::is_positive)
        {
            parts.push((
                Rect::from_x_y_ranges(left..=shaded.left(), rect.y_range()),
                false,
            ));
            parts.push((shaded, true));
            left = shaded.right();
        }
        if parts.is_empty() {
            return part(rect, false);
        }
        parts.push((
            Rect::from_x_y_ranges(left..=rect.right(), rect.y_range()),
            false,
        ));
        egui::Shape::Vec(
            parts
                .into_iter()
                .filter(|(rect, _)| rect.is_positive())
                .map(|(rect, shaded)| part(rect, shaded))
                .collect(),
        )
    }

    fn new_row(&mut self) {
        self.pos = Pos2::new(self.bounds.left(), self.pos.y + self.height);
        self.row_end = self.bounds.right();
        self.height = 22.0;
        self.background = self.painter.add(egui::Shape::Noop);
    }

    fn reserve_space(&mut self, size: Vec2) -> Rect {
        // A shortened first row gives way even to its first item; a full row has nowhere better.
        if (self.pos.x > self.bounds.left() || self.row_end < self.bounds.right())
            && self.pos.x + size.x > self.row_end
        {
            self.new_row();
        }
        self.height = self.height.max(size.y);
        let rect = Rect::from_min_size(
            Pos2::new(self.pos.x, self.pos.y + (self.height - size.y) / 2.0),
            size,
        );
        self.pos.x += size.x + 8.0;
        rect
    }

    fn reserve(&mut self, size: Vec2) -> Rect {
        let rect = self.reserve_space(size);
        self.painter.set(
            self.background,
            self.fill(
                Rect::from_min_max(
                    Pos2::new(self.bounds.left() - 2.0, self.pos.y),
                    Pos2::new(rect.right() + 4.0, self.pos.y + self.height),
                )
                .intersect(self.bounds.expand(2.0)),
                self.backdrop,
            ),
        );
        let background = (self.chart_background)(rect.center());
        if background.gamma_multiply(self.backdrop) != background {
            // Profiles can be opaque or contain letters; protect the full content bounds.
            self.painter
                .add(self.fill(rect.expand(2.0).intersect(self.bounds.expand(2.0)), 1.0));
        }
        rect
    }

    fn galley(&mut self, galley: Arc<egui::Galley>) {
        let rect = self.reserve(galley.size());
        self.painter.galley(rect.min, galley, self.colors.text);
    }

    fn text(&mut self, text: String, size: f32, color: Color32, max_width: f32) {
        if text.is_empty() {
            return;
        }
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
        thousands_sep: Option<char>,
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
                    || {
                        crate::scales::group_thousands(
                            format!("{value:.precision$}"),
                            thousands_sep,
                        )
                    },
                    |formatter| formatter.format(value),
                ),
                color,
            );
        }
    }
}

fn format_volume(volume: f64) -> String {
    for (divisor, suffix) in [(1_000_000_000.0, "B"), (1_000_000.0, "M"), (1_000.0, "K")] {
        if volume.abs() >= divisor {
            return format!("{:.2}{suffix}", volume / divisor);
        }
    }
    format!("{volume:.0}")
}

#[cfg(test)]
mod mark_tests {
    use super::*;

    /// Session and data glyphs have distinct shapes, readable contrast and accessible names.
    /// The status toggle leaves host marks and the logo independent.
    #[test]
    fn legend_names_the_market_state_and_hides_it_independently() {
        use crate::theme::contrast;
        use MarketSession::{Closed, Open, PostMarket, PreMarket};
        use egui::Shape;
        for (_, theme) in crate::theme::presets() {
            for (session, data, shown, labels) in [
                (Some(Open), None, true, &["Market open"][..]),
                (
                    Some(PreMarket),
                    Some(DataStatus::Delayed),
                    true,
                    &["Pre-market", "Delayed data"][..],
                ),
                (
                    Some(PostMarket),
                    Some(DataStatus::EndOfDay),
                    true,
                    &["Post-market", "End-of-day data"][..],
                ),
                (Some(Closed), None, true, &["Market closed"][..]),
                (Some(Open), Some(DataStatus::Delayed), false, &[][..]),
                (None, Some(DataStatus::Delayed), true, &["Delayed data"][..]),
                (None, Some(DataStatus::Realtime), true, &[][..]),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                ctx.enable_accesskit();
                let mut instrument = Instrument::new("ABC");
                instrument.data_status = data;
                let bar = Bar::new(chrono::Utc::now(), 1.0, 2.0, 0.0, 1.0, 0.0);
                let host = [LegendStatus {
                    text: "HALT",
                    color: theme.ui.text,
                    background: theme.ui.panel_bg_secondary,
                }];
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    let mut legend = Legend::new("ABC", "1h", &bar);
                    legend.instrument = Some(&instrument);
                    legend.session = session;
                    legend.show_market_status = shown;
                    legend.statuses = &host;
                    legend.show(ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0)));
                });
                output.textures_delta.clear();
                let update = output.platform_output.accesskit_update.as_ref().unwrap();
                let mut nodes: Vec<_> = update
                    .nodes
                    .iter()
                    .map(|(_, node)| node)
                    .filter(|node| node.role() == egui::Role::Image)
                    .collect();
                nodes.sort_by(|a, b| a.bounds().unwrap().x0.total_cmp(&b.bounds().unwrap().x0));
                assert_eq!(
                    nodes
                        .iter()
                        .map(|node| node.label().unwrap())
                        .collect::<Vec<_>>(),
                    labels
                );
                let title = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.text() == "ABC · 1h" => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let mut left = title.pos.x + title.galley.size().x + 8.0;
                for (index, node) in nodes.iter().enumerate() {
                    let bounds = node.bounds().unwrap();
                    let rect = Rect::from_min_size(egui::pos2(left, 9.0), Vec2::new(20.0, 18.0));
                    assert_eq!(
                        (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
                        (
                            rect.left() as f64,
                            rect.top() as f64,
                            rect.right() as f64,
                            rect.bottom() as f64
                        )
                    );
                    let fill = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Rect(segment) if segment.rect == rect => Some(segment.fill),
                            _ => None,
                        })
                        .expect("pill segment");
                    let color = match labels[index] {
                        "Market open" => theme.ui.success,
                        "Pre-market" | "Delayed data" | "End-of-day data" => theme.ui.warning,
                        "Post-market" => theme.ui.info,
                        "Market closed" => theme.ui.text_muted,
                        _ => unreachable!(),
                    };
                    let letter = labels[index].ends_with("data");
                    assert!(contrast(color, fill) >= if letter { 4.5 } else { 3.0 });
                    let glyph = output.shapes.iter().any(|shape| match &shape.shape {
                        Shape::Circle(circle) if rect.contains(circle.center) => {
                            if labels[index] == "Market closed" {
                                circle.stroke.color == color && circle.stroke.width == 1.5
                            } else {
                                circle.fill == color
                                    && circle.radius
                                        == if labels[index] == "Pre-market" {
                                            2.0
                                        } else {
                                            4.0
                                        }
                            }
                        }
                        Shape::Text(text) if letter => {
                            text.fallback_color == color
                                && rect.contains_rect(Rect::from_min_size(
                                    text.pos,
                                    text.galley.size(),
                                ))
                        }
                        _ => false,
                    });
                    assert!(glyph, "{} glyph", labels[index]);
                    left += 20.0;
                }
                for label in ["A", "HALT"] {
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == label)));
                }
                if !shown {
                    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == "D")));
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// The mark uses the host override, then the instrument logo, then an initial, and the
    /// title keeps its five-point gap. Hiding the title or logo removes the mark.
    #[test]
    fn legend_marks_the_symbol_before_its_title() {
        for (_, theme) in crate::theme::presets() {
            for (badge, logo, shown, title, symbol) in [
                (true, false, true, true, "ABC"),
                (true, true, true, true, "ABC"),
                (false, true, true, true, "ABC"),
                (false, false, true, true, "ABC"),
                (true, true, false, true, "ABC"),
                (false, false, true, false, "ABC"),
                (false, false, true, true, ""),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let texture = ctx.load_texture(
                    "badge",
                    egui::ColorImage::filled([18, 18], Color32::BLUE),
                    Default::default(),
                );
                // Without image loaders the instrument's logo stays unreadable.
                #[cfg(feature = "ui")]
                {
                    egui_extras::install_image_loaders(&ctx);
                    ctx.include_bytes("bytes://logo.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18"><circle cx="9" cy="9" r="9" fill="blue"/></svg>"#);
                }
                let logo_ready = logo && cfg!(feature = "ui");
                let mut instrument = Instrument::new(symbol);
                if logo {
                    instrument = instrument.with_logo("bytes://logo.svg");
                }
                let bar = Bar::new(chrono::Utc::now(), 1.0, 2.0, 0.0, 1.0, 0.0);
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    let mut legend = Legend::new(symbol, "1h", &bar);
                    legend.instrument = Some(&instrument);
                    legend.badge = badge
                        .then(|| ImageSource::Texture((texture.id(), Vec2::splat(18.0)).into()));
                    legend.show_logo = shown;
                    legend.show_title = title;
                    legend.show(ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0)));
                });
                output.textures_delta.clear();
                let mark = shown && title && (badge || !symbol.is_empty());
                let images: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.fill_texture_id() != egui::TextureId::default() =>
                        {
                            Some(rect.fill_texture_id() == texture.id())
                        }
                        _ => None,
                    })
                    .collect();
                // One image, the badge's when supplied, else the instrument's ready logo.
                let image = (mark && (badge || logo_ready)).then_some(badge);
                assert_eq!(images, Vec::from_iter(image));
                let letters = output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == "A")
                    })
                    .count();
                assert_eq!(letters, usize::from(mark && !badge && !logo_ready));
                if title {
                    let label = if symbol.is_empty() { "1h" } else { "ABC · 1h" };
                    let text = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                            _ => None,
                        })
                        .expect("title");
                    assert_eq!(text.pos.x, theme.spacing.lg + if mark { 23.0 } else { 0.0 });
                }
                output.drop_without_applying_deltas();
            }
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    #[cfg(feature = "icons")]
    use super::*;
    #[cfg(feature = "icons")]
    use chrono::{TimeZone, Utc};
    #[cfg(feature = "icons")]
    use egui::{Context, Shape};

    /// The title's hide, settings and remove buttons are keyboard reachable and report their
    /// action, and a hidden series keeps its title and buttons so it can be shown again.
    #[cfg(feature = "icons")]
    #[test]
    fn title_controls_report_series_actions_for_a_hidden_series_too() {
        use egui::{Event, Key};
        let bar = Bar::new(
            Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            100.0,
            110.0,
            90.0,
            95.0,
            1.0,
        );
        let colors = StyleColors {
            bullish: Color32::GREEN,
            bearish: Color32::RED,
            text: Color32::WHITE,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: 0.0,
        };
        for series_visible in [true, false] {
            let ctx = Context::default();
            let frame = |events| {
                let mut action = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let mut legend = Legend::new("SAMPLE", "1h", &bar);
                        legend.series_visible = series_visible;
                        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0));
                        action = legend
                            .show_with_colors(ui, plot, &colors, None, f32::INFINITY)
                            .4;
                    },
                );
                let title = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, Shape::Text(text) if text.galley.text() == "SAMPLE · 1h")
                });
                output.drop_without_applying_deltas();
                (action, title)
            };
            let key = |pressed| Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            };
            assert!(
                frame(vec![]).1,
                "the title stays while the series is hidden"
            );
            // Tab stops: title, flag, more, then the series buttons.
            for stop in 0..3 {
                frame(vec![key(true)]);
                frame(vec![key(false)]);
                assert_eq!(frame(vec![]).0, None, "stop {stop}");
            }
            for expected in [
                SeriesAction::ToggleVisibility,
                SeriesAction::Settings,
                SeriesAction::Remove,
            ] {
                frame(vec![key(true)]);
                frame(vec![key(false)]);
                let enter = |pressed| Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Default::default(),
                };
                let (action, _) = frame(vec![enter(true)]);
                frame(vec![enter(false)]);
                assert_eq!(action, Some(expected), "visible: {series_visible}");
            }
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use crate::model::Bar;
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Pos2, Rect, Shape};

    /// Profiles painted first cannot bleed through volume, study text or reserved controls.
    #[cfg(feature = "studies")]
    #[test]
    fn supplementary_rows_mask_profiles_over_their_content_and_controls() {
        use crate::chart::rendering::Legend;
        use crate::studies::{CustomIndicator, IndicatorValue};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut study =
                CustomIndicator::new("Profile readout", |_| vec![IndicatorValue::Single(120.0)])
                    .with_overlay(true);
            crate::studies::Indicator::calculate(&mut study, &[]);
            let studies: Vec<Box<dyn crate::studies::Indicator>> = vec![Box::new(study)];
            let bar = Bar::new(Utc.timestamp_opt(0, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 240.0);
            let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(2);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut legend = Legend::new("SAMPLE", "1m", &bar);
                legend.studies = &studies;
                legend.instrument = Some(&instrument);
                legend.show(
                    ui,
                    Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)),
                );
            });
            output.textures_delta.clear();
            for (at, shape) in output.shapes.iter().enumerate() {
                if let Shape::Text(text) = &shape.shape {
                    let bounds = Rect::from_min_size(text.pos, text.galley.size());
                    assert!(output.shapes[..at].iter().any(|shape| matches!(&shape.shape,
                        Shape::Rect(rect) if rect.fill == theme.chart.bg && rect.rect.contains_rect(bounds)
                    )), "unprotected {}", text.galley.text());
                    if text.galley.text().starts_with("Vol ")
                        || text.galley.text() == "Profile readout"
                        || text.galley.text() == "120.00"
                    {
                        assert!(output.shapes[..at].iter().any(|shape| matches!(&shape.shape,
                            Shape::Rect(rect) if rect.fill == theme.chart.bg && rect.rect.contains_rect(bounds.expand(2.0))
                        )), "unprotected text padding");
                    }
                    if text.galley.text() == "120.00" {
                        let controls = Rect::from_min_size(
                            Pos2::new(bounds.right() + 8.0, bounds.center().y - 10.0),
                            egui::vec2(80.0, 20.0),
                        );
                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            Shape::Rect(rect) if rect.fill == theme.chart.bg && rect.rect.contains_rect(controls)
                        )), "unprotected study controls");
                    }
                }
            }
            output.drop_without_applying_deltas();
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn overlay_values_stay_inside_the_plot_when_rows_wrap() {
        use crate::chart::rendering::Legend;
        use crate::studies::{CustomIndicator, Indicator, IndicatorValue};
        let mut study = CustomIndicator::new("Levels", |_| {
            vec![IndicatorValue::Multiple(vec![1.0, 2.0, 3.0])]
        })
        .with_overlay(true)
        .with_line_count(3)
        .with_line_names(vec![
            "Ask levels".into(),
            "Bid levels".into(),
            "Total levels".into(),
        ]);
        study.calculate(&[]);
        let studies: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
        let ctx = Context::default();
        let bar = Bar::new(Utc.timestamp_opt(0, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 0.0);
        let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(2);
        for width in [320.0, 800.0, 1440.0] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut legend = Legend::new("SAMPLE", "1m", &bar);
                legend.studies = &studies;
                legend.instrument = Some(&instrument);
                legend.show_indicator_inputs = false;
                legend.show(
                    ui,
                    Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 400.0)),
                );
            });
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let Shape::Text(text) = &shape.shape {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect();
            for expected in ["1.00", "2.00", "3.00"] {
                let text = texts
                    .iter()
                    .find(|text| text.galley.text() == expected)
                    .expect(expected);
                assert!(text.pos.x + text.galley.size().x <= width);
            }
            output.drop_without_applying_deltas();
        }
    }

    /// Legend fields stay inside the plot and keep their positions during hover.
    #[test]
    fn legend_fields_stay_inside_the_plot() {
        use crate::chart::rendering::{Legend, LegendStatus};
        use crate::theme::apply_to_egui;
        use egui::Vec2;

        for (_, theme) in crate::theme::presets() {
            for width in [160.0, 300.0, 387.0, 400.0, 599.0, 600.0, 1200.0, 1440.0] {
                let ctx = Context::default();
                #[cfg(feature = "icons")]
                crate::icons::install_loader(&ctx);
                apply_to_egui(&ctx, &theme);
                let plot = Rect::from_min_size(Pos2::new(52.0, 40.0), Vec2::new(width, 400.0));
                let badge = ctx.load_texture(
                    "instrument",
                    egui::ColorImage::filled([18, 18], egui::Color32::BLUE),
                    Default::default(),
                );
                let statuses = [LegendStatus {
                    text: "S",
                    color: theme.ui.text,
                    background: theme.ui.panel_bg_secondary,
                }];
                let instrument = crate::model::Instrument::new("Sample equity fund")
                    .with_data_status(crate::model::DataStatus::Delayed);
                let bar = Bar::new(
                    Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    749.37,
                    751.09,
                    749.13,
                    750.74,
                    275_540.0,
                );
                let frame = |events| {
                    ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let mut legend = Legend::new("Sample equity fund", "1h", &bar);
                            legend.previous_close = Some(749.42);
                            legend.badge = Some(egui::ImageSource::Texture(
                                (badge.id(), Vec2::splat(18.0)).into(),
                            ));
                            legend.session = Some(crate::model::session::MarketSession::PostMarket);
                            legend.statuses = &statuses;
                            legend.instrument = Some(&instrument);
                            legend.show_volume = false;
                            legend.show(ui, plot);
                        },
                    )
                };
                let mut output = frame(vec![Event::PointerGone]);
                output.textures_delta.clear();
                let text: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect();
                assert!(!text.is_empty());
                let marks: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Rect(rect) if rect.fill_texture_id() == badge.id() => {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(marks.len(), 1);
                let mark = marks[0];
                assert!(
                    plot.shrink2(Vec2::new(theme.spacing.lg, theme.spacing.md))
                        .contains_rect(mark)
                );
                for field in &text {
                    assert!(!mark.intersects(Rect::from_min_size(field.pos, field.galley.size())));
                }
                for (index, field) in text.iter().enumerate() {
                    let bounds = Rect::from_min_size(field.pos, field.galley.size());
                    for other in &text[index + 1..] {
                        assert!(
                            !bounds.intersects(Rect::from_min_size(other.pos, other.galley.size()))
                        );
                    }
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            Shape::Rect(rect) if rect.fill == theme.chart.bg && rect.rect.height() >= 22.0 && rect.rect.contains_rect(bounds)
                        )), "missing row backdrop for {}", field.galley.text());
                    assert!(
                        bounds.right() <= plot.right() - 8.0 + 0.1,
                        "{width}: {} at {bounds:?}",
                        field.galley.text()
                    );
                }
                for shape in &output.shapes {
                    if let Shape::Rect(rect) = &shape.shape
                        && rect.fill == theme.chart.bg
                        && rect.rect.height() >= 22.0
                    {
                        assert!(
                            text.iter().any(|field| rect
                                .rect
                                .intersects(Rect::from_min_size(field.pos, field.galley.size()))),
                            "{width}: empty legend row at {:?}",
                            rect.rect
                        );
                    }
                }
                for (index, field) in ["O 749.37", "H 751.09", "L 749.13", "C 750.74"]
                    .into_iter()
                    .enumerate()
                {
                    assert_eq!(
                        text.iter().any(|text| text.galley.text() == field),
                        width >= 600.0 || index == 3,
                        "{width}: field visibility for {field}"
                    );
                }
                assert!(
                    text.iter()
                        .any(|text| text.galley.text() == "+1.32 (+0.18%)")
                );
                assert!(text.iter().any(|text| text.galley.text() == "D"
                    && text.galley.job.sections[0].format.color == theme.ui.warning));
                let centres: Vec<_> = text
                    .iter()
                    .map(|text| text.pos.y + text.galley.size().y / 2.0)
                    .collect();
                if width >= 1200.0 {
                    assert!(centres.iter().all(|y| (*y - centres[0]).abs() < 0.1));
                } else if width <= 300.0 {
                    assert!(centres.iter().any(|y| *y > centres[0]));
                }
                for field in text
                    .iter()
                    .filter(|text| text.galley.text().starts_with("O "))
                {
                    let sections = &field.galley.job.sections;
                    assert_eq!(sections[0].format.color, theme.ui.text_secondary);
                    assert_eq!(sections[1].format.color, theme.chart.candle_up);
                }
                let idle: Vec<_> = text
                    .iter()
                    .map(|text| (text.galley.text().to_owned(), text.pos, text.galley.size()))
                    .collect();
                let hovered = frame(vec![Event::PointerMoved(plot.min + Vec2::new(60.0, 20.0))]);
                let hovered_text: Vec<_> = hovered
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => {
                            Some((text.galley.text().to_owned(), text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .collect();
                hovered.drop_without_applying_deltas();
                output.drop_without_applying_deltas();
                assert_eq!(idle, hovered_text, "hover moved readout at {width}");
            }
        }
    }

    /// A value in the price pane's legend takes the colour its plot shows for that value: a pivot
    /// level keeps its colour on the first bar of a period, where only the segment from the
    /// previous period is hidden, and a plot drawn one bar to the right of its source bar takes
    /// the colour of the source bar its value comes from (`Indicator::plot_color`).
    #[cfg(feature = "studies")]
    #[test]
    fn price_legend_colours_each_value_as_its_plot_shows_it() {
        use crate::chart::rendering::Legend;
        use crate::model::Timeframe;
        use crate::studies::{
            CustomIndicator, Indicator, IndicatorValue, PivotPoints, PivotTimeframe, palette,
        };
        use egui::Color32;
        // Hourly bars from Monday 5 January 2026, UTC; Tuesday trades 10 higher.
        let bars: Vec<Bar> = (0..72)
            .map(|i| {
                let c = 100.0 + if (24..48).contains(&i) { 10.0 } else { 0.0 } + (i % 5) as f64;
                let time = Utc.timestamp_opt(1_767_571_200 + i * 3600, 0).unwrap();
                Bar::new(time, c, c + 1.0, c - 1.0, c, 1.0)
            })
            .collect();
        let mut pivots = PivotPoints::new().with_timeframe(PivotTimeframe::Daily);
        pivots.set_timeframe(Timeframe::Hour1);
        pivots.calculate(&bars);
        // Wednesday's first bar, 48, starts the levels made from Tuesday.
        assert_ne!(pivots.values().value(0, 47), pivots.values().value(0, 48));
        let mut shifted = CustomIndicator::new("Shifted", |bars| {
            (0..bars.len())
                .map(|i| IndicatorValue::Single(10.0 + i as f64))
                .collect()
        })
        .with_offsets(vec![1])
        .with_plot_color(|_, bar, _| Some([Color32::RED, Color32::BLUE][bar % 2]));
        shifted.calculate(&bars);
        let ctx = Context::default();
        let instrument = crate::model::Instrument::new("SAMPLE").with_price_decimals(2);
        let shown = |study: Box<dyn Indicator>, bar: usize| -> Vec<(f64, Color32)> {
            let studies = vec![study];
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut legend = Legend::new("SAMPLE", "1h", &bars[bar]);
                legend.studies = &studies;
                legend.instrument = Some(&instrument);
                legend.show_indicator_inputs = false;
                legend.show_volume = false;
                legend.show_ohlc = false;
                legend.show_change = false;
                legend.show_change_percent = false;
                legend.bar_index = bar;
                legend.show(
                    ui,
                    Rect::from_min_size(Pos2::ZERO, egui::vec2(3000.0, 400.0)),
                );
            });
            output.textures_delta.clear();
            let values = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) => {
                        let value = text.galley.text().replace(',', "").parse::<f64>().ok()?;
                        Some((value, text.galley.job.sections[0].format.color))
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            values
        };
        let levels: Vec<Color32> = shown(Box::new(pivots), 48)
            .into_iter()
            .map(|(_, color)| color)
            .collect();
        // Bar 5 shows source bar 4, coloured red.
        let got = (levels, shown(Box::new(shifted), 5));
        assert_eq!(
            got,
            (vec![palette::DARK_ORANGE; 11], vec![(14.0, Color32::RED)]),
            "{got:?}"
        );
    }

    /// Each part of a row's backdrop takes the background as shaded beneath it, opaque and at the
    /// study opacity, so a row crossing the edge of the extended hours shading shows no box on
    /// either side of it. The shaded colours are worked out by hand: the background times
    /// (255 - 40) / 255 plus the tint, then 153 / 255 for the 60% study rows.
    #[test]
    fn legend_rows_take_the_extended_hours_tint_beneath_them() {
        use crate::chart::rendering::Legend;
        use egui::Color32;
        fn unstroked(shape: &Shape, rects: &mut Vec<(Rect, Color32)>) {
            match shape {
                Shape::Rect(rect) if rect.stroke.width == 0.0 => rects.push((rect.rect, rect.fill)),
                Shape::Vec(shapes) => shapes.iter().for_each(|shape| unstroked(shape, rects)),
                _ => {}
            }
        }
        for (theme, plain, plain_study, tinted, tinted_study) in [
            (
                crate::theme::Theme::light(),
                Color32::WHITE,
                Color32::from_rgba_premultiplied(153, 153, 153, 153),
                Color32::from_rgb(215, 215, 255),
                Color32::from_rgba_premultiplied(129, 129, 153, 153),
            ),
            (
                crate::theme::Theme::dark(),
                Color32::BLACK,
                Color32::from_rgba_premultiplied(0, 0, 0, 153),
                Color32::from_rgb(0, 0, 40),
                Color32::from_rgba_premultiplied(0, 0, 24, 153),
            ),
        ] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let bar = Bar::new(Utc.timestamp_opt(0, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 240.0);
            let shaded = [Rect::from_min_max(Pos2::ZERO, Pos2::new(30.0, 600.0))];
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut legend = Legend::new("SAMPLE", "1m", &bar);
                legend.extended_hours = (&shaded, Color32::from_rgba_premultiplied(0, 0, 40, 40));
                legend.show(
                    ui,
                    Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)),
                );
            });
            output.textures_delta.clear();
            let mut rects = Vec::new();
            for shape in &output.shapes {
                unstroked(&shape.shape, &mut rects);
            }
            output.drop_without_applying_deltas();
            let filled = |fill: Color32| -> Vec<Rect> {
                rects
                    .iter()
                    .filter(|(_, color)| *color == fill)
                    .map(|(rect, _)| *rect)
                    .collect()
            };
            for (plain, tinted) in [(plain, tinted), (plain_study, tinted_study)] {
                let (plain, tinted) = (filled(plain), filled(tinted));
                for rect in &plain {
                    assert!(
                        rect.left() >= 30.0,
                        "plain backdrop under the tint at {rect:?}"
                    );
                }
                for rect in &tinted {
                    assert!(
                        rect.right() <= 30.0,
                        "tinted backdrop past the tint at {rect:?}"
                    );
                }
                assert!(
                    tinted.iter().any(|tinted| tinted.right() == 30.0
                        && plain.iter().any(
                            |plain| plain.left() == 30.0 && plain.y_range() == tinted.y_range()
                        )),
                    "no row meets at the tint's edge: {plain:?} {tinted:?}"
                );
            }
        }
    }
}
