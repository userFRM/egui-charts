//! Visibility tab - the intervals a drawing shows on.

use std::ops::RangeInclusive;

use crate::localization as l10n;
use crate::model::TimeframeUnit;
use crate::theme::ThemeExt;
use egui::Ui;

use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::FormGrid;
use crate::ui_kit::ext::UiExt;

/// One row per interval unit: its label and the range a newly ticked row starts with.
const UNITS: [(TimeframeUnit, &str, RangeInclusive<u64>); 11] = [
    (TimeframeUnit::Tick, "Ticks", 1..=1_000_000),
    (TimeframeUnit::Volume, "Volume", 1..=1_000_000),
    (TimeframeUnit::Range, "Range", 1..=1_000_000),
    (TimeframeUnit::Delta, "Delta", 1..=1_000_000),
    (TimeframeUnit::Millisecond, "Milliseconds", 1..=999),
    (TimeframeUnit::Second, "Seconds", 1..=59),
    (TimeframeUnit::Minute, "Minutes", 1..=59),
    (TimeframeUnit::Hour, "Hours", 1..=24),
    (TimeframeUnit::Day, "Days", 1..=366),
    (TimeframeUnit::Week, "Weeks", 1..=52),
    (TimeframeUnit::Month, "Months", 1..=12),
];

/// Visibility tab renderer
pub struct VisibilityTab;

impl VisibilityTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps) {
        ui.space_md();
        let visibility = &mut props.timeframe_visibility;
        ui.checkbox(
            &mut visibility.show_on_all,
            l10n::text(ui.ctx(), "Show on all timeframes"),
        );
        if visibility.show_on_all {
            return;
        }
        ui.space_lg();
        FormGrid::new("timeframes_grid")
            .columns(4)
            .spacing(ui.active_theme().spacing.lg, ui.active_theme().spacing.sm)
            .show(ui, |ui| {
                for (unit, name, start) in UNITS {
                    let saved = visibility.range(unit);
                    let mut ticked = saved.is_some();
                    let (mut from, mut to) = saved.unwrap_or(start).into_inner();
                    let name = l10n::text(ui.ctx(), name);
                    let label = |ui: &Ui, end: &str| {
                        l10n::format_text(ui.ctx(), end, &[("0", name.clone())])
                    };
                    let mut changed = ui.checkbox(&mut ticked, &name).changed();
                    changed |= ui
                        .add_enabled(ticked, egui::DragValue::new(&mut from).range(1..=to))
                        .accessible_name(label(ui, "{0} from"))
                        .changed();
                    ui.add_enabled(ticked, egui::Label::new(l10n::text(ui.ctx(), "to")));
                    changed |= ui
                        .add_enabled(ticked, egui::DragValue::new(&mut to).range(from..=u64::MAX))
                        .accessible_name(label(ui, "{0} to"))
                        .changed();
                    // Only an edited row is written, so saved labels of other units stay exact.
                    if changed {
                        visibility.set_range(unit, ticked.then_some(from..=to));
                    }
                    ui.end_row();
                }
            });
    }
}
