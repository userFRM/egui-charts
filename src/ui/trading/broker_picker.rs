//! The broker picker: the dialog in which the user chooses the broker to trade with.

use crate::localization as l10n;
use egui::{
    Align, Align2, Color32, Context, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, vec2,
};

use super::account_panel::semibold;
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;
use crate::ui_kit::modal::Modal;

const TILE: egui::Vec2 = vec2(164.0, 152.0);
const GAP: f32 = 8.0;
/// Width of a column of the comparison view.
const COLUMN: f32 = 200.0;
/// Brokers compared at once.
const MAX_COMPARED: usize = 3;

/// One row of a broker in the picker's comparison view.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum BrokerFact {
    /// A value, such as a price or the leverage, formatted by the host.
    Value(String),
    /// Whether the broker supports a feature.
    Supported(bool),
}

/// One broker of the [`BrokerPicker`], as the host offers it.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BrokerTile {
    /// The host's id for the broker, returned by [`BrokerPickerAction::Connect`].
    pub id: String,
    /// The broker's name.
    pub name: String,
    /// The broker's logo, drawn in a 48 point square. Without one the tile shows the name's
    /// first letter.
    pub icon: Option<egui::ImageSource<'static>>,
    /// A line under the name, such as what the broker offers.
    pub subtitle: Option<String>,
    /// A rating out of 5, shown with a star under the name when there is no subtitle.
    pub rating: Option<f32>,
    /// Marks the tile as featured.
    pub featured: bool,
    /// The user's favourite; the user toggles it with the tile's star. Persist it in the host.
    pub favorite: bool,
    /// What the connect card says about the broker.
    pub description: String,
    /// The rows of the comparison view by label, such as pricing, leverage and features. A
    /// label that another compared broker has and this one lacks shows as not available.
    pub facts: Vec<(String, BrokerFact)>,
}

impl BrokerTile {
    /// The id of [`paper`](Self::paper).
    pub const PAPER: &'static str = "paper";

    /// A tile with only a name.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: None,
            subtitle: None,
            rating: None,
            featured: false,
            favorite: false,
            description: String::new(),
            facts: Vec::new(),
        }
    }

    /// The built-in paper account, for a [`PaperAccount`](crate::services::PaperAccount).
    pub fn paper() -> Self {
        let mut tile = Self::new(Self::PAPER, "Paper Trading");
        tile.subtitle = Some("Brokerage simulator".to_owned());
        tile.description = "Paper trading lets you practise buying and selling with simulated \
            money: orders fill against the chart's prices as they would at a broker, and no \
            real money is used.\n\nThe account keeps your positions, order history, balance \
            history and a journal of every action, so you can review your trades."
            .to_owned();
        tile
    }
}

/// What the user chose in the [`BrokerPicker`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BrokerPickerAction {
    /// Connect to the broker with this [`BrokerTile::id`], such as [`BrokerTile::PAPER`].
    Connect(String),
    /// The footer link: show how to open an account at a broker.
    NeedBroker,
}

/// The dialog in which the user picks a broker: a grid of the host's [`BrokerTile`]s, the
/// paper account first, with a search and a footer link. Its compare button shows the
/// [`facts`](BrokerTile::facts) of two brokers side by side in aligned rows, with a control to
/// add another and one to connect each. Picking a tile shows its connect card,
/// whose Connect button returns [`BrokerPickerAction::Connect`] and closes the picker; the host
/// then connects [`Trading`](super::Trading) to that broker. Open it with
/// [`open`](Self::open), such as from the toolbar's Trade button while no broker is connected.
pub struct BrokerPicker {
    /// The tiles, in order. [`new`](Self::new) puts the paper account first.
    pub tiles: Vec<BrokerTile>,
    open: bool,
    searching: bool,
    query: String,
    /// The tile whose connect card is open.
    selected: Option<usize>,
    /// The ids of the compared brokers, while the comparison view is shown.
    compared: Option<Vec<String>>,
}

impl Default for BrokerPicker {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl BrokerPicker {
    /// A closed picker showing the paper account, then `tiles`.
    pub fn new(tiles: Vec<BrokerTile>) -> Self {
        Self {
            tiles: std::iter::once(BrokerTile::paper()).chain(tiles).collect(),
            open: false,
            searching: false,
            query: String::new(),
            selected: None,
            compared: None,
        }
    }

