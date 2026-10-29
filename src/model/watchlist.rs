//! Watchlists as plain data: named lists of symbols and sections with colour flags and text
//! import and export. The panel in `ui::widget_bar` shows them and the settings store keeps them.

use egui::Color32;
use serde::{Deserialize, Serialize};

/// A colour flag marking a watchlist symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WatchlistFlag {
    /// Red watchlist flag.
    Red,
    /// Blue watchlist flag.
    Blue,
    /// Green watchlist flag.
    Green,
    /// Orange watchlist flag.
    Orange,
    /// Purple watchlist flag.
    Purple,
    /// Cyan watchlist flag.
    Cyan,
    /// Pink watchlist flag.
    Pink,
}

impl WatchlistFlag {
    /// Every flag, in menu order.
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Blue,
        Self::Green,
        Self::Orange,
        Self::Purple,
        Self::Cyan,
        Self::Pink,
    ];

    /// The flag colour, the same in every theme.
    pub fn color(self) -> Color32 {
        match self {
            Self::Red => Color32::from_rgb(229, 57, 53),
            Self::Blue => Color32::from_rgb(30, 136, 229),
            Self::Green => Color32::from_rgb(67, 160, 71),
            Self::Orange => Color32::from_rgb(251, 140, 0),
            Self::Purple => Color32::from_rgb(142, 36, 170),
            Self::Cyan => Color32::from_rgb(0, 172, 193),
            Self::Pink => Color32::from_rgb(216, 27, 96),
        }
    }
}

/// One row of a watchlist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WatchlistItem {
    /// A symbol, optionally flagged.
    Symbol {
        /// Instrument identifier used by the host.
        symbol: String,
        /// Optional user-assigned color flag.
        flag: Option<WatchlistFlag>,
    },
    /// A section title grouping the symbols below it, up to the next section.
    Section(String),
}

/// A named list of symbols and sections, in the order the user arranged them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watchlist {
    /// Display name.
    pub name: String,
    /// Symbols and section headings in display order.
    pub items: Vec<WatchlistItem>,
}

impl Watchlist {
    /// An empty list.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
        }
    }

    /// A copy named after this list and `taken` names: "Name copy", then "Name copy 2" and so on.
    /// Sections, symbol order and flags carry over.
    pub fn duplicate<'a>(&self, taken: impl IntoIterator<Item = &'a str>) -> Self {
        let taken: Vec<&str> = taken.into_iter().collect();
        let base = format!("{} copy", self.name);
        let name = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base} {n}")))
            .find(|name| !taken.contains(&name.as_str()))
            .expect("an unused name exists");
        Self {
            name,
            items: self.items.clone(),
        }
    }

    /// The symbols of the list, in list order.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|item| match item {
            WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
            WatchlistItem::Section(_) => None,
        })
    }

    /// Appends `symbol`, trimmed. Returns false, leaving the list unchanged, for an empty symbol,
    /// one containing a text separator or starting with `###`, or one the list already holds.
    pub fn add_symbol(&mut self, symbol: &str) -> bool {
        let symbol = symbol.trim();
        if symbol.is_empty()
            || symbol.contains([',', '\n', '\r'])
            || symbol.starts_with("###")
            || self.symbols().any(|s| s == symbol)
        {
            return false;
        }
        self.items.push(WatchlistItem::Symbol {
            symbol: symbol.to_owned(),
            flag: None,
        });
        true
    }

    /// The list as comma-separated text, each section written as `###` and its title, the format
    /// [`from_text`](Self::from_text) reads. Flags are not exported. Commas and line breaks in a
    /// section title become spaces.
    pub fn to_text(&self) -> String {
        let entries: Vec<String> = self
            .items
            .iter()
            .map(|item| match item {
                WatchlistItem::Symbol { symbol, .. } => symbol.clone(),
                WatchlistItem::Section(title) => {
                    format!("###{}", title.replace([',', '\n', '\r'], " "))
                }
            })
            .collect();
        entries.join(",")
    }

    /// A list named `name` from text separated by commas or line breaks. An entry starting with
    /// `###` is a section title; every other entry is a symbol. Entries are trimmed; empty
    /// entries and symbols already listed are skipped.
    pub fn from_text(name: impl Into<String>, text: &str) -> Self {
        let mut list = Self::new(name);
        for entry in text.split([',', '\n', '\r']).map(str::trim) {
            if let Some(title) = entry.strip_prefix("###") {
                list.items
                    .push(WatchlistItem::Section(title.trim().to_owned()));
            } else {
                list.add_symbol(entry);
            }
        }
        list
    }
}
