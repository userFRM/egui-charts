//! Watermark support for chart branding and symbol display
//!
//! Provide text and image watermarks with multi-line support,
//! position control, alpha blending, z-order layering, and
//! a manager for handling multiple watermarks simultaneously.

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Painter, Pos2, Rect, TextureHandle, Vec2};
use serde::{Deserialize, Serialize};

/// Watermark position on the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WatermarkPos {
    /// Top-left corner
    TopLeft,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-right corner
    BottomRight,
    /// Center of the chart
    #[default]
    Center,
    /// Custom position (x, y) in pixels from top-left
    Custom { x: i32, y: i32 },
}

/// Horizontal alignment for text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

/// Vertical alignment for text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

/// Options for a single line of text in a text watermark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextWatermarkLineOptions {
    /// The text to display
    pub text: String,
    /// Font size for this line
    pub font_size: f32,
    /// Text color with alpha
    pub color: Color32,
    /// Whether to use bold font
    pub bold: bool,
}

impl TextWatermarkLineOptions {
    /// Create a new text line
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: DESIGN_TOKENS.sizing.watermark.font_size,
            color: Color32::from_rgba_premultiplied(
                DESIGN_TOKENS.semantic.extended.gray.r(),
                DESIGN_TOKENS.semantic.extended.gray.g(),
                DESIGN_TOKENS.semantic.extended.gray.b(),
                DESIGN_TOKENS.semantic.chart.watermark_alpha,
            ),
            bold: true,
        }
    }

    /// Set font size
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Set bold
    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }
}

/// Text watermark options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextWatermark {
    /// Lines of text to display
    pub lines: Vec<TextWatermarkLineOptions>,
    /// Pos on the chart
    pub position: WatermarkPos,
    /// Horizontal alignment
    pub h_align: HorizontalAlign,
    /// Vertical alignment
    pub v_align: VerticalAlign,
    /// Padding from edges (in pixels)
    pub padding: f32,
    /// Line spacing multiplier
    pub line_spacing: f32,
    /// Whether the watermark is visible
    pub visible: bool,
    /// Z-order for layering (higher = on top)
    pub z_order: i32,
}

