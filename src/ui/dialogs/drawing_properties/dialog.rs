//! Drawing properties dialog - modal popup for editing drawing properties.

use crate::localization as l10n;
use egui::{Context, Event, Key, Modifiers, Ui, Vec2};

use super::actions::{DrawingId, DrawingPropertiesAction, DrawingProps};
use super::config::DrawingPropertiesConfig;
use super::tabs::{
    CoordinatesState, CoordinatesTab, InputsTab, PropertiesTab, StyleTab, TextTab, VisibilityTab,
};
use crate::drawings::{Drawing, DrawingTemplate};
use crate::services::StoreKind;
use crate::ui::dialogs::templates::TemplateStore;
use crate::ui::drawing_toolbar::{DrawingToolbarAction, utilities::TemplatesButton};
use crate::ui_kit::ColorPickerState;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::{DialogFooter, DialogFrame, dialog_header};
use crate::ui_kit::tab_bar::TabBar;

/// Drawing properties dialog
#[derive(Debug)]
pub struct DrawingPropertiesDialog {
    /// Display and layout options for this component.
    pub config: DrawingPropertiesConfig,
    /// Whether the dialog is currently visible.
    pub is_open: bool,
    /// Identifier of the drawing being edited, when open.
    pub drawing_id: Option<DrawingId>,
    /// Display name of the drawing being edited.
    pub drawing_name: String,
    /// Currently selected tab.
    pub active_tab: PropertiesTab,
    /// Editable drawing appearance and tool settings.
    pub props: DrawingProps,
    /// Editable drawing anchors.
    pub coordinates: CoordinatesState,
    /// Named templates, kept in memory unless the host attaches its storage service.
    pub templates: TemplateStore<DrawingTemplate>,
    /// Estimated and total bars of the edited anchored VWAP, last read by `show_and_apply`.
    vwap_estimated: Option<(usize, usize)>,
    preview: Option<(String, DrawingId, Vec<PreviewDrawing>)>,
    template_name: String,
    saving_template: bool,
    default_template: Option<DrawingTemplate>,
    custom_tool: Option<crate::drawings::RegisteredDrawingTool>,
    /// Validation error from the most recent edit.
    pub error: Option<String>,
}

impl Default for DrawingPropertiesDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingPropertiesDialog {
    /// Create a [`DrawingPropertiesDialog`] with default settings.
    pub fn new() -> Self {
        Self {
            config: DrawingPropertiesConfig::default(),
            is_open: false,
            drawing_id: None,
            drawing_name: String::new(),
            active_tab: PropertiesTab::Style,
            props: DrawingProps::default(),
            coordinates: CoordinatesState::default(),
            error: None,
            templates: TemplateStore::new(StoreKind::DrawingTemplates),
            vwap_estimated: None,
            preview: None,
            template_name: String::new(),
            saving_template: false,
            default_template: None,
            custom_tool: None,
        }
    }

    /// Open the dialog for a specific drawing
    pub fn open(&mut self, id: DrawingId, name: String, props: DrawingProps) {
        self.custom_tool = None;
        self.is_open = true;
        self.drawing_id = Some(id);
        self.drawing_name = name;
        self.coordinates = props.coordinates.clone();
        self.error = None;
        self.template_name.clear();
        self.saving_template = false;
        self.default_template = props.tool_settings.as_ref().map(|template| {
            DrawingTemplate::from_drawing(&Drawing::new(0, template.settings.tool_type))
        });
        self.props = props;
        self.active_tab = PropertiesTab::Style;
    }

    /// Open with the selected drawing's saved properties and all of its coordinates.
    pub fn open_for_chart(&mut self, chart: &crate::widget::Chart, id: DrawingId) -> bool {
        let Some(drawing) = chart.drawings().drawings.iter().find(|d| d.id == id.0) else {
            return false;
        };
        self.open(
            id,
            chart.drawings().registry.tool_name(drawing).into(),
            DrawingProps::from_drawing(drawing, chart),
        );
        self.custom_tool = drawing
            .custom_tool
            .as_deref()
            .and_then(|id| chart.drawings().registry.get(id))
            .cloned();
        let manager = chart.drawings();
        self.default_template = Some(DrawingTemplate::from_drawing(&match drawing
            .custom_tool
            .as_deref()
        {
            Some(tool) => manager.new_custom_drawing(0, tool),
            None => manager.new_drawing(0, drawing.tool_type),
        }));
        true
    }

    /// Preview validated edits while open; Ok commits one undoable edit and Cancel restores
    /// the original properties. Apply is a notification the host must not apply a second time.
    /// Call every frame, including after closing externally, to restore an unfinished preview.
    pub fn show_and_apply(
        &mut self,
        ctx: &Context,
        chart: &mut crate::widget::Chart,
    ) -> DrawingPropertiesAction {
        self.vwap_estimated = self
            .drawing_id
            .filter(|_| self.is_open)
            .and_then(|id| chart.vwap_estimated_bars(id.0));
        let action = self.show(ctx);
        // Each frame edits the drawings as they were before the preview, as Ok does.
        self.restore_preview(chart);
        if let DrawingPropertiesAction::Apply(id, props) = &action {
            if let Err(error) = props.apply_to_chart(*id, chart) {
                self.error = Some(error.into());
                self.is_open = true;
                return DrawingPropertiesAction::None;
            }
        } else if self.is_open
            && let Some(id) = self.drawing_id
        {
            let mut props = self.props.clone();
            props.coordinates = self.coordinates.clone();
            match props.edited_drawings(id, chart) {
                Ok(edits) => {
                    if self.props.save_as_default
                        && let Some(mut template) = self.current_template()
                    {
                        template.settings.visible = true;
                        chart.drawings_mut().save_tool_default(template.clone());
                        self.default_template = Some(template);
                        self.props.save_as_default = false;
                    }
                    let changed = chart
                        .drawings()
                        .drawings
                        .iter()
                        .find(|d| d.id == id.0)
                        .is_some_and(|current| {
                            serde_json::to_value(DrawingTemplate::from_drawing(current)).ok()
                                != self
                                    .current_template()
                                    .and_then(|template| serde_json::to_value(template).ok())
                        })
                        || !props
                            .coordinates
                            .resolve(chart)
                            .unwrap_or_default()
                            .is_empty()
                        || self.props.template.is_some();
                    if !changed {
                        self.error = None;
                        return action;
                    }
                    let (_, _, originals) = self
                        .preview
                        .get_or_insert_with(|| (chart.symbol().to_owned(), id, Vec::new()));
                    for edit in &edits {
                        if !originals
                            .iter()
                            .any(|original| original.drawing.id == edit.id)
                            && let Some(drawing) =
                                chart.drawings().drawings.iter().find(|d| d.id == edit.id)
                        {
                            originals.push(PreviewDrawing {
                                drawing: drawing.clone(),
                                times: drawing
                                    .chart_points
                                    .iter()
                                    .map(|p| chart.drawing_point_time(p))
                                    .collect(),
                            });
                        }
                    }
                    for edit in edits {
                        if let Some(drawing) = chart
                            .drawings_mut()
                            .drawings
                            .iter_mut()
                            .find(|d| d.id == edit.id)
                        {
                            *drawing = edit;
                        }
                    }
                    self.error = None;
                }
                Err(error) => self.error = Some(error.into()),
            }
        }
        action
    }

