//! Saved level, background, coefficient and trend line settings of the projection tool.

use super::Drawing;
use crate::drawings::validation::{MAX_PROJECTION_LEVEL, MAX_STROKE_WIDTH};
use crate::model::LineStyle;
use serde::{Deserialize, Serialize};

/// One projected target: the third anchor moved by `coefficient` times the move from the first
/// anchor to the second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionLevel {
    /// Multiple of the measured move, from 0 to 100: 1 projects the whole move.
    pub coefficient: f32,
    /// Whether the level's line, target zone and labels are drawn.
    pub visible: bool,
    /// Colour as RGBA of the level's line, the border of its target zone, its target label and
    /// its coefficient.
    pub color: [u8; 4],
    /// Stroke style of the level's line.
    pub line_style: LineStyle,
    /// Stroke width in points. `None` takes the drawing's width.
    #[serde(default)]
    pub width: Option<f32>,
}

/// The projection's levels, the target zone behind each level, its coefficient labels and the
/// style of its measured leg, which takes the drawing's colour and width.
///
/// ```
/// use egui_charts::drawings::{ProjectionConfig, ProjectionLevel};
/// use egui_charts::model::LineStyle;
///
/// let mut config = ProjectionConfig {
///     show_coefficients: false,
///     trend_line_style: LineStyle::Dashed,
///     ..Default::default()
/// };
/// config.levels.push(ProjectionLevel {
///     coefficient: 1.618,
///     visible: true,
///     color: [242, 54, 69, 255],
///     line_style: LineStyle::Dotted,
///     width: Some(1.0),
/// });
/// config.background_colors[1] = [76, 175, 80, 51];
/// assert!(config.is_valid());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionConfig {
    /// Projected targets, drawn in list order.
    pub levels: Vec<ProjectionLevel>,
    /// Whether each visible level draws its target zone.
    pub show_background: bool,
    /// Colours as RGBA of the zone's half on the side the move comes from, then of its half
    /// past the target. Their alpha is the opacity.
    pub background_colors: [[u8; 4]; 2],
    /// Whether each visible level writes its coefficient beside its target zone.
    pub show_coefficients: bool,
    /// Whether the measured leg from the first anchor to the second, its arrowhead and its
    /// move label are drawn.
    pub trend_visible: bool,
    /// Stroke style of the measured leg.
    pub trend_line_style: LineStyle,
}

impl Default for ProjectionConfig {
    /// The whole move as one solid grey level, a blue zone before the target and a purple one
    /// past it at 20%, coefficients shown and a solid leg.
    fn default() -> Self {
        use super::drawing::{BLUE, GREY, PURPLE};
        let [r, g, b, _] = BLUE;
        Self {
            levels: vec![ProjectionLevel {
                coefficient: 1.0,
                visible: true,
                color: GREY,
                line_style: LineStyle::Solid,
                width: None,
            }],
            show_background: true,
            background_colors: [[r, g, b, 51], [PURPLE[0], PURPLE[1], PURPLE[2], 51]],
            show_coefficients: true,
            trend_visible: true,
            trend_line_style: LineStyle::Solid,
        }
    }
}

impl ProjectionConfig {
    /// Whether every coefficient is between 0 and 100 and every set width above 0 and at most
    /// 100, as saving and the dialog require.
    pub fn is_valid(&self) -> bool {
        self.levels.iter().all(|level| {
            (0.0..=MAX_PROJECTION_LEVEL).contains(&level.coefficient)
                && level.width.is_none_or(|w| w > 0.0 && w <= MAX_STROKE_WIDTH)
        })
    }

    /// The settings a projection saved without them is drawn with: one level at the whole move
    /// in the drawing colour at alpha 150, dashed; a zone in the fill colour, or the drawing
    /// colour at alpha 30, on both sides; no coefficients; the leg in the drawing's line style.
    pub(crate) fn legacy(drawing: &Drawing) -> Self {
        let [r, g, b, _] = drawing.color;
        let zone = drawing.fill_color.unwrap_or([r, g, b, 30]);
        Self {
            levels: vec![ProjectionLevel {
                coefficient: 1.0,
                visible: true,
                color: [r, g, b, 150],
                line_style: LineStyle::Dashed,
                width: None,
            }],
            show_background: true,
            background_colors: [zone, zone],
            show_coefficients: false,
            trend_visible: true,
            trend_line_style: drawing.line_style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{DrawingManager, DrawingTemplate, DrawingToolType};

    /// Templates and saved defaults retain edited settings; a default saved before the field
    /// existed keeps the legacy look, rather than taking new-drawing defaults.
    #[test]
    fn projection_settings_reach_templates_and_tool_defaults() {
        let mut drawing = Drawing::new(0, DrawingToolType::ProjectionTool);
        assert_eq!(drawing.color, [156, 156, 156, 255]);
        assert!(
            !drawing.theme_shade,
            "new projections paint their specified greys"
        );
        let config = drawing.projection.as_mut().unwrap();
        assert_eq!(config.levels[0].color, [128, 128, 128, 255]);
        assert_eq!(config.levels[0].line_style, LineStyle::Solid);
        assert_eq!(config.levels[0].coefficient, 1.0);
        assert_eq!(
            config.background_colors,
            [[41, 98, 255, 51], [156, 39, 176, 51]]
        );
        assert!(config.show_background && config.show_coefficients && config.trend_visible);
        config.levels[0].coefficient = 0.5;
        config.show_background = false;
        let template = DrawingTemplate::from_drawing(&drawing);
        let mut saved = serde_json::to_value(&template).unwrap();
        let mut manager = DrawingManager::new();
        manager.tool_defaults.insert(
            DrawingToolType::ProjectionTool,
            serde_json::from_value(saved.clone()).unwrap(),
        );
        let mut peer = Drawing::new(1, DrawingToolType::ProjectionTool);
        template.apply_to(&mut peer).unwrap();
        assert_eq!(peer.projection, drawing.projection);
        assert_eq!(
            manager
                .new_drawing(1, DrawingToolType::ProjectionTool)
                .projection,
            drawing.projection
        );
        saved["settings"]
            .as_object_mut()
            .unwrap()
            .remove("projection");
        manager.tool_defaults.insert(
            DrawingToolType::ProjectionTool,
            serde_json::from_value(saved).unwrap(),
        );
        assert_eq!(
            manager
                .new_drawing(2, DrawingToolType::ProjectionTool)
                .projection,
            None
        );
    }
}