impl TextWatermark {
    /// Create a new text watermark with a single line
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            lines: vec![TextWatermarkLineOptions::new(text)],
            position: WatermarkPos::Center,
            h_align: HorizontalAlign::Center,
            v_align: VerticalAlign::Center,
            padding: DESIGN_TOKENS.sizing.watermark.padding,
            line_spacing: DESIGN_TOKENS.sizing.watermark.line_spacing,
            visible: true,
            z_order: 0,
        }
    }

    /// Create a multi-line text watermark
    pub fn multi_line(lines: Vec<TextWatermarkLineOptions>) -> Self {
        Self {
            lines,
            position: WatermarkPos::Center,
            h_align: HorizontalAlign::Center,
            v_align: VerticalAlign::Center,
            padding: DESIGN_TOKENS.sizing.watermark.padding,
            line_spacing: DESIGN_TOKENS.sizing.watermark.line_spacing,
            visible: true,
            z_order: 0,
        }
    }

    /// Set position
    pub fn with_pos(mut self, position: WatermarkPos) -> Self {
        self.position = position;
        self
    }

    /// Set horizontal alignment
    pub fn with_h_align(mut self, align: HorizontalAlign) -> Self {
        self.h_align = align;
        self
    }

    /// Set vertical alignment
    pub fn with_v_align(mut self, align: VerticalAlign) -> Self {
        self.v_align = align;
        self
    }

    /// Set padding
    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Set line spacing
    pub fn with_line_spacing(mut self, spacing: f32) -> Self {
        self.line_spacing = spacing;
        self
    }

    /// Set z-order
    pub fn with_z_order(mut self, z_order: i32) -> Self {
        self.z_order = z_order;
        self
    }

    /// Show the watermark
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the watermark
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Render the text watermark
    pub fn render(&self, painter: &Painter, rect: Rect) {
        if !self.visible || self.lines.is_empty() {
            return;
        }

        // Calculate total height of all lines
        let mut total_height = 0.0;
        let mut line_heights = Vec::new();

        for line in &self.lines {
            let font_id = if line.bold {
                FontId::proportional(line.font_size)
            } else {
                FontId::monospace(line.font_size)
            };

            let galley = painter.layout_no_wrap(line.text.clone(), font_id, line.color);
            let height = galley.size().y;
            line_heights.push(height);
            total_height += height;
        }

        // Add spacing between lines
        if self.lines.len() > 1 {
            total_height +=
                (self.lines.len() - 1) as f32 * line_heights[0] * (self.line_spacing - 1.0);
        }

        // Calculate base position
        let base_pos = self.calculate_base_pos(rect, total_height);

        // Render each line
        let mut curr_y = base_pos.y;

        for (i, line) in self.lines.iter().enumerate() {
            let font_id = if line.bold {
                FontId::proportional(line.font_size)
            } else {
                FontId::monospace(line.font_size)
            };

            let galley = painter.layout_no_wrap(line.text.clone(), font_id.clone(), line.color);
            let line_width = galley.size().x;

            // Calculate x position based on horizontal alignment
            let x = match self.h_align {
                HorizontalAlign::Left => base_pos.x,
                HorizontalAlign::Center => base_pos.x - line_width / 2.0,
                HorizontalAlign::Right => base_pos.x - line_width,
            };

            painter.galley(Pos2::new(x, curr_y), galley, line.color);

            curr_y += line_heights[i];
            if i < self.lines.len() - 1 {
                curr_y += line_heights[i] * (self.line_spacing - 1.0);
            }
        }
    }

    /// Calculate the base position for rendering
    fn calculate_base_pos(&self, rect: Rect, total_height: f32) -> Pos2 {
        let (base_x, base_y) = match self.position {
            WatermarkPos::TopLeft => (rect.min.x + self.padding, rect.min.y + self.padding),
            WatermarkPos::TopRight => (rect.max.x - self.padding, rect.min.y + self.padding),
            WatermarkPos::BottomLeft => (rect.min.x + self.padding, rect.max.y - self.padding),
            WatermarkPos::BottomRight => (rect.max.x - self.padding, rect.max.y - self.padding),
            WatermarkPos::Center => (rect.center().x, rect.center().y),
            WatermarkPos::Custom { x, y } => (rect.min.x + x as f32, rect.min.y + y as f32),
        };

        // Adjust for vertical alignment
        let y = match self.v_align {
            VerticalAlign::Top => base_y,
            VerticalAlign::Center => base_y - total_height / 2.0,
            VerticalAlign::Bottom => base_y - total_height,
        };

        Pos2::new(base_x, y)
    }
}

/// Image scaling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageScaleMode {
    /// Fit the image within the bounds, maintaining aspect ratio
    Fit,
    /// Stretch the image to fill the bounds
    Stretch,
    /// Tile the image
    Tile,
    /// Original size (no scaling)
    Original,
}

/// Image watermark options
#[derive(Clone)]
pub struct ImageWatermark {
    /// The image texture
    pub texture: Option<TextureHandle>,
    /// Pos on the chart
    pub position: WatermarkPos,
    /// Target size (width, height) in pixels
    pub size: Option<Vec2>,
    /// Alpha transparency (0.0 = transparent, 1.0 = opaque)
    pub alpha: f32,
    /// Scaling mode
    pub scale_mode: ImageScaleMode,
    /// Padding from edges (in pixels)
    pub padding: f32,
    /// Whether the watermark is visible
    pub visible: bool,
    /// Z-order for layering (higher = on top)
    pub z_order: i32,
}

impl ImageWatermark {
    /// Create a new image watermark
    pub fn new() -> Self {
        Self {
            texture: None,
            position: WatermarkPos::Center,
            size: None,
            alpha: 0.2,
            scale_mode: ImageScaleMode::Fit,
            padding: DESIGN_TOKENS.sizing.watermark.padding,
            visible: true,
            z_order: 0,
        }
    }

    /// Set the texture
    pub fn with_texture(mut self, texture: TextureHandle) -> Self {
        self.texture = Some(texture);
        self
    }

    /// Set position
    pub fn with_pos(mut self, position: WatermarkPos) -> Self {
        self.position = position;
        self
    }

