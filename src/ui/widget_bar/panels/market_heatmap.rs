//! A grouped treemap of values and changes supplied by the host.

use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, PanelHeader};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::BTreeMap;

/// One instrument of the market heatmap. The library computes no market data.
#[derive(Clone, Debug, PartialEq)]
pub struct MarketHeatmapEntry {
    /// Symbol returned when the tile is selected.
    pub symbol: String,
    /// Sector or another group named by the host.
    pub group: String,
    /// Tile weight, such as market capitalization or volume. Nonpositive or non-finite values
    /// have no tile; the remaining tile areas are proportional to their weights.
    pub value: f64,
    /// Signed percentage change; the theme's neutral colour represents a non-finite change.
    pub change_percent: f64,
}

/// Host-fed market heatmap. Update `entries` in place and request an egui repaint when the host
/// data changes. The same panel recomputes its geometry from the current values on each frame.
#[derive(Default)]
pub struct MarketHeatmapPanel {
    /// Instruments to show, grouped by `group` and sized by `value`.
    pub entries: Vec<MarketHeatmapEntry>,
}

impl MarketHeatmapPanel {
    /// Render the current entries, returning the symbol the host should open when selected.
    /// Colours diverge from the theme's panel background towards its down/up colours and
    /// saturate at a five percent move in either direction. Values are shown verbatim on hover.
    pub fn show(&self, ui: &mut Ui) -> Option<String> {
        PanelHeader::new("Market heatmap").show_plain(ui);
        let rect = ui.available_rect_before_wrap();
        let groups = layout(&self.entries, rect);
        if groups.is_empty() {
            EmptyState::new("No market heatmap data").show(ui);
            return None;
        }
        ui.allocate_exact_size(rect.size(), Sense::hover());
        let theme = ui.active_theme();
        let mut selected = None;
        for group in groups {
            for (index, tile) in group.tiles {
                let entry = &self.entries[index];
                let tile = tile.shrink(1.0);
                if !tile.is_positive() {
                    continue;
                }
                let response = ui.interact(
                    tile,
                    ui.scope_id()
                        .with(("market_heatmap", &entry.symbol, &entry.group)),
                    Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &entry.symbol)
                });
                let base = theme.ui.panel_bg_secondary;
                let direction = if entry.change_percent >= 0.0 {
                    theme.chart.candle_up
                } else {
                    theme.chart.candle_down
                };
                let amount = if entry.change_percent.is_finite() {
                    (entry.change_percent.abs() / 5.0).min(1.0) as f32
                } else {
                    0.0
                };
                let color: Color32 = (egui::Rgba::from(base) * (1.0 - amount)
                    + egui::Rgba::from(direction) * amount)
                    .into();
                ui.painter().rect_filled(tile, 0.0, color);
                let linear = egui::Rgba::from(color);
                let luminance = 0.2126 * linear.r() + 0.7152 * linear.g() + 0.0722 * linear.b();
                let text = if luminance > 0.179 {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                };
                let painter = ui.painter().with_clip_rect(tile);
                let change = if entry.change_percent.is_finite() {
                    format!("{:+.2}%", entry.change_percent)
                } else {
                    "No change data".into()
                };
                if tile.width() >= 35.0 && tile.height() >= 32.0 {
                    painter.text(
                        tile.center() - Vec2::new(0.0, 8.0),
                        Align2::CENTER_CENTER,
                        &entry.symbol,
                        FontId::proportional(14.0),
                        text,
                    );
                    painter.text(
                        tile.center() + Vec2::new(0.0, 10.0),
                        Align2::CENTER_CENTER,
                        &change,
                        FontId::proportional(11.0),
                        text,
                    );
                }
                if response.hovered() || response.has_focus() {
                    painter.rect_stroke(
                        tile,
                        0.0,
                        Stroke::new(2.0, theme.ui.text),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    selected = Some(entry.symbol.clone());
                }
                response.on_hover_text(format!(
                    "{}\n{}\nValue: {}\nChange: {change}",
                    entry.symbol, entry.group, entry.value
                ));
            }
            ui.painter().rect_stroke(
                group.rect,
                0.0,
                Stroke::new(2.0, theme.ui.border),
                egui::StrokeKind::Inside,
            );
            let painter = ui.painter().with_clip_rect(group.rect);
            let label = painter.layout_no_wrap(
                group.name.to_owned(),
                FontId::proportional(11.0),
                theme.ui.text,
            );
            let at = group.rect.left_top() + Vec2::new(4.0, 3.0);
            painter.rect_filled(
                Rect::from_min_size(at, label.size()).expand(2.0),
                0.0,
                theme.ui.panel_bg,
            );
            painter.galley(at, label, theme.ui.text);
        }
        selected
    }
}

