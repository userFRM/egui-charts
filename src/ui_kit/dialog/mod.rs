//! Dialog chrome shared by every dialog in the crate.
//!
//! - `DialogFrame`: centred, non-modal window in the dialog frame
//! - `dialog_header`: title row with a close button
//! - `DialogFooter`: a rule above right-aligned action buttons, with optional left content
//!
//! The kit `Modal`, `ConfirmDialog` and `PromptDialog` use the same frame, header and footer
//! over a scrim.

use crate::localization as l10n;
use crate::theme::ThemeExt;
use egui::{
    Align, Align2, Color32, Context, Frame, Layout, Response, RichText, Stroke, Ui, UiBuilder,
    UiStackInfo, Vec2,
};

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};

/// Minimum width of a short footer action such as Ok.
const FOOTER_BUTTON_MIN_WIDTH: f32 = 44.0;

/// Marks the content Ui of a fixed-height [`DialogFrame`].
const FIXED_HEIGHT_TAG: &str = "egui_charts::dialog_frame::fixed_height";

/// The dialog surface, with room for its title and right-aligned footer actions.
pub(crate) fn dialog_frame(ctx: &Context) -> Frame {
    let theme = ctx.active_theme();
    let mut frame = Frame::window(&ctx.global_style()).fill(ctx.global_style().visuals.panel_fill);
    if !theme.dark {
        frame.stroke = Stroke::NONE;
        frame.shadow.blur = (2.0 * theme.spacing.sm) as u8;
        frame.shadow.offset = [0, theme.spacing.xs as i8];
    }
    let stroke_width = frame.stroke.width;
    frame.inner_margin(egui::Margin {
        left: (theme.spacing.xxl + theme.spacing.sm - stroke_width) as i8,
        right: (theme.spacing.xxl + theme.spacing.sm - DESIGN_TOKENS.stroke.hairline - stroke_width)
            as i8,
        top: (theme.spacing.xxl - stroke_width) as i8,
        bottom: (theme.spacing.xxl - stroke_width) as i8,
    })
}

/// A centered dialog window with standard styling.
///
/// The requested size is the preferred size of the content inside the frame padding: the content
/// area is at least that large, grows with its content, and is never sized beyond the application
/// window.
///
/// # Example
/// ```
/// use egui::Vec2;
/// use egui_charts::ui_kit::dialog::{DialogFrame, dialog_header};
///
/// # egui::__run_test_ctx(|ctx| {
/// # let mut open = true;
/// DialogFrame::new("Series Settings", Vec2::new(600.0, 500.0))
///     .show(ctx, |ui| {
///         if dialog_header(ui, "Series Settings") {
///             open = false;
///         }
///         // content...
///     });
/// # });
/// ```
pub struct DialogFrame {
    id: String,
    size: Vec2,
    auto_height: bool,
    rounding: Option<f32>,
}

impl DialogFrame {
    /// Create a centered dialog with a preferred size.
    pub fn new(id: impl Into<String>, size: Vec2) -> Self {
        Self {
            id: id.into(),
            size,
            auto_height: false,
            rounding: None,
        }
    }

    /// Create a centered dialog with a preferred width and auto-height.
    pub fn with_width(id: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            size: Vec2::new(width, 0.0),
            auto_height: true,
            rounding: None,
        }
    }

    /// Override the corner radius (default: the theme's window radius)
    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = Some(r);
        self
    }

    /// Show the dialog. The closure receives `&mut Ui` for the dialog content.
    pub fn show(self, ctx: &Context, add_contents: impl FnOnce(&mut Ui)) {
        let mut frame = dialog_frame(ctx);
        if let Some(r) = self.rounding {
            frame = frame.corner_radius(r);
        }
        let max = ctx
            .content_rect()
            .shrink(ctx.active_theme().spacing.xxxl)
            .size()
            - frame.total_margin().sum();
        let inner = self.size.min(max).max(Vec2::ZERO);

        let window = egui::Window::new(&self.id)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .max_width((max.x + frame.total_margin().sum().x).max(0.0))
            .frame(frame);
        let window = if self.auto_height {
            window
                .default_width(inner.x)
                .default_height((max.y + frame.total_margin().sum().y).max(0.0))
                .max_height((max.y + frame.total_margin().sum().y).max(0.0))
        } else {
            window.fixed_size(inner)
        };

        let input_blocked = ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);
        let auto_height = self.auto_height;
        window.show(ctx, |ui| {
            let translated_label = l10n::text(ui.ctx(), &self.id);
            ui.response().widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Dialog, ui.is_enabled(), &translated_label)
            });
            ui.data_mut(|d| {
                d.insert_temp(ui.layer_id().id.with("dialog_input_blocked"), input_blocked)
            });
            // A non-resizable window shrinks to its content, so hold the preferred size here.
            ui.set_min_size(inner);
            ui.set_max_width(inner.x);
            if auto_height {
                ui.set_max_height(max.y.max(0.0));
                add_contents(ui);
            } else {
                // The tag lets a `DialogFooter` drawn directly in this Ui sit at the bottom.
                let info = UiStackInfo::default().with_tag(FIXED_HEIGHT_TAG);
                ui.scope_builder(UiBuilder::new().ui_stack_info(info), add_contents);
            }
            let focused = ui.ctx().memory(|m| m.focused()).is_some();
            ui.data_mut(|d| d.insert_temp(ui.layer_id().id.with("dialog_had_focus"), focused));
        });
    }
}

