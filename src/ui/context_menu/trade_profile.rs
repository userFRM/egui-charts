//! User commands for host-owned session and composite trade profiles.

use crate::localization as l10n;
use crate::studies::TradeProfileRanges;
use chrono::{DateTime, Utc};

/// Shows merge/split commands for `index` inside a host menu. `split_at` is a bar boundary
/// selected by the user, such as the hovered bar. Returns whether the ranges changed; rebuild
/// the profiles with [`crate::studies::trade_profiles`] after a change.
pub fn trade_profile_menu(
    ui: &mut egui::Ui,
    ranges: &mut TradeProfileRanges,
    index: usize,
    split_at: Option<DateTime<Utc>>,
) -> bool {
    let merge = index < ranges.ranges().len().saturating_sub(1);
    if ui
        .add_enabled(
            merge,
            egui::Button::new(l10n::text(ui.ctx(), "Merge with next profile")),
        )
        .clicked()
    {
        ui.close();
        return ranges.merge_next(index);
    }
    let split = split_at.filter(|time| {
        ranges
            .ranges()
            .get(index)
            .is_some_and(|r| r.start < *time && *time < r.end)
    });
    if ui
        .add_enabled(
            split.is_some(),
            egui::Button::new(l10n::text(ui.ctx(), "Split profile at selected bar")),
        )
        .clicked()
    {
        ui.close();
        return split.is_some_and(|time| ranges.split(index, time));
    }
    false
}
