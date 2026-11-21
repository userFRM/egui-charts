//! Validated host creation and editing of timestamp-based drawings.

use super::PaneId;
use crate::Chart;
use crate::drawings::{ChartPoint, Drawing};
use chrono::{DateTime, Utc};

/// Why a host drawing creation or edit changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DrawingError {
    /// No drawing has this id.
    UnknownDrawing(usize),
    /// The drawing with this id is locked.
    Locked(usize),
    /// The price of the anchor at this index, counted from 0, is NaN or infinite.
    NonFinitePrice {
        /// The index of the anchor.
        point: usize,
    },
    /// The drawing was refused; the text says why, such as a disabled or unknown tool,
    /// anchors the tool cannot use or a picture without an address.
    Invalid(&'static str),
}

impl std::fmt::Display for DrawingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDrawing(id) => write!(f, "Unknown drawing {id}"),
            Self::Locked(id) => write!(f, "Drawing {id} is locked"),
            Self::NonFinitePrice { point } => {
                write!(f, "Drawing price of point {point} must be finite")
            }
            Self::Invalid(reason) => f.write_str(reason),
        }
    }
}
impl std::error::Error for DrawingError {}

impl Chart {
    /// Creates a completed drawing from UTC times and prices, with the supplied drawing's
    /// properties, pane, group and layer. Its id and storage identity are replaced.
    /// Returns an error without changing history or drawings for invalid anchors or settings.
    /// Times can precede loaded history; their projection is updated when bars arrive.
    /// Wrap this call in [`Self::user_action`] to include it in chart undo history.
    ///
    /// ```
    /// use egui_charts::{Chart, drawings::{Drawing, DrawingToolType}};
    /// let mut chart = Chart::new(egui_charts::model::BarData::default());
    /// let time = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    /// let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
    /// line.locked = true;
    /// let id = chart.create_drawing(line, &[(time, 100.0)]).unwrap();
    /// assert_eq!(chart.drawing_points(id).unwrap(), vec![(time, 100.0)]);
    /// ```
    pub fn create_drawing(
        &mut self,
        mut drawing: Drawing,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<usize, DrawingError> {
        drawing.chart_points = self.map_drawing_points(points)?;
        self.drawings_mut()
            .add_drawing(drawing)
            .map_err(DrawingError::Invalid)
    }

    /// Returns a drawing's UTC anchors and prices, or `None` for an unknown id or an index
    /// without a time mapping. Retains exact supplied times between bars and in whitespace.
    /// An anchored text, or a table anchored to the screen, reports the time and price under the
    /// point it was last drawn at, which change as the chart scrolls and zooms; while one moved
    /// outside the view waits to be drawn inside its pane, the point it was given.
    pub fn drawing_points(&self, id: usize) -> Option<Vec<(DateTime<Utc>, f64)>> {
        self.drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)?
            .chart_points
            .iter()
            .map(|p| Some((self.drawing_point_time(p)?, p.price)))
            .collect()
    }