/// Standard dialog title row: left-aligned title and a close button centred on it.
///
/// Returns `true` on a close click or an unhandled Escape key.
pub fn dialog_header(ui: &mut Ui, title: &str) -> bool {
    let close = title_row(ui, title, true);
    let had_focus = ui
        .data(|d| d.get_temp::<bool>(ui.layer_id().id.with("dialog_had_focus")))
        .unwrap_or(false);
    close || (!had_focus && dialog_key(ui, egui::Key::Escape))
}

fn dialog_key(ui: &Ui, key: egui::Key) -> bool {
    if ui
        .data(|d| d.get_temp::<bool>(ui.layer_id().id.with("dialog_input_blocked")))
        .unwrap_or(false)
        || egui::Popup::is_any_open(ui.ctx())
        || ui.ctx().any_popup_open()
        || ui.ctx().top_layer_id() != Some(ui.layer_id())
        || ui.ctx().memory(|m| m.focused()).is_some()
    {
        return false;
    }
    ui.input_mut(|input| {
        let fresh = input.events.iter().any(|event| {
            matches!(event,
                egui::Event::Key { key: pressed, pressed: true, repeat: false, modifiers, .. }
                if *pressed == key && modifiers.is_none()
            )
        });
        fresh && input.consume_key(egui::Modifiers::NONE, key)
    })
}

/// Title row shared with the modal dialogs, which hide the close button when not closable.
pub(crate) fn title_row(ui: &mut Ui, title: &str, closable: bool) -> bool {
    // Fix the row height before placing the title so the title centres on the button.
    let row = Vec2::new(ui.available_width(), ButtonSize::MD.pixels());
    // The close button first, so a long title is elided in the width left rather than widening
    // the dialog past a narrow window.
    let close = ui
        .allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
            let close = closable
                && IconButton::new(&icons::CLOSE)
                    .icon_scale(1.0)
                    .tooltip("Close")
                    .show(ui)
                    .clicked();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(l10n::text(ui.ctx(), title))
                            .strong()
                            .size(ui.active_theme().text.dialog_title)
                            .variation("wght", 600.0),
                    )
                    .truncate(),
                );
            });
            close
        })
        .inner;
    ui.add_space(ui.active_theme().spacing.sm);
    close
}

/// Standard dialog footer with right-aligned action buttons.
///
/// Renders a rule, then a row with optional left content and, on the right, the secondary
/// (outlined) button followed by the primary (filled) button. Shown directly in a
/// [`DialogFrame`] content Ui, the footer sits at the bottom of the frame.
///
/// # Example
/// ```
/// # use egui_charts::ui_kit::dialog::DialogFooter;
/// # egui::__run_test_ui(|ui| {
/// let (ok, cancel) = DialogFooter::new("Ok")
///     .secondary("Cancel")
///     .show(ui);
/// if ok { /* apply */ }
/// if cancel { /* close */ }
/// # });
/// ```
pub struct DialogFooter {
    primary_label: String,
    secondary_label: Option<String>,
    primary_enabled: bool,
    danger: bool,
}

impl DialogFooter {
    /// Create a footer with a primary action label.
    pub fn new(primary_label: impl Into<String>) -> Self {
        Self {
            primary_label: primary_label.into(),
            secondary_label: None,
            primary_enabled: true,
            danger: false,
        }
    }

    /// Add a secondary (cancel) button.
    pub fn secondary(mut self, label: impl Into<String>) -> Self {
        self.secondary_label = Some(label.into());
        self
    }

    /// Enable/disable the primary button.
    pub fn primary_enabled(mut self, enabled: bool) -> Self {
        self.primary_enabled = enabled;
        self
    }

