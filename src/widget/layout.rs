//! Tiled and tabbed charts with independent state and timestamp-based synchronization.

use super::Chart;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use egui::{Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[cfg(feature = "drawings")]
mod drawings;
#[cfg(feature = "drawings")]
use drawings::DrawingLinks;

/// Multi-chart arrangement. Changing presets retains the host's hidden charts.
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
    /// Two columns and two rows.
    Four,
    /// Five charts in three columns.
    Five,
    /// Three columns and two rows.
    Six,
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
    /// All selectable presets.
    pub fn all() -> &'static [Self] {
        &[
            Self::Single,
            Self::TwoHorizontal,
            Self::TwoVertical,
            Self::Three,
            Self::Four,
            Self::Five,
            Self::Six,
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

    /// Human-readable preset name.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::TwoHorizontal => "2 Horizontal",
            Self::TwoVertical => "2 Vertical",
            Self::Three => "3 Charts",
            Self::Four => "2x2 Grid",
            Self::Five => "5 Charts",
            Self::Six => "3x2 Grid",
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
        match self {
            Self::Single => 1,
            Self::TwoHorizontal | Self::TwoVertical => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten => 10,
            Self::Eleven => 11,
            Self::Twelve => 12,
            Self::Thirteen => 13,
            Self::Fourteen => 14,
            Self::Fifteen => 15,
            Self::Sixteen | Self::Tabs => 16,
        }
    }

    /// Maximum columns and rows. An incomplete final row fills the available width.
    pub fn grid_size(&self) -> (usize, usize) {
        match self {
            Self::Single | Self::Tabs => (1, 1),
            Self::TwoHorizontal => (2, 1),
            Self::TwoVertical => (1, 2),
            Self::Three | Self::Four => (2, 2),
            Self::Five | Self::Six => (3, 2),
            Self::Seven | Self::Eight => (4, 2),
            Self::Nine => (3, 3),
            Self::Ten | Self::Eleven | Self::Twelve => (4, 3),
            _ => (4, 4),
        }
    }
}

impl std::fmt::Display for ChartLayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
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
/// grid preset. The container does not acquire data or persist it.
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
    focus_active: bool,
    groups: [Option<String>; 16],
    detached: [bool; 16],
    previous: [Option<(String, Option<Timeframe>)>; 16],
    #[cfg(feature = "drawings")]
    drawings: DrawingLinks,
    previous_preset: Option<(ChartLayoutMode, usize)>,
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
        for (index, chart) in charts.iter_mut().take(16).enumerate() {
            if index == self.active
                || (self.sync.sync_date_range && self.same_group(index, self.active))
            {
                chart.apply_synced_time_range(range);
            }
        }
    }

    fn same_group(&self, a: usize, b: usize) -> bool {
        self.groups[a] == self.groups[b]
    }

    /// Shows the preset and synchronizes its charts. Click or scroll a panel to activate it.
    /// Its corner menu offers detachment and named groups. The active border is painted
    /// inside the chart; rows have a two-point gap and columns meet at their edge.
    /// Extra charts remain owned by the host and are not painted by a grid preset.
    pub fn show(&mut self, ui: &mut Ui, charts: &mut [&mut Chart]) -> Response {
        let total = charts.len().min(16);
        let charts = &mut charts[..total];
        let count = total.min(self.mode.chart_count());
        if count == 0 {
            return ui.allocate_response(Vec2::ZERO, Sense::hover());
        }
        self.active = self.active.min(count - 1);
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
                self.detached[index] = !close;
                if activate {
                    self.active = index;
                }
                if self.mode != ChartLayoutMode::Tabs || index == self.active {
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
            if count > 1 && index == self.active {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(2.0, ui.active_theme().ui.accent),
                    StrokeKind::Inside,
                );
            }
        }
        self.focus_active = false;
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
        for (index, chart) in charts.iter().enumerate() {
            self.previous[index] = Some((chart.symbol().to_owned(), chart.timeframe()));
        }
        response
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

    fn sync_view(&self, charts: &mut [&mut Chart], count: usize, sync_range: bool) {
        let hovered = charts
            .iter()
            .take(count)
            .position(|c| c.hover_time().is_some());
        let time = hovered.and_then(|source| charts[source].hover_time());
        let range = charts[self.active].visible_time_range();
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            chart.set_synced_crosshair_time(None);
            if self.sync.sync_crosshair
                && let Some(source) = hovered
                && index != source
                && self.same_group(index, source)
            {
                chart.set_synced_crosshair_time(time);
            }
            if index != self.active
                && self.same_group(index, self.active)
                && self.sync.sync_time_axis
                && sync_range
                && let Some(range) = range
            {
                chart.apply_synced_time_range(range);
            }
        }
    }

    fn rects(&self, rect: Rect, count: usize) -> Vec<Rect> {
        if self.mode == ChartLayoutMode::Tabs {
            return vec![rect; count];
        }
        let (columns, _) = self.mode.grid_size();
        let columns = columns.min(count);
        let rows = count.div_ceil(columns);
        let height = ((rect.height() - 2.0 * (rows - 1) as f32) / rows as f32).max(0.0);
        (0..count)
            .map(|index| {
                let row = index / columns;
                let row_columns = columns.min(count - row * columns);
                let width = rect.width() / row_columns as f32;
                Rect::from_min_size(
                    rect.min
                        + Vec2::new(
                            (index % columns) as f32 * width,
                            row as f32 * (height + 2.0),
                        ),
                    Vec2::new(width, height),
                )
            })
            .collect()
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

    #[test]
    fn presets_preserve_chart_state_and_route_input_to_visible_slots() {
        let rect = Rect::from_min_size(pos2(54.0, 70.0), vec2(1386.0, 716.0));
        for &mode in ChartLayoutMode::all() {
            let layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let rects = layout.rects(rect, mode.chart_count());
            assert_eq!(rects.len(), mode.chart_count());
            if mode == ChartLayoutMode::Tabs {
                continue;
            }
            let (_, rows) = mode.grid_size();
            let area: f32 = rects.iter().map(Rect::area).sum();
            assert!(
                (area - rect.area() + 2.0 * (rows - 1) as f32 * rect.width()).abs() < 1.0,
                "{mode:?}"
            );
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
        }
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
                chart.drawings_mut().add_synced_drawing(drawing);
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
            charts[0].drawings_mut().add_synced_drawing(drawing);
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
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, Some(4.0));
        assert_eq!(refs[2].synced_crosshair_bar_idx, None);
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
    }
}
