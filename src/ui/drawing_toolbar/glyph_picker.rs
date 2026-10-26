//! Searchable bundled icons, emoji and stickers with session-local recents.

use super::DrawingToolbarAction;
use crate::drawings::glyphs::{self, GLYPHS, Glyph};
use crate::drawings::{DrawingToolRegistry, DrawingToolType};
use egui::{Rect, Ui, Vec2};

/// Session state for the drawing glyph picker.
#[derive(Clone, Debug)]
pub struct GlyphPicker {
    /// Search by glyph name across icons, emoji and stickers.
    pub query: String,
    /// Size in logical pixels, independent of line width.
    pub size: f32,
    group: &'static str,
    recent: Vec<&'static str>,
}

impl Default for GlyphPicker {
    fn default() -> Self {
        Self {
            query: String::new(),
            size: 32.0,
            group: "All",
            recent: Vec::new(),
        }
    }
}

impl GlyphPicker {
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        registry: &DrawingToolRegistry,
    ) -> Option<DrawingToolbarAction> {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        ui.set_width(312.0_f32.min(ui.ctx().content_rect().width() - 32.0));
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search icons, emoji and stickers")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            for group in ["All", "Icons", "Emoji", "Stickers"] {
                ui.selectable_value(&mut self.group, group, group);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Size");
            ui.add(
                egui::DragValue::new(&mut self.size)
                    .range(8.0..=128.0)
                    .suffix(" px"),
            );
        });
        ui.separator();
        let query = self.query.trim().to_lowercase();
        let matches = |glyph: &&Glyph| {
            registry.availability(tool_type(glyph.id)).visible
                && ((query.is_empty()
                    && (self.group == "All" || glyph.category.starts_with(self.group)))
                    || (!query.is_empty() && glyph.name.to_lowercase().contains(&query)))
        };
        let mut picked = None;
        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                let recent: Vec<_> = self
                    .recent
                    .iter()
                    .filter_map(|id| GLYPHS.iter().find(|g| g.id == *id))
                    .filter(matches)
                    .collect();
                if !recent.is_empty() {
                    ui.label("Recent");
                    picked = cells(ui, &recent, registry);
                }
                let filtered: Vec<_> = GLYPHS.iter().filter(matches).collect();
                if filtered.is_empty() {
                    ui.label("No matching glyphs");
                }
                let mut start = 0;
                while start < filtered.len() {
                    let category = filtered[start].category;
                    let end = start
                        + filtered[start..]
                            .iter()
                            .take_while(|g| g.category == category)
                            .count();
                    ui.label(category);
                    picked = picked.or(cells(ui, &filtered[start..end], registry));
                    start = end;
                }
            });
        let glyph = picked?;
        self.recent.retain(|id| *id != glyph);
        self.recent.insert(0, glyph);
        self.recent.truncate(12);
        Some(DrawingToolbarAction::InsertGlyph {
            icon_name: glyph,
            size: self.size,
        })
    }
}

fn cells(ui: &mut Ui, glyphs: &[&Glyph], registry: &DrawingToolRegistry) -> Option<&'static str> {
    let columns = ((ui.available_width() / 48.0) as usize).max(1);
    let mut picked = None;
    for row in glyphs.chunks(columns) {
        ui.horizontal(|ui| {
            for glyph in row {
                let response = ui
                    .add_enabled(
                        registry.availability(tool_type(glyph.id)).enabled,
                        egui::Button::new("").min_size(Vec2::splat(40.0)),
                    )
                    .on_hover_text(glyph.name);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, glyph.name)
                });
                let rect = Rect::from_center_size(response.rect.center(), Vec2::splat(26.0));
                let color = ui.visuals().text_color();
                if !glyphs::paint(ui.painter(), glyph.id, rect, color) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        glyph.id,
                        egui::FontId::proportional(24.0),
                        color,
                    );
                }
                if response.clicked() {
                    picked = Some(glyph.id);
                }
            }
        });
    }
    picked
}

fn tool_type(id: &str) -> DrawingToolType {
    if id.starts_with("sticker:") {
        DrawingToolType::Sticker
    } else {
        DrawingToolType::FontIcon
    }
}
