//! Tiled and tabbed charts with independent state and timestamp-based synchronization.

use super::keyboard::ShortcutRequest;
use super::{Chart, VisibleTimeRange};
use crate::config::KeyboardAction;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use egui::{Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[cfg(feature = "drawings")]
mod drawings;
#[cfg(feature = "drawings")]
use drawings::DrawingLinks;

/// Multi-chart arrangement. Changing presets retains the host's hidden charts. Charts fill a preset
/// in slot order, row by row from the top; presets with a large chart at the left or right fill
/// column by column from the left.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum ChartLayoutMode {
    /// One chart.
    #[default]
    Single,
    /// Two columns.
    TwoHorizontal,
    /// Two rows.
    TwoVertical,
    /// Three charts, with the last spanning the bottom row.
    Three,
    /// Three columns.
    ThreeHorizontal,
    /// Three rows.
    ThreeVertical,
    /// One chart in the left half, two stacked in the right half.
    ThreeLargeLeft,
    /// Two charts stacked in the left half, one in the right half.
    ThreeLargeRight,
    /// One chart in the top half, two side by side in the bottom half; [`Three`](Self::Three) is
    /// the reverse.
    ThreeLargeTop,
    /// Two columns and two rows.
    Four,
    /// Four columns.
    FourHorizontal,
    /// Four rows.
    FourVertical,
    /// One chart in the left half, three stacked in the right half.
    FourLargeLeft,
    /// Three charts stacked in the left half, one in the right half.
    FourLargeRight,
    /// One chart in the top half, three side by side in the bottom half.
    FourLargeTop,
    /// Three charts side by side in the top half, one in the bottom half.
    FourLargeBottom,
    /// Five charts in three columns.
    Five,
    /// Five columns.
    FiveHorizontal,
    /// Five rows.
    FiveVertical,
    /// One chart in the left half, four stacked in the right half.
    FiveLargeLeft,
    /// Four charts stacked in the left half, one in the right half.
    FiveLargeRight,
    /// One chart in the top half, four side by side in the bottom half.
    FiveLargeTop,
    /// Four charts side by side in the top half, one in the bottom half.
    FiveLargeBottom,
    /// Three columns and two rows.
    Six,
    /// Six columns.
    SixHorizontal,
    /// Six rows.
    SixVertical,
    /// One chart in the left half, five stacked in the right half.
    SixLargeLeft,
    /// Five charts stacked in the left half, one in the right half.
    SixLargeRight,
    /// One chart in the top half, five side by side in the bottom half.
    SixLargeTop,
    /// Five charts side by side in the top half, one in the bottom half.
    SixLargeBottom,
    /// Seven charts in four columns.
    Seven,
    /// Four columns and two rows.
    Eight,
    /// Three columns and three rows.
    Nine,
    /// Ten charts in four columns.
    Ten,
    /// Eleven charts in four columns.
    Eleven,
    /// Four columns and three rows.
    Twelve,
    /// Thirteen charts in four columns.
    Thirteen,
    /// Fourteen charts in four columns.
    Fourteen,
    /// Fifteen charts in four columns.
    Fifteen,
    /// Four columns and four rows.
    Sixteen,
    /// One tab per supplied chart, up to sixteen.
    Tabs,
}