    /// Opens the picker with an empty search.
    pub fn open(&mut self) {
        self.open = true;
        self.searching = false;
        self.query.clear();
        self.selected = None;
        self.compared = None;
    }

    /// Whether the picker is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows the picker while it is open, and its connect card.
    pub fn show(&mut self, ctx: &Context) -> Option<BrokerPickerAction> {
        let mut action = None;
        let theme = ctx.active_theme();
        let query = self.query.to_lowercase();
        let mut chosen = None;
        let mut close = false;
        Modal::new("broker_picker", "")
            .fill(if theme.dark {
                theme.ui.btn_bg_hover
            } else {
                theme.ui.panel_bg
            })
            .width(1000.0)
            .height(760.0)
            .closable(self.selected.is_none())
            .show(ctx, &mut self.open, |ui| {
                let inset = 52.0_f32.min((ui.available_width() - TILE.x).max(0.0) / 2.0);
                let close_rect = Rect::from_min_size(
                    ui.max_rect().right_top() - vec2(24.0, 0.0),
                    vec2(24.0, 24.0),
                );
                close = ui
                    .place(close_rect, |ui: &mut Ui| {
                        IconButton::new(&icons::CLOSE).tooltip("Close").show(ui)
                    })
                    .clicked();
                ui.add_space(32.0);
                ui.horizontal(|ui| {
                    ui.add_space(inset);
                    if self.compared.is_some() {
                        ui.label(semibold(
                            &l10n::text(ui.ctx(), "Compare brokers"),
                            20.0,
                            theme.ui.text,
                        ));
                    } else if self.searching {
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text(l10n::text(ui.ctx(), "Search"))
                                .desired_width((ui.available_width() - 88.0).max(40.0)),
                        );
                        search.request_focus();
                    } else {
                        ui.label(semibold(
                            &l10n::text(ui.ctx(), "Trade with your broker"),
                            if ui.available_width() < 360.0 {
                                16.0
                            } else {
                                20.0
                            },
                            theme.ui.text,
                        ));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(inset);
                        if IconButton::new(&icons::COMPARE_ADD)
                            .tooltip("Compare brokers")
                            .selected(self.compared.is_some())
                            .show(ui)
                            .clicked()
                        {
                            self.compared = match self.compared {
                                Some(_) => None,
                                None => {
                                    Some(self.tiles.iter().take(2).map(|t| t.id.clone()).collect())
                                }
                            };
                        }
                        if self.compared.is_none()
                            && IconButton::new(&icons::QUICK_SEARCH)
                                .tooltip("Search brokers")
                                .selected(self.searching)
                                .show(ui)
                                .clicked()
                        {
                            self.searching = !self.searching;
                            self.query.clear();
                        }
                    });
                });
                ui.add_space(20.0);

                let footer = 56.0;
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - footer).max(0.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(compared) = &mut self.compared {
                            egui::ScrollArea::horizontal().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(inset);
                                    chosen = comparison(ui, &self.tiles, compared);
                                });
                            });
                            return;
                        }
                        let width = ui.available_width() - inset * 2.0;
                        let columns = ((width + GAP) / (TILE.x + GAP)).floor().max(1.0) as usize;
                        let indent = inset;
                        // Room for the featured badges, which overlap their tile's top edge.
                        ui.add_space(8.0);
                        let shown: Vec<usize> = (0..self.tiles.len())
                            .filter(|&i| self.tiles[i].name.to_lowercase().contains(&query))
                            .collect();
                        if shown.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(40.0);
                                ui.label(
                                    RichText::new(l10n::text(
                                        ui.ctx(),
                                        "No brokers match your search",
                                    ))
                                    .color(theme.ui.text_secondary),
                                );
                            });
                        }
                        for row in shown.chunks(columns) {
                            ui.horizontal(|ui| {
                                ui.add_space(indent);
                                ui.spacing_mut().item_spacing.x = GAP;
                                for &index in row {
                                    if tile(ui, &mut self.tiles[index]) {
                                        chosen = Some(index);
                                    }
                                }
                            });
                            ui.add_space(GAP);
                        }
                    });

                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    let link = ui
                        .add(
                            egui::Button::new((
                                icons::SQUARE_PLUS.as_image_square(20.0),
                                RichText::new(l10n::text(ui.ctx(), "Need a broker?"))
                                    .size(theme.text.body + 2.0)
                                    .color(theme.ui.text),
                            ))
                            .frame(false)
                            .image_tint_follows_text_color(true),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if link.clicked() {
                        action = Some(BrokerPickerAction::NeedBroker);
                    }
                });
            });
        if close {
            self.open = false;
        }
        if chosen.is_some() {
            self.selected = chosen;
        }

        let Some(index) = self.selected.filter(|_| self.open) else {
            self.selected = None;
            return action;
        };
        let Some(tile) = self.tiles.get(index) else {
            self.selected = None;
            return action;
        };
        let mut card = true;
        Modal::new("broker_picker_connect", "")
            .fill(theme.ui.panel_bg_floating)
            .width(400.0)
            .min_height(0.0)
            .show(ctx, &mut card, |ui| {
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (icon, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                        paint_icon(ui, tile, icon);
                        ui.add_space(4.0);
                        ui.label(semibold(&tile.name, theme.text.title + 2.0, theme.ui.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if IconButton::new(&icons::CLOSE)
                                .tooltip("Back to brokers")
                                .show(ui)
                                .clicked()
                            {
                                self.selected = None;
                            }
                        });
                    });
                    ui.add_space(16.0);
                    let connect = ui.add_sized(
                        vec2(ui.available_width(), 40.0),
                        egui::Button::new(semibold(
                            &l10n::text(ui.ctx(), "Connect"),
                            theme.text.body,
                            theme.ui.panel_bg_floating,
                        ))
                        .fill(theme.ui.text)
                        .corner_radius(ui.active_theme().radius.md),
                    );
                    if connect.clicked() {
                        action = Some(BrokerPickerAction::Connect(tile.id.clone()));
                        self.open = false;
                    }
                    if !tile.description.is_empty() {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), &tile.description))
                                .size(theme.text.body)
                                .color(theme.ui.text_secondary),
                        );
                    }
                });
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), "Account connection"))
                            .size(theme.text.small)
                            .color(theme.ui.text_muted),
                    );
                });
            });
        if !card {
            self.selected = None;
        }
        action
    }
}