    /// Set target size
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Some(Vec2::new(width, height));
        self
    }

    /// Set alpha
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha.clamp(0.0, 1.0);
        self
    }

    /// Set scale mode
    pub fn with_scale_mode(mut self, mode: ImageScaleMode) -> Self {
        self.scale_mode = mode;
        self
    }

    /// Set padding
    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Set z-order
    pub fn with_z_order(mut self, z_order: i32) -> Self {
        self.z_order = z_order;
        self
    }

    /// Show the watermark
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the watermark
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Render the image watermark
    pub fn render(&self, painter: &Painter, rect: Rect) {
        if !self.visible {
            return;
        }

        let Some(texture) = &self.texture else {
            return;
        };

        let texture_size = texture.size_vec2();

        // Calculate display size based on scale mode
        let display_size = match self.scale_mode {
            ImageScaleMode::Original => texture_size,
            ImageScaleMode::Fit => {
                let target = self.size.unwrap_or(texture_size);
                let scale = (target.x / texture_size.x).min(target.y / texture_size.y);
                texture_size * scale
            }
            ImageScaleMode::Stretch => self.size.unwrap_or(texture_size),
            ImageScaleMode::Tile => texture_size, // Handled separately
        };

        // Calculate position
        let pos = self.calculate_pos(rect, display_size);

        // Calculate image rect
        let image_rect = Rect::from_min_size(pos, display_size);

        // Apply alpha tint
        let tint = Color32::from_rgba_premultiplied(255, 255, 255, (self.alpha * 255.0) as u8);

        // Render image
        match self.scale_mode {
            ImageScaleMode::Tile => {
                // Tile the image
                let mut x = pos.x;
                while x < rect.max.x {
                    let mut y = pos.y;
                    while y < rect.max.y {
                        let tile_rect = Rect::from_min_size(Pos2::new(x, y), display_size);
                        painter.image(
                            texture.id(),
                            tile_rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            tint,
                        );
                        y += display_size.y;
                    }
                    x += display_size.x;
                }
            }
            _ => {
                painter.image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    tint,
                );
            }
        }
    }

    /// Calculate the position for rendering
    fn calculate_pos(&self, rect: Rect, image_size: Vec2) -> Pos2 {
        match self.position {
            WatermarkPos::TopLeft => {
                Pos2::new(rect.min.x + self.padding, rect.min.y + self.padding)
            }
            WatermarkPos::TopRight => Pos2::new(
                rect.max.x - self.padding - image_size.x,
                rect.min.y + self.padding,
            ),
            WatermarkPos::BottomLeft => Pos2::new(
                rect.min.x + self.padding,
                rect.max.y - self.padding - image_size.y,
            ),
            WatermarkPos::BottomRight => Pos2::new(
                rect.max.x - self.padding - image_size.x,
                rect.max.y - self.padding - image_size.y,
            ),
            WatermarkPos::Center => Pos2::new(
                rect.center().x - image_size.x / 2.0,
                rect.center().y - image_size.y / 2.0,
            ),
            WatermarkPos::Custom { x, y } => {
                Pos2::new(rect.min.x + x as f32, rect.min.y + y as f32)
            }
        }
    }
}

impl Default for ImageWatermark {
    fn default() -> Self {
        Self::new()
    }
}

/// Watermark type enum for manager
#[derive(Clone)]
pub enum Watermark {
    Text(TextWatermark),
    Image(ImageWatermark),
}

impl Watermark {
    pub fn z_order(&self) -> i32 {
        match self {
            Watermark::Text(w) => w.z_order,
            Watermark::Image(w) => w.z_order,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Watermark::Text(w) => w.visible,
            Watermark::Image(w) => w.visible,
        }
    }

    pub fn render(&self, painter: &Painter, rect: Rect) {
        match self {
            Watermark::Text(w) => w.render(painter, rect),
            Watermark::Image(w) => w.render(painter, rect),
        }
    }
}

/// Watermark manager for handling multiple watermarks
/// Supports z-order layering and show/hide toggle
pub struct WatermarkManager {
    watermarks: Vec<(String, Watermark)>,
    needs_resort: bool,
}

impl WatermarkManager {
    /// Create a new watermark manager
    pub fn new() -> Self {
        Self {
            watermarks: Vec::new(),
            needs_resort: false,
        }
    }

    /// Add a text watermark
    pub fn add_text(&mut self, id: impl Into<String>, watermark: TextWatermark) {
        self.watermarks
            .push((id.into(), Watermark::Text(watermark)));
        self.needs_resort = true;
    }

