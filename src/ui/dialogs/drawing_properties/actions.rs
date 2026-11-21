//! Actions from the drawing properties dialog.

use super::tabs::{CoordinatesState, has_text_box};
use crate::drawings::domain::GANN_SQUARE_UNITS;
use crate::drawings::validation::{MAX_PITCHFORK_LEVEL, MAX_STROKE_WIDTH};
use crate::drawings::{Drawing, DrawingTemplate, DrawingToolType, TimeframeVisibility};
use crate::model::LineStyle;
use crate::styles::typography::FONT_SIZES;
use crate::widget::Chart;
use egui::Color32;

/// Unique identifier for a drawing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DrawingId(pub usize);

/// Drawing properties that can be edited
#[derive(Clone, Debug)]
pub struct DrawingProps {
    /// Symbol whose drawing was opened; absent for manually assembled properties.
    pub symbol: Option<String>,
    /// Tool-specific settings, edited by the Inputs page.
    pub tool_settings: Option<Box<DrawingTemplate>>,
    /// Template loaded from the Template menu, or the tool's defaults after Reset to defaults.
    /// Applying first gives every selected drawing of its tool that accepts user edits all of the
    /// template, then the edited values; drawings of other tools take the edited values.
    pub template: Option<Box<DrawingTemplate>>,
    /// Instrument tick size for position level inputs; absent disables editing by ticks.
    pub tick_size: Option<f64>,
    /// Staged anchor edits, applied only to the drawing whose dialog was opened.
    pub coordinates: CoordinatesState,
    /// Save these properties as the default for new drawings of this tool, under its registered
    /// id in `DrawingManager::custom_tool_defaults` or its built-in type.
    pub save_as_default: bool,
    /// Primary color
    pub color: Color32,
    /// The text colour last picked under Color in the Style tab, for a Text or Anchored text with
    /// a text colour of its own. While it matches the edited text colour, applying gives its hue
    /// to every selected drawing, as a Color pick does on other tools. Properties assembled by
    /// hand leave it `None`.
    pub color_row_text_pick: Option<[u8; 4]>,
    /// Line width (1-10)
    pub line_width: f32,
    /// Line style
    pub line_style: LineStyle,
    /// Fill color (for shapes)
    pub fill_color: Option<Color32>,
    /// Fill enabled
    pub fill_enabled: bool,
    /// Extend line left
    pub extend_left: bool,
    /// Extend line right
    pub extend_right: bool,
    /// Text label
    pub label: String,
    /// Font size
    pub font_size: f32,
    /// The intervals the drawing shows on
    pub timeframe_visibility: TimeframeVisibility,
}

impl Default for DrawingProps {
    fn default() -> Self {
        Self {
            symbol: None,
            tick_size: None,
            tool_settings: None,
            template: None,
            coordinates: CoordinatesState::default(),
            save_as_default: false,
            color: crate::theme::Theme::default().ui.accent,
            color_row_text_pick: None,
            line_width: 1.0,
            line_style: LineStyle::Solid,
            fill_color: None,
            fill_enabled: false,
            extend_left: false,
            extend_right: false,
            label: String::new(),
            font_size: 12.0,
            timeframe_visibility: TimeframeVisibility::all(),
        }
    }
}

/// Actions from the drawing properties dialog
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DrawingPropertiesAction {
    /// No action requested.
    None,
    /// Cancel the current interaction.
    Cancel,
    /// Apply the supplied properties to the drawing.
    Apply(DrawingId, Box<DrawingProps>),
    /// Delete the identified drawing.
    Delete(DrawingId),
    /// Duplicate the identified drawing.
    Clone(DrawingId),
}

