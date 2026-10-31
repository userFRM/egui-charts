//! Grid presets and synchronization switches for a chart layout.

use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::ui::model::ChartLayoutMode;
use crate::ui::top_toolbar::TopToolbarConfig;
use crate::ui::top_toolbar::buttons::ToolbarIconButton;
use crate::widget::ChartSyncOptions;
use egui::{RichText, Ui, Vec2};

/// Width of the open menu; a smaller window narrows it.
const MENU_WIDTH: f32 = 428.0;
/// An arrangement tile. Its glyph is the 24-unit layout artwork drawn at 27 points, so the frame
/// spans about 19 by 16 points.
const TILE: Vec2 = Vec2::new(29.0, 27.0);

/// A layout trigger with an anchored, scrollable preset menu.
#[derive(Default)]
pub struct ChartGridSelector {
    current: ChartLayoutMode,
    /// Synchronization options chosen by the user. Apply these to the layout each frame.
    pub sync: ChartSyncOptions,
}

impl ChartGridSelector {
    /// Creates a selector for the initial preset.
    pub fn new(initial: ChartLayoutMode) -> Self {
        Self {
            current: initial,
            ..Default::default()
        }
    }

    /// Selected preset.
    pub fn current(&self) -> ChartLayoutMode {
        self.current
    }

    /// Updates the selection from host state.
    pub fn set_layout(&mut self, layout: ChartLayoutMode) {
        self.current = layout;
    }

    /// Shows the trigger and returns a changed grid or tab preset.
    pub fn show(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        // Sized and labelled like the toolbar buttons beside it, with their tooltip.
        let button = ToolbarIconButton::new(
            layout_icon(self.current),
            "Chart layout",
            &TopToolbarConfig::default(),
        )
        .show(ui);
        let mut selected = None;
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                selected = self.show_menu(ui);
            });
        selected
    }

    /// Every arrangement, in one row per chart count, and the synchronization switches.
    pub fn show_menu(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        let theme = ui.active_theme();
        ui.set_width(
            MENU_WIDTH
                .min(ui.ctx().content_rect().width() - 24.0)
                .max(0.0),
        );
        // The menu scrolls only when the window is too short to hold it below the trigger.
        let max_height = ui.ctx().content_rect().bottom()
            - ui.next_widget_position().y
            - ui.spacing().menu_margin.sum().y
            - theme.spacing.lg;
        ui.set_max_height(max_height);
        let mut selected = None;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.spacing.sm, 0.0);
                for count in 1..=16 {
                    if count > 1 {
                        ui.add(egui::Separator::default().spacing(1.0));
                    }
                    // Tiles wrap onto a second line in a window too narrow for the row.
                    let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
                    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 35.0), row, |ui| {
                        ui.add_sized(
                            Vec2::new(32.0, TILE.y),
                            egui::Label::new(
                                RichText::new(count.to_string())
                                    .size(theme.text.small)
                                    .color(theme.ui.text_muted),
                            ),
                        );
                        for &layout in ChartLayoutMode::all()
                            .iter()
                            .filter(|layout| layout.chart_count() == count)
                        {
                            if arrangement(ui, layout, self.current == layout).clicked() {
                                self.current = layout;
                                selected = Some(layout);
                                ui.close();
                            }
                        }
                    });
                }
                ui.add(egui::Separator::default().spacing(theme.spacing.lg));
                ui.horizontal(|ui| {
                    ui.add_space(theme.spacing.lg);
                    ui.label(
                        RichText::new(
                            l10n::text(ui.ctx(), "Synchronize within group").to_uppercase(),
                        )
                        .size(theme.text.small)
                        .color(theme.ui.text_muted),
                    );
                });
                ui.add_space(theme.spacing.sm);
                for (value, label, effect) in [
                (
                    &mut self.sync.sync_symbol,
                    "Symbol",
                    "A symbol chosen on one chart applies to every chart of the group.",
                ),
                (
                    &mut self.sync.sync_timeframe,
                    "Interval",
                    "An interval chosen on one chart applies to every chart of the group.",
                ),
                (
                    &mut self.sync.sync_crosshair,
                    "Crosshair",
                    "Every chart of the group marks the time hovered on one of them.",
                ),
                (
                    &mut self.sync.sync_time_axis,
                    "Scroll and zoom",
                    "Scrolling or zooming one chart shows the same time span on every chart of the group.",
                ),
                (
                    &mut self.sync.sync_date_range,
                    "Date range",
                    "A date range chosen on one chart applies to every chart of the group.",
                ),
                (
                    &mut self.sync.sync_drawings,
                    "Drawings on the same symbol",
                    "A drawing completed on one chart appears on the charts of the group showing the same symbol.",
                ),
                ] {
                    sync_switch(ui, value, label, effect);
                }
            });
        selected
    }
}

