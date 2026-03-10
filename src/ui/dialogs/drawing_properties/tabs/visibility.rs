//! Visibility tab - timeframe visibility settings.

use egui::Ui;

use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::drawing_properties::actions::DrawingProps;
use crate::ui_kit::FormGrid;

/// Available timeframes
const TIMEFRAMES: &[&str] = &["1m", "5m", "15m", "30m", "1h", "4h", "1D", "1W", "1M"];

/// Visibility tab renderer
pub struct VisibilityTab;

impl VisibilityTab {
    pub fn show(ui: &mut Ui, props: &mut DrawingProps) {
        ui.space_md();

        ui.checkbox(&mut props.visible_all_timeframes, "Show on all timeframes");

        if !props.visible_all_timeframes {
            ui.space_lg();
            ui.label("Select timeframes:");
            ui.space_sm();

            FormGrid::new("timeframes_grid")
                .columns(3)
                .spacing(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.sm)
                .show(ui, |ui| {
                    for (i, tf) in TIMEFRAMES.iter().enumerate() {
                        let mut is_visible = props.visible_timeframes.contains(&tf.to_string());
                        if ui.checkbox(&mut is_visible, *tf).changed() {
                            if is_visible {
                                if !props.visible_timeframes.contains(&tf.to_string()) {
                                    props.visible_timeframes.push(tf.to_string());
                                }
                            } else {
                                props.visible_timeframes.retain(|t| t != *tf);
                            }
                        }

                        if (i + 1) % 3 == 0 {
                            ui.end_row();
                        }
                    }
                });
        }
    }
}