impl ChartLayoutMode {
    /// All selectable presets, by chart count.
    pub fn all() -> &'static [Self] {
        &[
            Self::Single,
            Self::TwoHorizontal,
            Self::TwoVertical,
            Self::Three,
            Self::ThreeHorizontal,
            Self::ThreeVertical,
            Self::ThreeLargeLeft,
            Self::ThreeLargeRight,
            Self::ThreeLargeTop,
            Self::Four,
            Self::FourHorizontal,
            Self::FourVertical,
            Self::FourLargeLeft,
            Self::FourLargeRight,
            Self::FourLargeTop,
            Self::FourLargeBottom,
            Self::Five,
            Self::FiveHorizontal,
            Self::FiveVertical,
            Self::FiveLargeLeft,
            Self::FiveLargeRight,
            Self::FiveLargeTop,
            Self::FiveLargeBottom,
            Self::Six,
            Self::SixHorizontal,
            Self::SixVertical,
            Self::SixLargeLeft,
            Self::SixLargeRight,
            Self::SixLargeTop,
            Self::SixLargeBottom,
            Self::Seven,
            Self::Eight,
            Self::Nine,
            Self::Ten,
            Self::Eleven,
            Self::Twelve,
            Self::Thirteen,
            Self::Fourteen,
            Self::Fifteen,
            Self::Sixteen,
            Self::Tabs,
        ]
    }

    /// Human-readable preset name, distinct for every preset.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::TwoHorizontal => "2 Horizontal",
            Self::TwoVertical => "2 Vertical",
            Self::Three => "2 Top, 1 Bottom",
            Self::ThreeHorizontal => "3 Horizontal",
            Self::ThreeVertical => "3 Vertical",
            Self::ThreeLargeLeft => "1 Left, 2 Right",
            Self::ThreeLargeRight => "2 Left, 1 Right",
            Self::ThreeLargeTop => "1 Top, 2 Bottom",
            Self::Four => "2x2 Grid",
            Self::FourHorizontal => "4 Horizontal",
            Self::FourVertical => "4 Vertical",
            Self::FourLargeLeft => "1 Left, 3 Right",
            Self::FourLargeRight => "3 Left, 1 Right",
            Self::FourLargeTop => "1 Top, 3 Bottom",
            Self::FourLargeBottom => "3 Top, 1 Bottom",
            Self::Five => "3 Top, 2 Bottom",
            Self::FiveHorizontal => "5 Horizontal",
            Self::FiveVertical => "5 Vertical",
            Self::FiveLargeLeft => "1 Left, 4 Right",
            Self::FiveLargeRight => "4 Left, 1 Right",
            Self::FiveLargeTop => "1 Top, 4 Bottom",
            Self::FiveLargeBottom => "4 Top, 1 Bottom",
            Self::Six => "3x2 Grid",
            Self::SixHorizontal => "6 Horizontal",
            Self::SixVertical => "6 Vertical",
            Self::SixLargeLeft => "1 Left, 5 Right",
            Self::SixLargeRight => "5 Left, 1 Right",
            Self::SixLargeTop => "1 Top, 5 Bottom",
            Self::SixLargeBottom => "5 Top, 1 Bottom",
            Self::Seven => "7 Charts",
            Self::Eight => "4x2 Grid",
            Self::Nine => "3x3 Grid",
            Self::Ten => "10 Charts",
            Self::Eleven => "11 Charts",
            Self::Twelve => "4x3 Grid",
            Self::Thirteen => "13 Charts",
            Self::Fourteen => "14 Charts",
            Self::Fifteen => "15 Charts",
            Self::Sixteen => "4x4 Grid",
            Self::Tabs => "Tabs",
        }
    }

    /// Number of charts in a preset; tabs can show up to sixteen supplied charts.
    pub fn chart_count(&self) -> usize {
        if *self == Self::Tabs {
            return 16;
        }
        let (Tracks::Rows(cells) | Tracks::Columns(cells)) = self.tracks();
        cells.iter().sum()
    }

    /// Maximum columns and rows. A row or column with fewer cells than another stretches them to
    /// fill it.
    pub fn grid_size(&self) -> (usize, usize) {
        match self.tracks() {
            Tracks::Rows(cells) => (cells.iter().copied().max().unwrap_or(1), cells.len()),
            Tracks::Columns(cells) => (cells.len(), cells.iter().copied().max().unwrap_or(1)),
        }
    }

    /// How the preset divides its rectangle. Tabs show one chart at a time in one cell.
    fn tracks(&self) -> Tracks {
        use Tracks::{Columns, Rows};
        match self {
            Self::Single | Self::Tabs => Rows(&[1]),
            Self::TwoHorizontal => Rows(&[2]),
            Self::TwoVertical => Rows(&[1, 1]),
            Self::Three => Rows(&[2, 1]),
            Self::ThreeHorizontal => Rows(&[3]),
            Self::ThreeVertical => Rows(&[1, 1, 1]),
            Self::ThreeLargeLeft => Columns(&[1, 2]),
            Self::ThreeLargeRight => Columns(&[2, 1]),
            Self::ThreeLargeTop => Rows(&[1, 2]),
            Self::Four => Rows(&[2, 2]),
            Self::FourHorizontal => Rows(&[4]),
            Self::FourVertical => Rows(&[1, 1, 1, 1]),
            Self::FourLargeLeft => Columns(&[1, 3]),
            Self::FourLargeRight => Columns(&[3, 1]),
            Self::FourLargeTop => Rows(&[1, 3]),
            Self::FourLargeBottom => Rows(&[3, 1]),
            Self::Five => Rows(&[3, 2]),
            Self::FiveHorizontal => Rows(&[5]),
            Self::FiveVertical => Rows(&[1, 1, 1, 1, 1]),
            Self::FiveLargeLeft => Columns(&[1, 4]),
            Self::FiveLargeRight => Columns(&[4, 1]),
            Self::FiveLargeTop => Rows(&[1, 4]),
            Self::FiveLargeBottom => Rows(&[4, 1]),
            Self::Six => Rows(&[3, 3]),
            Self::SixHorizontal => Rows(&[6]),
            Self::SixVertical => Rows(&[1, 1, 1, 1, 1, 1]),
            Self::SixLargeLeft => Columns(&[1, 5]),
            Self::SixLargeRight => Columns(&[5, 1]),
            Self::SixLargeTop => Rows(&[1, 5]),
            Self::SixLargeBottom => Rows(&[5, 1]),
            Self::Seven => Rows(&[4, 3]),
            Self::Eight => Rows(&[4, 4]),
            Self::Nine => Rows(&[3, 3, 3]),
            Self::Ten => Rows(&[4, 4, 2]),
            Self::Eleven => Rows(&[4, 4, 3]),
            Self::Twelve => Rows(&[4, 4, 4]),
            Self::Thirteen => Rows(&[4, 4, 4, 1]),
            Self::Fourteen => Rows(&[4, 4, 4, 2]),
            Self::Fifteen => Rows(&[4, 4, 4, 3]),
            Self::Sixteen => Rows(&[4, 4, 4, 4]),
        }
    }
}