impl DrawingProps {
    /// Read every editable property without changing the drawing or its selection.
    pub fn from_drawing(drawing: &Drawing, chart: &Chart) -> Self {
        Self {
            symbol: Some(chart.symbol().to_owned()),
            tick_size: chart
                .instrument()
                .and_then(|instrument| instrument.tick_size),
            tool_settings: Some(Box::new(DrawingTemplate::from_drawing(drawing))),
            coordinates: CoordinatesState::from_drawing(drawing, chart),
            color: drawing.color32(),
            line_width: drawing.stroke_width,
            line_style: drawing.line_style,
            fill_color: drawing.fill_color32(),
            fill_enabled: drawing.fill_color.is_some(),
            extend_left: drawing.extend_left,
            extend_right: drawing.extend_right,
            font_size: drawing.font_size,
            timeframe_visibility: drawing.timeframe_visibility.clone(),
            label: drawing.text.clone().unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Exact saved or picked RGB at the current opacity when it encodes the paint colour.
    pub(super) fn color_rgba(&self) -> [u8; 4] {
        self.tool_settings
            .as_ref()
            .map(|template| &template.settings)
            .filter(|settings| self.paints(settings.color))
            .map_or_else(
                || self.color.to_srgba_unmultiplied(),
                |settings| {
                    let [r, g, b, _] = settings.color;
                    [r, g, b, self.color.a()]
                },
            )
    }

    /// Whether the paint colour shows the given colour's RGB at its own opacity.
    fn paints(&self, [r, g, b, _]: [u8; 4]) -> bool {
        Color32::from_rgba_unmultiplied(r, g, b, self.color.a()) == self.color
    }

    /// Whether one RGB shows as `paint` at that colour's opacity and as the paint colour at its
    /// own, so that the two differ in opacity only as far as either holds its hue.
    fn keeps_hue_of(&self, paint: Color32) -> bool {
        let at = |c: u8, a: u8| Color32::from_rgba_unmultiplied(c, c, c, a).r();
        (0..3).all(|i| {
            (0..=255)
                .any(|c| at(c, paint.a()) == paint[i] && at(c, self.color.a()) == self.color[i])
        })
    }

    /// Validate and apply one undoable edit to the selected drawings that accept user edits.
    /// A value that differs from drawing `id`'s, which these properties were read from, was
    /// edited, and only edited values change the drawings: text only those that show text, tool
    /// inputs only drawings of the same tool, coordinates only `id`. A loaded `template` first
    /// gives every selected drawing of its tool that accepts user edits all of its values.
    /// Errors leave all live drawings unchanged.
    pub fn apply_to_chart(&self, id: DrawingId, chart: &mut Chart) -> Result<(), &'static str> {
        let edits = self.edited_drawings(id, chart)?;
        chart.user_action(|chart| {
            let manager = chart.drawings_mut();
            if !manager.is_selected(id.0) {
                manager.select(id.0);
            }
            let ids: Vec<_> = edits.iter().map(|drawing| drawing.id).collect();
            manager.edit_drawings(&ids, |drawing| {
                if let Some(edited) = edits.iter().find(|edited| edited.id == drawing.id) {
                    *drawing = edited.clone();
                }
            })?;
            if self.save_as_default
                && let Some(drawing) = manager.drawings.iter().find(|d| d.id == id.0)
            {
                manager.save_tool_default(DrawingTemplate::from_drawing(drawing));
            }
            Ok(())
        })
    }

    /// Prepare validated edits without history or persistence side effects, for live preview.
    /// Values compare with drawing `id` as `chart` holds it, with a loaded `template` applied, so
    /// a preview restores its drawings before calling this.
    pub(super) fn edited_drawings(
        &self,
        id: DrawingId,
        chart: &Chart,
    ) -> Result<Vec<Drawing>, &'static str> {
        if self
            .symbol
            .as_ref()
            .is_some_and(|symbol| symbol != chart.symbol())
        {
            return Err("The chart symbol changed; reopen drawing properties");
        }
        self.validate()?;
        let points = self.coordinates.resolve(chart)?;
        let target = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id.0)
            .ok_or("Drawing was removed")?;
        if !chart.drawings().accepts_user_edits(target)
            && (self.save_as_default
                || !chart.drawings().is_selected(id.0)
                || !chart.drawings().drawings.iter().any(|drawing| {
                    chart.drawings().accepts_user_edits(drawing)
                        && chart.drawings().is_selected(drawing.id)
                }))
        {
            return Err(if !target.user_edit_enabled {
                "This drawing cannot be edited"
            } else if chart.drawings().is_locked() {
                "Unlock all drawings before editing its properties"
            } else {
                "Unlock the drawing before editing its properties"
            });
        }
        let adds_position_stop = matches!(
            target.tool_type,
            crate::drawings::DrawingToolType::LongPos | crate::drawings::DrawingToolType::ShortPos
        ) && target.chart_points.len() == 2
            && points.len() == 3;
        if !points.is_empty() && points.len() != target.chart_points.len() && !adds_position_stop {
            return Err("Drawing anchors changed; reopen its properties");
        }
        // A loaded template gives the drawings all of its values, and edits compare with it.
        let mut target = target.clone();
        if let Some(template) = &self.template {
            template.apply_to(&mut target)?;
        }
        let selected = chart.drawings().is_selected(id.0);
        let fill = self.fill_enabled.then(|| {
            self.fill_color.unwrap_or_else(|| {
                let [r, g, b, a] =
                    Drawing::first_fill(target.tool_type, self.color.to_srgba_unmultiplied());
                Color32::from_rgba_unmultiplied(r, g, b, a)
            })
        });
        let [r, g, b, a] = self.color_rgba();
        // Only a recorded Color-row pick propagates text colour through the whole selection.
        let text_pick = self
            .tool_settings
            .as_ref()
            .and_then(|template| template.settings.text_color_override())
            .zip(target.text_color_override())
            .filter(|(picked, _)| self.color_row_text_pick == Some(*picked));
        let hue_edited = if let Some((picked, shown)) = text_pick {
            picked[..3] != shown[..3]
        } else {
            // Without tool settings only the paint colour is held, and it reads back as another
            // hue at each opacity: the hue was edited unless one RGB paints both colours.
            [r, g, b] != target.color[..3]
                && (self.tool_settings.is_some() || !self.keeps_hue_of(target.color32()))
        };
        let mut edits = Vec::new();
        for mut drawing in chart
            .drawings()
            .drawings
            .iter()
            .filter(|drawing| {
                chart.drawings().accepts_user_edits(drawing)
                    && if selected {
                        chart.drawings().is_selected(drawing.id)
                    } else {
                        drawing.id == id.0
                    }
            })
            .cloned()
        {
            if let Some(template) = &self.template {
                // A drawing of another tool refuses it and takes the edited values only.
                let _ = template.apply_to(&mut drawing);
            }
            // The dialog opened with the target's values: one that differs from them was edited,
            // and only edited values are written, so each drawing keeps the rest of its own.
            if hue_edited {
                let [.., opacity] = drawing.text_color_override().unwrap_or(drawing.color);
                drawing.recolor([r, g, b, opacity]);
            }
            if a != target.color[3] {
                drawing.color[3] = a;
                if drawing.text_color_override().is_some() {
                    drawing.text_color = drawing.text_color.map(|[r, g, b, _]| [r, g, b, a]);
                }
            }
            if let Some((picked, shown)) = text_pick
                && picked[3] != shown[3]
                && matches!(
                    drawing.tool_type,
                    DrawingToolType::TextLabel | DrawingToolType::AnchoredText
                )
            {
                drawing.text_color.as_mut().unwrap_or(&mut drawing.color)[3] = picked[3];
            }
            if self.line_width != target.stroke_width {
                drawing.stroke_width = self.line_width;
            }
            if self.line_style != target.line_style {
                drawing.line_style = self.line_style;
            }
            if self.extend_left != target.extend_left {
                drawing.extend_left = self.extend_left;
            }
            if self.extend_right != target.extend_right {
                drawing.extend_right = self.extend_right;
            }
            // A tool that shows no text keeps it: it holds a sticker, an image or an icon.
            if self.label != target.text.as_deref().unwrap_or_default()
                && (drawing.id == id.0 || drawing.tool_type.supports_text())
            {
                drawing.text = (!self.label.is_empty()).then(|| self.label.clone());
            }
            if fill != target.fill_color32()
                && drawing.fill_color32() != fill
                && (drawing.id == id.0
                    || drawing.tool_type.supports_fill()
                    || (has_text_box(target.tool_type) && has_text_box(drawing.tool_type)))
            {
                drawing.fill_color = fill.map(|color| color.to_srgba_unmultiplied());
            }
            if self.font_size != target.font_size {
                drawing.font_size = self.font_size;
            }
            if self.timeframe_visibility != target.timeframe_visibility {
                drawing.timeframe_visibility = self.timeframe_visibility.clone();
            }
            if let Some(template) = &self.tool_settings
                && template.settings.tool_type == drawing.tool_type
                && template.settings.custom_tool == drawing.custom_tool
            {
                let settings = &template.settings;
                // A tool setting that differs from the target's was edited.
                macro_rules! edited {
                    ($($field:ident $(.$member:ident)?),+ $(,)?) => {
                        $(if settings.$field$(.$member)? != target.$field$(.$member)? {
                            drawing.$field$(.$member)?.clone_from(&settings.$field$(.$member)?);
                        })+
                    };
                }
                // Colour-row picks already keep each text opacity above unless it was edited.
                if text_pick.is_none() {
                    edited!(text_color);
                }
                edited!(
                    font_weight,
                    italic,
                    glyph_angle,
                    background,
                    border,
                    border_color,
                    text_wrap,
                    text_wrap_width,
                    text_follows_zoom,
                    note_text_style,
                    arrow_start_style,
                    arrow_end_style,
                    position_size,
                    position_sizing,
                    position_readout.persistent,
                    position_readout.condensed,
                    position_readout.quantity_decimals,
                    position_target_color,
                    position_stop_color,
                    position_style,
                    marker.show_text,
                    marker.glyph,
                    marker.show_glyph,
                    marker.glyph_color,
                    channel_midline,
                    anchor_price_style,
                    middle_line,
                    theme_shade,
                    line_measurements.price_change,
                    line_measurements.percent_change,
                    line_measurements.pip_change,
                    line_measurements.bar_count,
                    line_measurements.elapsed_time,
                    line_measurements.length,
                    line_measurements.inclination,
                    line_measurements.persistent,
                    line_measurements.midpoint,
                    line_measurements.price_labels,
                    line_measurements.position,
                    line_measurements.text_horizontal_align,
                    line_measurements.text_vertical_align,
                    line_measurements.text_orientation,
                    line_measurements.across_panes,
                    range_settings.price_change,
                    range_settings.percent_change,
                    range_settings.pip_change,
                    range_settings.bar_count,
                    range_settings.elapsed_time,
                    range_settings.volume,
                    range_settings.text_color,
                    range_settings.font_size,
                    range_settings.background,
                    range_settings.background_color,
                    range_settings.shadow_color,
                    range_settings.border,
                    range_settings.border_color,
                    range_settings.border_width,
                    range_settings.border_style,
                    range_settings.extend_top,
                    range_settings.extend_bottom,
                    price_axis_label,
                    time_axis_label,
                    elliott_degree,
                    elliott_show_wave,
                );
                // Each config takes only its edited members, keeping the peer's other settings.
                macro_rules! edited_config {
                    ($($field:ident => $shown:expr),+ $(,)?) => {
                        $(if settings.$field != target.$field {
                            let shown = $shown;
                            drawing.$field = settings.$field.as_ref().map(|edited| {
                                merged(&shown(&drawing), &shown(&target), edited)
                            });
                        })+
                    };
                }
                edited_config!(
                    pitchfork => super::tabs::pitchfork_shown,
                    gann => |d: &Drawing| d.gann_config().clone(),
                    ghost_feed => |d: &Drawing| d.ghost_feed.unwrap_or_default(),
                    bars_pattern => |d: &Drawing| d.bars_pattern.unwrap_or_default(),
                    projection => |d: &Drawing| d.projection_config().into_owned(),
                    forecast => |d: &Drawing| d.forecast_style(),
                    volume_profile => |d: &Drawing| d.volume_profile.unwrap_or_default(),
                    regression_trend => |d: &Drawing| d.regression_trend.unwrap_or_default(),
                    vwap => |d: &Drawing| d.vwap.unwrap_or_default(),
                    table => |d: &Drawing| d.table.clone().unwrap_or_default(),
                );
                if settings.custom_settings != target.custom_settings {
                    merge_edits(
                        &mut drawing.custom_settings,
                        &target.custom_settings,
                        &settings.custom_settings,
                    );
                }
                if settings.fib_config != target.fib_config {
                    if matches!(
                        drawing.tool_type,
                        crate::drawings::DrawingToolType::FibonacciRetracement
                            | crate::drawings::DrawingToolType::FibonacciCircles
                            | crate::drawings::DrawingToolType::FibonacciSpeedResistanceArcs
                            | crate::drawings::DrawingToolType::FibonacciSpiral
                            | crate::drawings::DrawingToolType::FibonacciExtension
                            | crate::drawings::DrawingToolType::FibonacciChannel
                            | crate::drawings::DrawingToolType::FibonacciWedge
                            | crate::drawings::DrawingToolType::FibonacciSpeedFan
                            | crate::drawings::DrawingToolType::FibonacciTimeZones
                            | crate::drawings::DrawingToolType::TrendBasedFibTime
                    ) {
                        let original = target.fibonacci_config();
                        let edited = settings.fibonacci_config();
                        let mut config = drawing.fibonacci_config().into_owned();
                        config.apply_edits(&original, &edited);
                        // An edited fan saved before it had time levels takes the current look.
                        if drawing.tool_type == crate::drawings::DrawingToolType::FibonacciSpeedFan
                            && config.time_levels.is_none()
                        {
                            config.time_levels = Some(config.levels.clone());
                        }
                        drawing.fib_config = Some(config);
                    } else {
                        drawing.fib_config.clone_from(&settings.fib_config);
                    }
                }
                if settings.position_readout.account_currency
                    != target.position_readout.account_currency
                {
                    drawing
                        .position_readout
                        .account_currency
                        .clone_from(&settings.position_readout.account_currency);
                    // A peer's old rate cannot follow it into a different currency.
                    drawing.position_readout.conversion_rate =
                        settings.position_readout.conversion_rate;
                } else if settings.position_readout.conversion_rate
                    != target.position_readout.conversion_rate
                    && drawing.position_readout.account_currency
                        == settings.position_readout.account_currency
                {
                    drawing.position_readout.conversion_rate =
                        settings.position_readout.conversion_rate;
                }
                // Only the image's opacity: each picture keeps its own frame size.
                let opacity = settings.image.map_or(255, |c| c.opacity);
                if opacity != target.image.map_or(255, |c| c.opacity) {
                    drawing.image.get_or_insert_with(Default::default).opacity = opacity;
                }
            }
            if drawing.id == id.0
                && !points.is_empty()
                && (drawing.chart_points.len() != points.len()
                    || drawing
                        .chart_points
                        .iter()
                        .zip(&points)
                        .any(|(a, b)| a != b || a.time_anchor != b.time_anchor))
            {
                drawing.chart_points = points.clone();
                drawing.screen_anchor = None;
            }
            edits.push(drawing);
        }
        Ok(edits)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !(self.line_width > 0.0
            && self.line_width <= MAX_STROKE_WIDTH
            && FONT_SIZES.contains(&self.font_size))
        {
            return Err("Line width must be above 0 and at most 100, font size 1 to 256");
        }
        if let Some(template) = &self.tool_settings {
            let settings = &template.settings;
            if settings
                .fib_config
                .as_ref()
                .is_some_and(|c| c.levels.iter().any(|l| !l.value.is_finite()))
            {
                return Err("Fibonacci ratios must be finite");
            }
            if settings.fib_config.as_ref().is_some_and(|c| !c.is_valid()) {
                return Err("Fibonacci widths must be above 0 and at most 100, font size 1 to 256");
            }
            if settings.pitchfork.as_ref().is_some_and(|c| {
                c.levels
                    .iter()
                    .any(|l| !(0.0..=MAX_PITCHFORK_LEVEL).contains(&l.value))
            }) {
                return Err("Pitchfork multiples must be between 0 and 100");
            }
            if settings.pitchfork.as_ref().is_some_and(|c| {
                c.levels
                    .iter()
                    .any(|l| l.width.is_some_and(|w| !(w > 0.0 && w <= MAX_STROKE_WIDTH)))
            }) {
                return Err("Pitchfork widths must be above 0 and at most 100");
            }
            if settings.projection.as_ref().is_some_and(|c| !c.is_valid()) {
                return Err(
                    "Projection coefficients must be between 0 and 100, widths above 0 and at most 100",
                );
            }
            if let Some(c) = settings.gann.as_ref() {
                let square = matches!(
                    settings.tool_type,
                    DrawingToolType::GannSquare | DrawingToolType::GannFixed
                );
                let max = if square { GANN_SQUARE_UNITS } else { 1.0 };
                if c.price_levels
                    .iter()
                    .chain(&c.time_levels)
                    .any(|l| !(0.0..=max).contains(&l.value))
                {
                    return Err(if square {
                        "Gann square levels must be between 0 and 5"
                    } else {
                        "Gann levels must be between 0 and 1"
                    });
                }
                if c.fan_ratios
                    .iter()
                    .chain(&c.arc_ratios)
                    .any(|l| !(l.value.is_finite() && l.value > 0.0))
                {
                    return Err("Gann fan and arc ratios must be positive and finite");
                }
                if c.price_levels
                    .iter()
                    .chain(&c.time_levels)
                    .chain(&c.fan_ratios)
                    .chain(&c.arc_ratios)
                    .any(|l| l.width.is_some_and(|w| !(w > 0.0 && w <= MAX_STROKE_WIDTH)))
                {
                    return Err("Gann widths must be above 0 and at most 100");
                }
            }
            if settings.ghost_feed.is_some_and(|c| !c.is_valid()) {
                return Err(
                    "Ghost feed average range must be positive and noise nonnegative, both finite",
                );
            }
            if !settings.position_readout.is_valid() {
                return Err("Invalid position readout settings");
            }
            if settings
                .note_text_style
                .is_some_and(|style| !style.is_valid())
            {
                return Err("Price note text size must be 1 to 256");
            }
            if settings
                .anchor_price_style
                .is_some_and(|style| !style.is_valid())
            {
                return Err("Price text size must be 1 to 256");
            }
            if !(settings.middle_line.width > 0.0 && settings.middle_line.width <= MAX_STROKE_WIDTH)
            {
                return Err("Middle line width must be above 0 and at most 100");
            }
            if !settings.position_style.is_valid() {
                return Err("Position line width must be 1 to 100 and font size 1 to 256");
            }
            if !settings.range_settings.is_valid() {
                return Err(
                    "Range label size must be 1 to 256 and border width above 0 and at most 100",
                );
            }
            if !settings.marker.is_valid() {
                return Err("Glyph must be 1 to 32 bytes");
            }
            if !settings.position_sizing.is_valid() {
                return Err(
                    "Position economics must be positive and finite; quantity step cannot be negative",
                );
            }
            if let Some(size) = settings.position_size
                && size.quantity(1.0, 0.0).is_none()
            {
                return Err("Position size must be positive and finite; risk cannot exceed 100%");
            }
        }
        Ok(())
    }
}