    /// Add an image watermark
    pub fn add_image(&mut self, id: impl Into<String>, watermark: ImageWatermark) {
        self.watermarks
            .push((id.into(), Watermark::Image(watermark)));
        self.needs_resort = true;
    }

    /// Remove a watermark by ID
    pub fn remove(&mut self, id: &str) -> bool {
        if let Some(pos) = self.watermarks.iter().position(|(wid, _)| wid == id) {
            self.watermarks.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get a watermark by ID
    pub fn get(&self, id: &str) -> Option<&Watermark> {
        self.watermarks
            .iter()
            .find(|(wid, _)| wid == id)
            .map(|(_, w)| w)
    }

    /// Get a mutable watermark by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Watermark> {
        self.watermarks
            .iter_mut()
            .find(|(wid, _)| wid == id)
            .map(|(_, w)| w)
    }

    /// Show a watermark
    pub fn show(&mut self, id: &str) {
        if let Some((_, watermark)) = self.watermarks.iter_mut().find(|(wid, _)| wid == id) {
            match watermark {
                Watermark::Text(w) => w.visible = true,
                Watermark::Image(w) => w.visible = true,
            }
        }
    }

    /// Hide a watermark
    pub fn hide(&mut self, id: &str) {
        if let Some((_, watermark)) = self.watermarks.iter_mut().find(|(wid, _)| wid == id) {
            match watermark {
                Watermark::Text(w) => w.visible = false,
                Watermark::Image(w) => w.visible = false,
            }
        }
    }

    /// Show all watermarks
    pub fn show_all(&mut self) {
        for (_, watermark) in &mut self.watermarks {
            match watermark {
                Watermark::Text(w) => w.visible = true,
                Watermark::Image(w) => w.visible = true,
            }
        }
    }

    /// Hide all watermarks
    pub fn hide_all(&mut self) {
        for (_, watermark) in &mut self.watermarks {
            match watermark {
                Watermark::Text(w) => w.visible = false,
                Watermark::Image(w) => w.visible = false,
            }
        }
    }

    /// Clear all watermarks
    pub fn clear(&mut self) {
        self.watermarks.clear();
    }

    /// Get the number of watermarks
    pub fn count(&self) -> usize {
        self.watermarks.len()
    }

    /// Check if a watermark exists
    pub fn has(&self, id: &str) -> bool {
        self.watermarks.iter().any(|(wid, _)| wid == id)
    }

    /// Render all watermarks (sorted by z-order)
    pub fn render(&mut self, painter: &Painter, rect: Rect) {
        if self.needs_resort {
            self.sort_by_z_order();
        }

        for (_, watermark) in &self.watermarks {
            if watermark.is_visible() {
                watermark.render(painter, rect);
            }
        }
    }

    /// Sort watermarks by z-order (lower values render first)
    fn sort_by_z_order(&mut self) {
        self.watermarks.sort_by_key(|(_, w)| w.z_order());
        self.needs_resort = false;
    }
}

impl Default for WatermarkManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_watermark_creation() {
        let watermark = TextWatermark::new("BTCUSDT")
            .with_pos(WatermarkPos::Center)
            .with_padding(30.0);

        assert_eq!(watermark.lines.len(), 1);
        assert_eq!(watermark.lines[0].text, "BTCUSDT");
        assert_eq!(watermark.padding, 30.0);
        assert!(watermark.visible);
    }

    #[test]
    fn test_multi_line_watermark() {
        let lines = vec![
            TextWatermarkLineOptions::new("Line 1").with_font_size(48.0),
            TextWatermarkLineOptions::new("Line 2").with_font_size(36.0),
        ];

        let watermark = TextWatermark::multi_line(lines);
        assert_eq!(watermark.lines.len(), 2);
    }

    #[test]
    fn test_watermark_visibility() {
        let mut watermark = TextWatermark::new("Test");
        assert!(watermark.visible);

        watermark.hide();
        assert!(!watermark.visible);

        watermark.show();
        assert!(watermark.visible);
    }

    #[test]
    fn test_image_watermark_creation() {
        let watermark = ImageWatermark::new()
            .with_alpha(0.5)
            .with_scale_mode(ImageScaleMode::Fit);

        assert_eq!(watermark.alpha, 0.5);
        assert_eq!(watermark.scale_mode, ImageScaleMode::Fit);
        assert!(watermark.visible);
    }
}
