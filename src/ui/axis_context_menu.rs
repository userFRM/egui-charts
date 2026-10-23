//! Price and time axis menus used by the chart's turnkey menus.

use super::context_menu::{ContextMenuAction, MenuEntry, MenuItem, POINTER_GAP, show_menu};
use crate::icons::icons;
use crate::scales::{PriceScaleMode, TimezoneMode};
use crate::theme::ThemeExt;
use crate::widget::Chart;
use egui::{Context, Id, Pos2, Rect};

#[derive(Clone)]
enum Action {
    Auto,
    Mode(PriceScaleMode),
    Invert,
    LockRatio,
    PlaceLeft(bool),
    LastValue,
    PreviousClose,
    Studies,
    Countdown,
    BidAsk,
    HighLow,
    Timezone(TimezoneMode),
    ExtendedHours,
    SessionBreaks,
    Settings,
}

#[derive(Default)]
pub(super) struct AxisMenus {
    open: Option<(bool, Pos2)>,
    just_opened: bool,
}

impl AxisMenus {
    pub(super) fn close(&mut self) {
        self.open = None;
    }

    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(super) fn open_from_pointer(&mut self, ctx: &Context, chart: &Chart) -> bool {
        let Some(pos) = ctx.input(|input| {
            input
                .pointer
                .secondary_clicked()
                .then(|| input.pointer.interact_pos())
                .flatten()
        }) else {
            return false;
        };
        let plot = chart.get_rendered_price_rect();
        let config = chart.config();
        let bottom = chart
            .get_rendered_indicator_panes()
            .last()
            .map_or(plot.bottom(), |pane| pane.panel_rect.bottom());
        let price = (config.show_right_axis
            && Rect::from_min_max(
                Pos2::new(plot.right(), plot.top()),
                Pos2::new(
                    plot.right() + config.right_axis_width.max(chart.measured_axis_width),
                    bottom,
                ),
            )
            .contains(pos))
            || (config.show_left_axis
                && Rect::from_min_max(
                    Pos2::new(
                        plot.left() - config.left_axis_width.max(chart.measured_axis_width),
                        plot.top(),
                    ),
                    Pos2::new(plot.left(), bottom),
                )
                .contains(pos));
        let time = config.show_time_labels
            && Rect::from_min_max(
                Pos2::new(plot.left(), bottom),
                Pos2::new(plot.right(), bottom + 28.0),
            )
            .contains(pos);
        if price || time {
            self.open = Some((time, pos));
            self.just_opened = true;
            true
        } else {
            false
        }
    }

