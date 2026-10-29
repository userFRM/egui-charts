//! A grouped treemap of values and changes supplied by the host.

use super::controls::dropdown;
use crate::localization as l10n;
use crate::services::MarketHeatmapSelection;
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
    /// The value of each metric by [`MarketHeatmapMetric::id`]. A missing or non-finite value is
    /// unavailable for this tile.
    pub values: BTreeMap<String, f64>,
}

/// A market the heatmap can show, with its instruments.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MarketHeatmapUniverse {
    /// Identifies the universe in [`MarketHeatmapSelection::universe`].
    pub id: String,
    /// The name shown, such as `"S&P 500"`.
    pub name: String,
    /// The instruments to show, grouped by `group`.
    pub entries: Vec<MarketHeatmapEntry>,
}

impl MarketHeatmapUniverse {
    /// The universe `id` named `name` of `entries`.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        entries: Vec<MarketHeatmapEntry>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            entries,
        }
    }
}

/// A quantity the host supplies for each tile, to size or to colour the tiles by.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MarketHeatmapMetric {
    /// Identifies the metric in [`MarketHeatmapEntry::values`] and in the selection.
    pub id: String,
    /// The name shown, such as `"Market cap"`.
    pub label: String,
    /// The unit of its values, such as `"%"` or `"USD"`; empty for none.
    pub unit: String,
    /// Whether the host has values for it now. An unavailable metric is listed but cannot be
    /// chosen, and a chosen one that becomes unavailable stays chosen.
    pub available: bool,
    /// The magnitude that colours a tile at full strength, such as `5.0` for a percentage change;
    /// the largest magnitude among the shown tiles when `None`.
    pub saturation: Option<f64>,
}

impl MarketHeatmapMetric {
    /// The available metric `id` named `label`, in `unit`.
    pub fn new(id: impl Into<String>, label: impl Into<String>, unit: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            unit: unit.into(),
            available: true,
            saturation: None,
        }
    }
}

/// Host-fed market heatmap. Update the fields in place and request an egui repaint when the host
/// data changes. The same panel recomputes its geometry from the current values on each frame.
///
/// The user picks the universe, the metric that sizes the tiles and the metric that colours them.
/// Without a choice the first universe is shown, sized by the first available metric and coloured
/// by the second. Store [`selection`](Self::selection) with the settings and assign it back to
/// restore the choices; one the host does not offer at the moment stays chosen and shows as
/// unavailable.
#[derive(Default)]
pub struct MarketHeatmapPanel {
    /// The markets to choose from.
    pub universes: Vec<MarketHeatmapUniverse>,
    /// The metrics to size and colour by.
    pub metrics: Vec<MarketHeatmapMetric>,
    /// The user's choices.
    pub selection: MarketHeatmapSelection,
}

impl MarketHeatmapPanel {
    /// The metric that sizes the tiles and the one that colours them, or `None` for a chosen one
    /// the host does not offer or has no values for.
    fn chosen(&self) -> (Option<&MarketHeatmapMetric>, Option<&MarketHeatmapMetric>) {
        let mut available = self.metrics.iter().filter(|metric| metric.available);
        let first = available.next();
        let second = available.next().or(first);
        let pick = |id: &Option<String>, default| match id {
            Some(id) => self
                .metrics
                .iter()
                .find(|metric| &metric.id == id && metric.available),
            None => default,
        };
        (
            pick(&self.selection.size, first),
            pick(&self.selection.colour, second),
        )
    }