    fn restore_preview(&mut self, chart: &mut crate::widget::Chart) {
        let Some((symbol, _, originals)) = self.preview.take() else {
            return;
        };
        if symbol != chart.symbol() {
            return;
        }
        for original in originals {
            let points: Vec<_> = original
                .drawing
                .chart_points
                .iter()
                .zip(&original.times)
                .map(|(point, time)| {
                    time.and_then(|time| chart.drawing_point_at(time, point.price))
                        .filter(|mapped| mapped.bar_idx != point.bar_idx)
                        .unwrap_or(*point)
                })
                .collect();
            if let Some(drawing) = chart.drawings_mut().drawings.iter_mut().find(|d| {
                d.id == original.drawing.id
                    && d.tool_type == original.drawing.tool_type
                    && d.stored_id == original.drawing.stored_id
            }) {
                let old = original.drawing;
                drawing.color = old.color;
                drawing.stroke_width = old.stroke_width;
                drawing.line_style = old.line_style;
                drawing.fill_color = old.fill_color;
                drawing.extend_left = old.extend_left;
                drawing.extend_right = old.extend_right;
                drawing.text = old.text;
                drawing.font_size = old.font_size;
                drawing.glyph_angle = old.glyph_angle;
                drawing.timeframe_visibility = old.timeframe_visibility;
                drawing.fib_config = old.fib_config;
                drawing.pitchfork = old.pitchfork;
                drawing.gann = old.gann;
                drawing.ghost_feed = old.ghost_feed;
                drawing.bars_pattern = old.bars_pattern;
                drawing.projection = old.projection;
                drawing.table = old.table;
                drawing.forecast = old.forecast;
                // Only the opacity is previewed; a corner drag may have resized the frame since.
                if let Some(image) = &mut drawing.image {
                    image.opacity = old.image.map_or(255, |c| c.opacity);
                }
                drawing.position_size = old.position_size;
                drawing.position_sizing = old.position_sizing;
                drawing.position_readout = old.position_readout;
                drawing.position_target_color = old.position_target_color;
                drawing.position_stop_color = old.position_stop_color;
                drawing.position_style = old.position_style;
                drawing.marker = old.marker;
                drawing.volume_profile = old.volume_profile;
                drawing.regression_trend = old.regression_trend;
                drawing.vwap = old.vwap;
                drawing.channel_midline = old.channel_midline;
                drawing.anchor_price_style = old.anchor_price_style;
                drawing.middle_line = old.middle_line;
                drawing.theme_shade = old.theme_shade;
                drawing.line_measurements = old.line_measurements;
                drawing.range_settings = old.range_settings;
                drawing.price_axis_label = old.price_axis_label;
                drawing.time_axis_label = old.time_axis_label;
                drawing.elliott_degree = old.elliott_degree;
                drawing.elliott_show_wave = old.elliott_show_wave;
                drawing.font_weight = old.font_weight;
                drawing.italic = old.italic;
                drawing.text_wrap = old.text_wrap;
                drawing.text_wrap_width = old.text_wrap_width;
                drawing.text_follows_zoom = old.text_follows_zoom;
                drawing.note_text_style = old.note_text_style;
                drawing.text_color = old.text_color;
                drawing.background = old.background;
                drawing.border = old.border;
                drawing.border_color = old.border_color;
                drawing.arrow_start_style = old.arrow_start_style;
                drawing.arrow_end_style = old.arrow_end_style;
                drawing.custom_settings = old.custom_settings;
                drawing.chart_points = points;
                drawing.screen_anchor = old.screen_anchor;
            }
        }
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
        self.drawing_id = None;
    }

