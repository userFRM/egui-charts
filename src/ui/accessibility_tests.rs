//! Accessible names, roles and states at the control boundary.

use crate::icons::icons;
use crate::theme::ThemeExt;
use egui::{Role, Ui, Vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[test]
fn custom_controls_report_names_roles_and_states() {
    type Control = (Role, &'static str, fn(&mut Ui, bool));
    let controls: &[Control] = &[
        (Role::TextInput, "Name:", |ui, _| {
            use super::top_toolbar::components::indicator_dialog::{
                IndicatorTemplatePanel, IndicatorTemplateState,
            };
            let mut state = IndicatorTemplateState {
                show_save_form: true,
                ..Default::default()
            };
            IndicatorTemplatePanel::new(&mut state, "SMA").show(ui);
        }),
        (Role::TextInput, "Interval value", |ui, _| {
            super::top_toolbar::components::timeframe_selector::CustomTimeframeDialog::default()
                .show(ui);
        }),
        (Role::TextInput, "Search indicators", |ui, _| {
            crate::ui_kit::filter::FilterWidget::new()
                .placeholder("Search indicators")
                .show(ui);
        }),
        (Role::Button, "Close", |ui, _| {
            crate::ui_kit::buttons::dialog_close_button(ui);
        }),
        (Role::Button, "Stay in drawing mode", |ui, selected| {
            crate::ui_kit::buttons::Button::new("Stay in drawing mode")
                .selected(selected)
                .show(ui);
        }),
        (Role::Button, "Favorite", |ui, selected| {
            crate::ui_kit::buttons::IconButton::new(&icons::STAR_EMPTY)
                .tooltip("Favorite")
                .selected(selected)
                .show(ui);
        }),
        (Role::Button, "Line tools", |ui, selected| {
            crate::ui_kit::buttons::SplitButton::new(&icons::TREND_LINE)
                .tooltip("Line tools")
                .selected(selected)
                .expanded(!selected)
                .show(ui);
        }),
        (Role::MenuItem, "Download image", |ui, _| {
            ui.scope_builder(
                egui::UiBuilder::new().ui_stack_info(egui::UiStackInfo::new(egui::UiKind::Menu)),
                |ui| {
                    crate::ui_kit::list_item::ListItem::new().show_text(ui, "Download image");
                },
            );
        }),
        (Role::Button, "Horizontal Line", |ui, selected| {
            let mut toolbar = super::drawing_toolbar::DrawingToolbar::new();
            toolbar.state.favorites = vec![crate::drawings::DrawingToolType::HorizontalLine];
            toolbar.state.sel_tool =
                selected.then_some(crate::drawings::DrawingToolType::HorizontalLine);
            toolbar.show_floating(ui, egui::pos2(20.0, 20.0));
        }),
        (Role::ComboBox, "Theme", |ui, _| {
            crate::ui_kit::form::FormRow::new("Theme").show(ui, |ui| {
                egui::ComboBox::from_id_salt("theme")
                    .selected_text("Dark")
                    .show_ui(ui, |ui| {
                        ui.label("Dark");
                    });
            });
        }),
        (Role::Button, "Trend line", |ui, selected| {
            super::drawing_toolbar::PairButton {
                icon: &icons::TREND_LINE,
                tooltip: "Trend line",
                arrow_tooltip: "More lines",
                is_icon_active: selected,
                is_expanded: selected,
            }
            .show_styled(ui);
        }),
        (Role::Button, "Trend line", |ui, selected| {
            super::drawing_toolbar::PairButton {
                icon: &icons::TREND_LINE,
                tooltip: "Trend line",
                arrow_tooltip: "More lines",
                is_icon_active: selected,
                is_expanded: selected,
            }
            .show(ui, 48.0, 48.0, 28.0, 10.0);
        }),
        (Role::Button, "Magnet", |ui, selected| {
            super::drawing_toolbar::components::draw_icon_btn(
                ui,
                &icons::MAGNET,
                "Magnet",
                selected,
                28.0,
            );
        }),
        (Role::Button, "Magnet", |ui, selected| {
            super::drawing_toolbar::components::draw_icon_button_padded(
                ui,
                &icons::MAGNET,
                "Magnet",
                selected,
                28.0,
                4.0,
            );
        }),
        (Role::Tab, "Inputs", |ui, selected| {
            crate::ui_kit::sidebar_layout::SidebarTab::new("Inputs", selected).show(ui);
        }),
        (Role::Tab, "Alerts", |ui, selected| {
            let mut tabs = super::widget_bar::RightPanelTabs::new();
            if !selected {
                tabs.active_tab = super::widget_bar::RightPanelTab::ObjectTree;
            }
            tabs.show_tab_bar(ui);
        }),
        (Role::Button, "1M", |ui, selected| {
            let mut ranges = super::timeframe_toolbar::RangeSwitcher::new();
            ranges.set_sel(if selected {
                super::timeframe_toolbar::DateRange::Month1
            } else {
                super::timeframe_toolbar::DateRange::Day1
            });
            ranges.show(ui);
        }),
        (Role::Button, "Go to date", |ui, _| {
            super::timeframe_toolbar::TimeframeBar::new().show(ui);
        }),
        (Role::Button, "Chart Sync Options", |ui, _| {
            super::top_toolbar::components::SyncButton::new().show(ui);
        }),
        (Role::Button, "Chart settings", |ui, _| {
            super::top_toolbar::components::settings_menu::SettingsMenu::new()
                .show_btn(ui, ui.max_rect());
        }),
        (Role::Tab, "Profiles", |ui, selected| {
            use super::top_toolbar::components::indicator_dialog::IndicatorTab;
            let mut active = if selected {
                IndicatorTab::Profiles
            } else {
                IndicatorTab::Indicators
            };
            crate::ui_kit::tab_bar::TabBar::new(IndicatorTab::all(), &mut active).show(ui);
        }),
        (Role::MenuItem, "Remove drawings", |ui, _| {
            super::drawing_toolbar::SubmenuBuilder::new(ui)
                .add_text_item("Remove drawings", "Remove drawings")
                .show();
        }),
        (Role::MenuItem, "Reset view", |ui, _| {
            use super::context_menu::{MenuEntry, MenuItem, show_menu};
            let mut item = MenuItem::new("Reset view", ());
            item.enabled = ui.is_enabled();
            show_menu(
                ui.ctx(),
                ui.scope_id().with("test_menu"),
                &crate::theme::Theme::dark(),
                egui::pos2(20.0, 20.0),
                0.0,
                &[MenuEntry::Item(item)],
                true,
            );
        }),
        (Role::Button, "Save", |ui, _| {
            super::top_toolbar::buttons::TextButton::new("Save".into(), &Default::default())
                .show(ui);
        }),
        (Role::ListBoxOption, "Example", |ui, selected| {
            crate::ui_kit::list_item::ListItem::new()
                .selected(selected)
                .show(ui, crate::ui_kit::list_item::LabelContent::new("Example"));
        }),
        (Role::Label, "Price: 12.50", |ui, _| {
            crate::ui_kit::list_item::ListItem::new().show_property(ui, "Price", "12.50");
        }),
        (Role::Label, "Export successful!", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.success_label("Export successful!");
        }),
        (Role::Label, "Unavailable", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.error_label("Unavailable");
        }),
        (Role::Label, "Warning", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.warning_label("Warning");
        }),
        (Role::Label, "Information", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.info_label("Information");
        }),
        (Role::Label, "Details", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.subdued_label("Details");
        }),
        (Role::Label, "Property", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.property_row("Property", |_| {});
        }),
        (Role::TextInput, "Search...", |ui, _| {
            use crate::ui_kit::ext::UiExt;
            ui.search_input(&mut String::new());
        }),
        (Role::Button, "Pane Settings", |ui, _| {
            super::chart_controls::PaneControls::new().show(
                ui,
                ui.max_rect(),
                super::chart_controls::PaneId(0),
                true,
            );
        }),
        (Role::Button, "Object type", |ui, _| {
            super::widget_bar::panels::object_tree::ObjectTreePanel::new().show(ui, None, &mut []);
        }),
        (Role::TreeItem, "Trend Line", |ui, selected| {
            use super::widget_bar::panels::object_tree::*;
            let item = SourceItem::drawing(
                1,
                crate::drawings::DrawingToolType::TrendLine,
                egui::Color32::RED,
            );
            let mut panel = ObjectTreePanel::new();
            if selected {
                panel.state_mut().select(1);
            }
            panel.show(ui, None, &mut [item]);
        }),
    ];
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for enabled in [true, false] {
            for selected in [true, false] {
                for &(role, label, draw) in controls {
                    let theme = theme.clone();
                    let mut harness = Harness::builder()
                        .with_context_setup(move |ctx| {
                            egui_extras::install_image_loaders(ctx);
                            crate::theme::apply_to_egui(ctx, &theme);
                        })
                        .with_size(Vec2::new(1440.0, 900.0))
                        .build_ui(|ui| {
                            ui.add_enabled_ui(enabled, |ui| draw(ui, selected));
                        });
                    harness.run();
                    let node = harness.get_by_role_and_label(role, label);
                    assert_eq!(node.accesskit_node().is_disabled(), !enabled, "{label}");
                    if matches!(role, Role::Tab | Role::TreeItem | Role::ListBoxOption) {
                        assert_eq!(node.accesskit_node().data().toggled(), None, "{label}");
                        assert_eq!(
                            node.accesskit_node().is_selected(),
                            Some(selected),
                            "{label}"
                        );
                    }
                    if [
                        "Trend line",
                        "Magnet",
                        "Horizontal Line",
                        "1M",
                        "Stay in drawing mode",
                        "Favorite",
                        "Line tools",
                    ]
                    .contains(&label)
                    {
                        assert_eq!(
                            node.accesskit_node().data().toggled(),
                            Some(if selected {
                                egui::accesskit::Toggled::True
                            } else {
                                egui::accesskit::Toggled::False
                            }),
                            "{label}"
                        );
                    }
                    if label == "Download image" {
                        assert_eq!(node.accesskit_node().is_selected(), None);
                        assert_eq!(node.accesskit_node().data().toggled(), None);
                    }
                    if label == "Close" {
                        assert_eq!(node.accesskit_node().data().toggled(), None);
                    }
                    if label == "Name:" {
                        harness.get_by_role_and_label(Role::TextInput, "Description (optional):");
                    }
                    if label == "Line tools" {
                        assert_eq!(node.accesskit_node().data().is_expanded(), Some(!selected));
                    }
                    if label == "Trend line" {
                        assert_eq!(
                            harness
                                .get_by_role_and_label(Role::Button, "More lines")
                                .accesskit_node()
                                .data()
                                .is_expanded(),
                            Some(selected)
                        );
                    }
                    if role == Role::TreeItem {
                        for label in ["Drawing properties", "Hide", "Lock", "Settings", "Remove"] {
                            harness.get_by_role_and_label(Role::Button, label);
                        }
                    }
                }
            }
        }
    }

    for &(role, label, draw) in controls {
        let mut harness = Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                crate::localization::set_translator(ctx, |english| {
                    (english != "12.50").then(|| format!("Translated: {english}"))
                });
            })
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui(|ui| draw(ui, true));
        harness.run();
        harness.get_by_role_and_label(role, &format!("Translated: {label}"));
    }
}