/// Writes changed members into `own`, descending through objects with the same members and
/// through arrays edited without a change of length, over the items `own` holds, as Fibonacci
/// levels are. A changed shape, such as a level list gaining an item, replaces the whole value.
fn merge_edits(
    own: &mut serde_json::Value,
    original: &serde_json::Value,
    edited: &serde_json::Value,
) {
    use serde_json::Value;
    if original == edited {
        return;
    }
    match (own, original, edited) {
        (Value::Object(own), Value::Object(original), Value::Object(edited))
            if own.len() == edited.len()
                && original.len() == edited.len()
                && edited
                    .keys()
                    .all(|key| own.contains_key(key) && original.contains_key(key)) =>
        {
            for (key, edited) in edited {
                if let Some(own) = own.get_mut(key) {
                    merge_edits(own, &original[key], edited);
                }
            }
        }
        // An RGBA colour's red, green and blue are one hue, replaced whole as the drawing
        // colour's is, so that no drawing takes a mix of its own and the picked channels.
        (Value::Array(own), Value::Array(original), Value::Array(edited))
            if [&own[..], &original[..], &edited[..]].iter().all(|colour| {
                colour.len() == 4 && colour.iter().all(|c| c.as_u64().is_some_and(|c| c <= 255))
            }) =>
        {
            if original[..3] != edited[..3] {
                own[..3].clone_from_slice(&edited[..3]);
            }
            if original[3] != edited[3] {
                own[3] = edited[3].clone();
            }
        }
        (Value::Array(own), Value::Array(original), Value::Array(edited))
            if original.len() == edited.len() =>
        {
            for ((own, original), edited) in own.iter_mut().zip(original).zip(edited) {
                merge_edits(own, original, edited);
            }
        }
        (own, _, edited) => *own = edited.clone(),
    }
}

