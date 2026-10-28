//! The host's services and outputs, and their wiring into the chart slots.

use super::Terminal;
use crate::services::{
    Alerts, Broker, Chats, Marks, Notifications, Quotes, Signals, Storage, SymbolSearch,
};
use crate::ui::export::ExportFormat;
use crate::ui::trading::BrokerTile;
use std::path::PathBuf;
use std::sync::Arc;

/// A finished export, for the host to save. The library writes no files.
#[derive(Clone, Debug)]
pub struct ExportFile {
    /// Suggested file name, with its extension.
    pub name: String,
    /// The destination entered in the export dialog, if any.
    pub path: Option<PathBuf>,
    /// What to write.
    pub contents: ExportContents,
}

/// The contents of an [`ExportFile`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ExportContents {
    /// An encoded PNG image.
    Png(Vec<u8>),
    /// Text, such as CSV.
    Text(String),
}

type Play = Box<dyn FnMut(&str, f32)>;

/// What the host supplied. Each service is handed to the components that consume it.
#[derive(Default)]
pub(super) struct Services {
    pub(super) symbol_search: Option<Arc<dyn SymbolSearch>>,
    pub(super) storage: Option<Arc<dyn Storage>>,
    pub(super) alerts: Option<Arc<dyn Alerts>>,
    pub(super) brokers: Vec<(BrokerTile, Arc<dyn Broker>)>,
    pub(super) signals: Vec<Arc<dyn Signals>>,
    pub(super) marks: Option<Arc<dyn Marks>>,
    pub(super) quotes: Option<Arc<dyn Quotes>>,
    pub(super) notifications: Option<Arc<dyn Notifications>>,
    pub(super) chats: Option<Arc<dyn Chats>>,
    pub(super) export_formats: &'static [ExportFormat],
    pub(super) save: Option<Box<dyn FnMut(ExportFile)>>,
    pub(super) sounds: Vec<String>,
    pub(super) play: Option<Play>,
}

impl Terminal {
    /// Connects chart slot `index` to the services the chart consumes and to the theme.
    pub(super) fn wire_slot(&mut self, index: usize) {
        let ctx = self.ctx();
        let services = &self.services;
        let chart = &mut self.charts[index];
        chart.config_mut().skip_background = true;
        if let Some(storage) = &services.storage {
            chart.set_storage(storage.clone());
        }
        chart.set_mark_source(services.marks.clone());
        chart.set_signal_sources(services.signals.clone());
        chart.set_theme(&ctx, &self.theme);
    }
}
