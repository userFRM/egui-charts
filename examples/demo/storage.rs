//! The demo's storage service: every document in one map that eframe keeps, in a file natively
//! and in the browser's local storage on the web, written out as soon as a document changes.

use egui_charts::data::Reply;
use egui_charts::services::{Storage, StoreKind};
use egui_charts::ui_kit::notifications::Toast;
use egui_charts::widget::SavedLayout;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The eframe key of the map, where earlier releases of the demo kept templates.
const KEY: &str = "templates";
/// The key of the document that holds the unnamed workspace between sessions.
const WORKSPACE: &str = "workspace";
/// The key of the document that holds the own-data choice, the profiles and the kept keys.
const OWN_DATA: &str = "own-data";

/// Progress of the unnamed workspace checkpoints.
#[derive(Default)]
pub(super) struct WorkspaceRecovery {
    /// The time of the last checkpoint, in seconds of egui time.
    checked_at: Option<f64>,
    /// Set when the stored workspace could not be opened; it is then kept as it is.
    unreadable: bool,
    /// The last checkpoint failure, so a repeated one is reported once.
    save_error: Option<String>,
}

/// The unnamed workspace: a saved layout with what the layout document does not hold.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedWorkspace {
    version: u32,
    #[serde(flatten)]
    layout: SavedLayout,
    /// The comparisons and spreads of each chart.
    comparisons: Vec<Vec<super::comparison::SavedComparison>>,
    #[serde(default)]
    groups: Option<super::replay::LayoutGroups>,
    /// The slot that was active.
    active_slot: usize,
    /// Whether each chart's feed serves the continuous 24/7 session.
    continuous_sessions: Vec<bool>,
}

impl super::DemoApp {
    /// Whether a layout of that name is in storage.
    pub(super) fn layout_is_saved(&self, name: &str) -> bool {
        self.storage
            .documents
            .lock()
            .unwrap()
            .contains_key(&key(StoreKind::Layouts, name))
    }

    /// Names the opening tab and shows the stored unnamed workspace in it; a document that cannot be
    /// read stays in storage and a default chart opens with a notice.
    pub(super) fn restore_workspace(&mut self) {
        let mut name = "Unnamed".to_owned();
        let mut suffix = 2;
        while self.layout_is_saved(&name) {
            name = format!("Unnamed {suffix}");
            suffix += 1;
        }
        self.layout_tabs[self.layout_tab].name = name.clone();
        self.refresh_layout_menu();
        let json = self
            .storage
            .documents
            .lock()
            .unwrap()
            .get(WORKSPACE)
            .cloned();
        let Some(json) = json else { return };
        let saved = (|| {
            let mut saved: SavedWorkspace =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            saved.layout = SavedLayout::from_json(&json).map_err(|e| e.to_string())?;
            if saved.active_slot >= saved.layout.charts.len() {
                return Err("invalid active chart slot".to_owned());
            }
            if saved.continuous_sessions.len() != saved.layout.charts.len() {
                return Err("invalid session count".to_owned());
            }
            if saved.comparisons.len() != saved.layout.charts.len() {
                return Err("invalid comparison chart count".to_owned());
            }
            Ok(saved)
        })();
        match saved {
            Ok(saved) => {
                if self.restore_layout(name, saved.layout, Ok(saved.comparisons)) {
                    self.restore_layout_groups(saved.groups);
                    let restore_view = |chart: &mut egui_charts::widget::Chart,
                                        live: &super::LiveFeed,
                                        continuous| {
                        let session = if continuous {
                            super::SessionType::Full24x7
                        } else if chart.extended_hours() {
                            super::SessionType::ETH
                        } else {
                            super::SessionType::RTH
                        };
                        live.set_view(session, chart.dividend_adjusted());
                        chart.reload_feed();
                        chart.set_data(
                            egui_charts::widget::SeriesId::MAIN,
                            live.snapshot(
                                chart
                                    .timeframe()
                                    .unwrap_or(egui_charts::model::Timeframe::Min1),
                                chart.bar_clock(),
                                chart.extended_hours(),
                            ),
                        );
                    };
                    let mut continuous = saved.continuous_sessions;
                    restore_view(
                        &mut self.chart,
                        &self.live,
                        continuous.remove(self.layout_slot),
                    );
                    for (extra, continuous) in self.extra_charts.iter_mut().zip(continuous) {
                        restore_view(&mut extra.chart, &extra.live, continuous);
                    }
                    self.activate_chart_slot(saved.active_slot);
                    self.bars = self
                        .chart
                        .bars(egui_charts::widget::SeriesId::MAIN)
                        .to_vec();
                    self.timeframe_toolbar
                        .set_session(self.live.view(self.chart.extended_hours()).0);
                    self.timeframe_toolbar.state.adjust_for_dividends =
                        self.chart.dividend_adjusted();
                    let zone = self
                        .chart
                        .chart_options()
                        .time_scale
                        .timezone
                        .to_tz()
                        .map_or_else(|| "UTC".to_owned(), |zone| zone.name().to_owned());
                    self.timeframe_toolbar.set_timezone(zone);
                } else {
                    self.workspace_recovery.unreadable = true;
                }
            }
            Err(error) => {
                self.workspace_recovery.unreadable = true;
                self.toasts
                    .error(format!("Could not open unnamed workspace: {error}"));
            }
        }
        if self.workspace_recovery.unreadable {
            self.toasts.add(Toast::warning("Opened a default chart. The unreadable workspace was kept; save a named layout to keep new work.").permanent());
        }
    }

