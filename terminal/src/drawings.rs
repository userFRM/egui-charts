//! Drawings: the drawing toolbar, the object tree, the data window and drawing properties.

use super::core::{Direction, SidePanel};
use super::layout::{Region, panel_frame};
use super::{Action, Terminal};
use egui_charts::config::{CrosshairMode, CrosshairStyle};
use egui_charts::drawings::{DrawingTemplate, DrawingToolType};
use egui_charts::services::UserSettings;
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{
    DrawingContextMenuAction, DrawingId, DrawingPropertiesAction, DrawingPropertiesDialog,
    LocalCopiesDialog, StudyContextMenuAction,
};
use egui_charts::ui::drawing_toolbar::categories::cursors::CursorType;
use egui_charts::ui::drawing_toolbar::{
    DrawingToolbar, DrawingToolbarAction, DrawingToolbarDomainState, MagnetType,
};
use egui_charts::ui::floating_toolbar::FloatingSelectionToolbar;
use egui_charts::ui::widget_bar::panels::object_tree::{
    DataWindowInfo, ObjectTreeAction, ObjectTreePanel, SourceItem, show_data_window,
};
use egui_charts::ui::widget_bar::{RightBarIcon, WidgetBarAction};
use egui_charts::widget::{ChartEvent, ChartPointer, SeriesId, ShortcutRequest};

/// Object tree id of the main series.
const MAIN_SERIES_ID: usize = usize::MAX;
/// Object tree ids of studies start here, above any drawing id, and end below the main series.
const STUDY_ID_BASE: usize = usize::MAX / 2;

/// What a drawing clipboard text starts with.
const DRAWINGS_CLIPBOARD: &str = "egui-charts-drawings\n";

fn study_tree_id(id: SeriesId) -> usize {
    STUDY_ID_BASE + id.0
}

fn study_from_tree_id(id: usize) -> Option<SeriesId> {
    if id == MAIN_SERIES_ID {
        None
    } else {
        id.checked_sub(STUDY_ID_BASE).map(SeriesId)
    }
}

#[derive(Default)]
pub(super) struct State {
    toolbar: DrawingToolbar,
    selection_toolbar: FloatingSelectionToolbar,
    object_tree: ObjectTreePanel,
    properties: DrawingPropertiesDialog,
    /// The copies of the active chart's drawings that the user kept in a conflict.
    local_copies: LocalCopiesDialog,
    /// Whether the data window is folded in its own panel.
    data_window_folded: bool,
    /// The last pointer position a chart reported, with its slot, which the data window reads
    /// while that chart is the active one.
    crosshair: Option<(usize, ChartPointer)>,
    /// A colour picker drag of the object tree is in progress.
    colour_drag: bool,
}

impl Terminal {
    pub(super) fn drawings_init(&mut self) {
        self.set_control_visible(Control::ObjectTree, true);
        // The rows of positions, orders and templates belong to features the terminal performs
        // in other groups; they stay hidden until those take them over.
        for control in [Control::Trading, Control::Templates] {
            self.drawings.toolbar.set_control_visible(control, false);
        }
    }

    pub(super) fn drawings_poll(&mut self, ctx: &egui::Context) {
        // A colour picker drag ends, as one edit, when the pointer is released.
        if self.drawings.colour_drag && !ctx.input(|i| i.pointer.any_down()) {
            self.drawings.colour_drag = false;
            self.chart_mut()
                .user_action(|chart| chart.drawings_mut().end_drag_handle());
        }
    }