/// One arrangement: a tile filled with the text colour, and its glyph in the panel colour, when it
/// is the current preset.
fn arrangement(ui: &mut Ui, layout: ChartLayoutMode, chosen: bool) -> egui::Response {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(TILE, egui::Sense::click());
    let fill = if chosen {
        theme.ui.text
    } else if response.is_pointer_button_down_on() {
        ui.visuals().widgets.active.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, theme.radius.sm, fill);
    let glyph = egui::Rect::from_center_size(rect.center(), Vec2::splat(27.0));
    let ink = if chosen {
        theme.ui.panel_bg
    } else {
        theme.ui.text
    };
    layout_icon(layout)
        .as_image_tinted(glyph.size(), ink)
        .paint_at(ui, glyph);
    crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.sm);
    let name = l10n::text(ui.ctx(), layout.label());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::MenuItemRadio, response.enabled(), chosen, &name)
    });
    response.on_hover_text(name)
}

/// A synchronization option. Clicking the row switches it in place. The information glyph explains
/// the option on hover; the explanation is also the switch's accessible description.
fn sync_switch(ui: &mut Ui, on: &mut bool, label: &str, effect: &str) {
    let theme = ui.active_theme();
    let (row, mut response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let name = l10n::text(ui.ctx(), label);
    let effect = l10n::text(ui.ctx(), effect);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Switch, response.enabled(), *on, &name)
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_description(effect.as_str()));
    let switch = egui::Rect::from_center_size(
        row.right_center() - Vec2::new(theme.spacing.lg + 19.0, 0.0),
        Vec2::new(38.0, 20.0),
    );
    // A long translation is cut short before the glyph and the switch; its name stays whole.
    let room = switch.left() - row.left() - 2.0 * theme.spacing.lg - theme.spacing.sm - 14.0;
    let text = egui::WidgetText::from(name.as_str()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        room.max(0.0),
        egui::TextStyle::Body,
    );
    let text_rect = egui::Align2::LEFT_CENTER.anchor_size(
        row.left_center() + Vec2::new(theme.spacing.lg, 0.0),
        text.size(),
    );
    ui.painter().galley(text_rect.min, text, theme.ui.text);
    let info = egui::Rect::from_min_size(
        egui::pos2(text_rect.right() + theme.spacing.sm, row.center().y - 7.0),
        Vec2::splat(14.0),
    );
    icons::INFO
        .as_image_tinted(info.size(), theme.ui.text_muted)
        .paint_at(ui, info);
    // The glyph takes its own clicks, so asking for the explanation leaves the option as it is.
    ui.interact(info, response.id.with("info"), egui::Sense::CLICK)
        .on_hover_text(effect);
    let track = if *on {
        theme.ui.text
    } else {
        theme.ui.border_control
    };
    ui.painter().rect_filled(switch, 10.0, track);
    let knob = if *on {
        switch.right() - 10.0
    } else {
        switch.left() + 10.0
    };
    ui.painter()
        .circle_filled(egui::pos2(knob, switch.center().y), 7.0, theme.ui.panel_bg);
    if response.has_focus() {
        // Inside the row, as in the cursor flyout, because the scroll area clips outside rings.
        ui.painter().rect_stroke(
            row.shrink(2.0),
            theme.radius.md,
            egui::Stroke::new(2.0, crate::styles::focus::focus_ring_color(ui)),
            egui::StrokeKind::Inside,
        );
    }
}