    /// Render the chosen universe, returning the symbol the host should open when selected.
    /// Tile areas follow the size metric. Colours diverge from the theme's panel background
    /// towards its down/up colours by the colour metric, with a legend of its scale. Values are
    /// shown verbatim on hover.
    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        PanelHeader::new("Market heatmap").show_plain(ui);
        let (size, colour) = self.chosen();
        let (size, colour) = (size.cloned(), colour.cloned());
        let universe = match &self.selection.universe {
            Some(id) => self.universes.iter().find(|universe| &universe.id == id),
            None => self.universes.first(),
        };
        let mut selection = self.selection.clone();
        ui.horizontal_wrapped(|ui| {
            if !self.universes.is_empty() {
                let shown =
                    universe.map_or("Unavailable market", |universe| universe.name.as_str());
                dropdown(ui, "heatmap_universe", shown, |ui| {
                    for entry in &self.universes {
                        ui.selectable_value(
                            &mut selection.universe,
                            Some(entry.id.clone()),
                            l10n::text(ui.ctx(), &entry.name),
                        );
                    }
                });
                ui.end_row();
            }
            for (id, prefix, target, chosen) in [
                ("heatmap_size", "Size", &mut selection.size, &size),
                ("heatmap_colour", "Colour", &mut selection.colour, &colour),
            ] {
                if self.metrics.is_empty() {
                    continue;
                }
                let name = chosen.as_ref().map_or("Unavailable", |m| m.label.as_str());
                let text = format!("{}: {name}", l10n::text(ui.ctx(), prefix));
                dropdown(ui, id, &text, |ui| {
                    for metric in &self.metrics {
                        let label = if metric.available {
                            l10n::text(ui.ctx(), &metric.label).to_string()
                        } else {
                            format!(
                                "{} ({})",
                                l10n::text(ui.ctx(), &metric.label),
                                l10n::text(ui.ctx(), "unavailable")
                            )
                        };
                        ui.add_enabled_ui(metric.available, |ui| {
                            ui.selectable_value(target, Some(metric.id.clone()), label)
                        });
                    }
                });
            }
        });
        self.selection = selection;
        let mut rect = ui.available_rect_before_wrap();
        rect.set_bottom(rect.bottom() - LEGEND_HEIGHT);
        let (Some(universe), Some(size)) = (universe, &size) else {
            EmptyState::new(if universe.is_none() && !self.universes.is_empty() {
                "This market is unavailable"
            } else if size.is_none() && !self.metrics.is_empty() {
                "The size metric is unavailable"
            } else {
                "No market heatmap data"
            })
            .show(ui);
            return None;
        };
        let groups = layout(&universe.entries, &size.id, rect);
        if groups.is_empty() {
            EmptyState::new("No market heatmap data").show(ui);
            return None;
        }
        ui.allocate_exact_size(rect.size(), Sense::hover());
        let theme = ui.active_theme();
        let scale = colour.as_ref().and_then(|metric| {
            metric.saturation.filter(|s| *s > 0.0).or_else(|| {
                let largest = universe
                    .entries
                    .iter()
                    .filter_map(|entry| entry.values.get(&metric.id))
                    .filter(|v| v.is_finite())
                    .fold(0.0_f64, |a, v| a.max(v.abs()));
                (largest > 0.0).then_some(largest)
            })
        });
        let format = |metric: &MarketHeatmapMetric, value: Option<&f64>, signed: bool| {
            let Some(value) = value.filter(|v| v.is_finite()) else {
                return "No data".to_owned();
            };
            let number = if signed {
                format!("{value:+.2}")
            } else {
                format!("{value}")
            };
            match metric.unit.as_str() {
                "" => number,
                "%" => format!("{number}%"),
                unit => format!("{number} {unit}"),
            }
        };
        let mut selected = None;
        for group in groups {
            for (index, tile) in group.tiles {
                let entry = &universe.entries[index];
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
                let translated_label = l10n::text(ui.ctx(), &entry.symbol);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        ui.is_enabled(),
                        &translated_label,
                    )
                });
                let base = theme.ui.panel_bg_secondary;
                let shade = colour
                    .as_ref()
                    .and_then(|metric| entry.values.get(&metric.id))
                    .filter(|v| v.is_finite())
                    .zip(scale);
                let direction = if shade.is_none_or(|(value, _)| *value >= 0.0) {
                    theme.chart.candle_up
                } else {
                    theme.chart.candle_down
                };
                let amount =
                    shade.map_or(0.0, |(value, scale)| (value.abs() / scale).min(1.0) as f32);
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
                let shown = colour.as_ref().map_or_else(String::new, |metric| {
                    format(metric, entry.values.get(&metric.id), true)
                });
                if tile.width() >= 35.0 && tile.height() >= 32.0 {
                    painter.text(
                        tile.center() - Vec2::new(0.0, 8.0),
                        Align2::CENTER_CENTER,
                        l10n::text(ui.ctx(), &entry.symbol),
                        FontId::proportional(14.0),
                        text,
                    );
                    painter.text(
                        tile.center() + Vec2::new(0.0, 10.0),
                        Align2::CENTER_CENTER,
                        l10n::text(ui.ctx(), &shown),
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
                let colour_line = colour.as_ref().map_or_else(String::new, |metric| {
                    format!("\n{}: {shown}", l10n::text(ui.ctx(), &metric.label))
                });
                response.on_hover_text(format!(
                    "{}\n{}\n{}: {}{colour_line}",
                    entry.symbol,
                    entry.group,
                    l10n::text(ui.ctx(), &size.label),
                    format(size, entry.values.get(&size.id), false),
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
                l10n::text(ui.ctx(), group.name),
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
        let legend = Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() + 4.0),
            Vec2::new(rect.width(), LEGEND_HEIGHT - 4.0),
        );
        ui.allocate_exact_size(Vec2::new(rect.width(), LEGEND_HEIGHT), Sense::hover());
        legend_bar(ui, legend, size, colour.as_ref(), scale, &format);
        selected
    }
}