#[test]
fn dialogs_expose_titles_and_label_their_fields() {
    use super::dialogs::{
        drawing_properties::{DrawingId, DrawingPropertiesDialog, DrawingProps, PropertiesTab},
        series_settings::{SeriesSettingsDialog, SeriesSettingsTab},
    };
    use super::top_toolbar::components::{
        indicator_dialog::IndicatorSettingsDialog,
        settings_menu::SettingsDialog,
        symbol_search::{CompareSymbolsDialog, SymbolSearchDialog},
    };
    type Dialog = (
        &'static str,
        fn(&egui::Context),
        &'static [(Role, &'static str)],
    );
    let dialogs: &[Dialog] = &[
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(DrawingId(1), "Text".into(), DrawingProps::default());
                dialog.props.tool_settings = Some(Box::new(
                    crate::drawings::DrawingTemplate::from_drawing(&crate::drawings::Drawing::new(
                        1,
                        crate::drawings::DrawingToolType::TextLabel,
                    )),
                ));
                dialog.active_tab = PropertiesTab::Text;
                dialog.show(ctx);
            },
            &[
                (Role::MultilineTextInput, "Label"),
                (Role::ComboBox, "Font size"),
            ],
        ),
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(DrawingId(1), "Trend Line".into(), DrawingProps::default());
                dialog.coordinates.points.push(Default::default());
                dialog.active_tab = PropertiesTab::Coordinates;
                dialog.show(ctx);
            },
            &[
                (Role::TextInput, "Price"),
                (Role::TextInput, "Date (UTC)"),
                (Role::TextInput, "Time (UTC)"),
            ],
        ),
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(DrawingId(1), "Trend Line".into(), DrawingProps::default());
                dialog.props.tool_settings = Some(Box::new(
                    crate::drawings::DrawingTemplate::from_drawing(&crate::drawings::Drawing::new(
                        1,
                        crate::drawings::DrawingToolType::FibonacciRetracement,
                    )),
                ));
                dialog.active_tab = PropertiesTab::Inputs;
                dialog.show(ctx);
            },
            &[
                (Role::CheckBox, "Level 1 visible"),
                (Role::SpinButton, "Level 1 ratio"),
                (Role::TextInput, "Level 1 label"),
            ],
        ),
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(
                    DrawingId(1),
                    "Volume Profile".into(),
                    DrawingProps::default(),
                );
                let drawing = crate::drawings::Drawing::new(
                    1,
                    crate::drawings::DrawingToolType::FixedRangeVolumeProfile,
                );
                dialog.props.tool_settings = Some(Box::new(
                    crate::drawings::DrawingTemplate::from_drawing(&drawing),
                ));
                dialog.active_tab = PropertiesTab::Inputs;
                dialog.show(ctx);
            },
            &[
                (Role::ComboBox, "Row layout"),
                (Role::SpinButton, "Rows"),
                (Role::SpinButton, "Value area (%)"),
                (Role::ComboBox, "Volume"),
            ],
        ),
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(
                    DrawingId(1),
                    "Volume Profile".into(),
                    DrawingProps::default(),
                );
                let mut drawing = crate::drawings::Drawing::new(
                    1,
                    crate::drawings::DrawingToolType::FixedRangeVolumeProfile,
                );
                let mut config = crate::data::VolumeProfileConfig::default()
                    .with_row_size(Some(1.0))
                    .unwrap();
                config.display = crate::data::ProfileDisplay::UpDown;
                drawing.volume_profile = Some(config);
                dialog.props.tool_settings = Some(Box::new(
                    crate::drawings::DrawingTemplate::from_drawing(&drawing),
                ));
                dialog.active_tab = PropertiesTab::Inputs;
                dialog.show(ctx);
            },
            &[
                (Role::ComboBox, "Row layout"),
                (Role::SpinButton, "Price height"),
                (Role::SpinButton, "Value area (%)"),
                (Role::ComboBox, "Volume"),
            ],
        ),
        (
            "Main series · Candles",
            |ctx| {
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(crate::widget::SeriesId::MAIN, Default::default());
                dialog.active_tab = SeriesSettingsTab::StatusLine;
                dialog.settings.chart_type = crate::model::ChartType::Candles;
                dialog.show(ctx);
            },
            &[
                (Role::CheckBox, "Show OHLC values"),
                (Role::SpinButton, "Decimal places"),
            ],
        ),
        (
            "Main series · Candles",
            |ctx| {
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(crate::widget::SeriesId::MAIN, Default::default());
                dialog.active_tab = SeriesSettingsTab::ScalesAndLines;
                dialog.settings.chart_type = crate::model::ChartType::Candles;
                dialog.show(ctx);
            },
            &[
                (Role::ComboBox, "Scale mode"),
                (Role::CheckBox, "Invert scale"),
            ],
        ),
        (
            "Main series · Candles",
            |ctx| {
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(crate::widget::SeriesId::MAIN, Default::default());
                dialog.active_tab = SeriesSettingsTab::Canvas;
                dialog.settings.chart_type = crate::model::ChartType::Candles;
                dialog.show(ctx);
            },
            &[
                (Role::ComboBox, "Type"),
                (Role::ComboBox, "Mode"),
                (Role::CheckBox, "Vertical grid lines"),
            ],
        ),
        (
            "Main series · Line",
            |ctx| {
                let mut dialog = SeriesSettingsDialog::new();
                dialog.open(crate::widget::SeriesId::MAIN, Default::default());
                dialog.active_tab = SeriesSettingsTab::Symbol;
                dialog.settings.chart_type = crate::model::ChartType::Line;
                dialog.show(ctx);
            },
            &[
                (Role::ComboBox, "Price source"),
                (Role::ComboBox, "Line style"),
                (Role::SpinButton, "Line width"),
            ],
        ),
        (
            "Drawing Properties",
            |ctx| {
                let mut dialog = DrawingPropertiesDialog::new();
                dialog.open(DrawingId(1), "Trend Line".into(), DrawingProps::default());
                dialog.show(ctx);
            },
            &[(Role::ComboBox, "Line style")],
        ),
        (
            "Main series · Candles",
            |ctx| {
                let mut dialog = super::dialogs::series_settings::SeriesSettingsDialog::new();
                dialog.open(crate::widget::SeriesId::MAIN, Default::default());
                dialog.show(ctx);
            },
            &[(Role::Tab, "Symbol")],
        ),
        (
            "Create Alert",
            |ctx| {
                let mut dialog = super::dialogs::alert_dialog::AlertDialog::new();
                dialog.open("TEST".into(), 12.0);
                dialog.show(ctx);
            },
            &[
                (Role::TextInput, "Name"),
                (Role::TextInput, "Symbol"),
                (Role::ComboBox, "Watchlist"),
                (Role::ComboBox, "Frequency"),
            ],
        ),
        (
            "Search symbols",
            |ctx| {
                let mut dialog = SymbolSearchDialog::new();
                dialog.open();
                let _ = dialog.show(
                    ctx,
                    &[crate::model::Instrument::new("TEST").with_description("Example")],
                );
            },
            &[
                (Role::TextInput, "Search symbols"),
                (Role::ListBoxOption, "TEST"),
            ],
        ),
        (
            "Compare symbols",
            |ctx| {
                let mut dialog = CompareSymbolsDialog::new();
                dialog.open();
                dialog.show(
                    ctx,
                    &[crate::model::Instrument::new("TEST").with_description("Example")],
                );
            },
            &[
                (Role::TextInput, "Search symbols"),
                (Role::ListBoxOption, "TEST"),
            ],
        ),
        (
            "SMA settings",
            |ctx| {
                let mut dialog = IndicatorSettingsDialog::default();
                let mut study =
                    crate::studies::ConfiguredStudy::new(Box::new(crate::studies::SMA::new(2)));
                crate::studies::Indicator::set_input(
                    &mut study,
                    "calculation_timeframe",
                    crate::studies::InputValue::Timeframe(Some(crate::model::Timeframe::Custom(
                        std::num::NonZeroU64::new(60).unwrap(),
                    ))),
                )
                .unwrap();
                dialog.open(crate::widget::SeriesId::MAIN, &study, None);
                dialog.show(ctx, &ctx.active_theme());
            },
            &[
                (Role::SpinButton, "Period"),
                (Role::Tab, "Inputs"),
                (Role::ComboBox, "Source"),
                (Role::TextInput, "Symbol"),
                (Role::ComboBox, "Interval"),
                (Role::SpinButton, "Interval"),
                (Role::CheckBox, "Wait for interval close"),
            ],
        ),
        (
            "Chart settings",
            |ctx| {
                let mut dialog = SettingsDialog::new();
                dialog.open(Default::default());
                dialog.show(ctx, &ctx.active_theme());
            },
            &[(Role::Tab, "Symbol")],
        ),
        (
            "Search actions",
            |ctx| {
                let mut palette = crate::ui_kit::command_palette::CommandPalette::new(Vec::new());
                palette.show(ctx, &mut true);
            },
            &[(Role::TextInput, "Search actions")],
        ),
    ];
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        for width in [320.0, 1440.0] {
            for &(title, show, controls) in dialogs {
                let theme = theme.clone();
                let mut harness = Harness::builder()
                    .with_context_setup(move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &theme);
                    })
                    .with_size(Vec2::new(width, 900.0))
                    .build_ui(|ui| show(ui.ctx()));
                harness.run();
                let dialog = harness.get_by_role_and_label(Role::Dialog, title);
                assert_eq!(
                    dialog.accesskit_node().data().is_modal(),
                    title != "Drawing Properties",
                    "{title}"
                );
                for &(role, label) in controls {
                    harness.get_by_role_and_label(role, label);
                }
                let labels: Vec<String> = std::iter::once(title)
                    .chain(controls.iter().map(|(_, label)| *label))
                    .filter(|label| *label != "TEST")
                    .map(str::to_owned)
                    .collect();
                let id = dialog.accesskit_node().id();
                crate::localization::set_translator(&harness.ctx, move |english| {
                    if english == "TEST" {
                        return Some("This identifier must stay unchanged".into());
                    }
                    (labels.iter().any(|label| label == english)
                        || english.starts_with("Level {0}"))
                    .then(|| format!("Translated: {english}"))
                });
                harness.run();
                assert_eq!(
                    harness
                        .get_by_role_and_label(Role::Dialog, &format!("Translated: {title}"))
                        .accesskit_node()
                        .id(),
                    id
                );
                for &(role, label) in controls {
                    let expected = if label == "TEST" {
                        label.to_owned()
                    } else {
                        format!("Translated: {label}")
                    };
                    harness.get_by_role_and_label(role, &expected);
                }
                crate::localization::clear_translator(&harness.ctx);
                harness.run();
                harness.get_by_role_and_label(Role::Dialog, title);
            }
        }
    }
}