    /// Stores the unnamed workspace when it changed. A saved layout owns its tab, and an unreadable
    /// document is never replaced.
    pub(super) fn checkpoint_workspace(&mut self) {
        if self.own_data.is_some()
            || self.workspace_recovery.unreadable
            || self.layout_is_saved(&self.layout_tabs[self.layout_tab].name)
        {
            return;
        }
        let document = (|| {
            let layout = self.capture_layout()?;
            let mut continuous_sessions: Vec<_> = self
                .extra_charts
                .iter()
                .map(|extra| {
                    extra.live.view(extra.chart.extended_hours()).0 == super::SessionType::Full24x7
                })
                .collect();
            continuous_sessions.insert(
                self.layout_slot,
                self.live.view(self.chart.extended_hours()).0 == super::SessionType::Full24x7,
            );
            continuous_sessions.truncate(layout.charts.len());
            let saved = SavedWorkspace {
                version: SavedLayout::VERSION,
                layout,
                comparisons: self.capture_comparisons()?,
                groups: Some(self.layout_groups()),
                active_slot: self.layout_slot,
                continuous_sessions,
            };
            serde_json::to_string(&saved).map_err(|e| e.to_string())
        })();
        match document {
            Ok(json) => {
                let mut documents = self.storage.documents.lock().unwrap();
                if documents.get(WORKSPACE) != Some(&json) {
                    documents.insert(WORKSPACE.into(), json);
                    self.storage.changed.store(true, Ordering::Relaxed);
                }
                self.workspace_recovery.save_error = None;
            }
            Err(error) => {
                if self.workspace_recovery.save_error.as_ref() != Some(&error) {
                    self.toasts
                        .error(format!("Could not save unnamed workspace: {error}"));
                }
                self.workspace_recovery.save_error = Some(error);
            }
        }
    }

