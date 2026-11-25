//! Colours and drawings in scripts: the colour type and how a colour is read, the bodies of the
//! colour and drawing helpers, and the drawing calls of a script, read when it compiles, so the
//! study states before it runs what it may draw.

use super::engine::{number, refused, span};
use super::syntax::{self, Token, pieces};
use crate::ScriptError;
use crate::runtime::program::{DrawingCalls, PlotDecl, Span};
use crate::runtime::{self, bindings, drawings::Drawn};
use ::rhai::{AST, Dynamic, EvalAltResult, ImmutableString, NativeCallContext};
use ecolor::Color32;
use egui_charts::studies::{
    CrossDirection, IndicatorMarker, LabelPlacement, StudyLabel, StudySegment, StudyZone,
};

type Result<T> = std::result::Result<T, Box<EvalAltResult>>;

/// Most bars a drawing may lie after the bar calculated, as far as an offset moves a plot.
const MAX_AHEAD: i64 = 10_000;

/// Where a label's text sits, as a script names it.
const PLACEMENTS: [(&str, LabelPlacement); 4] = [
    ("above", LabelPlacement::Above),
    ("below", LabelPlacement::Below),
    ("above_left", LabelPlacement::AboveLeft),
    ("above_right", LabelPlacement::AboveRight),
];

/// Where a marker points, as a script names it.
const DIRECTIONS: [(&str, CrossDirection); 2] =
    [("up", CrossDirection::Up), ("down", CrossDirection::Down)];

/// A colour as a script holds it: `rgb(...)`, `rgba(...)` or a `color` input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColorValue(pub Color32);

