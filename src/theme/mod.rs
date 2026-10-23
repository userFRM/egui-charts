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
    /// Type scale and chart text roles. Not applied to egui's text styles.
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
    /// Secondary labels.
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
    /// Primary action fill and active icon colour; links derive a readable shade.
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
    /// Opacity of the volume colours returned by [`Self::volume_up`] and [`Self::volume_down`].
    pub volume_alpha: u8,
}

impl Default for ChartColors {
    fn default() -> Self {
        Theme::dark().chart
    }
}

impl ChartColors {
    /// Rising volume bars: #26a69a at `volume_alpha`.
    pub fn volume_up(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(38, 166, 154, self.volume_alpha)
    }

    /// Falling volume bars: #ef5350 at `volume_alpha`.
    pub fn volume_down(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(239, 83, 80, self.volume_alpha)
    }
}

/// Font sizes, in points.
///
/// Components choose which role to use; setting a role does not change egui's text styles.
/// Use `..Default::default()` in struct literals to keep defaults for the other roles.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct TextSizes {
    /// Badges and footnotes.
    pub tiny: f32,
    /// Secondary labels.
    pub small: f32,
    /// Control labels and axis text.
    pub label: f32,
    /// Body text and buttons.
    pub body: f32,
    /// Panel and dialog titles.
    pub title: f32,
    /// Section headings.
    pub heading: f32,
    /// In-pane chart title, independently of its OHLC values.
    pub chart_title: f32,
    /// Toolbar labels, including Save.
    pub toolbar: f32,
    /// OHLC values and supplementary legend rows.
    pub ohlc: f32,
    /// Price and time axis labels.
    pub axis: f32,
    /// Price tags on the axis.
    pub price_tag: f32,
    /// Bottom range and status bar labels.
    pub bottom_bar: f32,
}

impl Default for TextSizes {
    fn default() -> Self {
        use crate::styles::typography;

        Self {
            tiny: 10.0,
            small: 11.0,
            label: 12.0,
            body: 13.0,
            title: 14.0,
            heading: 16.0,
            chart_title: typography::CHART_TITLE,
            toolbar: typography::TOOLBAR,
            ohlc: typography::OHLC,
            axis: typography::AXIS,
            price_tag: typography::PRICE_TAG,
            bottom_bar: typography::BOTTOM_BAR,
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
            chart_title: self.chart_title * factor,
            toolbar: self.toolbar * factor,
            ohlc: self.ohlc * factor,
            axis: self.axis * factor,
            price_tag: self.price_tag * factor,
            bottom_bar: self.bottom_bar * factor,
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
    /// Also derives opaque hover and pressed shades at least 1.2:1 apart, black or
    /// white text reading at 4.5:1 on every shade, a selection at least 1.3:1 from
    /// the panel, and a light theme's focus ring. These overrides promise 4.5:1
    /// text contrast rather than the high-contrast preset's 7:1.
    pub fn with_accent(mut self, accent: Color32) -> Self {
        let ui = &mut self.ui;
        let accent = ui.panel_bg.blend(accent);
        let (on_accent, away) =
            if contrast(Color32::WHITE, accent) >= contrast(Color32::BLACK, accent) {
                (Color32::WHITE, Color32::BLACK)
            } else {
                (Color32::BLACK, Color32::WHITE)
            };
        // Near black or white, move towards the text instead so both steps have room.
        let towards = if contrast(accent, away) >= 1.5 {
            away
        } else {
            on_accent
        };
        let shade = |fill: Color32| {
            (1..=255)
                .map(|step| fill.lerp_to_gamma(towards, step as f32 / 255.0))
                .find(|&next| contrast(next, fill) >= 1.2 && contrast(on_accent, next) >= 4.5)
                .unwrap_or(fill)
        };
        ui.accent = accent;
        ui.accent_hover = shade(accent);
        ui.accent_active = shade(ui.accent_hover);
        ui.accent_text = on_accent;
        ui.selection_text = if self.dark && ui.text == Color32::WHITE {
            ui.text_secondary
        } else if self.dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        let selection = if self.dark {
            accent
        } else {
            ui.panel_bg.lerp_to_gamma(accent, 1.0 / 6.0)
        };
        let selection = readable_color(selection, ui.text, &[ui.panel_bg], 1.3);
        ui.selection_bg = readable_color(selection, ui.panel_bg, &[ui.selection_text], 4.5);
        if !self.dark {
            ui.focus = readable_color(
                accent,
                ui.text,
                &[
                    ui.panel_bg,
                    ui.panel_bg_secondary,
                    ui.panel_bg_floating,
                    ui.btn_bg_hover,
                ],
                3.0,
            );
        }
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

/// Apply `theme` to egui's visuals and store it for [`ThemeExt::active_theme`].
/// Also install Roboto ahead of the application's proportional fonts.
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
        state.corner_radius = theme.radius.xs.into();
    }

    // egui keeps a dark and a light style and follows the system appearance by default, so
    // style both: the theme then stays when the system switches.
    let spacing = &theme.spacing;
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.window_margin = egui::Margin::same(spacing.xl as i8);
        style.spacing.menu_margin = egui::Margin::symmetric(spacing.lg as i8, spacing.sm as i8);
        style.spacing.item_spacing = egui::vec2(spacing.lg, spacing.sm);
        style.spacing.button_padding = egui::vec2(spacing.sm, DESIGN_TOKENS.spacing.hairline);
        style.spacing.interact_size.y = DESIGN_TOKENS.sizing.control_height;
    });

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

