//! Price and time axis menus used by the chart's turnkey menus.

use super::{ContextMenuAction, MenuEntry, MenuItem, POINTER_GAP, show_menu};
use crate::icons::icons;
use crate::model::{AxisSide, PaneAxisOptions};
use crate::scales::{PriceScaleMode, TimezoneMode};
use crate::theme::ThemeExt;
use crate::widget::{Chart, PaneId, PaneScale, SeriesId};
use egui::{Context, Id, Pos2, Rect};

#[derive(Clone)]
enum Action {
    Auto,
    PriceOnly,
    Mode(PriceScaleMode),
    Invert,
    LockRatio,
    PlaceLeft(bool),
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
        // A touch held in place asks for the menu as a secondary click does. egui reports the
        // hold to the widget a tap would click, which every axis that takes clicks is part of.
        let held = ctx.interaction_snapshot(|widgets| widgets.long_touched.is_some());
        let Some(pos) = ctx.input(|input| {
            (held || input.pointer.secondary_clicked())
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
        let target = chart.axis_at_position(pos).filter(|_| pos.y < bottom);
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
                return Self::layout_rows(chart, pane);
            };
            let scale = chart.pane_price_scale(pane, key);
            // The fit and label rows switch settings of the price chart, so only the axes of the
            // main series' pane offer them.
            let price_pane = chart.series_pane(SeriesId::MAIN).unwrap_or(PaneId::MAIN) == pane;
            let mut entries = vec![row("Auto scale", Action::Auto, !scale.is_manual())];
            if price_pane {
                entries.push(row(
                    "Scale price chart only",
                    Action::PriceOnly,
                    config.scale_price_chart_only,
                ));
            }
            entries.push(MenuEntry::Separator);
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
            ]);
            if chart.series_pane(SeriesId::MAIN) == Some(pane)
                && chart.series_scale(SeriesId::MAIN) == Some(key)
            {
                let axis = PaneScale::Named(key);
                entries.extend([
                    row(
                        "Place scale on left",
                        Action::PlaceLeft(true),
                        chart.pane_scales(pane, AxisSide::Left).contains(&axis),
                    ),
                    row(
                        "Place scale on right",
                        Action::PlaceLeft(false),
                        chart.pane_scales(pane, AxisSide::Right).contains(&axis),
                    ),
                    MenuEntry::Separator,
                ]);
            }
            if price_pane {
                entries.extend([
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
                        config.price_axis.show_study_values,
                    ),
                    row(
                        "Countdown to bar close",
                        Action::Countdown,
                        config.show_countdown,
                    ),
                    row("Bid and ask", Action::BidAsk, config.show_bid_ask),
                    row("High and low", Action::HighLow, config.show_high_low),
                    MenuEntry::Separator,
                ]);
            }
            entries.extend([
                row("Scale settings…", Action::Settings, false),
                MenuEntry::Separator,
            ]);
            entries.extend(Self::layout_rows(chart, pane));
            entries
        }
    }

    fn layout_rows(chart: &Chart, pane: PaneId) -> Vec<MenuEntry<Action>> {
        layout_entries(chart.pane_axis_options(pane).unwrap_or_default())
            .into_iter()
            .map(|entry| {
                entry.map(&|action| match action {
                    LayoutAction::Options(options) => Action::Layout(options),
                    LayoutAction::Merge(side) => Action::Merge(side),
                })
            })
            .collect()
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
                    // The frame holds the main series' pane to its plot, any other to its panel.
                    let height =
                        if chart.series_pane(SeriesId::MAIN).unwrap_or(PaneId::MAIN) == pane {
                            chart.get_rendered_price_rect().height()
                        } else {
                            chart.pane_height(pane).unwrap_or_default()
                        };
                    if let PaneScale::Named(key) = key {
                        chart
                            .pane_price_scale_mut(pane, key)
                            .set_bar_ratio_locked(locked, spacing, height);
                    }
                }
                Action::PlaceLeft(left) => {
                    let options = chart.pane_axis_options(pane).unwrap_or_default();
                    chart.set_pane_axis_options(
                        pane,
                        PaneAxisOptions {
                            side: None,
                            ..options
                        },
                    );
                    chart.config_mut().show_left_axis = left;
                    chart.config_mut().show_right_axis = !left;
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
                    chart.config_mut().price_axis.show_study_values =
                        !chart.config().price_axis.show_study_values
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
    let mut entries = Vec::new();
    for (label, side) in [
        ("Axes on left", AxisSide::Left),
        ("Axes on right", AxisSide::Right),
    ] {
        entries.push(row(
            label,
            LayoutAction::Options(PaneAxisOptions {
                side: (options.side != Some(side)).then_some(side),
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

    /// The corner under the axis opens no price menu, and only the main series' axis offers
    /// its own placement.
    #[test]
    fn price_menu_follows_axis_rows_and_offers_main_axis_placement() {
        let ctx = Context::default();
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[101.0]);
        chart.config_mut().show_volume = false;
        chart.add_series(crate::widget::SeriesOptions {
            scale: crate::model::SeriesScale::Left,
            ..Default::default()
        });
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let plot = chart.get_rendered_price_rect();
        let mut menus = AxisMenus::default();
        let click = |menus: &mut AxisMenus, chart: &mut Chart, at: f64, pos: Pos2| {
            for (step, pressed) in [true, false].into_iter().enumerate() {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                        time: Some(at + step as f64 * 0.1),
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
                        menus.open_from_pointer(ui.ctx(), chart);
                        menus.show(ui.ctx(), chart);
                    },
                )
                .drop_without_applying_deltas();
            }
        };
        let x = plot.right() + 8.0;
        click(
            &mut menus,
            &mut chart,
            0.1,
            Pos2::new(x, plot.bottom() + 8.0),
        );
        assert!(!menus.is_open(), "the corner under the axis");
        click(&mut menus, &mut chart, 0.4, Pos2::new(x, plot.center().y));
        assert!(menus.is_open(), "the axis itself");

        let places = |target: PaneScale| {
            AxisMenus::entries(false, &chart, Some((PaneId::MAIN, target)))
                .iter()
                .filter(|entry| {
                    matches!(entry, MenuEntry::Item(item) if matches!(item.action, Action::PlaceLeft(_)))
                })
                .count()
        };
        let main = PaneScale::Named(crate::model::SeriesScale::Right);
        assert_eq!(places(main), 2);
        assert_eq!(places(PaneScale::Named(crate::model::SeriesScale::Left)), 0);
    }

    /// Choosing the active side again returns the pane to placement by assignment, the only
    /// way back in a secondary pane.
    #[test]
    fn layout_rows_toggle_the_active_side_off() {
        let side_after = |options, label: &str| {
            layout_entries(options)
                .into_iter()
                .find_map(|entry| match entry {
                    MenuEntry::Item(item) if item.label == label => match item.action {
                        LayoutAction::Options(next) => Some(next.side),
                        LayoutAction::Merge(_) => None,
                    },
                    _ => None,
                })
        };
        let left = PaneAxisOptions {
            side: Some(AxisSide::Left),
            ..Default::default()
        };
        assert_eq!(side_after(left, "Axes on left"), Some(None));
        assert_eq!(
            side_after(left, "Axes on right"),
            Some(Some(AxisSide::Right))
        );
        assert_eq!(
            side_after(Default::default(), "Axes on left"),
            Some(Some(AxisSide::Left))
        );
    }

    /// The lock row of a secondary pane's axis measures that pane, so the axis holds its range
    /// on the frames after locking.
    #[test]
    fn lock_row_measures_the_pane_of_its_axis() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[101.0]);
        chart.config_mut().show_volume = false;
        let id = chart.add_indicator(Box::new(
            CustomIndicator::new("Lower", |_| vec![IndicatorValue::Single(10.0); 100])
                .with_overlay(false),
        ));
        let pane = chart.series_pane(id).unwrap();
        let axis = PaneScale::Named(crate::model::SeriesScale::Right);
        let mut harness = Harness::builder()
            .allow_missing_glyphs()
            .with_size(Vec2::new(800.0, 600.0))
            .build_ui_state(
                |ui, (chart, menus): &mut (Chart, AxisMenus)| {
                    chart.show(ui);
                    menus.show(ui.ctx(), chart);
                },
                (chart, AxisMenus::default()),
            );
        harness.run();
        let span = |chart: &Chart| {
            let range = chart.pane_scale(pane, axis).unwrap().range();
            range.max - range.min
        };
        let fitted = span(&harness.state().0);
        let menus = &mut harness.state_mut().1;
        menus.open = Some((false, Pos2::new(100.0, 100.0)));
        menus.target = Some((pane, axis));
        menus.just_opened = true;
        harness.run();
        harness.get_by_label("Lock price to bar ratio").click();
        harness.run();

        let chart = &harness.state().0;
        let height = chart.pane_height(pane).unwrap();
        assert_ne!(height, chart.get_rendered_price_rect().height());
        let spacing = chart.state().time_scale().bar_spacing();
        let ratio = chart.pane_scale(pane, axis).unwrap().bar_ratio().unwrap();
        assert!((ratio - fitted * f64::from(spacing / height)).abs() <= 1e-4 * ratio);
        assert!((span(chart) - fitted).abs() <= 1e-4 * fitted, "a jump");
    }
}