/// Draws one tile; returns whether the user picked it. Its star toggles the favourite.
fn tile(ui: &mut Ui, tile: &mut BrokerTile) -> bool {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(TILE, Sense::click());
    let translated_label = l10n::text(ui.ctx(), &tile.name);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label));
    let star = Rect::from_center_size(rect.right_top() + vec2(-18.0, 18.0), vec2(20.0, 20.0));
    let star_response = ui
        .interact(star, response.id.with("favorite"), Sense::click())
        .on_hover_text(l10n::text(
            ui.ctx(),
            if tile.favorite {
                "Remove from favourites"
            } else {
                "Add to favourites"
            },
        ));
    if star_response.clicked() {
        tile.favorite = !tile.favorite;
    }
    let painter = ui.painter();
    let hovered = response.hovered() || star_response.hovered();
    painter.rect(
        rect,
        8.0,
        if hovered {
            theme.ui.btn_bg_hover
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(1.0, theme.ui.border),
        egui::StrokeKind::Inside,
    );
    if tile.featured {
        let badge = painter.layout_no_wrap(
            l10n::text(ui.ctx(), "FEATURED"),
            FontId::proportional(theme.text.tiny),
            theme.ui.panel_bg_floating,
        );
        let badge_rect = Rect::from_min_size(
            rect.left_top() + vec2(6.0, -6.0),
            badge.size() + vec2(10.0, 2.0),
        );
        painter.rect_filled(badge_rect, 3.0, theme.ui.text);
        painter.galley(badge_rect.min + vec2(5.0, 1.0), badge, theme.ui.text);
    }
    let star_icon = if tile.favorite {
        icons::STAR_FILLED.as_image_tinted(star.size() - vec2(2.0, 2.0), theme.ui.warning)
    } else {
        icons::STAR_EMPTY.as_image_tinted(star.size() - vec2(2.0, 2.0), theme.ui.text_secondary)
    };
    star_icon.paint_at(ui, star.shrink(1.0));
    let icon = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 48.0),
        vec2(48.0, 48.0),
    );
    paint_icon(ui, tile, icon);
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.center().x, icon.bottom() + 18.0),
        Align2::CENTER_CENTER,
        l10n::text(ui.ctx(), &tile.name),
        FontId::proportional(theme.text.body + 2.0),
        theme.ui.text,
    );
    if let Some(subtitle) = &tile.subtitle {
        let galley = painter.layout(
            l10n::text(ui.ctx(), subtitle.clone()),
            FontId::proportional(theme.text.small),
            theme.ui.text_secondary,
            rect.width() - 24.0,
        );
        let at = egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            icon.bottom() + 30.0,
        );
        painter.galley(at, galley, theme.ui.text_secondary);
    } else if let Some(rating) = tile.rating {
        let text = painter.layout_no_wrap(
            format!("{rating:.1}"),
            FontId::proportional(theme.text.body),
            theme.ui.text,
        );
        let width = 16.0 + 4.0 + text.size().x;
        let left = rect.center().x - width / 2.0;
        let y = icon.bottom() + 40.0;
        icons::STAR_FILLED
            .as_image_tinted(vec2(16.0, 16.0), theme.ui.text)
            .paint_at(
                ui,
                Rect::from_min_size(egui::pos2(left, y - 8.0), vec2(16.0, 16.0)),
            );
        ui.painter().galley(
            egui::pos2(left + 20.0, y - text.size().y / 2.0),
            text,
            theme.ui.text,
        );
    }
    response.clicked() && !star_response.clicked()
}

