//! Shared controls and cells for the host-fed market panels.

use crate::localization as l10n;
use egui::{Align, Color32, Label, Layout, Rect, Response, RichText, Sense, Ui, UiBuilder, vec2};

use super::ROW_HEIGHT;
use crate::icons::icons;
use crate::theme::ThemeExt;

pub(super) fn dropdown(
    ui: &mut Ui,
    id: &str,
    text: &str,
    contents: impl FnOnce(&mut Ui),
) -> Response {
    ui.push_id(id, |ui| {
        let button = ui.add(
            egui::Button::new((
                l10n::text(ui.ctx(), text),
                icons::CHEVRON_DOWN.as_image_square(16.0),
            ))
            .frame_when_inactive(false)
            .image_tint_follows_text_color(true)
            .min_size(vec2(0.0, 28.0))
            .truncate(),
        );
        egui::Popup::menu(&button).show(|ui| {
            egui::ScrollArea::vertical()
                .max_height(ui.spacing().combo_height)
                .show(ui, contents);
        });
        button
    })
    .inner
}

pub(super) fn row(ui: &mut Ui) -> (Rect, Response) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, ui.active_theme().ui.btn_bg_hover);
    }
    (rect, response)
}

/// Clip each cell independently so long host text cannot cover a neighbouring value.
pub(super) fn cell(ui: &mut Ui, rect: Rect, text: &str, right: bool, color: Color32) -> Response {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(if right {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    }));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.add(
        Label::new(RichText::new(l10n::text(ui.ctx(), text)).color(color))
            .truncate()
            .selectable(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, apply_to_egui};

    /// Host lists can exceed the window height; their last choice must remain reachable.
    #[test]
    fn dropdowns_scroll_to_the_last_choice_in_short_windows() {
        for theme in [Theme::light(), Theme::dark()] {
            for height in [180.0, 360.0] {
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &theme);
                ctx.global_style_mut(|style| {
                    style.scroll_animation = egui::style::ScrollAnimation::none()
                });
                let mut last = Rect::NOTHING;
                let mut clip = Rect::NOTHING;
                let mut frame = |events| {
                    ctx.run_ui(
                        egui::RawInput {
                            events,
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                vec2(280.0, height),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            dropdown(ui, "choices", "Choose", |ui| {
                                for index in 0..80 {
                                    let response =
                                        ui.selectable_label(false, format!("Choice {index}"));
                                    if index == 79 {
                                        response.scroll_to_me(Some(Align::BOTTOM));
                                        last = response.rect;
                                        clip = ui.clip_rect();
                                    }
                                }
                            });
                        },
                    )
                    .drop_without_applying_deltas();
                };
                frame(vec![]);
                let pos = egui::pos2(20.0, 14.0);
                frame(vec![egui::Event::PointerMoved(pos)]);
                for pressed in [true, false] {
                    frame(vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }]);
                }
                for _ in 0..3 {
                    frame(vec![]);
                }
                assert!(last.is_positive(), "the menu must open");
                assert!(
                    clip.contains_rect(last),
                    "last choice is clipped: {last:?}, {clip:?}"
                );
                assert!(
                    last.bottom() <= height,
                    "last choice is outside the window: {last:?}"
                );
            }
        }
    }
}