    /// Replaces a drawing's anchors as one undoable edit. Invalid points and locked drawings
    /// leave the original untouched. Screen positions are rebuilt on the next frame.
    /// An anchored text, or a table anchored to the screen, moves to the new point and keeps its
    /// place in the pane from the first frame that draws that point inside the pane.
    pub fn set_drawing_points(
        &mut self,
        id: usize,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<(), DrawingError> {
        let mut drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?
            .clone();
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        drawing.chart_points = self.map_drawing_points(points)?;
        self.drawings()
            .validate_drawing(&drawing)
            .map_err(DrawingError::Invalid)?;
        self.drawings_mut()
            .edit_drawing(id, |target| {
                target.chart_points = drawing.chart_points;
                // A pane position would keep the text where it was; the first frame that draws
                // the new point inside the pane takes one there.
                target.screen_anchor = None;
            })
            .map_err(DrawingError::Invalid)
    }

    /// Replaces a trend line's, ray's, extended line's, info line's, trend angle's or arrow's
    /// readouts, midpoint, price labels and text placement as one undoable edit. A horizontal
    /// line or ray and a regression trend draw only the text placement, and a vertical line the
    /// text placement, the text's orientation and the reach across panes. Unknown, locked and
    /// other drawing types, and the drawing's own measurements, leave drawings and history
    /// untouched.
    pub fn set_line_measurements(
        &mut self,
        id: usize,
        measurements: crate::drawings::LineMeasurements,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if !(drawing.tool_type.supports_line_measurements()
            || matches!(
                drawing.tool_type,
                crate::drawings::DrawingToolType::HorizontalLine
                    | crate::drawings::DrawingToolType::HorizontalRay
                    | crate::drawings::DrawingToolType::VerticalLine
                    | crate::drawings::DrawingToolType::RegressionTrend
            ))
        {
            return Err(DrawingError::Invalid(
                "Drawing is not a trend line, ray, extended line, info line, trend angle, arrow, horizontal line, horizontal ray, vertical line or regression trend",
            ));
        }
        if drawing.line_measurements == measurements {
            return Ok(());
        }
        self.user_action(|chart| {
            chart.drawings_mut().edit_drawing(id, |drawing| {
                drawing.line_measurements = measurements;
            })
        })
        .map_err(DrawingError::Invalid)
    }

    /// Replace a retracement's, extension's, channel's, wedge's, circle's, resistance arc's, spiral's,
    /// time projection's or speed resistance fan's Fibonacci settings (levels, typography, text
    /// placement, construction visibility, fan grid and fan labels) as one undoable edit. A fan
    /// given changed settings without time levels takes their price levels as its time levels.
    /// Unknown ids, locked drawings, invalid settings and the drawing's own settings change
    /// nothing.
    pub fn set_fibonacci_config(
        &mut self,
        id: usize,
        mut config: crate::drawings::FibonacciConfig,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if !matches!(
            drawing.tool_type,
            crate::drawings::DrawingToolType::FibonacciRetracement
                | crate::drawings::DrawingToolType::FibonacciCircles
                | crate::drawings::DrawingToolType::FibonacciSpeedResistanceArcs
                | crate::drawings::DrawingToolType::FibonacciSpiral
                | crate::drawings::DrawingToolType::FibonacciExtension
                | crate::drawings::DrawingToolType::FibonacciChannel
                | crate::drawings::DrawingToolType::FibonacciWedge
                | crate::drawings::DrawingToolType::FibonacciTimeZones
                | crate::drawings::DrawingToolType::TrendBasedFibTime
                | crate::drawings::DrawingToolType::FibonacciSpeedFan
        ) {
            return Err(DrawingError::Invalid(
                "Drawing is not a Fibonacci retracement, circle, resistance arc, spiral, extension, channel, wedge, time projection or speed resistance fan",
            ));
        }
        if !config.is_valid() {
            return Err(DrawingError::Invalid("Invalid Fibonacci settings"));
        }
        // A fan given changed settings saved before it had time levels takes the current look.
        if drawing.tool_type == crate::drawings::DrawingToolType::FibonacciSpeedFan
            && config.time_levels.is_none()
            && drawing.fib_config.as_ref() != Some(&config)
        {
            config.time_levels = Some(config.levels.clone());
        }
        if drawing.fib_config.as_ref() != Some(&config) {
            self.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.fib_config = Some(config))
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace an anchored VWAP's settings (source, whether it counts executions, bands, their
    /// unit and line styles, fills, line plot, latest-value tracking and its precision) as one
    /// undoable edit.
    /// Unknown ids, locked drawings, other tools and invalid settings change nothing.
    pub fn set_vwap_config(
        &mut self,
        id: usize,
        config: crate::drawings::VwapConfig,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if drawing.tool_type != crate::drawings::DrawingToolType::AnchoredVWAP {
            return Err(DrawingError::Invalid("Drawing is not an anchored VWAP"));
        }
        if !config.is_valid() {
            return Err(DrawingError::Invalid("Invalid anchored VWAP settings"));
        }
        if drawing.vwap.unwrap_or_default() != config {
            self.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.vwap = Some(config))
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace a regression trend's settings (source, upper and lower bounds and whether each is
    /// a deviation or the extreme price, the visibility and style of each line, the correlation
    /// label and its precision) as one undoable edit. Unknown ids, locked drawings and other tools
    /// change nothing.
    pub fn set_regression_trend_config(
        &mut self,
        id: usize,
        config: crate::drawings::RegressionTrendConfig,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if drawing.tool_type != crate::drawings::DrawingToolType::RegressionTrend {
            return Err(DrawingError::Invalid("Drawing is not a regression trend"));
        }
        if drawing.regression_trend.unwrap_or_default() != config {
            self.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.regression_trend = Some(config))
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace a projection's levels, gradient colours, coefficient labels and outline style
    /// as one undoable edit. Unknown ids, locked drawings, other tools and invalid settings
    /// change nothing.
    pub fn set_projection_config(
        &mut self,
        id: usize,
        config: crate::drawings::ProjectionConfig,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if drawing.tool_type != crate::drawings::DrawingToolType::ProjectionTool {
            return Err(DrawingError::Invalid("Drawing is not a projection"));
        }
        if !config.is_valid() {
            return Err(DrawingError::Invalid("Invalid projection settings"));
        }
        if drawing.projection.as_ref() != Some(&config) {
            self.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.projection = Some(config))
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace a position drawing's statistics, account currency and quantity precision in
    /// one undoable edit. A different currency needs a host-supplied conversion rate before
    /// monetary statistics can be calculated. Invalid settings and locked drawings change nothing.
    pub fn set_position_readout(
        &mut self,
        id: usize,
        readout: crate::drawings::PositionReadout,
    ) -> Result<(), DrawingError> {
        let drawing = self.position_drawing(id)?;
        if !readout.is_valid() {
            return Err(DrawingError::Invalid("Invalid position readout settings"));
        }
        if drawing.position_readout != readout {
            self.user_action(|chart| {
                chart.drawings_mut().edit_drawing(id, |drawing| {
                    drawing.position_readout = readout;
                })
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace a position drawing's lines, labels, zone fill and border, and its target and stop
    /// colours, whose alpha is the opacity of their zones (`None` keeps the defaults), in one
    /// undoable edit. Invalid widths or font sizes and locked drawings change nothing.
    pub fn set_position_style(
        &mut self,
        id: usize,
        style: crate::drawings::PositionStyle,
        target_color: Option<[u8; 4]>,
        stop_color: Option<[u8; 4]>,
    ) -> Result<(), DrawingError> {
        let drawing = self.position_drawing(id)?;
        if !style.is_valid() {
            return Err(DrawingError::Invalid("Invalid position style"));
        }
        if (
            drawing.position_style,
            drawing.position_target_color,
            drawing.position_stop_color,
        ) != (style, target_color, stop_color)
        {
            self.user_action(|chart| {
                chart.drawings_mut().edit_drawing(id, |drawing| {
                    drawing.position_style = style;
                    drawing.position_target_color = target_color;
                    drawing.position_stop_color = stop_color;
                })
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Replace the marker of a pin, a signpost, a horizontal line, a horizontal ray, an up, down,
    /// left or right arrow mark, or a parallel, flat top/bottom or disjoint channel in one
    /// undoable edit. The text switch shows or hides a pin's text popup and the text of the
    /// lines, arrow marks and channels; the glyph, its switch and the disc colour apply to a
    /// signpost. Other drawing tools, such as a trend line or an arrow marker, a glyph that is
    /// empty or longer than 32 bytes, locked drawings and the drawing's own marker change nothing.
    pub fn set_annotation_marker(
        &mut self,
        id: usize,
        marker: crate::drawings::AnnotationMarker,
    ) -> Result<(), DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if !(drawing.tool_type.has_text_switch()
            || matches!(
                drawing.tool_type,
                crate::drawings::DrawingToolType::AnchoredNote
                    | crate::drawings::DrawingToolType::Signpost
            ))
        {
            return Err(DrawingError::Invalid(
                "Drawing is not a pin, signpost, horizontal line, horizontal ray, arrow mark or parallel, flat top/bottom or disjoint channel",
            ));
        }
        if !marker.is_valid() {
            return Err(DrawingError::Invalid("Invalid annotation marker"));
        }
        if drawing.marker != marker {
            self.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.marker = marker)
            })
            .map_err(DrawingError::Invalid)?;
        }
        Ok(())
    }

    /// Exchange a position drawing's target and stop prices and switch its direction as one
    /// undoable edit. Entry, horizontal extent, anchor times and style stay in place. A legacy
    /// two-anchor position gains its mirrored stop before the exchange. Invalid geometry,
    /// unknown ids, other drawing tools and locked drawings leave history untouched.
    pub fn reverse_position_drawing(&mut self, id: usize) -> Result<(), DrawingError> {
        use crate::drawings::DrawingToolType as T;
        let mut drawing = self.position_drawing(id)?.clone();
        let [entry, target, rest @ ..] = drawing.chart_points.as_slice() else {
            return Err(DrawingError::Invalid(
                "Position needs entry and target anchors",
            ));
        };
        let stop_price = rest
            .first()
            .map_or(2.0 * entry.price - target.price, |p| p.price);
        if !stop_price.is_finite() || !entry.price.is_finite() || !target.price.is_finite() {
            return Err(DrawingError::Invalid("Position prices must be finite"));
        }
        if drawing.chart_points.len() == 2 {
            let mut stop = *target;
            stop.price = stop_price;
            drawing.chart_points.push(stop);
        }
        let target_price = drawing.chart_points[1].price;
        drawing.chart_points[1].price = stop_price;
        drawing.chart_points[2].price = target_price;
        drawing.tool_type = if drawing.tool_type == T::LongPos {
            T::ShortPos
        } else {
            T::LongPos
        };
        self.drawings()
            .validate_drawing(&drawing)
            .map_err(DrawingError::Invalid)?;
        drawing.points.clear();
        self.user_action(|chart| {
            chart
                .drawings_mut()
                .edit_drawing(id, |target| *target = drawing)
        })
        .map_err(DrawingError::Invalid)
    }

    fn position_drawing(&self, id: usize) -> Result<&Drawing, DrawingError> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or(DrawingError::UnknownDrawing(id))?;
        if drawing.locked {
            return Err(DrawingError::Locked(id));
        }
        if !matches!(
            drawing.tool_type,
            crate::drawings::DrawingToolType::LongPos | crate::drawings::DrawingToolType::ShortPos
        ) {
            return Err(DrawingError::Invalid("Drawing is not a position"));
        }
        Ok(drawing)
    }

    /// Sets an image drawing's picture as one undoable edit. `uri` is the address the host's
    /// image loaders resolve, and `size` is its pixel size, which gives the frame its
    /// proportions, scaled down to fit 240 x 180 points. Closes this drawing's text editor.
    /// Unknown, locked and non-image drawings, an empty address and a size that is not
    /// positive and finite change nothing. Each frame side is at least one point.
    pub fn set_drawing_image(
        &mut self,
        id: usize,
        uri: impl Into<String>,
        size: egui::Vec2,
    ) -> Result<(), &'static str> {
        let drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or("Unknown drawing")?;
        if drawing.locked {
            return Err("Drawing is locked");
        }
        if drawing.tool_type != crate::drawings::DrawingToolType::Image {
            return Err("Drawing is not an image");
        }
        let uri = uri.into();
        let frame = image_frame(&uri, size)?;
        self.user_action(|chart| {
            let manager = chart.drawings_mut();
            if manager
                .text_editor
                .as_ref()
                .is_some_and(|editor| editor.id == id)
            {
                manager.text_editor = None;
            }
            manager.edit_drawing(id, |drawing| {
                drawing.text = Some(uri);
                drawing.image.get_or_insert_with(Default::default).size = Some(frame.into());
            })
        })
    }

    /// Places a picture as a new image drawing and returns its id: the host's answer to a
    /// [`ShortcutRequest::PasteImage`](crate::widget::ShortcutRequest::PasteImage), or to a
    /// paste without drawing text after it found a picture on its clipboard. The picture's
    /// top-left corner is at the bar slot and price under the pointer when the pointer is over
    /// the price pane, and otherwise at the centre of the price pane as last drawn. `uri` and
    /// `size` are as for [`Self::set_drawing_image`], which sizes the frame the same way. The
    /// drawing is selected, the call is one undoable step and no
    /// [`ChartEvent::ImageRequested`](crate::widget::ChartEvent::ImageRequested) follows.
    ///
    /// Changes nothing and returns [`DrawingError::Invalid`] with the reason for an empty
    /// address, a size that is not positive and finite, a chart not drawn yet, a chart without
    /// bars to date the anchor and a disabled Image tool.
    pub fn paste_image(
        &mut self,
        uri: impl Into<String>,
        size: egui::Vec2,
    ) -> Result<usize, DrawingError> {
        let uri = uri.into();
        let frame = image_frame(&uri, size).map_err(DrawingError::Invalid)?;
        let plot = self.last_rendered_price_rect;
        if !plot.is_positive() {
            return Err(DrawingError::Invalid("Chart has not been drawn"));
        }
        let pos = (self.host_events.pointer.as_ref())
            .map(|pointer| pointer.pos)
            .filter(|pos| plot.contains(*pos))
            .unwrap_or_else(|| plot.center());
        let (time, price) = self
            .price_pane_point(pos)
            .ok_or(DrawingError::Invalid("Chart has no bars to date the image"))?;
        let mut drawing = self
            .drawings
            .new_drawing(0, crate::drawings::DrawingToolType::Image);
        drawing.z_order = self.drawings.top_z_order();
        drawing.text = Some(uri);
        drawing.image.get_or_insert_with(Default::default).size = Some(frame.into());
        let id = self.user_action(|chart| chart.create_drawing(drawing, &[(time, price)]))?;
        self.drawings.select(id);
        Ok(id)
    }

    /// The time and price at `pos` on the price pane as last drawn, the time at the nearest bar
    /// slot. `None` without bars to date it.
    pub(super) fn price_pane_point(&self, pos: egui::Pos2) -> Option<(DateTime<Utc>, f64)> {
        let plot = self.last_rendered_price_rect;
        let index = self
            .state
            .time_scale()
            .coord_to_idx_precise(pos.x, plot.left(), plot.width())
            .round();
        let time = crate::model::time_at(self.state.shown(), index)?;
        Some((time, self.last_price_map.y_to_price(pos.y)))
    }

    /// The pane that draws drawing `id`, or `None` for an unknown drawing.
    pub fn drawing_pane(&self, id: usize) -> Option<PaneId> {
        let drawing = self.drawings().drawings.iter().find(|d| d.id == id)?;
        Some(self.owner_pane(drawing).unwrap_or_else(|| {
            let panes = self.panes();
            let main = self.main_series_pane();
            // The original pane may be gone with the main series.
            if panes.contains(&main) {
                main
            } else {
                panes.first().copied().unwrap_or(main)
            }
        }))
    }

    /// The pane that draws the owner of `drawing`: a pane, or a series or study it draws. `None`
    /// for a drawing without one, which the pane holding the main series draws, or whose owner
    /// no pane draws.
    pub(super) fn owner_pane(&self, drawing: &Drawing) -> Option<PaneId> {
        let owner = drawing.pane?;
        self.pane_layout
            .panes
            .iter()
            .map(|pane| pane.id)
            .find(|pane| pane.0 == owner || self.pane_series_iter(*pane).any(|id| id.0 == owner))
    }

    /// The pane that draws the drawing being placed, once it has an anchor. `None` without one,
    /// or when no pane draws its owner.
    pub(super) fn placement_pane(&self) -> Option<PaneId> {
        let drawing = (self.drawings.curr_drawing.as_ref())
            .filter(|drawing| !drawing.chart_points.is_empty())?;
        match drawing.pane {
            Some(_) => self.owner_pane(drawing),
            None => Some(self.main_series_pane()).filter(|pane| self.panes().contains(pane)),
        }
    }

    /// Cancels the drawing being placed when `pane`, the pane that drew it before a removal or a
    /// history step, no longer draws it, so its anchors never land in another pane. The tool
    /// stays selected.
    pub(super) fn keep_placement_in(&mut self, pane: Option<PaneId>) {
        if pane.is_some() && self.placement_pane() != pane {
            self.drawings.cancel_curr_drawing();
        }
    }

    /// Moves drawing `id` into `pane` as one undoable edit, keeping its anchors and style; the
    /// anchors' prices are read on the new pane's scale. Returns `false` for an unknown drawing
    /// or pane and for a locked drawing.
    pub fn move_drawing_to_pane(&mut self, id: usize, pane: PaneId) -> bool {
        // The pane holding the main series paints the drawings that belong to no pane.
        let owner = if pane == self.main_series_pane() && self.panes().contains(&pane) {
            None
        } else if let Some(pane) = self
            .pane_layout
            .panes
            .iter()
            .find(|item| item.id == pane)
            .filter(|_| self.panes().contains(&pane))
        {
            // The pane's drawings belong to its first member, or to an empty pane itself.
            Some(pane.members.first().map_or(pane.id.0, |id| id.0))
        } else {
            return false;
        };
        let drawings = self.drawings_mut();
        if !drawings.drawings.iter().any(|d| d.id == id && !d.locked) {
            return false;
        }
        drawings.edit_drawing(id, |d| d.pane = owner).is_ok()
    }

    /// Deletes the selected drawings as one undo step, as Delete and Backspace do. Locked drawings,
    /// and every drawing while
    /// [`DrawingManager::set_locked`](crate::drawings::DrawingManager::set_locked) locks them all,
    /// stay selected and in place, and
    /// [`ChartEvent::LockedDrawingsKept`](super::ChartEvent::LockedDrawingsKept) reports how many,
    /// so the host can tell the user why. Drawings whose user edits are off also stay selected
    /// and in place, and are not counted.
    pub fn delete_selected_drawings(&mut self) {
        let ids: Vec<_> = self.drawings.selected_ids().collect();
        self.delete_drawings_keeping_locked(&ids);
    }

    /// Deletes the drawings in `ids` as one undo step. Locked drawings, and every drawing while
    /// [`DrawingManager::set_locked`](crate::drawings::DrawingManager::set_locked) locks them all,
    /// stay selected and in place. Reports how
    /// many it kept with [`ChartEvent::LockedDrawingsKept`](super::ChartEvent::LockedDrawingsKept).
    /// Drawings whose user edits are off also stay selected and in place, and are not counted.
    pub(crate) fn delete_drawings_keeping_locked(&mut self, ids: &[usize]) {
        let all_locked = self.drawings.is_locked();
        let kept = self
            .drawings
            .drawings
            .iter()
            .filter(|d| (d.locked || all_locked) && d.user_edit_enabled && ids.contains(&d.id))
            .count();
        let ids: Vec<_> = self
            .drawings
            .drawings
            .iter()
            .filter(|d| self.drawings.accepts_user_edits(d) && ids.contains(&d.id))
            .map(|d| d.id)
            .collect();
        self.drawings.delete_drawings(&ids);
        if kept > 0 {
            self.push_event(super::ChartEvent::LockedDrawingsKept(kept));
        }
    }

    /// Adds a horizontal line at `price` in the pane under `pos`, the pane holding the main series
    /// when no other pane is there, with the tool's default style, and selects it. Returns its
    /// id, or `None` for a price that is not finite or a disabled tool.
    #[cfg(feature = "ui")]
    pub(crate) fn draw_horizontal_line_at(&mut self, pos: egui::Pos2, price: f64) -> Option<usize> {
        if !price.is_finite() {
            return None;
        }
        let tool = crate::drawings::DrawingToolType::HorizontalLine;
        let mut drawing = self.drawings.new_drawing(0, tool);
        // Another pane's drawings belong to its first member, as in `move_drawing_to_pane`.
        drawing.pane = self
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.id != self.main_series_pane() && pane.rect.contains(pos))
            .map(|pane| pane.members.first().map_or(pane.id.0, |id| id.0));
        drawing.z_order = self.drawings.top_z_order();
        let bar = self.get_chart_mapping().x_to_idx_f64(pos.x).round() as f32;
        drawing.chart_points = vec![ChartPoint::new(bar, price)];
        let id = self.drawings.add_drawing(drawing).ok()?;
        self.drawings.select(id);
        Some(id)
    }

    /// Reveals an offscreen drawing by panning, retaining zoom and the auto/manual scale state.
    #[cfg(feature = "ui")]
    pub(crate) fn reveal_drawing(&mut self, id: usize) {
        let Some(drawing) = self.drawings.drawings.iter().find(|d| d.id == id) else {
            return;
        };
        if !drawing.selectable || drawing.screen_anchor.is_some() || self.state.shown().is_empty() {
            return;
        }
        let pane = self.pane_layout.panes.iter().position(|pane| {
            drawing.pane == Some(pane.id.0)
                || pane.members.iter().any(|id| drawing.pane == Some(id.0))
        });
        let price = match pane {
            Some(at) => &self.pane_layout.panes[at].scale,
            None => self.state.price_scale(),
        };
        let transform = price.transform();
        let bounds = drawing
            .chart_points
            .iter()
            .filter_map(|point| {
                let price = transform.to_scale(point.price);
                (point.bar_idx.is_finite() && price.is_finite()).then_some((point.bar_idx, price))
            })
            .fold(None, |bounds, (bar, price)| {
                Some(match bounds {
                    None => (bar, bar, price, price),
                    Some((left, right, low, high)) => (
                        bar.min(left),
                        bar.max(right),
                        price.min(low),
                        price.max(high),
                    ),
                })
            });
        let Some((left, right, low, high)) = bounds else {
            return;
        };
        let range = self.state.time_scale().visible_logical_range();
        let padding = ((range.right - range.left) * 0.05).clamp(0.0, 2.0);
        let shift = if right < range.left {
            right - range.left - padding
        } else if left > range.right {
            left - range.right + padding
        } else {
            0.0
        };
        if shift != 0.0 {
            self.cancel_view_motion();
            self.tracking_mode_active = false;
            let scale = self.state.time_scale_mut();
            scale.set_right_offset(scale.right_offset_precise() + f64::from(shift));
        }
        let scale = match pane {
            Some(at) => &mut self.pane_layout.panes[at].scale,
            None => self.state.price_scale_mut(),
        };
        if scale.is_manual() {
            let range = scale.range();
            let padding = (range.max - range.min) * 0.05;
            let shift = if high < range.min {
                high - range.min - padding
            } else if low > range.max {
                low - range.max + padding
            } else {
                0.0
            };
            if shift != 0.0 && (range.min + shift).is_finite() && (range.max + shift).is_finite() {
                scale.set_range(range.min + shift, range.max + shift);
            }
        }
    }

    fn map_drawing_points(
        &self,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<Vec<ChartPoint>, DrawingError> {
        points
            .iter()
            .enumerate()
            .map(|(point, &(time, price))| {
                if !price.is_finite() {
                    return Err(DrawingError::NonFinitePrice { point });
                }
                Ok(self.anchor_at_time(time, price))
            })
            .collect()
    }

    /// The anchor at `time` and the finite `price`, also before the loaded history or without
    /// bars, where it keeps `time` until the bars arrive.
    pub(super) fn anchor_at_time(&self, time: DateTime<Utc>, price: f64) -> ChartPoint {
        self.drawing_point_at(time, price).unwrap_or_else(|| {
            let mut point = ChartPoint::new(0.0, price);
            point.time_anchor = Some((time, 0.0));
            point
        })
    }
}

/// The frame of a picture of `size` pixels, scaled down to fit twice the default image frame
/// with each side at least one point. Refuses an empty address and a size that is not
/// positive and finite.
fn image_frame(uri: &str, size: egui::Vec2) -> Result<egui::Vec2, &'static str> {
    if uri.trim().is_empty() {
        return Err("Image address is empty");
    }
    if !(size.x > 0.0 && size.y > 0.0 && size.is_finite()) {
        return Err("Image size must be positive and finite");
    }
    let sizing = &crate::tokens::DESIGN_TOKENS.sizing.media;
    let limit = egui::vec2(
        2.0 * sizing.image_default_width,
        2.0 * sizing.image_default_height,
    );
    let scale = 1.0_f32.min(limit.x / size.x).min(limit.y / size.y);
    Ok((size * scale).max(egui::Vec2::splat(1.0)).min(limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use crate::drawings::manager::DrawingSyncAction;
    use crate::model::{Bar, BarData};
    use crate::studies::{CustomIndicator, IndicatorValue};

    /// A pasted picture is one selected, undoable image drawing at the pointer over the price
    /// pane, else at the pane's centre; refusals change nothing.
    #[test]
    fn pasted_pictures_land_at_the_pointer_or_the_price_pane_centre() {
        use crate::drawings::ToolAvailability;
        use crate::widget::ChartEvent;
        use crate::widget::test_frames::{chart, frame};
        use egui::{Context, Event, pos2, vec2};

        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        chart.add_indicator(Box::new(
            CustomIndicator::new("Pane", |bars| {
                vec![IndicatorValue::Single(50.0); bars.len()]
            })
            .with_overlay(false),
        ));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let map = chart.get_chart_mapping();
        let plot = chart.last_rendered_price_rect;
        // Unequal scale margins put the pane centre away from the data's midpoint.
        let centre_price = map.y_to_price(plot.center().y);
        let over = pos2(map.idx_to_x(30), map.price_to_y(16.0));
        let study = &chart.last_rendered_indicator_panes[0];
        let study = pos2(study.mapping.idx_to_x(30), study.chart_rect.center().y);
        for (time, pointer, at, expected) in [
            (0.1, Event::PointerMoved(over), over, 16.0),
            (0.2, Event::PointerMoved(study), plot.center(), centre_price),
            (0.3, Event::PointerGone, plot.center(), centre_price),
        ] {
            frame(&ctx, &mut chart, time, vec![pointer]);
            chart.drain_events().for_each(drop);
            let id = chart.paste_image("test://a", vec2(1920.0, 1080.0)).unwrap();
            let image = chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .unwrap();
            assert_eq!(image.tool_type, DrawingToolType::Image);
            assert_eq!(image.text.as_deref(), Some("test://a"));
            assert_eq!(image.image.unwrap().size, Some([240.0, 135.0]));
            assert_eq!(chart.drawings().sel_drawing(), Some(id));
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::ImageRequested(_)))
            );
            let [(anchor, price)] = chart.drawing_points(id).unwrap()[..] else {
                panic!("one anchor");
            };
            let seconds = anchor.timestamp() - 1_700_000_000;
            assert_eq!(seconds % 86_400, 0, "{anchor}");
            let x = map.idx_to_x((seconds / 86_400) as usize);
            assert!(
                (x - at.x).abs() <= map.bar_spacing / 2.0 + 1e-3,
                "{x} {at:?}"
            );
            assert!((price - expected).abs() < 1e-6, "{price} {at:?}");
            assert!(chart.undo());
            assert!(chart.drawings().drawings.is_empty());
        }

        let drawn = |bars: bool| {
            let ctx = Context::default();
            let mut chart = if bars {
                crate::widget::test_frames::chart(&[10.0], &[20.0])
            } else {
                Chart::new(BarData::default())
            };
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart
        };
        let mut disabled = drawn(true);
        disabled.drawings_mut().registry.set_availability(
            DrawingToolType::Image,
            ToolAvailability {
                visible: true,
                enabled: false,
            },
        );
        let size = "Image size must be positive and finite";
        for (mut chart, uri, picture, error) in [
            (
                drawn(true),
                "  ",
                vec2(10.0, 10.0),
                "Image address is empty",
            ),
            (drawn(true), "test://a", vec2(0.0, 10.0), size),
            (drawn(true), "test://a", vec2(10.0, f32::INFINITY), size),
            (
                crate::widget::test_frames::chart(&[10.0], &[20.0]),
                "test://a",
                vec2(10.0, 10.0),
                "Chart has not been drawn",
            ),
            (
                drawn(false),
                "test://a",
                vec2(10.0, 10.0),
                "Chart has no bars to date the image",
            ),
            (
                disabled,
                "test://a",
                vec2(10.0, 10.0),
                "Drawing tool is disabled",
            ),
        ] {
            assert!(!chart.can_undo(), "{error}");
            assert_eq!(
                chart.paste_image(uri, picture),
                Err(DrawingError::Invalid(error))
            );
            assert!(chart.drawings().drawings.is_empty(), "{error}");
            assert!(!chart.can_undo(), "{error}");
        }
    }

    /// Round-family host updates share the saved config, reject invalid widths and remain one
    /// undoable edit.
    #[test]
    fn round_fibonacci_host_edits_are_one_undoable_step() {
        use crate::drawings::{DrawingToolType as T, FibonacciConfig};
        for tool in [
            T::FibonacciCircles,
            T::FibonacciSpeedResistanceArcs,
            T::FibonacciSpiral,
        ] {
            let mut chart = crate::widget::test_frames::chart(&[90.0; 20], &[210.0; 20]);
            let mut drawing = Drawing::new(7, tool);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(1.0, 200.0)];
            drawing.completed = true;
            chart.drawings_mut().drawings.push(drawing.clone());
            let mut saved = serde_json::to_value(FibonacciConfig::for_tool(tool)).unwrap();
            saved["spiral_counterclockwise"] = true.into();
            saved["close_arcs"] = true.into();
            if tool != T::FibonacciSpiral {
                for (i, level) in saved["levels"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                {
                    level["width"] = ((i + 1) as f32).into();
                    level["line_style"] = if i % 2 == 0 { "Dotted" } else { "Dashed" }.into();
                }
            }
            let config: FibonacciConfig = serde_json::from_value(saved).unwrap();
            chart.set_fibonacci_config(7, config.clone()).unwrap();
            chart.set_fibonacci_config(7, config.clone()).unwrap();
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].fib_config, drawing.fib_config);
            assert!(!chart.can_undo());
            assert!(chart.redo());
            let id = chart.drawings().drawings[0].id;
            let mut invalid = config.clone();
            invalid.label_size = f32::NAN;
            assert!(chart.set_fibonacci_config(id, invalid).is_err());
            if !config.levels.is_empty() {
                let mut invalid = config.clone();
                invalid.levels[0].width = Some(0.0);
                assert!(chart.set_fibonacci_config(id, invalid).is_err());
            }
            assert!(chart.undo());
            assert!(!chart.can_undo(), "rejected edits add no history");
            chart.drawings_mut().drawings[0].locked = true;
            assert_eq!(
                chart.set_fibonacci_config(id, config.clone()),
                Err(DrawingError::Locked(id))
            );
            assert_eq!(
                chart.set_fibonacci_config(99, config),
                Err(DrawingError::UnknownDrawing(99))
            );
        }
    }

    /// Extension settings retain every level through templates, persistence and one undo step;
    /// invalid fonts, locked drawings and wrong tools leave the drawing and history intact.
    #[test]
    fn fibonacci_extension_edits_preserve_levels_and_validate_atomically() {
        use crate::drawings::{DrawingTemplate, DrawingToolType as T, FibonacciConfig};
        let bars = (0..20)
            .map(|i| {
                Bar::new(
                    DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(),
                    100.0,
                    220.0,
                    90.0,
                    150.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let mut drawing = Drawing::new(7, T::FibonacciExtension);
        drawing.completed = true;
        drawing.chart_points = [(0.0, 100.0), (5.0, 200.0), (10.0, 150.0)]
            .map(|(x, y)| ChartPoint::new(x, y))
            .to_vec();
        let original = drawing.fib_config.clone();
        let mut config = original.clone().unwrap();
        assert_eq!(config.levels.len(), 24);
        for (index, level) in config.levels.iter_mut().enumerate() {
            level.text = format!("Target {index}");
            level.width = Some(1.5 + index as f32);
        }
        config.label_size = 18.0;
        config.trend_visible = false;
        config.reverse = true;
        config.logarithmic = true;
        config.label_position = Some(crate::drawings::LineTextHorizontalAlign::Center);
        chart.drawings_mut().drawings.push(drawing.clone());
        chart.set_fibonacci_config(7, config.clone()).unwrap();
        assert_eq!(
            chart.drawings().drawings[0].chart_points,
            drawing.chart_points
        );
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].fib_config, original);
        assert!(!chart.can_undo());
        assert!(chart.redo());
        let mut peer = Drawing::new(8, T::FibonacciExtension);
        DrawingTemplate::from_drawing(&chart.drawings().drawings[0])
            .apply_to(&mut peer)
            .unwrap();
        assert_eq!(peer.fib_config, Some(config.clone()));
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        chart.load_drawings(&saved).unwrap();
        assert_eq!(
            chart.drawings().drawings[0].fib_config,
            Some(config.clone())
        );
        // Repeated settings do not consume a history step.
        chart
            .set_fibonacci_config(chart.drawings().drawings[0].id, config.clone())
            .unwrap();
        let id = chart.drawings().drawings[0].id;
        for size in [0.0, 257.0, f32::NAN, f32::INFINITY] {
            let mut invalid = config.clone();
            invalid.label_size = size;
            assert!(chart.set_fibonacci_config(id, invalid).is_err());
            assert_eq!(
                chart.drawings().drawings[0].fib_config,
                Some(config.clone())
            );
        }
        chart.drawings_mut().drawings[0].locked = true;
        assert_eq!(
            chart.set_fibonacci_config(id, FibonacciConfig::extension_default()),
            Err(DrawingError::Locked(id))
        );
        chart.drawings_mut().drawings[0].locked = false;
        chart.drawings_mut().drawings[0].tool_type = T::TrendLine;
        assert!(chart.set_fibonacci_config(id, config).is_err());
        assert!(!chart.can_undo());
    }

    /// Host edits retain measurements in saved records and templates and undo in one step;
    /// old records keep every readout hidden. Rejected and unchanged edits do not add history.
    #[test]
    fn line_measurements_survive_host_edits_templates_and_saved_records() {
        use crate::drawings::persistence::{StoredDrawing, drawing_to_stored, stored_to_drawing};
        use crate::drawings::{
            DrawingTemplate, DrawingToolType as T, LineMeasurementPosition, LineMeasurements,
        };
        let mut chart = Chart::new(crate::model::BarData::default());
        let mut line = Drawing::new(1, T::ExtendedLine);
        line.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(3.0, 200.0)];
        line.completed = true;
        chart.drawings_mut().drawings.push(line);
        let measurements = LineMeasurements {
            persistent: true,
            price_change: true,
            percent_change: true,
            pip_change: true,
            bar_count: true,
            elapsed_time: true,
            length: true,
            inclination: true,
            midpoint: true,
            position: LineMeasurementPosition::Center,
            price_labels: true,
            text_horizontal_align: crate::drawings::LineTextHorizontalAlign::Right,
            text_vertical_align: crate::drawings::LineTextVerticalAlign::Top,
            text_orientation: crate::drawings::LineTextOrientation::Horizontal,
            across_panes: true,
        };
        chart.set_line_measurements(1, measurements).unwrap();
        chart.set_line_measurements(1, measurements).unwrap();
        assert_eq!(chart.drawings().drawings[0].line_measurements, measurements);
        assert!(chart.undo());
        assert_eq!(
            chart.drawings().drawings[0].line_measurements,
            Default::default()
        );
        assert!(!chart.undo());
        assert!(chart.redo());
        let line = &chart.drawings().drawings[0];
        let stored = drawing_to_stored(line, |bar| bar as i64 * 3_600_000);
        let json = serde_json::to_string(&stored).unwrap();
        let loaded: StoredDrawing = serde_json::from_str(&json).unwrap();
        let restored = stored_to_drawing(&loaded, |ms| ms as f32 / 3_600_000.0, 2);
        assert_eq!(restored.line_measurements, measurements);
        assert_eq!(restored.chart_points, line.chart_points);
        let mut old = serde_json::to_value(&stored).unwrap();
        old["drawing"]
            .as_object_mut()
            .unwrap()
            .remove("line_measurements");
        let old: StoredDrawing = serde_json::from_value(old).unwrap();
        assert_eq!(old.drawing.line_measurements, Default::default());
        // An arrow saved before its statistics were offered keeps its readouts off and every
        // style setting unchanged when it is restored and saved again.
        let old: StoredDrawing = serde_json::from_str(
            r#"{"id":"7","points":[{"ts_ms":7200000,"price":100.0},{"ts_ms":28800000,"price":200.0}],"drawing":{"label":null,"pane":null,"selectable":true,"saveable":true,"listed":true,"user_edit_enabled":true,"group":null,"folder":[],"custom_tool":null,"custom_settings":null,"tool_type":"Arrow","screen_anchor":null,"color":[41,98,255,255],"stroke_width":2.0,"text":"Saved arrow","post":null,"line_style":"Solid","fill_color":null,"locked":false,"visible":true,"font_size":14.0,"font_weight":"Normal","italic":false,"text_color":null,"background":true,"border":true,"border_color":null,"text_wrap":false,"text_wrap_width":200.0,"text_follows_zoom":false,"text_zoom_spacing":null,"note_text_style":null,"extend_left":false,"extend_right":false,"arrow_start_style":"None","arrow_end_style":"Arrow","timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"price_axis_label":true,"time_axis_label":true,"line_measurements":{"persistent":false,"price_change":false,"percent_change":false,"pip_change":false,"bar_count":false,"elapsed_time":false,"length":false,"inclination":false,"midpoint":false,"price_labels":false,"position":"Right","text_horizontal_align":"Center","text_vertical_align":"Bottom","text_orientation":"Vertical","across_panes":false},"range_settings":{"price_change":true,"percent_change":true,"pip_change":true,"bar_count":true,"elapsed_time":true,"volume":true,"text_color":null,"font_size":12.0,"background":true,"background_color":null,"border":true,"border_color":null,"border_width":null,"border_style":null,"extend_top":false,"extend_bottom":false},"position_size":null,"position_sizing":{"point_value":1.0,"lot_size":1.0,"quantity_step":0.0,"leverage":1.0},"position_readout":{"persistent":false,"condensed":false,"account_currency":null,"conversion_rate":null,"quantity_decimals":null},"position_target_color":null,"position_stop_color":null,"position_style":{"line_color":null,"line_width":1.0,"text_color":null,"font_size":12.0,"label_background":true,"label_color":null,"fill":true,"border":false,"border_color":[102,123,139,255]},"marker":{"show_text":true,"glyph":"🙂","show_glyph":false,"glyph_color":null},"fib_config":null,"volume_profile":null,"regression_trend":null,"vwap":null,"gann":null,"ghost_feed":null,"channel_midline":false,"anchor_price_style":null,"middle_line":{"color":null,"width":1.0,"line_style":"Dashed"},"theme_shade":true,"image":null,"glyph_angle":0.0,"closed":false,"pitchfork":null,"elliott_degree":"Minor","elliott_show_wave":true,"bars_pattern":null,"projection":null,"z_order":0},"created_at":1795162944672,"modified_at":1795162944672,"notes":null}"#,
        )
        .unwrap();
        let arrow = stored_to_drawing(&old, |ms| ms as f32 / 3_600_000.0, 7);
        assert_eq!(arrow.tool_type, T::Arrow);
        assert_eq!(arrow.line_measurements, Default::default());
        assert_eq!(
            arrow.chart_points,
            [ChartPoint::new(2.0, 100.0), ChartPoint::new(8.0, 200.0)]
        );
        let saved = drawing_to_stored(&arrow, |bar| bar as i64 * 3_600_000);
        assert_eq!(
            serde_json::to_value(&saved.drawing).unwrap(),
            serde_json::to_value(&old.drawing).unwrap()
        );
        // A horizontal line saved before these settings keeps its text above the line.
        let mut old = serde_json::to_value(&stored).unwrap();
        old["drawing"]["tool_type"] = serde_json::to_value(T::HorizontalLine).unwrap();
        old["drawing"]
            .as_object_mut()
            .unwrap()
            .remove("line_measurements");
        let old: StoredDrawing = serde_json::from_value(old).unwrap();
        assert_eq!(old.drawing.line_measurements, Default::default());
        // A vertical line saved before its reach and orientation stays in its pane, its text along
        // it, in the place it always was.
        let mut old = serde_json::to_value(&stored).unwrap();
        old["drawing"]["tool_type"] = serde_json::to_value(T::VerticalLine).unwrap();
        let settings = old["drawing"]["line_measurements"].as_object_mut().unwrap();
        settings.remove("text_orientation");
        settings.remove("across_panes");
        let old: StoredDrawing = serde_json::from_value(old).unwrap();
        assert_eq!(
            (
                old.drawing.line_measurements.text_orientation,
                old.drawing.line_measurements.across_panes
            ),
            (crate::drawings::LineTextOrientation::Vertical, false)
        );
        // An info line saved before these settings keeps showing its readouts.
        let mut old = serde_json::to_value(&stored).unwrap();
        old["drawing"]["tool_type"] = serde_json::to_value(T::InfoLine).unwrap();
        old["drawing"]
            .as_object_mut()
            .unwrap()
            .remove("line_measurements");
        let old: StoredDrawing = serde_json::from_value(old).unwrap();
        assert!(old.drawing.line_measurements.persistent);
        assert!(old.drawing.line_measurements.price_change);
        let template: DrawingTemplate = serde_json::from_str(
            &serde_json::to_string(&DrawingTemplate::from_drawing(line)).unwrap(),
        )
        .unwrap();
        let mut other = Drawing::new(2, T::ExtendedLine);
        template.apply_to(&mut other).unwrap();
        assert_eq!(other.line_measurements, measurements);
        chart.drawings_mut().drawings[0].locked = true;
        assert_eq!(
            chart.set_line_measurements(1, Default::default()),
            Err(DrawingError::Locked(1))
        );
        assert_eq!(
            chart.set_line_measurements(999, measurements),
            Err(DrawingError::UnknownDrawing(999))
        );
        chart.drawings_mut().drawings[0].locked = false;
        chart.drawings_mut().drawings[0].tool_type = T::Rect;
        assert!(matches!(
            chart.set_line_measurements(1, measurements),
            Err(DrawingError::Invalid(_))
        ));
        assert_eq!(chart.drawings().drawings[0].line_measurements, measurements);
        for tool in [
            T::TrendLine,
            T::Ray,
            T::InfoLine,
            T::TrendAngle,
            T::Arrow,
            T::HorizontalLine,
            T::HorizontalRay,
            T::VerticalLine,
            T::RegressionTrend,
        ] {
            chart.drawings_mut().drawings[0].tool_type = tool;
            chart.drawings_mut().drawings[0].line_measurements = measurements;
            chart.set_line_measurements(1, Default::default()).unwrap();
            assert_eq!(
                chart.drawings().drawings[0].line_measurements,
                Default::default(),
                "{tool:?}"
            );
        }
    }

    /// Hosts can update each family in one history entry, including spare wedge arcs and the
    /// fan's own time levels, grid and label sides; invalid, locked, missing and unrelated
    /// drawings leave their settings and history untouched.
    #[test]
    fn fibonacci_family_settings_are_validated_and_undoable() {
        use crate::drawings::{DrawingTemplate, FibonacciConfig};
        for tool in [
            DrawingToolType::FibonacciChannel,
            DrawingToolType::FibonacciWedge,
            DrawingToolType::FibonacciSpeedFan,
        ] {
            let mut chart = crate::widget::test_frames::chart(&[90.0; 20], &[210.0; 20]);
            let mut drawing = Drawing::new(7, tool);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(2.0, 100.0),
                ChartPoint::new(8.0, 200.0),
                ChartPoint::new(2.0, 150.0),
            ];
            let original = drawing.fib_config.clone();
            chart.drawings_mut().drawings.push(drawing);
            let mut json = serde_json::to_value(FibonacciConfig::for_tool(tool)).unwrap();
            json["label_size"] = 20.0.into();
            json["label_position"] = "Center".into();
            json["trend_visible"] = false.into();
            if tool == DrawingToolType::FibonacciSpeedFan {
                json["time_levels"][1]["visible"] = false.into();
                json["grid_style"] = "Dashed".into();
                json["top_labels"] = false.into();
                json["reverse"] = true.into();
            }
            for (i, level) in json["levels"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                level["width"] = ((i + 1) as f64).into();
                level["line_style"] = if i % 2 == 0 { "Dotted" } else { "Dashed" }.into();
            }
            let edited: FibonacciConfig = serde_json::from_value(json.clone()).unwrap();
            chart.set_fibonacci_config(7, edited.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(chart.drawings().drawings[0].fib_config.as_ref().unwrap())
                    .unwrap(),
                json
            );
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].fib_config, original);
            assert!(!chart.can_undo());
            assert!(chart.redo());
            // An unchanged assignment does not create a second history entry.
            chart.set_fibonacci_config(7, edited.clone()).unwrap();
            let template = DrawingTemplate::from_drawing(&chart.drawings().drawings[0]);
            let mut peer = Drawing::new(8, tool);
            template.apply_to(&mut peer).unwrap();
            assert_eq!(peer.fib_config, Some(edited.clone()));
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            let mut restored = Chart::new(BarData::default());
            restored.load_drawings(&saved).unwrap();
            assert_eq!(
                restored.drawings().drawings[0].fib_config,
                Some(edited.clone())
            );
            for bad in [f32::NAN, 0.0, 101.0] {
                let mut invalid = edited.clone();
                invalid.levels[0].width = Some(bad);
                assert!(chart.set_fibonacci_config(7, invalid).is_err());
                let mut invalid = edited.clone();
                invalid.grid_width = bad;
                assert!(chart.set_fibonacci_config(7, invalid).is_err());
                let mut invalid = edited.clone();
                if let Some(time) = &mut invalid.time_levels {
                    time[0].width = Some(bad);
                    assert!(chart.set_fibonacci_config(7, invalid).is_err());
                }
            }
            chart.drawings_mut().drawings[0].locked = true;
            assert_eq!(
                chart.set_fibonacci_config(7, edited.clone()),
                Err(DrawingError::Locked(7))
            );
            assert_eq!(
                chart.set_fibonacci_config(77, edited.clone()),
                Err(DrawingError::UnknownDrawing(77))
            );
            chart.drawings_mut().drawings[0].locked = false;
            assert!(chart.undo());
            assert!(!chart.can_undo());
            assert_eq!(chart.drawings().drawings[0].fib_config, original);
            if tool == DrawingToolType::FibonacciSpeedFan {
                // Settings saved before fans had time levels take the price levels as theirs.
                let mut shared = edited.clone();
                shared.time_levels = None;
                chart.set_fibonacci_config(7, shared.clone()).unwrap();
                let config = chart.drawings().drawings[0].fib_config.as_ref().unwrap();
                assert_eq!(config.time_levels.as_ref(), Some(&edited.levels));
                // Setting them again changes nothing, so one undo restores the fan.
                chart.set_fibonacci_config(7, shared).unwrap();
                assert!(chart.undo());
                assert!(!chart.can_undo());
            }
            chart.drawings_mut().drawings[0].tool_type = DrawingToolType::TrendLine;
            assert!(chart.set_fibonacci_config(7, edited).is_err());
        }
    }