/// Draws the compared brokers in columns beside the labels of their facts, with a column to add
/// another broker; returns the broker whose Connect button the user pressed.
fn comparison(ui: &mut Ui, tiles: &[BrokerTile], compared: &mut Vec<String>) -> Option<usize> {
    let theme = ui.active_theme();
    compared.retain(|id| tiles.iter().any(|tile| tile.id == *id));
    let columns: Vec<_> = compared
        .iter()
        .filter_map(|id| tiles.iter().position(|tile| tile.id == *id))
        .collect();
    let mut labels: Vec<&str> = Vec::new();
    for (label, _) in columns.iter().flat_map(|&i| &tiles[i].facts) {
        if !labels.contains(&label.as_str()) {
            labels.push(label);
        }
    }
    let mut chosen = None;
    let mut removed = None;
    let mut added = None;
    egui::Grid::new("broker_comparison")
        .min_col_width(COLUMN)
        .min_row_height(40.0)
        .spacing(vec2(GAP, 4.0))
        .show(ui, |ui| {
            ui.label("");
            for &index in &columns {
                let tile = &tiles[index];
                ui.vertical_centered(|ui| {
                    ui.set_width(COLUMN);
                    let (rect, _) = ui.allocate_exact_size(vec2(COLUMN, 48.0), Sense::hover());
                    paint_icon(
                        ui,
                        tile,
                        Rect::from_center_size(rect.center(), vec2(48.0, 48.0)),
                    );
                    let remove =
                        Rect::from_min_size(rect.right_top() - vec2(24.0, 0.0), vec2(24.0, 24.0));
                    if ui
                        .place(remove, |ui: &mut Ui| {
                            IconButton::new(&icons::CLOSE)
                                .tooltip("Remove from comparison")
                                .show(ui)
                        })
                        .clicked()
                    {
                        removed = Some(tile.id.clone());
                    }
                    ui.add_space(4.0);
                    ui.label(semibold(
                        &l10n::text(ui.ctx(), &tile.name),
                        theme.text.body + 2.0,
                        theme.ui.text,
                    ));
                    if let Some(rating) = tile.rating {
                        ui.label(
                            RichText::new(format!("{rating:.1} / 5"))
                                .color(theme.ui.text_secondary),
                        );
                    }
                    ui.add_space(4.0);
                    if ui.button(l10n::text(ui.ctx(), "Connect")).clicked() {
                        chosen = Some(index);
                    }
                });
            }
            let others: Vec<_> = tiles
                .iter()
                .filter(|tile| !compared.contains(&tile.id))
                .collect();
            if compared.len() < MAX_COMPARED && !others.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.set_width(COLUMN);
                    ui.add_space(12.0);
                    let add = ui.add(
                        egui::Button::new((
                            icons::PLUS.as_image_square(20.0),
                            RichText::new(l10n::text(ui.ctx(), "Add broker"))
                                .size(theme.text.body + 2.0)
                                .color(theme.ui.text),
                        ))
                        .frame(false)
                        .image_tint_follows_text_color(true),
                    );
                    egui::Popup::menu(&add).show(|ui| {
                        for tile in others {
                            if ui.button(l10n::text(ui.ctx(), &tile.name)).clicked() {
                                added = Some(tile.id.clone());
                            }
                        }
                    });
                });
            }
            ui.end_row();
            for label in &labels {
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), *label)).color(theme.ui.text_secondary),
                );
                for &index in &columns {
                    let fact = tiles[index]
                        .facts
                        .iter()
                        .find(|(l, _)| l == label)
                        .map(|(_, fact)| fact);
                    fact_ui(ui, fact);
                }
                ui.end_row();
            }
        });
    if let Some(id) = removed {
        compared.retain(|other| *other != id);
    }
    compared.extend(added);
    chosen
}