/// Height reserved below the tiles for the legend.
const LEGEND_HEIGHT: f32 = 44.0;

/// The size metric in words and the colour scale from the falling colour through the background
/// to the rising colour, labelled with the values at its ends and middle.
fn legend_bar(
    ui: &Ui,
    rect: Rect,
    size: &MarketHeatmapMetric,
    colour: Option<&MarketHeatmapMetric>,
    scale: Option<f64>,
    format: &dyn Fn(&MarketHeatmapMetric, Option<&f64>, bool) -> String,
) {
    let theme = ui.active_theme();
    let painter = ui.painter();
    let unit = |metric: &MarketHeatmapMetric| {
        if metric.unit.is_empty() {
            l10n::text(ui.ctx(), &metric.label).to_string()
        } else {
            format!("{} ({})", l10n::text(ui.ctx(), &metric.label), metric.unit)
        }
    };
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        format!("{}: {}", l10n::text(ui.ctx(), "Size"), unit(size)),
        FontId::proportional(11.0),
        theme.ui.text_secondary,
    );
    let (Some(colour), Some(scale)) = (colour, scale) else {
        painter.text(
            rect.left_top() + Vec2::new(0.0, 14.0),
            Align2::LEFT_TOP,
            l10n::text(ui.ctx(), "Colour data unavailable"),
            FontId::proportional(11.0),
            theme.ui.text_secondary,
        );
        return;
    };
    let bar = Rect::from_min_size(
        Pos2::new(rect.left() + rect.width() * 0.45, rect.top() + 18.0),
        Vec2::new(rect.width() * 0.5, 8.0),
    );
    const STEPS: usize = 24;
    for step in 0..STEPS {
        let position = (step as f32 + 0.5) / STEPS as f32 * 2.0 - 1.0;
        let direction = if position < 0.0 {
            theme.chart.candle_down
        } else {
            theme.chart.candle_up
        };
        let color: Color32 = (egui::Rgba::from(theme.ui.panel_bg_secondary)
            * (1.0 - position.abs())
            + egui::Rgba::from(direction) * position.abs())
        .into();
        let x = bar.left() + bar.width() * step as f32 / STEPS as f32;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, bar.top()),
                Pos2::new(x + bar.width() / STEPS as f32 + 0.5, bar.bottom()),
            ),
            0.0,
            color,
        );
    }
    for (value, at, align) in [
        (-scale, bar.left_bottom(), Align2::LEFT_TOP),
        (0.0, bar.center_bottom(), Align2::CENTER_TOP),
        (scale, bar.right_bottom(), Align2::RIGHT_TOP),
    ] {
        painter.text(
            at + Vec2::new(0.0, 1.0),
            align,
            format(colour, Some(&value), value != 0.0),
            FontId::proportional(10.0),
            theme.ui.text_secondary,
        );
    }
    painter.text(
        Pos2::new(rect.left(), rect.top() + 14.0),
        Align2::LEFT_TOP,
        format!("{}: {}", l10n::text(ui.ctx(), "Colour"), unit(colour)),
        FontId::proportional(11.0),
        theme.ui.text_secondary,
    );
}

