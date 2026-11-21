//! Card, badge and anchor colours of the forecast tool.

use super::Drawing;
use crate::theme::ChartColors;
use egui::Color32;
use serde::{Deserialize, Serialize};

/// Colours of a forecast's start and target cards, its SUCCESS, FAILED and PENDING badges and
/// its anchor dots, as unmultiplied RGBA, and how far their backgrounds fade. The default is
/// blue cards with a blue border and white text, a green SUCCESS and a red FAILED badge with
/// white text, a pale yellow PENDING badge with brown text while undecided, dark grey anchor
/// dots and backgrounds at 90% opacity. An unset card or badge colour takes the active
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
    /// Background of the PENDING badge, shown while the target bar is still ahead or forming.
    pub pending_background: Option<[u8; 4]>,
    /// Text colour of the PENDING badge.
    pub pending_text: Option<[u8; 4]>,
    /// Fill of the dots at the start and target anchors; `None` takes the forecast's line colour.
    pub anchor_color: Option<[u8; 4]>,
    /// How far the card and badge backgrounds fade, in percent: 0 paints them as they are and
    /// 100, or more, hides them. Borders and text keep their colours.
    pub transparency: u8,
}

impl Default for ForecastStyle {
    /// Blue cards with a blue border and white text, a green SUCCESS and a red FAILED badge
    /// with white text, a pale yellow PENDING badge with brown text while undecided, dark grey
    /// anchor dots and backgrounds at 90% opacity.
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
            pending_background: Some([234, 210, 137, 230]),
            pending_text: Some([109, 77, 34, 255]),
            anchor_color: Some([32, 32, 32, 255]),
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
    /// and decided badge, as such forecasts were drawn, and the default pending badge and
    /// anchor colours, which these forecasts did not have.
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
            ..Self::default()
        }
    }

    /// The colours of the start card, the target card, the success, failure and pending badges,
    /// in this order, with unset ones taken from `chart`. Badges have no border.
    pub(crate) fn cards(&self, chart: &ChartColors) -> [CardColors; 5] {
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
            card(
                self.pending_background,
                self.pending_text,
                None,
                chart.axis_text,
            ),
        ]
    }
}

impl Drawing {
    /// The card, badge and anchor colours this forecast is drawn with: its own, or for one
    /// saved without them, its former cards and decided badges with the new defaults otherwise.
    pub(crate) fn forecast_style(&self) -> ForecastStyle {
        self.forecast.unwrap_or_else(|| ForecastStyle::legacy(self))
    }
}