/// One cell of the comparison view: the value, a check for a supported feature, a cross for an
/// unsupported one, and a dash for a fact the host did not give.
fn fact_ui(ui: &mut Ui, fact: Option<&BrokerFact>) {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(vec2(COLUMN, 40.0), Sense::hover());
    let (name, mark) = match fact {
        Some(BrokerFact::Value(value)) => (l10n::text(ui.ctx(), value), None),
        Some(BrokerFact::Supported(true)) => (
            l10n::text(ui.ctx(), "Supported"),
            Some((&icons::CHECK, theme.ui.success)),
        ),
        Some(BrokerFact::Supported(false)) => (
            l10n::text(ui.ctx(), "Not supported"),
            Some((&icons::CLOSE, theme.ui.error)),
        ),
        None => (l10n::text(ui.ctx(), "Not available"), None),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    let font = FontId::proportional(theme.text.body + 2.0);
    match (fact, mark) {
        (_, Some((icon, color))) => icon
            .as_image_tinted(vec2(20.0, 20.0), color)
            .paint_at(ui, Rect::from_center_size(rect.center(), vec2(20.0, 20.0))),
        (Some(_), None) => {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                &name,
                font,
                theme.ui.text,
            );
        }
        (None, None) => {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "-",
                font,
                theme.ui.text_muted,
            );
            response.on_hover_text(name);
        }
    }
}

/// Draws the tile's logo in `rect`: the host's image, a candlestick mark for the paper account,
/// or the name's first letter.
fn paint_icon(ui: &Ui, tile: &BrokerTile, rect: Rect) {
    let theme = ui.active_theme();
    let radius = rect.width() * 0.22;
    if let Some(icon) = &tile.icon {
        egui::Image::new(icon.clone())
            .corner_radius(radius)
            .paint_at(ui, rect);
        return;
    }
    ui.painter().rect_filled(rect, radius, theme.ui.text);
    if tile.id == BrokerTile::PAPER {
        icons::CANDLESTICK
            .as_image_tinted(rect.size() * 0.6, theme.ui.panel_bg_floating)
            .paint_at(ui, Rect::from_center_size(rect.center(), rect.size() * 0.6));
    } else {
        let initial = tile
            .name
            .chars()
            .next()
            .unwrap_or(' ')
            .to_uppercase()
            .to_string();
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            l10n::text(ui.ctx(), initial),
            FontId::proportional(rect.height() * 0.5),
            theme.ui.panel_bg_floating,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// The compared brokers share one row per fact, in the order the brokers give them, and a
    /// broker without a fact shows it as not available, unlike a feature it does not support.
    #[test]
    fn comparison_aligns_facts_and_tells_unavailable_from_unsupported() {
        let mut a = BrokerTile::new("a", "A");
        a.facts = vec![
            ("Leverage".to_owned(), BrokerFact::Value("30:1".to_owned())),
            ("Crypto".to_owned(), BrokerFact::Supported(false)),
        ];
        let mut b = BrokerTile::new("b", "B");
        b.facts = vec![("Crypto".to_owned(), BrokerFact::Supported(true))];
        let tiles = [a, b];
        let mut compared = vec!["a".to_owned(), "b".to_owned()];
        let mut harness = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .with_size(vec2(1000.0, 600.0))
            .build_ui(|ui| {
                comparison(ui, &tiles, &mut compared);
            });
        harness.run();
        let y = |label: &str| harness.get_by_label(label).rect().center().y;
        assert_eq!(y("30:1"), y("Not available"), "leverage row");
        assert_eq!(y("Not supported"), y("Supported"), "crypto row");
        assert!(y("Leverage") < y("Crypto"));
    }
}
