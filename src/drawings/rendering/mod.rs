//! Rendering implementations for all drawing tool types.
//!
//! This module implements `Drawing::render()` and `Drawing::render_original()`,
//! which dispatch to category-specific rendering methods based on
//! [`DrawingToolType`](crate::drawings::DrawingToolType). Each category is
//! implemented in its own sub-module as `impl Drawing` blocks that add private
//! render methods (e.g., `render_trend_line`, `render_fibonacci`, etc.).
//!
//! # Category sub-modules
//!
//! | Module | Tools | Description |
//! |---|---|---|
//! | `lines` | 9 | Trend lines, rays, extended/horizontal/vertical lines |
//! | `shapes` | 10 | Rectangles, circles, ellipses, triangles, arcs, curves |
//! | `channels` | 4 | Parallel channels, regression, flat top/bottom, disjoint |
//! | `pitchfork` | 4 | Andrews/Schiff/Modified Schiff/Inside pitchforks |
//! | `fibonacci` | 11 | Retracements, extensions, fans, time zones, circles, spiral |
//! | `gann` | 4 | Gann fan, box, square, fixed |
//! | `patterns` | 6 | XABCD, cypher, head & shoulders, ABCD, triangle, three drives |
//! | `elliott` | 5 | Impulse, correction, triangle, double/triple combo waves |
//! | `cycles` | 3 | Cyclic lines, time cycles, sine line |
//! | `annotations` | 11 | Text labels, notes, callouts, price labels, signposts |
//! | `measurements` | 4 | Measure, price range, date range, date+price range |
//! | `trading` | 9 | Long/short positions, forecasts, VWAP, volume profiles |
//! | `media` | 3 | Image/tweet/idea placeholders |
//!
//! # Public API
//!
//! - `DrawingRenderer` trait (in `traits`) -- defines the interface for modular renderers
//! - `Drawing::render()` -- the main entry point, dispatches to `render_original()`
//!
//! # Usage
//!
//! ```ignore
//! use egui_charts::drawings::{Drawing, DrawingToolType};
//!
//! let drawing = Drawing::new(1, DrawingToolType::TrendLine);
//! // After setting up points and calling update_screen_coords:
//! drawing.render(&painter, price_rect);
//! ```

mod annotations;
mod channels;
mod cycles;
mod elliott;
mod fibonacci;
mod gann;
mod lines;
mod measurements;
mod media;
mod patterns;
mod pitchfork;
mod shapes;
mod trading;
pub mod traits;
pub(crate) mod utils;

pub use traits::DrawingRenderer;

use crate::drawings::domain::{Drawing, DrawingToolType};
use egui::Rect;

