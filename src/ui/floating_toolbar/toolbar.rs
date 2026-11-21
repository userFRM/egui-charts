//! Floating Selection Toolbar Implementation
//!
//! Floating toolbar for object customization.

use crate::drawings::{ChartPoint, DrawingToolType, LineMeasurements};
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::LineStyle;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui::app_state::set_control_visible;
use crate::ui::context_menu::drawing::{DrawingContextMenu, DrawingContextMenuAction};
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::color_picker::keep_opacity;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState};
use crate::widget::Chart;
use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

/// Action that can be triggered from the floating toolbar
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FloatingToolbarAction {
    /// Change a color slot (index, new color)
    ChangeColor(usize, Color32),
    /// Change the stroke width in pixels.
    ChangeWidth(f32),
    /// Change the stroke pattern.
    ChangeStyle(LineStyle),
    /// An entry from the drawing context menu.
    DrawingMenu(DrawingContextMenuAction),
    /// Open settings dialog
    OpenSettings,
    /// Add an alert on the selected object
    AddAlert,
    /// Duplicate the selected object
    Duplicate,
    /// Delete the selected object
    Delete,
    /// Lock the selected drawings, or unlock them when the drawing the toolbar follows is
    /// locked. [`FloatingSelectionToolbar::show_and_apply`] returns it as
    /// [`DrawingContextMenuAction::Lock`].
    ToggleLock,
    /// Hide the selected drawings, or show them when the drawing the toolbar follows is hidden.
    /// [`FloatingSelectionToolbar::show_and_apply`] returns it as
    /// [`DrawingContextMenuAction::Hide`].
    ToggleVisibility,
    /// Close the toolbar
    Close,
}

/// Configuration for color slots shown in toolbar
#[derive(Clone, Debug)]
pub struct ColorSlot {
    /// Text displayed by the component.
    pub label: &'static str,
    /// Color.
    pub color: Color32,
}

/// Floating selection toolbar state
#[derive(Clone, Debug)]
pub struct FloatingSelectionToolbar {
    /// Whether toolbar is visible
    visible: bool,
    /// Current position
    position: Pos2,
    /// Color slots to display (mutable for color picker)
    color_slots: Vec<ColorSlot>,
    /// Palette state per colour slot
    pickers: Vec<ColorPickerState>,
    /// Shared line width, mixed when the inner value is `None`; hidden when the followed tool
    /// draws no width
    width: Option<Option<f32>>,
    /// Shared line style, mixed when the inner value is `None`; hidden when the followed tool
    /// draws no style
    line_style: Option<Option<LineStyle>>,
    /// Symbol and id of the drawing the toolbar follows
    selected: Option<(String, usize)>,
    /// What the toolbar last stayed clear of the drawing's label or readout card for: the
    /// drawing's readouts and anchors, the toolbar's size and the painted label's size
    placed_for: Option<(LineMeasurements, Vec<ChartPoint>, Vec2, Option<Vec2>)>,
    /// Whether a custom colour gesture is still open as one undo step
    colour_drag: bool,
    /// Last palette or custom pick before taking the followed drawing's opacity, and whether
    /// the custom editor set the opacity
    last_pick: Option<(Color32, bool)>,
    /// The "More" menu
    menu: DrawingContextMenu,
    /// Whether object is locked
    is_locked: bool,
    /// Whether the user can edit the object
    user_edit_enabled: bool,
    /// Whether object is visible
    is_visible: bool,
    /// Last rendered rect (for hit testing)
    last_rect: Rect,
}

impl Default for FloatingSelectionToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingSelectionToolbar {
    /// Create a new hidden floating selection toolbar at the default position
    pub fn new() -> Self {
        Self {
            visible: false,
            position: Pos2::new(
                DESIGN_TOKENS.sizing.floating_toolbar.default_x,
                DESIGN_TOKENS.sizing.floating_toolbar.default_y,
            ),
            color_slots: Vec::new(),
            pickers: Vec::new(),
            width: None,
            line_style: None,
            selected: None,
            placed_for: None,
            colour_drag: false,
            last_pick: None,
            menu: DrawingContextMenu::new(),
            is_locked: false,
            user_edit_enabled: true,
            is_visible: true,
            last_rect: Rect::NOTHING,
        }
    }

    /// Show the toolbar at a specific position
    pub fn show_at(&mut self, position: Pos2) {
        self.visible = true;
        self.position = position;
    }

    /// Hide the toolbar
    pub fn hide(&mut self) {
        self.visible = false;
        self.selected = None;
        self.placed_for = None;
        self.pickers.clear();
        self.menu.close();
    }

    /// Check if toolbar is visible
    #[allow(clippy::misnamed_getters)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if a point is inside the toolbar area (for hit testing)
    pub fn contains_point(&self, point: Pos2) -> bool {
        self.visible && self.last_rect.contains(point)
    }

    /// Set the color slots to display
    pub fn set_colors(&mut self, slots: Vec<ColorSlot>) {
        self.color_slots = slots;
    }

    /// Get mutable reference to color slots (for updating from outside)
    pub fn color_slots_mut(&mut self) -> &mut Vec<ColorSlot> {
        &mut self.color_slots
    }

    /// Set locked state
    pub fn set_locked(&mut self, locked: bool) {
        self.is_locked = locked;
    }

    /// Set whether the user can edit the object; off disables its colour, width and style
    /// controls, Reverse position, Lock and Delete, as the drawing menu does.
    pub fn set_user_edit_enabled(&mut self, enabled: bool) {
        self.user_edit_enabled = enabled;
    }

    /// Set visibility state
    pub fn set_object_visible(&mut self, visible: bool) {
        self.is_visible = visible;
    }