/// The glyph of `layout` on the trigger and in the menu: its cells, as the layout tiles the charts.
fn layout_icon(layout: ChartLayoutMode) -> &'static Icon {
    use ChartLayoutMode as M;
    match layout {
        M::Single => &icons::LAYOUT_SINGLE,
        M::TwoHorizontal => &icons::LAYOUT_TWO_HORIZONTAL,
        M::TwoVertical => &icons::LAYOUT_TWO_VERTICAL,
        M::Three => &icons::LAYOUT_THREE,
        M::ThreeHorizontal => &icons::LAYOUT_THREE_HORIZONTAL,
        M::ThreeVertical => &icons::LAYOUT_THREE_VERTICAL,
        M::ThreeLargeLeft => &icons::LAYOUT_THREE_LARGE_LEFT,
        M::ThreeLargeRight => &icons::LAYOUT_THREE_LARGE_RIGHT,
        M::ThreeLargeTop => &icons::LAYOUT_THREE_LARGE_TOP,
        M::Four => &icons::LAYOUT_FOUR,
        M::FourHorizontal => &icons::LAYOUT_FOUR_HORIZONTAL,
        M::FourVertical => &icons::LAYOUT_FOUR_VERTICAL,
        M::FourLargeLeft => &icons::LAYOUT_FOUR_LARGE_LEFT,
        M::FourLargeRight => &icons::LAYOUT_FOUR_LARGE_RIGHT,
        M::FourLargeTop => &icons::LAYOUT_FOUR_LARGE_TOP,
        M::FourLargeBottom => &icons::LAYOUT_FOUR_LARGE_BOTTOM,
        M::Five => &icons::LAYOUT_FIVE,
        M::FiveHorizontal => &icons::LAYOUT_FIVE_HORIZONTAL,
        M::FiveVertical => &icons::LAYOUT_FIVE_VERTICAL,
        M::FiveLargeLeft => &icons::LAYOUT_FIVE_LARGE_LEFT,
        M::FiveLargeRight => &icons::LAYOUT_FIVE_LARGE_RIGHT,
        M::FiveLargeTop => &icons::LAYOUT_FIVE_LARGE_TOP,
        M::FiveLargeBottom => &icons::LAYOUT_FIVE_LARGE_BOTTOM,
        M::Six => &icons::LAYOUT_SIX,
        M::SixHorizontal => &icons::LAYOUT_SIX_HORIZONTAL,
        M::SixVertical => &icons::LAYOUT_SIX_VERTICAL,
        M::SixLargeLeft => &icons::LAYOUT_SIX_LARGE_LEFT,
        M::SixLargeRight => &icons::LAYOUT_SIX_LARGE_RIGHT,
        M::SixLargeTop => &icons::LAYOUT_SIX_LARGE_TOP,
        M::SixLargeBottom => &icons::LAYOUT_SIX_LARGE_BOTTOM,
        M::Seven => &icons::LAYOUT_SEVEN,
        M::Eight => &icons::LAYOUT_EIGHT,
        M::Nine => &icons::LAYOUT_NINE,
        M::Ten => &icons::LAYOUT_TEN,
        M::Eleven => &icons::LAYOUT_ELEVEN,
        M::Twelve => &icons::LAYOUT_TWELVE,
        M::Thirteen => &icons::LAYOUT_THIRTEEN,
        M::Fourteen => &icons::LAYOUT_FOURTEEN,
        M::Fifteen => &icons::LAYOUT_FIFTEEN,
        M::Sixteen => &icons::LAYOUT_SIXTEEN,
        M::Tabs => &icons::LAYOUT_TABS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Role;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    /// Clicking a synchronization switch changes that option alone, and the menu stays open on the
    /// switch's new state.
    #[test]
    fn sync_switch_toggles_its_option_in_place() {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::light());
            })
            .build_ui_state(
                |ui, selector: &mut ChartGridSelector| {
                    selector.show(ui);
                },
                ChartGridSelector::default(),
            );
        harness
            .get_by_role_and_label(Role::Button, "Chart layout")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(Role::Switch, "Crosshair")
            .click();
        harness.run();
        let expected = ChartSyncOptions {
            sync_crosshair: false,
            ..Default::default()
        };
        assert_eq!(harness.state().sync, expected);
        assert_eq!(
            harness
                .get_by_role_and_label(Role::Switch, "Crosshair")
                .accesskit_node()
                .data()
                .toggled(),
            Some(egui::accesskit::Toggled::False)
        );
    }
}
