//! Replay progress and status display.
//!
//! Pass host-managed [`ReplayStatus`] to [`ReplayStatusIndicator`] to show playback
//! state, progress and optional execution counts.

use crate::icons::icons;
use crate::localization as l10n;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use crate::ui::model::ReplayProgress;
use egui::{Response, RichText, Sense, Ui, Vec2, Widget};

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
    Paused {
        /// Instrument identifier used by the host.
        symbol: String,
        /// Playback progress in percent.
        progress_pct: f32,
    },
    /// Replay finished
    Finished {
        /// Instrument identifier used by the host.
        symbol: String,
        /// Total number of executions processed by the finished replay.
        total_ticks: u64,
    },
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
        let theme = ui.active_theme();
        let (muted, size) = (theme.ui.text_muted, theme.text.small);
        match self.status {
            // Takes no space when not replaying.
            ReplayStatus::Inactive => ui.allocate_response(Vec2::ZERO, Sense::hover()),
            ReplayStatus::Playing {
                symbol: _,
                progress_pct,
                ticks_processed,
                speed,
            } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing.xs;

                    // Replay indicator - icon + text
                    ui.add(icons::MEDIA_PLAY.as_image_tinted(
                        Vec2::splat(icon_sizes::STATUS_INDICATOR),
                        theme.ui.warning,
                    ));
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), "Replay"))
                            .color(theme.ui.warning)
                            .strong()
                            .size(size),
                    );

                    if self.show_details {
                        // Progress
                        ui.label(
                            RichText::new(format!("{progress_pct:.1}%"))
                                .color(muted)
                                .size(size),
                        );

                        // Speed
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), format!("{speed}x")))
                                .color(muted)
                                .size(size),
                        );

                        // Ticks
                        ui.label(
                            RichText::new(l10n::format_text(
                                ui.ctx(),
                                "{0}K ticks",
                                &[("0", format!("{}", ticks_processed / 1000))],
                            ))
                            .color(muted)
                            .size(size),
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
                    ui.spacing_mut().item_spacing.x = theme.spacing.xs;

                    // Paused indicator - icon + text
                    ui.add(
                        icons::MEDIA_PAUSE
                            .as_image_tinted(Vec2::splat(icon_sizes::STATUS_INDICATOR), muted),
                    );
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), "Paused"))
                            .color(muted)
                            .size(size),
                    );

                    if self.show_details {
                        ui.label(
                            RichText::new(format!("{progress_pct:.1}%"))
                                .color(muted)
                                .size(size),
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
                    ui.spacing_mut().item_spacing.x = theme.spacing.xs;

                    // Complete indicator - icon + text
                    ui.add(icons::SUCCESS.as_image_tinted(
                        Vec2::splat(icon_sizes::STATUS_INDICATOR),
                        theme.ui.success,
                    ));
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), "Complete"))
                            .color(theme.ui.success)
                            .size(size),
                    );

                    if self.show_details {
                        ui.label(
                            RichText::new(l10n::format_text(
                                ui.ctx(),
                                "{0}K ticks",
                                &[("0", format!("{}", total_ticks / 1000))],
                            ))
                            .color(muted)
                            .size(size),
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