    /// Checkpoints the workspace once a second.
    pub(super) fn poll_workspace(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        if self
            .workspace_recovery
            .checked_at
            .is_none_or(|last| now - last >= 1.0)
        {
            self.checkpoint_workspace();
            self.workspace_recovery.checked_at = Some(now);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

#[derive(Default)]
pub(super) struct DemoStorage {
    documents: Mutex<HashMap<String, String>>,
    /// Whether a document changed since the map was last written out.
    changed: AtomicBool,
}

impl DemoStorage {
    /// The documents eframe kept, with the drawings earlier releases kept under their own key.
    pub fn restore(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut documents: HashMap<String, String> = storage
            .and_then(|storage| eframe::get_value(storage, KEY))
            .unwrap_or_default();
        let drawings: HashMap<String, String> = storage
            .and_then(|storage| eframe::get_value(storage, "drawings"))
            .unwrap_or_default();
        for (symbol, json) in drawings {
            documents
                .entry(key(StoreKind::Drawings, &symbol))
                .or_insert(json);
        }
        Self {
            documents: Mutex::new(documents),
            changed: AtomicBool::new(false),
        }
    }

    /// Puts the documents in eframe's storage, which writes them out on exit and periodically.
    pub fn persist(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, KEY, &*self.documents.lock().unwrap());
        storage.remove_string("drawings");
    }

    /// Writes the documents out now if one changed, so that a save is kept before the next
    /// periodic write.
    pub fn flush(&self, storage: Option<&mut (dyn eframe::Storage + 'static)>) {
        if let Some(storage) = storage
            && self.changed.swap(false, Ordering::Relaxed)
        {
            self.persist(storage);
            storage.flush();
        }
    }

    pub(super) fn save_comparisons(
        &self,
        name: &str,
        comparisons: &[Vec<super::comparison::SavedComparison>],
    ) -> Result<(), String> {
        let document = serde_json::to_string(comparisons).map_err(|error| error.to_string())?;
        self.documents
            .lock()
            .unwrap()
            .insert(format!("layout-comparisons/{name}"), document);
        self.changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub(super) fn comparisons(
        &self,
        name: &str,
    ) -> Result<Vec<Vec<super::comparison::SavedComparison>>, String> {
        self.documents
            .lock()
            .unwrap()
            .get(&format!("layout-comparisons/{name}"))
            .map_or_else(
                || Ok(Vec::new()),
                |json| {
                    serde_json::from_str(json)
                        .map_err(|error| format!("Could not open comparisons: {error}"))
                },
            )
    }

    pub(super) fn save_sample_study(&self, title: &str, text: &str) {
        self.documents
            .lock()
            .unwrap()
            .insert(format!("sample-studies/{title}"), text.to_owned());
        self.changed.store(true, Ordering::Relaxed);
    }

    pub(super) fn sample_study(&self, title: &str) -> Option<String> {
        self.documents
            .lock()
            .unwrap()
            .get(&format!("sample-studies/{title}"))
            .cloned()
    }

    pub(super) fn sample_studies(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .documents
            .lock()
            .unwrap()
            .keys()
            .filter_map(|key| key.strip_prefix("sample-studies/").map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    /// The own-data document, when one is stored.
    pub(super) fn own_data(&self) -> Option<String> {
        self.documents.lock().unwrap().get(OWN_DATA).cloned()
    }

    /// Stores the own-data document.
    pub(super) fn save_own_data(&self, document: &str) {
        self.documents
            .lock()
            .unwrap()
            .insert(OWN_DATA.to_owned(), document.to_owned());
        self.changed.store(true, Ordering::Relaxed);
    }

    pub(super) fn save_layout_groups(
        &self,
        name: &str,
        groups: &super::replay::LayoutGroups,
    ) -> Result<(), String> {
        let document = serde_json::to_string(groups).map_err(|error| error.to_string())?;
        self.documents
            .lock()
            .unwrap()
            .insert(format!("layout-groups/{name}"), document);
        self.changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub(super) fn layout_groups(
        &self,
        name: &str,
    ) -> Result<Option<super::replay::LayoutGroups>, String> {
        self.documents
            .lock()
            .unwrap()
            .get(&format!("layout-groups/{name}"))
            .map(|json| {
                serde_json::from_str(json)
                    .map_err(|error| format!("Could not open chart groups: {error}"))
            })
            .transpose()
    }

    fn write(&self, kind: StoreKind, name: &str, document: Option<String>) {
        let mut documents = self.documents.lock().unwrap();
        match document {
            Some(document) => documents.insert(key(kind, name), document),
            None => documents.remove(&key(kind, name)),
        };
        self.changed.store(true, Ordering::Relaxed);
    }
}

fn key(kind: StoreKind, name: &str) -> String {
    format!("{kind:?}/{name}")
}

impl Storage for DemoStorage {
    fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>) {
        let prefix = key(kind, "");
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(str::to_owned))
            .collect()));
    }
    fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .get(&key(kind, name))
            .cloned()));
    }
    fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
        self.write(kind, name, Some(document));
        reply.send(Ok(()));
    }
    fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>) {
        self.write(kind, name, None);
        if kind == StoreKind::Layouts {
            self.documents
                .lock()
                .unwrap()
                .remove(&format!("layout-comparisons/{name}"));
            self.documents
                .lock()
                .unwrap()
                .remove(&format!("layout-groups/{name}"));
        }
        reply.send(Ok(()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::DemoApp;
    use eframe::App;
    use egui_charts::ui::dialogs::StudyContextMenuAction;
    use egui_charts::widget::SeriesId;

    #[derive(Default)]
    struct AppStorage(HashMap<String, String>);

    impl eframe::Storage for AppStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    fn restart(ctx: &egui::Context, storage: &AppStorage) -> Box<DemoApp> {
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(storage);
        let mut app = DemoApp::new_at(&cc, "2026-10-27T19:29:50Z".parse().unwrap(), true);
        show_charts(&mut app, ctx);
        app
    }

    fn show_charts(app: &mut DemoApp, ctx: &egui::Context) {
        for _ in 0..3 {
            ctx.run_ui(Default::default(), |ui| {
                app.chart.show(ui);
                for extra in &mut app.extra_charts {
                    extra.chart.show(ui);
                }
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn study_favorites_survive_restart_and_removal_with_legacy_settings() {
        let ctx = egui::Context::default();
        let mut storage = AppStorage::default();
        eframe::set_value(
            &mut storage,
            KEY,
            &HashMap::from([("Settings/settings", r#"{"version":1,"interval":"Min5"}"#)]),
        );
        let mut app = restart(&ctx, &storage);
        assert!(app.indicator_dialog.favorites.is_empty());
        assert_eq!(app.state.timeframe, egui_charts::model::Timeframe::Min5);
        app.indicator_dialog.favorites.insert("RSI".into());
        app.handle_study_context(
            &ctx,
            SeriesId::MAIN,
            StudyContextMenuAction::AddToFavorites("SMA".into()),
        );
        // Keep stars for host studies that are temporarily absent from the catalogue.
        app.indicator_dialog.favorites.insert("Host study".into());
        for expected in [["RSI", "SMA"].as_slice(), ["SMA"].as_slice()] {
            app.save(&mut storage);
            app = restart(&ctx, &storage);
            ctx.run_ui(Default::default(), |_| app.show_templates(&ctx))
                .drop_without_applying_deltas();
            let mut favorites: Vec<_> = app
                .top_toolbar
                .favorite_indicators
                .iter()
                .map(|(id, _)| id.as_str())
                .collect();
            favorites.sort_unstable();
            assert_eq!(favorites, expected);
            assert!(app.indicator_dialog.favorites.contains("Host study"));
            app.indicator_dialog.favorites.remove("RSI");
        }
    }

    #[test]
    fn unnamed_workspace_recovers_chart_settings_and_keeps_named_save_policy() {
        use egui_charts::model::ChartType;
        use egui_charts::scales::PriceScaleMode;
        use egui_charts::ui::timeframe_toolbar::SessionType;
        use egui_charts::ui::top_toolbar::{CompareAction, TopToolbarAction};
        use egui_charts::widget::ChartLayoutMode;
        for (mode, session) in [
            (ChartLayoutMode::Single, SessionType::Full24x7),
            (ChartLayoutMode::TwoHorizontal, SessionType::RTH),
        ] {
            let ctx = egui::Context::default();
            let mut storage = AppStorage::default();
            let mut app = restart(&ctx, &storage);
            app.ensure_chart_slots(mode);
            app.activate_chart_slot(mode.chart_count() - 1);
            app.timeframe_toolbar.set_session(session);
            app.apply_session();
            show_charts(&mut app, &ctx);
            app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Line));
            app.chart
                .add_indicator(Box::new(egui_charts::studies::SMA::new(7)));
            let rsi = app
                .chart
                .add_indicator(Box::new(egui_charts::studies::RSI::new(5)));
            let pane = app.chart.series_pane(rsi).unwrap();
            app.chart.move_pane(pane, 0);
            app.chart.set_pane_stretch_factor(pane, 2.25);
            app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
            app.chart.price_scale_mut().options_mut().mode = PriceScaleMode::Logarithmic;
            app.chart.price_scale_mut().reset();
            show_charts(&mut app, &ctx);
            app.chart.price_scale_mut().set_range(2.0, 6.0);
            app.chart
                .set_crosshair_style(egui_charts::config::CrosshairStyle::Dot);
            let mut settings =
                egui_charts::widget::settings::ChartSettingsState::from_chart(&app.chart);
            settings.timezone = "+05:45".into();
            app.apply_chart_settings(settings);
            app.layout.set_sync_group(0, Some("Recovered".into()));
            app.layout.set_detached(mode.chart_count() - 1, true);
            app.layout.sync.sync_symbol = false;
            app.layout.sync.sync_drawings = true;
            let saved_groups = app.layout_groups();
            let studies = |app: &DemoApp| {
                app.chart
                    .indicators()
                    .map(|(_, study)| {
                        (
                            study.name().to_owned(),
                            study
                                .inputs()
                                .iter()
                                .map(|input| input.value.clone())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let saved_studies = studies(&app);
            app.save(&mut storage);
            app = restart(&ctx, &storage);
            assert_eq!(app.layout.mode, mode);
            assert_eq!(app.layout_slot, mode.chart_count() - 1);
            assert!(app.layout_groups() == saved_groups);
            assert_eq!(app.chart.chart_type(), ChartType::Line);
            assert_eq!(app.timeframe_toolbar.state.session_type, session);
            assert_eq!(app.timeframe_toolbar.state.timezone, "+05:45");
            assert_eq!(
                app.chart.state().price_scale().options().mode,
                PriceScaleMode::Logarithmic
            );
            assert_eq!(app.chart.state().price_scale().range().min, 2.0);
            assert_eq!(app.chart.state().price_scale().range().max, 6.0);
            assert_eq!(
                app.chart.chart_options().crosshair.style,
                egui_charts::config::CrosshairStyle::Dot
            );
            assert_eq!(studies(&app), saved_studies);
            let rsi = app
                .chart
                .indicators()
                .find(|(_, study)| study.name() == "RSI")
                .unwrap()
                .0;
            let pane = app.chart.series_pane(rsi).unwrap();
            assert_eq!(app.chart.panes()[0], pane);
            assert_eq!(app.chart.pane_stretch_factor(pane), Some(2.25));
            assert_eq!(app.comparisons.len(), 1);
            assert!(!app.chart.bars(app.comparisons[0].1).is_empty());
            assert!(app.top_toolbar.layout_menu.layouts.is_empty());

            // Recovery is also checkpointed during frames, before eframe's periodic save.
            app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Area));
            ctx.run_ui(
                egui::RawInput {
                    time: Some(2.0),
                    ..Default::default()
                },
                |ui| {
                    app.ui(ui, &mut eframe::Frame::_new_kittest());
                },
            )
            .drop_without_applying_deltas();
            app.storage.persist(&mut storage);
            app = restart(&ctx, &storage);
            assert_eq!(app.chart.chart_type(), ChartType::Area);
            assert!(app.layout_groups() == saved_groups);

            app.handle_top_toolbar(&ctx, TopToolbarAction::Save);
            app.poll_storage(&ctx);
            app.save(&mut storage);
            for autosave in [false, true] {
                app = restart(&ctx, &storage);
                app.poll_storage(&ctx);
                app.activate_chart_slot(mode.chart_count() - 1);
                assert_eq!(app.chart.chart_type(), ChartType::Area);
                app.top_toolbar.layout_menu.autosave = autosave;
                app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Line));
                for _ in 0..3 {
                    ctx.run_ui(
                        egui::RawInput {
                            time: Some(ctx.input(|input| input.time) + 2.0),
                            ..Default::default()
                        },
                        |ui| {
                            app.ui(ui, &mut eframe::Frame::_new_kittest());
                        },
                    )
                    .drop_without_applying_deltas();
                }
                app.save(&mut storage);
                app = restart(&ctx, &storage);
                app.poll_storage(&ctx);
                app.activate_chart_slot(mode.chart_count() - 1);
                assert_eq!(
                    app.chart.chart_type(),
                    if autosave {
                        ChartType::Line
                    } else {
                        ChartType::Area
                    }
                );
            }

            show_charts(&mut app, &ctx);
            let layout = app.capture_layout().unwrap();
            app.top_toolbar.layout_menu.save("Unnamed 2", &layout);
            app.poll_storage(&ctx);
            app.handle_layout_action(
                egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction::CreateNew,
            );
            assert!(
                !app.top_toolbar
                    .layout_menu
                    .layouts
                    .contains(&app.layout_tabs[app.layout_tab].name)
            );
            app.handle_top_toolbar(&ctx, TopToolbarAction::ChartStyleSelected(ChartType::Area));
            for _ in 0..2 {
                show_charts(&mut app, &ctx);
                app.save(&mut storage);
                let documents: HashMap<String, String> = eframe::get_value(&storage, KEY).unwrap();
                assert_eq!(
                    egui_charts::services::UserSettings::from_json(&documents["Settings/settings"])
                        .unwrap()
                        .layout,
                    None,
                    "current tab: {}, menu: {:?}",
                    app.layout_tabs[app.layout_tab].name,
                    app.top_toolbar.layout_menu.curr_layout,
                );
                assert_eq!(
                    SavedLayout::from_json(&documents["workspace"])
                        .unwrap()
                        .charts[0]
                        .chart_type,
                    ChartType::Area,
                    "checkpoint notices: {:?}",
                    app.toasts
                        .toasts()
                        .iter()
                        .map(|toast| &toast.message)
                        .collect::<Vec<_>>(),
                );
                app = restart(&ctx, &storage);
                app.poll_storage(&ctx);
                assert_eq!(app.chart.chart_type(), ChartType::Area);
                assert!(
                    !app.top_toolbar
                        .layout_menu
                        .layouts
                        .contains(&app.layout_tabs[app.layout_tab].name)
                );
            }
        }
    }

    #[test]
    fn workspace_without_groups_restores_with_default_synchronization() {
        let ctx = egui::Context::default();
        let mut storage = AppStorage::default();
        let mut app = restart(&ctx, &storage);
        app.chart
            .set_chart_type(egui_charts::model::ChartType::Area);
        app.save(&mut storage);
        let mut documents: HashMap<String, String> = eframe::get_value(&storage, KEY).unwrap();
        let mut workspace: serde_json::Value = serde_json::from_str(&documents[WORKSPACE]).unwrap();
        workspace.as_object_mut().unwrap().remove("groups");
        documents.insert(WORKSPACE.into(), workspace.to_string());
        eframe::set_value(&mut storage, KEY, &documents);

        app = restart(&ctx, &storage);
        assert_eq!(app.chart.chart_type(), egui_charts::model::ChartType::Area);
        assert_eq!(app.layout.sync_group(0), None);
        assert!(!app.layout.is_detached(0));
        assert!(app.layout.sync == Default::default());
    }

    /// The first-visit choice is on screen at the first start, each way of answering it is
    /// remembered across a restart, an application without storage is not asked, and own data
    /// that was connected is on again at the next start.
    #[test]
    fn the_first_visit_choice_is_asked_once() {
        use super::super::owndata::feed::tests::{Server, pages, profile};
        use super::super::owndata::store::Saved;
        use super::super::tests::{ui_click, ui_frame};
        const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);
        /// Three whole frames of the application, and the texts of the last one.
        fn frames(app: &mut DemoApp, ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ui_frame(ctx, SIZE, vec![], |ui| {
                    app.ui(ui, &mut eframe::Frame::_new_kittest());
                });
            }
            texts
        }
        fn offered(texts: &[(String, egui::Rect)]) -> bool {
            texts.iter().any(|(text, _)| text == "Connect my own data")
        }

        for answer in ["Use synthetic data", "Connect my own data", "Escape"] {
            let ctx = egui::Context::default();
            let mut storage = AppStorage::default();
            let mut app = restart(&ctx, &storage);
            let texts = frames(&mut app, &ctx);
            assert!(offered(&texts), "{answer}");
            if answer == "Escape" {
                let escape = egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                };
                ui_frame(&ctx, SIZE, vec![escape], |ui| {
                    app.ui(ui, &mut eframe::Frame::_new_kittest());
                });
            } else {
                let button = texts.iter().find(|(text, _)| text == answer);
                let at = button.unwrap().1.center();
                ui_click(&ctx, SIZE, at, |ui| {
                    app.ui(ui, &mut eframe::Frame::_new_kittest());
                });
            }
            app.save(&mut storage);
            let mut app = restart(&ctx, &storage);
            assert!(!offered(&frames(&mut app, &ctx)), "{answer}");
            assert!(app.own_data.is_none(), "{answer}");
        }

        // Without storage the answer could not be remembered, so the choice is not offered.
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        assert!(!offered(&frames(&mut app, &ctx)));

        // Own data that was connected is on again at the next start, without a dialog.
        let server = Server::start(pages(vec!["[]".to_owned()]));
        let ctx = egui::Context::default();
        let mut storage = AppStorage::default();
        let mut app = restart(&ctx, &storage);
        let connected = Saved {
            asked: true,
            own: true,
            current: Some("Local".to_owned()),
            profiles: vec![profile(&server.origin)],
            ..Saved::default()
        };
        connected.save(&app.storage);
        app.save(&mut storage);
        let mut app = restart(&ctx, &storage);
        assert!(!offered(&frames(&mut app, &ctx)));
        let on_chart = app.own_data.as_ref().map(|own| own.profile.name.as_str());
        assert_eq!(on_chart, Some("Local"));
        assert_eq!(app.chart.symbol(), "ABC");
    }

    #[test]
    fn unreadable_workspace_opens_a_default_chart_without_overwriting_the_document() {
        let ctx = egui::Context::default();
        let mut app = restart(&ctx, &AppStorage::default());
        let mut valid: serde_json::Value =
            serde_json::from_str(&app.capture_layout().unwrap().to_json().unwrap()).unwrap();
        valid["active_slot"] = 0.into();
        valid["comparisons"] = serde_json::json!([[]]);
        valid["continuous_sessions"] = serde_json::json!([true]);
        let valid = valid.to_string();
        let invalid_slot = valid.replace("\"active_slot\":0", "\"active_slot\":99");
        let invalid_comparisons = valid.replace("\"comparisons\":[[]]", "\"comparisons\":[]");
        let invalid_chart = valid.replace("\"charts\":[", "\"charts\":[{\"symbol\":false},");
        let unsupported = valid.replace("\"version\":3", "\"version\":999");
        for json in [
            "{",
            invalid_slot.as_str(),
            invalid_comparisons.as_str(),
            invalid_chart.as_str(),
            unsupported.as_str(),
        ] {
            let mut storage = AppStorage::default();
            eframe::set_value(&mut storage, KEY, &HashMap::from([("workspace", json)]));
            let mut app = restart(&ctx, &storage);
            assert_eq!(
                app.chart.chart_type(),
                egui_charts::model::ChartType::Candles
            );
            assert!(
                app.toasts
                    .toasts()
                    .iter()
                    .any(|toast| toast.message.contains("Could not open"))
            );
            assert!(
                app.toasts
                    .toasts()
                    .iter()
                    .any(|toast| toast.message.contains("default chart"))
            );
            app.chart
                .set_chart_type(egui_charts::model::ChartType::Line);
            app.save(&mut storage);
            let kept: HashMap<String, String> = eframe::get_value(&storage, KEY).unwrap();
            assert_eq!(kept["workspace"], json);
        }
    }
}
