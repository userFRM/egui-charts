//! The application's side of own data: entering and leaving it, the Data source dialog's place
//! in the frame, and the one question the rest of the demo asks, whether a feature belongs to
//! the synthetic session.

use std::sync::Arc;

use egui_charts::ui::top_toolbar::LayoutAction;
use egui_charts::ui::top_toolbar::components::SymbolSearchDialog;
use egui_charts::widget::SeriesId;

use super::super::DemoApp;
use super::Secrets;
use super::dialog::Outcome;
use super::feed::MappedFeed;
use super::profile::Profile;

/// What the application keeps while the chart shows a visitor's own data.
pub(crate) struct OwnData {
    /// The profile on the chart.
    pub(crate) profile: Profile,
    /// Whether the user paused the feed from the menu. The chart has no getter for it.
    pub(crate) paused: bool,
    /// Whether the synthetic feed was paused when own data was entered. Leaving restores it.
    pub(crate) synthetic_paused: bool,
    /// The feed error shown last, so that a standing error is notified once.
    pub(crate) reported: Option<String>,
    /// True from a symbol or interval change until its first bars arrive. The price-based
    /// chart types are then sized from those bars.
    pub(crate) sizing: bool,
    /// The symbol search of the synthetic session, set aside while own data is on.
    pub(crate) search: SymbolSearchDialog,
}

impl DemoApp {
    /// Draws the Data source dialog and carries out what it decides. Its first call also asks
    /// the dialog what the saved state wants at the start, and every call notifies a new error
    /// of the own-data feed. Called as the last statement of `show_dialogs`.
    pub(crate) fn show_data_source(&mut self, ctx: &egui::Context) {
        if let Some(outcome) = self.data_source.startup() {
            self.carry_out(outcome);
        }
        let running = match self.own_data {
            Some(_) => self.chart.feed_state(SeriesId::MAIN),
            None => None,
        };
        if let Some(outcome) = self.data_source.show(ctx, running) {
            self.carry_out(outcome);
        }
        if let Some(own) = &mut self.own_data {
            let feed = self.chart.feed_state(SeriesId::MAIN);
            let error = feed.and_then(|feed| feed.last_error.as_ref());
            let error = error.map(ToString::to_string);
            // A standing error is notified once, and again when it comes back after a recovery.
            if error != own.reported {
                if let Some(error) = &error {
                    self.toasts.error(error.clone());
                }
                own.reported = error;
            }
        }
    }

