//! Theme: the colours and scales of the UI and the chart.
//!
//! A [`Theme`] is a plain value with public fields. Start from a preset
//! ([`Theme::dark`], the default, [`Theme::light`] or [`Theme::high_contrast`]),
//! change the fields you need, and pass it to [`apply_to_egui`], once at startup
//! and again after every change.
//!
//! ```
//! use egui::Color32;
//! use egui_charts::theme::{Theme, apply_to_egui};
//!
//! # let ctx = &egui::Context::default();
//! let mut theme = Theme::dark().with_accent(Color32::from_rgb(255, 120, 0));
//! theme.chart.candle_up = Color32::from_rgb(0, 160, 120);
//! apply_to_egui(ctx, &theme);
//! ```

pub mod components;
mod context;
mod presets;

pub use components::{ContextMenuStyle, DialogStyle, SettingsDialogStyle};
pub use context::ThemeExt;
pub(crate) use context::applied_theme;
#[cfg(feature = "ui")]
pub(crate) use context::{ThemeDerived, follow_active_theme, theme_derived};
pub use presets::presets;

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// The colours and scales of the UI and the chart.
///
/// With the `serde` feature, a theme serialises with any serde format, and fields
/// missing from the input take their value from [`Theme::dark`]. Colours are egui's
/// premultiplied `[r, g, b, a]` arrays.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Theme {
    /// Whether the UI is dark: selects egui's base visuals and the shadow strength.
    pub dark: bool,
    /// Colours of the UI chrome: panels, text, controls and status.
    pub ui: UiColors,
    /// Colours of the chart surface.
    pub chart: ChartColors,
    /// Type scale. Not yet applied to egui's text styles.
    pub text: TextSizes,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Corner radius scale.
    pub radius: Radii,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Colours of the UI chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct UiColors {
    /// Panels, toolbars and inputs.
    pub panel_bg: Color32,
    /// Sidebars, cards and the gaps between panels.
    pub panel_bg_secondary: Color32,
    /// Windows, menus, popups and tooltips.
    pub panel_bg_floating: Color32,
    /// Primary text.
    pub text: Color32,
    /// Labels, and icons at rest.
    pub text_secondary: Color32,
    /// Hints, shortcuts and other weak text.
    pub text_muted: Color32,
    /// Text of disabled controls.
    pub text_disabled: Color32,
    /// Separators and window outlines.
    pub border: Color32,
    /// Resting outline of checkboxes, radios, inputs and combo boxes.
    pub border_control: Color32,
    /// Fill of a hovered button.
    pub btn_bg_hover: Color32,
    /// Fill of a pressed or open button.
    pub btn_bg_active: Color32,
    /// Fill of selected text and selected items.
    pub selection_bg: Color32,
    /// Text on `selection_bg`; egui also outlines a focused text input with it.
    pub selection_text: Color32,
    /// Keyboard focus ring.
    pub focus: Color32,
    /// Fill of primary actions, and the colour of links and active icons.
    pub accent: Color32,
    /// `accent` when hovered.
    pub accent_hover: Color32,
    /// `accent` when pressed.
    pub accent_active: Color32,
    /// Text and icons on the accent fills.
    pub accent_text: Color32,
    /// Fill of destructive actions.
    pub danger_bg: Color32,
    /// `danger_bg` when hovered.
    pub danger_bg_hover: Color32,
    /// `danger_bg` when pressed.
    pub danger_bg_active: Color32,
    /// Text and icons on the danger fills.
    pub danger_text: Color32,
    /// Success text and icons.
    pub success: Color32,
    /// Warning text and icons.
    pub warning: Color32,
    /// Error text and icons.
    pub error: Color32,
    /// Informational text and icons.
    pub info: Color32,
}

impl Default for UiColors {
    fn default() -> Self {
        Theme::dark().ui
    }
}