    /// Hosts set an anchored VWAP's source, band unit, fills, line plot and tracking in one
    /// history entry, which templates and saved drawings carry; invalid settings, locked, missing
    /// and other drawings change nothing.
    #[test]
    fn anchored_vwap_settings_are_validated_and_undoable() {
        use crate::drawings::{DrawingTemplate, VwapBandMode, VwapConfig, VwapFill};
        let mut chart = crate::widget::test_frames::chart(&[90.0; 20], &[210.0; 20]);
        let mut drawing = Drawing::new(7, DrawingToolType::AnchoredVWAP);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(2.0, 100.0)];
        chart.drawings_mut().drawings.push(drawing);
        let edited = VwapConfig {
            source: crate::model::PriceSource::OHLC4,
            executions: true,
            band_mode: VwapBandMode::Percentage,
            show_line: false,
            plot_type: crate::drawings::VwapPlotType::StepLine,
            track_price: true,
            precision: Some(3),
            band_fill: VwapFill::new(false, Some([1, 2, 3, 255]), 0.4).unwrap(),
            background: VwapFill::new(true, None, 0.2).unwrap(),
            ..VwapConfig::default()
        };
        chart.set_vwap_config(7, edited).unwrap();
        assert_eq!(chart.drawings().drawings[0].vwap, Some(edited));
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].vwap, None);
        assert!(!chart.can_undo());
        assert!(chart.redo());
        // An unchanged assignment does not create a second history entry.
        chart.set_vwap_config(7, edited).unwrap();
        assert!(chart.undo());
        assert!(!chart.can_undo());
        assert!(chart.redo());
        let template = DrawingTemplate::from_drawing(&chart.drawings().drawings[0]);
        let mut peer = Drawing::new(8, DrawingToolType::AnchoredVWAP);
        template.apply_to(&mut peer).unwrap();
        assert_eq!(peer.vwap, Some(edited));
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        let mut restored = Chart::new(BarData::default());
        restored.load_drawings(&saved).unwrap();
        assert_eq!(restored.drawings().drawings[0].vwap, Some(edited));
        let invalid = VwapConfig {
            precision: Some(9),
            ..edited
        };
        assert!(chart.set_vwap_config(7, invalid).is_err());
        chart.drawings_mut().drawings[0].locked = true;
        assert_eq!(
            chart.set_vwap_config(7, VwapConfig::default()),
            Err(DrawingError::Locked(7))
        );
        assert_eq!(
            chart.set_vwap_config(77, VwapConfig::default()),
            Err(DrawingError::UnknownDrawing(77))
        );
        chart.drawings_mut().drawings[0].locked = false;
        chart.drawings_mut().drawings[0].tool_type = DrawingToolType::TrendLine;
        assert!(chart.set_vwap_config(7, VwapConfig::default()).is_err());
        chart.drawings_mut().drawings[0].tool_type = DrawingToolType::AnchoredVWAP;
        assert_eq!(chart.drawings().drawings[0].vwap, Some(edited));
        assert!(chart.undo());
        assert!(!chart.can_undo());
    }

    /// Projection edits validate before changing drawings or history and commit once.
    #[test]
    fn projection_settings_are_validated_and_undoable() {
        use crate::drawings::{DrawingTemplate, ProjectionConfig};
        let mut chart = crate::widget::test_frames::chart(&[80.0], &[200.0]);
        let mut drawing = Drawing::new(7, DrawingToolType::ProjectionTool);
        drawing.chart_points = vec![
            ChartPoint::new(10.0, 100.0),
            ChartPoint::new(20.0, 150.0),
            ChartPoint::new(40.0, 120.0),
        ];
        drawing.completed = true;
        let original = drawing.projection.clone();
        chart.drawings_mut().drawings.push(drawing);
        assert_eq!(
            chart.set_projection_config(77, ProjectionConfig::default()),
            Err(DrawingError::UnknownDrawing(77))
        );
        for (tool, locked, expected) in [
            (
                DrawingToolType::TrendLine,
                false,
                DrawingError::Invalid("Drawing is not a projection"),
            ),
            (
                DrawingToolType::ProjectionTool,
                true,
                DrawingError::Locked(7),
            ),
        ] {
            let drawing = &mut chart.drawings_mut().drawings[0];
            drawing.tool_type = tool;
            drawing.locked = locked;
            assert_eq!(
                chart.set_projection_config(7, ProjectionConfig::default()),
                Err(expected)
            );
        }
        let drawing = &mut chart.drawings_mut().drawings[0];
        drawing.locked = false;
        for (coefficient, width) in [
            (f32::NAN, None),
            (f32::INFINITY, None),
            (-0.5, None),
            (100.5, None),
            (1.0, Some(0.0)),
            (1.0, Some(-1.0)),
            (1.0, Some(f32::NAN)),
            (1.0, Some(f32::INFINITY)),
            (1.0, Some(100.5)),
        ] {
            let mut invalid = ProjectionConfig::default();
            invalid.levels[0].coefficient = coefficient;
            invalid.levels[0].width = width;
            assert_eq!(
                chart.set_projection_config(7, invalid.clone()),
                Err(DrawingError::Invalid("Invalid projection settings"))
            );
            assert!(
                chart
                    .drawings_mut()
                    .edit_drawing(7, |d| d.projection = Some(invalid.clone()))
                    .is_err()
            );
            let mut template = DrawingTemplate::from_drawing(&chart.drawings().drawings[0]);
            template.settings.projection = Some(invalid);
            let mut peer = chart.drawings().drawings[0].clone();
            assert!(template.apply_to(&mut peer).is_err());
            assert_eq!(chart.drawings().drawings[0].projection, original);
            assert!(!chart.can_undo());
        }
        let mut edited = ProjectionConfig::default();
        edited.levels[0].coefficient = 0.0;
        edited.levels[0].width = Some(100.0);
        let mut far = edited.levels[0].clone();
        far.coefficient = 100.0;
        edited.levels.push(far);
        chart.set_projection_config(7, edited.clone()).unwrap();
        chart.set_projection_config(7, edited.clone()).unwrap();
        assert_eq!(chart.drawings().drawings[0].projection, Some(edited));
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].projection, original);
        assert!(!chart.can_undo());
    }

    /// Hosts set a regression trend's source, bounds, line styles and correlation label in one
    /// history entry, which templates and saved drawings carry; locked, missing and other drawings
    /// change nothing.
    #[test]
    fn regression_trend_settings_are_undoable_and_saved() {
        use crate::drawings::{DrawingTemplate, RegressionTrendConfig};
        let mut chart = crate::widget::test_frames::chart(&[90.0; 20], &[210.0; 20]);
        let mut drawing = Drawing::new(7, DrawingToolType::RegressionTrend);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(2.0, 100.0), ChartPoint::new(8.0, 200.0)];
        chart.drawings_mut().drawings.push(drawing);
        let mut edited = RegressionTrendConfig::default()
            .with_deviations(1.5, 3.0)
            .and_then(|config| config.with_precision(Some(4)))
            .unwrap();
        edited.use_upper = false;
        edited.source = crate::model::PriceSource::HL2;
        edited.line_style = crate::model::LineStyle::Dotted;
        edited.show_base_line = false;
        edited.upper_line = crate::drawings::RegressionBandLine::new([200, 0, 0, 128])
            .with_width(Some(5.0))
            .unwrap();
        edited.upper_line.line_style = crate::model::LineStyle::Dotted;
        edited.lower_line.visible = false;
        edited.show_correlation = false;
        chart.set_regression_trend_config(7, edited).unwrap();
        assert_eq!(chart.drawings().drawings[0].regression_trend, Some(edited));
        assert!(chart.undo());
        assert_eq!(
            chart.drawings().drawings[0].regression_trend,
            Some(RegressionTrendConfig::drawing_default())
        );
        assert!(!chart.can_undo());
        assert!(chart.redo());
        // An unchanged assignment does not create a second history entry.
        chart.set_regression_trend_config(7, edited).unwrap();
        let measurements = crate::drawings::LineMeasurements {
            text_horizontal_align: crate::drawings::LineTextHorizontalAlign::Left,
            text_vertical_align: crate::drawings::LineTextVerticalAlign::Top,
            ..Default::default()
        };
        chart.set_line_measurements(7, measurements).unwrap();
        chart
            .user_action(|chart| {
                chart.drawings_mut().edit_drawing(7, |drawing| {
                    drawing.font_weight = crate::drawings::FontWeight::Bold;
                    drawing.italic = true;
                    drawing.text_color = Some([1, 2, 3, 128]);
                })
            })
            .unwrap();
        let template = DrawingTemplate::from_drawing(&chart.drawings().drawings[0]);
        let mut peer = Drawing::new(8, DrawingToolType::RegressionTrend);
        template.apply_to(&mut peer).unwrap();
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        let mut restored = Chart::new(BarData::default());
        restored.load_drawings(&saved).unwrap();
        for drawing in [&peer, &restored.drawings().drawings[0]] {
            assert_eq!(drawing.regression_trend, Some(edited));
            assert_eq!(drawing.line_measurements, measurements);
            assert_eq!(drawing.font_weight, crate::drawings::FontWeight::Bold);
            assert!(drawing.italic);
            assert_eq!(drawing.text_color, Some([1, 2, 3, 128]));
        }
        let default = RegressionTrendConfig::default();
        chart.drawings_mut().drawings[0].locked = true;
        assert_eq!(
            chart.set_regression_trend_config(7, default),
            Err(DrawingError::Locked(7))
        );
        assert_eq!(
            chart.set_regression_trend_config(77, default),
            Err(DrawingError::UnknownDrawing(77))
        );
        chart.drawings_mut().drawings[0].locked = false;
        chart.drawings_mut().drawings[0].tool_type = DrawingToolType::TrendLine;
        assert!(chart.set_regression_trend_config(7, default).is_err());
        chart.drawings_mut().drawings[0].tool_type = DrawingToolType::RegressionTrend;
        assert_eq!(chart.drawings().drawings[0].regression_trend, Some(edited));
        assert!(chart.undo());
        assert_eq!(
            chart.drawings().drawings[0].font_weight,
            crate::drawings::FontWeight::Normal
        );
        assert!(chart.undo());
        assert_eq!(
            chart.drawings().drawings[0].line_measurements,
            Default::default()
        );
        assert!(chart.undo());
        assert!(!chart.can_undo());
        // Saved before regression line settings, at a non-default width. The missing band
        // widths follow that width and the text keeps its centred-above placement.
        let old = r#"{"version":10,"symbol":"LEGACY","timeframe":null,"drawings":[{"id":"7","points":[{"ts_ms":120000,"price":100.0},{"ts_ms":480000,"price":200.0}],"drawing":{"label":null,"pane":null,"selectable":true,"saveable":true,"listed":true,"group":null,"folder":[],"custom_tool":null,"custom_settings":null,"tool_type":"RegressionTrend","screen_anchor":null,"color":[242,54,69,255],"stroke_width":3.0,"text":"Legacy trend","post":null,"line_style":"Solid","fill_color":[41,98,255,51],"locked":false,"visible":true,"font_size":14.0,"font_weight":"Normal","italic":false,"text_color":null,"background":true,"border":true,"border_color":null,"text_wrap":false,"text_wrap_width":200.0,"text_follows_zoom":false,"text_zoom_spacing":null,"note_text_style":null,"extend_left":false,"extend_right":false,"arrow_start_style":"None","arrow_end_style":"None","timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"price_axis_label":true,"time_axis_label":true,"line_measurements":{"persistent":false,"price_change":false,"percent_change":false,"pip_change":false,"bar_count":false,"elapsed_time":false,"length":false,"inclination":false,"midpoint":false,"price_labels":false,"position":"Right","text_horizontal_align":"Center","text_vertical_align":"Bottom","text_orientation":"Vertical","across_panes":false},"position_size":null,"position_sizing":{"point_value":1.0,"lot_size":1.0,"quantity_step":0.0,"leverage":1.0},"position_readout":{"persistent":false,"condensed":false,"account_currency":null,"conversion_rate":null,"quantity_decimals":null},"position_target_color":null,"position_stop_color":null,"position_style":{"line_color":null,"line_width":1.0,"text_color":null,"font_size":12.0,"label_background":true,"label_color":null,"fill":true,"border":false,"border_color":[102,123,139,255]},"marker":{"show_text":true,"glyph":"🙂","show_glyph":false,"glyph_color":null},"fib_config":null,"volume_profile":null,"regression_trend":{"upper":2.0,"lower":2.0,"use_upper":true,"use_lower":true,"source":"Close","line_style":"Dashed","precision":null},"vwap":null,"gann":null,"ghost_feed":null,"channel_midline":false,"anchor_price_style":null,"middle_line":{"color":null,"width":1.0,"line_style":"Dashed"},"theme_shade":true,"image":null,"closed":false,"pitchfork":null,"elliott_degree":"Minor","bars_pattern":null,"z_order":0},"created_at":1795031062320,"modified_at":1795031062320,"notes":null}],"metadata":{"created_at":1795031062320,"modified_at":1795031062320,"app_version":null}}"#;
        restored.set_symbol("LEGACY");
        restored.load_drawings(old).unwrap();
        let drawing = &restored.drawings().drawings[0];
        assert!(drawing.user_edit_enabled);
        assert_eq!(
            drawing.regression_trend,
            Some(RegressionTrendConfig::default())
        );
        assert_eq!(drawing.stroke_width, 3.0);
        assert_eq!(drawing.text.as_deref(), Some("Legacy trend"));
        assert_eq!(drawing.line_measurements, Default::default());
        assert_eq!(drawing.font_weight, crate::drawings::FontWeight::Normal);
        assert!(!drawing.italic);
        assert_eq!(drawing.text_color, None);
    }

    /// A host sets the marker of a pin, a signpost and each drawing with a text switch in one
    /// undoable step, switching the text off and on again; other tools, a glyph that is empty or
    /// longer than 32 bytes, locked drawings and an unchanged marker change nothing.
    #[test]
    fn annotation_marker_host_edits_are_validated_and_undoable() {
        use crate::drawings::AnnotationMarker;
        let marker = AnnotationMarker {
            show_text: false,
            glyph: "🚀".into(),
            show_glyph: true,
            glyph_color: Some([255, 152, 0, 255]),
        };
        let shown = AnnotationMarker {
            show_text: true,
            ..marker.clone()
        };
        for tool in [
            DrawingToolType::AnchoredNote,
            DrawingToolType::Signpost,
            DrawingToolType::HorizontalLine,
            DrawingToolType::HorizontalRay,
            DrawingToolType::ArrowMarkUp,
            DrawingToolType::ArrowMarkDown,
            DrawingToolType::ArrowMarkLeft,
            DrawingToolType::ArrowMarkRight,
            DrawingToolType::ParallelChannel,
            DrawingToolType::FlatTopBottom,
            DrawingToolType::DisjointChannel,
        ] {
            let mut chart = Chart::new(BarData::default());
            chart.drawings_mut().drawings.push(Drawing::new(7, tool));
            chart.set_annotation_marker(7, marker.clone()).unwrap();
            chart.set_annotation_marker(7, marker.clone()).unwrap();
            assert_eq!(chart.drawings().drawings[0].marker, marker, "{tool:?}");
            chart.set_annotation_marker(7, shown.clone()).unwrap();
            assert_eq!(chart.drawings().drawings[0].marker, shown, "{tool:?}");
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].marker, marker, "{tool:?}");
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].marker, Default::default());
            assert!(!chart.undo(), "an unchanged marker adds no step");
            for glyph in ["", "012345678901234567890123456789012"] {
                let invalid = AnnotationMarker {
                    glyph: glyph.into(),
                    ..marker.clone()
                };
                assert!(matches!(
                    chart.set_annotation_marker(7, invalid),
                    Err(DrawingError::Invalid(_))
                ));
            }
            assert_eq!(
                chart.set_annotation_marker(8, marker.clone()),
                Err(DrawingError::UnknownDrawing(8))
            );
            chart.drawings_mut().drawings[0].locked = true;
            assert_eq!(
                chart.set_annotation_marker(7, marker.clone()),
                Err(DrawingError::Locked(7))
            );
            chart.drawings_mut().drawings[0].locked = false;
            for other in [
                DrawingToolType::Note,
                DrawingToolType::TrendLine,
                DrawingToolType::ArrowMarker,
            ] {
                chart.drawings_mut().drawings[0].tool_type = other;
                assert!(
                    matches!(
                        chart.set_annotation_marker(7, marker.clone()),
                        Err(DrawingError::Invalid(_))
                    ),
                    "{other:?}"
                );
            }
            assert_eq!(chart.drawings().drawings[0].marker, Default::default());
            assert!(!chart.can_undo());
        }
    }

    /// Readout edits and reversal each undo in one step, preserve times and identity, and
    /// round-trip through saved records and templates. Legacy records retain visible statistics.
    #[test]
    fn position_readouts_and_reversal_survive_history_and_storage() {
        use crate::drawings::{DrawingTemplate, PositionReadout};
        for anchors in [2, 3] {
            let bars = (0..20)
                .map(|i| {
                    Bar::new(
                        DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(),
                        100.0,
                        106.0,
                        95.0,
                        100.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars));
            let mut drawing = Drawing::new(7, DrawingToolType::LongPos);
            drawing.completed = true;
            drawing.chart_points = vec![
                ChartPoint::new(2.0, 100.0),
                ChartPoint::new(8.0, 105.0),
                ChartPoint::new(6.0, 97.5),
            ];
            drawing.chart_points.truncate(anchors);
            chart.drawings_mut().drawings.push(drawing.clone());
            let readout = PositionReadout {
                persistent: true,
                condensed: true,
                account_currency: Some("EUR".into()),
                conversion_rate: Some(0.8),
                quantity_decimals: Some(2),
            };
            chart.set_position_readout(7, readout.clone()).unwrap();
            assert!(chart.undo());
            assert_eq!(
                chart.drawings().drawings[0].position_readout,
                PositionReadout::default()
            );
            assert!(!chart.undo());
            assert!(chart.redo());
            chart.reverse_position_drawing(7).unwrap();
            let reversed = &chart.drawings().drawings[0];
            assert_eq!(reversed.tool_type, DrawingToolType::ShortPos);
            assert_eq!(
                reversed
                    .chart_points
                    .iter()
                    .map(|p| p.price)
                    .collect::<Vec<_>>(),
                [100.0, if anchors == 2 { 95.0 } else { 97.5 }, 105.0]
            );
            assert_eq!(reversed.chart_points[1].bar_idx, 8.0);
            assert_eq!(
                reversed.chart_points[2].bar_idx,
                if anchors == 2 { 8.0 } else { 6.0 }
            );
            assert_eq!(reversed.position_readout, readout);
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            let mut loaded = Chart::new(BarData::default());
            loaded.load_drawings(&saved).unwrap();
            assert_eq!(
                loaded.drawings().drawings[0].tool_type,
                DrawingToolType::ShortPos
            );
            assert_eq!(loaded.drawings().drawings[0].position_readout, readout);
            let template = DrawingTemplate::from_drawing(reversed);
            let mut other = Drawing::new(8, DrawingToolType::ShortPos);
            template.apply_to(&mut other).unwrap();
            assert_eq!(other.position_readout, readout);
            let mut old = serde_json::to_value(&drawing).unwrap();
            old.as_object_mut().unwrap().remove("position_readout");
            let old: Drawing = serde_json::from_value(old).unwrap();
            assert!(old.position_readout.persistent);
            assert!(chart.undo());
            assert_eq!(
                chart.drawings().drawings[0].chart_points,
                drawing.chart_points
            );
            assert!(chart.redo());
            chart.reverse_position_drawing(7).unwrap();
            let restored = &chart.drawings().drawings[0];
            assert_eq!(restored.tool_type, DrawingToolType::LongPos);
            assert_eq!(&restored.chart_points[..anchors], drawing.chart_points);
            let before = format!("{:?}", chart.drawings().drawings);
            assert!(
                chart
                    .set_position_readout(
                        7,
                        PositionReadout {
                            conversion_rate: Some(f64::NAN),
                            ..readout.clone()
                        }
                    )
                    .is_err()
            );
            chart.drawings_mut().drawings[0].locked = true;
            assert_eq!(
                chart.reverse_position_drawing(7),
                Err(DrawingError::Locked(7))
            );
            assert_eq!(
                chart.set_position_readout(7, readout.clone()),
                Err(DrawingError::Locked(7))
            );
            chart.drawings_mut().drawings[0].locked = false;
            assert_eq!(format!("{:?}", chart.drawings().drawings), before);
            assert_eq!(
                chart.reverse_position_drawing(99),
                Err(DrawingError::UnknownDrawing(99))
            );
            chart.drawings_mut().drawings[0].tool_type = DrawingToolType::TrendLine;
            assert!(matches!(
                chart.reverse_position_drawing(7),
                Err(DrawingError::Invalid(_))
            ));
        }
    }

    /// Guards #977: a drawing moves between the price pane and a study pane as one undoable
    /// edit with its anchors and style, and joins the pane the chart draws it in.
    #[test]
    fn drawings_move_between_panes_with_their_anchors() {
        let bars = (0..20)
            .map(|i| {
                let time = DateTime::from_timestamp(i * 60, 0).unwrap();
                Bar::new(time, 10., 12., 8., 10. + i as f64 % 3., 1.)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let study = chart.add_indicator(Box::new(
            CustomIndicator::new("Pane", |bars| {
                vec![IndicatorValue::Single(50.0); bars.len()]
            })
            .with_overlay(false),
        ));
        let pane = chart.series_pane(study).unwrap();
        let mut line = Drawing::new(0, DrawingToolType::TrendLine);
        line.color = [1, 2, 3, 255];
        line.chart_points = vec![ChartPoint::new(2.0, 30.0), ChartPoint::new(9.0, 70.0)];
        let id = chart.drawings_mut().add_drawing(line).unwrap();
        let before = chart.drawings().drawings[0].clone();
        assert_eq!(chart.drawing_pane(id), Some(PaneId::MAIN));

        assert!(chart.user_action(|chart| chart.move_drawing_to_pane(id, pane)));
        let moved = &chart.drawings().drawings[0];
        assert_eq!(chart.drawing_pane(id), Some(pane));
        assert_eq!(moved.pane, Some(study.0));
        assert_eq!(
            (&moved.chart_points, moved.color),
            (&before.chart_points, before.color)
        );
        assert!(chart.undo());
        assert_eq!(chart.drawing_pane(id), Some(PaneId::MAIN));
        assert!(chart.redo());
        assert!(chart.move_drawing_to_pane(id, PaneId::MAIN));
        assert_eq!(chart.drawings().drawings[0].pane, None);

        assert!(!chart.move_drawing_to_pane(id, PaneId(999)));
        assert!(!chart.move_drawing_to_pane(id + 1, pane));
        chart
            .drawings_mut()
            .edit_drawing(id, |d| d.locked = true)
            .unwrap();
        assert!(!chart.move_drawing_to_pane(id, pane));
        assert_eq!(chart.drawing_pane(id), Some(PaneId::MAIN));
    }

    #[test]
    fn timestamp_creation_validates_before_mutation_and_round_trips_edits() {
        let time = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        let bars = [0, 60, 180]
            .map(|seconds| Bar::new(time(seconds), 10., 12., 8., 10., 1.))
            .to_vec();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let points = [(time(-30), 9.), (time(120), 11.)];
        let id = chart
            .user_action(|chart| {
                chart.create_drawing(Drawing::new(99, DrawingToolType::TrendLine), &points)
            })
            .unwrap();
        let drawing = &chart.drawings().drawings[0];
        // -30/60 = -0.5; 1 + (120-60)/(180-60) = 1.5.
        assert_eq!(
            drawing
                .chart_points
                .iter()
                .map(|p| p.bar_idx)
                .collect::<Vec<_>>(),
            [-0.5, 1.5]
        );
        assert_eq!(chart.drawing_points(id).unwrap(), points);
        assert!(chart.undo());
        assert!(chart.drawings().drawings.is_empty());
        assert!(chart.redo());
        chart.drawings().pending_sync.lock().unwrap().clear();
        for invalid in [
            vec![],
            vec![points[0]],
            vec![points[0]; 3],
            vec![(time(0), f64::NAN); 2],
        ] {
            assert!(
                chart
                    .create_drawing(Drawing::new(0, DrawingToolType::TrendLine), &invalid)
                    .is_err()
            );
            assert!(chart.set_drawing_points(id, &invalid).is_err());
            assert_eq!(chart.drawing_points(id).unwrap(), points);
        }
        assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
        let edited = [(time(30), 10.), (time(360), 12.)];
        chart.set_drawing_points(id, &edited).unwrap();
        let json = chart.save_drawings().unwrap().to_json().unwrap();
        let mut empty = Chart::new(crate::model::BarData::default());
        empty.load_drawings(&json).unwrap();
        let restored_id = empty.drawings().drawings[0].id;
        assert_eq!(empty.drawing_points(restored_id).unwrap(), edited);
        chart
            .drawings_mut()
            .edit_drawing(id, |d| {
                d.saveable = false;
                d.listed = false;
            })
            .unwrap();
        assert!(chart.save_drawings().unwrap().drawings().is_empty());
        assert!(chart.drawings().listed_drawings().next().is_none());
        assert!(matches!(
            chart.drawings().pending_sync.lock().unwrap().last(),
            Some(DrawingSyncAction::Delete(_))
        ));
        assert!(chart.drawings_mut().undo());
        assert_eq!(chart.save_drawings().unwrap().drawings().len(), 1);
        assert!(matches!(
            chart.drawings().pending_sync.lock().unwrap().last(),
            Some(DrawingSyncAction::SaveDrawing(_))
        ));
        let mut volatile = Drawing::new(0, DrawingToolType::HorizontalLine);
        volatile.saveable = false;
        chart.drawings().pending_sync.lock().unwrap().clear();
        let volatile = chart.create_drawing(volatile, &points[..1]).unwrap();
        chart.drawings_mut().delete_drawing(volatile);
        assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
    }

    /// A drawing moved into, or added in, the pane holding the main series is painted there
    /// whether or not the original pane exists, and `drawing_pane` reports that pane. A drawing
    /// saved with the main series as its owner shows there too once the original pane is gone.
    #[cfg(feature = "ui")]
    #[test]
    fn drawing_moved_to_relocated_price_pane_stays_drawn() {
        use crate::ui::context_menu::drawing::move_targets;
        use crate::widget::test_frames::{chart, frame, shapes};
        use crate::widget::{SeriesId, SeriesOptions};

        let blue = egui::Color32::from_rgb(0, 0, 255);
        let green = egui::Color32::from_rgb(0, 255, 0);
        let red = egui::Color32::from_rgb(255, 0, 0);
        for original_kept in [false, true] {
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config.show_volume = false;
            let data = chart.bars(SeriesId::MAIN).to_vec();
            let price = chart.add_pane();
            assert!(chart.move_series_to_pane(SeriesId::MAIN, price));
            let other = chart.add_pane();
            let series = chart.add_series(SeriesOptions::default());
            chart.set_data(series, data);
            assert!(chart.move_series_to_pane(series, other));
            let mut menu = vec![other];
            if original_kept {
                menu.push(PaneId::MAIN);
            } else {
                assert!(chart.remove_pane(PaneId::MAIN));
                assert_eq!(chart.panes(), [price, other]);
            }
            let mut line =
                Drawing::with_color(1, DrawingToolType::HorizontalLine, [0, 0, 255, 255]);
            line.chart_points = vec![ChartPoint::new(70.0, 105.0)];
            line.completed = true;
            chart.drawings_mut().drawings.push(line);
            let ctx = egui::Context::default();
            let rect = |chart: &Chart, pane: PaneId| {
                chart
                    .pane_layout
                    .panes
                    .iter()
                    .find(|item| item.id == pane)
                    .unwrap()
                    .rect
            };
            // A drawing paints line segments, or the mesh it kept from the previous pass.
            let painted_in = |shapes: &[egui::Shape], rect: egui::Rect, color: egui::Color32| {
                shapes.iter().any(|shape| match shape {
                    egui::Shape::LineSegment { points, stroke } => {
                        stroke.color == color && rect.y_range().contains(points[0].y)
                    }
                    egui::Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| {
                        vertex.color == color && rect.y_range().contains(vertex.pos.y)
                    }),
                    _ => false,
                })
            };
            frame(&ctx, &mut chart, 0.0, vec![]);
            let before = shapes(&ctx, &mut chart, 0.1, vec![]);
            assert!(
                painted_in(&before, rect(&chart, price), blue),
                "a drawing without a pane is drawn over the main series"
            );
            let reported = chart.drawing_pane(1);
            let targets: Vec<_> = move_targets(None, &chart, 1)
                .into_iter()
                .map(|(pane, _)| pane)
                .collect();
            assert!(chart.move_drawing_to_pane(1, price));
            let added = chart
                .draw_horizontal_line_at(rect(&chart, price).center(), 105.0)
                .unwrap();
            chart
                .drawings_mut()
                .edit_drawing(added, |drawing| drawing.color = [0, 255, 0, 255])
                .unwrap();
            let mut saved =
                Drawing::with_color(9, DrawingToolType::HorizontalLine, [255, 0, 0, 255]);
            saved.chart_points = vec![ChartPoint::new(70.0, 105.0)];
            saved.completed = true;
            saved.pane = Some(SeriesId::MAIN.0);
            chart.drawings_mut().drawings.push(saved);
            frame(&ctx, &mut chart, 0.2, vec![]);
            let after = shapes(&ctx, &mut chart, 0.3, vec![]);
            let target = rect(&chart, price);
            assert_eq!(
                (
                    reported,
                    targets,
                    painted_in(&after, target, blue),
                    painted_in(&after, target, green),
                    painted_in(&after, target, red),
                ),
                (Some(price), menu, true, true, !original_kept),
                "original pane kept: {original_kept}"
            );
        }
    }

    /// With the price series moved beside a study, a drawing without an owner and one owned by
    /// an overlay of the original pane report the panes that draw them, also after a save and
    /// load: the first is saved for the price pane, not for the study beside the price series.
    #[test]
    fn saved_drawings_keep_their_panes_beside_a_relocated_price_series() {
        use crate::widget::SeriesId;
        use crate::widget::test_frames::chart;

        let study = |overlay| {
            Box::new(
                CustomIndicator::new("Study", |bars| {
                    vec![IndicatorValue::Single(50.0); bars.len()]
                })
                .with_overlay(overlay),
            )
        };
        let mut chart = chart(&[100.0], &[110.0]);
        let overlay = chart.add_indicator(study(true));
        let price = chart.add_pane();
        assert!(chart.move_series_to_pane(SeriesId::MAIN, price));
        let beside = chart.add_indicator(study(false));
        assert!(chart.move_series_to_pane(beside, price));
        for (id, owner) in [(1, None), (2, Some(overlay.0))] {
            let mut line = Drawing::new(id, DrawingToolType::HorizontalLine);
            line.chart_points = vec![ChartPoint::new(70.0, 105.0)];
            line.completed = true;
            line.pane = owner;
            chart.drawings_mut().drawings.push(line);
        }
        let panes = |chart: &Chart| -> Vec<_> {
            chart
                .drawings()
                .drawings
                .iter()
                .map(|drawing| chart.drawing_pane(drawing.id))
                .collect()
        };
        let before = panes(&chart);
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        chart.load_drawings(&saved).unwrap();
        assert_eq!(
            [before, panes(&chart)],
            [[Some(price), Some(PaneId::MAIN)]; 2],
            "before and after the load"
        );
    }
}