impl Drawing {
    /// Main rendering dispatcher - routes to category-specific render methods
    ///
    /// This method handles rendering for all 97 drawing tool types,
    /// delegating to the appropriate category-specific implementation.
    pub fn render_original(&self, painter: &egui::Painter, rect: Rect) {
        if self.points.is_empty() {
            return;
        }

        let color = self.color32();
        let stroke = self.stroke();

        match self.tool_type {
            // Simple Lines (9 tools)
            DrawingToolType::TrendLine => self.render_trend_line(painter, color, stroke),
            DrawingToolType::Ray => self.render_ray(painter, rect, color, stroke),
            DrawingToolType::ExtendedLine => {
                self.render_extended_line(painter, rect, color, stroke)
            }
            DrawingToolType::HorizontalLine => self.render_horizontal_line(painter, rect, stroke),
            DrawingToolType::HorizontalRay => {
                self.render_horizontal_ray(painter, rect, color, stroke)
            }
            DrawingToolType::VerticalLine => self.render_vertical_line(painter, rect, stroke),
            DrawingToolType::CrossLine => self.render_cross_line(painter, rect, color, stroke),
            DrawingToolType::TrendAngle => self.render_trend_angle(painter, color, stroke),
            DrawingToolType::InfoLine => self.render_info_line(painter),

            // Shapes (8 tools)
            DrawingToolType::Rect => self.render_rect(painter, color, stroke),
            DrawingToolType::RotatedRect => self.render_rotated_rect(painter),
            DrawingToolType::Circle => self.render_circle(painter, color, stroke),
            DrawingToolType::Ellipse => self.render_ellipse(painter, color, stroke),
            DrawingToolType::Triangle => self.render_triangle(painter, color, stroke),
            DrawingToolType::Arc => self.render_arc(painter, color, stroke),
            DrawingToolType::Polyline => self.render_polyline(painter, color, stroke),
            DrawingToolType::DoubleCurve => self.render_double_curve(painter),

            // Fibonacci (10 tools)
            DrawingToolType::FibonacciRetracement => self.render_fibonacci(painter, rect),
            DrawingToolType::FibonacciExtension => self.render_fibonacci_extension(painter, rect),
            DrawingToolType::FibonacciSpeedResistanceArcs => self.render_fibonacci_arc(painter),
            DrawingToolType::FibonacciTimeZones => self.render_fibonacci_time_zones(painter, rect),
            DrawingToolType::FibonacciChannel => self.render_fibonacci_channel(painter, rect),
            DrawingToolType::FibonacciCircles => self.render_fibonacci_circles(painter),
            DrawingToolType::FibonacciSpeedFan => self.render_fibonacci_speed_fan(painter, rect),
            DrawingToolType::FibonacciSpiral => self.render_fibonacci_spiral(painter),
            DrawingToolType::FibonacciWedge => self.render_fibonacci_wedge(painter, rect),
            DrawingToolType::TrendBasedFibTime => self.render_trend_based_fib_time(painter, rect),

            // Gann (4 tools)
            DrawingToolType::GannFan => self.render_gann_fan(painter, rect),
            DrawingToolType::GannSquare => self.render_gann_square(painter),
            DrawingToolType::GannBox => self.render_gann_box(painter),
            DrawingToolType::GannFixed => self.render_gann_fixed(painter),

            // Pitchforks (4 tools)
            DrawingToolType::Pitchfork => self.render_pitchfork(painter, rect, false),
            DrawingToolType::SchiffPitchfork => self.render_pitchfork(painter, rect, true),
            DrawingToolType::ModifiedSchiffPitchfork => {
                self.render_modified_schiff_pitchfork(painter, rect)
            }
            DrawingToolType::InsidePitchfork => self.render_inside_pitchfork(painter, rect),

            // Channels (4 tools)
            DrawingToolType::ParallelChannel => self.render_parallel_channel(painter),
            DrawingToolType::RegressionTrend => self.render_regression_trend(painter, rect),
            DrawingToolType::FlatTopBottom => self.render_flat_top_bottom(painter),
            DrawingToolType::DisjointChannel => self.render_disjoint_channel(painter),

            // Patterns (6 tools)
            DrawingToolType::XABCDPattern => self.render_xabcd_pattern(painter),
            DrawingToolType::CypherPattern => self.render_cypher_pattern(painter),
            DrawingToolType::HeadAndShoulders => self.render_head_and_shoulders(painter),
            DrawingToolType::ABCDPattern => self.render_abcd_pattern(painter),
            DrawingToolType::TrianglePattern => self.render_triangle_pattern(painter),
            DrawingToolType::ThreeDrivesPattern => self.render_three_drives_pattern(painter),

            // Elliott Wave (5 tools)
            DrawingToolType::ElliottImpulse => self.render_elliott_impulse(painter),
            DrawingToolType::ElliottCorrection => self.render_elliott_correction(painter),
            DrawingToolType::ElliottTriangle => self.render_elliott_triangle(painter),
            DrawingToolType::ElliottDoubleCombo => self.render_elliott_double_combo(painter),
            DrawingToolType::ElliottTripleCombo => self.render_elliott_triple_combo(painter),

            // Cycles (3 tools)
            DrawingToolType::CyclicLines => self.render_cyclic_lines(painter, rect),
            DrawingToolType::TimeCycles => self.render_time_cycles(painter, rect),
            DrawingToolType::SineLine => self.render_sine_line(painter, rect),

            // Projection (6 tools)
            DrawingToolType::LongPos => self.render_pos_tool(painter, true),
            DrawingToolType::ShortPos => self.render_pos_tool(painter, false),
            DrawingToolType::Forecast => self.render_forecast(painter, rect),
            DrawingToolType::BarsPattern => self.render_bars_pattern(painter, rect),
            DrawingToolType::GhostFeed => self.render_ghost_feed(painter),
            DrawingToolType::ProjectionTool => self.render_projection_tool(painter, rect),

            // Volume (3 tools)
            DrawingToolType::AnchoredVWAP => self.render_anchored_vwap(painter, rect),
            DrawingToolType::FixedRangeVolumeProfile => {
                self.render_fixed_range_volume_profile(painter, rect)
            }
            DrawingToolType::AnchoredVolumeProfile => {
                self.render_anchored_volume_profile(painter, rect)
            }

            // Measurers (4 tools)
            DrawingToolType::Measure => self.render_measure(painter),
            DrawingToolType::PriceRange => self.render_price_range(painter, rect),
            DrawingToolType::DateRange => self.render_date_range(painter, rect),
            DrawingToolType::DateAndPriceRange => self.render_date_and_price_range(painter, rect),

            // Annotations (7 tools)
            DrawingToolType::TextLabel => self.render_text_label(painter),
            DrawingToolType::AnchoredText => self.render_anchored_text(painter),
            DrawingToolType::Note => self.render_note(painter),
            DrawingToolType::AnchoredNote => self.render_anchored_note(painter),
            DrawingToolType::Callout => self.render_callout(painter),
            DrawingToolType::PriceLabel => self.render_price_label(painter),
            DrawingToolType::FlagNote => self.render_flag_note(painter),

            // Brushes (3 tools)
            DrawingToolType::Brush => self.render_brush(painter),
            DrawingToolType::Highlighter => self.render_highlighter(painter),
            DrawingToolType::Paintbrush => self.render_brush(painter),

            // Arrows (4 tools)
            DrawingToolType::Arrow => self.render_arrow(painter, color, stroke),
            DrawingToolType::ArrowMarker => self.render_arrow_marker(painter, color),
            DrawingToolType::ArrowMarkUp => self.render_arrow_mark_up(painter),
            DrawingToolType::ArrowMarkDown => self.render_arrow_mark_down(painter),

            // Additional tools
            DrawingToolType::Pitchfan => self.render_pitchfan(painter, rect),
            DrawingToolType::Path => self.render_path(painter),
            DrawingToolType::Curve => self.render_curve(painter),
            DrawingToolType::PriceNote => self.render_price_note(painter, rect),
            DrawingToolType::Table => self.render_table(painter),
            DrawingToolType::Comment => self.render_comment(painter),
            DrawingToolType::Signpost => self.render_signpost(painter, rect),
            DrawingToolType::Image => self.render_image_placeholder(painter),
            DrawingToolType::Tweet => self.render_tweet_placeholder(painter),
            DrawingToolType::Idea => self.render_idea_placeholder(painter),
            DrawingToolType::FontIcon => self.render_font_icon(painter),

            // Cursor tools (4 - not rendered as drawings)
            DrawingToolType::CrossCursor
            | DrawingToolType::DotCursor
            | DrawingToolType::ArrowCursor
            | DrawingToolType::Eraser => {}
        }
    }

    /// Render this drawing using the modular render system
    pub fn render(&self, painter: &egui::Painter, rect: Rect) {
        self.render_original(painter, rect);
    }
}