    /// Mark the primary action as destructive: it is filled with the danger colour.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Show the footer. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show(self, ui: &mut Ui) -> (bool, bool) {
        self.show_with_left(ui, |_| ())
    }

    /// Show the footer with left-side content. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show_with_left<R>(
        self,
        ui: &mut Ui,
        left_content: impl FnOnce(&mut Ui) -> R,
    ) -> (bool, bool) {
        // In a fixed-height `DialogFrame`, the free space goes between the content and the rule.
        // The footer height is measured on the previous frame; the window's sizing pass measures
        // it unpinned, so the first visible frame already has the window at its final size.
        let pinned = ui.stack().tags().contains(FIXED_HEIGHT_TAG);
        let height_id = ui.scope_id().with("dialog_footer_height");
        if pinned && !ui.is_sizing_pass() {
            let height = ui.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(0.0);
            ui.add_space((ui.available_height() - height).max(0.0));
        }
        let top = ui.cursor().top();

        ui.add_space(ui.active_theme().spacing.lg);
        ui.separator();
        ui.add_space(ui.active_theme().spacing.lg);

        let row = ui.horizontal(|ui| {
            left_content(ui);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let theme = ui.active_theme();
                let secondary_min_width = DESIGN_TOKENS.sizing.settings_dialog.button_min_width;
                ui.spacing_mut().item_spacing.x = theme.spacing.xl;
                ui.spacing_mut().button_padding.x = theme.spacing.xl + theme.spacing.xs;
                let font = egui::TextStyle::Button.resolve(ui.style());
                let width = std::iter::once((&self.primary_label, FOOTER_BUTTON_MIN_WIDTH))
                    .chain(
                        self.secondary_label
                            .iter()
                            .map(|label| (label, secondary_min_width)),
                    )
                    .map(|(label, min_width)| {
                        (ui.painter()
                            .layout_no_wrap(
                                l10n::text(ui.ctx(), label.clone()),
                                font.clone(),
                                Color32::PLACEHOLDER,
                            )
                            .size()
                            .x
                            + 2.0 * ui.spacing().button_padding.x)
                            .max(min_width)
                    })
                    .sum::<f32>()
                    + if self.secondary_label.is_some() {
                        theme.spacing.xl
                    } else {
                        0.0
                    };
                ui.allocate_ui_with_layout(
                    Vec2::new(width, DESIGN_TOKENS.sizing.settings_dialog.button_height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let colors = &theme.ui;
                        let outline = ui.visuals().widgets.inactive.fg_stroke.color;
                        let secondary_clicked =
                            self.secondary_label.as_ref().is_some_and(|label| {
                                let hover = ui.visuals().widgets.hovered.bg_fill;
                                let fill = [Color32::TRANSPARENT, hover, hover];
                                let stroke = Stroke::new(DESIGN_TOKENS.stroke.hairline, outline);
                                footer_button(ui, label, secondary_min_width, fill, outline, stroke)
                                    .clicked()
                            });
                        let (fill, text) = if self.danger {
                            let fill = [
                                colors.danger_bg,
                                colors.danger_bg_hover,
                                colors.danger_bg_active,
                            ];
                            (fill, colors.danger_text)
                        } else {
                            // The text colour rather than the accent, so the confirming action
                            // has the strongest contrast in the dialog in every theme.
                            let fill = [colors.text, colors.text_secondary, colors.text_muted];
                            (fill, colors.panel_bg)
                        };
                        let primary_clicked = ui
                            .add_enabled_ui(self.primary_enabled, |ui| {
                                footer_button(
                                    ui,
                                    &self.primary_label,
                                    FOOTER_BUTTON_MIN_WIDTH,
                                    fill,
                                    text,
                                    Stroke::NONE,
                                )
                            })
                            .inner
                            .clicked();

                        (primary_clicked, secondary_clicked)
                    },
                )
                .inner
            })
            .inner
        });

        if pinned {
            let height = row.response.rect.bottom() - top;
            ui.data_mut(|d| d.insert_temp(height_id, height));
        }
        let (primary, secondary) = row.inner;
        (
            primary || (self.primary_enabled && dialog_key(ui, egui::Key::Enter)),
            secondary,
        )
    }
}

