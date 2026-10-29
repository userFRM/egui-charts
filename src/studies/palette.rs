//! Theme-independent default colours for study plots.
//!
//! UI palette changes do not recolour existing studies. Hosts override plots through
//! [`Indicator::set_colors`](super::Indicator::set_colors) or study settings.

use egui::Color32;

/// Default accent study colour.
pub const ACCENT: Color32 = Color32::from_rgb(50, 107, 250);

/// Default bb middle study colour.
pub const BB_MIDDLE: Color32 = Color32::from_rgb(224, 128, 36);

/// Default blue study colour.
pub const BLUE: Color32 = Color32::from_rgb(50, 107, 250);

/// Default brown study colour.
pub const BROWN: Color32 = Color32::from_rgb(151, 118, 97);

/// Default cyan study colour.
pub const CYAN: Color32 = Color32::from_rgb(54, 150, 170);

/// Default dark green study colour.
pub const DARK_GREEN: Color32 = Color32::from_rgb(46, 128, 112);

/// Default dark red study colour.
pub const DARK_RED: Color32 = Color32::from_rgb(191, 78, 90);

/// Default deep green study colour.
pub const DEEP_GREEN: Color32 = Color32::from_rgb(49, 116, 103);

/// Default deep orange study colour.
pub const DEEP_ORANGE: Color32 = Color32::from_rgb(220, 104, 36);

/// Default deep purple study colour.
pub const DEEP_PURPLE: Color32 = Color32::from_rgb(137, 80, 216);

/// Default deep red study colour.
pub const DEEP_RED: Color32 = Color32::from_rgb(171, 71, 83);

/// Default disabled study colour.
pub const DISABLED: Color32 = Color32::from_rgb(151, 155, 166);

/// Default down study colour, matching candles in the dark and light themes.
pub const DOWN: Color32 = Color32::from_rgb(242, 54, 69);

/// Default gold study colour.
pub const GOLD: Color32 = Color32::from_rgb(190, 141, 27);

/// Default gray study colour.
pub const GRAY: Color32 = Color32::from_rgb(128, 133, 145);

/// Default green study colour.
pub const GREEN: Color32 = Color32::from_rgb(8, 153, 129);

/// Default indigo study colour.
pub const INDIGO: Color32 = Color32::from_rgb(111, 119, 190);

/// Default light blue study colour.
pub const LIGHT_BLUE: Color32 = Color32::from_rgb(112, 157, 220);

/// Default light green study colour.
pub const LIGHT_GREEN: Color32 = Color32::from_rgb(95, 184, 168);

/// Default light red study colour.
pub(crate) const LIGHT_RED: Color32 = Color32::from_rgb(247, 139, 148);

/// Default muted study colour.
pub const MUTED: Color32 = Color32::from_rgb(128, 133, 145);

/// Default orange study colour.
pub const ORANGE: Color32 = Color32::from_rgb(224, 128, 36);

/// Default pink study colour.
pub const PINK: Color32 = Color32::from_rgb(189, 110, 151);

/// Default purple study colour.
pub const PURPLE: Color32 = Color32::from_rgb(160, 106, 240);

/// Default red study colour.
pub const RED: Color32 = Color32::from_rgb(242, 54, 69);

/// Default teal study colour.
pub const TEAL: Color32 = Color32::from_rgb(8, 153, 129);

/// Default up study colour, matching candles in the dark and light themes.
pub const UP: Color32 = Color32::from_rgb(8, 153, 129);

/// Default white study colour.
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);

/// Default yellow study colour.
pub const YELLOW: Color32 = Color32::from_rgb(179, 155, 32);