    fn entries(time: bool, chart: &Chart) -> Vec<MenuEntry<Action>> {
        let row = |label: &str, action, checked| {
            let mut item = MenuItem::new(label, action);
            if checked {
                item = item.with_icon(&icons::CHECK);
            }
            MenuEntry::Item(item)
        };
        let config = chart.config();
        if time {
            let timezone = &chart.chart_options.time_scale.timezone;
            let exchange = TimezoneMode::Exchange(chrono_tz::UTC);
            let mut entries = vec![
                row(
                    "UTC",
                    Action::Timezone(TimezoneMode::Utc),
                    *timezone == TimezoneMode::Utc,
                ),
                row(
                    "Local time",
                    Action::Timezone(TimezoneMode::Local),
                    *timezone == TimezoneMode::Local,
                ),
                row(
                    "Exchange time",
                    Action::Timezone(exchange),
                    matches!(timezone, TimezoneMode::Exchange(_)),
                ),
            ];
            entries.extend([
                MenuEntry::Separator,
                row(
                    "Extended hours",
                    Action::ExtendedHours,
                    chart.extended_hours(),
                ),
                row(
                    "Session breaks",
                    Action::SessionBreaks,
                    config.show_session_breaks,
                ),
                MenuEntry::Separator,
                row("Time zone and session settings…", Action::Settings, false),
            ]);
            entries
        } else {
            let scale = chart.state().price_scale();
            let mut entries = vec![
                row("Auto scale", Action::Auto, !scale.is_manual()),
                MenuEntry::Separator,
            ];
            for (label, mode) in [
                ("Regular", PriceScaleMode::Normal),
                ("Percent", PriceScaleMode::Percentage),
                ("Indexed to 100", PriceScaleMode::IndexedTo100),
                ("Logarithmic", PriceScaleMode::Logarithmic),
            ] {
                entries.push(row(label, Action::Mode(mode), scale.options().mode == mode));
            }
            entries.extend([
                MenuEntry::Separator,
                row("Invert scale", Action::Invert, scale.options().invert_scale),
                row(
                    "Lock price to bar ratio",
                    Action::LockRatio,
                    scale.bar_ratio_locked(),
                ),
                MenuEntry::Separator,
                row(
                    "Place scale on left",
                    Action::PlaceLeft(true),
                    config.show_left_axis && !config.show_right_axis,
                ),
                row(
                    "Place scale on right",
                    Action::PlaceLeft(false),
                    config.show_right_axis,
                ),
                MenuEntry::Separator,
                row(
                    "Last price label",
                    Action::LastValue,
                    config.show_symbol_last_val,
                ),
                row(
                    "Previous close label",
                    Action::PreviousClose,
                    config.show_symbol_prev_close,
                ),
                row(
                    "Indicator values",
                    Action::Studies,
                    config.show_indicator_last_val,
                ),
                row(
                    "Countdown to bar close",
                    Action::Countdown,
                    config.show_countdown,
                ),
                row("Bid and ask", Action::BidAsk, config.show_bid_ask),
                row("High and low", Action::HighLow, config.show_high_low),
                MenuEntry::Separator,
                row("Scale settings…", Action::Settings, false),
            ]);
            entries
        }
    }

    pub(super) fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ContextMenuAction> {
        let (time, pos) = self.open?;
        let entries = Self::entries(time, chart);
        if let Some(action) = show_menu(
            ctx,
            Id::unique("axis_context_menu"),
            &ctx.active_theme(),
            pos,
            POINTER_GAP,
            &entries,
            std::mem::take(&mut self.just_opened),
        ) {
            self.close();
            let scale = chart.price_scale_mut();
            match action? {
                Action::Auto => {
                    if scale.is_manual() {
                        scale.reset();
                    } else {
                        let range = scale.range();
                        scale.set_range(range.min, range.max);
                    }
                }
                Action::Mode(mode) => {
                    scale.reset();
                    scale.options_mut().mode = mode;
                }
                Action::Invert => scale.options_mut().invert_scale = !scale.options().invert_scale,
                Action::LockRatio => {
                    let locked = !scale.bar_ratio_locked();
                    let spacing = chart.state().time_scale().bar_spacing();
                    let height = chart.get_rendered_price_rect().height();
                    chart
                        .price_scale_mut()
                        .set_bar_ratio_locked(locked, spacing, height);
                }
                Action::PlaceLeft(left) => {
                    chart.config_mut().show_left_axis = left;
                    chart.config_mut().show_right_axis = !left;
                }
                Action::LastValue => {
                    chart.config_mut().show_symbol_last_val = !chart.config().show_symbol_last_val
                }
                Action::PreviousClose => {
                    chart.config_mut().show_symbol_prev_close =
                        !chart.config().show_symbol_prev_close
                }
                Action::Studies => {
                    chart.config_mut().show_indicator_last_val =
                        !chart.config().show_indicator_last_val
                }
                Action::Countdown => {
                    chart.config_mut().show_countdown = !chart.config().show_countdown
                }
                Action::BidAsk => chart.config_mut().show_bid_ask = !chart.config().show_bid_ask,
                Action::HighLow => chart.config_mut().show_high_low = !chart.config().show_high_low,
                Action::Timezone(zone) => chart.chart_options.time_scale.timezone = zone,
                Action::ExtendedHours => chart.set_extended_hours(!chart.extended_hours()),
                Action::SessionBreaks => {
                    chart.config_mut().show_session_breaks = !chart.config().show_session_breaks
                }
                Action::Settings => return Some(ContextMenuAction::OpenSettings),
            }
        }
        None
    }
}