    #[cfg(feature = "serde")]
    #[test]
    fn saved_text_roles_default_when_missing_and_keep_overrides_when_scaled() {
        for (role, default) in [
            ("chart_title", 16.0),
            ("toolbar", 14.0),
            ("ohlc", 13.0),
            ("axis", 12.0),
            ("price_tag", 12.0),
            ("bottom_bar", 14.0),
        ] {
            for override_size in [None, Some(19.0)] {
                let mut saved = serde_json::json!({"text": {"body": 17.0}});
                if let Some(size) = override_size {
                    saved["text"][role] = serde_json::json!(size);
                }
                let mut theme: Theme = serde_json::from_value(saved).unwrap();
                theme.text = theme.text.scaled(1.5);
                let saved = serde_json::to_value(&theme).unwrap();
                assert_eq!(
                    saved["text"][role],
                    serde_json::json!(override_size.unwrap_or(default) * 1.5),
                    "{role}: {override_size:?}"
                );
                assert_eq!(saved["text"]["body"], serde_json::json!(25.5));
                assert_eq!(serde_json::from_value::<Theme>(saved).unwrap(), theme);
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
            ("active toggle", dark.ui.accent, [49, 121, 245]),
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
            let settings = SettingsDialogStyle::from_theme(&theme);
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
                // The dark preset's specified active blue reads at 4.05:1 with white.
                let min = if name == "dark" && fill == "accent" {
                    4.0
                } else {
                    text_min
                };
                check("on-accent text", ui.accent_text, fill, bg, min);
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
                "active settings text",
                settings.tab_text_active,
                "pressed fill",
                settings.tab_bg_active,
                text_min,
            );
            check(
                "active settings icon",
                settings.tab_icon_active,
                "pressed fill",
                settings.tab_bg_active,
                outline_min,
            );
            for (role, stroke) in [
                ("open outline", visuals.widgets.open.bg_stroke),
                ("settings input", settings.input_border),
                ("settings hovered input", settings.input_border_hover),
                ("settings dropdown", settings.dropdown_border),
                ("settings checkbox", settings.checkbox_border),
                ("settings secondary button", settings.btn_secondary_border),
            ] {
                check(role, stroke.color, "panels", panel, outline_min);
            }
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
                {
                    check("muted text", ui.text_muted, surface, bg, text_min);
                    check(
                        "weak text",
                        visuals.weak_text_color(),
                        surface,
                        bg,
                        text_min,
                    );
                }
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
