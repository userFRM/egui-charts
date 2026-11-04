//! Price and time axis menus used by the chart's turnkey menus.

use super::{ContextMenuAction, MenuEntry, MenuItem, POINTER_GAP, show_menu};
use crate::icons::icons;
use crate::model::{AxisSide, PaneAxisOptions};
use crate::scales::{PriceScaleMode, TimezoneMode};
use crate::theme::ThemeExt;
use crate::widget::{Chart, PaneId, PaneScale};
use egui::{Context, Id, Pos2, Rect};

#[derive(Clone)]
enum Action {
    Auto,
    PriceOnly,
    Mode(PriceScaleMode),
    Invert,
    LockRatio,
    Layout(PaneAxisOptions),
    Merge(AxisSide),
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
    target: Option<(PaneId, PaneScale)>,
    pub(super) hidden: std::collections::HashSet<crate::ui::Control>,
}

impl AxisMenus {
    pub(super) fn close(&mut self) {
        self.open = None;
    }

    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(super) fn open_from_pointer(&mut self, ctx: &Context, chart: &Chart) -> bool {
        if egui::Popup::is_any_open(ctx) {
            return false;
        }
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
        let target = chart.axis_at_position(pos);
        let price = target.is_some();
        let time = config.show_time_labels
            && Rect::from_min_max(
                Pos2::new(plot.left(), bottom),
                Pos2::new(plot.right(), bottom + 28.0),
            )
            .contains(pos);
        if price || time {
            self.open = Some((time, pos));
            self.target = target;
            self.just_opened = true;
            true
        } else {
            false
        }
    }

    fn entries(
        time: bool,
        chart: &Chart,
        target: Option<(PaneId, PaneScale)>,
    ) -> Vec<MenuEntry<Action>> {
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
            let exchange = TimezoneMode::Exchange(crate::model::TimezoneId::utc());
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
            let (pane, key) = target.unwrap_or((
                PaneId::MAIN,
                PaneScale::Named(crate::model::SeriesScale::Right),
            ));
            let PaneScale::Named(key) = key else {
                return layout_entries(chart.pane_axis_options(pane).unwrap_or_default())
                    .into_iter()
                    .filter(|entry| !matches!(entry, MenuEntry::Separator))
                    .map(|entry| {
                        entry.map(&|action| match action {
                            LayoutAction::Options(options) => Action::Layout(options),
                            LayoutAction::Merge(side) => Action::Merge(side),
                        })
                    })
                    .collect();
            };
            let scale = chart.pane_price_scale(pane, key);
            let mut entries = vec![
                row("Auto scale", Action::Auto, !scale.is_manual()),
                row(
                    "Scale price chart only",
                    Action::PriceOnly,
                    config.scale_price_chart_only,
                ),
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
            entries.extend(
                layout_entries(chart.pane_axis_options(pane).unwrap_or_default())
                    .into_iter()
                    .map(|entry| {
                        entry.map(&|action| match action {
                            LayoutAction::Options(options) => Action::Layout(options),
                            LayoutAction::Merge(side) => Action::Merge(side),
                        })
                    }),
            );
            entries
        }
    }

