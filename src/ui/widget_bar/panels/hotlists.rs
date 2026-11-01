//! Hotlists the feed supplies: the market's top movers by the measures the host names.

use crate::localization as l10n;
use std::sync::Arc;

use egui::{Sense, Ui, vec2};

use super::controls::{cell, dropdown, row};
use super::{ROW_HEIGHT, signed};
use super::{format_price, price_decimals};
use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, HotlistEntry, HotlistQuery};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// Symbols asked for at most.
const LIMIT: usize = 50;

/// An action the host performs for the hotlists panel.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum HotlistsAction {
    /// No host action.
    #[default]
    None,
    /// Show this symbol, such as on the chart.
    SelectSymbol(String),
}

/// Hotlists fed by the host's [`Datafeed::hotlist`], one at a time.
///
/// The host names the lists, such as top gainers, top losers and most active; the user picks
/// one, each pick asks the feed, and only the answer to the latest request is shown. Call
/// [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is open.
pub struct HotlistsPanel {
    lists: Vec<String>,
    list: usize,
    feed: Option<Arc<dyn Datafeed>>,
    entries: Vec<HotlistEntry>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
}

impl HotlistsPanel {
    /// A panel of the hotlists named `lists`, showing the first, without a feed.
    pub fn new(lists: Vec<String>) -> Self {
        Self {
            lists,
            list: 0,
            feed: None,
            entries: Vec::new(),
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
        }
    }

    /// Ask `feed` for the lists from now on, starting at once.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        self.refresh();
    }

    /// Ask the feed again, such as to update the figures.
    pub fn refresh(&mut self) {
        let (Some(feed), Some(list)) = (&self.feed, self.lists.get(self.list)) else {
            return;
        };
        self.request += 1;
        self.loading = true;
        let query = HotlistQuery {
            list: list.clone(),
            limit: LIMIT,
        };
        feed.hotlist(query, self.inbox.reply(self.request));
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (key, answer) in self.inbox.drain().answers {
            if key != self.request {
                continue;
            }
            self.loading = false;
            match *answer
                .downcast::<Result<Vec<HotlistEntry>, FeedError>>()
                .expect("hotlist")
            {
                Ok(entries) => {
                    self.entries = entries;
                    self.error = None;
                }
                Err(error) => {
                    self.entries.clear();
                    self.error = Some(error);
                }
            }
        }
    }

    /// Render the chosen list after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> HotlistsAction {
        let mut action = HotlistsAction::None;
        let theme = ui.active_theme();
        PanelHeader::new("Hotlists").show_plain(ui);
        let before = self.list;
        if let Some(name) = self.lists.get(self.list) {
            dropdown(ui, "hotlist", name, |ui| {
                for (index, name) in self.lists.iter().enumerate() {
                    ui.selectable_value(&mut self.list, index, l10n::text(ui.ctx(), name));
                }
            });
        }
        if self.list != before {
            self.refresh();
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no hotlists.");
            }
            Some(error) => {
                status_message(
                    ui,
                    MessageKind::Error,
                    &l10n::format_text(
                        ui.ctx(),
                        "Hotlist unavailable: {error}",
                        &[("error", error.to_string().as_str())],
                    ),
                );
            }
            None if self.loading && self.entries.is_empty() => {
                LoadingIndicator::new().message("Loading hotlist").show(ui);
            }
            None if self.entries.is_empty() => {
                EmptyState::new("No symbols in this list")
                    .icon(&icons::WIDGET_BAR_HOTLISTS)
                    .fill_height()
                    .show(ui);
            }
            None => {}
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let columns = |rect: egui::Rect| {
                    let number = ((rect.width() - 18.0).max(0.0) * 0.3).min(96.0);
                    let right = rect.right() - 8.0;
                    let edges = [
                        rect.left() + 10.0,
                        right - 2.0 * number,
                        right - number,
                        right,
                    ];
                    std::array::from_fn::<_, 3, _>(|i| {
                        egui::Rect::from_min_max(
                            egui::pos2(edges[i], rect.top()),
                            egui::pos2((edges[i + 1] - 4.0).max(edges[i]), rect.bottom()),
                        )
                    })
                };
                let (header, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                for (index, title) in ["Symbol", "Last", "Chg%"].iter().enumerate() {
                    cell(
                        ui,
                        columns(header)[index],
                        title,
                        index != 0,
                        theme.ui.text_secondary,
                    );
                }
                for entry in &self.entries {
                    let color = if entry.change_percent > 0.0 {
                        theme.chart.candle_up
                    } else if entry.change_percent < 0.0 {
                        theme.chart.candle_down
                    } else {
                        theme.ui.text
                    };
                    let (rect, response) = row(ui);
                    let cells = columns(rect);
                    cell(ui, cells[0], &entry.symbol, false, theme.ui.text);
                    cell(
                        ui,
                        cells[1],
                        &format_price(
                            entry.last,
                            Some(
                                entry
                                    .price_decimals
                                    .unwrap_or_else(|| price_decimals(entry.last)),
                            ),
                        ),
                        true,
                        theme.ui.text,
                    );
                    cell(
                        ui,
                        cells[2],
                        &format!("{}%", signed(entry.change_percent, 2)),
                        true,
                        color,
                    );
                    let translated_label = l10n::text(ui.ctx(), &entry.symbol);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                    });
                    if response.clicked() {
                        action = HotlistsAction::SelectSymbol(entry.symbol.clone());
                    }
                }
            });
        action
    }
}