impl ColorValue {
    /// The colour as text: `#RRGGBBAA` of its unmultiplied bytes.
    pub fn written(self) -> String {
        let [r, g, b, a] = self.0.to_srgba_unmultiplied();
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// The colour `value` stands for: a colour, a string `"#RRGGBB"` or `"#RRGGBBAA"`, or `na` for
/// none. `Err` names what it got instead: the string, quoted, else its type.
pub(crate) fn color_of(value: &Dynamic) -> std::result::Result<Option<Color32>, String> {
    if let Some(color) = value.read_lock::<ColorValue>() {
        return Ok(Some(color.0));
    }
    if let Some(text) = value.read_lock::<ImmutableString>() {
        return syntax::hex(&text)
            .map(Some)
            .ok_or_else(|| format!("{:?}", text.as_str()));
    }
    match value.as_float() {
        Ok(value) if value.is_nan() => Ok(None),
        _ => Err(syntax::type_name(value).to_owned()),
    }
}

/// The colour helper `name` gives for red, green and blue from 0 to 255 and an opacity from 0 to
/// 1, each held to its range and rounded to a byte, or `na` when one is missing.
pub(crate) fn rgba(
    context: &NativeCallContext,
    name: &str,
    rgb: [&Dynamic; 3],
    alpha: Option<&Dynamic>,
) -> Result<Dynamic> {
    let [r, g, b] = rgb.map(|value| number(context, name, value));
    let (r, g, b) = (r?, g?, b?);
    let alpha = alpha.map_or(Ok(1.0), |alpha| number(context, name, alpha))?;
    if [r, g, b, alpha].iter().any(|value| value.is_nan()) {
        return Ok(Dynamic::from_float(f64::NAN));
    }
    let byte = |value: f64| value.clamp(0.0, 255.0).round() as u8;
    let color = Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(alpha * 255.0));
    Ok(Dynamic::from(ColorValue(color)))
}

/// `label(plot, bar, price, text)`, with a placement in the longer form.
pub(crate) fn label(
    context: &NativeCallContext,
    [plot, bar, price, text]: [&Dynamic; 4],
    placement: Option<&Dynamic>,
) -> Result<()> {
    declared(context, "label", |calls| calls.labels)?;
    let plot = titled(context, "label", plot)?;
    let bar = self::bar(context, "label bar", bar)?;
    let price = number(context, "label price", price)?;
    let Some(text) = text
        .read_lock::<ImmutableString>()
        .map(|text| text.to_string())
    else {
        let got = syntax::type_name(text);
        return Err(refused(
            context,
            format!("label text is a string; got {got}"),
        ));
    };
    let placement = match placement {
        Some(placement) => word(context, "label placement", placement, &PLACEMENTS)?,
        None => LabelPlacement::Above,
    };
    let drawn = bar.filter(|_| price.is_finite()).map(|bar| {
        Drawn::Label(StudyLabel {
            bar,
            price,
            text,
            placement,
            plot,
        })
    });
    draw(context, drawn)
}

/// `segment(plot, bar1, price1, bar2, price2)`.
pub(crate) fn segment(
    context: &NativeCallContext,
    [plot, bar1, price1, bar2, price2]: [&Dynamic; 5],
) -> Result<()> {
    declared(context, "segment", |calls| calls.segments)?;
    let plot = titled(context, "segment", plot)?;
    let start = bar(context, "segment bar1", bar1)?;
    let start_price = number(context, "segment price1", price1)?;
    let end = bar(context, "segment bar2", bar2)?;
    let end_price = number(context, "segment price2", price2)?;
    let drawn = (start.zip(end))
        .filter(|_| start_price.is_finite() && end_price.is_finite())
        .map(|(start_bar, end_bar)| {
            Drawn::Segment(StudySegment {
                start_bar,
                start_price,
                end_bar,
                end_price,
                plot,
            })
        });
    draw(context, drawn)
}

/// `zone(plot, bar1, bar2, top, bottom)`, its bars and prices in either order.
pub(crate) fn zone(
    context: &NativeCallContext,
    [plot, bar1, bar2, top, bottom]: [&Dynamic; 5],
) -> Result<()> {
    declared(context, "zone", |calls| calls.zones)?;
    let plot = titled(context, "zone", plot)?;
    let first = bar(context, "zone bar1", bar1)?;
    let second = bar(context, "zone bar2", bar2)?;
    let top = number(context, "zone top", top)?;
    let bottom = number(context, "zone bottom", bottom)?;
    let drawn = (first.zip(second))
        .filter(|_| top.is_finite() && bottom.is_finite())
        .map(|(first, second)| {
            Drawn::Zone(StudyZone {
                bars: first.min(second)..first.max(second) + 1,
                top: top.max(bottom),
                bottom: top.min(bottom),
                plot,
            })
        });
    draw(context, drawn)
}

/// `marker(bar, price, direction)`.
pub(crate) fn marker(
    context: &NativeCallContext,
    [bar, price, direction]: [&Dynamic; 3],
) -> Result<()> {
    declared(context, "marker", |calls| calls.markers)?;
    let bar = self::bar(context, "marker bar", bar)?;
    let value = number(context, "marker price", price)?;
    let direction = word(context, "marker direction", direction, &DIRECTIONS)?;
    let drawn = bar.filter(|_| value.is_finite()).map(|bar| {
        Drawn::Marker(IndicatorMarker {
            bar,
            value,
            direction,
        })
    });
    draw(context, drawn)
}

/// An error unless the script calls drawing helper `name` as [`called`] reads it, so that what the
/// study says it draws holds.
fn declared(
    context: &NativeCallContext,
    name: &str,
    kind: fn(&DrawingCalls) -> bool,
) -> Result<()> {
    if runtime::with(|run| kind(&run.spec.calls)).unwrap_or(false) {
        return Ok(());
    }
    Err(refused(
        context,
        format!(
            "{name} draws only when called as {name}(...) in a script that defines no function \
             {name}"
        ),
    ))
}

/// The plot titled `title`, whose style a drawing of helper `name` takes.
fn titled(context: &NativeCallContext, name: &str, title: &Dynamic) -> Result<usize> {
    let Some(title) = title.read_lock::<ImmutableString>() else {
        let got = syntax::type_name(title);
        return Err(refused(
            context,
            format!("{name} takes a plot title; got {got}"),
        ));
    };
    let plot = runtime::with(|run| {
        (run.spec.plots.iter()).position(|plot| plot.title.as_str() == title.as_str())
    });
    plot.flatten()
        .ok_or_else(|| refused(context, format!("no plot {:?} is declared", title.as_str())))
}

/// The bar parameter `name` gives, `None` for `na`: a whole number from 0 to [`MAX_AHEAD`] bars
/// after the bar calculated.
fn bar(context: &NativeCallContext, name: &str, value: &Dynamic) -> Result<Option<usize>> {
    let value = number(context, name, value)?;
    if value.is_nan() {
        return Ok(None);
    }
    let Some(bar) = bindings::whole(value) else {
        return Err(refused(
            context,
            format!("{name} {value} is not a whole number"),
        ));
    };
    let calculated = runtime::with(|run| run.bar as i64).unwrap_or(0);
    if bar < 0 {
        Err(refused(
            context,
            format!("{name} {bar} is before the first bar"),
        ))
    } else if bar > calculated + MAX_AHEAD {
        Err(refused(
            context,
            format!("{name} {bar} is more than {MAX_AHEAD} bars after the bar calculated"),
        ))
    } else {
        Ok(Some(bar as usize))
    }
}

/// The meaning of the word parameter `name` gives, one of `words`.
fn word<T: Copy>(
    context: &NativeCallContext,
    name: &str,
    value: &Dynamic,
    words: &[(&str, T)],
) -> Result<T> {
    let quoted: Vec<_> = words.iter().map(|(word, _)| format!("{word:?}")).collect();
    let (last, first) = quoted.split_last().expect("a parameter has words");
    let words_are = format!("{} or {last}", first.join(", "));
    let Some(text) = value.read_lock::<ImmutableString>() else {
        let got = syntax::type_name(value);
        return Err(refused(
            context,
            format!("{name} is {words_are}; got {got}"),
        ));
    };
    (words.iter())
        .find(|(word, _)| *word == text.as_str())
        .map(|(_, meaning)| *meaning)
        .ok_or_else(|| {
            refused(
                context,
                format!("{name} {:?} is {words_are}", text.as_str()),
            )
        })
}

/// Keep `drawn` for the bar being calculated, unless it is `None` or a strategy decides, and count
/// the call on its line. A drawing that takes the study past the memory limit stops the run.
fn draw(context: &NativeCallContext, drawn: Option<Drawn>) -> Result<()> {
    let span = span(context.call_position());
    let stopped = runtime::with(|run| {
        run.mark(span);
        match drawn {
            Some(drawn) if run.decision.is_none() => run.draw(drawn),
            _ => false,
        }
    });
    if stopped == Some(true) {
        return Err(refused(context, String::new()));
    }
    Ok(())
}

/// The drawing helpers `source`, which compiled into `ast`, calls as functions, or the first such
/// call that names no plot of `plots` by its title written in the call: a word followed by `(`,
/// not after a single `.`, for a name the script defines no function of.
pub(crate) fn called(
    source: &str,
    ast: &AST,
    plots: &[PlotDecl],
) -> std::result::Result<DrawingCalls, ScriptError> {
    let mut calls = DrawingCalls::default();
    let tokens: Vec<_> = pieces(source)
        .into_iter()
        .filter_map(|(token, span, range)| Some((token?, span, range)))
        .collect();
    for (at, (token, ..)) in tokens.iter().enumerate() {
        let symbol = |at: usize, c: char| tokens.get(at).is_some_and(|t| t.0 == Token::Symbol(c));
        // A range `a..label(...)` ends in a `.` too.
        let method = at > 1 && symbol(at - 1, '.') && !symbol(at - 2, '.');
        let Token::Word(name) = *token else {
            continue;
        };
        let (kind, titled) = match name {
            "label" => (&mut calls.labels, true),
            "segment" => (&mut calls.segments, true),
            "zone" => (&mut calls.zones, true),
            "marker" => (&mut calls.markers, false),
            _ => continue,
        };
        if !symbol(at + 1, '(') || method || ast.iter_functions().any(|f| f.name == name) {
            continue;
        }
        *kind = true;
        if !titled {
            continue;
        }
        match (tokens.get(at + 2), tokens.get(at + 3)) {
            // Rhai unescapes a string only after a `\`, so the text written is the title passed.
            (Some((Token::Text(title), span, range)), Some((Token::Symbol(','), ..)))
                if source[range.clone()].starts_with('"') && !title.contains('\\') =>
            {
                if !plots.iter().any(|plot| plot.title == *title) {
                    let message = format!("no plot {title:?} is declared");
                    return Err(ScriptError::at(message, *span, None));
                }
            }
            // A call without the arguments the helper takes is refused when it runs.
            (Some((Token::Symbol(')'), ..)), _)
            | (Some((Token::Text(_), ..)), Some((Token::Symbol(')'), ..))) => {}
            (title, _) => {
                let span = title.map_or(Span::default(), |title| title.1);
                let message =
                    format!("{name} names its plot by a title in double quotes, without escapes");
                return Err(ScriptError::at(message, span, None));
            }
        }
    }
    Ok(calls)
}