    pub(super) fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ContextMenuAction> {
        let (time, pos) = self.open?;
        if let Some((pane, key)) = self.target
            && !chart.pane_scales(pane, AxisSide::Left).contains(&key)
            && !chart.pane_scales(pane, AxisSide::Right).contains(&key)
        {
            self.close();
            return None;
        }
        let mut entries = Self::entries(time, chart, self.target);
        entries.retain(|entry| {
            use crate::ui::Control;
            let control = match entry {
                MenuEntry::Item(item) => match item.action {
                    Action::Settings => Control::Settings,
                    Action::Timezone(_) => Control::TimeZone,
                    Action::ExtendedHours | Action::SessionBreaks => Control::Session,
                    _ => return true,
                },
                MenuEntry::Submenu { .. } | MenuEntry::Separator => return true,
            };
            !self.hidden.contains(&control)
        });
        super::trim_separators(&mut entries);
        if entries.is_empty() {
            self.close();
            return None;
        }
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
            let action = action?;
            let (pane, key) = self.target.unwrap_or((
                PaneId::MAIN,
                PaneScale::Named(crate::model::SeriesScale::Right),
            ));
            if let Action::Merge(side) = action {
                chart.merge_pane_scales(pane, side);
                return None;
            }
            if let Action::Layout(options) = action {
                chart.set_pane_axis_options(pane, options);
                return None;
            }
            let scale = match self.target {
                Some((pane, PaneScale::Named(key))) => chart.pane_price_scale_mut(pane, key),
                Some((_, PaneScale::Study(_))) => return None,
                None => chart.price_scale_mut(),
            };
            match action {
                Action::Auto => {
                    if scale.is_manual() {
                        scale.reset();
                    } else {
                        let range = scale.range();
                        scale.set_range(range.min, range.max);
                    }
                }
                Action::PriceOnly => {
                    chart.config_mut().scale_price_chart_only =
                        !chart.config().scale_price_chart_only;
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
                    if let PaneScale::Named(key) = key {
                        chart
                            .pane_price_scale_mut(pane, key)
                            .set_bar_ratio_locked(locked, spacing, height);
                    }
                }
                Action::Layout(_) | Action::Merge(_) => unreachable!("layout was applied above"),
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

pub(super) enum LayoutAction {
    Options(PaneAxisOptions),
    Merge(AxisSide),
}

/// The same pane controls remain reachable from the plot when all axes are hidden.
pub(super) fn layout_entries(options: PaneAxisOptions) -> Vec<MenuEntry<LayoutAction>> {
    let row = |label: &str, next, checked| {
        let mut item = MenuItem::new(label, next);
        if checked {
            item = item.with_icon(&icons::CHECK);
        }
        MenuEntry::Item(item)
    };
    let mut entries = vec![MenuEntry::Separator];
    for (label, side) in [
        ("Axes on left", AxisSide::Left),
        ("Axes on right", AxisSide::Right),
    ] {
        entries.push(row(
            label,
            LayoutAction::Options(PaneAxisOptions {
                side: Some(side),
                ..options
            }),
            options.side == Some(side),
        ));
    }
    entries.push(MenuEntry::Submenu {
        label: "Join price scales".into(),
        icon: None,
        entries: vec![
            row("At left edge", LayoutAction::Merge(AxisSide::Left), false),
            row("At right edge", LayoutAction::Merge(AxisSide::Right), false),
        ],
    });
    entries.push(row(
        "Hide unused axes",
        LayoutAction::Options(PaneAxisOptions {
            hide_unused: !options.hide_unused,
            ..options
        }),
        options.hide_unused,
    ));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{CustomIndicator, IndicatorValue};
    use egui::{Event, Modifiers, PointerButton, RawInput, Vec2};

    /// The selected pane owns the menu, which closes if that pane or its last source leaves.
    #[test]
    fn axis_menu_tracks_its_pane_and_closes_after_source_removal() {
        for remove_pane in [false, true] {
            let ctx = Context::default();
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[101.0]);
            chart.config_mut().show_volume = false;
            let id = chart.add_indicator(Box::new(
                CustomIndicator::new("Lower", |_| vec![IndicatorValue::Single(10.0); 100])
                    .with_overlay(false),
            ));
            let pane = chart.series_pane(id).unwrap();
            crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            let panel = chart.get_rendered_indicator_panes()[0].panel_rect;
            let pos = Pos2::new(
                chart.get_rendered_price_rect().right() + 8.0,
                panel.center().y,
            );
            let mut menus = AxisMenus::default();
            for (at, pressed) in [true, false].into_iter().enumerate() {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                        time: Some(0.1 + at as f64 * 0.1),
                        events: vec![
                            Event::PointerMoved(pos),
                            Event::PointerButton {
                                pos,
                                button: PointerButton::Secondary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| {
                        menus.open_from_pointer(ui.ctx(), &chart);
                    },
                )
                .drop_without_applying_deltas();
            }
            assert!(menus.is_open(), "the secondary pane's axis opens its menu");
            if remove_pane {
                assert!(chart.remove_pane(pane));
            } else {
                assert!(chart.remove_series(id));
            }
            ctx.run_ui(RawInput::default(), |ui| {
                menus.show(ui.ctx(), &mut chart);
            })
            .drop_without_applying_deltas();
            assert!(
                !menus.is_open(),
                "a removed target cannot retain an axis menu"
            );
        }
    }
}