    /// Show or hide a control the toolbar offers. Only [`Control::Alerts`] applies here: hiding it
    /// leaves the Add alert button and the More menu's alert row and its separator out, as it does
    /// in the drawing context menu.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        set_control_visible(&mut self.menu.hidden, control, visible);
    }

    /// Follow the selected drawing, applying quick style edits as undoable chart actions. Colour,
    /// fill, width and style edits reach every selected drawing that is unlocked, accepts user edits
    /// and draws the property, in one undo step; a palette colour keeps each drawing's opacity.
    /// Width and style read Mixed while the selected drawings differ.
    /// It offers Add alert when the drawing has an alert level on a chart with a symbol, as the
    /// drawing context menu does, unless [`Control::Alerts`] is hidden through
    /// [`Self::set_control_visible`].
    /// Returns menu actions for the host's dialogs, clipboard and drawing commands.
    /// Call each frame, including after deselection, to finish a continuous colour edit.
    pub fn show_and_apply(
        &mut self,
        ui: &mut Ui,
        chart: &mut Chart,
    ) -> Option<DrawingContextMenuAction> {
        if self.colour_drag && !ui.input(|i| i.pointer.any_down()) {
            chart.user_action(|chart| chart.drawings_mut().end_drag_handle());
            self.colour_drag = false;
        }
        let drawing = chart.drawings().sel_drawing().and_then(|id| {
            chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id && d.visible && d.completed)
        });
        let Some(drawing) = drawing else {
            self.hide();
            return None;
        };
        let id = drawing.id;
        let selected = (chart.symbol().to_owned(), id);
        if self.selected.as_ref() != Some(&selected) {
            self.hide();
            let anchor = drawing
                .points
                .first()
                .copied()
                .unwrap_or(chart.get_rendered_price_rect().center());
            // Sits just above and right of the first anchor, clear of the line it styles.
            self.show_at(anchor + Vec2::new(12.0, -52.0));
            self.selected = Some(selected);
        }
        // Waits for the pointer so that the toolbar holds still while anchors are dragged; only
        // edits of the drawing, of its label's size and the measured size move it, and only off
        // the label or card.
        // shortcut: panning or zooming can still slide the label under the toolbar, which does
        // not follow the view; compare the anchors' screen points if that proves to matter.
        let size = self.size();
        let label = chart.drawings().painted_label_rect(drawing);
        let label_size = label.map(|label| label.size());
        if !ui.input(|i| i.pointer.any_down())
            && self
                .placed_for
                .as_ref()
                .is_none_or(|(readouts, points, placed, painted)| {
                    *readouts != drawing.line_measurements
                        || points.as_slice() != drawing.chart_points.as_slice()
                        || *placed != size
                        || *painted != label_size
                })
        {
            if let Some(card) = chart.line_measurement_card_rect(ui, drawing).or(label) {
                self.position = clear_of_card(
                    self.position,
                    size,
                    card,
                    &drawing.points,
                    drawing.trend_line_ends(ui.ctx().content_rect()),
                    ui.ctx().content_rect(),
                );
            }
            self.placed_for = Some((
                drawing.line_measurements,
                drawing.chart_points.clone(),
                size,
                label_size,
            ));
        }
        self.set_locked(drawing.locked);
        self.set_user_edit_enabled(drawing.user_edit_enabled);
        let (editable, unprotected) = (drawing.accepts_user_edits(), drawing.user_edit_enabled);
        self.set_object_visible(drawing.visible);
        // Without a colour slot, the fill is the first slot.
        let fill_slot = usize::from(drawing.tool_type.supports_color());
        self.set_colors(if drawing.tool_type.supports_color() {
            vec![ColorSlot {
                // A flag mark and a pin paint their marker in it. Text always paints its text in
                // the slot's colour; a note or callout does until its text has its own colour.
                label: match drawing.tool_type {
                    DrawingToolType::FlagNote | DrawingToolType::AnchoredNote => "Color",
                    DrawingToolType::TextLabel | DrawingToolType::AnchoredText => "Text color",
                    DrawingToolType::Note | DrawingToolType::Callout
                        if drawing.text_color.is_none() =>
                    {
                        "Text color"
                    }
                    _ => "Line color",
                },
                color: drawing
                    .text_color_override()
                    .map_or(drawing.color32(), |[r, g, b, a]| {
                        Color32::from_rgba_unmultiplied(r, g, b, a)
                    }),
            }]
        } else {
            Vec::new()
        });
        if drawing.tool_type.supports_fill() {
            self.color_slots.push(ColorSlot {
                label: "Fill color",
                color: drawing.fill_color32().unwrap_or(Color32::TRANSPARENT),
            });
        }
        self.width = drawing.tool_type.supports_line_width().then(|| {
            shared(chart, DrawingToolType::supports_line_width, |d| {
                d.stroke_width
            })
        });
        self.line_style = drawing.tool_type.supports_line_style().then(|| {
            shared(chart, DrawingToolType::supports_line_style, |d| {
                d.line_style
            })
        });
        // Refresh context without reopening the menu or resetting its dismiss state.
        if !self.menu.is_open() {
            self.menu.open_on(ui.ctx(), chart, self.position, id);
            self.menu.close();
        }
        let multiple = chart.drawings().selected_ids().nth(1).is_some();
        let action = self.show_with_shortcuts(ui, chart.shortcuts(), multiple)?;
        // Only editable selected drawings that draw the property take its edit.
        let targets = |chart: &Chart, draws: fn(&DrawingToolType) -> bool| -> Vec<usize> {
            let drawings = chart.drawings();
            drawings
                .drawings
                .iter()
                .filter(|d| {
                    drawings.is_selected(d.id) && d.accepts_user_edits() && draws(&d.tool_type)
                })
                .map(|d| d.id)
                .collect()
        };
        match action {
            FloatingToolbarAction::ChangeColor(slot, color) if editable => {
                let fill = slot == fill_slot;
                let (picked, custom) = self.last_pick.take().unwrap_or((color, true));
                let [r, g, b, _] = picked.to_srgba_unmultiplied();
                let ids = targets(
                    chart,
                    if fill {
                        DrawingToolType::supports_fill
                    } else {
                        DrawingToolType::supports_color
                    },
                );
                let edit = |d: &mut crate::drawings::Drawing| {
                    let own = if fill {
                        d.fill_color32().unwrap_or(Color32::TRANSPARENT)
                    } else {
                        d.color32()
                    };
                    // A swatch keeps each drawing's opacity; custom opacity reaches them all.
                    let rgba = [r, g, b, keep_opacity(picked, own, custom).a()];
                    if fill {
                        d.fill_color = Some(rgba)
                    } else {
                        d.recolor(rgba)
                    }
                };
                let down = ui.input(|i| i.pointer.any_down());
                chart.user_action(|chart| {
                    let _ = if down {
                        chart.drawings_mut().drag_edit_drawings(&ids, edit)
                    } else {
                        chart.drawings_mut().edit_drawings(&ids, edit)
                    };
                });
                self.colour_drag = down;
            }
            FloatingToolbarAction::ChangeWidth(width) if editable => {
                let ids = targets(chart, DrawingToolType::supports_line_width);
                chart.user_action(|chart| {
                    let _ = chart
                        .drawings_mut()
                        .edit_drawings(&ids, |d| d.stroke_width = width);
                });
            }
            FloatingToolbarAction::ChangeStyle(style) if editable => {
                let ids = targets(chart, DrawingToolType::supports_line_style);
                chart.user_action(|chart| {
                    let _ = chart
                        .drawings_mut()
                        .edit_drawings(&ids, |d| d.line_style = style);
                });
            }
            FloatingToolbarAction::OpenSettings => return Some(DrawingContextMenuAction::Edit),
            FloatingToolbarAction::Duplicate => return Some(DrawingContextMenuAction::Clone),
            FloatingToolbarAction::ToggleLock if unprotected => {
                return Some(DrawingContextMenuAction::Lock);
            }
            FloatingToolbarAction::ToggleVisibility => return Some(DrawingContextMenuAction::Hide),
            FloatingToolbarAction::Delete if unprotected => {
                return Some(DrawingContextMenuAction::Delete);
            }
            FloatingToolbarAction::AddAlert => {
                return self
                    .menu
                    .alert
                    .clone()
                    .map(|alert| DrawingContextMenuAction::AddAlert(Box::new(alert)));
            }
            FloatingToolbarAction::DrawingMenu(DrawingContextMenuAction::ReversePosition)
                if editable =>
            {
                let _ = chart.reverse_position_drawing(id);
            }
            FloatingToolbarAction::DrawingMenu(action)
                if action != DrawingContextMenuAction::ReversePosition
                    && !action.apply_interval_visibility(chart, id) =>
            {
                return Some(action);
            }
            _ => {}
        }
        ui.ctx().request_repaint();
        None
    }

    /// The toolbar's measured size, or its typical size before its first frame.
    fn size(&self) -> Vec2 {
        if self.last_rect.is_finite() {
            self.last_rect.size()
        } else {
            // A line drawing's width until egui has measured the bar once.
            Vec2::new(332.0, DESIGN_TOKENS.sizing.floating_toolbar.height)
        }
    }

    /// Render the toolbar.
    pub fn show(&mut self, ui: &mut Ui) -> Option<FloatingToolbarAction> {
        self.show_with_shortcuts(ui, &crate::widget::ShortcutMap::default(), false)
    }

    fn show_with_shortcuts(
        &mut self,
        ui: &mut Ui,
        shortcuts: &crate::widget::ShortcutMap,
        multiple: bool,
    ) -> Option<FloatingToolbarAction> {
        if !self.visible {
            return None;
        }
        let bounds = ui.ctx().content_rect();
        let size = self.size();
        self.position.x = self
            .position
            .x
            .min(bounds.right() - size.x)
            .max(bounds.left());
        self.position.y = self
            .position
            .y
            .min(bounds.bottom() - size.y)
            .max(bounds.top());
        self.pickers
            .resize_with(self.color_slots.len(), ColorPickerState::default);
        let mut action = None;
        let area = egui::Area::new(egui::Id::unique("floating_selection_toolbar"))
            .fixed_pos(self.position)
            .default_size(size)
            .movable(false)
            .constrain_to(bounds)
            .order(egui::Order::Middle)
            .show(ui.ctx(), |ui| {
                ui.set_max_width(bounds.width());
                egui::Frame::popup(ui.style())
                    .corner_radius(ui.active_theme().radius.md)
                    .inner_margin(egui::Margin::symmetric(
                        ui.active_theme().spacing.md as i8,
                        ui.active_theme().spacing.sm as i8,
                    ))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = ui.active_theme().spacing.sm;
                            let drag = Self::render_drag_handle(ui);
                            if drag.dragged() {
                                self.position += drag.drag_delta();
                            }
                            Self::render_separator(ui);
                            ui.add_enabled_ui(!self.is_locked && self.user_edit_enabled, |ui| {
                                ui.horizontal(|ui| {
                                    for (idx, slot) in self.color_slots.iter_mut().enumerate() {
                                        let picker = &mut self.pickers[idx];
                                        picker.sel_color = slot.color;
                                        let custom = picker.custom_mode;
                                        let picked =
                                            ColorPicker::new(picker, ("selection_color", idx))
                                                .show(ui);
                                        picked
                                            .response
                                            .on_hover_text(l10n::text(ui.ctx(), slot.label));
                                        if let Some(color) = picked.inner {
                                            self.last_pick =
                                                Some((color, custom || picker.custom_mode));
                                            let color = keep_opacity(
                                                color,
                                                slot.color,
                                                custom || picker.custom_mode,
                                            );
                                            // The followed colour can already be the pick while others differ.
                                            if color != slot.color || multiple {
                                                slot.color = color;
                                                action = Some(FloatingToolbarAction::ChangeColor(
                                                    idx, color,
                                                ));
                                            }
                                        }
                                    }
                                    if let Some(width) = &mut self.width
                                        && ui.combo_select_width(
                                            "selection_width",
                                            width,
                                            [1.0, 2.0, 3.0, 4.0].map(Some),
                                            |v| {
                                                v.map_or_else(
                                                    || "Mixed".to_owned(),
                                                    |v| format!("{v} px"),
                                                )
                                            },
                                            48.0,
                                        )
                                        && let Some(width) = *width
                                    {
                                        action = Some(FloatingToolbarAction::ChangeWidth(width));
                                    }
                                    let ctx = ui.ctx().clone();
                                    if let Some(style) = &mut self.line_style
                                        && ui.combo_select_width(
                                            "selection_style",
                                            style,
                                            LineStyle::all().iter().copied().map(Some),
                                            |v| l10n::text(&ctx, v.map_or("Mixed", |v| v.name())),
                                            72.0,
                                        )
                                        && let Some(style) = *style
                                    {
                                        action = Some(FloatingToolbarAction::ChangeStyle(style));
                                    }
                                });
                            });
                            Self::render_separator(ui);
                            if IconButton::new(&icons::SETTINGS)
                                .size(ButtonSize::SM)
                                .tooltip("Settings")
                                .show(ui)
                                .clicked()
                            {
                                action = Some(FloatingToolbarAction::OpenSettings);
                            }
                            if self.menu.position_direction_available()
                                && IconButton::new(&icons::ROTATE_CCW)
                                    .size(ButtonSize::SM)
                                    .tooltip("Reverse position")
                                    .enabled(!self.is_locked && self.user_edit_enabled)
                                    .show(ui)
                                    .clicked()
                            {
                                action = Some(FloatingToolbarAction::DrawingMenu(
                                    DrawingContextMenuAction::ReversePosition,
                                ));
                            }
                            if self.menu.alert.is_some()
                                && !self.menu.hidden.contains(&Control::Alerts)
                                && IconButton::new(&icons::ALERTS)
                                    .size(ButtonSize::SM)
                                    .tooltip("Add alert")
                                    .show(ui)
                                    .clicked()
                            {
                                action = Some(FloatingToolbarAction::AddAlert);
                            }
                            if IconButton::new(if self.is_locked {
                                &icons::LOCK
                            } else {
                                &icons::UNLOCK
                            })
                            .size(ButtonSize::SM)
                            .tooltip(if self.is_locked { "Unlock" } else { "Lock" })
                            .enabled(self.user_edit_enabled)
                            .show(ui)
                            .clicked()
                            {
                                action = Some(FloatingToolbarAction::ToggleLock);
                            }
                            if IconButton::new(&icons::TRASH)
                                .size(ButtonSize::SM)
                                .tooltip("Delete")
                                .enabled(!self.is_locked && self.user_edit_enabled)
                                .show(ui)
                                .clicked()
                            {
                                action = Some(FloatingToolbarAction::Delete);
                            }
                            Self::render_separator(ui);
                            let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                                .size(ButtonSize::SM)
                                .tooltip("More")
                                .show(ui);
                            if more.clicked() {
                                self.menu.set_context(
                                    self.menu.drawing_type.clone(),
                                    self.is_locked,
                                    self.is_visible,
                                );
                                self.menu.user_edit_enabled = self.user_edit_enabled;
                                self.menu.open(
                                    more.rect.left_bottom(),
                                    self.menu.drawing_id.unwrap_or_default(),
                                );
                            }
                        });
                    });
            });
        self.last_rect = area.response.rect;
        // Keep the next frame's anchor in step with egui's measured, constrained surface.
        if !ui.input(|i| i.pointer.any_down()) {
            self.position = self.last_rect.min;
        }
        self.menu
            .show_with_shortcuts(ui.ctx(), &ui.active_theme(), shortcuts);
        let menu_action = self.menu.take_action();
        if menu_action != DrawingContextMenuAction::None {
            action = Some(match menu_action {
                DrawingContextMenuAction::Edit => FloatingToolbarAction::OpenSettings,
                DrawingContextMenuAction::Clone => FloatingToolbarAction::Duplicate,
                DrawingContextMenuAction::Lock => FloatingToolbarAction::ToggleLock,
                DrawingContextMenuAction::Hide => FloatingToolbarAction::ToggleVisibility,
                DrawingContextMenuAction::Delete => FloatingToolbarAction::Delete,
                action => FloatingToolbarAction::DrawingMenu(action),
            });
        }
        action
    }

    fn render_drag_handle(ui: &mut Ui) -> egui::Response {
        let size = Vec2::new(
            ui.active_theme().spacing.lg + ui.active_theme().spacing.md,
            DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_height,
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::DRAG);

        let painter = ui.painter();
        let dot_color = ui.style().visuals.text_color().gamma_multiply(0.4);
        let dot_size = DESIGN_TOKENS.sizing.floating_toolbar.dot_size;
        let dot_spacing = DESIGN_TOKENS.sizing.floating_toolbar.dot_spacing;

        // Draw 6-dot drag handle pattern (2 columns, 3 rows)
        let start_x = rect.center().x - dot_spacing / 2.0;
        let start_y = rect.center().y - dot_spacing;

        for col in 0..2 {
            for row in 0..3 {
                let x = start_x + col as f32 * dot_spacing;
                let y = start_y + row as f32 * dot_spacing;
                painter.circle_filled(Pos2::new(x, y), dot_size, dot_color);
            }
        }
        response.on_hover_cursor(egui::CursorIcon::Grab)
    }

    fn render_separator(ui: &mut Ui) {
        let height = DESIGN_TOKENS.sizing.floating_toolbar.separator_height;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(DESIGN_TOKENS.stroke.hairline, height),
            Sense::hover(),
        );
        let painter = ui.painter();
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        painter.line_segment(
            [
                Pos2::new(rect.center().x, rect.min.y),
                Pos2::new(rect.center().x, rect.max.y),
            ],
            stroke,
        );
        ui.space_xs();
    }
}