    /// Show the dialog
    pub fn show(&mut self, ctx: &Context) -> DrawingPropertiesAction {
        self.templates.poll(ctx);
        if !self.is_open {
            return DrawingPropertiesAction::None;
        }

        let mut action = DrawingPropertiesAction::None;

        DialogFrame::new(
            "Drawing Properties",
            Vec2::new(self.config.width, self.config.height),
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

        action
    }

    fn current_template(&self) -> Option<DrawingTemplate> {
        let mut template = self.props.tool_settings.as_deref()?.clone();
        let d = &mut template.settings;
        d.color = self.props.color_rgba();
        d.stroke_width = self.props.line_width;
        d.line_style = self.props.line_style;
        d.fill_color = self.props.fill_enabled.then(|| {
            self.props
                .fill_color
                .unwrap_or(self.props.color.gamma_multiply(0.2))
                .to_srgba_unmultiplied()
        });
        d.extend_left = self.props.extend_left;
        d.extend_right = self.props.extend_right;
        d.text = (!self.props.label.is_empty()).then(|| self.props.label.clone());
        d.font_size = self.props.font_size;
        d.timeframe_visibility = self.props.timeframe_visibility.clone();
        Some(template)
    }

    fn load_template(&mut self, template: DrawingTemplate) {
        if self.props.tool_settings.as_ref().is_none_or(|current| {
            current.settings.tool_type != template.settings.tool_type
                || current.settings.custom_tool != template.settings.custom_tool
        }) {
            return;
        }
        let d = &template.settings;
        self.props.color = d.color32();
        self.props.line_width = d.stroke_width;
        self.props.line_style = d.line_style;
        self.props.fill_color = d.fill_color32();
        self.props.fill_enabled = d.fill_color.is_some();
        self.props.extend_left = d.extend_left;
        self.props.extend_right = d.extend_right;
        // A tool without text keeps its picture, sticker or icon there.
        if d.tool_type.supports_text() {
            self.props.label = d.text.clone().unwrap_or_default();
        }
        self.props.font_size = d.font_size;
        self.props.timeframe_visibility = d.timeframe_visibility.clone();
        self.props.template = Some(Box::new(template.clone()));
        self.props.tool_settings = Some(Box::new(template));
    }

    fn template_menu(&mut self, ui: &mut Ui) {
        let mut load = None;
        let menu = Button::new("Template").ghost().show(ui);
        egui::Popup::menu(&menu).show(|ui| {
            if ui
                .button(l10n::text(ui.ctx(), "Save as template…"))
                .clicked()
            {
                self.saving_template = true;
                ui.close();
            }
            let tool = self.props.tool_settings.as_ref().map(|t| &t.settings);
            // The check `load_template` makes: another tool's default would not load.
            let defaults = self.default_template.as_ref().map(|t| &t.settings);
            let resets = tool.zip(defaults).is_some_and(|(tool, defaults)| {
                tool.tool_type == defaults.tool_type && tool.custom_tool == defaults.custom_tool
            });
            if tool.is_some()
                && ui
                    .button(l10n::text(ui.ctx(), "Save as default for this tool"))
                    .clicked()
            {
                self.props.save_as_default = true;
                ui.close();
            }
            if resets
                && ui
                    .button(l10n::text(ui.ctx(), "Reset to defaults"))
                    .clicked()
            {
                load = self.default_template.clone();
                ui.close();
            }
            ui.separator();
            let mut delete = None;
            if let Some(current) = &self.props.tool_settings {
                let mut rows: Vec<_> = self
                    .templates
                    .entries()
                    .iter()
                    .filter(|(_, template)| {
                        template.settings.tool_type == current.settings.tool_type
                            && template.settings.custom_tool == current.settings.custom_tool
                    })
                    .map(|(document, template)| {
                        (self.templates.template_name(document), document, template)
                    })
                    .collect();
                // By the name shown: one saved under its name alone sorts apart from prefixed ones.
                rows.sort_by_key(|(name, ..)| *name);
                for (name, document, template) in rows {
                    if ui.button(name).clicked() {
                        load = Some(template.clone());
                        ui.close();
                    }
                    if ui
                        .button(l10n::format_text(
                            ui.ctx(),
                            "Delete {name}",
                            &[("name", name)],
                        ))
                        .clicked()
                    {
                        delete = Some(document.clone());
                    }
                }
            }
            if let Some(name) = delete
                && let Err(error) = self.templates.delete(&name)
            {
                self.templates.error = Some(error);
            }
        });
        if let Some(template) = load {
            self.load_template(template);
        }
        if self.saving_template
            && let Some(action) =
                TemplatesButton::show_name_dialog(ui, ui.min_rect(), &mut self.template_name)
        {
            match action {
                DrawingToolbarAction::SaveTemplate { name } => {
                    if let Some(template) = self.current_template() {
                        let saved = self
                            .props
                            .validate()
                            .map_err(str::to_owned)
                            .and_then(|()| self.templates.save_for_tool(&name, template));
                        match saved {
                            Ok(()) => {
                                self.saving_template = false;
                                self.template_name.clear();
                            }
                            Err(error) => self.templates.error = Some(error),
                        }
                    }
                }
                _ => self.saving_template = false,
            }
        }
        if let Some(error) = &self.templates.error {
            ui.colored_label(ui.visuals().error_fg_color, l10n::text(ui.ctx(), error));
        }
    }

    fn render_contents(&mut self, ui: &mut Ui) -> DrawingPropertiesAction {
        let ctx = ui.ctx().clone();
        // Read before the body: a combo popup closes itself on the Escape it receives there.
        let popup_open = egui::Popup::is_any_open(&ctx);
        let state_id = ui.scope_id().with("keyboard_and_pickers");
        let mut state: UiState = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();

        let mut cancel = dialog_header(ui, &self.drawing_name);
        let has_inputs = self
            .props
            .tool_settings
            .as_ref()
            .is_some_and(|t| InputsTab::available(t.settings.tool_type));
        let tabs: Vec<_> = PropertiesTab::all()
            .into_iter()
            .filter(|t| match t {
                PropertiesTab::Inputs => has_inputs,
                PropertiesTab::Text => self
                    .props
                    .tool_settings
                    .as_ref()
                    .is_some_and(|t| TextTab::available(t.settings.tool_type)),
                PropertiesTab::Coordinates => self
                    .props
                    .tool_settings
                    .as_ref()
                    .is_none_or(|t| CoordinatesTab::available(t.settings.tool_type)),
                _ => true,
            })
            .collect();
        if !tabs.contains(&self.active_tab) {
            self.active_tab = PropertiesTab::Style;
        }
        egui::ScrollArea::horizontal()
            .id_salt("properties_tabs")
            .show(ui, |ui| TabBar::new(&tabs, &mut self.active_tab).show(ui));

        // Vertical auto-shrink stays on: a full-height body would push the pinned footer out.
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .max_height(
                (ui.available_height()
                    - FOOTER_HEIGHT
                    - if self.error.is_some() { 40.0 } else { 0.0 })
                .max(0.0),
            )
            .show(ui, |ui| match self.active_tab {
                PropertiesTab::Style => {
                    StyleTab::show(
                        ui,
                        &mut self.props,
                        &mut state.pickers,
                        &mut state.text_pickers,
                        &mut state.card_pickers,
                        self.custom_tool.as_ref(),
                    );
                }
                PropertiesTab::Inputs => {
                    let color = self.props.color_rgba();
                    if let Some(template) = &mut self.props.tool_settings {
                        template.settings.color = color;
                        InputsTab::show(
                            ui,
                            &mut template.settings,
                            &mut state.level_pickers,
                            &mut self.coordinates,
                            self.props.tick_size,
                            self.vwap_estimated,
                        );
                    }
                }
                PropertiesTab::Coordinates => CoordinatesTab::show(ui, &mut self.coordinates),
                PropertiesTab::Visibility => {
                    VisibilityTab::show(ui, &mut self.props.timeframe_visibility)
                }
                PropertiesTab::Text => TextTab::show(ui, &mut self.props, &mut state.text_pickers),
            });

        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, l10n::text(ui.ctx(), error));
        }

        let (mut apply, cancel_clicked) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .show_with_left(ui, |ui| self.template_menu(ui));
        cancel |= cancel_clicked;

        // An open popup, colour picker or focused field takes Escape and Enter first. egui clears
        // the focus on Escape before this frame runs, so the focus is read from the previous frame.
        // Only a fresh press acts: the repeats of a key held after it closed a popup, picker or
        // field must not go on to cancel or apply the dialog.
        let consume = |key| {
            ctx.input_mut(|i| {
                let fresh = i.events.iter().any(|e| {
                    matches!(e, Event::Key { key: k, pressed: true, repeat: false, modifiers, .. }
                        if *k == key && modifiers.is_none())
                });
                fresh && i.consume_key(Modifiers::NONE, key)
            })
        };
        if state
            .pickers
            .iter()
            .chain(&state.text_pickers)
            .chain(&state.card_pickers)
            .chain(state.level_pickers.iter().flatten())
            .any(|p| p.is_open)
        {
            if consume(Key::Escape) {
                state
                    .pickers
                    .iter_mut()
                    .chain(&mut state.text_pickers)
                    .chain(&mut state.card_pickers)
                    .chain(state.level_pickers.iter_mut().flatten())
                    .for_each(|p| p.is_open = false);
            }
        } else if !popup_open && !state.had_focus {
            cancel |= consume(Key::Escape);
            apply |= !cancel && !self.saving_template && consume(Key::Enter);
        }
        state.had_focus = ctx.memory(|m| m.focused()).is_some();

        let action = match self.drawing_id {
            Some(id) if apply => {
                self.props.coordinates = self.coordinates.clone();
                DrawingPropertiesAction::Apply(id, Box::new(self.props.clone()))
            }
            _ if cancel => DrawingPropertiesAction::Cancel,
            _ => DrawingPropertiesAction::None,
        };
        // A closed dialog starts with fresh keyboard and picker state next time.
        if matches!(action, DrawingPropertiesAction::None) {
            ui.data_mut(|d| d.insert_temp(state_id, state));
        } else {
            self.is_open = false;
            ui.data_mut(|d| d.remove::<UiState>(state_id));
        }
        action
    }
}

/// Height of the `DialogFooter` block below the body: two `spacing.lg` gaps, the rule and a
/// `ButtonSize::MD` row, with their item spacing.
const FOOTER_HEIGHT: f32 = 60.0;