impl std::fmt::Display for ChartLayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The cells of a preset: rows from the top or columns from the left, each holding the given
/// number of equal cells. Charts fill one row or column after the other.
enum Tracks {
    Rows(&'static [usize]),
    Columns(&'static [usize]),
}

/// Independent synchronization switches within each layout group.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSyncOptions {
    /// Synchronize scrolling and zooming using UTC plot edges.
    pub sync_time_axis: bool,
    /// Synchronize the hovered timestamp.
    pub sync_crosshair: bool,
    /// Synchronize completed main-pane drawings between charts of the same symbol.
    /// Study-owned drawings stay local because study ids and scales belong to each chart.
    /// Charts that keep their drawings in the same storage are left to it: they read each other's
    /// stored drawings whether this is on or off, so a drawing reaches them once a chart history
    /// edit writes it. Only drawings that are not saveable still sync between them here.
    pub sync_drawings: bool,
    /// Synchronize symbol changes. A host without a feed must supply replacement bars.
    pub sync_symbol: bool,
    /// Synchronize interval changes. A host without a feed must supply replacement bars.
    pub sync_timeframe: bool,
    /// Apply host date-range selections to the group, independently of scrolling and zooming.
    pub sync_date_range: bool,
}

impl Default for ChartSyncOptions {
    fn default() -> Self {
        Self {
            sync_time_axis: true,
            sync_crosshair: true,
            sync_drawings: false,
            sync_symbol: false,
            sync_timeframe: false,
            sync_date_range: false,
        }
    }
}

/// Container state for up to sixteen host-owned charts. Supply charts in stable slot order.
///
/// Each chart keeps its own feed, settings, scales, studies and drawings. An unnamed group
/// synchronizes with other unnamed charts; named groups are isolated. Save this state and the
/// charts through host storage as needed. Synchronization includes supplied slots hidden by a
/// grid preset. The container does not acquire data or persist it. Charts that keep their
/// drawings in the same storage (`Chart::set_storage`) read the drawings of their symbol again
/// after another of them wrote them, in any group.
///
/// ```
/// # fn example(ui: &mut egui::Ui) {
/// use egui_charts::widget::{Chart, ChartLayout, ChartLayoutMode};
/// let mut a = Chart::new(egui_charts::model::BarData::new());
/// let mut b = Chart::new(egui_charts::model::BarData::new());
/// let mut layout = ChartLayout::default();
/// layout.mode = ChartLayoutMode::TwoHorizontal;
/// layout.show(ui, &mut [&mut a, &mut b]);
/// # }
/// ```
#[derive(Clone, Default)]
pub struct ChartLayout {
    /// The selected grid or tab preset.
    pub mode: ChartLayoutMode,
    /// Synchronization controls, applied within each group.
    pub sync: ChartSyncOptions,
    active: usize,
    /// The chart slot Alt+Enter shows alone in place of the preset.
    maximized: Option<usize>,
    focus_active: bool,
    groups: [Option<String>; 16],
    detached: [bool; 16],
    previous: [Option<(String, Option<Timeframe>)>; 16],
    previous_ranges: [Option<VisibleTimeRange>; 16],
    previous_bars: [BarsKey; 16],
    pending_ranges: [Option<PendingRange>; 16],
    #[cfg(feature = "drawings")]
    drawings: DrawingLinks,
    previous_preset: Option<(ChartLayoutMode, usize)>,
}

/// Identifies a chart's bars: it changes when they are replaced, not when they stream.
type BarsKey = (u64, Option<chrono::DateTime<chrono::Utc>>);

fn bars_key(chart: &Chart) -> BarsKey {
    (
        chart.state.revision(),
        chart.state.shown().first().map(|bar| bar.time),
    )
}

/// A range a chart fits once bars of its new interval replace those it held at the switch.
#[derive(Clone, Copy)]
struct PendingRange {
    range: VisibleTimeRange,
    /// The [`Chart::sync_source`] of the chart it came from, `None` for the chart's own range.
    source: Option<(Timeframe, f64)>,
    bars: BarsKey,
}

impl ChartLayout {
    /// Active chart slot, clamped to supplied charts by [`Self::show`].
    pub fn active(&self) -> usize {
        self.active
    }

    /// Selects a chart slot and focuses its canvas when shown; out-of-range indices are ignored.
    pub fn set_active(&mut self, index: usize) {
        if index < 16 {
            self.active = index;
            self.focus_active = true;
        }
    }

    /// Assigns a named synchronization group; `None` restores the unnamed group.
    pub fn set_sync_group(&mut self, index: usize, group: Option<String>) {
        if let Some(slot) = self.groups.get_mut(index) {
            *slot = group;
        }
    }

    /// The group assigned to a slot, or `None` for the unnamed group or an invalid slot.
    pub fn sync_group(&self, index: usize) -> Option<&str> {
        self.groups.get(index).and_then(|g| g.as_deref())
    }

    /// Detaches a chart into an immediate viewport. Backends without native multi-window
    /// support embed the viewport. Closing it returns the chart to its grid or tab.
    pub fn set_detached(&mut self, index: usize, detached: bool) {
        if let Some(slot) = self.detached.get_mut(index) {
            *slot = detached;
        }
    }

    /// Whether a chart is detached.
    pub fn is_detached(&self, index: usize) -> bool {
        self.detached.get(index).copied().unwrap_or(false)
    }

    /// Fits an explicit date selection to the active chart, or its synchronization group
    /// when `sync_date_range` is enabled. Call after the charts have rendered at their sizes.
    pub fn apply_date_range(&self, charts: &mut [&mut Chart], range: super::VisibleTimeRange) {
        let Some(active) = charts.get_mut(self.active) else {
            return;
        };
        let source = active
            .fit_time_range(range, f64::INFINITY)
            .then(|| active.sync_source())
            .flatten();
        for (index, chart) in charts.iter_mut().take(16).enumerate() {
            if index != self.active
                && self.sync.sync_date_range
                && self.same_group(index, self.active)
            {
                chart.apply_synced_time_range_from(range, source);
            }
        }
    }

    fn same_group(&self, a: usize, b: usize) -> bool {
        self.groups[a] == self.groups[b]
    }

