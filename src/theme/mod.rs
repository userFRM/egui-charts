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

mod context;
mod presets;
mod style;

pub use context::ThemeExt;
#[cfg(feature = "ui")]
pub(crate) use context::{ThemeDerived, follow_active_theme, theme_derived};
pub(crate) use context::{ThemeScope, applied_theme};
pub use presets::presets;
pub use style::apply_to_egui;
pub(crate) use style::{apply_to_style, contrast, readable_color};

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// The colours and scales of the UI and the chart.
///
/// With the `serde` feature, a theme serialises with any serde format. The `dark`
/// flag and both complete colour palettes are required; omitted text, spacing and
/// radius scales use their defaults. Colours are egui's premultiplied `[r, g, b, a]`
/// arrays.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    /// Whether the UI is dark: selects egui's base visuals and the shadow strength.
    pub dark: bool,
    /// Colours of the UI chrome: panels, text, controls and status.
    pub ui: UiColors,
    /// Colours of the chart surface.
    pub chart: ChartColors,
    /// Type scale and chart text roles, also applied to egui's built-in text styles.
    #[cfg_attr(feature = "serde", serde(default))]
    pub text: TextSizes,
    /// Spacing scale.
    #[cfg_attr(feature = "serde", serde(default))]
    pub spacing: Spacing,
    /// Corner radius scale.
    #[cfg_attr(feature = "serde", serde(default))]
    pub radius: Radii,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Colours of the UI chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UiColors {
    /// Panels, toolbars and inputs.
    pub panel_bg: Color32,
    /// Sidebars, cards and the gaps between panels.
    pub panel_bg_secondary: Color32,
    /// Windows, menus, popups and tooltips.
    pub panel_bg_floating: Color32,
    /// Scrim behind modal dialogs; transparent when the border alone separates them.
    pub modal_overlay: Color32,
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
    /// Take-profit lines, labels and controls.
    #[cfg_attr(feature = "serde", serde(default = "default_take_profit"))]
    pub take_profit: Color32,
    /// Stop-loss lines, labels and controls.
    #[cfg_attr(feature = "serde", serde(default = "default_stop_loss"))]
    pub stop_loss: Color32,
    /// Warning text and icons.
    pub warning: Color32,
    /// Error text and icons.
    pub error: Color32,
    /// Informational text and icons.
    pub info: Color32,
}

#[cfg(feature = "serde")]
fn default_take_profit() -> Color32 {
    Color32::from_rgb(0, 166, 147)
}
#[cfg(feature = "serde")]
fn default_stop_loss() -> Color32 {
    Color32::from_rgb(255, 166, 0)
}

impl Default for UiColors {
    fn default() -> Self {
        Theme::dark().ui
    }
}

/// Colours of the chart surface.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Rising volume bars: the rising candle colour at `volume_alpha`.
    pub fn volume_up(&self) -> Color32 {
        self.candle_up.gamma_multiply_u8(self.volume_alpha)
    }

    /// Falling volume bars: the falling candle colour at `volume_alpha`.
    pub fn volume_down(&self) -> Color32 {
        self.candle_down.gamma_multiply_u8(self.volume_alpha)
    }
}

/// Font sizes, in points.
///
/// Components choose a role; the built-in egui text styles use the same scale.
/// Use `..Default::default()` in struct literals to keep defaults for the other roles.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct TextSizes {
    /// Small badges.
    pub micro: f32,
    /// Chart markers and compact annotations.
    pub marker: f32,
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
    /// Modal dialog titles.
    pub dialog_title: f32,
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
        Self {
            micro: 8.0,
            marker: 9.0,
            dialog_title: 20.0,
            tiny: 10.0,
            small: 11.0,
            label: 12.0,
            body: 13.0,
            title: 14.0,
            heading: 16.0,
            chart_title: 16.0,
            toolbar: 14.0,
            ohlc: 13.0,
            axis: 12.0,
            price_tag: 12.0,
            bottom_bar: 14.0,
        }
    }
}

impl TextSizes {
    /// Every size multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        Self {
            micro: self.micro * factor,
            marker: self.marker * factor,
            tiny: self.tiny * factor,
            small: self.small * factor,
            label: self.label * factor,
            body: self.body * factor,
            title: self.title * factor,
            heading: self.heading * factor,
            dialog_title: self.dialog_title * factor,
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
    /// Extra small, in egui logical points.
    pub xs: f32,
    /// Small, in egui logical points.
    pub sm: f32,
    /// Medium, in egui logical points.
    pub md: f32,
    /// Large, in egui logical points.
    pub lg: f32,
    /// Extra large, in egui logical points.
    pub xl: f32,
    /// Double extra large, in egui logical points.
    pub xxl: f32,
    /// Triple extra large, in egui logical points.
    pub xxxl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: DESIGN_TOKENS.spacing.xs,
            sm: DESIGN_TOKENS.spacing.sm,
            md: DESIGN_TOKENS.spacing.md,
            lg: DESIGN_TOKENS.spacing.lg,
            xl: DESIGN_TOKENS.spacing.xl,
            xxl: DESIGN_TOKENS.spacing.xxl,
            xxxl: DESIGN_TOKENS.spacing.xxxl,
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
    /// Extra small, in egui logical points.
    pub xs: f32,
    /// Small, in egui logical points.
    pub sm: f32,
    /// Medium, in egui logical points.
    pub md: f32,
    /// Large, in egui logical points.
    pub lg: f32,
    /// Extra large, in egui logical points.
    pub xl: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            xs: DESIGN_TOKENS.rounding.xs,
            sm: DESIGN_TOKENS.rounding.sm,
            md: DESIGN_TOKENS.rounding.md,
            lg: DESIGN_TOKENS.rounding.lg,
            xl: DESIGN_TOKENS.rounding.xl,
        }
    }
}

/// The shared buy and sell colours of trading controls and execution marks.
pub(crate) fn trading_side_color(theme: &Theme, side: crate::model::OrderSide) -> Color32 {
    match side {
        crate::model::OrderSide::Buy => theme.ui.accent,
        crate::model::OrderSide::Sell => theme.chart.candle_down,
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
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    #[test]
    fn partial_light_palettes_are_rejected_instead_of_mixing_presets() {
        let partial = serde_json::json!({"dark": false, "ui": {
            "panel_bg": [255, 255, 255, 255],
            "panel_bg_floating": [255, 255, 255, 255],
            "text": [19, 23, 34, 255]
        }});
        assert!(serde_json::from_value::<Theme>(partial).is_err());
        for (_, theme) in presets() {
            let mut saved = serde_json::to_value(&theme).unwrap();
            for scale in ["text", "spacing", "radius"] {
                saved.as_object_mut().unwrap().remove(scale);
            }
            assert_eq!(serde_json::from_value::<Theme>(saved).unwrap(), theme);
        }
    }

    #[test]
    fn saved_text_roles_default_when_missing_and_keep_overrides_when_scaled() {
        for (role, default) in [
            ("micro", 8.0),
            ("marker", 9.0),
            ("dialog_title", 20.0),
            ("chart_title", 16.0),
            ("toolbar", 14.0),
            ("ohlc", 13.0),
            ("axis", 12.0),
            ("price_tag", 12.0),
            ("bottom_bar", 14.0),
        ] {
            for override_size in [None, Some(19.0)] {
                let mut saved = serde_json::to_value(Theme::dark()).unwrap();
                saved["text"] = serde_json::json!({"body": 17.0});
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
}
