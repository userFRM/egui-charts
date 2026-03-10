//! Replay Status Indicator
//!
//! Displays replay status when tick file replay is active.
//! Development mode indicator.

use crate::icons::icons;
use crate::styles::icons as icon_sizes;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::ReplayProgress;
use egui::{Response, RichText, Ui, Vec2, Widget};

/// Replay status for the indicator
#[derive(Clone, Debug, Default)]
pub enum ReplayStatus {
    /// Not replaying (using live data or sample)
    #[default]
    Inactive,
    /// Actively replaying tick file
    Playing {
        /// Symbol being replayed
        symbol: String,
        /// Progress (0.0 - 100.0)
        progress_pct: f32,
        /// Ticks processed
        ticks_processed: u64,
        /// Replay speed multiplier
        speed: f32,
    },
    /// Replay paused
    Paused { symbol: String, progress_pct: f32 },
    /// Replay finished
    Finished { symbol: String, total_ticks: u64 },
}

impl From<(&ReplayProgress, &str, f32)> for ReplayStatus {
    fn from((progress, symbol, speed): (&ReplayProgress, &str, f32)) -> Self {
        if progress.progress_pct >= 99.9 {
            ReplayStatus::Finished {
                symbol: symbol.to_string(),
                total_ticks: progress.ticks_processed,
            }
        } else {
            ReplayStatus::Playing {
                symbol: symbol.to_string(),
                progress_pct: progress.progress_pct,
                ticks_processed: progress.ticks_processed,
                speed,
            }
        }
    }
}

/// Replay status indicator widget
pub struct ReplayStatusIndicator<'a> {
    /// Current replay status
    status: &'a ReplayStatus,
    /// Whether to show detailed info
    show_details: bool,
}

impl<'a> ReplayStatusIndicator<'a> {
    /// Create a new replay status indicator
    pub fn new(status: &'a ReplayStatus) -> Self {
        Self {
            status,
            show_details: false,
        }
    }

    /// Show detailed info (progress, speed)
    #[must_use]
    pub fn with_details(mut self) -> Self {
        self.show_details = true;
        self
    }
}

impl<'a> Widget for ReplayStatusIndicator<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        match self.status {
            ReplayStatus::Inactive => {
                // Show nothing when not replaying
                ui.label("")
            }
            ReplayStatus::Playing {
                symbol: _,
                progress_pct,
                ticks_processed,
                speed,
            } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.xs;

                    // Replay indicator - icon + text
                    ui.add(icons::MEDIA_PLAY.as_image_tinted(
                        Vec2::splat(icon_sizes::STATUS_INDICATOR),
                        DESIGN_TOKENS.semantic.status.caution,
                    ));
                    ui.label(
                        RichText::new("REPLAY")
                            .color(DESIGN_TOKENS.semantic.status.caution)
                            .strong()
                            .size(11.0),
                    );

                    if self.show_details {
                        // Progress
                        ui.label(
                            RichText::new(format!("{:.1}%", progress_pct))
                                .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                                .size(11.0),
                        );

                        // Speed
                        ui.label(
                            RichText::new(format!("{}x", speed))
                                .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                                .size(11.0),
                        );

                        // Ticks
                        ui.label(
                            RichText::new(format!("{}K ticks", ticks_processed / 1000))
                                .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                                .size(10.0),
                        );
                    }
                })
                .response
            }
            ReplayStatus::Paused {
                symbol: _,
                progress_pct,
            } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.xs;

                    // Paused indicator - icon + text
                    ui.add(icons::MEDIA_PAUSE.as_image_tinted(
                        Vec2::splat(icon_sizes::STATUS_INDICATOR),
                        DESIGN_TOKENS.semantic.ui.text_muted_dark,
                    ));
                    ui.label(
                        RichText::new("PAUSED")
                            .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                            .size(11.0),
                    );

                    if self.show_details {
                        ui.label(
                            RichText::new(format!("{:.1}%", progress_pct))
                                .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                                .size(11.0),
                        );
                    }
                })
                .response
            }
            ReplayStatus::Finished {
                symbol: _,
                total_ticks,
            } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.xs;

                    // Complete indicator - icon + text
                    ui.add(icons::SETTINGS_STATUS_LINE.as_image_tinted(
                        Vec2::splat(icon_sizes::STATUS_INDICATOR),
                        DESIGN_TOKENS.semantic.extended.bullish,
                    ));
                    ui.label(
                        RichText::new("COMPLETE")
                            .color(DESIGN_TOKENS.semantic.extended.bullish)
                            .size(11.0),
                    );

                    if self.show_details {
                        ui.label(
                            RichText::new(format!("{}K ticks", total_ticks / 1000))
                                .color(DESIGN_TOKENS.semantic.ui.text_muted_dark)
                                .size(10.0),
                        );
                    }
                })
                .response
            }
        }
    }
}

/// Show a replay status indicator in the UI
pub fn show_replay_status(ui: &mut Ui, status: &ReplayStatus) -> Response {
    ReplayStatusIndicator::new(status).ui(ui)
}

/// Show a replay status indicator with details
pub fn show_replay_status_with_details(ui: &mut Ui, status: &ReplayStatus) -> Response {
    ReplayStatusIndicator::new(status).with_details().ui(ui)
}