/// Apply the changed members, retaining the former whole-config copy if the merged settings
/// cannot round trip, for example when a peer holds a non-finite number.
fn merged<T: serde::Serialize + serde::de::DeserializeOwned + Clone>(
    own: &T,
    original: &T,
    edited: &T,
) -> T {
    let json = |config: &T| serde_json::to_value(config).ok();
    let (Some(mut value), Some(original), Some(changed)) =
        (json(own), json(original), json(edited))
    else {
        return edited.clone();
    };
    merge_edits(&mut value, &original, &changed);
    serde_json::from_value(value).unwrap_or_else(|_| edited.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Existing drawing settings survive opening and applying the properties, bulk edits over the
    /// selection stay atomic, and every tool's saved settings reach the drawing.
    #[test]
    fn props_round_trip_visibility_fill_fonts_and_tool_settings() {
        use crate::drawings::{Drawing, DrawingToolType, TimeframeVisibility};
        use crate::model::{BarData, LineStyle};
        use crate::widget::Chart;
        let mut chart = Chart::new(BarData::new());
        let mut drawing = Drawing::new(7, DrawingToolType::TextLabel);
        drawing.line_style = LineStyle::Dotted;
        drawing.fill_color = Some([51, 102, 153, 85]);
        drawing.font_size = 24.0;
        drawing.extend_left = true;
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        drawing.text = Some("Annotation".into());
        drawing.locked = true;
        chart.drawings_mut().drawings.push(drawing);
        let props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        assert_eq!(
            props.timeframe_visibility,
            TimeframeVisibility::specific(&["1h"])
        );
        assert!(props.fill_enabled);
        assert_eq!(
            props.fill_color.unwrap().to_srgba_unmultiplied(),
            [51, 102, 153, 85]
        );
        assert_eq!(props.font_size, 24.0);
        assert!(matches!(props.line_style, LineStyle::Dotted));
        assert!(props.extend_left);
        assert!(
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart)
            .is_err()
        );
        assert_eq!(chart.drawings().drawings[0].font_size, 24.0);
        chart
            .drawings_mut()
            .edit_drawing(7, |d| d.locked = false)
            .unwrap();
        chart
            .drawings_mut()
            .edit_drawing(7, |d| d.user_edit_enabled = false)
            .unwrap();
        assert_eq!(
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart),
            Err("This drawing cannot be edited")
        );
        assert_eq!(chart.drawings().drawings[0].font_size, 24.0);
        chart
            .drawings_mut()
            .edit_drawing(7, |d| d.user_edit_enabled = true)
            .unwrap();
        chart.drawings_mut().set_locked(true);
        assert_eq!(
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart),
            Err("Unlock all drawings before editing its properties")
        );
        assert_eq!(chart.drawings().drawings[0].font_size, 24.0);
        chart.drawings_mut().set_locked(false);
        // A size the drawings file would refuse cannot be applied.
        assert!(
            DrawingProps {
                font_size: 300.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart)
            .is_err()
        );
        props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
        let mut second = chart.drawings().drawings[0].clone();
        second.id = 8;
        chart.drawings_mut().drawings.push(second);
        chart.drawings_mut().select_drawings([7, 8]);
        let mut visibility = TimeframeVisibility::default();
        visibility.set_range(crate::model::TimeframeUnit::Hour, Some(2..=4));
        let changed = DrawingProps {
            timeframe_visibility: visibility,
            font_size: 28.0,
            fill_enabled: false,
            ..props.clone()
        };
        changed.apply_to_chart(DrawingId(7), &mut chart).unwrap();
        let saved = &chart.drawings().drawings[0];
        assert!(saved.timeframe_visibility.is_visible_on("4h"));
        assert!(!saved.timeframe_visibility.is_visible_on("1h"));
        assert_eq!(saved.fill_color, None);
        assert_eq!(saved.font_size, 28.0);
        assert_eq!(chart.drawings().drawings[1].font_size, 28.0);
        assert!(chart.undo());
        assert!(
            chart
                .drawings()
                .drawings
                .iter()
                .all(|d| d.font_size == 24.0)
        );
        assert!(chart.redo());

        use crate::data::VolumeProfileConfig;
        use crate::drawings::{
            FibonacciConfig, GannConfig, GannLevel, PitchforkConfig, PositionSize, PositionSizing,
            RegressionTrendConfig,
        };
        for tool in [
            DrawingToolType::FibonacciRetracement,
            DrawingToolType::Pitchfork,
            DrawingToolType::GannBox,
            DrawingToolType::LongPos,
            DrawingToolType::ShortPos,
            DrawingToolType::FixedRangeVolumeProfile,
            DrawingToolType::RegressionTrend,
            DrawingToolType::ParallelChannel,
            DrawingToolType::DisjointChannel,
            DrawingToolType::Rect,
            DrawingToolType::TrendLine,
            DrawingToolType::Ray,
            DrawingToolType::ExtendedLine,
            DrawingToolType::InfoLine,
            DrawingToolType::TrendAngle,
            DrawingToolType::Arrow,
            DrawingToolType::Path,
            DrawingToolType::Polyline,
            DrawingToolType::HorizontalLine,
            DrawingToolType::HorizontalRay,
            DrawingToolType::VerticalLine,
            DrawingToolType::CrossLine,
            DrawingToolType::DateRange,
            DrawingToolType::DateAndPriceRange,
        ] {
            chart.drawings_mut().drawings.clear();
            let mut drawing = Drawing::new(7, tool);
            // A shown middle line, so that its style rows would show on a rectangle if offered.
            drawing.channel_midline = true;
            chart.drawings_mut().drawings.push(drawing);
            let mut peer = Drawing::new(8, DrawingToolType::TextLabel);
            // A boxed label, so that the settings below would show on it if they leaked.
            (peer.background, peer.border, peer.border_color) = (true, true, None);
            peer.chart_points
                .push(crate::drawings::ChartPoint::new(5.0, 75.0));
            chart.drawings_mut().drawings.push(peer);
            chart.drawings_mut().select_drawings([7, 8]);
            let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
            let output = egui::Context::default().run_ui(Default::default(), |ui| {
                let mut pickers =
                    [(); 3].map(|_| crate::ui_kit::ColorPickerState::new(props.color));
                super::super::tabs::StyleTab::show(
                    ui,
                    &mut props,
                    &mut pickers,
                    &mut Default::default(),
                    &mut Default::default(),
                    None,
                );
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            let line = tool.supports_line_ends();
            assert_eq!(labels.contains(&"Line start"), line, "{tool:?}");
            assert_eq!(labels.contains(&"Line end"), line, "{tool:?}");
            let middle = tool.supports_middle_line();
            assert_eq!(labels.contains(&"Middle line"), middle, "{tool:?}");
            // A new parallel channel shows its middle line and styles it; a rectangle's is fixed.
            assert_eq!(
                labels.contains(&"Middle line width"),
                tool == DrawingToolType::ParallelChannel,
                "{tool:?}"
            );
            use DrawingToolType as T;
            let (price, time) = (
                matches!(
                    tool,
                    T::HorizontalLine | T::HorizontalRay | T::CrossLine | T::LongPos | T::ShortPos
                ),
                matches!(tool, T::VerticalLine | T::CrossLine),
            );
            assert_eq!(labels.contains(&"Price"), price, "{tool:?}");
            assert_eq!(labels.contains(&"Time"), time, "{tool:?}");
            assert_eq!(
                labels.contains(&"Through all panes"),
                tool == T::VerticalLine,
                "{tool:?}"
            );
            for label in [
                "Middle point",
                "Keep statistics visible",
                "Price labels",
                "Price change",
                "Percentage change",
                "Pip change",
                "Bar count",
                "Elapsed time",
                "Length in periods",
                "Inclination",
                "Statistics placement",
            ] {
                assert_eq!(
                    labels.contains(&label),
                    tool.supports_line_measurements(),
                    "{tool:?}: {label}"
                );
            }
            output.drop_without_applying_deltas();
            let settings = &mut props.tool_settings.as_mut().unwrap().settings;
            settings.fib_config = Some(FibonacciConfig {
                reverse: true,
                ..Default::default()
            });
            settings.projection = Some(crate::drawings::ProjectionConfig {
                show_background: false,
                ..Default::default()
            });
            settings.pitchfork = Some(PitchforkConfig {
                background_opacity: 75,
                ..Default::default()
            });
            settings.gann = Some(GannConfig {
                price_levels: [0.0, 0.5, 1.0]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
                time_levels: [0.25, 0.75]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
                ..Default::default()
            });
            settings.position_size = Some(PositionSize::RiskPercent {
                account_size: 10_000.0,
                percent: 1.0,
            });
            settings.position_sizing = PositionSizing {
                point_value: 50.0,
                lot_size: 10.0,
                quantity_step: 0.01,
                leverage: 5.0,
            };
            let sizing = settings.position_sizing;
            settings.position_target_color = Some([40, 100, 220, 255]);
            settings.position_stop_color = Some([220, 140, 40, 255]);
            settings.position_style.font_size = 16.0;
            let marker = crate::drawings::AnnotationMarker {
                show_text: false,
                glyph: "🚀".into(),
                show_glyph: true,
                glyph_color: Some([7, 8, 9, 255]),
            };
            settings.marker = marker.clone();
            settings.volume_profile = VolumeProfileConfig::new(48, 0.8);
            settings.regression_trend = RegressionTrendConfig::new(3.0);
            settings.channel_midline = true;
            let measurements = crate::drawings::LineMeasurements {
                persistent: true,
                price_change: true,
                percent_change: true,
                pip_change: true,
                bar_count: true,
                elapsed_time: true,
                length: true,
                inclination: true,
                midpoint: true,
                position: crate::drawings::LineMeasurementPosition::Left,
                price_labels: true,
                text_horizontal_align: crate::drawings::LineTextHorizontalAlign::Right,
                text_vertical_align: crate::drawings::LineTextVerticalAlign::Top,
                text_orientation: crate::drawings::LineTextOrientation::Horizontal,
                across_panes: true,
            };
            settings.line_measurements = measurements;
            settings.bars_pattern = Some(crate::drawings::BarsPatternConfig {
                mirrored: true,
                ..Default::default()
            });
            settings.price_axis_label = false;
            settings.time_axis_label = false;
            let mut vwap = crate::drawings::VwapConfig::default();
            vwap.bands[2] = crate::drawings::VwapBand::new(true, 0.5).unwrap();
            settings.vwap = Some(vwap);
            settings.elliott_degree = crate::drawings::ElliottDegree::Minute;
            settings.italic = true;
            settings.text_color = Some([1, 2, 3, 255]);
            settings.background = false;
            settings.border = false;
            settings.border_color = Some([4, 5, 6, 255]);
            settings.text_wrap = true;
            settings.text_wrap_width = 120.0;
            settings.text_follows_zoom = true;
            let note_text = crate::drawings::NoteTextStyle {
                color: [7, 8, 9, 255],
                font_size: 18.0,
                bold: true,
                italic: false,
            };
            settings.note_text_style = Some(note_text);
            settings.arrow_start_style = crate::drawings::ArrowStyle::Arrow;
            settings.arrow_end_style = crate::drawings::ArrowStyle::None;
            settings.range_settings.shadow_color = [50, 60, 70, 128];
            let expected = format!("{:?}", settings.fib_config);
            let template: DrawingTemplate = serde_json::from_value(
                serde_json::to_value(props.tool_settings.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(
                template.settings.range_settings.shadow_color,
                [50, 60, 70, 128]
            );
            props.tool_settings = Some(Box::new(template));
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            let saved = &chart.drawings().drawings[0];
            assert_eq!(format!("{:?}", saved.fib_config), expected);
            assert_eq!(saved.pitchfork.as_ref().unwrap().background_opacity, 75);
            assert!(!saved.projection.as_ref().unwrap().show_background);
            assert_eq!(
                saved
                    .gann
                    .as_ref()
                    .unwrap()
                    .time_levels
                    .iter()
                    .map(|l| l.value)
                    .collect::<Vec<_>>(),
                [0.25, 0.75]
            );
            assert_eq!(
                saved.position_size.unwrap().quantity(100.0, 98.0),
                Some(50.0)
            );
            assert_eq!(saved.volume_profile.unwrap().rows(), 48);
            assert_eq!(saved.position_sizing, sizing);
            assert_eq!(saved.position_target_color, Some([40, 100, 220, 255]));
            assert_eq!(saved.position_stop_color, Some([220, 140, 40, 255]));
            assert_eq!(saved.position_style.font_size, 16.0);
            assert_eq!(saved.marker, marker);
            assert_eq!(saved.regression_trend.unwrap().upper_deviation(), 3.0);
            assert!(saved.channel_midline);
            assert_eq!(saved.line_measurements, measurements);
            assert!(saved.bars_pattern.unwrap().mirrored);
            assert!(
                !saved.price_axis_label && !saved.time_axis_label,
                "{tool:?}"
            );
            assert_eq!(saved.vwap, Some(vwap));
            assert_eq!(saved.elliott_degree, crate::drawings::ElliottDegree::Minute);
            assert!(saved.italic && !saved.background && !saved.border);
            assert_eq!(saved.text_color, Some([1, 2, 3, 255]));
            assert_eq!(saved.border_color, Some([4, 5, 6, 255]));
            assert_eq!((saved.text_wrap, saved.text_wrap_width), (true, 120.0));
            assert!(saved.text_follows_zoom);
            assert_eq!(saved.note_text_style, Some(note_text));
            assert_eq!(saved.arrow_start_style, crate::drawings::ArrowStyle::Arrow);
            assert_eq!(saved.arrow_end_style, crate::drawings::ArrowStyle::None);
            assert_eq!(saved.range_settings.shadow_color, [50, 60, 70, 128]);
            assert!(chart.drawings().drawings[1].fib_config.is_none());
            let peer = &chart.drawings().drawings[1];
            assert!(!peer.italic && peer.background && peer.border);
            assert!(peer.price_axis_label && peer.time_axis_label);
            assert_eq!((peer.text_color, peer.border_color), (None, None));
            assert_eq!((peer.text_wrap, peer.text_wrap_width), (false, 200.0));
            assert!(!peer.text_follows_zoom);
            assert_eq!(peer.note_text_style, None);
            assert_eq!(chart.drawings().drawings[1].chart_points[0].price, 75.0);
            assert!(chart.undo());
            assert_eq!(
                chart.drawings().drawings[0].range_settings.shadow_color,
                Drawing::new(7, tool).range_settings.shadow_color
            );
            assert_eq!(
                chart.drawings().drawings[0].line_measurements,
                crate::drawings::LineMeasurements::for_tool(tool)
            );
            assert_eq!(
                format!("{:?}", chart.drawings().drawings[0].fib_config),
                format!("{:?}", Drawing::new(7, tool).fib_config)
            );
            assert!(chart.redo());
            assert_eq!(
                chart.drawings().drawings[0].range_settings.shadow_color,
                [50, 60, 70, 128]
            );
            chart
                .drawings_mut()
                .edit_drawing(7, |d| d.locked = true)
                .unwrap();
            props.font_size = 18.0;
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            assert_eq!(
                chart.drawings().drawings[0].font_size,
                Drawing::new(7, tool).font_size
            );
            assert_eq!(chart.drawings().drawings[1].font_size, 18.0);
            // Malformed ratios or widths cannot partly change another selected drawing.
            for (ratio, width) in [(f32::NAN, 2.0), (1.0, 0.0), (1.0, 100.01), (1.0, f32::NAN)] {
                let config = props
                    .tool_settings
                    .as_mut()
                    .unwrap()
                    .settings
                    .fib_config
                    .as_mut()
                    .unwrap();
                config.levels[0].value = ratio;
                config.level_width = width;
                props.line_width = 9.0;
                assert!(props.apply_to_chart(DrawingId(7), &mut chart).is_err());
                assert_eq!(
                    chart.drawings().drawings[0].stroke_width,
                    Drawing::new(7, tool).stroke_width
                );
                assert_eq!(
                    chart.drawings().drawings[1].stroke_width,
                    Drawing::new(8, DrawingToolType::TextLabel).stroke_width
                );
            }
        }
    }

    /// Editing another property keeps each count's degree; editing the degree reaches matching tools.
    #[test]
    fn selection_preserves_wave_degrees_until_the_degree_is_edited() {
        use crate::drawings::{DrawingToolType::*, ElliottDegree::*};
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = [
            (ElliottImpulse, Minor),
            (ElliottImpulse, Minute),
            (ElliottCorrection, Cycle),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (tool, degree))| {
            let mut drawing = Drawing::new(id, tool);
            drawing.elliott_degree = degree;
            drawing
        })
        .collect();
        chart.drawings_mut().select_drawings([0, 1, 2]);
        let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        // A colour edit keeps the degrees of the other selected counts.
        props.color = Color32::GREEN;
        props.apply_to_chart(DrawingId(0), &mut chart).unwrap();
        assert_eq!(
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.elliott_degree)
                .collect::<Vec<_>>(),
            [Minor, Minute, Cycle]
        );
        // A degree edit reaches the other impulse, leaving the correction's degree alone.
        props
            .tool_settings
            .as_mut()
            .unwrap()
            .settings
            .elliott_degree = Minuette;
        props.apply_to_chart(DrawingId(0), &mut chart).unwrap();
        assert_eq!(
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.elliott_degree)
                .collect::<Vec<_>>(),
            [Minuette, Minuette, Cycle]
        );
    }

    /// Confirming the dialog of one drawing in a selection changes the others only where it was
    /// edited: each keeps its own text, width, extension, visibility and tool settings, and an
    /// edited text reaches only the drawings that show text. A recoloured price label keeps its
    /// badge one colour.
    #[test]
    fn selection_edits_change_only_the_edited_values() {
        use crate::drawings::{
            DrawingToolType::{PriceLabel, Sticker},
            FontWeight,
        };
        let mut alpha = Drawing::new(1, DrawingToolType::TextLabel);
        alpha.text = Some("Alpha".into());
        let mut beta = Drawing::new(2, DrawingToolType::TextLabel);
        beta.text = Some("Beta".into());
        beta.color = [0, 128, 0, 255];
        beta.font_weight = FontWeight::Bold;
        beta.italic = true;
        let mut line = Drawing::new(3, DrawingToolType::TrendLine);
        line.text = Some("Breakout".into());
        line.stroke_width = 3.0;
        line.extend_right = true;
        line.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        // A sticker keeps its kind in its text, which it does not show.
        let sticker = Drawing::new(4, DrawingToolType::Sticker);
        // A price label shows no text, and its border shares the colour of its badge, as an older
        // save holds it.
        let mut label = Drawing::new(5, DrawingToolType::PriceLabel);
        label.border_color = Some(label.color);
        let drawings = vec![alpha, beta, line, sticker, label];
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = drawings.clone();
        chart.drawings_mut().select_drawings([1, 2, 3, 4, 5]);
        let opened = DrawingProps::from_drawing(&drawings[0], &chart);
        // Each case: the edit made in the dialog, then what it changes on each drawing.
        type Case = (&'static str, fn(&mut DrawingProps), fn(&mut Drawing));
        let cases: [Case; 5] = [
            ("nothing edited", |_| {}, |_| {}),
            (
                "opacity only",
                // The custom editor returns the saved hue at the new opacity.
                |props| {
                    let [r, g, b, _] = props.color_rgba();
                    props.color = Color32::from_rgba_unmultiplied(r, g, b, 84);
                },
                |d| d.color[3] = 84,
            ),
            (
                "colour",
                |props| props.color = Color32::BLUE,
                |d| {
                    d.color = [0, 0, 255, 255];
                    if d.tool_type == PriceLabel {
                        d.border_color = Some([0, 0, 255, 255]);
                    }
                },
            ),
            (
                "text",
                |props| props.label = "Gamma".into(),
                |d| {
                    if !matches!(d.tool_type, Sticker | PriceLabel) {
                        d.text = Some("Gamma".into());
                    }
                },
            ),
            (
                "text cleared",
                |props| props.label.clear(),
                |d| {
                    if !matches!(d.tool_type, Sticker | PriceLabel) {
                        d.text = None;
                    }
                },
            ),
        ];
        for (case, edit, change) in cases {
            let mut props = opened.clone();
            edit(&mut props);
            let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
            let mut expected = drawings.clone();
            expected.iter_mut().for_each(change);
            assert_eq!(format!("{edits:?}"), format!("{expected:?}"), "{case}");
        }
        // Editing typography or one caption leaves every other saved level of a peer intact.
        chart.drawings_mut().drawings = (0..2)
            .map(|id| {
                let mut drawing = Drawing::new(id, DrawingToolType::FibonacciExtension);
                let config = drawing.fib_config.as_mut().unwrap();
                config.levels[1].text = format!("Peer {id}");
                config.levels[1].value = 0.25 + id as f32;
                config.level_width = 2.0 + id as f32;
                drawing
            })
            .collect();
        chart.drawings_mut().select_drawings([0, 1]);
        let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        let config = props
            .tool_settings
            .as_mut()
            .unwrap()
            .settings
            .fib_config
            .as_mut()
            .unwrap();
        config.label_size = 18.0;
        config.levels[0].text = "Target".into();
        let edits = props.edited_drawings(DrawingId(0), &chart).unwrap();
        for (index, drawing) in edits.iter().enumerate() {
            let config = drawing.fib_config.as_ref().unwrap();
            assert_eq!(config.label_size, 18.0);
            assert_eq!(config.levels[0].text, "Target");
            assert_eq!(config.levels[1].text, format!("Peer {index}"));
            assert_eq!(config.levels[1].value, 0.25 + index as f32);
            assert_eq!(config.level_width, 2.0 + index as f32);
        }
        // A fan saved before it had time levels takes its price levels as time levels, and with
        // them the current look, on any edit of its settings.
        let mut fan = Drawing::new(0, DrawingToolType::FibonacciSpeedFan);
        fan.fib_config.as_mut().unwrap().time_levels = None;
        chart.drawings_mut().drawings = vec![fan];
        chart.drawings_mut().select_drawings([0]);
        let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        let settings = &mut props.tool_settings.as_mut().unwrap().settings;
        settings.fib_config.as_mut().unwrap().grid_visible = false;
        let edits = props.edited_drawings(DrawingId(0), &chart).unwrap();
        let config = edits[0].fib_config.as_ref().unwrap();
        assert!(!config.grid_visible);
        assert_eq!(config.time_levels.as_ref(), Some(&config.levels));
        // A rate belongs to an account currency; changing the currency also replaces its rate.
        for currency_changed in [false, true] {
            chart.drawings_mut().drawings = [("EUR", 0.8), ("GBP", 0.6), ("EUR", 0.7)]
                .into_iter()
                .enumerate()
                .map(|(id, (currency, rate))| {
                    let mut drawing = Drawing::new(id, DrawingToolType::LongPos);
                    drawing.position_readout.account_currency = Some(currency.into());
                    drawing.position_readout.conversion_rate = Some(rate);
                    drawing
                })
                .collect();
            chart.drawings_mut().select_drawings([0, 1, 2]);
            let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
            let readout = &mut props
                .tool_settings
                .as_mut()
                .unwrap()
                .settings
                .position_readout;
            readout.conversion_rate = Some(0.9);
            if currency_changed {
                readout.account_currency = Some("CAD".into());
            }
            let edits = props.edited_drawings(DrawingId(0), &chart).unwrap();
            for (id, drawing) in edits.iter().enumerate() {
                let expected = if currency_changed {
                    ("CAD", 0.9)
                } else if id == 1 {
                    ("GBP", 0.6)
                } else {
                    ("EUR", 0.9)
                };
                assert_eq!(
                    drawing.position_readout.account_currency.as_deref(),
                    Some(expected.0)
                );
                assert_eq!(drawing.position_readout.conversion_rate, Some(expected.1));
            }
        }
        // Anchoring one of two selected tables to the screen anchors both, each keeping its own
        // column widths.
        chart.drawings_mut().drawings = [vec![Some(90.0)], vec![None, Some(50.0)]]
            .into_iter()
            .enumerate()
            .map(|(id, column_widths)| {
                let mut table = Drawing::new(id + 1, DrawingToolType::Table);
                table.table = Some(Box::new(crate::drawings::TableConfig {
                    column_widths,
                    ..Default::default()
                }));
                table
            })
            .collect();
        chart.drawings_mut().select_drawings([1, 2]);
        let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        let settings = &mut props.tool_settings.as_mut().unwrap().settings;
        settings.table.as_mut().unwrap().screen_anchored = true;
        let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
        let tables: Vec<_> = edits.iter().map(|d| d.table.clone().unwrap()).collect();
        assert!(
            tables.iter().all(|table| table.screen_anchored),
            "{tables:?}"
        );
        assert_eq!(tables[0].column_widths, [Some(90.0)]);
        assert_eq!(tables[1].column_widths, [None, Some(50.0)]);
        // An image's opacity is independent of its frame and of the other selected pictures.
        for opacity in [None, Some(128)] {
            let mut chart = Chart::new(crate::model::BarData::new());
            for (id, size, opacity) in [(1, [240.0, 120.0], 255), (2, [120.0, 240.0], 64)] {
                let mut picture = Drawing::new(id, DrawingToolType::Image);
                picture.image = Some(crate::drawings::ImageConfig {
                    size: Some(size),
                    opacity,
                });
                chart.drawings_mut().drawings.push(picture);
            }
            chart.drawings_mut().select_drawings([1, 2]);
            let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
            // The picture was resized after the dialog opened.
            chart.drawings_mut().drawings[0]
                .image
                .as_mut()
                .unwrap()
                .size = Some([480.0, 240.0]);
            props.color = Color32::BLUE;
            if let Some(opacity) = opacity {
                props
                    .tool_settings
                    .as_mut()
                    .unwrap()
                    .settings
                    .image
                    .as_mut()
                    .unwrap()
                    .opacity = opacity;
            }
            let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
            for (drawing, size, original_opacity) in [
                (&edits[0], [480.0, 240.0], 255),
                (&edits[1], [120.0, 240.0], 64),
            ] {
                let config = drawing.image.unwrap();
                assert_eq!(config.size, Some(size));
                assert_eq!(config.opacity, opacity.unwrap_or(original_opacity));
            }
        }
        // Opened drawing, peer, dialog edit, and the peer's independently expected settings.
        use crate::drawings::{
            BarsPatternConfig, ChartPoint, ForecastStyle, GannConfig, GhostFeedConfig,
            PitchforkConfig, PitchforkLevel, RegressionTrendConfig, VwapConfig,
        };
        type ConfigCase = (
            &'static str,
            DrawingToolType,
            fn(&mut Drawing),
            fn(&mut Drawing),
            fn(&mut Drawing),
            fn(&mut Drawing),
        );
        let cases: &[ConfigCase] = &[
            (
                "bars pattern",
                DrawingToolType::BarsPattern,
                |d| {
                    d.bars_pattern = Some(BarsPatternConfig {
                        mirrored: true,
                        ..Default::default()
                    })
                },
                |d| d.bars_pattern = None,
                |d| d.bars_pattern.as_mut().unwrap().flipped = true,
                |d| {
                    d.bars_pattern = Some(BarsPatternConfig {
                        flipped: true,
                        ..Default::default()
                    })
                },
            ),
            (
                "pitchfork levels",
                DrawingToolType::Pitchfork,
                |d| d.pitchfork = Some(PitchforkConfig::default()),
                |d| {
                    let mut config = PitchforkConfig::default();
                    config.levels[3].color = [1, 2, 3, 255];
                    config.background_opacity = 10;
                    d.pitchfork = Some(config);
                },
                |d| d.pitchfork.as_mut().unwrap().levels[0].visible = false,
                |d| d.pitchfork.as_mut().unwrap().levels[0].visible = false,
            ),
            (
                "legacy pitchfork",
                DrawingToolType::Pitchfork,
                |d| d.pitchfork = Some(PitchforkConfig::default()),
                |d| {
                    d.pitchfork = None;
                    d.color = [0, 128, 0, 255];
                    d.stroke_width = 2.0;
                },
                |d| d.pitchfork.as_mut().unwrap().background_opacity = 75,
                |d| {
                    d.pitchfork = Some(PitchforkConfig {
                        levels: [0.0, 0.5, 1.0, 1.5]
                            .map(|value| PitchforkLevel {
                                value,
                                visible: true,
                                color: [0, 128, 0, 255],
                                line_style: LineStyle::Solid,
                                width: Some(if value == 1.0 {
                                    crate::styles::stroke::HAIRLINE
                                } else {
                                    2.0
                                }),
                            })
                            .to_vec(),
                        show_background: true,
                        background_opacity: 75,
                        extend_lines: true,
                    })
                },
            ),
            (
                "gann",
                DrawingToolType::GannSquare,
                |d| {
                    let mut config = GannConfig::for_tool(DrawingToolType::GannSquare);
                    config.show_fan = !config.show_fan;
                    d.gann = Some(config);
                },
                |d| d.gann = None,
                |d| {
                    let config = d.gann.as_mut().unwrap();
                    config.show_arcs = !config.show_arcs;
                },
                |d| {
                    let mut config = GannConfig::for_tool(DrawingToolType::GannSquare);
                    config.show_arcs = !config.show_arcs;
                    d.gann = Some(config);
                },
            ),
            (
                "regression trend",
                DrawingToolType::RegressionTrend,
                |d| d.regression_trend = RegressionTrendConfig::new(2.0),
                |d| d.regression_trend = RegressionTrendConfig::default().with_deviations(1.5, 2.5),
                |d| d.regression_trend.as_mut().unwrap().use_upper = false,
                |d| d.regression_trend.as_mut().unwrap().use_upper = false,
            ),
            (
                "custom settings",
                DrawingToolType::Polyline,
                |d| {
                    d.custom_tool = Some("host-path".into());
                    d.custom_settings = serde_json::json!({"a": 1, "b": 2});
                },
                |d| {
                    d.custom_tool = Some("host-path".into());
                    d.custom_settings = serde_json::json!({"a": 5, "b": 7});
                },
                |d| d.custom_settings["b"] = serde_json::json!(3),
                |d| d.custom_settings = serde_json::json!({"a": 5, "b": 3}),
            ),
            (
                "ghost feed",
                DrawingToolType::GhostFeed,
                |d| {
                    d.ghost_feed = Some(GhostFeedConfig {
                        average_range_ticks: 20.0,
                        ..Default::default()
                    })
                },
                |d| d.ghost_feed = None,
                |d| d.ghost_feed.as_mut().unwrap().noise_ticks = 0.0,
                |d| {
                    d.ghost_feed = Some(GhostFeedConfig {
                        average_range_ticks: 10.0,
                        noise_ticks: 0.0,
                        ..Default::default()
                    })
                },
            ),
            (
                "ghost feed body colour sharing a channel with the peer's",
                DrawingToolType::GhostFeed,
                |_| {},
                |d| d.ghost_feed = None,
                |d| d.ghost_feed.as_mut().unwrap().body_up = [38, 229, 154, 255],
                |d| {
                    d.ghost_feed = Some(GhostFeedConfig {
                        body_up: [38, 229, 154, 255],
                        ..Default::default()
                    })
                },
            ),
            (
                "volume profile",
                DrawingToolType::FixedRangeVolumeProfile,
                |d| d.volume_profile = crate::data::VolumeProfileConfig::new(24, 0.7),
                |d| d.volume_profile = crate::data::VolumeProfileConfig::new(48, 0.8),
                |d| d.volume_profile.as_mut().unwrap().extend_right = true,
                |d| d.volume_profile.as_mut().unwrap().extend_right = true,
            ),
            (
                "volume profile ticks per row",
                DrawingToolType::FixedRangeVolumeProfile,
                |d| d.volume_profile = crate::data::VolumeProfileConfig::new(24, 0.7),
                |d| {
                    d.volume_profile = crate::data::VolumeProfileConfig::new(48, 0.8)
                        .and_then(|c| c.with_row_size(Some(2.0)))
                },
                |d| {
                    let config = d.volume_profile.as_mut().unwrap();
                    *config = config.with_row_ticks(Some(4)).unwrap();
                },
                |d| {
                    let config = d.volume_profile.as_mut().unwrap();
                    *config = config.with_row_ticks(Some(4)).unwrap();
                },
            ),
            (
                "vwap band line",
                DrawingToolType::AnchoredVWAP,
                |d| d.vwap = Some(VwapConfig::default()),
                |d| {
                    let mut config = VwapConfig::default();
                    config.bands[0].lower.color = [1, 2, 3, 255];
                    config.track_price = true;
                    d.vwap = Some(config);
                },
                |d| d.vwap.as_mut().unwrap().bands[0].upper.visible = false,
                |d| d.vwap.as_mut().unwrap().bands[0].upper.visible = false,
            ),
            (
                // A forecast saved before its card colours keeps its fill as every background.
                "forecast saved before its colours",
                DrawingToolType::Forecast,
                |_| {},
                |d| {
                    d.forecast = None;
                    d.fill_color = Some([1, 2, 3, 200]);
                },
                |d| d.forecast.as_mut().unwrap().start_text = Some([9, 8, 7, 255]),
                |d| {
                    let fill = Some([1, 2, 3, 200]);
                    d.forecast = Some(ForecastStyle {
                        start_background: fill,
                        start_text: Some([9, 8, 7, 255]),
                        start_border: None,
                        target_background: fill,
                        target_text: None,
                        target_border: None,
                        success_background: fill,
                        success_text: None,
                        failure_background: fill,
                        failure_text: None,
                        transparency: 0,
                    });
                },
            ),
            (
                "edited level of a list of a different length",
                DrawingToolType::Pitchfork,
                |d| d.pitchfork = Some(PitchforkConfig::default()),
                |d| {
                    let mut config = PitchforkConfig::default();
                    config.levels.truncate(4);
                    config.background_opacity = 10;
                    d.pitchfork = Some(config);
                },
                |d| d.pitchfork.as_mut().unwrap().levels[0].visible = false,
                |d| d.pitchfork.as_mut().unwrap().levels[0].visible = false,
            ),
        ];
        for &(case, tool, opened, peer, edit, change) in cases {
            let mut drawings = [Drawing::new(1, tool), Drawing::new(2, tool)];
            for drawing in &mut drawings {
                drawing.chart_points = [(0.0, 10.0), (2.0, 11.0), (5.0, 10.0)]
                    .map(|(bar, price)| ChartPoint::new(bar, price))
                    .to_vec();
            }
            opened(&mut drawings[0]);
            peer(&mut drawings[1]);
            let mut chart = Chart::new(crate::model::BarData::new());
            chart.drawings_mut().drawings = drawings.to_vec();
            chart.drawings_mut().select_drawings([1, 2]);
            let mut props = DrawingProps::from_drawing(&drawings[0], &chart);
            edit(&mut props.tool_settings.as_mut().unwrap().settings);
            let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
            edit(&mut drawings[0]);
            change(&mut drawings[1]);
            assert_eq!(format!("{edits:?}"), format!("{drawings:?}"), "{case}");
        }
        // Properties assembled without tool settings hold the colour premultiplied, which reads
        // 76 back as 75 at alpha 51: an unedited colour leaves every hue as saved.
        let mut highlighter = Drawing::new(1, DrawingToolType::Highlighter);
        highlighter.color = [76, 175, 80, 51];
        let drawings = vec![highlighter, Drawing::new(2, DrawingToolType::TrendLine)];
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = drawings.clone();
        chart.drawings_mut().select_drawings([1, 2]);
        let props = DrawingProps {
            tool_settings: None,
            font_size: 20.0,
            ..DrawingProps::from_drawing(&drawings[0], &chart)
        };
        let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
        assert_eq!(
            edits.iter().map(|d| d.color).collect::<Vec<_>>(),
            drawings.iter().map(|d| d.color).collect::<Vec<_>>()
        );
        // The editor may still hold the hue of an earlier dialog, here [240, 55, 70], which the
        // default highlighter read back at 20% and paints the same at alpha 52: its opacity
        // dragged to 153 is still an opacity edit.
        chart.drawings_mut().drawings[0].color = [242, 54, 69, 52];
        let props = DrawingProps {
            tool_settings: None,
            color: Color32::from_rgba_unmultiplied(240, 55, 70, 153),
            ..DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart)
        };
        let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
        assert_eq!(
            edits.iter().map(|d| d.color).collect::<Vec<_>>(),
            [[242, 54, 69, 153], [41, 98, 255, 153]]
        );
    }

    /// Two selected trend lines: the first shows its price change, the second its inclination on
    /// the left with its text below the line. Ticking "Elapsed time" in the dialog of the first, or
    /// changing the first's text alignment, edits only that value on each line.
    #[test]
    fn selection_readout_edit_keeps_the_other_lines_readouts() {
        use crate::drawings::{
            LineMeasurementPosition as P, LineMeasurements, LineTextHorizontalAlign as H,
            LineTextVerticalAlign as V,
        };
        let mut first = Drawing::new(1, DrawingToolType::TrendLine);
        first.line_measurements.price_change = true;
        let mut second = Drawing::new(2, DrawingToolType::TrendLine);
        second.line_measurements = LineMeasurements {
            inclination: true,
            position: P::Left,
            text_vertical_align: V::Top,
            ..Default::default()
        };
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = vec![first.clone(), second.clone()];
        chart.drawings_mut().select_drawings([1, 2]);
        let opened = DrawingProps::from_drawing(&first, &chart);
        let mut got = Vec::new();
        let changes: [fn(&mut LineMeasurements); 13] = [
            |m| m.price_change = false,
            |m| m.percent_change = true,
            |m| m.pip_change = true,
            |m| m.bar_count = true,
            |m| m.elapsed_time = true,
            |m| m.length = true,
            |m| m.inclination = true,
            |m| m.persistent = true,
            |m| m.midpoint = true,
            |m| m.price_labels = true,
            |m| m.position = P::Center,
            |m| m.text_horizontal_align = H::Right,
            |m| m.text_vertical_align = V::Middle,
        ];
        for edit in changes {
            let mut props = opened.clone();
            edit(
                &mut props
                    .tool_settings
                    .as_mut()
                    .unwrap()
                    .settings
                    .line_measurements,
            );
            let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
            got.push(
                edits
                    .iter()
                    .map(|d| d.line_measurements)
                    .collect::<Vec<_>>(),
            );
        }
        let want: Vec<_> = changes
            .into_iter()
            .map(|edit| {
                [first.line_measurements, second.line_measurements]
                    .into_iter()
                    .map(|mut settings| {
                        edit(&mut settings);
                        settings
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(got, want);
    }

    /// Selection fill edits skip tools without a fill, while text backgrounds still reach
    /// other text boxes even when their Style tab has no shape fill.
    #[test]
    fn selection_fill_skips_drawings_without_a_fill() {
        for tool in [
            DrawingToolType::Rect,
            DrawingToolType::TextLabel,
            DrawingToolType::AnchoredText,
            DrawingToolType::AnchoredNote,
        ] {
            for (stored, enabled) in [
                (None, true),
                (Some([10, 20, 30, 40]), true),
                (Some([10, 20, 30, 40]), false),
            ] {
                let mut target = Drawing::new(1, tool);
                target.background = true;
                target.fill_color = (!enabled).then_some([0, 0, 255, 51]);
                let mut highlighter = Drawing::new(2, DrawingToolType::Highlighter);
                highlighter.fill_color = stored;
                let mut peer = Drawing::new(3, tool);
                peer.background = true;
                peer.fill_color = Some([0, 255, 0, 51]);
                let mut chart = Chart::new(crate::model::BarData::new());
                chart.drawings_mut().drawings = vec![target, highlighter, peer];
                chart.drawings_mut().select_drawings([1, 2, 3]);
                let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
                props.fill_enabled = enabled;
                props.fill_color = Some(Color32::from_rgba_unmultiplied(255, 0, 0, 51));
                let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
                let fill = enabled.then_some([255, 0, 0, 51]);
                assert_eq!(edits[0].fill_color, fill);
                assert_eq!(
                    edits[1].fill_color, stored,
                    "tools without a fill keep their stored value on enable, colour and disable edits"
                );
                assert_eq!(
                    edits[2].fill_color, fill,
                    "{tool:?} keeps selection background edits"
                );
            }
        }
    }
}
