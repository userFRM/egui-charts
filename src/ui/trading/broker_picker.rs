//! The broker picker: the dialog in which the user chooses the broker to trade with.

use egui::{
    Align, Align2, Color32, Context, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, vec2,
};

use super::account_panel::semibold;
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;
use crate::ui_kit::modal::Modal;

const TILE: egui::Vec2 = vec2(156.0, 144.0);
const GAP: f32 = 8.0;

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
/// paper account first, with a search and a footer link. Picking a tile shows its connect card,
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
        }
    }

    /// Opens the picker with an empty search.
    pub fn open(&mut self) {
        self.open = true;
        self.searching = false;
        self.query.clear();
        self.selected = None;
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
            .width(TILE.x * 5.0 + GAP * 4.0 + 160.0)
            .height(680.0)
            .closable(self.selected.is_none())
            .show(ctx, &mut self.open, |ui| {
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    if self.searching {
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Search")
                                .desired_width(ui.available_width() - 120.0),
                        );
                        search.request_focus();
                    } else {
                        ui.label(semibold("Trade with your broker", 20.0, theme.ui.text));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if IconButton::new(&icons::CLOSE)
                            .tooltip("Close")
                            .show(ui)
                            .clicked()
                        {
                            close = true;
                        }
                        ui.add_space(16.0);
                        if IconButton::new(&icons::QUICK_SEARCH)
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
                        let width = ui.available_width() - 80.0;
                        let columns = ((width + GAP) / (TILE.x + GAP)).floor().max(1.0) as usize;
                        let indent =
                            (ui.available_width() - (columns as f32 * (TILE.x + GAP) - GAP)) / 2.0;
                        // Room for the featured badges, which overlap their tile's top edge.
                        ui.add_space(8.0);
                        let shown: Vec<usize> = (0..self.tiles.len())
                            .filter(|&i| self.tiles[i].name.to_lowercase().contains(&query))
                            .collect();
                        if shown.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(40.0);
                                ui.label(
                                    RichText::new("No brokers match your search")
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
                                RichText::new("Need a broker?")
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
        let tile = &self.tiles[index];
        let mut card = true;
        Modal::new("broker_picker_connect", "")
            .width(400.0)
            .min_height(0.0)
            .show(ctx, &mut card, |ui| {
                ui.horizontal(|ui| {
                    let (icon, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
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
                        "Connect",
                        theme.text.body,
                        theme.ui.panel_bg_floating,
                    ))
                    .fill(theme.ui.text)
                    .corner_radius(6),
                );
                if connect.clicked() {
                    action = Some(BrokerPickerAction::Connect(tile.id.clone()));
                    self.open = false;
                }
                if !tile.description.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(&tile.description)
                            .size(theme.text.small)
                            .color(theme.ui.text_secondary),
                    );
                }
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
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &tile.name));
    let star = Rect::from_center_size(rect.right_top() + vec2(-18.0, 18.0), vec2(20.0, 20.0));
    let star_response = ui
        .interact(star, response.id.with("favorite"), Sense::click())
        .on_hover_text(if tile.favorite {
            "Remove from favourites"
        } else {
            "Add to favourites"
        });
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
            "FEATURED".to_owned(),
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
        &tile.name,
        FontId::proportional(theme.text.body),
        theme.ui.text,
    );
    if let Some(subtitle) = &tile.subtitle {
        let galley = painter.layout(
            subtitle.clone(),
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
            initial,
            FontId::proportional(rect.height() * 0.5),
            theme.ui.panel_bg_floating,
        );
    }
}