/// Keyboard and colour picker state kept in egui memory between frames.
#[derive(Clone, Default)]
struct UiState {
    /// Colour, fill colour and signpost disc colour pickers of the Style tab; they also serve the
    /// ghost feed's rising body, falling body and wick, and the colour and disc pickers serve the
    /// regression trend's base and upper lines.
    pickers: [ColorPickerState; 3],
    /// Text colour, background and border pickers of the Text tab, and of the Style tab's rows
    /// for a price label's badge and a price note's label; the text and background pickers also
    /// serve the ghost feed's rising and falling borders, and the background picker the regression
    /// trend's lower line.
    text_pickers: [ColorPickerState; 3],
    /// Background, text and border pickers of a forecast's cards and badges in the Style tab, in
    /// the order of its rows.
    card_pickers: [ColorPickerState; 10],
    /// One list of colour pickers per level grid of the current tool.
    level_pickers: Vec<Vec<ColorPickerState>>,
    /// Whether a widget had keyboard focus at the end of the previous frame.
    had_focus: bool,
}

/// Only previewed drawings are restored; incoming host drawings and metadata stay intact.
#[derive(Debug)]
struct PreviewDrawing {
    drawing: Drawing,
    times: Vec<Option<chrono::DateTime<chrono::Utc>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType as Tool};
    use crate::model::BarData;
    use crate::widget::Chart;
    use egui::{Color32, Pos2, Rect, Shape};

    /// The registered settings editor is reachable, and Ok records its edit as one undo step.
    #[test]
    fn registered_tool_settings_show_in_the_style_tab() {
        for common in [true, false] {
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::new());
            let mut tool = crate::drawings::registry::tests::midpoint();
            tool.style.color = common;
            tool.style.line_width = common;
            tool.style.line_style = common;
            tool.style.fill = false;
            tool.style.line_ends = false;
            chart
                .drawings_mut()
                .registry
                .register("midpoint", tool)
                .unwrap();
            let mut drawing = Drawing::custom("midpoint");
            drawing.id = 7;
            drawing.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
            chart.drawings_mut().drawings.push(drawing);
            let mut dialog = DrawingPropertiesDialog::new();
            assert!(dialog.open_for_chart(&chart, DrawingId(7)));
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            let labels = frame(&mut dialog, &mut chart, &ctx, vec![]);
            for (label, present) in [
                ("Radius", true),
                ("Color", common),
                ("Line width", common),
                ("Line style", common),
                ("Fill", false),
                ("Line start", false),
                ("Line end", false),
            ] {
                assert_eq!(
                    labels.iter().any(|(text, _)| text == label),
                    present,
                    "{label}"
                );
            }
            click(&mut dialog, &mut chart, &ctx, "Ok");
            assert_eq!(
                chart.drawings().drawings[0].custom_settings,
                serde_json::json!({"radius": 10})
            );
            assert!(chart.undo());
            assert_eq!(
                chart.drawings().drawings[0].custom_settings,
                serde_json::Value::Null
            );
            assert!(!chart.can_undo());
        }
    }

    fn frame(
        dialog: &mut DrawingPropertiesDialog,
        chart: &mut Chart,
        ctx: &Context,
        events: Vec<Event>,
    ) -> Vec<(String, Pos2)> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                dialog.show_and_apply(ui.ctx(), chart);
            },
        );
        let text = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos + t.galley.size() / 2.0)),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        output.drop_without_applying_deltas();
        text
    }

    fn click(dialog: &mut DrawingPropertiesDialog, chart: &mut Chart, ctx: &Context, label: &str) {
        let texts = frame(dialog, chart, ctx, vec![]);
        let pos = texts
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("Missing {label}: {texts:?}"))
            .1;
        frame(dialog, chart, ctx, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                dialog,
                chart,
                ctx,
                vec![Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    /// Unsupported settings never appear, including when reusing the dialog after a text tool.
    #[test]
    fn properties_rows_follow_the_tool_renderer() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = Chart::new(BarData::new());
            let mut dialog = DrawingPropertiesDialog::new();
            for (tool, fill, extend, text, (width, style)) in [
                (Tool::HorizontalLine, false, false, true, (true, true)),
                (Tool::Rect, true, true, true, (true, true)),
                (Tool::Brush, true, false, false, (true, true)),
                (Tool::Curve, true, true, false, (true, true)),
                (Tool::DoubleCurve, true, true, false, (true, true)),
                (Tool::TrendLine, false, true, true, (true, true)),
                (Tool::TrendAngle, false, true, true, (true, true)),
                (Tool::TextLabel, false, false, true, (false, false)),
                (Tool::Highlighter, false, false, false, (true, true)),
                (Tool::ArrowMarker, false, false, true, (false, false)),
                (Tool::ArrowMarkDown, false, false, true, (false, false)),
                (Tool::FibonacciChannel, true, false, true, (false, false)),
                (Tool::Pitchfork, false, true, false, (true, false)),
                (Tool::SchiffPitchfork, false, true, false, (true, false)),
                (
                    Tool::ModifiedSchiffPitchfork,
                    false,
                    true,
                    false,
                    (true, false),
                ),
                (Tool::InsidePitchfork, false, true, false, (true, false)),
                (Tool::Pitchfan, false, false, false, (true, false)),
                (Tool::Tweet, true, false, true, (false, true)),
                (Tool::Sticker, false, false, false, (false, false)),
                (Tool::FontIcon, false, false, false, (false, false)),
                (Tool::GhostFeed, false, false, false, (false, false)),
            ] {
                let mut drawing = Drawing::new(7, tool);
                drawing.chart_points = vec![ChartPoint::new(1.0, 100.0); 3];
                chart.drawings_mut().drawings = vec![drawing];
                dialog.open_for_chart(&chart, DrawingId(7));
                let freehand = matches!(tool, Tool::Brush | Tool::Highlighter);
                assert_eq!(
                    dialog.coordinates.points.len(),
                    if freehand { 0 } else { 3 },
                    "{tool:?}: staged anchors"
                );
                frame(&mut dialog, &mut chart, &ctx, vec![]);
                let labels = frame(&mut dialog, &mut chart, &ctx, vec![]);
                for (label, present) in [
                    ("Fill", fill),
                    ("Extend", extend),
                    (
                        "Extend lines",
                        matches!(
                            tool,
                            Tool::Pitchfork
                                | Tool::SchiffPitchfork
                                | Tool::ModifiedSchiffPitchfork
                                | Tool::InsidePitchfork
                        ),
                    ),
                    ("Text", text),
                    (
                        "Color",
                        !matches!(
                            tool,
                            Tool::FibonacciChannel
                                | Tool::Pitchfork
                                | Tool::SchiffPitchfork
                                | Tool::ModifiedSchiffPitchfork
                                | Tool::InsidePitchfork
                                | Tool::Pitchfan
                                | Tool::Tweet
                                | Tool::Sticker
                                | Tool::GhostFeed
                        ),
                    ),
                    ("Line width", width),
                    ("Line style", style),
                    ("Wick", tool == Tool::GhostFeed),
                    ("Angle", matches!(tool, Tool::FontIcon | Tool::Sticker)),
                    ("Coordinates", !freehand),
                    ("Template", true),
                    ("Ok", true),
                    ("Delete", false),
                    ("Clone", false),
                ] {
                    assert_eq!(
                        labels.iter().any(|(text, _)| text == label),
                        present,
                        "{tool:?}: {label}"
                    );
                }
                dialog.close();
                frame(&mut dialog, &mut chart, &ctx, vec![]);
            }
        }
    }

    /// The forward switch previews, survives templates and commits once, or restores on Cancel.
    #[test]
    fn fork_extension_controls_preview_cancel_and_commit() {
        use crate::drawings::PitchforkConfig;
        use egui_kittest::{Harness, kittest::Queryable};
        // The four forks share the row, whose presence the table above checks per tool.
        let tool = Tool::Pitchfork;
        for original in [Some(PitchforkConfig::default()), None] {
            for confirm in [false, true] {
                let mut chart = Chart::new(BarData::new());
                let mut drawing = Drawing::new(7, tool);
                drawing.pitchfork = original.clone();
                drawing.line_style = crate::model::LineStyle::Dashed;
                drawing.chart_points = vec![ChartPoint::new(1.0, 100.0); 3];
                let mut expected = super::super::tabs::pitchfork_shown(&drawing);
                expected.extend_lines = false;
                assert!(
                    expected.levels.iter().all(|level| level.line_style
                        == if original.is_none() {
                            crate::model::LineStyle::Dashed
                        } else {
                            crate::model::LineStyle::Solid
                        }),
                    "{tool:?}: editing reach keeps the drawn patterns"
                );
                chart.drawings_mut().drawings.push(drawing);
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open_for_chart(&chart, DrawingId(7));
                let mut h = Harness::builder()
                    .with_size(egui::vec2(1440.0, 900.0))
                    .with_accessibility_check(false)
                    .allow_missing_glyphs()
                    .build_ui_state(
                        |ui, (chart, dialog): &mut (Chart, DrawingPropertiesDialog)| {
                            dialog.show_and_apply(ui.ctx(), chart);
                        },
                        (chart, dialog),
                    );
                h.run();
                assert_eq!(h.state().0.drawings().drawings[0].pitchfork, original);
                h.get_by_role_and_label(egui::Role::CheckBox, "Extend lines")
                    .click();
                h.run();
                assert_eq!(
                    h.state().0.drawings().drawings[0].pitchfork,
                    Some(expected.clone())
                );
                assert!(!h.state().0.can_undo());
                let template = h.state().1.current_template().unwrap();
                let template: DrawingTemplate =
                    serde_json::from_str(&serde_json::to_string(&template).unwrap()).unwrap();
                let mut peer = Drawing::new(8, tool);
                template.apply_to(&mut peer).unwrap();
                assert_eq!(peer.pitchfork, Some(expected));
                if confirm {
                    h.get_by_role_and_label(egui::Role::Button, "Ok").click();
                    h.run();
                    assert!(h.state_mut().0.undo());
                    assert!(!h.state().0.can_undo());
                } else {
                    h.state_mut().1.close();
                    h.run();
                }
                assert_eq!(h.state().0.drawings().drawings[0].pitchfork, original);
            }
        }
    }

    /// A trend angle's or chart pattern's Text tab styles its own labels: it has no text to write,
    /// align or box. Text also offers to follow the zoom and to wrap.
    #[test]
    fn text_tab_rows_follow_the_tool() {
        let ctx = Context::default();
        let mut chart = Chart::new(BarData::new());
        let mut dialog = DrawingPropertiesDialog::new();
        for (tool, writes, styles, boxed) in [
            (Tool::TrendAngle, false, true, false),
            (Tool::TrendLine, true, true, false),
            (Tool::TextLabel, true, true, true),
            (Tool::DateRange, true, true, false),
            (Tool::DateAndPriceRange, true, true, false),
            (Tool::XABCDPattern, false, true, false),
        ] {
            let mut drawing = Drawing::new(7, tool);
            drawing.chart_points = vec![ChartPoint::new(1.0, 100.0); 2];
            chart.drawings_mut().drawings = vec![drawing];
            dialog.open_for_chart(&chart, DrawingId(7));
            dialog.active_tab = PropertiesTab::Text;
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            let labels = frame(&mut dialog, &mut chart, &ctx, vec![]);
            let has = |label: &str| labels.iter().any(|(text, _)| text == label);
            assert!(has("Font size"), "{tool:?}");
            assert_eq!(has("Label"), writes, "{tool:?}");
            assert_eq!(
                has("Horizontal alignment"),
                tool == Tool::TrendLine,
                "{tool:?}"
            );
            assert_eq!(has("Text style") && has("Text color"), styles, "{tool:?}");
            assert_eq!(has("Background"), boxed, "{tool:?}");
            assert_eq!(
                has("Text size") && has("Text wrap"),
                tool == Tool::TextLabel,
                "{tool:?}"
            );
            dialog.close();
            frame(&mut dialog, &mut chart, &ctx, vec![]);
        }
    }

    /// The Visibility tab writes only the unit row the user edits, so labels saved before
    /// interval ranges keep their exact visibility.
    #[test]
    fn visibility_rows_keep_saved_interval_labels_until_edited() {
        use crate::drawings::TimeframeVisibility;
        use crate::model::TimeframeUnit;
        let ctx = Context::default();
        let mut chart = Chart::new(BarData::new());
        let mut drawing = Drawing::new(7, Tool::TrendLine);
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h", "4h"]);
        chart.drawings_mut().drawings = vec![drawing];
        let mut dialog = DrawingPropertiesDialog::new();
        dialog.open_for_chart(&chart, DrawingId(7));
        dialog.active_tab = PropertiesTab::Visibility;
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        let mut want = TimeframeVisibility::specific(&["1h", "4h"]);
        assert_eq!(chart.drawings().drawings[0].timeframe_visibility, want);
        click(&mut dialog, &mut chart, &ctx, "Ticks");
        want.ranges.insert(TimeframeUnit::Tick, 1..=1_000_000);
        assert_eq!(chart.drawings().drawings[0].timeframe_visibility, want);
    }

    /// Preview edits leave both histories alone; accepting a sequence commits one bulk edit.
    #[test]
    fn preview_edits_commit_once_or_restore_without_history() {
        for finish in ["Ok", "Cancel", "Escape", "close", "reopen"] {
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::new());
            for (id, width) in [(7, 20.0), (8, 4.0), (9, 6.0)] {
                let mut drawing = Drawing::new(id, Tool::TrendLine);
                drawing.stroke_width = width;
                // As saved before the theme shade, which Reset to defaults switches on.
                drawing.theme_shade = false;
                drawing.chart_points =
                    vec![ChartPoint::new(2.0, 100.0), ChartPoint::new(5.0, 110.0)];
                chart.drawings_mut().drawings.push(drawing);
            }
            chart
                .user_action(|chart| {
                    chart
                        .drawings_mut()
                        .edit_drawing(9, |d| d.stroke_width = 8.0)
                })
                .unwrap();
            assert!(chart.undo());
            chart.drawings_mut().select_drawings([7, 8]);
            let mut dialog = DrawingPropertiesDialog::new();
            dialog.open_for_chart(&chart, DrawingId(7));
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            assert_eq!(
                chart.drawings().drawings[1].stroke_width,
                4.0,
                "opening a selection is not an edit"
            );
            assert_eq!(
                chart.drawings().drawings[0].stroke_width,
                20.0,
                "opening keeps a wide stroke"
            );
            assert!(!chart.can_undo());
            // At 20, drawing 7's own width, the width is not edited and 8 shows its own.
            for (width, peer) in [(3.0, 3.0), (20.0, 4.0), (5.0, 5.0)] {
                dialog.props.line_width = width;
                dialog.props.color = Color32::GREEN;
                let forecast = crate::drawings::ForecastStyle {
                    start_border: Some([4, 5, 6, 255]),
                    ..Default::default()
                };
                let settings = &mut dialog.props.tool_settings.as_mut().unwrap().settings;
                settings.forecast = Some(forecast);
                let table = crate::drawings::TableConfig {
                    screen_anchored: true,
                    column_widths: vec![None, Some(90.0)],
                    ..Default::default()
                };
                settings.table = Some(Box::new(table.clone()));
                settings.position_target_color = Some([40, 100, 220, 255]);
                settings.position_stop_color = Some([220, 140, 40, 255]);
                settings.position_style.border = true;
                settings.marker.show_text = false;
                settings.price_axis_label = false;
                settings.time_axis_label = false;
                settings.italic = true;
                settings.glyph_angle = 30.0;
                settings.text_wrap = true;
                settings.text_wrap_width = 120.0;
                settings.text_follows_zoom = true;
                let note_text = crate::drawings::NoteTextStyle {
                    color: [7, 8, 9, 255],
                    font_size: 18.0,
                    bold: true,
                    italic: false,
                };
                settings.note_text_style = Some(note_text);
                settings.text_color = Some([1, 2, 3, 255]);
                settings.line_measurements.price_labels = true;
                settings.range_settings.volume = false;
                settings.theme_shade = true;
                settings.anchor_price_style = Some(note_text);
                settings.middle_line.width = 3.0;
                settings.elliott_show_wave = false;
                dialog.coordinates.points[1].price = "125".into();
                frame(&mut dialog, &mut chart, &ctx, vec![]);
                assert_eq!(
                    chart.drawings().drawings[0].stroke_width,
                    width,
                    "{finish}: live preview"
                );
                assert_eq!(chart.drawings().drawings[1].stroke_width, peer, "{finish}");
                let preview = &chart.drawings().drawings[0];
                assert_eq!(preview.position_target_color, Some([40, 100, 220, 255]));
                assert_eq!(preview.position_stop_color, Some([220, 140, 40, 255]));
                assert!(preview.position_style.border);
                assert!(!preview.marker.show_text);
                assert!(!preview.price_axis_label && !preview.time_axis_label);
                assert!(preview.italic);
                assert_eq!(preview.glyph_angle, 30.0);
                assert_eq!((preview.text_wrap, preview.text_wrap_width), (true, 120.0));
                assert!(preview.text_follows_zoom);
                assert_eq!(preview.note_text_style, Some(note_text));
                assert_eq!(preview.text_color, Some([1, 2, 3, 255]));
                assert!(preview.line_measurements.price_labels);
                assert!(!preview.range_settings.volume);
                assert!(preview.theme_shade);
                assert_eq!(preview.anchor_price_style, Some(note_text));
                assert_eq!(preview.middle_line.width, 3.0);
                assert!(!preview.elliott_show_wave);
                assert_eq!(
                    preview.forecast,
                    Some(forecast),
                    "{finish}: forecast preview"
                );
                assert_eq!(
                    preview.table,
                    Some(Box::new(table)),
                    "{finish}: table preview"
                );
                assert_eq!(chart.drawings().drawings[2].stroke_width, 6.0);
                assert!(!chart.can_undo());
                assert!(chart.can_redo(), "preview must retain existing redo");
                assert!(!chart.drawings().can_undo());
            }
            match finish {
                "Ok" | "Cancel" => click(&mut dialog, &mut chart, &ctx, finish),
                "Escape" => {
                    frame(
                        &mut dialog,
                        &mut chart,
                        &ctx,
                        vec![Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                    );
                }
                _ => {
                    if finish == "reopen" {
                        dialog.open_for_chart(&chart, DrawingId(9));
                        frame(&mut dialog, &mut chart, &ctx, vec![]);
                        assert_eq!(chart.drawings().drawings[0].stroke_width, 20.0);
                        assert_eq!(chart.drawings().drawings[1].stroke_width, 4.0);
                    }
                    dialog.close();
                    frame(&mut dialog, &mut chart, &ctx, vec![]);
                }
            }
            assert!(!dialog.is_open, "{finish}");
            if finish == "Ok" {
                assert_eq!(chart.drawings().drawings[0].stroke_width, 5.0);
                assert_eq!(chart.drawings().drawings[0].chart_points[1].price, 125.0);
                assert!(chart.undo());
            }
            let restored = &chart.drawings().drawings[0];
            assert_eq!(restored.position_target_color, None, "{finish}");
            assert_eq!(restored.position_stop_color, None, "{finish}");
            assert!(!restored.position_style.border, "{finish}");
            assert!(restored.marker.show_text, "{finish}");
            assert!(
                restored.price_axis_label && restored.time_axis_label,
                "{finish}"
            );
            assert!(!restored.italic, "{finish}");
            assert_eq!(restored.glyph_angle, 0.0, "{finish}");
            assert!(!restored.text_follows_zoom, "{finish}");
            assert_eq!(restored.note_text_style, None, "{finish}");
            assert_eq!(
                (restored.text_wrap, restored.text_wrap_width),
                (false, 200.0),
                "{finish}"
            );
            assert_eq!(restored.text_color, Some([41, 98, 255, 255]), "{finish}");
            assert_eq!(restored.line_measurements, Default::default(), "{finish}");
            assert_eq!(
                restored.range_settings,
                crate::drawings::RangeSettings::default(),
                "{finish}"
            );
            assert!(!restored.theme_shade, "{finish}");
            assert_eq!(restored.anchor_price_style, None, "{finish}");
            assert_eq!(
                restored.middle_line,
                crate::drawings::MiddleLineStyle::default(),
                "{finish}"
            );
            assert!(restored.elliott_show_wave, "{finish}");
            assert_eq!(restored.forecast, None, "{finish}");
            assert_eq!(restored.table, None, "{finish}");
            assert_eq!(chart.drawings().drawings[0].stroke_width, 20.0, "{finish}");
            assert_eq!(chart.drawings().drawings[1].stroke_width, 4.0, "{finish}");
            assert_eq!(
                chart.drawings().drawings[0].chart_points[1].price,
                110.0,
                "{finish}"
            );
            assert!(!chart.can_undo(), "{finish}: exactly one accepted step");
            if finish == "Ok" {
                assert!(chart.redo());
                assert_eq!(chart.drawings().drawings[0].stroke_width, 5.0);
            } else {
                assert!(chart.redo(), "{finish}: cancellation retains existing redo");
                assert_eq!(chart.drawings().drawings[2].stroke_width, 8.0);
                assert_eq!(chart.drawings().drawings[0].stroke_width, 20.0);
            }
        }
    }

    /// A previewed image opacity is restored by Cancel and committed by Ok as one undo step,
    /// without touching a frame resized while the dialog is open.
    #[test]
    fn image_opacity_preview_restores_or_commits_once() {
        for finish in ["Ok", "Cancel"] {
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::new());
            let mut picture = Drawing::new(7, Tool::Image);
            picture.chart_points = vec![ChartPoint::new(2.0, 100.0)];
            chart.drawings_mut().drawings.push(picture);
            let mut dialog = DrawingPropertiesDialog::new();
            dialog.open_for_chart(&chart, DrawingId(7));
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            chart.drawings_mut().drawings[0]
                .image
                .get_or_insert_with(Default::default)
                .size = Some([200.0, 100.0]);
            let settings = &mut dialog.props.tool_settings.as_mut().unwrap().settings;
            settings.image.get_or_insert_with(Default::default).opacity = 64;
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            assert_eq!(chart.drawings().drawings[0].image.unwrap().opacity, 64);
            click(&mut dialog, &mut chart, &ctx, finish);
            let image = chart.drawings().drawings[0].image.unwrap();
            assert_eq!(image.size, Some([200.0, 100.0]), "{finish}");
            if finish == "Ok" {
                assert_eq!(image.opacity, 64);
                assert!(chart.undo());
            }
            assert_eq!(
                chart.drawings().drawings[0]
                    .image
                    .map_or(255, |c| c.opacity),
                255,
                "{finish}"
            );
            assert!(!chart.can_undo(), "{finish}");
        }
    }

    /// Reset uses the chart's creation style, including a host tint or a saved tool default.
    #[test]
    fn reset_to_defaults_keeps_the_host_colour() {
        let host = [10, 200, 30, 255];
        for (tool, custom, saved) in [
            (Tool::TrendLine, false, false),
            (Tool::TrendLine, false, true),
            (Tool::Rect, false, false),
            (Tool::Rect, false, true),
            (Tool::Polyline, true, true),
        ] {
            let ctx = Context::default();
            let mut chart = Chart::new(BarData::new());
            let manager = chart.drawings_mut();
            manager.options.default_color = Some(host);
            if saved {
                let mut defaults = Drawing::with_color(0, tool, [60, 70, 80, 255]);
                defaults.fill_color = Some([90, 99, 111, 85]);
                if custom {
                    defaults.custom_tool = Some("host-path".into());
                }
                defaults.stroke_width = 3.0;
                manager.save_tool_default(DrawingTemplate::from_drawing(&defaults));
            }
            let mut drawing = if custom {
                manager.new_custom_drawing(7, "host-path")
            } else {
                manager.new_drawing(7, tool)
            };
            drawing.stroke_width = 5.0;
            drawing.chart_points = vec![ChartPoint::new(1.0, 100.0), ChartPoint::new(2.0, 110.0)];
            let anchors = drawing.chart_points.clone();
            manager.drawings.push(drawing);
            let mut dialog = DrawingPropertiesDialog::new();
            assert!(dialog.open_for_chart(&chart, DrawingId(7)));
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            click(&mut dialog, &mut chart, &ctx, "Template");
            click(&mut dialog, &mut chart, &ctx, "Reset to defaults");
            click(&mut dialog, &mut chart, &ctx, "Ok");
            let reset = &chart.drawings().drawings[0];
            assert_eq!(
                reset.color,
                if saved { [60, 70, 80, 255] } else { host },
                "{tool:?}"
            );
            assert_eq!(
                reset.fill_color,
                if saved {
                    Some([90, 99, 111, 85])
                } else if tool == Tool::Rect {
                    Some([10, 200, 30, 51])
                } else {
                    None
                },
                "{tool:?}"
            );
            assert_eq!(reset.stroke_width, if saved { 3.0 } else { 2.0 });
            assert_eq!(reset.chart_points, anchors);
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].stroke_width, 5.0);
            assert!(!chart.can_undo());
        }
    }

    #[test]
    fn footer_templates_apply_save_defaults_and_reset() {
        let ctx = Context::default();
        let mut chart = Chart::new(BarData::new());
        let mut drawing = Drawing::new(7, Tool::TrendLine);
        drawing.arrow_end_style = crate::drawings::ArrowStyle::Circle;
        chart.drawings_mut().drawings.push(drawing);
        let mut dialog = DrawingPropertiesDialog::new();
        dialog.open_for_chart(&chart, DrawingId(7));
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        dialog.props.line_width = 7.0;
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(&mut dialog, &mut chart, &ctx, "Save as template…");
        frame(
            &mut dialog,
            &mut chart,
            &ctx,
            vec![Event::Text("Thick line".into())],
        );
        click(&mut dialog, &mut chart, &ctx, "Save");
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(&mut dialog, &mut chart, &ctx, "Reset to defaults");
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(&mut dialog, &mut chart, &ctx, "Thick line");
        assert_eq!(chart.drawings().drawings[0].stroke_width, 7.0);
        assert_eq!(
            chart.drawings().drawings[0].arrow_end_style,
            crate::drawings::ArrowStyle::Circle
        );
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(&mut dialog, &mut chart, &ctx, "Reset to defaults");
        assert_eq!(chart.drawings().drawings[0].stroke_width, 2.0);
        dialog.props.line_width = 4.0;
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(
            &mut dialog,
            &mut chart,
            &ctx,
            "Save as default for this tool",
        );
        assert_eq!(
            chart.drawings().tool_defaults[&Tool::TrendLine]
                .settings
                .stroke_width,
            4.0
        );
        click(&mut dialog, &mut chart, &ctx, "Ok");
        assert_eq!(
            chart.drawings().tool_defaults[&Tool::TrendLine]
                .settings
                .stroke_width,
            4.0
        );
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].stroke_width, 2.0);
        // A host tool saves its default under its id and leaves the polyline's alone.
        let polyline =
            DrawingTemplate::from_drawing(&chart.drawings().new_drawing(0, Tool::Polyline));
        chart
            .drawings_mut()
            .tool_defaults
            .insert(Tool::Polyline, polyline.clone());
        let mut custom = Drawing::custom("host-path");
        custom.id = 8;
        chart.drawings_mut().drawings.push(custom);
        dialog.open_for_chart(&chart, DrawingId(8));
        dialog.props.line_width = 5.0;
        // The menu hides the row for a host tool, but a host may still set the flag.
        dialog.props.save_as_default = true;
        click(&mut dialog, &mut chart, &ctx, "Ok");
        assert_eq!(
            serde_json::to_value(&chart.drawings().tool_defaults[&Tool::Polyline]).unwrap(),
            serde_json::to_value(&polyline).unwrap()
        );
        assert_eq!(chart.drawings().drawings[1].stroke_width, 5.0);
        assert_eq!(
            chart.drawings().custom_tool_defaults["host-path"]
                .settings
                .stroke_width,
            5.0
        );
    }

    /// Several tools keep a template of one name apart in the host's storage, and each lists and
    /// deletes only its own. A template saved under its name alone, before templates were kept
    /// per tool, loads under its tool and is replaced in place.
    #[test]
    fn drawing_templates_of_different_tools_keep_one_name_apart() {
        use crate::data::Reply;
        use crate::services::Storage;
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Host(Mutex<BTreeMap<String, String>>);
        impl Storage for Host {
            fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
                reply.send(Ok(self.0.lock().unwrap().keys().cloned().collect()));
            }
            fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
                reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
            }
            fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
                self.0.lock().unwrap().insert(name.into(), document);
                reply.send(Ok(()));
            }
            fn delete(&self, _: StoreKind, name: &str, reply: Reply<()>) {
                self.0.lock().unwrap().remove(name);
                reply.send(Ok(()));
            }
        }
        let host = Arc::new(Host::default());
        let legacy = DrawingTemplate::from_drawing(&Drawing::new(0, Tool::HorizontalLine));
        host.0.lock().unwrap().insert(
            "Legacy".into(),
            serde_json::json!({ "version": 1, "template": legacy }).to_string(),
        );
        let documents = || host.0.lock().unwrap().keys().cloned().collect::<Vec<_>>();
        let ctx = Context::default();
        let mut chart = Chart::new(BarData::new());
        let mut custom = Drawing::custom("host-path");
        custom.id = 9;
        chart.drawings_mut().drawings = vec![
            Drawing::new(7, Tool::HorizontalLine),
            Drawing::new(8, Tool::VerticalLine),
            custom,
            Drawing::new(10, Tool::Polyline),
        ];
        let save_as = |dialog: &mut DrawingPropertiesDialog, chart: &mut Chart, name: &str| {
            click(dialog, chart, &ctx, "Template");
            click(dialog, chart, &ctx, "Save as template…");
            frame(dialog, chart, &ctx, vec![Event::Text(name.into())]);
            click(dialog, chart, &ctx, "Save");
        };
        let menu = |dialog: &mut DrawingPropertiesDialog, chart: &mut Chart, id| {
            dialog.close();
            frame(dialog, chart, &ctx, vec![]);
            dialog.open_for_chart(chart, DrawingId(id));
            frame(dialog, chart, &ctx, vec![]);
            click(dialog, chart, &ctx, "Template");
            let labels = frame(dialog, chart, &ctx, vec![]);
            labels.into_iter().map(|(text, _)| text).collect::<Vec<_>>()
        };
        let mut dialog = DrawingPropertiesDialog::new();
        dialog.templates.set_storage(host.clone());
        for id in [7, 8, 9, 10] {
            dialog.open_for_chart(&chart, DrawingId(id));
            frame(&mut dialog, &mut chart, &ctx, vec![]);
            save_as(&mut dialog, &mut chart, "Red");
        }
        assert_eq!(
            documents(),
            [
                "HorizontalLine/Red",
                "Legacy",
                "Polyline/Red",
                "Polyline:host-path/Red",
                "VerticalLine/Red"
            ]
        );
        // After a restart: one list and five loads.
        let mut dialog = DrawingPropertiesDialog::new();
        dialog.templates.set_storage(host.clone());
        for _ in 0..6 {
            frame(&mut dialog, &mut chart, &ctx, vec![]);
        }
        let labels = menu(&mut dialog, &mut chart, 7);
        assert!(
            ["Red", "Legacy"]
                .iter()
                .all(|name| labels.contains(&name.to_string())),
            "{labels:?}"
        );
        let labels = menu(&mut dialog, &mut chart, 8);
        assert!(
            labels.contains(&"Red".into()) && !labels.contains(&"Legacy".into()),
            "{labels:?}"
        );
        click(&mut dialog, &mut chart, &ctx, "Delete Red");
        assert_eq!(
            documents(),
            [
                "HorizontalLine/Red",
                "Legacy",
                "Polyline/Red",
                "Polyline:host-path/Red"
            ]
        );
        dialog.close();
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        dialog.open_for_chart(&chart, DrawingId(7));
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        dialog.props.line_width = 3.0;
        save_as(&mut dialog, &mut chart, "Legacy");
        assert_eq!(documents().len(), 4, "replaced in place: {:?}", documents());
        let saved: serde_json::Value =
            serde_json::from_str(&host.0.lock().unwrap()["Legacy"]).unwrap();
        assert_eq!(saved["template"]["settings"]["stroke_width"], 3.0);
    }

    /// A template loaded in the dialog of one drawing gives every unlocked selected drawing of its
    /// tool all of its style, whatever each held before, and leaves the selected drawings of other
    /// tools and the locked ones as they were. One undo restores them all.
    #[test]
    fn a_loaded_template_restyles_every_unlocked_selected_drawing_of_its_tool() {
        let ctx = Context::default();
        let mut chart = Chart::new(BarData::new());
        let anchors = [ChartPoint::new(1.0, 100.0), ChartPoint::new(3.0, 110.0)];
        for (id, tool, color) in [
            (1, Tool::Rect, [242, 54, 69, 255]),
            (2, Tool::Rect, [41, 98, 255, 255]),
            (3, Tool::TrendLine, [41, 98, 255, 255]),
            (4, Tool::Rect, [41, 98, 255, 255]),
        ] {
            let mut drawing = Drawing::new(id, tool);
            (drawing.color, drawing.stroke_width, drawing.fill_color) = (color, 1.0, None);
            drawing.extend_right = id == 2;
            drawing.locked = id == 4;
            drawing.chart_points = anchors.to_vec();
            chart.drawings_mut().drawings.push(drawing);
        }
        chart.drawings_mut().select_drawings([1, 2, 3, 4]);
        let mut zone = Drawing::new(0, Tool::Rect);
        (zone.color, zone.stroke_width) = ([242, 54, 69, 255], 2.0);
        zone.line_style = crate::model::LineStyle::Dashed;
        zone.fill_color = Some([76, 175, 80, 51]);
        let template = DrawingTemplate::from_drawing(&zone);
        let mut dialog = DrawingPropertiesDialog::new();
        dialog
            .templates
            .save_for_tool("Red zone", template.clone())
            .unwrap();
        let before = serde_json::to_value(&chart.drawings().drawings).unwrap();
        dialog.open_for_chart(&chart, DrawingId(1));
        frame(&mut dialog, &mut chart, &ctx, vec![]);
        click(&mut dialog, &mut chart, &ctx, "Template");
        click(&mut dialog, &mut chart, &ctx, "Red zone");
        click(&mut dialog, &mut chart, &ctx, "Ok");
        let style = |drawing: &Drawing| {
            serde_json::to_value(DrawingTemplate::from_drawing(drawing)).unwrap()
        };
        let drawings = &chart.drawings().drawings;
        let want = serde_json::to_value(&template).unwrap();
        assert_eq!(style(&drawings[0]), want);
        assert_eq!(style(&drawings[1]), want, "the blue rectangle");
        assert_eq!(drawings[1].chart_points, anchors);
        for index in [2, 3] {
            assert_eq!(
                serde_json::to_value(&drawings[index]).unwrap(),
                before[index]
            );
        }
        assert!(chart.undo());
        assert_eq!(
            serde_json::to_value(&chart.drawings().drawings).unwrap(),
            before
        );
        assert!(!chart.can_undo());
    }
}
