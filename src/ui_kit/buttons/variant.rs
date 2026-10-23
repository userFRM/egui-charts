//! ButtonVariant - Button emphasis levels
//!
//! Provides semantic button variants that communicate different levels
//! of importance and action types.

use egui::{Color32, Ui};

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;

/// Button variant for different emphasis levels
///
/// Semantic variants communicate importance and action type visually.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// High emphasis - inverse colors (Save, Submit, Confirm)
    ///
    /// Use for primary actions that are the main call-to-action on a screen.
    Primary,

    /// Medium emphasis - subtle background (Cancel, Reset)
    ///
    /// Use for secondary actions that support the primary action.
    Secondary,

    /// Low emphasis - transparent background (default toolbar buttons)
    ///
    /// Use for tertiary actions or when you need buttons to blend in.
    #[default]
    Ghost,

    /// Border stroke - alternative to Secondary
    ///
    /// Use when you need a button that's more prominent than Ghost
    /// but less prominent than Secondary.
    Outlined,

    /// Destructive action - red colors (Delete, Remove)
    ///
    /// Use for actions that are destructive or irreversible.
    Danger,
}

impl ButtonVariant {
    /// Get the background color for this variant in the given state.
    ///
    /// Primary and Danger use the theme's opaque state shades. Ghost and Outlined
    /// share egui's hover and pressed fills; Secondary retains its filled state ladder.
    /// In a disabled `ui`, Primary and Danger paint as Secondary so fading keeps
    /// their text readable.
    pub fn bg_color(&self, ui: &Ui, hovered: bool, pressed: bool) -> Color32 {
        let state = |rest, hover, press| {
            if pressed {
                press
            } else if hovered {
                hover
            } else {
                rest
            }
        };
        match self {
            Self::Primary if ui.is_enabled() => {
                let colors = ui.active_theme().ui;
                return state(colors.accent, colors.accent_hover, colors.accent_active);
            }
            Self::Danger if ui.is_enabled() => {
                let colors = ui.active_theme().ui;
                return state(
                    colors.danger_bg,
                    colors.danger_bg_hover,
                    colors.danger_bg_active,
                );
            }
            Self::Ghost | Self::Outlined => {
                return state(
                    Color32::TRANSPARENT,
                    ui.visuals().widgets.hovered.bg_fill,
                    ui.visuals().widgets.active.bg_fill,
                );
            }
            Self::Secondary | Self::Primary | Self::Danger => {}
        }
        let steps = 1 + u8::from(hovered || pressed) + u8::from(pressed);
        Color32::TRANSPARENT.lerp_to_gamma(ui.visuals().strong_text_color(), 0.1 * f32::from(steps))
    }

    /// Get the foreground (text/icon) color for this variant
    pub fn fg_color(&self, ui: &Ui) -> Color32 {
        match self {
            Self::Primary if ui.is_enabled() => ui.active_theme().ui.accent_text,
            Self::Danger if ui.is_enabled() => ui.active_theme().ui.danger_text,
            _ => ui.visuals().widgets.inactive.fg_stroke.color,
        }
    }

    /// Get the border color for this variant: Outlined, and a disabled Primary, which keeps an
    /// accent outline so a disabled selected button still reads as selected
    pub fn border_color(&self, ui: &Ui) -> Option<Color32> {
        match self {
            Self::Outlined => Some(ui.visuals().widgets.inactive.fg_stroke.color),
            Self::Primary if !ui.is_enabled() => Some(ui.active_theme().ui.accent),
            _ => None,
        }
    }

    /// Apply this variant's styling to `ui`'s widget visuals
    ///
    /// Call it inside `ui.scope` so the style is restored after rendering the button.
    pub fn apply_to_style(&self, ui: &mut Ui, hovered: bool, active: bool) {
        let bg = self.bg_color(ui, hovered, active);
        let fg = if self.is_inverse() {
            self.fg_color(ui)
        } else if active {
            ui.visuals().widgets.active.fg_stroke.color
        } else if hovered {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            self.fg_color(ui)
        };
        let border = self.border_color(ui);

        let widgets = &mut ui.visuals_mut().widgets;
        for w in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            w.bg_fill = bg;
            w.weak_bg_fill = bg;
            w.fg_stroke.color = fg;
            if let Some(border) = border {
                w.bg_stroke = egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, border);
            }
        }
    }

    /// Check if this variant uses inverse colors (light text on dark bg)
    pub fn is_inverse(&self) -> bool {
        matches!(self, Self::Primary | Self::Danger)
    }

    /// Check if this variant has a visible background
    pub fn has_background(&self) -> bool {
        !matches!(self, Self::Ghost | Self::Outlined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, apply_to_egui, contrast, presets};

    #[test]
    fn neutral_buttons_share_widget_fills_and_resting_foreground() {
        for (_, theme) in presets() {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                // A host may distinguish resting controls from strong body text.
                ui.visuals_mut().widgets.inactive.fg_stroke.color =
                    Color32::from_rgb(150, 160, 170);
                for variant in [ButtonVariant::Ghost, ButtonVariant::Outlined] {
                    assert_eq!(
                        variant.fg_color(ui),
                        ui.visuals().widgets.inactive.fg_stroke.color
                    );
                    assert_eq!(variant.bg_color(ui, false, false), Color32::TRANSPARENT);
                    for (hovered, pressed, expected) in [
                        (true, false, ui.visuals().widgets.hovered.bg_fill),
                        (false, true, ui.visuals().widgets.active.bg_fill),
                        (true, true, ui.visuals().widgets.active.bg_fill),
                    ] {
                        assert_eq!(variant.bg_color(ui, hovered, pressed), expected);
                    }
                }
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn primary_and_danger_text_reads_on_the_fill_in_every_state_and_preset() {
        let mut failures = Vec::new();
        // The presets all have `danger_text == accent_text`; an orange accent makes them differ.
        let orange = Theme::light().with_accent(Color32::from_rgb(255, 120, 0));
        for (name, theme) in presets()
            .into_iter()
            .chain([("light, orange accent", orange)])
        {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            ctx.run_ui(Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    // Disabled buttons are exempt from 4.5:1 but must stay as legible as other
                    // disabled buttons; egui fades everything a disabled `Ui` paints.
                    for (enabled, min) in [(true, 4.5), (false, 3.0)] {
                        ui.add_enabled_ui(enabled, |ui| {
                            let fade = ui.painter().opacity();
                            let surface = ui.visuals().panel_fill;
                            for variant in [ButtonVariant::Primary, ButtonVariant::Danger] {
                                let text = variant.fg_color(ui).gamma_multiply(fade);
                                for (state, hovered, pressed) in [
                                    ("rest", false, false),
                                    ("hovered", true, false),
                                    ("pressed", true, true),
                                ] {
                                    let fill = variant.bg_color(ui, hovered, pressed);
                                    let ratio =
                                        contrast(text, surface.blend(fill.gamma_multiply(fade)));
                                    if ratio < min {
                                        failures.push(format!(
                                            "{name}: {variant:?} {state} (enabled: {enabled}) is \
                                             {ratio:.2}:1, below {min}:1"
                                        ));
                                    }
                                }
                            }
                        });
                    }
                });
            })
            .drop_without_applying_deltas();
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
