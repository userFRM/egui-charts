//! Mapping of a [`Theme`] onto egui's styles.

use super::{Theme, context};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// WCAG 2 contrast ratio of `fg` painted over the opaque `bg`.
pub(crate) fn contrast(fg: Color32, bg: Color32) -> f32 {
    let luminance = |c: Color32| {
        let c = egui::Rgba::from(c);
        0.2126 * c.r() + 0.7152 * c.g() + 0.0722 * c.b()
    };
    let (a, b) = (luminance(bg.blend(fg)), luminance(bg));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Move `color` towards `text` only as far as needed to read on every background.
pub(crate) fn readable_color(
    color: Color32,
    text: Color32,
    backgrounds: &[Color32],
    min: f32,
) -> Color32 {
    (0..=255)
        .map(|step| color.lerp_to_gamma(text, step as f32 / 255.0))
        .find(|&candidate| backgrounds.iter().all(|&bg| contrast(candidate, bg) >= min))
        .unwrap_or(text)
}

/// Applies the same palette and spacing to a local UI or to the global styles.
pub(crate) fn apply_to_style(style: &mut egui::Style, theme: &Theme) {
    let mut visuals = if theme.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let ui = &theme.ui;

    // Window/panel backgrounds
    visuals.panel_fill = ui.panel_bg;
    // Windows, popups, menus and tooltips sit one elevation step above panels.
    visuals.window_fill = ui.panel_bg_floating;
    visuals.extreme_bg_color = ui.panel_bg;
    // faint_bg_color must match panel_fill to avoid thin line at window top
    // Previously used a different color for "visible gaps" but this caused artifacts
    visuals.faint_bg_color = ui.panel_bg;

    // Widget backgrounds
    visuals.widgets.noninteractive.bg_fill = ui.panel_bg;
    visuals.widgets.inactive.bg_fill = ui.panel_bg;
    visuals.widgets.hovered.bg_fill = ui.btn_bg_hover;
    visuals.widgets.active.bg_fill = ui.btn_bg_active;
    visuals.widgets.open.bg_fill = ui.btn_bg_active;
    // Buttons, combo boxes and headers paint weak fills rather than bg_fill.
    let widgets = &mut visuals.widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.weak_bg_fill = state.bg_fill;
    }

    // Widget borders
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border);
    // egui outlines checkboxes, radios, text inputs and combo boxes with these strokes.
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.accent);

    visuals.widgets.open.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);

    // Text/foreground colors
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);

    visuals.widgets.open.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);

    // Selection: egui paints selected labels in `selection.stroke`, on top of `bg_fill`, and
    // outlines a focused text input with `selection.stroke`. Dark themes use white on the accent,
    // light themes accent text on a pale accent fill.
    visuals.text_cursor.stroke.color = ui.text;
    visuals.selection.bg_fill = ui.selection_bg;
    visuals.selection.stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.selection_text);

    // egui fades weak text by alpha by default, which drops it below a readable contrast.
    visuals.weak_text_color = Some(ui.text_muted);
    visuals.error_fg_color = ui.error;
    visuals.warn_fg_color = ui.warning;

    // Hyperlinks
    visuals.hyperlink_color = readable_color(
        ui.accent,
        ui.text,
        &[
            ui.panel_bg,
            ui.panel_bg_floating,
            ui.panel_bg_secondary,
            ui.btn_bg_hover,
        ],
        7.0,
    );

    // Floating chrome: hairline border, small radius and a tight shadow that stays close to
    // the surface instead of smearing over neighbouring toolbars.
    let shadow_alpha = if theme.dark { 96 } else { 32 };
    visuals.window_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border);
    visuals.window_corner_radius = theme.radius.md.into();
    visuals.menu_corner_radius = theme.radius.sm.into();
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha / 2),
    };

    // egui's built-in widgets keep the theme's compact look: small corners here, and tight
    // padding and control height on the style below.
    let widgets = &mut visuals.widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = theme.radius.sm.into();
    }

    let spacing = &theme.spacing;
    style.visuals = visuals;
    for (role, size) in [
        (egui::TextStyle::Small, theme.text.small),
        (egui::TextStyle::Body, theme.text.body),
        (egui::TextStyle::Button, theme.text.body),
        (egui::TextStyle::Heading, theme.text.heading),
        (egui::TextStyle::Monospace, theme.text.body),
    ] {
        if let Some(font) = style.text_styles.get_mut(&role) {
            font.size = size;
        }
    }
    style.spacing.window_margin = egui::Margin::same(spacing.xl as i8);
    style.spacing.menu_margin = egui::Margin::symmetric(spacing.lg as i8, spacing.sm as i8);
    style.spacing.item_spacing = egui::vec2(spacing.lg, spacing.sm);
    style.spacing.button_padding = egui::vec2(spacing.sm, DESIGN_TOKENS.spacing.hairline);
    style.spacing.interact_size.y = DESIGN_TOKENS.sizing.control_height;
}

