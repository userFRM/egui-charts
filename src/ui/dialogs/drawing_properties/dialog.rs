//! Drawing properties dialog - modal popup for editing drawing properties.

use egui::{Context, Event, Key, Modifiers, Ui, Vec2};

use super::actions::{DrawingId, DrawingPropertiesAction, DrawingProps};
use super::config::DrawingPropertiesConfig;
use super::tabs::{
    CoordinatesState, CoordinatesTab, InputsTab, PropertiesTab, StyleTab, TextTab, VisibilityTab,
};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::ColorPickerState;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::{DialogFooter, DialogFrame, dialog_header};
use crate::ui_kit::tab_bar::TabBar;

/// Drawing properties dialog
pub struct DrawingPropertiesDialog {
    pub config: DrawingPropertiesConfig,
    pub is_open: bool,
    pub drawing_id: Option<DrawingId>,
    pub drawing_name: String,
    pub active_tab: PropertiesTab,
    pub props: DrawingProps,
    pub coordinates: CoordinatesState,
    /// Validation error from the most recent Apply attempt.
    pub error: Option<String>,
}

impl Default for DrawingPropertiesDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingPropertiesDialog {
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
        }
    }

    /// Open the dialog for a specific drawing
    pub fn open(&mut self, id: DrawingId, name: String, props: DrawingProps) {
        self.is_open = true;
        self.drawing_id = Some(id);
        self.drawing_name = name;
        self.coordinates = props.coordinates.clone();
        self.error = None;
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
            drawing.tool_type.as_str().into(),
            DrawingProps::from_drawing(drawing, chart),
        );
        true
    }

    /// Show and apply validated edits through the chart's undo and persistence path.
    /// Apply is returned as a notification: the host must not apply it a second time.
    /// Delete and Clone remain host intents; Cancel leaves the live drawing untouched.
    pub fn show_and_apply(
        &mut self,
        ctx: &Context,
        chart: &mut crate::widget::Chart,
    ) -> DrawingPropertiesAction {
        let action = self.show(ctx);
        if let DrawingPropertiesAction::Apply(id, props) = &action
            && let Err(error) = props.apply_to_chart(*id, chart)
        {
            self.error = Some(error.into());
            self.is_open = true;
            return DrawingPropertiesAction::None;
        }
        action
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
        self.drawing_id = None;
    }

    /// Sync dialog open/close state from an external flag.
    ///
    /// Opening requires drawing context, so only close is synced here.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open && self.is_open {
            self.close();
        }
    }

    /// Show the dialog
    pub fn show(&mut self, ctx: &Context) -> DrawingPropertiesAction {
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
            .filter(|t| *t != PropertiesTab::Inputs || has_inputs)
            .collect();
        if self.active_tab == PropertiesTab::Inputs && !has_inputs {
            self.active_tab = PropertiesTab::Style;
        }
        TabBar::new(&tabs, &mut self.active_tab).show(ui);

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
                    StyleTab::show(ui, &mut self.props, &mut state.pickers);
                    ui.checkbox(
                        &mut self.props.save_as_default,
                        "Save as default for this tool",
                    );
                }
                PropertiesTab::Inputs => {
                    let color = self.props.color.to_srgba_unmultiplied();
                    if let Some(template) = &mut self.props.tool_settings {
                        template.settings.color = color;
                        InputsTab::show(ui, &mut template.settings, &mut state.level_pickers);
                    }
                }
                PropertiesTab::Coordinates => CoordinatesTab::show(ui, &mut self.coordinates),
                PropertiesTab::Visibility => VisibilityTab::show(ui, &mut self.props),
                PropertiesTab::Text => TextTab::show(ui, &mut self.props),
            });

        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }

        let mut delete = false;
        let mut clone = false;
        let (mut apply, cancel_clicked) = DialogFooter::new("Apply")
            .secondary("Cancel")
            .show_with_left(ui, |ui| {
                ui.spacing_mut().item_spacing.x = ui.active_theme().spacing.lg;
                delete = Button::icon_and_text(&icons::TRASH, "Delete")
                    .ghost()
                    .show(ui)
                    .clicked();
                clone = Button::icon_and_text(&icons::DUPLICATE, "Clone")
                    .ghost()
                    .show(ui)
                    .clicked();
            });
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
            .chain(&state.level_pickers)
            .any(|p| p.is_open)
        {
            if consume(Key::Escape) {
                state
                    .pickers
                    .iter_mut()
                    .chain(&mut state.level_pickers)
                    .for_each(|p| p.is_open = false);
            }
        } else if !popup_open && !state.had_focus {
            cancel |= consume(Key::Escape);
            apply |= !cancel && consume(Key::Enter);
        }
        state.had_focus = ctx.memory(|m| m.focused()).is_some();

        let action = match self.drawing_id {
            Some(id) if delete => DrawingPropertiesAction::Delete(id),
            Some(id) if apply => {
                self.props.coordinates = self.coordinates.clone();
                DrawingPropertiesAction::Apply(id, self.props.clone())
            }
            _ if cancel => DrawingPropertiesAction::Cancel,
            Some(id) if clone => DrawingPropertiesAction::Clone(id),
            _ => DrawingPropertiesAction::None,
        };
        // Clone keeps the dialog open; a closed dialog starts with fresh state next time.
        if matches!(
            action,
            DrawingPropertiesAction::None | DrawingPropertiesAction::Clone(_)
        ) {
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

/// Dialog state kept in egui memory, so the public struct keeps its fields.
#[derive(Clone, Default)]
struct UiState {
    /// Colour and fill colour pickers of the Style tab.
    pickers: [ColorPickerState; 2],
    /// Pickers for the current tool's editable levels.
    level_pickers: Vec<ColorPickerState>,
    /// Whether a widget had keyboard focus at the end of the previous frame.
    had_focus: bool,
}