/// A footer action button: `fill` at rest, hovered and pressed, with `text` and `stroke`.
fn footer_button(
    ui: &mut Ui,
    label: &str,
    min_width: f32,
    fill: [Color32; 3],
    text: Color32,
    stroke: Stroke,
) -> Response {
    let radius = ui.active_theme().radius.md;
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        for (visuals, fill) in [
            (&mut widgets.inactive, fill[0]),
            (&mut widgets.hovered, fill[1]),
            (&mut widgets.active, fill[2]),
        ] {
            visuals.weak_bg_fill = fill;
            visuals.bg_stroke = stroke;
            visuals.fg_stroke.color = text;
        }
        let response = ui.add(
            egui::Button::new(l10n::text(ui.ctx(), label))
                .corner_radius(radius)
                .min_size(Vec2::new(
                    min_width,
                    DESIGN_TOKENS.sizing.settings_dialog.button_height,
                )),
        );
        crate::styles::focus::draw_focus_ring(ui, &response, radius);
        response
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_keyboard_dismisses_and_activates_only_an_enabled_primary() {
        for (key, enabled, expected) in [
            (egui::Key::Escape, true, (true, false)),
            (egui::Key::Enter, true, (false, true)),
            (egui::Key::Enter, false, (false, false)),
        ] {
            let ctx = Context::default();
            let frame = |events| {
                let mut result = (false, false);
                ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        DialogFrame::new("keys", Vec2::new(420.0, 240.0)).show(ui.ctx(), |ui| {
                            result.0 = dialog_header(ui, "Keys");
                            result.1 = DialogFooter::new("Apply")
                                .primary_enabled(enabled)
                                .show(ui)
                                .0;
                        });
                    },
                )
                .drop_without_applying_deltas();
                result
            };
            frame(vec![]);
            frame(vec![]);
            assert_eq!(
                frame(vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]),
                expected
            );
        }
    }

    #[test]
    fn footer_tab_order_follows_the_buttons_from_left_to_right() {
        let ctx = Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::light());
        let frame = |events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    DialogFooter::new("Apply").secondary("Cancel").show(ui);
                },
            )
            .drop_without_applying_deltas();
            ctx.memory(|m| m.focused())
                .and_then(|id| ctx.read_response(id))
                .map(|r| r.rect)
        };
        frame(vec![]);
        let tab = || egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let first = frame(vec![tab()]).unwrap();
        let second = frame(vec![tab()]).unwrap();
        assert_eq!(first.size(), Vec2::new(72.0, 34.0));
        assert_eq!(second.height(), 34.0);
        assert_eq!(second.left() - first.right(), 12.0);
    }

    #[test]
    fn footer_sits_at_the_bottom_of_a_fixed_frame_from_the_first_visible_frame() {
        let ctx = Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let frame = || {
            let mut row = egui::Rect::NOTHING;
            let mut window = egui::Id::NULL;
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                DialogFrame::new("d", Vec2::new(420.0, 240.0)).show(ui.ctx(), |ui| {
                    window = ui.layer_id().id;
                    ui.label("Body");
                    DialogFooter::new("Apply")
                        .secondary("Cancel")
                        .show_with_left(ui, |ui| row = ui.max_rect());
                });
            })
            .drop_without_applying_deltas();
            (ctx.memory(|m| m.area_rect(window)).unwrap(), row)
        };
        frame(); // sizing pass
        let first = frame();
        let second = frame();
        assert_eq!(
            first.0, second.0,
            "the window moved after its first visible frame"
        );
        let inner_bottom = second.0.bottom() - ctx.active_theme().spacing.xxl;
        assert!(
            (second.1.top() + DESIGN_TOKENS.sizing.settings_dialog.button_height - inner_bottom)
                .abs()
                <= 1.5
        );
    }

    #[test]
    fn auto_height_scroll_body_reaches_the_screen_cap() {
        let ctx = Context::default();
        for (width, height) in [(1440.0, 900.0), (320.0, 450.0), (1440.0, 900.0)] {
            let mut window = egui::Id::NULL;
            for _ in 0..3 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(width, height),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        DialogFrame::with_width("scroll", 500.0).show(ui.ctx(), |ui| {
                            window = ui.layer_id().id;
                            dialog_header(ui, "Scrolling body");
                            egui::ScrollArea::vertical()
                                .max_height(ui.available_height())
                                .show(ui, |ui| {
                                    for i in 0..100 {
                                        ui.label(format!("Row {i}"));
                                    }
                                });
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
            let rect = ctx.memory(|m| m.area_rect(window)).unwrap();
            assert!((rect.height() - (height - 48.0)).abs() <= 2.0, "{rect:?}");
            assert!(rect.top() >= 23.0 && rect.bottom() <= height - 23.0);
            assert!(
                rect.left() >= 23.0 && rect.right() <= width - 23.0,
                "{rect:?}"
            );
        }
    }
}
