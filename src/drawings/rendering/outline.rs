use super::{gann, lines, patterns, shapes};
use crate::drawings::domain::{Drawing, DrawingToolType};
use egui::{Pos2, Rect};

/// The lines a drawing draws and the convex areas it fills, in screen
/// coordinates, from the geometry its renderer draws.
#[derive(Debug, Default)]
pub(crate) struct Outline {
    pub(crate) lines: Vec<[Pos2; 2]>,
    pub(crate) areas: Vec<Vec<Pos2>>,
    /// The width the freehand brushes are stroked at, which can exceed the
    /// hit distance; zero for the other tools.
    pub(crate) width: f32,
}

impl Drawing {
    /// What this drawing draws, with lines that run to the chart's edge cut at
    /// the edge of `rect`. `None` for the tools not described here (text,
    /// media, the data-driven tools and the Fibonacci arcs, circles, spiral and
    /// wedge, the cycles, the position and projection tools) and below the
    /// anchors a tool needs.
    pub(crate) fn outline(&self, rect: Rect) -> Option<Outline> {
        use DrawingToolType as T;
        let p = self.points.as_slice();
        let path = |closed: bool| {
            let mut lines: Vec<[Pos2; 2]> = p.windows(2).map(|w| [w[0], w[1]]).collect();
            if closed && p.len() >= 3 {
                lines.push([p[p.len() - 1], p[0]]);
            }
            lines
        };
        let corners = |r: Rect| {
            vec![
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ]
        };
        let edges = |c: &[Pos2]| (0..c.len()).map(|i| [c[i], c[(i + 1) % c.len()]]).collect();
        // A stroked and filled box: its border, and its inside.
        let boxed = |outline: &mut Outline, r: Rect| {
            let c = corners(r);
            outline.lines = edges(&c);
            outline.areas.push(c);
        };
        let clip = rect.expand(self.stroke_width + 8.0);
        let filled = self.fill_color.is_some();
        let &first = p.first()?;
        let across = |y: f32| [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)];
        let mut outline = Outline::default();
        match self.tool_type {
            T::HorizontalRay => outline.lines = vec![[first, Pos2::new(rect.max.x, first.y)]],
            T::CrossLine => {
                let down = [
                    Pos2::new(first.x, rect.min.y),
                    Pos2::new(first.x, rect.max.y),
                ];
                outline.lines = vec![across(first.y), down];
            }
            _ => {}
        }
        if !outline.lines.is_empty() {
            return Some(outline);
        }
        let &second = p.get(1)?;
        match self.tool_type {
            T::TrendLine | T::Ray | T::ExtendedLine => {
                let (left, right) = self.trend_line_ends(rect)?;
                outline.lines = vec![[left, right]];
            }
            T::TrendAngle
            | T::InfoLine
            | T::Arrow
            | T::Path
            | T::XABCDPattern
            | T::CypherPattern
            | T::ABCDPattern
            | T::ThreeDrivesPattern
            | T::ElliottImpulse
            | T::ElliottCorrection
            | T::ElliottTriangle
            | T::ElliottDoubleCombo
            | T::ElliottTripleCombo => {
                outline.lines = path(false);
            }
            T::Brush | T::Highlighter => {
                outline.lines = self
                    .brush_points(clip, 1.0)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
                outline.width = self.stroke_width;
            }
            T::Polyline => outline.lines = path(self.closed),
            T::ArrowMarker => match lines::arrow_marker_outline(first, second) {
                Some(arrow) => {
                    outline.lines = edges(&arrow);
                    // Head and shaft are the convex parts needed for containment.
                    outline.areas = vec![vec![arrow[0], arrow[1], arrow[6]], arrow[2..6].to_vec()];
                }
                None => outline.lines = path(false),
            },
            T::HeadAndShoulders => {
                outline.lines = path(false);
                if let Some((neckline, regions)) = patterns::head_and_shoulders_outline(p) {
                    outline.lines.push(neckline);
                    if filled {
                        outline.areas.extend(regions.iter().map(|r| r.to_vec()));
                    }
                }
            }
            T::TrianglePattern => match patterns::triangle_boundaries(p, f32::INFINITY) {
                Some((lines, fill)) => {
                    outline.lines = path(false);
                    outline.lines.extend(lines);
                    if filled {
                        outline.areas.extend(fill);
                    }
                }
                None => outline.lines = path(p.len() == 3),
            },
            T::Sector => {
                let points = self.sector_points(clip, 1.0)?;
                outline.lines = edges(&points);
                if filled {
                    outline.areas.extend(
                        points[1..]
                            .windows(2)
                            .map(|edge| vec![points[0], edge[0], edge[1]]),
                    );
                }
            }
            T::Triangle => {
                let [a, b, c, ..] = *p else { return None };
                outline.lines = vec![[a, b], [b, c], [c, a]];
                if filled {
                    outline.areas.push(vec![a, b, c]);
                }
            }
            T::Measure | T::DateRange | T::GannBox => {
                boxed(&mut outline, Rect::from_two_pos(first, second));
            }
            T::GannSquare | T::GannFixed => {
                let corner = self.gann_square_corner(first, second);
                boxed(&mut outline, Rect::from_two_pos(first, corner));
            }
            T::RotatedRect => {
                let corners = self.rotated_rect_corners()?;
                outline.lines = edges(&corners);
                if filled {
                    outline.areas.push(corners.to_vec());
                }
            }
            T::Rect => boxed(&mut outline, self.rectangle_bounds(rect)?),
            T::PriceRange | T::DateAndPriceRange => {
                boxed(&mut outline, Rect::from_two_pos(first, second));
                let edges = self.rectangle_bounds(rect)?;
                outline.lines.extend(
                    [first.y, second.y]
                        .map(|y| [Pos2::new(edges.min.x, y), Pos2::new(edges.max.x, y)]),
                );
            }
            T::Circle => {
                let radius = first.distance(second);
                let ring = super::style::ellipse_arc(
                    first,
                    egui::Vec2::splat(radius),
                    [0.0, std::f64::consts::TAU],
                    clip,
                    1.0,
                );
                outline.lines = ring.windows(2).map(|w| [w[0], w[1]]).collect();
                if filled {
                    outline.areas.push(ring);
                }
            }
            T::Ellipse => {
                let ring = lines::ellipse_points(first, second, clip, 1.0);
                outline.lines = ring.windows(2).map(|w| [w[0], w[1]]).collect();
                if filled {
                    outline.areas.push(ring);
                }
            }
            T::Arc => {
                outline.lines = self
                    .arc_path(clip, 1.0)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::Curve => {
                let [p0, p1, p2] = self.curve_controls()?;
                outline.lines = shapes::curve_points(p0, p1, p2, clip, 1.0)
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::DoubleCurve => {
                outline.lines = self
                    .double_curve_points(clip, 1.0)?
                    .windows(2)
                    .map(|w| [w[0], w[1]])
                    .collect();
            }
            T::Pitchfork
            | T::SchiffPitchfork
            | T::ModifiedSchiffPitchfork
            | T::InsidePitchfork
            | T::Pitchfan => {
                outline.lines = self.pitchfork_lines(rect)?;
                outline.areas = self.pitchfork_bands(rect);
            }
            T::GannFan => {
                let config = self.gann_config();
                let visible = config.fan_ratios.iter().filter(|l| l.visible);
                outline.lines = gann::gann_fan_lines(first, second, rect, visible.map(|l| l.value));
            }
            T::FlatTopBottom | T::DisjointChannel => return self.wedge_channel_outline(rect),
            T::ParallelChannel => return self.parallel_channel_outline(rect),
            T::FibonacciTimeZones => outline.lines = self.fibonacci_time_zone_lines(rect),
            T::FibonacciWedge => return self.fibonacci_wedge_outline(clip),
            T::FibonacciSpeedFan => outline.lines = self.fibonacci_speed_fan_lines(rect),
            _ => return None,
        }
        if filled && let Some(area) = self.enclosed_area(clip, 1.0) {
            outline.areas.extend(
                shapes::even_odd_trapezoids(&area)
                    .into_iter()
                    .map(Vec::from),
            );
        }
        Some(outline)
    }
}
