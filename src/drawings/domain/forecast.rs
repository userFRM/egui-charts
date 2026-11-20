//! Card and badge colours of the forecast tool.

use super::Drawing;
use crate::theme::ChartColors;
use egui::Color32;
use serde::{Deserialize, Serialize};

/// Colours of a forecast's start and target cards and of its SUCCESS and FAILED badges, as
/// unmultiplied RGBA, and how far their backgrounds fade. The default is the look of a new
/// forecast: blue cards with a blue border and white text, a green SUCCESS and a red FAILED
/// badge with white text, and backgrounds at 90% opacity. An unset colour takes the active
/// theme's: the cards write in the axis text colour, the success badge in the rising candle
/// colour and the failure badge in the falling candle colour, all on the axis background. A
/// card has a border only while its border colour is set. The forecast's line keeps the
/// drawing's colour, width and pattern.
///
/// ```
/// use egui_charts::drawings::{Drawing, DrawingToolType, ForecastStyle};
///
/// let mut style = ForecastStyle::default();
/// style.target_background = Some([8, 153, 129, 255]);
/// style.target_border = None;
/// // The theme's axis text colour.
/// style.start_text = None;
/// style.transparency = 25;
/// let mut forecast = Drawing::new(1, DrawingToolType::Forecast);
/// forecast.forecast = Some(style);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ForecastStyle {
    /// Background of the card at the start anchor, which shows its price and time.
    pub start_background: Option<[u8; 4]>,
    /// Text colour of the start card.
    pub start_text: Option<[u8; 4]>,
    /// Border of the start card; `None` draws none.
    pub start_border: Option<[u8; 4]>,
    /// Background of the card at the target, which shows the change, duration, price and time.
    pub target_background: Option<[u8; 4]>,
    /// Text colour of the target card.
    pub target_text: Option<[u8; 4]>,
    /// Border of the target card; `None` draws none.
    pub target_border: Option<[u8; 4]>,
    /// Background of the SUCCESS badge, shown once a bar reaches the target in time.
    pub success_background: Option<[u8; 4]>,
    /// Text colour of the SUCCESS badge.
    pub success_text: Option<[u8; 4]>,
    /// Background of the FAILED badge, shown once the target bar has passed without that.
    pub failure_background: Option<[u8; 4]>,
    /// Text colour of the FAILED badge.
    pub failure_text: Option<[u8; 4]>,
    /// How far the card and badge backgrounds fade, in percent: 0 paints them as they are and
    /// 100, or more, hides them. Borders and text keep their colours.
    pub transparency: u8,
}

impl Default for ForecastStyle {
    /// Blue cards with a blue border and white text, a green SUCCESS and a red FAILED badge
    /// with white text, and backgrounds at 90% opacity.
    fn default() -> Self {
        use super::drawing::{BLUE, GREEN, RED, WHITE, tint};
        Self {
            start_background: Some(tint(BLUE, 230)),
            start_text: Some(WHITE),
            start_border: Some(BLUE),
            target_background: Some(tint(BLUE, 230)),
            target_text: Some(WHITE),
            target_border: Some(BLUE),
            success_background: Some(tint(GREEN, 230)),
            success_text: Some(WHITE),
            failure_background: Some(tint(RED, 230)),
            failure_text: Some(WHITE),
            transparency: 0,
        }
    }
}

/// One card's or badge's colours as painted, before the transparency fades its background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CardColors {
    pub(crate) background: Color32,
    pub(crate) text: Color32,
    pub(crate) border: Option<Color32>,
}

impl ForecastStyle {
    /// The settings a forecast saved without them is drawn with: the theme's colours, no
    /// borders and no fading, with its fill colour, when set, as the background of every card
    /// and badge, as such forecasts were drawn.
    pub(crate) fn legacy(drawing: &Drawing) -> Self {
        let fill = drawing.fill_color;
        Self {
            start_background: fill,
            start_text: None,
            start_border: None,
            target_background: fill,
            target_text: None,
            target_border: None,
            success_background: fill,
            success_text: None,
            failure_background: fill,
            failure_text: None,
            transparency: 0,
        }
    }

    /// The colours of the start card, the target card, the success badge and the failure
    /// badge, in this order, with unset ones taken from `chart`. Badges have no border.
    pub(crate) fn cards(&self, chart: &ChartColors) -> [CardColors; 4] {
        let color = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let card =
            |background: Option<[u8; 4]>, text: Option<[u8; 4]>, border, theme_text| CardColors {
                background: background.map_or(chart.bg_axis, color),
                text: text.map_or(theme_text, color),
                border: Option::map(border, color),
            };
        [
            card(
                self.start_background,
                self.start_text,
                self.start_border,
                chart.axis_text,
            ),
            card(
                self.target_background,
                self.target_text,
                self.target_border,
                chart.axis_text,
            ),
            card(
                self.success_background,
                self.success_text,
                None,
                chart.candle_up,
            ),
            card(
                self.failure_background,
                self.failure_text,
                None,
                chart.candle_down,
            ),
        ]
    }
}

impl Drawing {
    /// The card and badge colours this forecast is drawn with: its own, or for one saved
    /// without them, the colours that draw it as it was drawn.
    pub(crate) fn forecast_style(&self) -> ForecastStyle {
        self.forecast.unwrap_or_else(|| ForecastStyle::legacy(self))
    }
}