struct Group<'a> {
    name: &'a str,
    rect: Rect,
    tiles: Vec<(usize, Rect)>,
}

fn layout(entries: &[MarketHeatmapEntry], rect: Rect) -> Vec<Group<'_>> {
    let maximum = entries
        .iter()
        .filter(|entry| entry.value.is_finite())
        .map(|entry| entry.value)
        .fold(0.0_f64, f64::max);
    if maximum <= 0.0 || !rect.is_positive() {
        return vec![];
    }
    let mut groups: BTreeMap<&str, Vec<(usize, f64)>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let weight = entry.value / maximum;
        if entry.value.is_finite() && weight > 0.0 {
            groups
                .entry(&entry.group)
                .or_default()
                .push((index, weight));
        }
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    for (_, items) in &mut groups {
        items.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    let mut weights: Vec<_> = groups
        .iter()
        .enumerate()
        .map(|(index, (_, items))| (index, items.iter().map(|item| item.1).sum::<f64>()))
        .collect();
    weights.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    partition(rect, &weights)
        .into_iter()
        .map(|(index, rect)| Group {
            name: groups[index].0,
            rect,
            tiles: partition(rect, &groups[index].1),
        })
        .collect()
}

/// Binary treemap: each split allocates exactly its share of the remaining area along the
/// longer edge. Splitting near half the weight avoids unnecessary thin strips.
fn partition(rect: Rect, items: &[(usize, f64)]) -> Vec<(usize, Rect)> {
    let mut pending = vec![(rect, items)];
    let mut tiles = Vec::new();
    while let Some((rect, items)) = pending.pop() {
        if items.is_empty() {
            continue;
        }
        if items.len() == 1 {
            tiles.push((items[0].0, rect));
            continue;
        }
        let total: f64 = items.iter().map(|item| item.1).sum::<f64>();
        let mut sum = 0.0;
        let mut split = (1, items[0].1);
        for (index, item) in items.iter().take(items.len() - 1).enumerate() {
            sum += item.1;
            if (sum - total / 2.0).abs() < (split.1 - total / 2.0).abs() {
                split = (index + 1, sum);
            }
        }
        let fraction = (split.1 / total) as f32;
        let (first, second) = if rect.width() >= rect.height() {
            let x = rect.left() + rect.width() * fraction;
            (
                Rect::from_min_max(rect.min, Pos2::new(x, rect.bottom())),
                Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
            )
        } else {
            let y = rect.top() + rect.height() * fraction;
            (
                Rect::from_min_max(rect.min, Pos2::new(rect.right(), y)),
                Rect::from_min_max(Pos2::new(rect.left(), y), rect.max),
            )
        };
        pending.push((second, &items[split.0..]));
        pending.push((first, &items[..split.0]));
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_tile_areas_follow_host_weights_including_updates() {
        for factor in [1.0, 1e307] {
            let mut panel = MarketHeatmapPanel {
                entries: [
                    ("A", "Tech", 6.0),
                    ("B", "Tech", 3.0),
                    ("C", "Other", 1.0),
                    ("bad", "Other", f64::NAN),
                    ("zero", "Other", 0.0),
                    ("negative", "Other", -1.0),
                ]
                .into_iter()
                .map(|(symbol, group, value)| MarketHeatmapEntry {
                    symbol: symbol.into(),
                    group: group.into(),
                    value: value * factor,
                    change_percent: 1.0,
                })
                .collect(),
            };
            for updated in [false, true] {
                if updated {
                    panel.entries[0].value = factor;
                }
                let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
                let groups = layout(&panel.entries, rect);
                let tiles: Vec<_> = groups.iter().flat_map(|group| group.tiles.iter()).collect();
                // 10,000 pixels / (6+3+1): 6,000, 3,000, 1,000. After A=1:
                // 10,000 / (1+3+1): 2,000, 6,000, 2,000, across both groups.
                let expected = if updated {
                    [2000.0, 6000.0, 2000.0]
                } else {
                    [6000.0, 3000.0, 1000.0]
                };
                assert_eq!(tiles.len(), 3);
                for (index, tile) in &tiles {
                    assert!(
                        (tile.area() - expected[*index]).abs() < 0.01,
                        "{index}: {tile:?}"
                    );
                    assert!(rect.contains_rect(*tile));
                }
                for (i, (_, a)) in tiles.iter().enumerate() {
                    for (_, b) in &tiles[i + 1..] {
                        assert!(a.intersect(*b).area() <= 0.0);
                    }
                }
            }
        }
    }
}
