//! Searchable bundled icons, emoji and stickers with session-local recents.

use super::DrawingToolbarAction;
use crate::drawings::glyphs::{self, Glyph};
use crate::drawings::{DrawingToolRegistry, DrawingToolType};
use crate::localization as l10n;
use egui::{Rect, Ui, Vec2};

/// Session state for the drawing glyph picker.
///
/// ```
/// let mut picker = egui_charts::ui::drawing_toolbar::GlyphPicker::default();
/// picker.sticker_size = 96.0;
/// ```
#[derive(Clone, Debug)]
pub struct GlyphPicker {
    /// Search by glyph name across icons, emoji and stickers.
    pub query: String,
    /// Size in logical pixels of a placed icon or emoji, independent of line width.
    pub size: f32,
    /// Size in logical pixels of a placed sticker, independent of [`Self::size`].
    pub sticker_size: f32,
    group: &'static str,
    recent: Vec<&'static str>,
}

impl Default for GlyphPicker {
    fn default() -> Self {
        Self {
            query: String::new(),
            size: 40.0,
            sticker_size: 110.0,
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
                .hint_text(l10n::text(ui.ctx(), "Search icons, emoji and stickers"))
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            for group in ["All", "Icons", "Emoji", "Stickers"] {
                ui.selectable_value(&mut self.group, group, l10n::text(ui.ctx(), group));
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, size) in [
                ("Size", &mut self.size),
                ("Sticker size", &mut self.sticker_size),
            ] {
                ui.label(l10n::text(ui.ctx(), label));
                ui.add(
                    egui::DragValue::new(size)
                        .range(8.0..=128.0)
                        .suffix(l10n::text(ui.ctx(), " px")),
                );
            }
        });
        ui.separator();
        let query = self.query.trim().to_lowercase();
        let ctx = ui.ctx().clone();
        let matches = |glyph: &&Glyph| {
            registry.availability(tool_type(glyph.id)).visible
                && ((query.is_empty()
                    && (self.group == "All" || glyph.category.starts_with(self.group)))
                    || (!query.is_empty()
                        && (glyph.name.to_lowercase().contains(&query)
                            || l10n::text(&ctx, glyph.name).to_lowercase().contains(&query))))
        };
        let mut picked = None;
        egui::ScrollArea::vertical()
            .max_height(400.0)
            .show(ui, |ui| {
                let recent: Vec<_> = self
                    .recent
                    .iter()
                    .filter_map(|id| glyphs::picker_glyphs().find(|g| g.id == *id))
                    .filter(matches)
                    .collect();
                if !recent.is_empty() {
                    ui.label(l10n::text(ui.ctx(), "Recent"));
                    picked = cells(ui, &recent, registry);
                }
                let filtered: Vec<_> = glyphs::picker_glyphs().filter(matches).collect();
                if filtered.is_empty() {
                    ui.label(l10n::text(ui.ctx(), "No matching glyphs"));
                }
                let mut start = 0;
                while start < filtered.len() {
                    let category = filtered[start].category;
                    let end = start
                        + filtered[start..]
                            .iter()
                            .take_while(|g| g.category == category)
                            .count();
                    ui.label(l10n::text(ui.ctx(), category));
                    picked = picked.or(cells(ui, &filtered[start..end], registry));
                    start = end;
                }
            });
        let glyph = picked?;
        let tool = tool_type(glyph);
        if !registry.availability(tool).enabled {
            return Some(DrawingToolbarAction::ToolUnavailable(tool));
        }
        self.recent.retain(|id| *id != glyph);
        self.recent.insert(0, glyph);
        self.recent.truncate(12);
        Some(DrawingToolbarAction::InsertGlyph {
            icon_name: glyph,
            size: if glyph.starts_with("sticker:") {
                self.sticker_size
            } else {
                self.size
            },
        })
    }
}

fn cells(ui: &mut Ui, glyphs: &[&Glyph], registry: &DrawingToolRegistry) -> Option<&'static str> {
    let columns = ((ui.available_width() / 48.0) as usize).max(1);
    let mut picked = None;
    for row in glyphs.chunks(columns) {
        // Rows off screen take their place without widgets: the full emoji set has thousands of cells.
        let row_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 40.0));
        if !ui.is_rect_visible(row_rect) {
            ui.allocate_space(Vec2::new(ui.available_width(), 40.0));
            continue;
        }
        ui.horizontal(|ui| {
            for glyph in row {
                let enabled = registry.availability(tool_type(glyph.id)).enabled;
                // A disabled glyph is drawn as egui draws a disabled button but still answers
                // clicks, so the host can say why.
                let opacity = ui.opacity();
                if !enabled {
                    ui.multiply_opacity(ui.visuals().disabled_alpha());
                }
                let response = ui
                    .add(egui::Button::new("").min_size(Vec2::splat(40.0)))
                    .on_hover_text(l10n::text(ui.ctx(), glyph.name));
                let translated_label = l10n::text(ui.ctx(), glyph.name);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, enabled, &translated_label)
                });
                let rect = Rect::from_center_size(response.rect.center(), Vec2::splat(26.0));
                let color = ui.visuals().text_color();
                if !glyphs::paint(ui.painter(), glyph.id, rect, color, 0.0) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        l10n::text(ui.ctx(), glyph.id),
                        glyphs::font_id(ui.ctx(), glyph.id, 24.0),
                        color,
                    );
                }
                ui.set_opacity(opacity);
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