/// The value of every selected drawing whose tool draws it, or `None` when they differ.
fn shared<T: PartialEq>(
    chart: &Chart,
    draws: fn(&DrawingToolType) -> bool,
    value: impl Fn(&crate::drawings::Drawing) -> T,
) -> Option<T> {
    let drawings = chart.drawings();
    let mut values = drawings
        .drawings
        .iter()
        .filter(|d| drawings.is_selected(d.id) && draws(&d.tool_type))
        .map(value);
    let first = values.next()?;
    values.all(|v| v == first).then_some(first)
}

/// Where a toolbar of `size` at `position` goes so that it does not cover a drawing's label or
/// readout `card`: where it is when clear of it, else above the card and `anchors`, below them,
/// above or below the card alone, or beside it, preferring a place off the drawing's `line`.
/// Unmoved when `bounds` has no such place.
fn clear_of_card(
    position: Pos2,
    size: Vec2,
    card: Rect,
    anchors: &[Pos2],
    line: Option<(Pos2, Pos2)>,
    bounds: Rect,
) -> Pos2 {
    let within = |position: Pos2| position.clamp(bounds.min, (bounds.max - size).max(bounds.min));
    let covers = |position: Pos2| Rect::from_min_size(position, size).intersects(card);
    if !covers(within(position)) {
        return position;
    }
    let gap = DESIGN_TOKENS.spacing.md;
    let drawing = anchors
        .iter()
        .fold(card, |rect, anchor| rect.union(Rect::from_pos(*anchor)));
    let Pos2 { x, y } = within(position);
    let places = [
        Pos2::new(x, drawing.top() - gap - size.y),
        Pos2::new(x, drawing.bottom() + gap),
        Pos2::new(x, card.top() - gap - size.y),
        Pos2::new(x, card.bottom() + gap),
        Pos2::new(card.left() - gap - size.x, y),
        Pos2::new(card.right() + gap, y),
    ];
    let free =
        |&place: &Pos2| bounds.contains_rect(Rect::from_min_size(place, size)) && !covers(place);
    let on_line = |place: Pos2| {
        line.is_some_and(|(a, b)| {
            crate::painting::dashed_line::clip_segment(a, b, Rect::from_min_size(place, size))
                .is_some()
        })
    };
    places
        .iter()
        .copied()
        .filter(|place| free(place))
        .find(|&place| !on_line(place))
        .or_else(|| places.iter().copied().find(|place| free(place)))
        .unwrap_or(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A palette swatch keeps the opacity of a translucent line, such as a highlighter's, and of
    /// a fill.
    #[test]
    fn palette_colours_keep_each_slot_opacity() {
        use crate::ui_kit::color_picker::MAIN_COLORS;
        let swatch = MAIN_COLORS[3];
        let [r, g, b, _] = swatch.to_srgba_unmultiplied();
        for slot in 0..2 {
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            toolbar.set_colors(vec![
                ColorSlot {
                    label: "Line color",
                    color: Color32::from_rgba_unmultiplied(242, 54, 69, 51),
                },
                ColorSlot {
                    label: "Fill color",
                    color: Color32::from_rgba_unmultiplied(0, 188, 212, 51),
                },
            ]);
            let mut pickers = vec![ColorPickerState::default(); 2];
            pickers[slot].is_open = true;
            toolbar.pickers = pickers;
            let mut frame = |events| {
                let mut action = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| action = toolbar.show(ui),
                );
                let well = output.shapes.iter().find_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.fill == swatch => Some(r.rect.center()),
                    _ => None,
                });
                output.drop_without_applying_deltas();
                (well, action)
            };
            frame(vec![]);
            let pos = frame(vec![]).0.expect("the palette shows the swatch");
            let (_, action) = frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            let Some(FloatingToolbarAction::ChangeColor(picked, color)) = action else {
                panic!("slot {slot}: {action:?}");
            };
            assert_eq!(picked, slot);
            assert_eq!(color, Color32::from_rgba_unmultiplied(r, g, b, 51));
        }
    }

    /// Keyboard navigation announces each action, including the current lock and visibility state
    /// and the alert when it is available.
    #[test]
    fn action_buttons_have_accessible_names() {
        for (locked, user_edit) in [(false, true), (true, true), (false, false)] {
            let ctx = egui::Context::default();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            toolbar.set_locked(locked);
            toolbar.set_user_edit_enabled(user_edit);
            toolbar.set_object_visible(!locked);
            toolbar.menu.alert = locked.then(|| {
                crate::model::Alert::new("SYNTH", crate::model::AlertCondition::Crossing(1.0))
            });
            let mut frame = |tab| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events: if tab {
                            vec![egui::Event::Key {
                                key: egui::Key::Tab,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            }]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        toolbar.show(ui);
                    },
                )
            };
            frame(false).drop_without_applying_deltas();
            frame(false).drop_without_applying_deltas();
            let alert = locked.then_some("Add alert");
            for label in ["Settings"]
                .into_iter()
                .chain(alert)
                .chain(user_edit.then_some(if locked { "Unlock" } else { "Lock" }))
                .chain((!locked && user_edit).then_some("Delete"))
                .chain(["More"])
            {
                let mut output = frame(true);
                let events = std::mem::take(&mut output.platform_output.events);
                output.drop_without_applying_deltas();
                assert!(
                    events
                        .iter()
                        .any(|event| { event.widget_info().label.as_deref() == Some(label) }),
                    "missing accessible name: {label}"
                );
            }
        }
    }

    /// The colour of a text annotation is its text's where it paints the text; a note's own
    /// white text, a pin's text and a table's cells do not take it. A label on a line does not
    /// make the line's colour a text colour, although the colour paints both. A sticker, a ghost
    /// feed and a pitchfork keep their own colours, and a post card's first slot is its fill.
    /// Width and style are offered apart, each where the tool draws it.
    #[test]
    fn colour_slot_names_what_the_colour_paints() {
        use crate::drawings::{Drawing, DrawingToolType as Tool};
        let red = Some([242, 54, 69, 255]);
        for (tool, text_color, label, offers) in [
            (Tool::TrendLine, None, Some("Line color"), (true, true)),
            (Tool::TextLabel, None, Some("Text color"), (false, false)),
            (Tool::TextLabel, red, Some("Text color"), (false, false)),
            (Tool::AnchoredText, red, Some("Text color"), (false, false)),
            (Tool::Note, None, Some("Line color"), (false, false)),
            (Tool::Callout, None, Some("Line color"), (true, true)),
            (Tool::AnchoredNote, None, Some("Color"), (false, false)),
            (Tool::FlagNote, None, Some("Color"), (false, false)),
            (Tool::FlagNote, red, Some("Color"), (false, false)),
            (Tool::Table, None, Some("Line color"), (false, false)),
            (Tool::ArrowMarkUp, None, Some("Line color"), (false, false)),
            (Tool::ArrowMarker, None, Some("Line color"), (false, false)),
            (Tool::Pitchfork, None, None, (true, false)),
            (Tool::Tweet, None, Some("Fill color"), (false, true)),
            (Tool::Sticker, None, None, (false, false)),
            (Tool::GhostFeed, None, None, (false, false)),
        ] {
            let mut chart = Chart::new(crate::model::BarData::new());
            let mut drawing = Drawing::new(7, tool);
            if text_color.is_some() {
                drawing.text_color = text_color;
            }
            drawing.completed = true;
            drawing.points = vec![Pos2::new(100.0, 100.0)];
            chart.drawings_mut().drawings = vec![drawing];
            chart.drawings_mut().select(7);
            let mut toolbar = FloatingSelectionToolbar::new();
            egui::Context::default()
                .run_ui(Default::default(), |ui| {
                    toolbar.show_and_apply(ui, &mut chart);
                })
                .drop_without_applying_deltas();
            assert_eq!(
                toolbar.color_slots.first().map(|slot| slot.label),
                label,
                "{tool:?} {text_color:?}"
            );
            assert_eq!(
                (toolbar.width.is_some(), toolbar.line_style.is_some()),
                offers,
                "{tool:?}"
            );
        }
    }

    /// The first colour slot edits what it names. A price label's border follows its colour;
    /// a post card's first slot fills the card, leaving its unused colour alone. Text with a
    /// colour of its own takes the pick there.
    #[test]
    fn first_colour_slot_edits_what_it_names() {
        use crate::drawings::{Drawing, DrawingToolType};
        use crate::ui_kit::color_picker::{MAIN_COLORS, tests::click};
        for (tool, user_edit) in [
            (DrawingToolType::PriceLabel, true),
            (DrawingToolType::PriceLabel, false),
            (DrawingToolType::Tweet, true),
            (DrawingToolType::TextLabel, true),
        ] {
            let swatch = MAIN_COLORS[3];
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut chart = Chart::new(crate::model::BarData::new());
            let mut label = Drawing::new(7, tool);
            // A border kept in the label's colour, as an older save holds it.
            if tool == DrawingToolType::PriceLabel {
                label.border_color = Some(label.color);
            }
            if tool == DrawingToolType::TextLabel {
                label.text_color = Some([242, 54, 69, 255]);
            }
            label.user_edit_enabled = user_edit;
            let original = label.color;
            label.completed = true;
            label.points = vec![Pos2::new(100.0, 100.0)];
            chart.drawings_mut().drawings = vec![label];
            chart.drawings_mut().select(7);
            let mut toolbar = FloatingSelectionToolbar::new();
            let mut frame = |toolbar: &mut FloatingSelectionToolbar, events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        toolbar.show_and_apply(ui, &mut chart);
                    },
                );
                let well = output.shapes.iter().find_map(|s| match &s.shape {
                    egui::Shape::Rect(r) if r.fill == swatch => Some(r.rect.center()),
                    _ => None,
                });
                output.drop_without_applying_deltas();
                well
            };
            frame(&mut toolbar, vec![]);
            toolbar.pickers[0].is_open = true;
            frame(&mut toolbar, vec![]);
            let pos = frame(&mut toolbar, vec![]);
            if user_edit {
                assert!(pos.is_some(), "the palette shows the swatch");
            }
            if let Some(pos) = pos {
                frame(&mut toolbar, click(pos));
            }
            let label = &chart.drawings().drawings[0];
            let green = if user_edit {
                swatch.to_srgba_unmultiplied()
            } else {
                original
            };
            if tool == DrawingToolType::Tweet {
                assert_eq!((label.color, label.fill_color), (original, Some(green)));
            } else if tool == DrawingToolType::TextLabel {
                assert_eq!(
                    (label.color, label.text_box_colors(&ctx).text),
                    ([green[0], green[1], green[2], original[3]], swatch)
                );
            } else {
                assert_eq!((label.color, label.border_color), (green, Some(green)));
            }
        }
    }

    /// Quick styles reach the editable selection together, keep each opacity and undo together.
    #[test]
    fn style_edits_reach_every_editable_selected_drawing() {
        use crate::drawings::Drawing;
        use crate::ui_kit::color_picker::{MAIN_COLORS, tests::click};
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut chart = Chart::new(crate::model::BarData::new());
        for id in 1..=3 {
            let mut line = Drawing::new(id, DrawingToolType::TrendLine);
            line.completed = true;
            line.points = vec![
                Pos2::new(100.0, 100.0 + id as f32 * 50.0),
                Pos2::new(300.0, 200.0 + id as f32 * 50.0),
            ];
            line.chart_points = vec![ChartPoint::new(1.0, 105.0), ChartPoint::new(2.0, 106.0)];
            line.color = if id == 1 {
                [242, 54, 69, 51]
            } else {
                [41, 98, 255, 255]
            };
            line.stroke_width = 2.0;
            line.line_style = LineStyle::Solid;
            line.locked = id == 3;
            chart.drawings_mut().drawings.push(line);
        }
        let styles = |chart: &Chart| {
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| (d.color, d.stroke_width, d.line_style))
                .collect::<Vec<_>>()
        };
        let mut toolbar = FloatingSelectionToolbar::new();
        let frame = |chart: &mut Chart, toolbar: &mut FloatingSelectionToolbar, events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    toolbar.show_and_apply(ui, chart);
                },
            )
        };
        let text_pos = |output: &egui::FullOutput, label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing control: {label}"))
        };
        for property in ["colour", "same colour", "width", "style"] {
            if property == "same colour" {
                chart.drawings_mut().drawings[0].color = [76, 175, 80, 51];
            }
            let original = styles(&chart);
            chart.drawings_mut().select_drawings([1, 2, 3]);
            for _ in 0..2 {
                frame(&mut chart, &mut toolbar, vec![]).drop_without_applying_deltas();
            }
            assert_eq!(toolbar.width, Some(Some(2.0)));
            if property.ends_with("colour") {
                toolbar.pickers[0].is_open = true;
                frame(&mut chart, &mut toolbar, vec![]).drop_without_applying_deltas();
                let output = frame(&mut chart, &mut toolbar, vec![]);
                let pos = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Rect(r) if r.fill == MAIN_COLORS[3] => Some(r.rect.center()),
                        _ => None,
                    })
                    .expect("the palette shows the swatch");
                output.drop_without_applying_deltas();
                frame(&mut chart, &mut toolbar, click(pos)).drop_without_applying_deltas();
                assert_eq!(chart.drawings().drawings[0].color, [76, 175, 80, 51]);
                assert_eq!(chart.drawings().drawings[1].color, [76, 175, 80, 255]);
                assert_eq!(chart.drawings().drawings[2].color, [41, 98, 255, 255]);
            } else {
                let (current, chosen) = if property == "width" {
                    ("2 px", "4 px")
                } else {
                    ("Solid", "Dashed")
                };
                let output = frame(&mut chart, &mut toolbar, vec![]);
                let pos = text_pos(&output, current);
                output.drop_without_applying_deltas();
                frame(&mut chart, &mut toolbar, click(pos)).drop_without_applying_deltas();
                let output = frame(&mut chart, &mut toolbar, vec![]);
                let pos = text_pos(&output, chosen);
                output.drop_without_applying_deltas();
                frame(&mut chart, &mut toolbar, click(pos)).drop_without_applying_deltas();
                let drawings = &chart.drawings().drawings;
                if property == "width" {
                    assert_eq!(
                        drawings.iter().map(|d| d.stroke_width).collect::<Vec<_>>(),
                        [4.0, 4.0, 2.0]
                    );
                } else {
                    assert_eq!(
                        drawings.iter().map(|d| d.line_style).collect::<Vec<_>>(),
                        [LineStyle::Dashed, LineStyle::Dashed, LineStyle::Solid]
                    );
                }
                frame(&mut chart, &mut toolbar, vec![]).drop_without_applying_deltas();
                if property == "width" {
                    assert_eq!(toolbar.width, Some(None));
                } else {
                    assert_eq!(toolbar.line_style, Some(None));
                }
            }
            assert!(chart.undo(), "{property}");
            assert_eq!(styles(&chart), original, "{property}");
        }
    }

    /// Alert availability comes from the drawing and chart, and honours the shared control.
    #[test]
    fn add_alert_follows_drawing_levels_and_the_alerts_control() {
        use crate::drawings::Drawing;
        use crate::model::{AlertComparison, AlertCondition, AlertSource};
        for (tool, symbol, alerts_visible, offered) in [
            (DrawingToolType::TrendLine, "SYNTH", true, true),
            (DrawingToolType::TrendLine, "SYNTH", false, false),
            (DrawingToolType::VerticalLine, "SYNTH", true, false),
            (DrawingToolType::TrendLine, "", true, false),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
            chart.symbol = symbol.into();
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.points = vec![Pos2::new(100.0, 100.0)];
            drawing.chart_points = vec![ChartPoint::new(10.0, 105.0)];
            if tool == DrawingToolType::TrendLine {
                drawing.chart_points.push(ChartPoint::new(50.0, 108.0));
            }
            chart.drawings_mut().drawings = vec![drawing];
            chart.drawings_mut().select(7);
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.set_control_visible(Control::Alerts, alerts_visible);
            let mut action = None;
            let mut frame = |events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        action = toolbar.show_and_apply(ui, &mut chart);
                    },
                )
            };
            frame(vec![]).drop_without_applying_deltas();
            let output = frame(vec![]);
            let pos = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    if node.label() != Some("Add alert")
                        || node.role() == egui::accesskit::Role::Label
                    {
                        return None;
                    }
                    let rect = node.bounds()?;
                    Some(egui::pos2(
                        (rect.x0 + rect.x1) as f32 / 2.0,
                        (rect.y0 + rect.y1) as f32 / 2.0,
                    ))
                });
            output.drop_without_applying_deltas();
            assert_eq!(
                pos.is_some(),
                offered,
                "{tool:?}, {symbol}, visible={alerts_visible}"
            );
            if let Some(pos) = pos {
                for pressed in [true, false] {
                    frame(vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ])
                    .drop_without_applying_deltas();
                }
                let Some(DrawingContextMenuAction::AddAlert(alert)) = action else {
                    panic!("missing alert: {action:?}");
                };
                assert_eq!(alert.symbol, "SYNTH");
                assert_eq!(alert.source, AlertSource::Price);
                assert_eq!(
                    alert.condition,
                    AlertCondition::Compare {
                        other: AlertSource::Drawing("7".into()),
                        comparison: AlertComparison::Crossing
                    }
                );
            }
        }
    }

    /// Floating surfaces and group rules follow each preset's popup and separator visuals.
    #[test]
    fn surface_and_separators_follow_the_popup_theme() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let style = ctx.global_style();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            ctx.run_ui(raw.clone(), |ui| {
                toolbar.show(ui);
            })
            .drop_without_applying_deltas();
            let mut output = ctx.run_ui(raw, |ui| {
                toolbar.show(ui);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let shapes: Vec<_> = shapes
                .into_iter()
                .flat_map(|shape| match shape.shape {
                    egui::Shape::Vec(shapes) => shapes,
                    shape => vec![shape],
                })
                .collect();
            assert!(shapes.iter().any(|shape| matches!(shape,
                egui::Shape::Rect(rect) if rect.fill == style.visuals.window_fill()
                    && rect.stroke == style.visuals.window_stroke
            )));
            let shadow = style.visuals.popup_shadow;
            if shadow != egui::epaint::Shadow::NONE {
                assert!(shapes.iter().any(|shape| matches!(shape,
                    egui::Shape::Rect(rect) if rect.fill == shadow.color
                        && rect.blur_width == f32::from(shadow.blur)
                )));
            }
            let separators = shapes.iter().filter(|shape| matches!(shape,
                egui::Shape::LineSegment { points, stroke }
                    if points[0].x == points[1].x
                        && (points[1].y - points[0].y) == DESIGN_TOKENS.sizing.floating_toolbar.separator_height
                        && *stroke == style.visuals.widgets.noninteractive.bg_stroke
            )).count();
            assert_eq!(separators, 3);
        }
    }

    /// Tiny viewports must not panic. Position actions wrap to stay reachable in a narrow window.
    #[test]
    fn show_survives_viewport_smaller_than_toolbar() {
        for (w, h, expected) in [
            (100.0, 100.0, Pos2::new(0.0, 10.0)),
            (800.0, 20.0, Pos2::new(10.0, 0.0)),
            (100.0, 20.0, Pos2::ZERO),
        ] {
            let ctx = egui::Context::default();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(10.0, 10.0));
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
                ..Default::default()
            };
            // egui asserts that a frame's texture delta is consumed before the next frame.
            ctx.run_ui(raw.clone(), |_| {}).textures_delta.clear();
            ctx.run_ui(raw, |ui| {
                toolbar.show(ui);
            })
            .textures_delta
            .clear();
            assert_eq!(toolbar.position, expected, "viewport {w}x{h}");
        }
        for (width, dark, mixed) in [
            (320.0, false, false),
            (320.0, false, true),
            (320.0, true, false),
            (320.0, true, true),
            (800.0, false, false),
            (800.0, false, true),
            (800.0, true, false),
            (800.0, true, true),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(
                &ctx,
                &if dark {
                    crate::theme::Theme::dark()
                } else {
                    crate::theme::Theme::light()
                },
            );
            let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0));
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            toolbar.menu.set_context(
                crate::drawings::DrawingToolType::LongPos.as_str(),
                false,
                true,
            );
            toolbar.width = Some((!mixed).then_some(2.0));
            toolbar.line_style = Some((!mixed).then_some(LineStyle::Solid));
            toolbar.set_colors(vec![ColorSlot {
                label: "Line color",
                color: Color32::BLUE,
            }]);
            for _ in 0..3 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(bounds),
                        ..Default::default()
                    },
                    |ui| {
                        toolbar.show(ui);
                    },
                )
                .drop_without_applying_deltas();
            }
            assert!(
                bounds.contains_rect(toolbar.last_rect),
                "{width}, dark {dark}, mixed {mixed}: {:?}",
                toolbar.last_rect
            );
            let mut chart = Chart::new(crate::model::BarData::new());
            chart
                .drawings_mut()
                .drawings
                .push(crate::drawings::Drawing::new(
                    7,
                    crate::drawings::DrawingToolType::LongPos,
                ));
            let mut dialog = crate::ui::dialogs::DrawingPropertiesDialog::new();
            dialog.open_for_chart(&chart, crate::ui::dialogs::DrawingId(7));
            toolbar.position = bounds.center() - toolbar.last_rect.size() * 0.5;
            for _ in 0..3 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(bounds),
                        ..Default::default()
                    },
                    |ui| {
                        toolbar.show(ui);
                        dialog.show(ui.ctx());
                    },
                )
                .drop_without_applying_deltas();
            }
            let overlap = bounds.center();
            assert!(toolbar.last_rect.contains(overlap));
            assert_ne!(
                ctx.layer_id_at(overlap).unwrap().id,
                egui::Id::unique("floating_selection_toolbar"),
                "the settings dialog must receive clicks where it covers the toolbar"
            );
        }
    }

    /// The toolbar leaves the readout card of a selected line readable at every placement,
    /// including readouts enabled after selection, and stays where it is when already clear.
    #[test]
    fn toolbar_stays_clear_of_the_readout_card() {
        use crate::drawings::{Drawing, DrawingToolType, LineMeasurementPosition as P};
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for (position, a, b, enabled_later, stays) in [
            (P::Right, (200.0, 300.0), (240.0, 310.0), false, false),
            (P::Right, (200.0, 300.0), (240.0, 310.0), true, false),
            // Beside the right edge the window pushes the toolbar left, over a card on the left.
            (P::Left, (640.0, 300.0), (680.0, 290.0), false, false),
            (P::Center, (200.0, 300.0), (240.0, 290.0), false, false),
            // No room above a drawing at the window's top edge.
            (P::Right, (200.0, 45.0), (240.0, 50.0), false, false),
            (P::Right, (100.0, 300.0), (500.0, 290.0), false, true),
        ] {
            let case = format!("{position:?} {a:?} {b:?} later {enabled_later}");
            let ctx = egui::Context::default();
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[200.0]);
            let mut toolbar = FloatingSelectionToolbar::new();
            let raw = egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            };
            let frame = |chart: &mut Chart, toolbar: &mut FloatingSelectionToolbar| {
                let mut output = ctx.run_ui(raw.clone(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                    toolbar.show_and_apply(ui, chart);
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                shapes
            };
            frame(&mut chart, &mut toolbar);
            let point = |chart: &Chart, (x, y): (f32, f32)| {
                let pane = chart.get_rendered_price_rect();
                crate::drawings::ChartPoint::new(
                    chart
                        .state
                        .time_scale()
                        .coord_to_idx_precise(x, pane.min.x, pane.width())
                        as f32,
                    chart.last_price_map.y_to_price(y),
                )
            };
            let mut drawing = Drawing::new(7, DrawingToolType::ExtendedLine);
            drawing.completed = true;
            drawing.chart_points = vec![point(&chart, a), point(&chart, b)];
            let readouts = crate::drawings::LineMeasurements {
                position,
                price_change: true,
                percent_change: true,
                bar_count: true,
                length: true,
                inclination: true,
                ..Default::default()
            };
            if !enabled_later {
                drawing.line_measurements = readouts;
            }
            chart.drawings_mut().drawings = vec![drawing];
            chart.drawings_mut().select(7);
            for _ in 0..3 {
                frame(&mut chart, &mut toolbar);
            }
            if enabled_later {
                chart.drawings_mut().drawings[0].line_measurements = readouts;
            }
            let shapes = (0..3)
                .map(|_| frame(&mut chart, &mut toolbar))
                .last()
                .unwrap();
            fn card_text(shape: &egui::Shape) -> Option<Rect> {
                match shape {
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(card_text),
                    egui::Shape::Text(text) if text.galley.text().contains("periods") => {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                }
            }
            let text = shapes
                .iter()
                .find_map(|shape| card_text(&shape.shape))
                .unwrap_or_else(|| panic!("{case}: no card"));
            assert!(
                !toolbar.last_rect.intersects(text),
                "{case}: toolbar {:?} covers card {text:?}",
                toolbar.last_rect,
            );
            // egui rounds the measured width, so an edge can land a fraction past the window.
            assert!(
                bounds.expand(1.0).contains_rect(toolbar.last_rect),
                "{case}"
            );
            let drawing = &chart.drawings().drawings[0];
            let anchor = drawing.points[0];
            if stays {
                assert_eq!(toolbar.position, anchor + Vec2::new(12.0, -52.0), "{case}");
            } else {
                let (from, to) = drawing.trend_line_ends(bounds).unwrap();
                assert!(
                    crate::painting::dashed_line::clip_segment(from, to, toolbar.last_rect)
                        .is_none(),
                    "{case}: toolbar {:?} covers the line",
                    toolbar.last_rect,
                );
            }
            // The rectangle the toolbar avoids is the one painted.
            let mut card = None;
            ctx.run_ui(raw.clone(), |ui| {
                card = chart.line_measurement_card_rect(ui, &chart.drawings().drawings[0]);
            })
            .drop_without_applying_deltas();
            assert!(card.is_some_and(|card| card.contains_rect(text)), "{case}");
        }
    }

    /// The toolbar leaves a selected price label's badge and a pin's popup readable at the
    /// placements the readout test covers, including a pin's text typed after placing it, and
    /// stays where it is when already clear.
    #[test]
    fn toolbar_stays_clear_of_price_labels_and_pin_popups() {
        use crate::drawings::{Drawing, DrawingToolType as T};
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let magenta = Color32::from_rgba_unmultiplied(255, 0, 255, 255);
        let moved = Pos2::new(400.0, 450.0);
        for (tool, anchor, before, after, dragged) in [
            (T::PriceLabel, (200.0, 300.0), "", "", false),
            // Beside the right edge the window pushes the toolbar left, over the label.
            (T::PriceLabel, (680.0, 300.0), "", "", false),
            // No room above a label at the window's top edge.
            (T::PriceLabel, (200.0, 45.0), "", "", false),
            (T::AnchoredNote, (200.0, 300.0), "Pin text", "", false),
            (T::AnchoredNote, (680.0, 300.0), "Pin text", "", false),
            (T::AnchoredNote, (200.0, 50.0), "Pin text", "", false),
            // The popup appears once the text is typed.
            (T::AnchoredNote, (200.0, 300.0), "", "Pin text", false),
            (
                T::AnchoredNote,
                (200.0, 300.0),
                "Pin",
                "Pin text, longer",
                true,
            ),
        ] {
            let case = format!("{tool:?} {anchor:?} {before:?} {after:?}");
            let ctx = egui::Context::default();
            let mut chart = crate::widget::test_frames::chart(&[100.0], &[200.0]);
            let mut toolbar = FloatingSelectionToolbar::new();
            let raw = egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            };
            let frame = |chart: &mut Chart, toolbar: &mut FloatingSelectionToolbar| {
                let mut output = ctx.run_ui(raw.clone(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                    toolbar.show_and_apply(ui, chart);
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                shapes
            };
            frame(&mut chart, &mut toolbar);
            let point = |chart: &Chart, (x, y): (f32, f32)| {
                let pane = chart.get_rendered_price_rect();
                crate::drawings::ChartPoint::new(
                    chart
                        .state
                        .time_scale()
                        .coord_to_idx_precise(x, pane.min.x, pane.width())
                        as f32,
                    chart.last_price_map.y_to_price(y),
                )
            };
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![point(&chart, anchor)];
            drawing.text = Some(before.to_owned()).filter(|text| !text.is_empty());
            // The badge's price is the only magenta text.
            drawing.text_color = (tool == T::PriceLabel).then_some([255, 0, 255, 255]);
            chart.drawings_mut().drawings = vec![drawing];
            chart.drawings_mut().select(7);
            for _ in 0..3 {
                frame(&mut chart, &mut toolbar);
            }
            if dragged {
                toolbar.position = moved;
            }
            if !after.is_empty() {
                chart.drawings_mut().drawings[0].text = Some(after.to_owned());
            }
            let shapes = (0..3)
                .map(|_| frame(&mut chart, &mut toolbar))
                .last()
                .unwrap();
            let shown = if after.is_empty() { before } else { after };
            fn label(
                shape: &egui::Shape,
                find: &dyn Fn(&egui::epaint::TextShape) -> bool,
            ) -> Option<egui::epaint::TextShape> {
                match shape {
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| label(s, find)),
                    egui::Shape::Text(text) if find(text) => Some(text.clone()),
                    _ => None,
                }
            }
            let find = |text: &egui::epaint::TextShape| {
                if tool == T::PriceLabel {
                    text.fallback_color == magenta
                } else {
                    text.galley.text() == shown
                }
            };
            let (text, clip) = shapes
                .iter()
                .find_map(|shape| label(&shape.shape, &find).map(|text| (text, shape.clip_rect)))
                .unwrap_or_else(|| panic!("{case}: no label"));
            // The badge and the popup pad their text by the annotation label padding.
            let padding = Vec2::new(
                DESIGN_TOKENS.sizing.annotation.label_padding_x,
                DESIGN_TOKENS.sizing.annotation.label_padding_y,
            );
            let boxed = Rect::from_min_size(text.pos - padding, text.galley.size() + padding * 2.0)
                .intersect(clip);
            assert!(boxed.is_positive(), "{case}: the label is on screen");
            assert!(
                !toolbar.last_rect.intersects(boxed),
                "{case}: toolbar {:?} covers label {boxed:?}",
                toolbar.last_rect,
            );
            // egui rounds the measured width, so an edge can land a fraction past the window.
            assert!(
                bounds.expand(1.0).contains_rect(toolbar.last_rect),
                "{case}"
            );
            if dragged {
                assert_eq!(toolbar.position, moved, "{case}");
            }
        }
    }
}
