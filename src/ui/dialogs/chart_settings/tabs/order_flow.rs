//! Independent heatmap, live book and trade bubble controls.

use super::super::SettingsDialog;
use super::{checkbox, styled_checkbox, styled_dropdown};
use crate::chart::overlays::{BookProfile, BubbleColor, BubbleGrouping, BubbleScalingMode};
use crate::localization as l10n;
use crate::theme::Theme;
use egui::{RichText, Ui};

impl SettingsDialog {
    pub(in super::super) fn draw_order_flow_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        let [heatmap, book, orders, bubbles] = self.order_flow_layers;
        if heatmap {
            styled_checkbox(
                ui,
                &mut self.settings.order_flow.show_heatmap,
                "Market depth heatmap",
                theme.ui.text,
            );
            ui.add_enabled_ui(self.settings.order_flow.show_heatmap, |ui| {
                self.draw_setting_row_with_controls(ui, "Brightness", theme, |ui, dialog| {
                    ui.add(
                        egui::DragValue::new(&mut dialog.settings.order_flow.heatmap.brightness)
                            .speed(0.01)
                            .custom_formatter(|value, _| (value as f32).to_string())
                            .clamp_existing_to_range(false)
                            .range(0.0..=f32::MAX),
                    );
                });
                self.draw_setting_row_with_controls(ui, "Lower cutoff", theme, |ui, dialog| {
                    ui.add(
                        egui::DragValue::new(&mut dialog.settings.order_flow.heatmap.lower_cutoff)
                            .speed(0.1)
                            .custom_formatter(|value, _| value.to_string())
                            .clamp_existing_to_range(false)
                            .range(0.0..=f64::MAX),
                    );
                });
                self.draw_setting_row_with_controls(
                    ui,
                    "Full-intensity size",
                    theme,
                    |ui, dialog| {
                        let size = &mut dialog.settings.order_flow.heatmap.full_intensity_size;
                        let mut automatic = size.is_none();
                        if checkbox(
                            ui,
                            &mut automatic,
                            RichText::new(l10n::text(ui.ctx(), "Automatic"))
                                .size(theme.text.title)
                                .color(theme.ui.text),
                        )
                        .changed()
                        {
                            *size = if automatic { None } else { Some(1.0) };
                        }
                        if let Some(size) = size {
                            ui.add(
                                egui::DragValue::new(size)
                                    .speed(0.1)
                                    .custom_formatter(|value, _| value.to_string())
                                    .clamp_existing_to_range(false)
                                    .range(f64::MIN_POSITIVE..=f64::MAX),
                            );
                        }
                    },
                );
            });
        }
        if book {
            styled_checkbox(
                ui,
                &mut self.settings.order_flow.show_order_book,
                "Order book",
                theme.ui.text,
            );
            ui.add_enabled_ui(self.settings.order_flow.show_order_book, |ui| {
                self.draw_setting_row_with_controls(ui, "Size measure", theme, |ui, dialog| {
                    let profile = &mut dialog.settings.order_flow.order_book.profile;
                    let name = match profile {
                        BookProfile::Total => "Total size",
                        BookProfile::LargestOrder => "Largest order",
                    };
                    styled_dropdown(ui, "book_measure", 180.0, theme.ui.text, name, |ui| {
                        ui.selectable_value(
                            profile,
                            BookProfile::Total,
                            l10n::text(ui.ctx(), "Total size"),
                        );
                        if orders {
                            ui.selectable_value(
                                profile,
                                BookProfile::LargestOrder,
                                l10n::text(ui.ctx(), "Largest order"),
                            );
                        }
                    });
                    if !orders && *profile == BookProfile::LargestOrder {
                        ui.label(l10n::text(ui.ctx(), "Order-level depth unavailable"));
                    }
                });
            });
        }
        if bubbles {
            styled_checkbox(
                ui,
                &mut self.settings.order_flow.show_volume_bubbles,
                "Volume bubbles",
                theme.ui.text,
            );
            ui.add_enabled_ui(self.settings.order_flow.show_volume_bubbles, |ui| {
                self.draw_setting_row_with_controls(ui, "Colour mode", theme, |ui, dialog| {
                    let mode = &mut dialog.settings.order_flow.volume_bubbles.color_by;
                    let choices = [
                        (BubbleColor::Side, "Aggressor side"),
                        (BubbleColor::Delta, "Delta"),
                        (BubbleColor::Split, "Split bid/ask"),
                        (BubbleColor::AbsoluteDelta, "Absolute delta"),
                        (BubbleColor::DeltaPercent, "Delta percent"),
                        (BubbleColor::Volume, "Volume"),
                    ];
                    let selected = choices.iter().find(|(value, _)| value == mode).unwrap().1;
                    styled_dropdown(ui, "bubble_colour", 180.0, theme.ui.text, selected, |ui| {
                        for (value, label) in choices {
                            ui.selectable_value(mode, value, l10n::text(ui.ctx(), label));
                        }
                    });
                });
                self.draw_setting_row_with_controls(ui, "Grouping", theme, |ui, dialog| {
                    let grouping = &mut dialog.settings.order_flow.volume_bubbles.grouping;
                    let index = match grouping {
                        BubbleGrouping::Automatic => 0,
                        BubbleGrouping::Ungrouped => 1,
                        BubbleGrouping::TimeWindow(_) => 2,
                        BubbleGrouping::PriceStep(_) => 3,
                        BubbleGrouping::Executions(_) => 4,
                    };
                    let choices = [
                        (BubbleGrouping::Automatic, "Automatic"),
                        (BubbleGrouping::Ungrouped, "Ungrouped"),
                        (
                            BubbleGrouping::TimeWindow(chrono::TimeDelta::seconds(1)),
                            "Time window",
                        ),
                        (BubbleGrouping::PriceStep(1.0), "Price step"),
                        (BubbleGrouping::Executions(10), "Executions"),
                    ];
                    styled_dropdown(
                        ui,
                        "bubble_grouping",
                        180.0,
                        theme.ui.text,
                        choices[index].1,
                        |ui| {
                            for (i, (value, label)) in choices.iter().enumerate() {
                                if ui
                                    .selectable_label(i == index, l10n::text(ui.ctx(), *label))
                                    .clicked()
                                    && i != index
                                {
                                    *grouping = *value;
                                }
                            }
                        },
                    );
                });
                match self.settings.order_flow.volume_bubbles.grouping {
                    BubbleGrouping::TimeWindow(_) => {
                        self.draw_setting_row_with_controls(
                            ui,
                            "Window (ms)",
                            theme,
                            |ui, dialog| {
                                let grouping =
                                    &mut dialog.settings.order_flow.volume_bubbles.grouping;
                                if let BubbleGrouping::TimeWindow(window) = grouping {
                                    let mut millis = window.num_milliseconds();
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut millis)
                                                .clamp_existing_to_range(false)
                                                .range(1..=i64::MAX),
                                        )
                                        .changed()
                                    {
                                        *window = chrono::TimeDelta::milliseconds(millis);
                                    }
                                }
                            },
                        );
                    }
                    BubbleGrouping::PriceStep(_) => {
                        self.draw_setting_row_with_controls(
                            ui,
                            "Price step",
                            theme,
                            |ui, dialog| {
                                if let BubbleGrouping::PriceStep(step) =
                                    &mut dialog.settings.order_flow.volume_bubbles.grouping
                                {
                                    ui.add(
                                        egui::DragValue::new(step)
                                            .speed(0.01)
                                            .custom_formatter(|value, _| value.to_string())
                                            .clamp_existing_to_range(false)
                                            .range(f64::MIN_POSITIVE..=f64::MAX),
                                    );
                                }
                            },
                        );
                    }
                    BubbleGrouping::Executions(_) => {
                        self.draw_setting_row_with_controls(
                            ui,
                            "Execution count",
                            theme,
                            |ui, dialog| {
                                if let BubbleGrouping::Executions(count) =
                                    &mut dialog.settings.order_flow.volume_bubbles.grouping
                                {
                                    ui.add(
                                        egui::DragValue::new(count)
                                            .clamp_existing_to_range(false)
                                            .range(1..=usize::MAX),
                                    );
                                }
                            },
                        );
                    }
                    _ => {}
                }
                self.draw_setting_row_with_controls(ui, "Size scaling", theme, |ui, dialog| {
                    let mode = &mut dialog.settings.order_flow.volume_bubbles.scaling_mode;
                    let choices = [
                        (BubbleScalingMode::Linear, "Linear"),
                        (BubbleScalingMode::SquareRoot, "Square root"),
                        (BubbleScalingMode::Logarithmic, "Logarithmic"),
                    ];
                    let selected = choices.iter().find(|(value, _)| value == mode).unwrap().1;
                    styled_dropdown(ui, "bubble_scaling", 180.0, theme.ui.text, selected, |ui| {
                        for (value, label) in choices {
                            ui.selectable_value(mode, value, l10n::text(ui.ctx(), label));
                        }
                    });
                });
                self.draw_setting_row_with_controls(ui, "Minimum size", theme, |ui, dialog| {
                    let filter = &mut dialog.settings.order_flow.volume_bubbles.min_size_filter;
                    let mut size = filter.unwrap_or(0.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut size)
                                .speed(0.1)
                                .custom_formatter(|value, _| value.to_string())
                                .clamp_existing_to_range(false)
                                .range(0.0..=f64::MAX),
                        )
                        .changed()
                    {
                        *filter = (size > 0.0).then_some(size);
                    }
                });
            });
        }
    }
}
