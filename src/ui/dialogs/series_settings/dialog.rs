//! Series settings dialog with sidebar navigation.

use crate::chart::series::{SeriesId, SeriesSettings};
use egui::{Context, Ui, Vec2};

use super::actions::SeriesSettingsAction;
use super::config::SeriesSettingsConfig;
use super::tabs::{
    AlertsTab, CanvasTab, EventsTab, ScalesTab, SeriesSettingsTab, StatusLineTab, SymbolTab,
    TradingTab,
};
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{DialogFooter, DialogFrame, dialog_header};
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};

/// Series settings dialog with sidebar layout
pub struct SeriesSettingsDialog {
    pub config: SeriesSettingsConfig,
    pub is_open: bool,
    pub series_id: Option<SeriesId>,
    pub active_tab: SeriesSettingsTab,
    pub settings: SeriesSettings,
    pub original_settings: SeriesSettings,
}

impl Default for SeriesSettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SeriesSettingsDialog {
    pub fn new() -> Self {
        Self {
            config: SeriesSettingsConfig::default(),
            is_open: false,
            series_id: None,
            active_tab: SeriesSettingsTab::default(),
            settings: SeriesSettings::default(),
            original_settings: SeriesSettings::default(),
        }
    }

    pub fn open(&mut self, id: SeriesId, settings: SeriesSettings) {
        self.is_open = true;
        self.series_id = Some(id);
        self.settings = settings.clone();
        self.original_settings = settings;
        self.active_tab = SeriesSettingsTab::default();
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.series_id = None;
    }

    /// Sync dialog open/close state from an external flag.
    ///
    /// Opening requires series context, so only close is synced here.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open && self.is_open {
            self.close();
        }
    }

    pub fn show(&mut self, ctx: &Context) -> SeriesSettingsAction {
        if !self.is_open {
            return SeriesSettingsAction::None;
        }

        let mut action = SeriesSettingsAction::None;

        DialogFrame::new("Settings", Vec2::new(self.config.width, self.config.height)).show(
            ctx,
            |ui| {
                action = self.render_contents(ui);
            },
        );

        action
    }

    fn render_contents(&mut self, ui: &mut Ui) -> SeriesSettingsAction {
        let mut action = SeriesSettingsAction::None;

        // Title bar
        if dialog_header(ui, "Settings") {
            action = SeriesSettingsAction::Cancel;
            self.is_open = false;
        }

        // Main content: sidebar + content area
        let sidebar_width = DESIGN_TOKENS.sizing.dialog.series_sidebar_width;
        let active_tab = self.active_tab;
        let active_tab_mut = &mut self.active_tab;
        let settings = &mut self.settings;

        SidebarLayout::new(sidebar_width).show(
            ui,
            |ui| {
                ui.space_sm();
                for tab in SeriesSettingsTab::all() {
                    if SidebarTab::new(tab.label(), *active_tab_mut == tab)
                        .icon(tab.icon())
                        .show(ui)
                    {
                        *active_tab_mut = tab;
                    }
                }
            },
            |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.space_md();
                    match active_tab {
                        SeriesSettingsTab::Symbol => SymbolTab::show(ui, settings),
                        SeriesSettingsTab::StatusLine => StatusLineTab::show(ui, settings),
                        SeriesSettingsTab::ScalesAndLines => ScalesTab::show(ui, settings),
                        SeriesSettingsTab::Canvas => CanvasTab::show(ui, settings),
                        SeriesSettingsTab::Trading => TradingTab::show(ui, settings),
                        SeriesSettingsTab::Alerts => AlertsTab::show(ui, settings),
                        SeriesSettingsTab::Events => EventsTab::show(ui, settings),
                    }
                });
            },
        );

        // Footer
        let (ok, cancel) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .show_with_left(ui, |ui| {
                // Template dropdown (placeholder)
                egui::ComboBox::from_id_salt("template")
                    .selected_text("Template")
                    .width(DESIGN_TOKENS.sizing.dialog.input_width)
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(false, "Default");
                        let _ = ui.selectable_label(false, "Save as...");
                    });
            });
        if ok && let Some(id) = self.series_id {
            action = SeriesSettingsAction::Apply(id, self.settings.clone());
            self.is_open = false;
        }
        if cancel {
            action = SeriesSettingsAction::Cancel;
            self.is_open = false;
        }

        action
    }
}
