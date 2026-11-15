//! Watchlists as plain data: named lists of symbols and sections with colour flags, quote
//! column widths and text import and export. The panel in `ui::widget_bar` shows them and the
//! settings store keeps them.

use std::collections::BTreeMap;

use egui::Color32;
use serde::de::{self, IntoDeserializer, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// A colour flag marking a watchlist symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
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

/// A sortable watchlist column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WatchlistColumn {
    /// Sort by symbol.
    Symbol,
    /// Sort by last price.
    Last,
    /// Sort by price change.
    Change,
    /// Sort by percentage change.
    ChangePercent,
    /// Sort by the session's volume.
    Volume,
    /// Sort by the last price of the extended-hours session.
    ExtendedLast,
    /// Sort by the extended-hours price change from the regular last price.
    ExtendedChange,
    /// Sort by the extended-hours percentage change from the regular last price.
    ExtendedChangePercent,
}

impl WatchlistColumn {
    /// The columns after the symbol that the panel shows by default.
    pub const DEFAULT: [Self; 3] = [Self::Last, Self::Change, Self::ChangePercent];

    /// The columns after the symbol in the wide view: all of them.
    pub const WIDE: [Self; 7] = [
        Self::Last,
        Self::Change,
        Self::ChangePercent,
        Self::Volume,
        Self::ExtendedLast,
        Self::ExtendedChange,
        Self::ExtendedChangePercent,
    ];
}

/// One row of a watchlist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
///
/// ```
/// use egui_charts::model::{Watchlist, WatchlistColumn};
///
/// let mut list = Watchlist::new("Tech");
/// list.favorite = true;
/// list.show_descriptions = true;
/// list.column_widths.insert(WatchlistColumn::Last, 72.0);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Watchlist {
    /// Display name.
    pub name: String,
    /// Symbols and section headings in display order.
    pub items: Vec<WatchlistItem>,
    /// Whether the user starred the list, which the list menu then shows first.
    #[serde(default)]
    pub favorite: bool,
    /// Whether the rows show each symbol's description under it, where the host supplies one.
    #[serde(default)]
    pub show_descriptions: bool,
    /// The widths, in points, the user dragged quote columns to. The other columns share the
    /// room left, as they do by default. A column this release does not know is left out when
    /// reading.
    #[serde(default, deserialize_with = "known_columns")]
    pub column_widths: BTreeMap<WatchlistColumn, f32>,
}

/// Reads column widths, leaving out the columns this release does not know, such as one a newer
/// release added, so the rest of the list still reads.
fn known_columns<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<WatchlistColumn, f32>, D::Error> {
    /// A column read by name, `None` for a name this release does not know.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Known(Option<WatchlistColumn>);
    impl<'de> Deserialize<'de> for Known {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Name;
            impl Visitor<'_> for Name {
                type Value = Known;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a watchlist column")
                }
                fn visit_str<E: de::Error>(self, name: &str) -> Result<Known, E> {
                    let column: Result<WatchlistColumn, E> =
                        WatchlistColumn::deserialize(name.into_deserializer());
                    Ok(Known(column.ok()))
                }
            }
            // Read as an identifier: a JSON key reads as one, and so does a variant name in
            // other formats.
            deserializer.deserialize_identifier(Name)
        }
    }
    Ok(BTreeMap::<Known, f32>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|(Known(column), width)| Some((column?, width)))
        .collect())
}

impl Watchlist {
    /// An empty list.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            favorite: false,
            show_descriptions: false,
            column_widths: BTreeMap::new(),
        }
    }

    /// A copy named after this list and `taken` names: "Name copy", then "Name copy 2" and so on.
    /// Sections, symbol order, flags, descriptions and column widths carry over; the copy is not
    /// starred.
    pub fn duplicate<'a>(&self, taken: impl IntoIterator<Item = &'a str>) -> Self {
        let taken: Vec<&str> = taken.into_iter().collect();
        let base = format!("{} copy", self.name);
        let name = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base} {n}")))
            .find(|name| !taken.contains(&name.as_str()))
            .expect("an unused name exists");
        Self {
            name,
            favorite: false,
            ..self.clone()
        }
    }

    /// The symbols of the list, in list order.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|item| match item {
            WatchlistItem::Symbol { symbol, .. } => Some(symbol.as_str()),
            WatchlistItem::Section(_) => None,
        })
    }

    /// Moves the section at item `index` with its symbols above the previous section, or below
    /// the next one with `down`. The symbols before the first section stay first. Returns false,
    /// leaving the list unchanged, when `index` is not a section or there is no section to pass.
    pub fn move_section(&mut self, index: usize, down: bool) -> bool {
        let starts: Vec<usize> = (0..self.items.len())
            .filter(|&i| matches!(self.items[i], WatchlistItem::Section(_)))
            .collect();
        let Some(at) = starts.iter().position(|&start| start == index) else {
            return false;
        };
        // The earlier of the two sections that swap places.
        let first = match (down, at) {
            (true, _) => at,
            (false, 0) => return false,
            (false, _) => at - 1,
        };
        let (Some(&start), Some(&middle)) = (starts.get(first), starts.get(first + 1)) else {
            return false;
        };
        let end = starts.get(first + 2).copied().unwrap_or(self.items.len());
        self.items[start..end].rotate_left(middle - start);
        true
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
