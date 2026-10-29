//! Result rows and shared painting of the symbol choosers.

use super::{Row, SymbolSearchDialog};
use crate::localization as l10n;
use crate::model::Instrument;
use crate::theme::ThemeExt;
use egui::{Color32, Context, Pos2, Response, RichText, Sense, Ui, Vec2};

impl SymbolSearchDialog {
    /// Paints one result row. Also returns whether its member count, shown on the first
    /// instrument of a root, was clicked to show or hide the others.
    pub(super) fn render_symbol_item(
        &self,
        ui: &mut Ui,
        symbol: &Instrument,
        is_sel: bool,
        row: Row,
    ) -> (Response, bool) {
        let theme = ui.active_theme();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, 4.0);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::ListBoxOption,
                response.enabled(),
                &symbol.symbol,
            )
        });
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(is_sel));
        if is_sel || response.hovered() {
            ui.painter().rect_filled(rect, 4.0, theme.ui.btn_bg_hover);
        }
        let mut right = rect.right() - 8.0;
        // The exchange mark column exists once any row has one, so the other columns line up.
        if self.config.show_exchange
            && self
                .filtered_symbols
                .iter()
                .any(|symbol| symbol.exchange_logo.is_some())
        {
            let mark = egui::Rect::from_min_max(
                Pos2::new(right - 20.0, rect.center().y - 10.0),
                Pos2::new(right, rect.center().y + 10.0),
            );
            paint_image(ui, symbol.exchange_logo.as_deref(), mark);
            right = mark.left() - 12.0;
        }
        let kind = symbol
            .symbol_type
            .map(|kind| kind.to_string())
            .unwrap_or_default();
        let exchange = if self.config.show_exchange {
            symbol.exchange.as_deref().unwrap_or_default()
        } else {
            ""
        };
        let column_width = |kind_column: bool| {
            self.filtered_symbols
                .iter()
                .map(|symbol| {
                    let text = if kind_column {
                        symbol
                            .symbol_type
                            .map(|kind| kind.to_string())
                            .unwrap_or_default()
                    } else if self.config.show_exchange {
                        symbol.exchange.clone().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    ui.painter()
                        .layout_no_wrap(
                            l10n::text(ui.ctx(), text),
                            egui::FontId::proportional(11.0),
                            theme.ui.text_secondary,
                        )
                        .size()
                        .x
                })
                .fold(0.0_f32, f32::max)
                .min(rect.width() * 0.2)
        };
        let kind_width = column_width(true);
        let exchange_width = column_width(false);
        let exchange_right = right - kind_width - 16.0;
        for (text, edge, width) in [
            (kind.as_str(), right, kind_width),
            (exchange, exchange_right, exchange_width),
        ] {
            let galley = egui::WidgetText::from(
                RichText::new(l10n::text(ui.ctx(), text))
                    .size(11.0)
                    .color(theme.ui.text_secondary),
            )
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                width,
                egui::TextStyle::Body,
            );
            ui.painter().galley(
                Pos2::new(
                    edge - galley.size().x,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                theme.ui.text_secondary,
            );
        }
        let indent = if row.nested { 24.0 } else { 0.0 };
        let mark = egui::Rect::from_center_size(
            Pos2::new(rect.left() + 20.0 + indent, rect.center().y),
            Vec2::splat(24.0),
        );
        paint_symbol_mark(ui, symbol, mark);
        let left = mark.right() + 12.0;
        let mut text_right = exchange_right - exchange_width - 16.0;
        let mut toggled = false;
        if row.members > 0 {
            let expanded = symbol
                .root
                .as_ref()
                .is_some_and(|root| self.expanded.contains(root));
            let galley = ui.painter().layout_no_wrap(
                format!("+{}", row.members),
                egui::FontId::proportional(11.0),
                theme.ui.text,
            );
            let pill = egui::Rect::from_min_max(
                Pos2::new(text_right - galley.size().x - 16.0, rect.center().y - 10.0),
                Pos2::new(text_right, rect.center().y + 10.0),
            );
            text_right = pill.left() - 8.0;
            let toggle = ui.interact(pill, response.id.with("members"), Sense::click());
            let label = l10n::format_text(
                ui.ctx(),
                "{0} more instruments of {1}",
                &[
                    ("0", row.members.to_string()),
                    ("1", symbol.root.clone().unwrap_or_default()),
                ],
            );
            toggle.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, true, expanded, &label)
            });
            let fill = if toggle.hovered() || expanded {
                theme.ui.btn_bg_hover
            } else {
                theme.ui.panel_bg
            };
            ui.painter().rect(
                pill,
                pill.height() / 2.0,
                fill,
                ui.visuals().widgets.inactive.bg_stroke,
                egui::StrokeKind::Inside,
            );
            crate::styles::focus::draw_focus_ring(ui, &toggle, pill.height() / 2.0);
            ui.painter()
                .galley(pill.center() - galley.size() / 2.0, galley, theme.ui.text);
            toggled = toggle.clicked();
        }
        let width = (text_right - left).max(0.0);
        let name = egui::WidgetText::from(
            RichText::new(&symbol.symbol)
                .size(13.0)
                .strong()
                .color(theme.ui.text),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            width,
            egui::TextStyle::Body,
        );
        let top = if self.config.show_description {
            rect.top() + 1.0
        } else {
            rect.center().y - name.size().y / 2.0
        };
        ui.painter()
            .galley(Pos2::new(left, top), name, theme.ui.text);
        if self.config.show_description {
            let description = egui::WidgetText::from(
                RichText::new(symbol.description.as_deref().unwrap_or(&symbol.symbol))
                    .size(11.0)
                    .color(theme.ui.text_secondary),
            )
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                width,
                egui::TextStyle::Body,
            );
            ui.painter().galley(
                Pos2::new(left, rect.bottom() - description.size().y - 1.0),
                description,
                theme.ui.text_secondary,
            );
        }

        (response, toggled)
    }
}

