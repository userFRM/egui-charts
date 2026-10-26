//! Theme-independent default colours for study plots.
//!
//! UI palette changes do not recolour existing studies. Hosts override plots through
//! [`Indicator::set_colors`](super::Indicator::set_colors) or study settings.

use egui::Color32;

/// Default accent study colour.
pub const ACCENT: Color32 = Color32::from_rgb(41, 98, 255);

/// Default bb middle study colour.
pub const BB_MIDDLE: Color32 = Color32::from_rgb(255, 167, 38);

/// Default blue study colour.
pub const BLUE: Color32 = Color32::from_rgb(33, 150, 243);

/// Default brown study colour.
pub const BROWN: Color32 = Color32::from_rgb(121, 85, 72);

/// Default cyan study colour.
pub const CYAN: Color32 = Color32::from_rgb(0, 188, 212);

/// Default dark green study colour.
pub const DARK_GREEN: Color32 = Color32::from_rgb(46, 125, 50);

/// Default dark red study colour.
pub const DARK_RED: Color32 = Color32::from_rgb(211, 47, 47);

/// Default deep green study colour.
pub const DEEP_GREEN: Color32 = Color32::from_rgb(27, 94, 32);

/// Default deep orange study colour.
pub const DEEP_ORANGE: Color32 = Color32::from_rgb(255, 87, 34);

/// Default deep purple study colour.
pub const DEEP_PURPLE: Color32 = Color32::from_rgb(103, 58, 183);

/// Default deep red study colour.
pub const DEEP_RED: Color32 = Color32::from_rgb(183, 28, 28);

/// Default disabled study colour.
pub const DISABLED: Color32 = Color32::from_rgb(158, 158, 158);

/// Default down study colour.
pub const DOWN: Color32 = Color32::from_rgb(242, 54, 69);

/// Default gold study colour.
pub const GOLD: Color32 = Color32::from_rgb(255, 193, 7);

/// Default gray study colour.
pub const GRAY: Color32 = Color32::from_rgb(128, 128, 128);

/// Default green study colour.
pub const GREEN: Color32 = Color32::from_rgb(76, 175, 80);

/// Default indigo study colour.
pub const INDIGO: Color32 = Color32::from_rgb(63, 81, 181);

/// Default light blue study colour.
pub const LIGHT_BLUE: Color32 = Color32::from_rgb(100, 181, 246);

/// Default light green study colour.
pub const LIGHT_GREEN: Color32 = Color32::from_rgb(139, 195, 74);

/// Default muted study colour.
pub const MUTED: Color32 = Color32::from_rgb(131, 137, 151);

/// Default orange study colour.
pub const ORANGE: Color32 = Color32::from_rgb(255, 152, 0);

/// Default pink study colour.
pub const PINK: Color32 = Color32::from_rgb(233, 30, 99);

/// Default purple study colour.
pub const PURPLE: Color32 = Color32::from_rgb(156, 39, 176);

/// Default red study colour.
pub const RED: Color32 = Color32::from_rgb(244, 67, 54);

/// Default teal study colour.
pub const TEAL: Color32 = Color32::from_rgb(0, 150, 136);

/// Default up study colour.
pub const UP: Color32 = Color32::from_rgb(8, 153, 129);

/// Default white study colour.
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);

/// Default yellow study colour.
pub const YELLOW: Color32 = Color32::from_rgb(255, 235, 59);
