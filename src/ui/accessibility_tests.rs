//! Accessible names, roles and states at the control boundary.

use crate::icons::icons;
use egui::{Role, Ui, Vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

#[test]
fn custom_controls_report_names_roles_and_states() {
    type Control = (Role, &'static str, fn(&mut Ui, bool));
    let controls: &[Control] = &[
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
        (Role::Button, "Indicators", |ui, _| {
            super::top_toolbar::buttons::IconTextButton::new(
                &icons::INDICATORS,
                "Indicators",
                &Default::default(),
            )
            .show(ui);
        }),
        (Role::Button, "Pane Settings", |ui, _| {
            super::chart_controls::PaneControls::new().show(
                ui,
                ui.max_rect(),
                super::chart_controls::PaneId(0),
                true,
            );
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
    for enabled in [true, false] {
        for selected in [true, false] {
            for &(role, label, draw) in controls {
                let mut harness = Harness::builder()
                    .with_context_setup(|ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
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
                if ["Trend line", "Magnet", "1M"].contains(&label) {
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
