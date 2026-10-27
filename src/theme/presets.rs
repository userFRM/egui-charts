//! The dark, light and high-contrast themes.

use egui::Color32;

use super::{ChartColors, Radii, Spacing, TextSizes, Theme, UiColors};

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Every preset with its name: "dark", "light" and "high-contrast".
pub fn presets() -> [(&'static str, Theme); 3] {
    [
        ("dark", Theme::dark()),
        ("light", Theme::light()),
        ("high-contrast", Theme::high_contrast()),
    ]
}

impl Theme {
    /// Dark UI and chart. This is the default theme.
    pub fn dark() -> Self {
        let accent = rgb(41, 98, 255);
        Self {
            dark: true,
            ui: UiColors {
                panel_bg: Color32::BLACK,
                panel_bg_secondary: rgb(55, 59, 70),
                panel_bg_floating: rgb(42, 46, 57),
                modal_overlay: Color32::from_black_alpha(180),
                text: rgb(219, 219, 219),
                text_secondary: rgb(184, 184, 184),
                text_muted: rgb(166, 166, 166),
                text_disabled: rgb(87, 87, 87),
                border: rgb(42, 42, 42),
                border_control: rgb(144, 144, 144),
                btn_bg_hover: rgb(32, 32, 32),
                btn_bg_active: rgb(48, 48, 48),
                selection_bg: accent,
                selection_text: Color32::WHITE,
                focus: Color32::WHITE,
                accent,
                accent_hover: rgb(35, 88, 230),
                accent_active: rgb(31, 78, 205),
                accent_text: Color32::WHITE,
                danger_bg: rgb(211, 47, 47),
                danger_bg_hover: rgb(198, 40, 40),
                danger_bg_active: rgb(183, 28, 28),
                danger_text: Color32::WHITE,
                success: rgb(88, 187, 92),
                take_profit: rgb(0, 166, 147),
                stop_loss: rgb(255, 166, 0),
                warning: rgb(255, 152, 0),
                error: rgb(255, 130, 125),
                info: rgb(100, 181, 246),
            },
            chart: ChartColors {
                bg: Color32::BLACK,
                bg_axis: Color32::BLACK,
                bg_tooltip: rgb(55, 59, 70),
                grid_line: rgb(34, 34, 34),
                grid_line_major: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                axis_text: Color32::WHITE,
                axis_text_secondary: rgb(184, 184, 184),
                crosshair_line: Color32::from_rgba_unmultiplied(178, 181, 190, 120),
                crosshair_label_bg: rgb(55, 59, 70),
                crosshair_label_text: rgb(209, 212, 220),
                candle_up: rgb(8, 153, 129),
                candle_down: rgb(242, 54, 69),
                volume_alpha: 153,
            },
            text: TextSizes::default(),
            spacing: Spacing::default(),
            radius: Radii::default(),
        }
    }

    /// Light UI and chart.
    pub fn light() -> Self {
        let dark = Self::dark();
        Self {
            dark: false,
            ui: UiColors {
                panel_bg: Color32::WHITE,
                panel_bg_secondary: rgb(235, 235, 235),
                panel_bg_floating: Color32::WHITE,
                modal_overlay: Color32::from_black_alpha(96),
                text: rgb(26, 26, 26),
                text_secondary: rgb(51, 51, 51),
                text_muted: rgb(102, 102, 102),
                text_disabled: rgb(153, 153, 153),
                border: rgb(235, 235, 235),
                border_control: rgb(130, 130, 130),
                btn_bg_hover: rgb(234, 234, 234),
                btn_bg_active: rgb(214, 214, 214),
                accent: rgb(41, 98, 255),
                selection_bg: rgb(219, 229, 255),
                selection_text: dark.ui.accent_active,
                focus: dark.ui.accent_active,
                success: rgb(40, 115, 45),
                take_profit: rgb(0, 166, 147),
                stop_loss: rgb(255, 166, 0),
                warning: rgb(160, 75, 8),
                error: rgb(198, 40, 40),
                info: rgb(2, 119, 189),
                ..dark.ui
            },
            chart: ChartColors {
                bg: Color32::WHITE,
                bg_axis: Color32::WHITE,
                bg_tooltip: Color32::WHITE,
                grid_line: rgb(209, 209, 209),
                grid_line_major: rgb(209, 209, 209),
                axis_text: rgb(26, 26, 26),
                axis_text_secondary: rgb(51, 51, 51),
                // #9C9C9C on the white chart.
                crosshair_line: Color32::from_rgba_unmultiplied(135, 135, 135, 210),
                crosshair_label_bg: Color32::BLACK,
                crosshair_label_text: Color32::WHITE,
                candle_up: rgb(8, 153, 129),
                candle_down: rgb(242, 54, 69),
                volume_alpha: 128,
            },
            ..dark
        }
    }

    /// Black surfaces with white text, for low vision and bright surroundings.
    ///
    /// Every text colour except disabled text reads at 7:1 or better on panels and
    /// windows, and outlines and the focus ring at 4.5:1 or better.
    pub fn high_contrast() -> Self {
        let dark = Self::dark();
        Self {
            ui: UiColors {
                panel_bg: Color32::BLACK,
                panel_bg_secondary: rgb(18, 18, 18),
                panel_bg_floating: rgb(10, 10, 10),
                modal_overlay: Color32::TRANSPARENT,
                text: Color32::WHITE,
                text_secondary: rgb(235, 235, 235),
                text_muted: rgb(200, 200, 200),
                text_disabled: rgb(140, 140, 140),
                border: rgb(150, 150, 150),
                border_control: rgb(200, 200, 200),
                btn_bg_hover: rgb(45, 45, 45),
                btn_bg_active: rgb(70, 70, 70),
                // A dark selection keeps the selection text, which egui also uses to outline a
                // focused text input, light enough to read on the black panels.
                selection_bg: rgb(41, 71, 102),
                selection_text: Color32::WHITE,
                focus: rgb(255, 214, 0),
                // The shades get lighter, away from the black text on them.
                accent: rgb(102, 178, 255),
                accent_hover: rgb(140, 198, 255),
                accent_active: rgb(179, 218, 255),
                accent_text: Color32::BLACK,
                danger_bg: rgb(255, 120, 120),
                danger_bg_hover: rgb(255, 154, 154),
                danger_bg_active: rgb(255, 188, 188),
                danger_text: Color32::BLACK,
                success: rgb(90, 220, 120),
                take_profit: rgb(0, 166, 147),
                stop_loss: rgb(255, 166, 0),
                warning: rgb(255, 200, 60),
                error: rgb(255, 170, 170),
                info: rgb(120, 200, 255),
            },
            chart: ChartColors {
                bg: Color32::BLACK,
                bg_axis: Color32::BLACK,
                bg_tooltip: rgb(10, 10, 10),
                grid_line: Color32::from_white_alpha(40),
                grid_line_major: Color32::from_white_alpha(64),
                axis_text: Color32::WHITE,
                axis_text_secondary: rgb(200, 200, 200),
                crosshair_line: Color32::WHITE,
                crosshair_label_bg: rgb(45, 45, 45),
                crosshair_label_text: Color32::WHITE,
                candle_up: rgb(0, 220, 130),
                candle_down: rgb(255, 95, 95),
                volume_alpha: 128,
            },
            ..dark
        }
    }
}