/// Corner radius of the symbol search and comparison dialogs.
pub(super) const SEARCH_RADIUS: f32 = 20.0;

/// The backdrop of the search dialogs: a faint veil, #F1F1F1 over white, on light themes, and the
/// theme's modal overlay on dark ones.
pub(super) fn search_backdrop(ctx: &Context) -> Color32 {
    let theme = ctx.active_theme();
    if theme.dark {
        theme.ui.modal_overlay
    } else {
        Color32::from_black_alpha(14)
    }
}

/// Paints the image at `uri` into `rect` as a circle. Returns `false`, painting nothing, without
/// a URI or while the image is loading or unreadable.
fn paint_image(ui: &Ui, uri: Option<&str>, rect: egui::Rect) -> bool {
    let Some(uri) = uri else {
        return false;
    };
    let pixels = (rect.size() * ui.ctx().pixels_per_point()).round();
    let size = egui::SizeHint::Size {
        width: pixels.x as u32,
        height: pixels.y as u32,
        maintain_aspect_ratio: true,
    };
    match ui
        .ctx()
        .try_load_texture(uri, egui::TextureOptions::LINEAR, size)
    {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            egui::Image::from_texture(texture)
                .corner_radius(rect.width() / 2.0)
                .paint_at(ui, rect);
            true
        }
        _ => false,
    }
}

/// Paints the instrument's logo in `rect`, or its initial in a neutral circle without one.
pub(super) fn paint_symbol_mark(ui: &Ui, symbol: &Instrument, rect: egui::Rect) {
    if paint_image(ui, symbol.logo.as_deref(), rect) {
        return;
    }
    let theme = ui.active_theme();
    ui.painter()
        .circle_filled(rect.center(), rect.width() / 2.0, theme.ui.btn_bg_hover);
    let initial: String = symbol
        .symbol
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .collect();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(rect.height() * 0.5),
        theme.ui.text_secondary,
    );
}