    /// Carries out what the Data source dialog decided.
    fn carry_out(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Connect { profile, secrets } => self.connect_own_data(*profile, secrets),
            Outcome::Synthetic => self.leave_own_data(),
            Outcome::Export { file, text } => self.save_text(&file, &text),
        }
    }

    /// Sets the synthetic workspace aside and shows `profile` on one chart, read with
    /// `secrets`. While own data is already on, only the chart is replaced.
    pub(crate) fn connect_own_data(&mut self, profile: Profile, secrets: Secrets) {
        // The dialog connects only a profile without a problem. This keeps the indexing below
        // safe for every other caller.
        if profile.problem().is_some() {
            return;
        }
        if self.own_data.is_none() {
            // What a layout switch does first, so that the synthetic workspace is stored as it
            // is left.
            self.stop_replay();
            if self.top_toolbar.layout_menu.autosave {
                self.handle_layout_action(LayoutAction::Save);
            }
            self.checkpoint_workspace();
            // These show synthetic figures outside the workspace. They are closed, not set
            // aside.
            self.stop_live_strategy();
            self.strategy_report.set_report(None, &[]);
            self.strategy_trade = None;
            self.refresh_strategy_signals();
            self.financials = None;
            self.watchlist_wide = false;
            self.annotation_editor = false;
            self.timeframe_toolbar.state.session_menu_open = false;
            self.loading_bars = false;
        }
        // A reconnect stays on the symbol and interval on show when the profile still offers
        // them.
        let shown = self.own_data.as_ref();
        let symbol = shown.map(|_| self.chart.symbol().to_owned());
        let symbol = symbol.filter(|symbol| profile.symbols.contains(symbol));
        let symbol = symbol.unwrap_or_else(|| profile.symbols[0].clone());
        let timeframe = shown.and_then(|_| self.chart.timeframe());
        let timeframe =
            timeframe.filter(|timeframe| profile.service_interval(*timeframe).is_some());
        let timeframe = timeframe.unwrap_or(profile.intervals[0].chart);
        let intervals = profile.intervals.iter().map(|interval| interval.chart);
        let intervals = intervals.collect();
        let mut chart = egui_charts::ChartBuilder::new()
            .with_feed(Arc::new(MappedFeed::new(profile.clone(), secrets)))
            .with_symbol(symbol)
            .with_timeframe(timeframe)
            .with_theme(self.theme.clone())
            .with_config(self.chart.config().clone())
            .with_chart_options(self.chart.chart_options().clone())
            .with_drawing_registry(self.chart.drawings().registry.clone())
            .with_visible_candles(0)
            .build();
        self.formatting.apply(&mut chart);
        chart.set_event_visibility(self.chart.event_visibility().clone());
        chart.drawings_mut().tool_defaults = self.chart.drawings().tool_defaults.clone();
        // Bars are dated in the profile's zone, so the chart starts in it.
        let zone = profile.time_zone.name();
        super::super::apply_timezone(&mut chart, &mut self.timeframe_toolbar, zone);
        if let Some(own) = &mut self.own_data {
            // Dropping the previous chart ends its subscriptions, which closes its socket.
            self.chart = chart;
            own.profile = profile;
            own.paused = false;
            own.reported = None;
            own.sizing = true;
        } else {
            let synthetic_paused = self.live.paused();
            self.live.set_paused(true);
            self.park_workspace(chart);
            let search = std::mem::replace(&mut self.symbol_search, SymbolSearchDialog::new());
            self.own_data = Some(OwnData {
                profile,
                paused: false,
                synthetic_paused,
                reported: None,
                sizing: true,
                search,
            });
        }
        let selector = &mut self.top_toolbar.timeframe_selector;
        selector.set_supported_timeframes(Some(intervals));
        self.refresh_active_chart();
        let grid = &mut self.top_toolbar.chart_grid_selector;
        grid.set_layout(self.layout.mode);
        grid.sync = self.layout.sync.clone();
    }

    /// Drops the own-data chart, which closes its stream, and brings the synthetic workspace
    /// back as it was left. Does nothing on synthetic data.
    pub(crate) fn leave_own_data(&mut self) {
        let Some(own) = self.own_data.take() else {
            return;
        };
        // Preferences the application saves from the chart on show: they follow it both ways.
        let events = self.chart.event_visibility().clone();
        let tool_defaults = self.chart.drawings().tool_defaults.clone();
        self.symbol_search = own.search;
        let selector = &mut self.top_toolbar.timeframe_selector;
        selector.set_supported_timeframes(None);
        // Drops the own-data chart: its subscriptions end and its socket closes.
        self.resume_workspace();
        self.chart.set_event_visibility(events);
        self.chart.drawings_mut().tool_defaults = tool_defaults;
        self.live.set_paused(own.synthetic_paused);
        let zone = self.chart.chart_options().time_scale.timezone.to_tz();
        let zone = zone.map_or_else(|| "UTC".to_owned(), |zone| zone.name().to_owned());
        self.timeframe_toolbar.set_timezone(zone);
        // The frame then reads the bars, sizes the price-based chart types and annotates,
        // once.
        self.loading_bars = true;
    }

    /// True while own data is on, after telling the user that `feature` works on synthetic
    /// data only. Every entry point of a feature that reads the synthetic session starts with
    /// `if self.synthetic_only("..") { return; }`.
    pub(crate) fn synthetic_only(&mut self, feature: &str) -> bool {
        if self.own_data.is_none() {
            return false;
        }
        self.toasts.info(format!(
            "{feature}: available on synthetic data only. Open Data source in the menu to \
             switch back."
        ));
        true
    }
}

#[cfg(test)]
mod tests {
    use egui_charts::Chart;
    use egui_charts::model::Timeframe;
    use egui_charts::ui::top_toolbar::CompareAction;

    use super::super::super::tests::{AppStorage, settle};
    use super::super::feed::tests::{Server, Shown, pages, profile, shown, until};
    use super::*;