/// Colours of the chart surface.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ChartColors {
    /// Plot background.
    pub bg: Color32,
    /// Price and time axis background.
    pub bg_axis: Color32,
    /// Tooltip background.
    pub bg_tooltip: Color32,
    /// Minor grid lines.
    pub grid_line: Color32,
    /// Major grid lines.
    pub grid_line_major: Color32,
    /// Axis labels.
    pub axis_text: Color32,
    /// Secondary axis labels.
    pub axis_text_secondary: Color32,
    /// Crosshair lines.
    pub crosshair_line: Color32,
    /// Background of the crosshair value labels.
    pub crosshair_label_bg: Color32,
    /// Text of the crosshair value labels.
    pub crosshair_label_text: Color32,
    /// Rising candles and bars: body, border and wick.
    pub candle_up: Color32,
    /// Falling candles and bars: body, border and wick.
    pub candle_down: Color32,
    /// Opacity of volume bars, which take the candle colours.
    pub volume_alpha: u8,
}

impl Default for ChartColors {
    fn default() -> Self {
        Theme::dark().chart
    }
}

impl ChartColors {
    /// Rising volume bars: `candle_up` at `volume_alpha`.
    pub fn volume_up(&self) -> Color32 {
        let [r, g, b, _] = self.candle_up.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, self.volume_alpha)
    }

    /// Falling volume bars: `candle_down` at `volume_alpha`.
    pub fn volume_down(&self) -> Color32 {
        let [r, g, b, _] = self.candle_down.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, self.volume_alpha)
    }
}

/// Font sizes, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct TextSizes {
    /// Badges and footnotes.
    pub tiny: f32,
    /// Secondary labels and axis text.
    pub small: f32,
    /// Control labels.
    pub label: f32,
    /// Body text and buttons.
    pub body: f32,
    /// Panel and dialog titles.
    pub title: f32,
    /// Section headings.
    pub heading: f32,
}

impl Default for TextSizes {
    fn default() -> Self {
        Self {
            tiny: 10.0,
            small: 11.0,
            label: 12.0,
            body: 13.0,
            title: 14.0,
            heading: 16.0,
        }
    }
}

impl TextSizes {
    /// Every size multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        Self {
            tiny: self.tiny * factor,
            small: self.small * factor,
            label: self.label * factor,
            body: self.body * factor,
            title: self.title * factor,
            heading: self.heading * factor,
        }
    }
}

/// Spacing scale, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Spacing {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub xxxl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 2.0,
            sm: 4.0,
            md: 6.0,
            lg: 8.0,
            xl: 12.0,
            xxl: 16.0,
            xxxl: 24.0,
        }
    }
}

/// Corner radius scale, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Radii {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            xs: 2.0,
            sm: 4.0,
            md: 6.0,
            lg: 8.0,
            xl: 12.0,
        }
    }
}

impl Theme {
    /// This theme with `accent` as its accent colour.
    ///
    /// Also sets the colours derived from it: black or white `accent_text`, whichever
    /// reads better on `accent`; hover and pressed shades 10% and 20% further from that
    /// text colour; and the selection. Dark themes select with `accent` and white text
    /// when white reads on it, else with `accent` darkened; light themes with a pale
    /// `accent` over `panel_bg`. Text on the accent and the selection reads at 4.5:1 or
    /// better, not the 7:1 of [`Theme::high_contrast`].
    pub fn with_accent(mut self, accent: Color32) -> Self {
        let white_text = contrast(Color32::WHITE, accent) >= contrast(Color32::BLACK, accent);
        let (on_accent, away) = if white_text {
            (Color32::WHITE, Color32::BLACK)
        } else {
            (Color32::BLACK, Color32::WHITE)
        };
        let ui = &mut self.ui;
        ui.accent = accent;
        ui.accent_hover = accent.lerp_to_gamma(away, 0.1);
        ui.accent_active = accent.lerp_to_gamma(away, 0.2);
        ui.accent_text = on_accent;
        (ui.selection_bg, ui.selection_text) = match (self.dark, white_text) {
            (true, true) => (accent, Color32::WHITE),
            // egui outlines a focused text input with the selection text, so it stays light.
            (true, false) => (accent.lerp_to_gamma(Color32::BLACK, 0.6), Color32::WHITE),
            (false, true) => (
                ui.panel_bg.lerp_to_gamma(accent, 1.0 / 6.0),
                ui.accent_active,
            ),
            (false, false) => (ui.panel_bg.lerp_to_gamma(accent, 1.0 / 6.0), ui.text),
        };
        self
    }