    pub(super) fn drawings_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Left => self.show_drawing_toolbar(ui),
            Region::Right => self.show_drawings_panel(ui),
            Region::Chart => self.read_crosshair(ui.ctx()),
            Region::Windows => {
                if !self.keyboard_dialog_open()
                    && let Some(action) = self
                        .drawings
                        .selection_toolbar
                        .show_and_apply(ui, &mut self.charts[self.active])
                {
                    self.emit(Action::ChartMenu(ChartContextAction::Drawing(action)));
                }
                self.show_drawing_properties(ui.ctx());
                self.drawings
                    .local_copies
                    .show(ui.ctx(), &mut self.charts[self.active]);
            }
            Region::Top | Region::Bottom => {}
        }
    }

    pub(super) fn drawings_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::DrawingToolbar(action) => self.perform_drawing_toolbar(action),
            Action::ObjectTree(action) => self.perform_object_tree(action),
            Action::DrawingProperties(action) => {
                match action {
                    DrawingPropertiesAction::Delete(id) => {
                        self.chart_mut()
                            .user_action(|chart| chart.drawings_mut().delete_drawing(id.0));
                    }
                    DrawingPropertiesAction::Clone(id) => self.clone_drawing(id.0),
                    // Apply is applied by the dialog when it is shown.
                    _ => {}
                }
                None
            }
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::ObjectTree)) => {
                self.open_panel(SidePanel::ObjectTree);
                None
            }
            Action::ChartMenu(ChartContextAction::Study {
                id,
                action: StudyContextMenuAction::ObjectTree,
            }) => {
                self.open_panel(SidePanel::ObjectTree);
                self.drawings
                    .object_tree
                    .state_mut()
                    .select(study_tree_id(id));
                None
            }
            // The alert dialog belongs to the alerts group.
            action @ Action::ChartMenu(ChartContextAction::Drawing(
                DrawingContextMenuAction::AddAlert(_),
            )) => Some(action),
            Action::ChartMenu(ChartContextAction::Drawing(action)) => {
                self.user_action(|terminal| terminal.perform_drawing_menu(action));
                None
            }
            Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::ObjectTree)) => {
                if self.panel == Some(SidePanel::ObjectTree) {
                    self.panel = None;
                } else {
                    self.open_panel(SidePanel::ObjectTree);
                }
                None
            }
            // The study clipboard belongs to the commands group.
            Action::Shortcut(ShortcutRequest::PasteDrawing(text))
                if text.starts_with(DRAWINGS_CLIPBOARD) =>
            {
                if let Err(error) = self.chart_mut().paste_drawings(&text) {
                    self.toasts
                        .error(format!("Could not paste drawings: {error}"));
                }
                None
            }
            action => Some(action),
        }
    }

    pub(super) fn drawings_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        match direction {
            Direction::Load => {
                let defaults: Vec<_> = settings
                    .drawing_defaults
                    .iter()
                    .filter_map(|(tool, template)| {
                        Some((
                            serde_json::from_value::<DrawingToolType>(tool.clone().into()).ok()?,
                            serde_json::from_value::<DrawingTemplate>(template.clone()).ok()?,
                        ))
                    })
                    .collect();
                for chart in &mut self.charts {
                    chart.drawings_mut().tool_defaults = defaults.iter().cloned().collect();
                }
                self.drawings.toolbar.state.magnet_hint_dismissed |= settings.magnet_hint_dismissed;
            }
            Direction::Save => {
                settings.magnet_hint_dismissed = self.drawings.toolbar.state.magnet_hint_dismissed;
                settings.drawing_defaults = self
                    .chart()
                    .drawings()
                    .tool_defaults
                    .iter()
                    .filter_map(|(tool, template)| {
                        let serde_json::Value::String(tool) = serde_json::to_value(tool).ok()?
                        else {
                            return None;
                        };
                        Some((tool, serde_json::to_value(template).ok()?))
                    })
                    .collect();
            }
        }
    }

    pub(super) fn drawings_modal(&self) -> bool {
        self.drawings.properties.is_open || self.drawings.local_copies.is_open()
    }

    /// Offers the copies kept of the drawings of `symbol`, if the active chart shows it.
    pub(super) fn offer_local_copies(&mut self, symbol: &str) {
        let chart = &mut self.charts[self.active];
        if chart.symbol() == symbol {
            self.drawings.local_copies.open(chart);
        }
    }

    /// Shows the drawing toolbar in line with the active chart and queues what it emitted.
    fn show_drawing_toolbar(&mut self, ui: &mut egui::Ui) {
        let chart = &self.charts[self.active];
        let toolbar = &mut self.drawings.toolbar;
        if chart.zoom_was_applied() {
            toolbar.state.zoom_mode_active = false;
        }
        toolbar.state.has_zoom_history = chart.state().has_zoom_history();
        let manager = chart.drawings();
        let magnet_type = if manager.options.strong_magnet {
            MagnetType::Strong
        } else {
            MagnetType::Weak
        };
        toolbar.sync_from_domain_state(&DrawingToolbarDomainState {
            sel_tool: manager.active_tool(),
            magnet_mode: manager.options.magnet_mode,
            snap_to_indicators: manager.options.snap_to_indicators,
            magnet_type,
            current_cursor_type: match chart.chart_options().crosshair.style {
                CrosshairStyle::Full => CursorType::Cross,
                CrosshairStyle::Dot => CursorType::Dot,
                CrosshairStyle::Arrow | CrosshairStyle::Hidden => CursorType::Arrow,
                CrosshairStyle::Demonstration => CursorType::Demonstration,
                CrosshairStyle::Magic => CursorType::Magic,
            },
            eraser_mode_enabled: manager.is_eraser_mode(),
            stay_in_drawing_mode: manager.stay_in_drawing_mode,
            // The swatch shows the colour the active tool draws in, its saved default or its own.
            drawing_color: manager
                .new_drawing(
                    0,
                    manager.active_tool().unwrap_or(DrawingToolType::TrendLine),
                )
                .color,
        });
        let drawings = &manager.drawings;
        toolbar.state.lock_all = !drawings.is_empty() && drawings.iter().all(|d| d.locked);
        toolbar.state.hide_drawings = !drawings.is_empty() && drawings.iter().all(|d| !d.visible);
        let bg = ui.style().visuals.panel_fill;
        let mut action = DrawingToolbarAction::None;
        egui::Panel::left("drawing_toolbar")
            .resizable(false)
            .exact_size(54.0)
            .show_separator_line(false)
            .frame(
                panel_frame(bg)
                    .corner_radius(egui::CornerRadius {
                        ne: 4,
                        ..egui::CornerRadius::ZERO
                    })
                    .outer_margin(egui::Margin {
                        right: 4,
                        ..egui::Margin::ZERO
                    }),
            )
            .show(ui, |ui| action = toolbar.show_with_action(ui));
        if !matches!(action, DrawingToolbarAction::None) {
            self.emit(Action::DrawingToolbar(action));
        }
    }

    /// Performs a drawing toolbar action. Zoom is a view change and stays outside undo. The
    /// actions on studies, positions, orders and templates are left for their groups.
    fn perform_drawing_toolbar(&mut self, action: DrawingToolbarAction) -> Option<Action> {
        match action {
            DrawingToolbarAction::ZoomIn => {
                let active = self.drawings.toolbar.state.zoom_mode_active;
                self.chart_mut().set_zoom_mode(active);
            }
            DrawingToolbarAction::ZoomOut => {
                self.chart_mut().zoom_back();
            }
            DrawingToolbarAction::OpenTemplateMenu
            | DrawingToolbarAction::ManageTemplates
            | DrawingToolbarAction::SaveTemplate { .. }
            | DrawingToolbarAction::LoadTemplate { .. }
            | DrawingToolbarAction::HideAllPoss
            | DrawingToolbarAction::HideAllOrders
            | DrawingToolbarAction::RemoveAllIndicators
            | DrawingToolbarAction::RemoveAllStudies
            | DrawingToolbarAction::HideAllIndicators => {
                return Some(Action::DrawingToolbar(action));
            }
            action => self.user_action(|terminal| terminal.edit_with_drawing_toolbar(action)),
        }
        None
    }

    fn edit_with_drawing_toolbar(&mut self, action: DrawingToolbarAction) {
        let state = &self.drawings.toolbar.state;
        let (magnet_type, values_tooltip) = (state.magnet_type, state.values_tooltip_on_long_press);
        // The toolbar has already flipped these two, so they hold the state the user asked for.
        let (lock_all, hide_all) = (state.lock_all, state.hide_drawings);
        let chart = &mut self.charts[self.active];
        match action {
            DrawingToolbarAction::SetCursorType(cursor) => {
                chart.set_crosshair_style(match cursor {
                    CursorType::Cross => CrosshairStyle::Full,
                    CursorType::Dot => CrosshairStyle::Dot,
                    CursorType::Arrow | CursorType::Eraser => CrosshairStyle::Arrow,
                    CursorType::Demonstration => CrosshairStyle::Demonstration,
                    CursorType::Magic => CrosshairStyle::Magic,
                });
                chart.drawings_mut().set_active_tool(None);
                chart
                    .drawings_mut()
                    .set_eraser_mode(cursor == CursorType::Eraser);
            }
            DrawingToolbarAction::ToggleValuesTooltip => chart.set_tooltip_enabled(values_tooltip),
            DrawingToolbarAction::SelectTool(tool) => {
                chart.drawings_mut().set_active_tool(Some(tool))
            }
            DrawingToolbarAction::ToggleMagnet => {
                let manager = chart.drawings_mut();
                manager.options.magnet_mode = !manager.options.magnet_mode;
                manager.options.strong_magnet = magnet_type != MagnetType::Weak;
                manager.update_snap_options();
                let mode = if manager.options.magnet_mode {
                    CrosshairMode::Magnet
                } else {
                    CrosshairMode::Normal
                };
                chart.set_crosshair_mode(mode);
            }
            DrawingToolbarAction::SetSnapToIndicators(enabled) => {
                chart.drawings_mut().options.snap_to_indicators = enabled;
            }
            DrawingToolbarAction::SetMagnetType(kind) => {
                let manager = chart.drawings_mut();
                manager.options.magnet_mode = true;
                manager.options.strong_magnet = kind != MagnetType::Weak;
                manager.update_snap_options();
                chart.set_crosshair_mode(CrosshairMode::Magnet);
            }
            DrawingToolbarAction::ToggleEraserMode => {
                let manager = chart.drawings_mut();
                let on = !manager.is_eraser_mode();
                manager.set_eraser_mode(on);
            }
            DrawingToolbarAction::ToggleStayInDrawingMode => {
                let manager = chart.drawings_mut();
                manager.stay_in_drawing_mode = !manager.stay_in_drawing_mode;
            }
            DrawingToolbarAction::ColorChanged(rgba) => {
                // The colour becomes the active tool's saved default; other tools keep theirs.
                let manager = chart.drawings_mut();
                if let Some(tool) = manager.active_tool() {
                    let mut drawing = manager.new_drawing(0, tool);
                    drawing.color = rgba;
                    manager
                        .tool_defaults
                        .insert(tool, DrawingTemplate::from_drawing(&drawing));
                }
            }
            DrawingToolbarAction::ClearAllDrawings | DrawingToolbarAction::RemoveAllDrawings => {
                chart.drawings_mut().clear_all();
            }
            DrawingToolbarAction::HideAllDrawings => {
                chart.drawings_mut().set_visibility_all(!hide_all);
            }
            DrawingToolbarAction::LockAllDrawings => {
                let manager = chart.drawings_mut();
                let ids: Vec<_> = manager
                    .drawings
                    .iter()
                    .filter(|drawing| drawing.locked != lock_all)
                    .map(|drawing| drawing.id)
                    .collect();
                manager.edit_drawings(&ids, |drawing| drawing.locked = lock_all);
            }
            DrawingToolbarAction::InsertGlyph { icon_name, size } => {
                let manager = chart.drawings_mut();
                manager.start_icon_insertion(icon_name.to_string());
                if let Some(drawing) = manager.curr_drawing_mut() {
                    drawing.font_size = size;
                }
            }
            DrawingToolbarAction::InsertEmoji { icon_name } => {
                chart
                    .drawings_mut()
                    .start_icon_insertion(icon_name.to_string());
            }
            DrawingToolbarAction::HideAll => {
                chart.drawings_mut().set_visibility_all(false);
                let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    chart.set_indicator_visible(id, false);
                }
            }
            DrawingToolbarAction::RemoveDrawingsAndIndicators => {
                chart.drawings_mut().clear_all();
                let ids: Vec<SeriesId> = chart.indicators().map(|(id, _)| id).collect();
                for id in ids {
                    chart.remove_series(id);
                }
            }
            // Zoom and the actions of other groups never get here.
            _ => {}
        }
    }

    /// Performs a drawing menu action on the drawing the library selected for the menu.
    fn perform_drawing_menu(&mut self, action: DrawingContextMenuAction) {
        let Some(id) = self.chart().drawings().sel_drawing() else {
            return;
        };
        match action {
            DrawingContextMenuAction::Edit => self.open_drawing_properties(id),
            DrawingContextMenuAction::Clone => self.clone_drawing(id),
            DrawingContextMenuAction::Copy => match self.chart().copy_selected_drawings() {
                Ok(Some(text)) => self.ctx().copy_text(text),
                Ok(None) => {}
                Err(error) => self
                    .toasts
                    .error(format!("Could not copy drawings: {error}")),
            },
            DrawingContextMenuAction::BringForward | DrawingContextMenuAction::SendBackward => {
                let action = if action == DrawingContextMenuAction::BringForward {
                    ObjectTreeAction::MoveUp(id)
                } else {
                    ObjectTreeAction::MoveDown(id)
                };
                self.drawings.object_tree.apply_to_chart(
                    &mut self.charts[self.active],
                    &action,
                    &[],
                );
            }
            action => {
                let chart = self.chart_mut();
                let manager = chart.drawings_mut();
                match action {
                    DrawingContextMenuAction::Delete => manager.delete_selected(),
                    DrawingContextMenuAction::Lock => {
                        manager.edit_drawing(id, |d| d.locked = !d.locked)
                    }
                    DrawingContextMenuAction::Hide => {
                        manager.edit_drawing(id, |d| d.visible = !d.visible)
                    }
                    DrawingContextMenuAction::BringToFront => manager.bring_to_front(id),
                    DrawingContextMenuAction::SendToBack => manager.send_to_back(id),
                    DrawingContextMenuAction::MoveTo(pane) => {
                        chart.move_drawing_to_pane(id, pane);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(super) fn open_drawing_properties(&mut self, id: usize) {
        self.drawings
            .properties
            .open_for_chart(&self.charts[self.active], DrawingId(id));
    }

    /// Duplicates drawing `id`, or all selected drawings when it is one of them.
    fn clone_drawing(&mut self, id: usize) {
        self.chart_mut().user_action(|chart| {
            if chart.drawings().is_selected(id) {
                let copies: Vec<_> = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| chart.drawings().is_selected(d.id))
                    .cloned()
                    .collect();
                chart.drawings_mut().paste_drawings(&copies);
            } else {
                chart.drawings_mut().duplicate_drawing(id);
            }
        });
    }

    fn show_drawing_properties(&mut self, ctx: &egui::Context) {
        let chart = &mut self.charts[self.active];
        let action = self.drawings.properties.show_and_apply(ctx, chart);
        if !matches!(action, DrawingPropertiesAction::None) {
            self.emit(Action::DrawingProperties(action));
        }
    }

    /// Shows the object tree or the data window beside the chart.
    fn show_drawings_panel(&mut self, ui: &mut egui::Ui) {
        let panel = match self.panel {
            Some(SidePanel::ObjectTree) if self.shown(Control::ObjectTree) => SidePanel::ObjectTree,
            Some(SidePanel::DataWindow) => SidePanel::DataWindow,
            _ => return,
        };
        let data_window = self.build_data_window();
        let mut sources = self.build_source_items();
        let state = &mut self.drawings;
        let mut action = ObjectTreeAction::None;
        egui::Panel::right("detail_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
            .show(ui, |ui| {
                if panel == SidePanel::ObjectTree {
                    action = state.object_tree.show(ui, Some(&data_window), &mut sources);
                } else {
                    let expanded = show_data_window(ui, &data_window, !state.data_window_folded);
                    state.data_window_folded = !expanded;
                }
            });
        if !matches!(action, ObjectTreeAction::None) {
            self.emit(Action::ObjectTree(action));
        }
    }

    /// The rows of the object tree, read from the active chart.
    fn build_source_items(&self) -> Vec<SourceItem> {
        let chart = self.chart();
        let timeframe = chart.timeframe().map(|tf| tf.as_str()).unwrap_or_default();
        let mut main =
            SourceItem::data_source(MAIN_SERIES_ID, format!("{} · {timeframe}", chart.symbol()))
                .with_series_id(SeriesId::MAIN);
        main.visible = chart.is_main_series_visible();
        let mut items = vec![main];
        for (id, study) in chart.indicators() {
            let color = study
                .colors()
                .first()
                .copied()
                .unwrap_or(egui::Color32::GRAY);
            let mut item =
                SourceItem::indicator(study_tree_id(id), study.name(), color).with_series_id(id);
            item.visible = study.is_visible();
            items.push(item);
        }
        for drawing in chart.drawings().listed_drawings() {
            let mut item = SourceItem::drawing(drawing.id, drawing.tool_type, drawing.color32())
                .with_panes(chart);
            item.visible = drawing.visible;
            item.locked = drawing.locked;
            item.z_index = drawing.z_order;
            item.label = drawing.label.clone();
            item.folder = drawing.folder.clone();
            items.push(item);
        }
        items
    }

    /// Reads the hovered values from the crosshair payload alone; off the bars, the chart's own
    /// readout shows the latest bar.
    fn build_data_window(&self) -> DataWindowInfo {
        let chart = self.chart();
        let Some((pointer, bar)) = self
            .drawings
            .crosshair
            .as_ref()
            .filter(|(slot, _)| *slot == self.active)
            .and_then(|(_, pointer)| Some((pointer, pointer.bar.as_ref()?)))
        else {
            return DataWindowInfo::from_chart(chart);
        };
        let mut info = DataWindowInfo::new(chart.symbol())
            .with_timeframe(chart.timeframe().map(|tf| tf.as_str()).unwrap_or_default());
        if let Some(instrument) = chart.instrument() {
            info = info.with_instrument(instrument);
        }
        info = info
            .with_ohlcv(bar.open, bar.high, bar.low, bar.close, bar.volume)
            .with_timestamp(chart.format_time(bar.time))
            .with_prev_close(pointer.previous_close.unwrap_or(bar.open));
        info.studies = pointer.studies.iter().map(|(_, row)| row.clone()).collect();
        info
    }

    /// Follows the active chart's crosshair, which the data window drawn before the chart reads.
    fn read_crosshair(&mut self, ctx: &egui::Context) {
        let moved = self
            .chart_events
            .iter()
            .rev()
            .find_map(|(slot, event)| match event {
                ChartEvent::CrosshairMoved(pointer) if *slot == self.active => {
                    Some(pointer.clone())
                }
                _ => None,
            });
        if let Some(pointer) = moved {
            self.drawings.crosshair = pointer.map(|pointer| (self.active, pointer));
            // The data window was drawn before the chart; show the new values now.
            if matches!(
                self.panel,
                Some(SidePanel::ObjectTree | SidePanel::DataWindow)
            ) {
                ctx.request_repaint();
            }
        }
    }

    /// Performs an object tree action as one undoable step. Settings of the main series and of
    /// studies belong to the studies group, and what it leaves is returned.
    fn perform_object_tree(&mut self, action: ObjectTreeAction) -> Option<Action> {
        let sources = self.build_source_items();
        let state = &mut self.drawings;
        let chart = &mut self.charts[self.active];
        chart.begin_user_action();
        let handoff = 'edit: {
            if state.object_tree.apply_to_chart(chart, &action, &sources) {
                break 'edit None;
            }
            match action {
                ObjectTreeAction::ChangeColor(id, color) => {
                    if let Some(study) = study_from_tree_id(id) {
                        if let Some(study) = chart.indicator_mut(study) {
                            study.set_color(0, color);
                        }
                    } else {
                        chart
                            .drawings_mut()
                            .drag_edit_drawing(id, |d| d.color = color.to_srgba_unmultiplied());
                        state.colour_drag = true;
                    }
                    None
                }
                ObjectTreeAction::OpenProperties(id)
                | ObjectTreeAction::OpenIndicatorSettings(id) => {
                    if id == MAIN_SERIES_ID {
                        Some(ChartContextAction::Chart(ContextMenuAction::OpenSettings))
                    } else if let Some(study) = study_from_tree_id(id) {
                        Some(ChartContextAction::Study {
                            id: study,
                            action: StudyContextMenuAction::Settings,
                        })
                    } else {
                        state.properties.open_for_chart(chart, DrawingId(id));
                        None
                    }
                }
                _ => None,
            }
        };
        chart.end_user_action();
        handoff.and_then(|menu| self.perform(Action::ChartMenu(menu)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{SIZE, TestFeed, frame};
    use super::*;
    use egui::{Event, pos2};
    use egui_charts::drawings::{ChartPoint, Drawing};
    use egui_charts::model::ChartType;
    use egui_charts::studies::{CustomIndicator, IndicatorValue, SMA};
    use egui_charts::ui::top_toolbar::TopToolbarAction;
    use std::sync::Arc;

    /// A terminal whose chart has loaded its history.
    fn ready() -> Terminal {
        let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
        for _ in 0..6 {
            frame(&mut terminal, SIZE, vec![]);
        }
        terminal
    }

    fn line(id: usize) -> Drawing {
        let mut drawing = Drawing::new(id, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(10.0, 100.0), ChartPoint::new(20.0, 200.0)];
        drawing.completed = true;
        drawing
    }

    fn run(terminal: &mut Terminal, action: DrawingToolbarAction) {
        assert!(terminal.run(Action::DrawingToolbar(action)).is_none());
    }

    /// Selecting a drawing exposes current style values, and each quick edit is undoable.
    #[test]
    fn selected_toolbar_styles_and_more_menu_reach_the_chart() {
        use super::super::test_support::click;
        let mut terminal = ready();
        let mut drawing = line(0);
        // A red line, so that picking the palette blue changes it.
        drawing.color = [242, 54, 69, 255];
        drawing.chart_points = vec![ChartPoint::new(430.0, 180.0), ChartPoint::new(460.0, 182.0)];
        terminal
            .chart_mut()
            .drawings_mut()
            .add_synced_drawing(drawing);
        let id = terminal.chart().drawings().drawings[0].id;
        terminal.chart_mut().drawings_mut().select_drawings([id]);
        frame(&mut terminal, SIZE, vec![]);
        let rendered_text = |terminal: &mut Terminal, value: &str| {
            let ctx = terminal.ctx().clone();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                    time: Some(ctx.input(|i| i.time) + 0.1),
                    ..Default::default()
                },
                |ui| {
                    terminal.show(ui);
                },
            );
            let position = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == value => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .expect(value);
            output.drop_without_applying_deltas();
            position
        };
        let width = rendered_text(&mut terminal, "2 px");
        click(&mut terminal, SIZE, width);
        let menu = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            menu.text_at("4 px").expect("width choice"),
        );
        assert_eq!(terminal.chart().drawings().drawings[0].stroke_width, 4.0);
        assert!(terminal.chart_mut().undo());
        assert_eq!(terminal.chart().drawings().drawings[0].stroke_width, 2.0);
        assert!(!terminal.chart().can_undo(), "one undo step for the width");
        let style = rendered_text(&mut terminal, "Solid");
        click(&mut terminal, SIZE, style);
        let menu = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            menu.text_at("Dashed").expect("style choice"),
        );
        assert_eq!(
            terminal.chart().drawings().drawings[0].line_style,
            egui_charts::model::LineStyle::Dashed
        );
        assert!(terminal.chart_mut().undo());
        assert_eq!(
            terminal.chart().drawings().drawings[0].line_style,
            egui_charts::model::LineStyle::Solid
        );
        assert!(!terminal.chart().can_undo(), "one undo step for the style");
        let shown = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            shown.text_at("#F23645").expect("current line colour"),
        );
        let palette = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            palette.text_at("#2962FF").expect("palette colour"),
        );
        assert_eq!(
            terminal.chart().drawings().drawings[0].color,
            [41, 98, 255, 255]
        );
        assert!(terminal.chart_mut().undo());
        assert_eq!(
            terminal.chart().drawings().drawings[0].color,
            [242, 54, 69, 255]
        );
        assert!(!terminal.chart().can_undo(), "one undo step for the colour");
        for (tool, filled) in [
            (DrawingToolType::TrendLine, false),
            (DrawingToolType::Rect, true),
            (DrawingToolType::TextLabel, true),
        ] {
            let drawing = &mut terminal.chart_mut().drawings_mut().drawings[0];
            drawing.tool_type = tool;
            drawing.fill_color = Some([0, 255, 0, 85]);
            let shown = frame(&mut terminal, SIZE, vec![]);
            assert!(
                shown.text_at("#F23645").is_some(),
                "current colour: {tool:?}"
            );
            assert_eq!(
                shown.text_at("#00FF00").is_some(),
                filled,
                "fill swatch: {tool:?}"
            );
            if tool == DrawingToolType::Rect {
                click(&mut terminal, SIZE, shown.text_at("#00FF00").unwrap());
                let palette = frame(&mut terminal, SIZE, vec![]);
                click(&mut terminal, SIZE, palette.text_at("#FFFFFF").unwrap());
                assert_eq!(
                    terminal.chart().drawings().drawings[0].fill_color,
                    Some([255, 255, 255, 85]),
                    "a palette change preserves the fill's opacity"
                );
                assert!(terminal.chart_mut().undo());
                assert_eq!(
                    terminal.chart().drawings().drawings[0].fill_color,
                    Some([0, 255, 0, 85])
                );
                assert!(!terminal.chart().can_undo(), "one undo step for the fill");
            }
        }
        let shown = frame(&mut terminal, SIZE, vec![]);
        click(
            &mut terminal,
            SIZE,
            shown.text_at("More").expect("drawing menu button"),
        );
        let menu = frame(&mut terminal, SIZE, vec![]);
        for label in ["Bring forward", "Send backward", "Copy", "Clone", "Hide"] {
            assert!(menu.text_at(label).is_some(), "missing {label}");
        }
        terminal.chart_mut().drawings_mut().deselect();
        let shown = frame(&mut terminal, SIZE, vec![]);
        assert!(shown.text_at("More").is_none());
    }

    /// A double click opens geometry settings, while text tools retain their inline editor.
    #[test]
    fn double_click_drawings_opens_settings_except_for_inline_text() {
        use egui::{Modifiers, PointerButton};
        for (tool, settings) in [
            (DrawingToolType::TrendLine, true),
            (DrawingToolType::TextLabel, false),
        ] {
            let mut terminal = ready();
            let mut drawing = line(0);
            drawing.tool_type = tool;
            drawing.text = Some("Annotation".into());
            drawing.chart_points =
                vec![ChartPoint::new(430.0, 180.0), ChartPoint::new(460.0, 182.0)];
            terminal
                .chart_mut()
                .drawings_mut()
                .add_synced_drawing(drawing);
            frame(&mut terminal, SIZE, vec![]);
            let points = &terminal.chart().drawings().drawings[0].points;
            let pos = if settings {
                points[0].lerp(points[1], 0.5)
            } else {
                points[0] + egui::vec2(10.0, -5.0)
            };
            frame(&mut terminal, SIZE, vec![Event::PointerMoved(pos)]);
            for _ in 0..2 {
                // One frame per click keeps the second release within the double-click interval.
                frame(
                    &mut terminal,
                    SIZE,
                    [true, false]
                        .map(|pressed| Event::PointerButton {
                            pos,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers: Modifiers::NONE,
                        })
                        .to_vec(),
                );
            }
            assert_eq!(terminal.drawings_modal(), settings, "{tool:?}");
            assert!(
                !terminal.studies_modal(),
                "drawing must not open series settings"
            );
            if settings {
                let shown = frame(&mut terminal, SIZE, vec![]);
                assert!(
                    shown.text_at("More").is_none(),
                    "the selection toolbar must not cover the properties dialog"
                );
            }
        }
    }

    #[test]
    fn toolbar_follows_chart_modes_and_completed_tools() {
        let mut terminal = ready();
        assert_eq!(
            terminal.chart().chart_options().crosshair.style,
            CrosshairStyle::Full
        );
        // Crosshair style, magnet strength.
        for (style, cursor, magnet) in [
            (CrosshairStyle::Full, CursorType::Cross, MagnetType::Weak),
            (CrosshairStyle::Dot, CursorType::Dot, MagnetType::Strong),
            (CrosshairStyle::Arrow, CursorType::Arrow, MagnetType::Weak),
            (
                CrosshairStyle::Demonstration,
                CursorType::Demonstration,
                MagnetType::Strong,
            ),
        ] {
            terminal.drawings.toolbar.state.magnet_type = magnet;
            terminal.chart_mut().set_crosshair_style(style);
            let manager = terminal.chart_mut().drawings_mut();
            manager.options.magnet_mode = true;
            manager.options.strong_magnet = magnet != MagnetType::Weak;
            manager.set_active_tool(Some(DrawingToolType::TrendLine));
            frame(&mut terminal, SIZE, vec![]);
            let state = &terminal.drawings.toolbar.state;
            assert_eq!(state.sel_tool, Some(DrawingToolType::TrendLine));
            assert_eq!(state.current_cursor_type, cursor);
            assert!(state.magnet_mode);
            assert_eq!(state.magnet_type, magnet);
            // Completing the drawing clears the tool, and the toolbar follows.
            let manager = terminal.chart_mut().drawings_mut();
            manager.start_drawing_with_coords(
                DrawingToolType::TrendLine,
                pos2(1.0, 2.0),
                |x| x,
                f64::from,
            );
            manager.add_point_with_coords(pos2(3.0, 4.0), |x| x, f64::from);
            assert!(manager.active_tool().is_none());
            frame(&mut terminal, SIZE, vec![]);
            assert!(terminal.drawings.toolbar.state.sel_tool.is_none());
            terminal.chart_mut().drawings_mut().set_eraser_mode(true);
            frame(&mut terminal, SIZE, vec![]);
            assert!(terminal.drawings.toolbar.state.eraser_mode_enabled);
            terminal.chart_mut().drawings_mut().set_eraser_mode(false);
            terminal.chart_mut().drawings_mut().options.magnet_mode = false;
            frame(&mut terminal, SIZE, vec![]);
            assert!(!terminal.drawings.toolbar.state.eraser_mode_enabled);
            assert!(!terminal.drawings.toolbar.state.magnet_mode);
        }
    }

    #[test]
    fn zoom_controls_leave_drawing_history_untouched() {
        let mut terminal = ready();
        let chart = terminal.chart_mut();
        chart.user_action(|chart| chart.drawings_mut().add_synced_drawing(line(1)));
        chart.user_action(|chart| chart.drawings_mut().add_synced_drawing(line(2)));
        terminal.run(Action::TopToolbar(TopToolbarAction::Undo));
        let drawings = |terminal: &Terminal| {
            terminal
                .chart()
                .drawings()
                .drawings
                .iter()
                .map(|drawing| (drawing.id, drawing.chart_points.clone()))
                .collect::<Vec<_>>()
        };
        let kept = drawings(&terminal);
        assert_eq!(kept.len(), 1);
        terminal.drawings.toolbar.state.zoom_mode_active = true;
        run(&mut terminal, DrawingToolbarAction::ZoomIn);
        frame(&mut terminal, SIZE, vec![]);
        let before = terminal.chart().get_time_scale_state();
        let rect = terminal.chart().get_rendered_price_rect();
        assert!(terminal.chart_mut().execute_box_zoom(
            rect.min + rect.size() * 0.2,
            rect.max - rect.size() * 0.2,
            rect,
            rect.width()
        ));
        assert!(terminal.chart().get_time_scale_state().0 > before.0);
        frame(&mut terminal, SIZE, vec![]);
        assert!(terminal.drawings.toolbar.state.has_zoom_history);
        run(&mut terminal, DrawingToolbarAction::ZoomOut);
        assert_eq!(terminal.chart().get_time_scale_state(), before);
        assert!(!terminal.chart().state().has_zoom_history());
        // Zooming is no edit: the drawings are as they were, the undone one can still be
        // redone and the one kept can still be undone.
        assert_eq!(drawings(&terminal), kept);
        assert!(terminal.chart().can_redo());
        assert!(terminal.chart().can_undo());
    }

    #[test]
    fn object_tree_rows_and_edits_follow_the_chart_and_undo_as_one_step() {
        let mut terminal = ready();
        for id in 1..=3 {
            terminal
                .chart_mut()
                .drawings_mut()
                .add_synced_drawing(line(id));
        }
        let ids: Vec<_> = terminal
            .chart()
            .drawings()
            .drawings
            .iter()
            .map(|d| d.id)
            .collect();
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = false);
        assert!(
            !terminal
                .build_source_items()
                .iter()
                .any(|row| row.id == ids[2])
        );
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawing(ids[2], |drawing| drawing.listed = true);
        let study = terminal.chart_mut().add_indicator(Box::new(SMA::new(2)));
        // The main series, every study (the volume one included) and the three drawings, each
        // under its own id: studies above any drawing, the main series last of all.
        let studies: Vec<_> = terminal
            .chart()
            .indicators()
            .map(|(id, _)| usize::MAX / 2 + id.0)
            .collect();
        let row_ids: Vec<_> = terminal
            .build_source_items()
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(row_ids, [&[MAIN_SERIES_ID][..], &studies, &ids].concat(),);
        let object_tree = |terminal: &mut Terminal, action| {
            assert!(terminal.run(Action::ObjectTree(action)).is_none());
        };
        // The main series' settings are handed on as the chart menu's, which the studies group
        // performs by opening the chart settings.
        assert!(!terminal.studies_modal());
        object_tree(
            &mut terminal,
            ObjectTreeAction::OpenProperties(MAIN_SERIES_ID),
        );
        assert!(terminal.studies_modal());
        // A drawing and a study selected in the tree are deleted, and restored, together.
        let tree = terminal.drawings.object_tree.state_mut();
        tree.select(ids[0]);
        tree.toggle_selection(usize::MAX / 2 + study.0);
        object_tree(&mut terminal, ObjectTreeAction::DeleteSelected);
        assert_eq!(terminal.chart().drawings().drawings.len(), 2);
        assert!(terminal.chart().indicators().all(|(id, _)| id != study));
        assert!(terminal.chart_mut().undo());
        assert_eq!(terminal.chart().drawings().drawings.len(), 3);
        assert!(terminal.chart().indicators().any(|(id, _)| id == study));
    }

    #[test]
    fn object_tree_colours_are_unmultiplied_and_one_undo_step_per_drag() {
        let mut terminal = ready();
        terminal
            .chart_mut()
            .drawings_mut()
            .add_synced_drawing(line(7));
        let id = terminal.chart().drawings().drawings[0].id;
        let original = terminal.chart().drawings().drawings[0].color;
        let color = egui::Color32::from_rgba_unmultiplied(51, 102, 153, 85);
        for _ in 0..3 {
            assert!(
                terminal
                    .run(Action::ObjectTree(ObjectTreeAction::ChangeColor(id, color)))
                    .is_none()
            );
        }
        assert_eq!(
            terminal.chart().drawings().drawings[0].color,
            [51, 102, 153, 85]
        );
        assert!(terminal.drawings.colour_drag);
        // The pointer is up, so the next frame ends the drag as one edit.
        frame(&mut terminal, SIZE, vec![]);
        assert!(!terminal.drawings.colour_drag);
        assert!(terminal.chart_mut().undo());
        assert_eq!(terminal.chart().drawings().drawings[0].color, original);
    }

    #[test]
    fn hide_all_and_remove_both_reach_drawings_and_studies_in_one_step() {
        let mut terminal = ready();
        for id in 1..=2 {
            terminal
                .chart_mut()
                .drawings_mut()
                .add_synced_drawing(line(id));
        }
        for period in [3, 5] {
            terminal
                .chart_mut()
                .add_indicator(Box::new(SMA::new(period)));
        }
        // A rendered chart also has its built-in volume study.
        let study_ids: Vec<_> = terminal.chart().indicators().map(|(id, _)| id).collect();
        // Repeating Hide all must keep groups hidden rather than toggle them back on.
        run(&mut terminal, DrawingToolbarAction::HideAll);
        run(&mut terminal, DrawingToolbarAction::HideAll);
        assert!(
            terminal
                .chart()
                .drawings()
                .drawings
                .iter()
                .all(|d| !d.visible)
        );
        assert!(
            terminal
                .chart()
                .indicators()
                .all(|(_, study)| !study.is_visible())
        );
        terminal.run(Action::TopToolbar(TopToolbarAction::Undo));
        assert!(
            terminal
                .chart()
                .drawings()
                .drawings
                .iter()
                .all(|d| d.visible)
        );
        assert!(
            terminal
                .chart()
                .indicators()
                .all(|(_, study)| study.is_visible())
        );

        run(
            &mut terminal,
            DrawingToolbarAction::RemoveDrawingsAndIndicators,
        );
        assert!(terminal.chart().drawings().drawings.is_empty());
        assert_eq!(terminal.chart().indicators().count(), 0);
        terminal.run(Action::TopToolbar(TopToolbarAction::Undo));
        assert_eq!(terminal.chart().drawings().drawings.len(), 2);
        assert_eq!(
            terminal
                .chart()
                .indicators()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            study_ids,
        );
    }

    #[test]
    fn hide_and_lock_all_perform_the_state_the_toolbar_asks_for() {
        let mut terminal = ready();
        for id in 1..=2 {
            terminal
                .chart_mut()
                .drawings_mut()
                .add_synced_drawing(line(id));
        }
        let ids: Vec<_> = terminal
            .chart()
            .drawings()
            .drawings
            .iter()
            .map(|d| d.id)
            .collect();
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawing(ids[0], |drawing| drawing.visible = false);
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawing(ids[0], |drawing| drawing.locked = true);
        // The toolbar has flipped its flags before it emits: hide all and lock all.
        terminal.drawings.toolbar.state.hide_drawings = true;
        terminal.drawings.toolbar.state.lock_all = true;
        run(&mut terminal, DrawingToolbarAction::HideAllDrawings);
        run(&mut terminal, DrawingToolbarAction::LockAllDrawings);
        let drawings = &terminal.chart().drawings().drawings;
        assert!(drawings.iter().all(|d| !d.visible && d.locked));
        // And back: show all and unlock all.
        terminal.drawings.toolbar.state.hide_drawings = false;
        terminal.drawings.toolbar.state.lock_all = false;
        run(&mut terminal, DrawingToolbarAction::HideAllDrawings);
        run(&mut terminal, DrawingToolbarAction::LockAllDrawings);
        let drawings = &terminal.chart().drawings().drawings;
        assert!(drawings.iter().all(|d| d.visible && !d.locked));
        // The toolbar reads the lock state back from the chart, not from its last click.
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawings(&ids, |drawing| drawing.locked = true);
        frame(&mut terminal, SIZE, vec![]);
        assert!(terminal.drawings.toolbar.state.lock_all);
        terminal
            .chart_mut()
            .drawings_mut()
            .edit_drawing(ids[0], |drawing| drawing.locked = false);
        frame(&mut terminal, SIZE, vec![]);
        assert!(!terminal.drawings.toolbar.state.lock_all);
    }

    #[test]
    fn data_window_reads_the_hovered_rendered_element() {
        for kind in [ChartType::Candles, ChartType::Renko] {
            let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
            terminal.chart_mut().set_chart_type(kind);
            let average = terminal.chart_mut().add_indicator(Box::new(SMA::new(2)));
            let study = terminal.chart_mut().indicator_mut(average).unwrap();
            let mut settings = study.settings();
            settings.precision = Some(3);
            study.apply_settings(&[], settings).unwrap();
            let hidden = terminal
                .chart_mut()
                .add_indicator(Box::new(CustomIndicator::new("Invisible", |bars| {
                    vec![IndicatorValue::Single(123.0); bars.len()]
                })));
            terminal.chart_mut().set_indicator_visible(hidden, false);
            for _ in 0..8 {
                frame(&mut terminal, SIZE, vec![]);
            }
            let (start, end) = terminal.chart().state().visible_range();
            let index = (start + end) / 2;
            let map = terminal.chart().get_chart_mapping();
            let hover = pos2(map.idx_to_x(index), map.rect.center().y);
            frame(&mut terminal, SIZE, vec![Event::PointerMoved(hover)]);
            let info = terminal.build_data_window();
            let bars = terminal.chart().state().shown();
            assert_eq!(info.close, bars[index].close, "{kind:?}");
            assert_eq!(
                info.timestamp,
                bars[index].time.format("%b %-d, %Y").to_string(),
                "{kind:?}"
            );
            assert_eq!(info.prev_close, bars[index - 1].close, "{kind:?}");
            // The two-element average is the sum of the displayed closes divided by two.
            let row = info
                .studies
                .iter()
                .find(|row| row.plot == "SMA(2)")
                .expect("the visible study");
            assert!(info.studies.iter().all(|row| row.study != "Invisible"));
            assert_eq!(
                row.value,
                Some((bars[index - 1].close + bars[index].close) / 2.0),
                "{kind:?}"
            );
            // Off the bars the window shows the latest one.
            let latest = bars.last().unwrap().close;
            frame(&mut terminal, SIZE, vec![Event::PointerGone]);
            assert_eq!(terminal.build_data_window().close, latest, "{kind:?}");
        }
    }

    #[test]
    fn menus_the_rail_and_the_clipboard_act_on_the_chart_and_nothing_returns_to_the_host() {
        use egui_charts::ui::context_menu::ChartContextAction as Menu;
        use egui_charts::ui::dialogs::DrawingContextMenuAction as Draw;
        let mut terminal = ready();
        terminal
            .chart_mut()
            .drawings_mut()
            .add_synced_drawing(line(0));
        let id = terminal.chart().drawings().drawings[0].id;
        terminal.chart_mut().drawings_mut().select_drawings([id]);
        let count = |terminal: &Terminal| terminal.chart().drawings().drawings.len();
        let drawing_menu = |terminal: &mut Terminal, action| {
            assert!(
                terminal
                    .run(Action::ChartMenu(Menu::Drawing(action)))
                    .is_none()
            );
        };
        drawing_menu(&mut terminal, Draw::Lock);
        assert!(terminal.chart().drawings().drawings[0].locked);
        drawing_menu(&mut terminal, Draw::Lock);
        drawing_menu(&mut terminal, Draw::Hide);
        assert!(!terminal.chart().drawings().drawings[0].visible);
        assert!(terminal.chart_mut().undo());
        assert!(terminal.chart().drawings().drawings[0].visible);
        // Equal layers still move across exactly one neighbour, and undo restores every layer.
        for copy in 1..=2 {
            terminal
                .chart_mut()
                .drawings_mut()
                .add_synced_drawing(line(copy));
        }
        let ids: Vec<_> = terminal
            .chart()
            .drawings()
            .drawings
            .iter()
            .map(|d| d.id)
            .collect();
        terminal
            .chart_mut()
            .drawings_mut()
            .select_drawings([ids[1]]);
        for drawing in &mut terminal.chart_mut().drawings_mut().drawings {
            drawing.z_order = 0;
        }
        let original: Vec<_> = terminal
            .chart()
            .drawings()
            .drawings
            .iter()
            .map(|d| d.z_order)
            .collect();
        for (action, expected) in [
            (Draw::BringForward, vec![ids[0], ids[2], ids[1]]),
            (Draw::SendBackward, vec![ids[1], ids[0], ids[2]]),
        ] {
            drawing_menu(&mut terminal, action);
            let mut order: Vec<_> = terminal.chart().drawings().drawings.iter().collect();
            order.sort_by_key(|d| d.z_order);
            assert_eq!(order.iter().map(|d| d.id).collect::<Vec<_>>(), expected);
            assert!(terminal.chart_mut().undo());
            assert_eq!(
                terminal
                    .chart()
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| d.z_order)
                    .collect::<Vec<_>>(),
                original
            );
        }
        terminal.chart_mut().drawings_mut().drawings.truncate(1);
        terminal.chart_mut().drawings_mut().select_drawings([id]);
        drawing_menu(&mut terminal, Draw::Copy);
        assert!(terminal.ctx().output(|out| out.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text.starts_with(DRAWINGS_CLIPBOARD)))));
        // Clone duplicates the drawing; the properties dialog offers the same.
        drawing_menu(&mut terminal, Draw::Clone);
        assert_eq!(count(&terminal), 2);
        let action = Action::DrawingProperties(DrawingPropertiesAction::Clone(DrawingId(id)));
        assert!(terminal.run(action).is_none());
        assert_eq!(count(&terminal), 3);
        // Edit opens the dialog, which keys go to before the chart; Delete from it removes
        // the drawing as one undo step.
        terminal.chart_mut().drawings_mut().select_drawings([id]);
        drawing_menu(&mut terminal, Draw::Edit);
        assert!(terminal.drawings_modal());
        let action = Action::DrawingProperties(DrawingPropertiesAction::Delete(DrawingId(id)));
        assert!(terminal.run(action).is_none());
        assert_eq!(count(&terminal), 2);
        assert!(terminal.chart_mut().undo());
        assert_eq!(count(&terminal), 3);
        // The alert dialog belongs to another group, so it is left for it.
        assert!(
            terminal
                .run(Action::ChartMenu(Menu::Drawing(Draw::AddAlert(Box::new(
                    egui_charts::model::Alert::new(
                        "AAPL",
                        egui_charts::model::AlertCondition::Crossing(1.0)
                    )
                )))))
                .is_some()
        );
        // The rail's icon and the chart menu open the object tree; the icon closes it again.
        let rail = Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::ObjectTree));
        assert!(terminal.run(rail.clone()).is_none());
        assert_eq!(terminal.panel, Some(SidePanel::ObjectTree));
        assert!(terminal.run(rail).is_none());
        assert_eq!(terminal.panel, None);
        let menu = Action::ChartMenu(Menu::Chart(ContextMenuAction::ObjectTree));
        assert!(terminal.run(menu).is_none());
        assert_eq!(terminal.panel, Some(SidePanel::ObjectTree));
        // Pasting drawing text adds the copies once; other text is left for the commands group.
        terminal.chart_mut().drawings_mut().select_drawings([id]);
        let text = terminal.chart().copy_selected_drawings().unwrap().unwrap();
        let before = count(&terminal);
        let paste = |text: &str| Action::Shortcut(ShortcutRequest::PasteDrawing(text.into()));
        assert!(terminal.run(paste(&text)).is_none());
        assert_eq!(count(&terminal), before + 1);
        assert!(terminal.run(paste("egui-charts-study\n{}")).is_some());
        assert_eq!(count(&terminal), before + 1);
    }

    #[test]
    fn drawings_and_drawing_defaults_survive_a_restart() {
        use super::super::test_support::MemoryStorage;
        // Whether the storage holds its writes until the host has waited for them.
        for held in [false, true] {
            let storage = Arc::new(MemoryStorage::default());
            let mut terminal = Terminal::new(Arc::new(TestFeed))
                .with_symbol("AAPL")
                .with_storage(storage.clone());
            for _ in 0..6 {
                frame(&mut terminal, SIZE, vec![]);
            }
            let mut template = DrawingTemplate::from_drawing(&line(0));
            template.settings.stroke_width = 4.0;
            terminal
                .chart_mut()
                .drawings_mut()
                .tool_defaults
                .insert(DrawingToolType::TrendLine, template);
            let draw = |terminal: &mut Terminal| {
                terminal
                    .chart_mut()
                    .user_action(|chart| chart.drawings_mut().add_drawing(line(0)).unwrap());
            };
            if held {
                // The first line's write is unanswered when the second line is drawn.
                storage.hold();
                draw(&mut terminal);
                frame(&mut terminal, SIZE, vec![]);
            }
            // Written when the terminal closes, with no frame after the edit.
            draw(&mut terminal);
            if held {
                assert!(!terminal.flush_storage(), "a write is unanswered");
                storage.release();
                assert!(terminal.flush_storage());
            }
            drop(terminal);
            let mut restored = Terminal::new(Arc::new(TestFeed))
                .with_symbol("AAPL")
                .with_storage(storage);
            for _ in 0..6 {
                frame(&mut restored, SIZE, vec![]);
            }
            assert_eq!(
                restored.chart().drawings().drawings.len(),
                1 + usize::from(held)
            );
            let defaults = &restored.chart().drawings().tool_defaults;
            assert_eq!(defaults.len(), 1);
            assert_eq!(
                defaults[&DrawingToolType::TrendLine].settings.stroke_width,
                4.0
            );
        }
    }
}