    /// Entering own data shows one chart on the visitor's data and leaves the synthetic session
    /// untouched: stored as it was left, paused, and back exactly as it was when the visitor
    /// leaves. In between, what belongs to the synthetic session refuses.
    #[test]
    fn own_data_sets_the_synthetic_workspace_aside_and_brings_it_back() {
        const J3: &str = r#"{"t":1794407580,"o":100.75,"h":103.0,"l":100.5,"c":102.5,"v":1500}"#;
        const J4: &str = r#"{"t":1794407640,"o":102.5,"h":102.75,"l":101.0,"c":101.25,"v":800}"#;
        const J5: &str = r#"{"t":1794407700,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"v":700}"#;
        const B3: Shown = (1794407580, Some([100.75, 103.0, 100.5, 102.5, 1500.0]));
        const B4: Shown = (1794407640, Some([102.5, 102.75, 101.0, 101.25, 800.0]));
        const B5: Shown = (1794407700, Some([101.25, 101.5, 100.0, 100.25, 700.0]));
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        settle(&mut app, &ctx);
        // A synthetic workspace that is not the default one: two charts, a comparison, a study.
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
        app.chart
            .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
        settle(&mut app, &ctx);
        // The open layout as its document. A layout stamps the drawings of each chart with the
        // time it is captured at, which two captures do not share.
        let layout = |app: &mut DemoApp| {
            let text = app.capture_layout().unwrap().to_json().unwrap();
            let mut document: serde_json::Value = serde_json::from_str(&text).unwrap();
            for chart in document["charts"].as_array_mut().unwrap() {
                let stamps = chart["drawings"]["metadata"].as_object_mut().unwrap();
                stamps.remove("created_at");
                stamps.remove("modified_at");
            }
            document
        };
        let before = layout(&mut app);
        let symbol = app.state.symbol.clone();
        assert!(!app.live.paused());
        // The unnamed workspace as the browser would keep it.
        let workspace = |app: &DemoApp| {
            let mut kept = AppStorage::default();
            app.storage.persist(&mut kept);
            let documents: std::collections::HashMap<String, String> =
                eframe::get_value(&kept, "templates").unwrap();
            documents["workspace"].clone()
        };
        let notified = |app: &DemoApp, start: &str| {
            let mut toasts = app.toasts.toasts().iter();
            toasts.any(|toast| toast.message.starts_with(start))
        };
        let complete = |chart: &Chart| {
            let feed = chart.feed_state(SeriesId::MAIN);
            feed.is_some_and(|feed| feed.history_complete)
        };

        // Entering: one chart on the visitor's data; the synthetic session is set aside and
        // paused.
        let server = Server::start(pages(vec![
            format!("[{J3},{J4},{J5}]"),
            "[]".to_owned(),
            "[]".to_owned(),
            "[]".to_owned(),
        ]));
        let mut own = profile(&server.origin);
        own.symbols.push("XYZ".to_owned());
        app.connect_own_data(own, Secrets::new());
        assert!(app.own_data.is_some() && app.live.paused());
        assert!(app.extra_charts.is_empty() && app.comparisons.is_empty());
        until(&ctx, &mut app.chart, "the visitor's history", complete);
        assert_eq!(shown(&app.chart), [B3, B4, B5]);

        // A second of whole frames later, what is stored is still the synthetic workspace.
        let parked = workspace(&app);
        let later = egui::RawInput {
            time: Some(ctx.input(|input| input.time) + 2.0),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(later, |ui| {
            eframe::App::ui(&mut *app, ui, &mut eframe::Frame::_new_kittest());
        })
        .drop_without_applying_deltas();
        assert_eq!(workspace(&app), parked);
        assert_eq!(app.user_settings().symbol.as_deref(), Some(symbol.as_str()));

        // What belongs to the synthetic session refuses, and says so.
        app.toggle_replay();
        assert!(!app.state.replay_active);
        assert!(notified(&app, "Replay: available on synthetic data only."));
        let tabs = app.layout_tabs.len();
        app.handle_layout_action(LayoutAction::CreateNew);
        assert_eq!(app.layout_tabs.len(), tabs);
        assert!(notified(&app, "Layouts: available on synthetic data only."));
        // A symbol outside the profile is refused. One of the profile is asked of its service.
        app.switch_symbol("AAPL");
        assert_eq!(app.chart.symbol(), "ABC");
        assert!(notified(&app, "AAPL is not a symbol of this data source"));
        app.switch_symbol("XYZ");
        until(&ctx, &mut app.chart, "the other symbol", complete);
        assert_eq!(
            (app.chart.symbol(), app.state.symbol.as_str()),
            ("XYZ", "XYZ")
        );
        app.switch_timeframe(Timeframe::Day1);
        until(&ctx, &mut app.chart, "the other interval", complete);
        let lines: Vec<String> = server.seen().iter().map(|seen| seen.line.clone()).collect();
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!(
            lines[2].starts_with("GET /bars?symbol=XYZ&interval=1m&to="),
            "{lines:?}"
        );
        assert!(
            lines[3].starts_with("GET /bars?symbol=XYZ&interval=1d&to="),
            "{lines:?}"
        );

        // Leaving: the synthetic workspace is back as it was left.
        app.leave_own_data();
        assert!(app.own_data.is_none() && !app.live.paused());
        // The charts read their stored drawings again when they come back, as after a layout
        // switch, and a layout is captured once they have.
        app.chart.flush_stored_drawings();
        for extra in &mut app.extra_charts {
            extra.chart.flush_stored_drawings();
        }
        assert_eq!(layout(&mut app), before);
        assert_eq!((app.extra_charts.len(), app.comparisons.len()), (1, 1));
        assert_eq!(app.state.symbol, symbol);
    }
}
