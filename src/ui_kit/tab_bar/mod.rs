//! Tab bar component with underline indicator.
//!
//! Provides a horizontal tab bar that renders clickable text labels with
//! an accent-colored underline on the active tab. Used across all tabbed dialogs.
//!
//! # Example
//! ```ignore
//! use crate::ui_kit::tab_bar::{TabBar, TabLabel};
//!
//! impl TabLabel for MyTab {
//!     fn tab_label(&self) -> &str { self.display_name() }
//! }
//!
//! TabBar::new(MyTab::all(), &mut self.active_tab).show(ui);
//! ```

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Ui};

/// Trait for tab enum types to provide their display label.
///
/// Implement this on your tab enum so `TabBar` can render it.
pub trait TabLabel: Copy + PartialEq {
    fn tab_label(&self) -> &str;
}

/// Horizontal tab bar with underline indicator on the active tab.
///
/// Renders clickable text labels separated by standard spacing.
/// The active tab gets an accent-colored underline drawn via the painter.
pub struct TabBar<'a, T: TabLabel> {
    tabs: &'a [T],
    active: &'a mut T,
}

impl<'a, T: TabLabel> TabBar<'a, T> {
    pub fn new(tabs: &'a [T], active: &'a mut T) -> Self {
        Self { tabs, active }
    }

    /// Show the tab bar. Returns `true` if the active tab changed.
    pub fn show(self, ui: &mut Ui) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.spacing.lg);
            for tab in self.tabs {
                let is_active = *tab == *self.active;
                let text_color = if is_active {
                    ui.style().visuals.selection.bg_fill
                } else {
                    ui.style().visuals.text_color()
                };

                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(tab.tab_label())
                            .color(text_color)
                            .size(typography::MD),
                    )
                    .sense(egui::Sense::click()),
                );

                if response.clicked() {
                    *self.active = *tab;
                    changed = true;
                }

                // Draw underline for active tab
                if is_active {
                    let rect = response.rect;
                    ui.painter().line_segment(
                        [
                            Pos2::new(rect.min.x, rect.max.y + 2.0),
                            Pos2::new(rect.max.x, rect.max.y + 2.0),
                        ],
                        egui::Stroke::new(
                            DESIGN_TOKENS.stroke.thick,
                            ui.style().visuals.selection.bg_fill,
                        ),
                    );
                }

                ui.add_space(DESIGN_TOKENS.spacing.lg);
            }
        });

        changed
    }
}
