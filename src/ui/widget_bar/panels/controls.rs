//! Shared controls and cells for the host-fed market panels.

use egui::{Align, Color32, Label, Layout, Rect, Response, RichText, Sense, Ui, UiBuilder, vec2};

use super::watchlist::ROW_HEIGHT;
use crate::icons::icons;
use crate::theme::ThemeExt;

pub(super) fn dropdown(ui: &mut Ui, id: &str, text: &str, contents: impl FnOnce(&mut Ui)) {
    ui.push_id(id, |ui| {
        let button = ui.add(
            egui::Button::new((text, icons::CHEVRON_DOWN.as_image_square(16.0)))
                .frame_when_inactive(false)
                .image_tint_follows_text_color(true)
                .min_size(vec2(0.0, 28.0))
                .truncate(),
        );
        egui::Popup::menu(&button).show(contents);
    });
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
        Label::new(RichText::new(text).color(color))
            .truncate()
            .selectable(false),
    )
}
