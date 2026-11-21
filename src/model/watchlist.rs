//! Watchlists as plain data: named lists of symbols and sections with colour flags, quote
//! column widths and text import and export. The panel in `ui::widget_bar` shows them and the
//! settings store keeps them.

use std::collections::{BTreeMap, HashSet};

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
        /// The symbol's colour flag. `WatchlistPanel` keeps it equal in every list.
        flag: Option<WatchlistFlag>,
    },
    /// A section title grouping the symbols below it, up to the next section.
    Section(String),
}

/// The characters that separate the entries of list text.
const SEPARATORS: [char; 5] = [',', ';', '\t', '\n', '\r'];

/// `entry` without white space or byte order marks at either end. Editors write the mark at the
/// start of a file, and it is not white space.
fn trimmed(entry: &str) -> &str {
    entry.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// Whether `symbol`, already trimmed, can be a list symbol: not empty, without separators or
/// other control characters, and not a section title.
fn valid_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && !symbol.contains(SEPARATORS)
        && !symbol.chars().any(char::is_control)
        && !symbol.starts_with("###")
}

/// The entries [`Watchlist::read_text`] left out of a list, besides empty entries and symbols
/// already listed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SkippedEntries {
    /// Entries holding a control character, as a binary file chosen by mistake gives.
    pub invalid: usize,
    /// Symbols and sections after the list reached [`Watchlist::MAX_IMPORTED`] items.
    pub past_limit: usize,
}

/// A named list of symbols and sections, in the order the user arranged them.
///
/// ```
/// use egui_charts::model::{Watchlist, WatchlistColumn};
///
/// let mut list = Watchlist::new("Tech");
/// list.favorite = true;
/// list.show_descriptions = true;
/// list.show_tickers = false;
/// list.columns = Some(vec![WatchlistColumn::Last, WatchlistColumn::Volume]);
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
    /// Whether the rows show each symbol's logo, or its initial on a disc where the host supplies
    /// no logo, before the symbol. On by default, also for a saved list without this setting.
    #[serde(default = "shown")]
    pub show_logos: bool,
    /// Whether rows show tickers. Off, the host's nonblank description takes the ticker's
    /// place, falling back to the ticker. On by default, including in older saves.
    #[serde(default = "shown")]
    pub show_tickers: bool,
    /// Chosen compact quote columns, in [`WatchlistColumn::WIDE`] order after a menu edit.
    /// `None`, including in older saves, uses the panel's columns, initially
    /// [`WatchlistColumn::DEFAULT`]. Unknown column names are ignored when reading.
    #[serde(default, deserialize_with = "known_column_choice")]
    pub columns: Option<Vec<WatchlistColumn>>,
    /// The widths, in points, the user dragged quote columns to. The other columns share the
    /// room left, as they do by default. A column this release does not know is left out when
    /// reading.
    #[serde(default, deserialize_with = "known_columns")]
    pub column_widths: BTreeMap<WatchlistColumn, f32>,
    /// The widths, in points, the user dragged the host's columns to, by column name; see
    /// `WatchlistPanel::add_column`.
    #[serde(default)]
    pub host_column_widths: BTreeMap<String, f32>,
}

/// Logos and tickers stay shown in lists saved before those settings existed.
fn shown() -> bool {
    true
}

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

/// Reads chosen columns without rejecting a list saved by a newer release.
fn known_column_choice<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<WatchlistColumn>>, D::Error> {
    Ok(
        Option::<Vec<Known>>::deserialize(deserializer)?.map(|columns| {
            columns
                .into_iter()
                .filter_map(|Known(column)| column)
                .collect()
        }),
    )
}

/// Reads column widths, leaving out the columns this release does not know, such as one a newer
/// release added, so the rest of the list still reads.
fn known_columns<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<WatchlistColumn, f32>, D::Error> {
    Ok(BTreeMap::<Known, f32>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|(Known(column), width)| Some((column?, width)))
        .collect())
}

impl Watchlist {
    /// The most items, symbols and sections, [`read_text`](Self::read_text) keeps.
    pub const MAX_IMPORTED: usize = 1_000;

    /// An empty list.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            favorite: false,
            show_descriptions: false,
            show_logos: true,
            show_tickers: true,
            columns: None,
            column_widths: BTreeMap::new(),
            host_column_widths: BTreeMap::new(),
        }
    }

    /// A copy named after this list and `taken` names: "Name copy", then "Name copy 2" and so on.
    /// Sections, symbol order, flags, the descriptions, logos, tickers and columns settings and
    /// widths carry over; the copy is not starred.
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
    /// one containing a text separator (comma, semicolon, tab or line break) or another control
    /// character or starting with `###`, or one the list already holds.
    pub fn add_symbol(&mut self, symbol: &str) -> bool {
        let symbol = symbol.trim();
        if !valid_symbol(symbol) || self.symbols().any(|s| s == symbol) {
            return false;
        }
        self.items.push(WatchlistItem::Symbol {
            symbol: symbol.to_owned(),
            flag: None,
        });
        true
    }

    /// The list as comma-separated text, each section written as `###` and its title, the format
    /// [`from_text`](Self::from_text) reads. Flags are not exported. Separators and other control
    /// characters in a section title become spaces.
    pub fn to_text(&self) -> String {
        let entries: Vec<String> = self
            .items
            .iter()
            .map(|item| match item {
                WatchlistItem::Symbol { symbol, .. } => symbol.clone(),
                WatchlistItem::Section(title) => format!(
                    "###{}",
                    title.replace(|c: char| SEPARATORS.contains(&c) || c.is_control(), " ")
                ),
            })
            .collect();
        entries.join(",")
    }

    /// A list named `name` from text separated by commas, semicolons, tabs or line breaks, and
    /// what it left out. An entry starting with `###` is a section title; every other entry is a
    /// symbol. Entries are trimmed of white space and byte order marks; empty entries and symbols
    /// already listed are skipped without being counted. Entries with a control character are
    /// left out, and so is every symbol or section after the list holds
    /// [`MAX_IMPORTED`](Self::MAX_IMPORTED) items.
    pub fn read_text(name: impl Into<String>, text: &str) -> (Self, SkippedEntries) {
        let mut list = Self::new(name);
        let mut skipped = SkippedEntries::default();
        let mut listed: HashSet<&str> = HashSet::new();
        for entry in text.split(SEPARATORS).map(trimmed) {
            if entry.is_empty() {
                continue;
            }
            if entry.chars().any(char::is_control) {
                skipped.invalid += 1;
                continue;
            }
            let item = match entry.strip_prefix("###") {
                Some(title) => WatchlistItem::Section(trimmed(title).to_owned()),
                None if listed.contains(entry) => continue,
                None => WatchlistItem::Symbol {
                    symbol: entry.to_owned(),
                    flag: None,
                },
            };
            if list.items.len() == Self::MAX_IMPORTED {
                skipped.past_limit += 1;
                continue;
            }
            if matches!(item, WatchlistItem::Symbol { .. }) {
                listed.insert(entry);
            }
            list.items.push(item);
        }
        (list, skipped)
    }

    /// The list [`read_text`](Self::read_text) reads from `text`, without what it left out.
    pub fn from_text(name: impl Into<String>, text: &str) -> Self {
        Self::read_text(name, text).0
    }
}