    /// Shows the preset and synchronizes its charts. Click or scroll a panel to activate it.
    /// Its corner menu offers detachment and named groups. The active border is painted
    /// inside the chart; stacked charts have a two-point gap and side-by-side charts meet at their
    /// edge.
    /// Extra charts remain owned by the host and are not painted by a grid preset.
    pub fn show(&mut self, ui: &mut Ui, charts: &mut [&mut Chart]) -> Response {
        let total = charts.len().min(16);
        let charts = &mut charts[..total];
        let count = total.min(self.mode.chart_count());
        if count == 0 {
            return ui.allocate_response(Vec2::ZERO, Sense::hover());
        }
        self.active = self.active.min(count - 1);
        if self.maximized.is_some_and(|index| index >= count) {
            self.maximized = None;
        }
        if let Some(index) = self.maximized {
            self.active = index;
        }
        if self.mode == ChartLayoutMode::Tabs {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, chart) in charts.iter().take(count).enumerate() {
                        if ui
                            .selectable_label(index == self.active, chart.symbol())
                            .clicked()
                        {
                            self.active = index;
                        }
                    }
                });
            });
        }
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let rects = self.rects(rect, count);
        ui.painter()
            .rect_filled(rect, 0.0, ui.active_theme().ui.border);
        // Select before drawing so keyboard and toolbar state follow pointer input this frame.
        let wheel = super::pan_zoom::raw_wheel_delta(ui) != Vec2::ZERO;
        if self.mode != ChartLayoutMode::Tabs
            && let Some(index) = rects.iter().position(|r| {
                ui.rect_contains_pointer(*r) && ui.input(|i| i.pointer.any_pressed() || wheel)
            })
        {
            self.active = index;
        }
        self.sync_state(charts, total);
        for (index, chart) in charts.iter().enumerate() {
            if !self.sync.sync_time_axis || total == 1 {
                self.pending_ranges[index] = None;
            } else if self.previous[index]
                .as_ref()
                .is_some_and(|p| p.1 != chart.timeframe())
            {
                let own = self.previous_ranges[index].map(|range| PendingRange {
                    range,
                    source: None,
                    bars: self.previous_bars[index],
                });
                self.pending_ranges[index] = self.pending_ranges[index].or(own);
            }
        }
        for chart in charts.iter_mut() {
            chart.last_hover_bar_idx = None;
        }
        let mut pointer_down = ui.input(|i| i.pointer.any_down());
        let before_range = charts[self.active].visible_time_range();
        let before_sync: Vec<_> = charts
            .iter()
            .map(|c| (c.synced_crosshair_bar_idx, c.get_time_scale_state()))
            .collect();
        for index in 0..count {
            if self.mode == ChartLayoutMode::Tabs && index != self.active && !self.detached[index] {
                continue;
            }
            if self.maximized.is_some_and(|slot| slot != index) && !self.detached[index] {
                continue;
            }
            charts[index].layout_inactive =
                count > 1 && index != self.active && !self.detached[index];
            let focus = self.focus_active && index == self.active;
            if self.detached[index] {
                let id = egui::ViewportId::from_hash_of((ui.scope_id(), "detached_chart", index));
                let chart = &mut charts[index];
                let mut close = false;
                let mut activate = false;
                ui.ctx().show_viewport_immediate(
                    id,
                    egui::ViewportBuilder::default()
                        .with_title(chart.symbol())
                        .with_inner_size([900.0, 600.0]),
                    |ui, _| {
                        pointer_down |= ui.input(|i| i.pointer.any_down());
                        close |= ui.input(|i| i.viewport().close_requested());
                        if ui
                            .button(l10n::text(ui.ctx(), "Return to layout"))
                            .clicked()
                        {
                            close = true;
                        }
                        egui::CentralPanel::default().show(ui, |ui| {
                            let response = chart.show(ui);
                            if focus {
                                response.request_focus();
                            }
                            activate |= response.is_pointer_button_down_on()
                                || response.clicked()
                                || (response.hovered()
                                    && super::pan_zoom::raw_wheel_delta(ui) != Vec2::ZERO);
                        });
                    },
                );
                self.restore_time_range(index, chart);
                self.detached[index] = !close;
                if activate {
                    self.active = index;
                }
                if (self.mode != ChartLayoutMode::Tabs || index == self.active)
                    && self.maximized.is_none_or(|slot| slot == index)
                {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rects[index]), |ui| {
                        if ui
                            .button(l10n::text(ui.ctx(), "Return chart to layout"))
                            .clicked()
                        {
                            self.detached[index] = false;
                        }
                    });
                }
                continue;
            }
            let rect = rects[index];
            ui.painter()
                .rect_filled(rect, 0.0, charts[index].config().background_color);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(("chart_slot", index)),
                |ui| {
                    ui.shrink_clip_rect(rect);
                    let response = charts[index].show(ui);
                    charts[index].layout_inactive = false;
                    if focus {
                        response.request_focus();
                    }
                    if index == self.active
                        && (ui.rect_contains_pointer(rect) || egui::Popup::is_any_open(ui.ctx()))
                        && charts[index]
                            .pane_layout
                            .maximized
                            .is_none_or(|pane| pane == super::PaneId::MAIN)
                    {
                        let button = ui.put(
                            Rect::from_min_size(
                                rect.min
                                    + Vec2::new(
                                        (rect.width()
                                            - if rect.width() < 600.0 { 22.0 } else { 100.0 })
                                        .max(0.0),
                                        2.0,
                                    ),
                                Vec2::splat(20.0),
                            ),
                            egui::Button::new("…").frame(false),
                        );
                        egui::Popup::menu(&button)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                if ui.button(l10n::text(ui.ctx(), "Detach chart")).clicked() {
                                    self.detached[index] = true;
                                    ui.close();
                                }
                                ui.label(l10n::text(ui.ctx(), "Sync group"));
                                let mut group = self.groups[index].clone().unwrap_or_default();
                                if ui.text_edit_singleline(&mut group).changed() {
                                    self.groups[index] = (!group.is_empty()).then_some(group);
                                }
                            });
                    }
                },
            );
            self.restore_time_range(index, charts[index]);
            if count > 1 && self.maximized.is_none() && index == self.active {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(2.0, ui.active_theme().ui.accent),
                    StrokeKind::Inside,
                );
            }
        }
        self.focus_active = false;
        // Alt+Enter toggles the focused tile in a grid; a lone chart leaves it to the host.
        if self.mode != ChartLayoutMode::Tabs && count > 1 {
            for (index, chart) in charts.iter_mut().take(count).enumerate() {
                let requests = &mut chart.keyboard.requests;
                let before = requests.len();
                requests.retain(|request| {
                    *request != ShortcutRequest::Action(KeyboardAction::MaximizeChart)
                });
                // A discarded pass replays its input, so only the last one toggles.
                if requests.len() != before && !ui.ctx().will_discard() {
                    self.maximized = if self.maximized == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
                    self.set_active(index);
                    ui.ctx().request_repaint();
                }
            }
        }
        // Apply gestures from the source after it has handled this frame's input.
        let sync_range = charts[self.active].visible_time_range() != before_range
            || self.previous_preset != Some((self.mode, count));
        self.sync_view(charts, total, sync_range);
        self.previous_preset = Some((self.mode, count));
        if charts
            .iter()
            .zip(before_sync)
            .any(|(chart, (crosshair, (spacing, offset)))| {
                chart.synced_crosshair_bar_idx != crosshair
                    || (chart.get_time_scale_state().0 - spacing).abs() > 0.001
                    || (chart.get_time_scale_state().1 - offset).abs() > 0.001
            })
        {
            ui.ctx().request_repaint();
        }
        #[cfg(feature = "drawings")]
        if self.sync.sync_drawings
            && self
                .drawings
                .sync(charts, total, &self.groups, pointer_down)
        {
            ui.ctx().request_repaint();
        }
        #[cfg(feature = "drawings")]
        if Chart::share_stored_drawings(charts) {
            ui.ctx().request_repaint();
        }
        for (index, chart) in charts.iter().enumerate() {
            self.previous[index] = Some((chart.symbol().to_owned(), chart.timeframe()));
            self.previous_bars[index] = bars_key(chart);
            if self.pending_ranges[index].is_none()
                && let Some(range) = chart.visible_time_range()
            {
                self.previous_ranges[index] = Some(range);
            }
        }
        response
    }

    fn restore_time_range(&mut self, index: usize, chart: &mut Chart) {
        // Reloads can leave a chart empty for several frames, and a host without a feed replaces
        // the old bars a frame or more after the switch; keep UTC edges until new bars arrive.
        if let Some(pending) = self.pending_ranges[index]
            && chart.visible_time_range().is_some()
            && bars_key(chart) != pending.bars
        {
            // A range the new bars do not cover keeps the view, but must not block the group.
            chart.apply_synced_time_range_from(pending.range, pending.source);
            self.pending_ranges[index] = None;
        }
    }

    fn sync_state(&mut self, charts: &mut [&mut Chart], count: usize) {
        let source = &charts[self.active];
        let symbol = source.symbol().to_owned();
        let timeframe = source.timeframe();
        let previous = self.previous[self.active].as_ref();
        let symbol_changed = previous.is_some_and(|p| p.0 != symbol);
        let timeframe_changed = previous.is_some_and(|p| p.1 != timeframe);
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            if index == self.active || !self.same_group(index, self.active) {
                continue;
            }
            if self.sync.sync_symbol && symbol_changed {
                chart.set_symbol(&symbol);
            }
            if self.sync.sync_timeframe
                && timeframe_changed
                && let Some(tf) = timeframe
            {
                chart.set_timeframe(tf);
            }
        }
    }

    fn sync_view(&mut self, charts: &mut [&mut Chart], count: usize, sync_range: bool) {
        let hovered = charts
            .iter()
            .take(count)
            .position(|c| c.hover_time().is_some());
        let period = hovered.and_then(|source| charts[source].hover_period());
        // The source's main-pane price, or `None` over volume and indicator panes.
        let price = hovered.and_then(|source| charts[source].last_crosshair?.1);
        let range = charts[self.active].visible_time_range();
        let source = charts[self.active].sync_source();
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            chart.set_synced_crosshair(None, None);
            if self.sync.sync_crosshair
                && let Some(source) = hovered
                && index != source
                && self.same_group(index, source)
            {
                chart.set_synced_crosshair(period, price);
            }
            if index != self.active
                && self.same_group(index, self.active)
                && self.sync.sync_time_axis
                && self.pending_ranges[self.active].is_none()
                && sync_range
                && let Some(range) = range
            {
                // A chart awaiting its new interval's bars fits the latest range once they arrive.
                match &mut self.pending_ranges[index] {
                    Some(pending) => {
                        pending.range = range;
                        pending.source = source;
                    }
                    None => {
                        chart.apply_synced_time_range_from(range, source);
                    }
                }
            }
        }
    }

    fn rects(&self, rect: Rect, count: usize) -> Vec<Rect> {
        if self.mode == ChartLayoutMode::Tabs {
            return vec![rect; count];
        }
        if let Some(slot) = self.maximized {
            return (0..count)
                .map(|index| if index == slot { rect } else { Rect::NOTHING })
                .collect();
        }
        let (by_rows, tracks) = match self.mode.tracks() {
            Tracks::Rows(tracks) => (true, tracks),
            Tracks::Columns(tracks) => (false, tracks),
        };
        // Fewer charts than the preset holds fill its first rows or columns; empty ones go.
        let tracks: Vec<usize> = tracks
            .iter()
            .scan(count, |left, &cells| {
                let cells = cells.min(*left);
                *left -= cells;
                (cells > 0).then_some(cells)
            })
            .collect();
        let mut rects = Vec::with_capacity(count);
        for (track, &cells) in tracks.iter().enumerate() {
            for cell in 0..cells {
                let (column, columns, row, rows) = if by_rows {
                    (cell, cells, track, tracks.len())
                } else {
                    (track, tracks.len(), cell, cells)
                };
                let width = rect.width() / columns as f32;
                let height = ((rect.height() - 2.0 * (rows - 1) as f32) / rows as f32).max(0.0);
                rects.push(Rect::from_min_size(
                    rect.min + Vec2::new(column as f32 * width, row as f32 * (height + 2.0)),
                    Vec2::new(width, height),
                ));
            }
        }
        rects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData};
    use chrono::{DateTime, Duration};
    use egui::{Event, PointerButton, Pos2, RawInput, pos2, vec2};

    fn chart(step: i64) -> Chart {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut chart = Chart::new(BarData::from_bars(
            (0..100)
                .map(|i| {
                    Bar::new(
                        start + Duration::seconds(i * step),
                        10.0,
                        11.0,
                        9.0,
                        10.0,
                        1.0,
                    )
                })
                .collect(),
        ));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Min1);
        chart
    }

    /// Only the active and hovered charts expose their scale settings in a layout.
    #[cfg(feature = "icons")]
    #[test]
    fn scale_settings_follow_layout_activity_and_hover() {
        for (mode, maximized, hovered, expected) in [
            (ChartLayoutMode::Single, None, false, 1),
            (ChartLayoutMode::Four, None, false, 1),
            (ChartLayoutMode::Four, None, true, 2),
            (ChartLayoutMode::Tabs, None, false, 1),
            (ChartLayoutMode::Four, Some(2), false, 1),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            let mut layout = ChartLayout {
                mode,
                maximized,
                ..Default::default()
            };
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                        events: if hovered {
                            vec![Event::PointerMoved(pos2(900.0, 200.0))]
                        } else {
                            vec![Event::PointerGone]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                );
                output.textures_delta.clear();
                let gears = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.label() == Some("Scale settings"))
                    .count();
                assert_eq!(
                    gears, expected,
                    "{mode:?}, maximized={maximized:?}, hovered={hovered}"
                );
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn default_syncs_time_axis_and_crosshair_but_not_drawings() {
        let options = ChartSyncOptions::default();
        assert!(options.sync_time_axis);
        assert!(options.sync_crosshair);
        assert!(!options.sync_drawings);
    }

    #[test]
    fn presets_preserve_chart_state_and_route_input_to_visible_slots() {
        let rect = Rect::from_min_size(pos2(54.0, 70.0), vec2(1386.0, 716.0));
        let mut labels = std::collections::HashSet::new();
        for &mode in ChartLayoutMode::all() {
            assert!(labels.insert(mode.label()), "{mode:?}");
            let layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let rects = layout.rects(rect, mode.chart_count());
            assert_eq!(rects.len(), mode.chart_count());
            if mode == ChartLayoutMode::Tabs {
                continue;
            }
            let area: f32 = rects.iter().map(Rect::area).sum();
            // Each cell above the bottom edge leaves a two-point gap below it.
            let gaps: f32 = rects
                .iter()
                .filter(|cell| cell.bottom() < rect.bottom() - 1.0)
                .map(|cell| 2.0 * cell.width())
                .sum();
            assert!((area + gaps - rect.area()).abs() < 1.0, "{mode:?}");
            for (index, cell) in rects.iter().enumerate() {
                assert!(rect.contains_rect(*cell));
                assert!(
                    rects[..index]
                        .iter()
                        .all(|other| other.intersect(*cell).area() == 0.0)
                );
            }
            if mode == ChartLayoutMode::Four {
                assert_eq!(rects[0], Rect::from_min_size(rect.min, vec2(693.0, 357.0)));
                assert_eq!(rects[2].top(), 429.0);
                assert_eq!(rects[1].left(), 747.0);
            }
            if mode == ChartLayoutMode::ThreeLargeLeft {
                assert_eq!(
                    rects,
                    [
                        Rect::from_min_size(rect.min, vec2(693.0, 716.0)),
                        Rect::from_min_size(pos2(747.0, 70.0), vec2(693.0, 357.0)),
                        Rect::from_min_size(pos2(747.0, 429.0), vec2(693.0, 357.0)),
                    ]
                );
            }
        }
        // Fewer charts than a preset holds fill its first rows, which share the full height.
        let sixteen = ChartLayout {
            mode: ChartLayoutMode::Sixteen,
            ..Default::default()
        };
        assert_eq!(
            sixteen.rects(rect, 6)[5],
            Rect::from_min_size(pos2(747.0, 429.0), vec2(693.0, 357.0))
        );
        let ctx = egui::Context::default();
        for width in [80.0, 240.0] {
            let clip = Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0));
            let output = ctx.run_ui(RawInput::default(), |ui| {
                ui.set_clip_rect(clip);
                ChartLayout::default().show(ui, &mut [&mut chart(60)]);
            });
            let clipped = output.shapes.iter().all(|shape| {
                !matches!(shape.shape, egui::Shape::Text(_)) || clip.contains_rect(shape.clip_rect)
            });
            output.drop_without_applying_deltas();
            assert!(clipped, "a chart must respect its host's clip");
        }
        let mut charts = [chart(60), chart(120)];
        for chart in &mut charts {
            chart.set_symbol("LONG_SYMBOL_".repeat(12));
        }
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Tabs,
            ..Default::default()
        };
        layout.set_active(1);
        let frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    assert!(
                        ui.min_rect().right() <= 640.0,
                        "long chart tabs must scroll inside the container"
                    );
                },
            );
            let group = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Sync group" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            group
        };
        frame(&mut layout, &mut charts, vec![]);
        let point = charts[1].get_rendered_price_rect().center();
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(
            layout.active(),
            1,
            "clicking the tab's plot must not activate slot zero"
        );
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        layout.mode = ChartLayoutMode::TwoHorizontal;
        frame(&mut layout, &mut charts, vec![]);
        let point = charts[0].get_rendered_price_rect().center();
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(layout.active(), 0);
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        // A group editor can overlap the next panel without activating that panel.
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let corner = pos2(308.0, 12.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
        frame(&mut layout, &mut charts, vec![click(corner, true)]);
        frame(&mut layout, &mut charts, vec![click(corner, false)]);
        frame(&mut layout, &mut charts, vec![]);
        let input =
            frame(&mut layout, &mut charts, vec![]).expect("group editor") + vec2(100.0, 18.0);
        assert!(input.x > 320.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(input)]);
        frame(&mut layout, &mut charts, vec![click(input, true)]);
        frame(&mut layout, &mut charts, vec![click(input, false)]);
        frame(&mut layout, &mut charts, vec![Event::Text("Desk".into())]);
        assert_eq!(layout.sync_group(0), Some("Desk"));
        assert_eq!(layout.active(), 0);
        let escape = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        frame(&mut layout, &mut charts, vec![escape]);
        layout.set_sync_group(0, None);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(point)]);
        frame(&mut layout, &mut charts, vec![click(point, true)]);
        frame(&mut layout, &mut charts, vec![click(point, false)]);
        layout.sync.sync_time_axis = false;
        for chart in &mut charts {
            chart.apply_synced_time_scale(3.0, 0.0);
        }
        #[cfg(feature = "drawings")]
        {
            // Delete applies only to the chart that received the pointer focus.
            for chart in &mut charts {
                let mut drawing = crate::drawings::Drawing::new(
                    0,
                    crate::drawings::DrawingToolType::HorizontalLine,
                );
                drawing.completed = true;
                drawing
                    .chart_points
                    .push(crate::drawings::ChartPoint::new(10.0, 10.0));
                chart.drawings_mut().add_synced_drawing(drawing).unwrap();
                let id = chart.drawings().drawings[0].id;
                chart.drawings_mut().select(id);
            }
            frame(
                &mut layout,
                &mut charts,
                vec![Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(charts[0].drawings().drawings.is_empty());
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            let id = charts[1].drawings().drawings[0].id;
            charts[1].drawings_mut().delete_drawing(id);
        }

        // Hidden chart tabs must not remain crosshair sources.
        charts[0].last_hover_bar_idx = Some(10.0);
        layout.mode = ChartLayoutMode::Tabs;
        layout.set_active(1);
        frame(&mut layout, &mut charts, vec![Event::PointerGone]);
        assert_eq!(charts[0].hover_time(), None);
        assert_eq!(charts[1].synced_crosshair_bar_idx, None);

        #[cfg(feature = "drawings")]
        {
            // Preset changes retain links, including edits and deletions while a slot is hidden.
            layout.mode = ChartLayoutMode::TwoHorizontal;
            layout.sync.sync_drawings = true;
            let mut drawing =
                crate::drawings::Drawing::new(0, crate::drawings::DrawingToolType::HorizontalLine);
            drawing.completed = true;
            drawing
                .chart_points
                .push(crate::drawings::ChartPoint::new(10.0, 10.0));
            charts[0]
                .drawings_mut()
                .add_synced_drawing(drawing)
                .unwrap();
            frame(&mut layout, &mut charts, vec![]);
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            layout.mode = ChartLayoutMode::Single;
            frame(&mut layout, &mut charts, vec![]);
            layout.mode = ChartLayoutMode::TwoHorizontal;
            frame(&mut layout, &mut charts, vec![]);
            assert_eq!(charts[0].drawings().drawings.len(), 1);
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            layout.mode = ChartLayoutMode::Single;
            let id = charts[0].drawings().drawings[0].id;
            charts[0].drawings_mut().delete_drawing(id);
            frame(&mut layout, &mut charts, vec![]);
            layout.mode = ChartLayoutMode::TwoHorizontal;
            frame(&mut layout, &mut charts, vec![]);
            assert!(charts[1].drawings().drawings.is_empty());
        }

        layout.set_detached(1, true);
        frame(&mut layout, &mut charts, vec![]);
        assert_eq!(charts[1].bars(super::super::SeriesId::MAIN).len(), 100);
        layout.set_detached(1, false);
        frame(&mut layout, &mut charts, vec![]);
        assert!(!layout.is_detached(1));
    }

    #[test]
    fn sync_switches_and_groups_isolate_changes() {
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(120), chart(60), chart(60)];
        let mut refs: Vec<_> = charts.iter_mut().collect();
        layout.set_sync_group(2, Some("B".into()));
        layout.previous[0] = Some(("SAMPLE".into(), Some(Timeframe::Min1)));
        refs[0].set_symbol("OTHER");
        refs[0].set_timeframe(Timeframe::Hour1);
        layout.sync_state(&mut refs, 4);
        assert_eq!(refs[1].symbol(), "SAMPLE");
        layout.sync.sync_symbol = true;
        layout.sync.sync_timeframe = true;
        layout.sync_state(&mut refs, 4);
        assert_eq!(
            (refs[1].symbol(), refs[1].timeframe()),
            ("OTHER", Some(Timeframe::Hour1))
        );
        assert_eq!(
            (refs[2].symbol(), refs[2].timeframe()),
            ("SAMPLE", Some(Timeframe::Min1))
        );
        // The hovered chart emits crosshairs without becoming the selected chart.
        refs[1].last_hover_bar_idx = Some(2.0);
        let time = refs[1].hover_time().unwrap();
        refs[1].last_crosshair = Some((time, Some(10.5)));
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, Some(4.0));
        assert_eq!(refs[0].synced_crosshair_price, Some(10.5));
        assert_eq!(refs[2].synced_crosshair_bar_idx, None);
        refs[1].last_hover_bar_idx = None;
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, None);
        assert_eq!(refs[0].synced_crosshair_price, None);
        refs[1].last_hover_bar_idx = Some(2.0);
        layout.sync.sync_crosshair = false;
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, None);
        let range = super::super::VisibleTimeRange {
            from: refs[0].state.shown()[10].time,
            to: refs[0].state.shown()[30].time,
        };
        let before = refs[1].get_time_scale_state();
        layout.apply_date_range(&mut refs, range);
        assert_eq!(refs[1].get_time_scale_state(), before);
        layout.sync.sync_date_range = true;
        let isolated = refs[2].get_time_scale_state();
        layout.apply_date_range(&mut refs, range);
        assert_ne!(refs[1].get_time_scale_state(), before);
        assert_eq!(refs[2].get_time_scale_state(), isolated);

        // A minute chart at the spacing floor must retain its UTC view after an hourly reload.
        let ctx = egui::Context::default();
        for mode in [ChartLayoutMode::Four, ChartLayoutMode::Tabs] {
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            let frame = |layout: &mut ChartLayout, charts: &mut [Chart; 4]| {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                )
                .drop_without_applying_deltas();
            };
            frame(&mut layout, &mut charts);
            for active in [0, 1] {
                layout.set_active(active);
                frame(&mut layout, &mut charts);
                charts[active].apply_synced_time_scale(0.5, 0.0);
                frame(&mut layout, &mut charts);
                let remembered = charts[active].visible_time_range().unwrap();
                charts[active].set_timeframe(Timeframe::Hour1);
                charts[active].set_data(super::super::SeriesId::MAIN, vec![]);
                frame(&mut layout, &mut charts);
                let others: Vec<_> = charts.iter().map(Chart::get_time_scale_state).collect();
                frame(&mut layout, &mut charts);
                for (index, chart) in charts.iter().enumerate() {
                    if index != active {
                        assert_eq!(chart.get_time_scale_state(), others[index]);
                    }
                }
                charts[active].set_data(
                    super::super::SeriesId::MAIN,
                    chart(3600).state.shown().to_vec(),
                );
                frame(&mut layout, &mut charts);
                // A 60x longer interval needs 60x the spacing for the same UTC span.
                assert!((charts[active].get_time_scale_state().0 - 30.0).abs() < 0.01);
                let actual = charts[active].visible_time_range().unwrap();
                assert!((actual.from - remembered.from).num_milliseconds().abs() < 1);
                assert!((actual.to - remembered.to).num_milliseconds().abs() < 1);
            }
            layout.set_active(0);
            charts[0].set_visible_bars(65);
            frame(&mut layout, &mut charts);
            for chart in &charts {
                let scale = chart.state.time_scale();
                let range = scale.visible_logical_range();
                assert!(scale.bar_spacing().is_finite() && scale.bar_spacing() >= 0.5);
                assert!(range.left.ceil().max(0.0) <= range.right.floor().min(99.0));
            }
            // Bars that miss the remembered range keep the view and must not block the group.
            layout.set_active(2);
            frame(&mut layout, &mut charts);
            charts[2].set_timeframe(Timeframe::Day1);
            charts[2].set_data(super::super::SeriesId::MAIN, vec![]);
            frame(&mut layout, &mut charts);
            let mut later = chart(86_400).state.shown().to_vec();
            later
                .iter_mut()
                .for_each(|bar| bar.time += Duration::days(3650));
            charts[2].set_data(super::super::SeriesId::MAIN, later);
            frame(&mut layout, &mut charts);
            assert!(layout.pending_ranges.iter().all(Option::is_none));

            // A chart still loading a synced interval adopts the group's latest range, so its
            // arrival does not pull the group back to the range from before the switch, also
            // past 50 px per bar and when its old bars are replaced, not emptied, a frame later.
            for emptied in [true, false] {
                let mut layout = ChartLayout {
                    mode,
                    ..Default::default()
                };
                layout.sync.sync_timeframe = true;
                let mut charts = [chart(60), chart(60), chart(60), chart(60)];
                frame(&mut layout, &mut charts);
                charts[0].set_timeframe(Timeframe::Hour1);
                for chart in charts.iter_mut().filter(|_| emptied) {
                    chart.set_data(super::super::SeriesId::MAIN, vec![]);
                }
                frame(&mut layout, &mut charts);
                let hourly = chart(3600).state.shown().to_vec();
                charts[0].set_data(super::super::SeriesId::MAIN, hourly.clone());
                frame(&mut layout, &mut charts);
                charts[0].set_visible_bars(8);
                frame(&mut layout, &mut charts);
                assert!(charts[0].get_time_scale_state().0 > 50.0);
                let latest = charts[0].visible_time_range().unwrap();
                charts[1].set_data(super::super::SeriesId::MAIN, hourly);
                layout.set_active(1);
                frame(&mut layout, &mut charts);
                for chart in &charts[..2] {
                    let actual = chart.visible_time_range().unwrap();
                    assert!((actual.from - latest.from).num_milliseconds().abs() < 1);
                    assert!((actual.to - latest.to).num_milliseconds().abs() < 1);
                }
            }

            // Charts of one interval zoom together past the cross-interval spacing cap.
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            frame(&mut layout, &mut charts);
            charts[0].set_visible_bars(8);
            frame(&mut layout, &mut charts);
            assert!(charts[0].get_time_scale_state().0 > 72.0);
            let source = charts[0].visible_time_range().unwrap();
            for chart in &charts[1..] {
                let actual = chart.visible_time_range().unwrap();
                assert!((actual.from - source.from).num_milliseconds().abs() < 1);
                assert!((actual.to - source.to).num_milliseconds().abs() < 1);
            }
        }
    }

    /// Alt+Enter shows the focused tile alone and a second press brings the preset back with the
    /// tile's slot, viewport and synchronization group unchanged.
    #[test]
    fn alt_enter_maximizes_and_restores_the_active_tile() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(120), chart(60), chart(60)];
        layout.set_sync_group(2, Some("B".into()));
        layout.set_active(2);
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 4], pressed: bool| {
            time += 0.1;
            let events = if pressed {
                vec![Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                }]
            } else {
                vec![]
            };
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                },
            )
            .drop_without_applying_deltas();
        };
        for _ in 0..3 {
            frame(&mut layout, &mut charts, false);
        }
        let tile = charts[2].get_rendered_price_rect();
        let viewport = charts[2].get_time_scale_state();
        frame(&mut layout, &mut charts, true);
        // The chart's widget id follows its rect, so focus moves over to it on the next frames.
        frame(&mut layout, &mut charts, false);
        frame(&mut layout, &mut charts, false);
        assert_eq!((layout.maximized, layout.active()), (Some(2), 2));
        assert!(charts[2].get_rendered_price_rect().width() > tile.width() * 1.8);
        assert_eq!(layout.sync_group(2), Some("B"));
        assert!(charts[2].take_shortcut_requests().is_empty());
        frame(&mut layout, &mut charts, true);
        frame(&mut layout, &mut charts, false);
        frame(&mut layout, &mut charts, false);
        assert_eq!((layout.maximized, layout.active()), (None, 2));
        assert_eq!(charts[2].get_rendered_price_rect(), tile);
        assert_eq!(charts[2].get_time_scale_state(), viewport);
    }
}
