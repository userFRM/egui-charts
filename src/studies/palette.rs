//! Theme-independent default colours for study plots.
//!
//! UI palette changes do not recolour existing studies. Hosts override plots through
//! [`Indicator::set_colors`](super::Indicator::set_colors) or study settings.

use egui::Color32;

/// Default accent study colour.
pub const ACCENT: Color32 = Color32::from_rgb(41, 98, 255);

/// Default Bollinger basis colour.
pub const BB_MIDDLE: Color32 = Color32::from_rgb(41, 98, 255);

/// Default blue study colour.
pub const BLUE: Color32 = Color32::from_rgb(41, 98, 255);

/// Default brown study colour.
pub const BROWN: Color32 = Color32::from_rgb(121, 85, 72);

/// Default cyan study colour.
pub const CYAN: Color32 = Color32::from_rgb(38, 198, 218);

/// Default dark green study colour.
pub const DARK_GREEN: Color32 = Color32::from_rgb(67, 160, 71);

/// Default dark red study colour.
pub const DARK_RED: Color32 = Color32::from_rgb(183, 28, 28);

/// Default warm orange study colour.
pub const WARM_ORANGE: Color32 = Color32::from_rgb(255, 152, 0);

/// Default deep purple study colour.
pub const DEEP_PURPLE: Color32 = Color32::from_rgb(103, 58, 183);

/// Default deep red study colour.
pub const DEEP_RED: Color32 = Color32::from_rgb(213, 0, 0);

/// Default disabled study colour.
pub const DISABLED: Color32 = Color32::from_rgb(168, 173, 188);

/// Default down study colour, matching candles in the dark and light themes.
pub const DOWN: Color32 = Color32::from_rgb(242, 54, 69);

/// Default gray study colour.
pub const GRAY: Color32 = Color32::from_rgb(120, 123, 134);

/// Default green study colour.
pub const GREEN: Color32 = Color32::from_rgb(76, 175, 80);

/// Default light blue study colour.
pub const LIGHT_BLUE: Color32 = Color32::from_rgb(33, 150, 243);

/// Default light green study colour.
pub const LIGHT_GREEN: Color32 = Color32::from_rgb(165, 214, 167);

/// Default light red study colour.
pub const LIGHT_RED: Color32 = Color32::from_rgb(255, 205, 210);

/// Default muted study colour.
pub const MUTED: Color32 = Color32::from_rgb(120, 123, 134);

/// Default orange study colour.
pub const ORANGE: Color32 = Color32::from_rgb(255, 109, 0);

/// Default pink study colour.
pub const PINK: Color32 = Color32::from_rgb(233, 30, 99);

/// Default purple study colour.
pub const PURPLE: Color32 = Color32::from_rgb(126, 87, 194);

/// Default red study colour.
pub const RED: Color32 = Color32::from_rgb(242, 54, 69);

/// Default teal study colour.
pub const TEAL: Color32 = Color32::from_rgb(8, 153, 129);

/// Default up study colour, matching candles in the dark and light themes.
pub const UP: Color32 = Color32::from_rgb(8, 153, 129);

/// Default white study colour.
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);

/// Default yellow study colour.
pub const YELLOW: Color32 = Color32::from_rgb(253, 216, 53);

/// Default light orange study colour.
pub const LIGHT_ORANGE: Color32 = Color32::from_rgb(255, 183, 77);

/// Default histogram teal study colour.
pub const HISTOGRAM_TEAL: Color32 = Color32::from_rgb(38, 166, 154);

/// Default light teal study colour.
pub const LIGHT_TEAL: Color32 = Color32::from_rgb(178, 223, 219);

/// Default histogram red study colour.
pub const HISTOGRAM_RED: Color32 = Color32::from_rgb(255, 82, 82);