struct Group<'a> {
    name: &'a str,
    rect: Rect,
    tiles: Vec<(usize, Rect)>,
}

fn layout<'a>(entries: &'a [MarketHeatmapEntry], size: &str, rect: Rect) -> Vec<Group<'a>> {
    let value = |entry: &MarketHeatmapEntry| entry.values.get(size).copied().unwrap_or(f64::NAN);
    let maximum = entries
        .iter()
        .map(value)
        .filter(|value| value.is_finite())
        .fold(0.0_f64, f64::max);
    if maximum <= 0.0 || !rect.is_positive() {
        return vec![];
    }
    let mut groups: BTreeMap<&str, Vec<(usize, f64)>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let weight = value(entry) / maximum;
        if value(entry).is_finite() && weight > 0.0 {
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

    fn entry(symbol: &str, group: &str, size: f64) -> MarketHeatmapEntry {
        MarketHeatmapEntry {
            symbol: symbol.into(),
            group: group.into(),
            values: BTreeMap::from([("cap".into(), size), ("change".into(), 1.0)]),
        }
    }

    #[test]
    fn grouped_tile_areas_follow_host_weights_including_updates() {
        for factor in [1.0, 1e307] {
            let mut entries: Vec<_> = [
                ("A", "Tech", 6.0),
                ("B", "Tech", 3.0),
                ("C", "Other", 1.0),
                ("bad", "Other", f64::NAN),
                ("zero", "Other", 0.0),
                ("negative", "Other", -1.0),
            ]
            .into_iter()
            .map(|(symbol, group, value)| entry(symbol, group, value * factor))
            .collect();
            entries.push(MarketHeatmapEntry {
                symbol: "missing".into(),
                group: "Other".into(),
                values: BTreeMap::new(),
            });
            for updated in [false, true] {
                if updated {
                    entries[0].values.insert("cap".into(), factor);
                }
                let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
                let groups = layout(&entries, "cap", rect);
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

    /// Guards the choices: size and colour are picked independently from the host's metrics, the
    /// first two available ones are the defaults, and a chosen metric or universe the host does
    /// not offer or has no data for stays chosen, through a frame, rather than falling back.
    #[test]
    fn size_and_colour_are_independent_and_unavailable_choices_stay_chosen() {
        let mut volume = MarketHeatmapMetric::new("volume", "Volume", "");
        volume.available = false;
        let mut panel = MarketHeatmapPanel {
            universes: vec![
                MarketHeatmapUniverse::new("us", "US stocks", vec![entry("A", "Tech", 2.0)]),
                MarketHeatmapUniverse::new("coins", "Coins", vec![]),
            ],
            metrics: vec![
                volume,
                MarketHeatmapMetric::new("cap", "Market cap", "USD"),
                MarketHeatmapMetric::new("change", "Change", "%"),
            ],
            selection: MarketHeatmapSelection::default(),
        };
        let ids = |panel: &MarketHeatmapPanel| {
            let (size, colour) = panel.chosen();
            (size.map(|m| m.id.clone()), colour.map(|m| m.id.clone()))
        };
        assert_eq!(ids(&panel), (Some("cap".into()), Some("change".into())));
        panel.selection.colour = Some("cap".into());
        assert_eq!(ids(&panel), (Some("cap".into()), Some("cap".into())));
        panel.selection.size = Some("volume".into());
        assert_eq!(ids(&panel), (None, Some("cap".into())));
        panel.selection.size = Some("gone".into());
        panel.selection.universe = Some("delisted".into());
        assert_eq!(ids(&panel), (None, Some("cap".into())));

        let before = panel.selection.clone();
        let mut h = egui_kittest::Harness::builder()
            .with_size(Vec2::new(320.0, 500.0))
            .with_context_setup(|ctx| {
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                egui_extras::install_image_loaders(ctx);
            })
            .build_ui_state(
                |ui, panel: &mut MarketHeatmapPanel| _ = panel.show(ui),
                panel,
            );
        h.run();
        assert_eq!(h.state().selection, before);
    }
}