/// Apply `theme` to egui's visuals and store it for [`ThemeExt::active_theme`].
/// Also install Roboto ahead of the application's proportional fonts.
///
/// Call it once at startup and again after the theme changes. Inside `eframe::App::ui` the root
/// `Ui` already exists, so follow the call with `ui.set_style(ui.ctx().global_style())` for the
/// panels shown in that `Ui` to use the theme in the same frame.
pub fn apply_to_egui(ctx: &egui::Context, theme: &Theme) {
    // Style both appearances so system changes keep the applied theme.
    ctx.all_styles_mut(|style| apply_to_style(style, theme));

    // Interface text: Roboto, whose default digits are tabular, so prices keep their width.
    ctx.add_font(egui::FontInsert::new(
        "Roboto",
        egui::FontData::from_static(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/Roboto-Latin.ttf"
        ))),
        vec![
            egui::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::FontPriority::Highest,
            },
            egui::InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::FontPriority::Lowest,
            },
        ],
    ));

    // Store the theme so components resolve their colours from it while painting.
    context::set_active_theme(ctx, theme.clone());
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ThemeExt, presets};

    #[test]
    fn chart_theme_changes_keep_data_and_viewport_and_refresh_overlays() {
        use crate::{
            chart::selection::SeriesId,
            model::{Bar, BarData},
            widget::Chart,
        };
        let bars = vec![Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            1.0,
            2.0,
            1.0,
            2.0,
            10.0,
        )];
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        let ctx = egui::Context::default();
        let offset = chart.state().time_scale().right_offset();
        for (_, theme) in presets() {
            chart.set_theme(&ctx, &theme);
            assert_eq!(chart.bars(SeriesId::MAIN), bars);
            assert_eq!(chart.state().time_scale().right_offset(), offset);
            assert_eq!(chart.config().background_color, theme.chart.bg);
            let crosshair = chart.chart_options().crosshair.resolved(&ctx);
            assert_eq!(
                crosshair.vert_label_background_color,
                theme.chart.crosshair_label_bg
            );
            assert_eq!(
                crosshair.horz_label_background_color,
                theme.chart.crosshair_label_bg
            );
            assert_eq!(crosshair.label_text_color, theme.chart.crosshair_label_text);
            let tooltip = chart.chart_options().tooltip.resolved(&ctx);
            assert_eq!(tooltip.background_color, theme.chart.bg_tooltip);
        }
        // A builder theme must reach text and overlays locally, leaving the host palette and
        // egui styles intact for the next chart in the same frame.
        let global = Theme::dark();
        apply_to_egui(&ctx, &global);
        let original_style = ctx.global_style();
        let mut local = Theme::light();
        local.text.label = 19.0;
        let mut chart = crate::widget::ChartBuilder::new()
            .with_symbol("Scoped")
            .with_theme(local.clone())
            .build();
        chart.set_data(SeriesId::MAIN, bars.clone());
        let mut chart_id = None;
        for clear_globally in [false, true] {
            if clear_globally {
                chart.set_theme_override(Some(local.clone()));
            }
            for clear in [false, true] {
                if clear {
                    if clear_globally {
                        chart.set_theme(&ctx, &global);
                    } else {
                        chart.set_theme_override(None);
                    }
                }
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            let response = chart.show(ui);
                            if let Some(id) = chart_id {
                                assert_eq!(
                                    response.id, id,
                                    "theme changes must retain chart UI state"
                                );
                                assert!(response.has_focus());
                            } else {
                                chart_id = Some(response.id);
                                response.request_focus();
                            }
                        });
                    },
                );
                let sizes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text().parse::<f64>().is_ok() => {
                            Some(text.galley.job.sections[0].format.font_id.size)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!sizes.is_empty());
                assert_eq!(sizes.contains(&local.text.label), !clear, "{sizes:?}");
                if clear {
                    assert!(sizes.contains(&global.text.label), "{sizes:?}");
                }
                assert_eq!(ctx.active_theme(), global);
                assert_eq!(ctx.global_style().visuals, original_style.visuals);
                assert_eq!(ctx.global_style().spacing, original_style.spacing);
                output.drop_without_applying_deltas();
            }
        }
    }


    #[test]
    fn bundled_font_keeps_prices_tabular() {
        let ctx = egui::Context::default();
        for theme in [Theme::dark(), Theme::light(), Theme::dark()] {
            apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                ctx.fonts(|fonts| {
                    assert_eq!(
                        fonts.definitions().families[&egui::FontFamily::Proportional][0],
                        "Roboto",
                        "the bundled face must be the first proportional font"
                    );
                });
                for (size, weight) in [(12.0, None), (13.0, None), (13.0, Some(600.0))] {
                    let mut format = egui::text::TextFormat::simple(
                        egui::FontId::proportional(size),
                        Color32::WHITE,
                    );
                    if let Some(weight) = weight {
                        format.coords.push("wght", weight);
                    }
                    let widths: Vec<_> = ('0'..='9')
                        .map(|digit| {
                            ui.painter()
                                .layout_job(egui::text::LayoutJob::single_section(
                                    digit.to_string(),
                                    format.clone(),
                                ))
                                .size()
                                .x
                        })
                        .collect();
                    assert!(
                        widths.iter().all(|width| *width == widths[0]),
                        "digits must keep their width at size {size}, weight {weight:?}: {widths:?}"
                    );
                }
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn text_and_control_outlines_are_readable_in_every_preset() {
        let dark = Theme::dark();
        for (role, actual, expected) in [
            ("panel", dark.ui.panel_bg, [0, 0, 0]),
            ("plot", dark.chart.bg, [0, 0, 0]),
            ("axis", dark.chart.bg_axis, [0, 0, 0]),
            ("separator", dark.ui.border, [42, 42, 42]),
            ("text", dark.ui.text, [219, 219, 219]),
            ("secondary text", dark.ui.text_secondary, [184, 184, 184]),
            ("disabled text", dark.ui.text_disabled, [87, 87, 87]),
            ("price text", dark.chart.axis_text, [255, 255, 255]),
            (
                "secondary axis text",
                dark.chart.axis_text_secondary,
                [184, 184, 184],
            ),
            ("grid", dark.chart.grid_line, [34, 34, 34]),
            ("up candle", dark.chart.candle_up, [8, 153, 129]),
            ("down candle", dark.chart.candle_down, [242, 54, 69]),
            ("active toggle", dark.ui.accent, [41, 98, 255]),
        ] {
            assert_eq!(
                actual,
                Color32::from_rgb(expected[0], expected[1], expected[2]),
                "dark {role}"
            );
        }
        let orange = Color32::from_rgb(255, 120, 0);
        let purple = Color32::from_rgb(110, 60, 200);
        let mut themes = presets()
            .map(|(name, theme)| (name.to_owned(), theme))
            .to_vec();
        themes.extend([
            (
                "dark, orange accent".into(),
                Theme::dark().with_accent(orange),
            ),
            (
                "light, orange accent".into(),
                Theme::light().with_accent(orange),
            ),
            (
                "light, purple accent".into(),
                Theme::light().with_accent(purple),
            ),
        ]);
        for (name, base) in presets() {
            for (accent_name, accent) in [
                ("white", Color32::WHITE),
                ("black", Color32::BLACK),
                ("yellow", Color32::YELLOW),
                ("blue", Color32::BLUE),
                ("navy", Color32::from_rgb(10, 10, 30)),
            ] {
                themes.push((
                    format!("{name}, {accent_name} accent"),
                    base.clone().with_accent(accent),
                ));
            }
        }
        let focus_ring_of = |ctx: &egui::Context| {
            let mut focus_ring = Color32::TRANSPARENT;
            ctx.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    focus_ring = crate::styles::focus::focus_ring_color(ui);
                });
            })
            .drop_without_applying_deltas();
            focus_ring
        };
        let mut failures = Vec::new();
        for (name, theme) in themes {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            let visuals = ctx.global_style().visuals.clone();
            let ui = &theme.ui;
            if visuals.text_cursor.stroke.color != ui.text {
                failures.push(format!("{name}: caret does not use the theme text colour"));
            }
            for state in [
                &visuals.widgets.inactive,
                &visuals.widgets.hovered,
                &visuals.widgets.active,
                &visuals.widgets.open,
            ] {
                if state.corner_radius != egui::CornerRadius::from(theme.radius.sm)
                    || DESIGN_TOKENS.rounding.button != theme.radius.sm
                {
                    failures.push(format!("{name}: interactive radii differ"));
                }
            }
            for (role, fg) in [
                ("text", ui.text),
                ("secondary text", ui.text_secondary),
                ("price text", theme.chart.axis_text),
                ("secondary axis text", theme.chart.axis_text_secondary),
            ] {
                assert!(contrast(fg, theme.chart.bg) >= 4.5, "{name}: {role}");
            }
            assert_eq!(visuals.widgets.noninteractive.fg_stroke.color, ui.text);
            for (state, fill) in [
                (&visuals.widgets.noninteractive, ui.panel_bg),
                (&visuals.widgets.inactive, ui.panel_bg),
                (&visuals.widgets.hovered, ui.btn_bg_hover),
                (&visuals.widgets.active, ui.btn_bg_active),
                (&visuals.widgets.open, ui.btn_bg_active),
            ] {
                assert_eq!(state.weak_bg_fill, fill, "{name}: widget fill");
            }
            assert_eq!(visuals.widgets.open.fg_stroke.color, ui.text);
            #[cfg(feature = "ui")]
            assert_eq!(
                crate::ui::timeframe_toolbar::TimeframeBarConfig::from_theme(&theme)
                    .active_text_color,
                ui.text
            );
            let focus_ring = focus_ring_of(&ctx);
            // High contrast holds every text level to 7:1 and outlines to 4.5:1 (WCAG AAA).
            let (text_min, outline_min) = if name == "high-contrast" {
                (7.0, 4.5)
            } else {
                (4.5, 3.0)
            };
            let mut check = |role: &str, fg: Color32, surface: &str, bg: Color32, min: f32| {
                let ratio = contrast(fg, bg);
                if ratio < min {
                    failures.push(format!(
                        "{name}: {role} on {surface} is {ratio:.2}:1, below {min}:1"
                    ));
                }
            };
            check(
                "selected text",
                visuals.selection.stroke.color,
                "selection",
                visuals.selection.bg_fill,
                text_min,
            );
            for (fill, bg) in [
                ("accent", ui.accent),
                ("hovered accent", ui.accent_hover),
                ("pressed accent", ui.accent_active),
            ] {
                check("on-accent text", ui.accent_text, fill, bg, text_min);
            }
            for (fill, bg) in [
                ("danger", ui.danger_bg),
                ("hovered danger", ui.danger_bg_hover),
                ("pressed danger", ui.danger_bg_active),
            ] {
                check("on-danger text", ui.danger_text, fill, bg, text_min);
            }
            // Hover must read on a panel, and pressed must sit further from it than hover.
            let (panel, hovered) = (visuals.panel_fill, visuals.widgets.hovered.weak_bg_fill);
            check("hover fill", hovered, "panels", panel, 1.2);
            let pressed_min = contrast(hovered, panel) + 0.05;
            check(
                "pressed fill",
                visuals.widgets.active.weak_bg_fill,
                "panels",
                panel,
                pressed_min,
            );
            check(
                "pressed fill",
                visuals.widgets.active.weak_bg_fill,
                "hover fill",
                hovered,
                1.2,
            );
            check(
                "pressed text",
                ui.text,
                "pressed fill",
                ui.btn_bg_active,
                text_min,
            );
            check(
                "open outline",
                visuals.widgets.open.bg_stroke.color,
                "panels",
                panel,
                outline_min,
            );
            if name.contains("accent") {
                check("selection fill", ui.selection_bg, "panels", panel, 1.3);
                check("accent hover", ui.accent_hover, "accent", ui.accent, 1.2);
                check(
                    "accent pressed",
                    ui.accent_active,
                    "accent hover",
                    ui.accent_hover,
                    1.2,
                );
                assert_ne!(ui.selection_text, ui.text, "{name}: selected glyphs");
            }
            // egui outlines a focused text input with the selection stroke.
            check(
                "focused input outline",
                visuals.selection.stroke.color,
                "inputs",
                visuals.text_edit_bg_color(),
                outline_min,
            );
            for (surface, bg) in [
                ("panels", visuals.panel_fill),
                ("windows", visuals.window_fill),
                ("secondary panels", ui.panel_bg_secondary),
                ("hover fill", ui.btn_bg_hover),
            ] {
                check("links", visuals.hyperlink_color, surface, bg, 7.0);
                check("text", ui.text, surface, bg, text_min);
                check("label text", visuals.text_color(), surface, bg, text_min);
                check("muted text", ui.text_muted, surface, bg, text_min);
                check(
                    "weak text",
                    visuals.weak_text_color(),
                    surface,
                    bg,
                    text_min,
                );
                check("success text", ui.success, surface, bg, text_min);
                check("error text", visuals.error_fg_color, surface, bg, text_min);
                check("warning text", visuals.warn_fg_color, surface, bg, text_min);
                check(
                    "control outline",
                    visuals.widgets.inactive.bg_stroke.color,
                    surface,
                    bg,
                    outline_min,
                );
                check("keyboard focus ring", focus_ring, surface, bg, outline_min);
                check(
                    "text cursor",
                    visuals.text_cursor.stroke.color,
                    surface,
                    bg,
                    3.0,
                );
            }
        }
        // An app that styles egui itself and never applies a theme keeps a visible ring.
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Light);
        let (focus_ring, panel) = (focus_ring_of(&ctx), ctx.global_style().visuals.panel_fill);
        let ratio = contrast(focus_ring, panel);
        if ratio < 3.0 {
            failures.push(format!(
                "no theme, light visuals: keyboard focus ring on panels is {ratio:.2}:1, below 3:1"
            ));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// egui follows the system appearance by default and keeps a style for each, so an app that
    /// applies its theme once must keep it when the system switches between light and dark.
    #[test]
    fn the_theme_outlasts_a_switch_of_the_system_appearance() {
        let ctx = egui::Context::default();
        let frame = |system: egui::Theme, apply: bool| {
            let input = egui::RawInput {
                system_theme: Some(system),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                if apply {
                    apply_to_egui(ui.ctx(), &Theme::high_contrast());
                }
            })
            .drop_without_applying_deltas();
        };
        frame(egui::Theme::Dark, true);
        let applied = ctx.global_style();
        frame(egui::Theme::Light, false);
        let style = ctx.global_style();
        assert!(
            style.visuals == applied.visuals && style.spacing == applied.spacing,
            "switching the system to light dropped the applied theme"
        );
    }
}
