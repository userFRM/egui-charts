//! # Terminal
//!
//! The assembled terminal over the synthetic feed of the `live_feed` example: toolbars, menus
//! and the chart, performing their actions by default. Exports are written to the temporary
//! directory.
//!
//! Run with:
//! ```sh
//! cargo run --example terminal
//! ```

#[path = "live_feed.rs"]
#[allow(dead_code, reason = "only the feed of the example is used")]
mod live_feed;

use egui_charts::model::{Instrument, Timeframe};
use egui_charts::ui::export::ExportFormat;
use egui_charts::ui::terminal::{ExportContents, Terminal};
use std::sync::Arc;

fn main() -> eframe::Result {
    let symbols = ["SYNTH", "AAPL", "EURUSD", "BTCUSD"]
        .map(Instrument::new)
        .to_vec();
    let mut terminal = Terminal::new(Arc::new(live_feed::SyntheticFeed::new(20_000)))
        .with_symbol("SYNTH")
        .with_timeframe(Timeframe::Min1)
        .with_symbol_search(Arc::new(symbols))
        .with_file_export(&[ExportFormat::Png, ExportFormat::Csv], |file| {
            let path = file
                .path
                .unwrap_or_else(|| std::env::temp_dir().join(&file.name));
            let written = match file.contents {
                ExportContents::Png(bytes) => std::fs::write(&path, bytes),
                ExportContents::Text(text) => std::fs::write(&path, text),
                _ => return,
            };
            if let Err(error) = written {
                eprintln!("Could not save {}: {error}", path.display());
            }
        });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };
    eframe::run_ui_native("egui-charts: terminal", options, move |ui, _frame| {
        // Actions the terminal leaves to its host: host entries and those without a default.
        for action in terminal.show(ui) {
            let _ = action;
        }
    })
}
