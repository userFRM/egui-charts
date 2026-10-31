//! Result rows and shared painting of the symbol choosers.

use super::{Row, SymbolSearchDialog};
use crate::localization as l10n;
use crate::model::Instrument;
use crate::theme::ThemeExt;
use egui::{Color32, Context, Pos2, Response, Sense, Ui, Vec2};

/// Height shared by search and comparison results.
pub(super) const SYMBOL_ROW_HEIGHT: f32 = 56.0;

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
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), SYMBOL_ROW_HEIGHT),
            Sense::click(),
        );
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
                Pos2::new(right - 24.0, rect.center().y - 12.0),
                Pos2::new(right, rect.center().y + 12.0),
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
                            theme.ui.text_muted,
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
            if text.is_empty() {
                continue;
            }
            let mut job = egui::text::LayoutJob::simple_singleline(
                l10n::text(ui.ctx(), text),
                egui::FontId::proportional(11.0),
                theme.ui.text_muted,
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(width);
            let galley = ui.painter().layout_job(job);
            let chip = egui::Rect::from_center_size(
                Pos2::new(edge - galley.size().x / 2.0, rect.center().y),
                galley.size() + Vec2::new(8.0, 4.0),
            );
            ui.painter().rect_filled(
                chip,
                theme.radius.xs,
                theme
                    .ui
                    .panel_bg
                    .lerp_to_gamma(theme.ui.panel_bg_secondary, 0.6),
            );
            ui.painter().galley(
                chip.center() - galley.size() / 2.0,
                galley,
                theme.ui.text_muted,
            );
        }
        let indent = if row.nested { 24.0 } else { 0.0 };
        let mark = egui::Rect::from_center_size(
            Pos2::new(rect.left() + 22.0 + indent, rect.center().y),
            Vec2::splat(36.0),
        );
        paint_symbol_mark(ui, symbol, mark);
        let left = mark.right() + 8.0;
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
        paint_symbol_text(
            ui,
            symbol,
            egui::Rect::from_min_max(
                Pos2::new(left, rect.top()),
                Pos2::new(text_right.max(left), rect.bottom()),
            ),
            &self.search_query,
            [theme.ui.text, theme.ui.text_muted],
            self.config.show_description,
            false,
        );

        (response, toggled)
    }
}

/// Corner radius of the symbol search and comparison dialogs.
pub(super) const SEARCH_RADIUS: f32 = 20.0;

/// A light veil over light themes; the modal overlay on dark themes.
pub(super) fn search_backdrop(ctx: &Context) -> Color32 {
    let theme = ctx.active_theme();
    if theme.dark {
        theme.ui.modal_overlay
    } else {
        theme
            .ui
            .panel_bg
            .lerp_to_gamma(theme.ui.text, 0.2)
            .gamma_multiply(0.3)
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

/// Shared two-line instrument layout, with an optional type badge after the description.
pub(super) fn paint_symbol_text(
    ui: &Ui,
    symbol: &Instrument,
    rect: egui::Rect,
    query: &str,
    colors: [Color32; 2],
    description: bool,
    kind_badge: bool,
) {
    let theme = ui.active_theme();
    let left = rect.left();
    let width = rect.width();
    let line = |text: &str, size: f32, color: Color32, width: f32| {
        let mut job = highlighted_text(text, query, size, color, theme.ui.accent);
        job.wrap = egui::text::TextWrapping::truncate_at_width(width);
        ui.painter().layout_job(job)
    };
    let name = line(&symbol.symbol, 16.0, colors[0], width);
    ui.painter().galley(
        Pos2::new(
            left,
            if description {
                rect.center().y - 1.0 - name.size().y
            } else {
                rect.center().y - name.size().y / 2.0
            },
        ),
        name,
        colors[0],
    );
    if !description {
        return;
    }
    let kind = symbol
        .symbol_type
        .map(|kind| l10n::text(ui.ctx(), kind.to_string()).to_uppercase());
    let badge = kind.filter(|_| kind_badge).map(|kind| {
        ui.painter().layout_no_wrap(
            kind,
            egui::FontId::proportional(theme.text.micro),
            colors[1],
        )
    });
    let badge_width = badge.as_ref().map_or(0.0, |badge| badge.size().x + 14.0);
    let description = line(
        symbol.description.as_deref().unwrap_or(&symbol.symbol),
        12.0,
        colors[1],
        (width - badge_width).max(0.0),
    );
    let top = rect.center().y + 3.0;
    let badge_left = left + description.size().x + 6.0;
    let description_height = description.size().y;
    ui.painter()
        .galley(Pos2::new(left, top), description, colors[1]);
    if let Some(badge) = badge {
        let frame = egui::Rect::from_min_size(
            Pos2::new(
                badge_left,
                top + (description_height - badge.size().y - 4.0) / 2.0,
            ),
            badge.size() + Vec2::new(8.0, 4.0),
        );
        ui.painter()
            .rect_filled(frame, theme.radius.xs, theme.ui.btn_bg_hover);
        ui.painter()
            .galley(frame.min + Vec2::new(4.0, 2.0), badge, colors[1]);
    }
}

/// Match the same case-insensitive substrings as the local catalog, retaining original glyphs.
fn highlighted_text(
    text: &str,
    query: &str,
    size: f32,
    color: Color32,
    accent: Color32,
) -> egui::text::LayoutJob {
    let lower = text.to_lowercase();
    let query = query.to_lowercase();
    let matches: Vec<_> = if query.is_empty() {
        Vec::new()
    } else {
        lower
            .match_indices(&query)
            .map(|(start, matched)| start..start + matched.len())
            .collect()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut offset = 0;
    for ch in text.chars() {
        let end = offset + ch.to_lowercase().map(char::len_utf8).sum::<usize>();
        let matched = matches
            .iter()
            .any(|range| range.start < end && range.end > offset);
        job.append(
            &ch.to_string(),
            0.0,
            egui::TextFormat::simple(
                egui::FontId::proportional(size),
                if matched { accent } else { color },
            ),
        );
        offset = end;
    }
    job
}