    /// Apply theme to chart config
    pub fn apply_to_config(
        &self,
        mut config: crate::config::ChartConfig,
    ) -> crate::config::ChartConfig {
        config.background_color = self.chart.bg;
        config.grid_color = self.chart.grid_line;
        config.text_color = self.chart.axis_text;
        config.bullish_color = self.chart.candle_up;
        config.bearish_color = self.chart.candle_down;
        config.bullish_border_color = Some(self.chart.candle_up);
        config.bearish_border_color = Some(self.chart.candle_down);
        config.bullish_wick_color = Some(self.chart.candle_up);
        config.bearish_wick_color = Some(self.chart.candle_down);
        config
    }
}

/// WCAG 2 contrast ratio of `fg` painted over the opaque `bg`.
pub(crate) fn contrast(fg: Color32, bg: Color32) -> f32 {
    let luminance = |c: Color32| {
        let c = egui::Rgba::from(c);
        0.2126 * c.r() + 0.7152 * c.g() + 0.0722 * c.b()
    };
    let (a, b) = (luminance(bg.blend(fg)), luminance(bg));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Apply `theme` to egui's visuals and store it for [`ThemeExt::active_theme`].
///
/// Call it once at startup and again after the theme changes. Inside `eframe::App::ui` the root
/// `Ui` already exists, so follow the call with `ui.set_style(ui.ctx().global_style())` for the
/// panels shown in that `Ui` to use the theme in the same frame.
pub fn apply_to_egui(ctx: &egui::Context, theme: &Theme) {
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

    // Widget borders
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border);
    // egui outlines checkboxes, radios, text inputs and combo boxes with these strokes.
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.border_control);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.accent);

    // Text/foreground colors
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text_secondary);
    visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text_secondary);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.text);

    // Selection: egui paints selected labels in `selection.stroke`, on top of `bg_fill`, and
    // outlines a focused text input with `selection.stroke`. Dark themes use white on the accent,
    // light themes accent text on a pale accent fill.
    visuals.selection.bg_fill = ui.selection_bg;
    visuals.selection.stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, ui.selection_text);

    // egui fades weak text by alpha by default, which drops it below a readable contrast.
    visuals.weak_text_color = Some(ui.text_muted);
    visuals.error_fg_color = ui.error;
    visuals.warn_fg_color = ui.warning;

    // Hyperlinks
    visuals.hyperlink_color = ui.accent;

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

    ctx.set_visuals(visuals);

    let spacing = &theme.spacing;
    let mut style = (*ctx.global_style()).clone();
    style.spacing.window_margin = egui::Margin::same(spacing.xl as i8);
    style.spacing.menu_margin = egui::Margin::symmetric(spacing.lg as i8, spacing.sm as i8);
    style.spacing.item_spacing = egui::vec2(spacing.lg, spacing.sm);
    ctx.set_global_style(style);

    // Store the theme so components resolve their colours from it while painting.
    context::set_active_theme(ctx, theme.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_control_outlines_are_readable_in_every_preset() {
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
            let (panel, hovered) = (visuals.panel_fill, visuals.widgets.hovered.bg_fill);
            check("hover fill", hovered, "panels", panel, 1.2);
            let pressed_min = contrast(hovered, panel) + 0.05;
            check(
                "pressed fill",
                visuals.widgets.active.bg_fill,
                "panels",
                panel,
                pressed_min,
            );
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
            ] {
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
}
