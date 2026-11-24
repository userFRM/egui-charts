//! The studies a script calls. Each is computed natively over every bar of the run, once per
//! call and set of inputs, and read as a series.

use super::limits::Stop;
use super::program::{Prototype, ScriptSpec, Span};
use super::series::{Columns, SeriesRef};
use super::{Run, data};
use egui_charts::studies::{
    ConfiguredStudy, Indicator, IndicatorFactory, InputKind, InputValue, StudySource,
};
use std::sync::{Arc, Mutex};

/// Most studies a run computes.
pub(crate) const MAX_STUDIES: usize = 64;

/// A value a script gives an input of a study.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Arg {
    Int(i64),
    Number(f64),
    Bool(bool),
    Text(String),
    Series(SeriesRef),
    /// A value the input cannot take, by its type.
    Other(&'static str),
}

impl Arg {
    /// Whether `other` is the same value, numbers bit for bit, so that a missing one is the same
    /// on every bar.
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            _ => self == other,
        }
    }

    /// What an error says a script gave.
    fn got(&self) -> String {
        match self {
            Self::Int(value) => format!("; got {value}"),
            Self::Number(value) if value.is_nan() => "; got na".into(),
            Self::Number(value) => format!("; got {value}"),
            Self::Bool(value) => format!("; got {value}"),
            Self::Text(text) => format!("; got {text:?}"),
            Self::Other(kind) => format!("; got {kind}"),
            Self::Series(_) => String::new(),
        }
    }
}

/// Why a study call failed, and the place of the value at fault among those it gives, if one is.
pub(crate) type Refusal = (Option<usize>, String);

/// The study registered as `name` with every input at the default it declares, whatever defaults
/// the host set, and with the inputs the settings dialog shows, or `None` when there is none.
pub(crate) fn prototype(factory: &IndicatorFactory, name: &str) -> Option<Prototype> {
    let mut study = factory.create(name)?;
    // The inputs report the host's defaults as their own until these are cleared.
    study.set_input_defaults(&[]);
    // Twice, since one input may decide what another accepts.
    for _ in 0..2 {
        for input in study.inputs() {
            let _ = study.set_input(&input.id, input.default);
        }
    }
    if !study.has_settings() {
        study = Box::new(ConfiguredStudy::new(study));
    }
    // A study that calculates on its own says so once it has calculated.
    study.calculate(&[]);
    Some(Prototype(Arc::new(Mutex::new(study))))
}

/// The line of `names` that `title` names: the line shown as `title`, or the only line shown so
/// without its last parenthesised group. Where several lines share what is left, also the line at
/// the place of `title` among `defaults`, the names under the study's default inputs.
pub(crate) fn line(names: &[String], defaults: &[String], title: &str) -> Option<usize> {
    let shown = names.iter().position(|name| name == title);
    shown.or_else(|| only(names, title)).or_else(|| {
        let at = defaults.iter().position(|name| name == title)?;
        let name = names.get(at)?;
        only(names, short(name)).is_none().then_some(at)
    })
}

/// What refuses `title` as a line of study `study`, with the titles a script may write.
pub(crate) fn no_line(study: &str, names: &[String], title: &str) -> String {
    let titles: Vec<_> = titles(names)
        .iter()
        .map(|title| format!("{title:?}"))
        .collect();
    format!(
        "{study} has no output {title:?} with these inputs; it has {}",
        titles.join(", ")
    )
}

/// The title a script writes for each line of `names`: the name without its last parenthesised
/// group where no other line shares what is left, else the name as shown.
pub(crate) fn titles(names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| match only(names, short(name)) {
            Some(_) => short(name).to_owned(),
            None => name.clone(),
        })
        .collect()
}

/// Why a script cannot call study `name`, checked on `study` at its defaults when the script
/// compiles: its values would arrive later, or it has no line to read.
pub(crate) fn unreadable(name: &str, study: &dyn Indicator) -> Option<String> {
    if study.calculation().is_some() {
        Some(format!(
            "study {name:?} calculates on its own, so a script cannot read it"
        ))
    } else if study.line_count() == 0 {
        Some(format!("study {name:?} has no line to read"))
    } else {
        None
    }
}

/// Why study `name`, with the inputs a call gives, cannot run on a script's bars, checked on the
/// first bar the call runs: it needs data that only the chart supplies.
pub(crate) fn unsupplied(name: &str, study: &dyn Indicator) -> Option<String> {
    if !study.data_requests().is_empty() {
        Some(format!("{name} needs histories a script cannot supply"))
    } else if !study.order_flow_inputs().is_empty() {
        Some(format!("{name} needs order flow a script cannot supply"))
    } else {
        None
    }
}

/// The only line of `names` shown as `title` without its last parenthesised group.
fn only(names: &[String], title: &str) -> Option<usize> {
    let mut lines = (0..names.len()).filter(|&at| short(&names[at]) == title);
    lines.next().filter(|_| lines.next().is_none())
}

/// `title` without its last parenthesised group, which shows inputs.
fn short(title: &str) -> &str {
    let group = title.strip_suffix(')').and_then(|title| title.rfind('('));
    group.map_or(title, |at| title[..at].trim_end())
}

/// `value` as an int when it is a whole number.
pub(crate) fn whole(value: f64) -> Option<i64> {
    (value.fract() == 0.0 && value.abs() < 9.0e15).then_some(value as i64)
}

/// The value `arg` gives an input of `kind`, or `None` for one of another type.
fn value(kind: &InputKind, arg: &Arg) -> Option<InputValue> {
    let int = match arg {
        Arg::Int(value) => Some(*value),
        Arg::Number(value) => whole(*value),
        _ => None,
    };
    Some(match (kind, arg) {
        (InputKind::Length(..), _) => InputValue::Length(int?.try_into().ok()?),
        (InputKind::Integer(..), _) => InputValue::Integer(int?.try_into().ok()?),
        (InputKind::Number(..), Arg::Int(value)) => InputValue::Number(*value as f64),
        (InputKind::Number(..), Arg::Number(value)) => InputValue::Number(*value),
        (InputKind::Toggle, Arg::Bool(value)) => InputValue::Toggle(*value),
        (InputKind::Choice(labels), Arg::Text(text)) => {
            InputValue::Choice(labels.iter().position(|label| label == text)?)
        }
        (InputKind::Text, Arg::Text(text)) => InputValue::Text(text.clone()),
        (InputKind::Symbol, Arg::Text(text)) => InputValue::Symbol(text.clone()),
        (InputKind::Timeframe, Arg::Text(text)) => InputValue::Timeframe(data::interval(text)?),
        (InputKind::Source, Arg::Series(SeriesRef::Price(price))) => {
            InputValue::Source(StudySource::Price(*price))
        }
        _ => return None,
    })
}

/// What an input of `kind` takes, for an error; `columns` when it also reads whole columns.
pub(crate) fn takes(kind: &InputKind, columns: bool) -> String {
    let range = |what: &str, min: f64, max: f64| {
        let from = (min > f64::MIN).then(|| format!(" from {min}"));
        let to = (max < f64::MAX).then(|| format!(" to {max}"));
        format!(
            "{what}{}{}",
            from.unwrap_or_default(),
            to.unwrap_or_default()
        )
    };
    match kind {
        InputKind::Length(min, usize::MAX) => range("an int", *min as f64, f64::MAX),
        InputKind::Length(min, max) => range("an int", *min as f64, *max as f64),
        InputKind::Integer(min, max) => range("an int", (*min).into(), (*max).into()),
        InputKind::Number(min, max) => range("a number", *min, *max),
        InputKind::Toggle => "true or false".into(),
        InputKind::Choice(labels) => {
            let labels: Vec<_> = labels.iter().map(|label| format!("{label:?}")).collect();
            format!("one of {}", labels.join(", "))
        }
        InputKind::Text | InputKind::Symbol => "one line of text without spaces around it".into(),
        InputKind::Timeframe => {
            "an interval such as \"5min\", \"1h\" or \"1D\", or \"\" for the chart's".into()
        }
        InputKind::Source if columns => "a price, the source input or a study call".into(),
        InputKind::Source => "a price".into(),
        _ => "no value from a script".into(),
    }
}

/// A study computed for one call and one set of inputs.
pub(crate) struct Instance {
    site: usize,
    args: Vec<Arg>,
    /// The column its source reads in place of a price.
    source: Option<SeriesRef>,
    study: Box<dyn Indicator>,
    /// The line the call reads, of how many the study computes.
    line: usize,
    lines: usize,
    /// Where the call is.
    pub span: Span,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("site", &self.site)
            .finish_non_exhaustive()
    }
}

impl Instance {
    /// Study call `site` of `spec` with the inputs `args`, made at `span` and not yet calculated.
    fn new(spec: &ScriptSpec, site: usize, args: Vec<Arg>, span: Span) -> Result<Self, Refusal> {
        let decl = &spec.studies[site];
        let name = &decl.name;
        let mut study = decl.prototype.lock().clone_box();
        let defaults = study.line_names();
        let mut source = None;
        for (place, (given, arg)) in decl.inputs.iter().zip(&args).enumerate() {
            let id = &given.id;
            let fault = |message: String| (Some(place), message);
            // The chart's own input follows a study's of the same id.
            let input = study.inputs().into_iter().rev().find(|i| i.id == *id);
            let input = input.ok_or_else(|| fault(format!("study {name:?} has no input {id}")))?;
            let columns = id == "source" && study.source().is_some();
            match arg {
                Arg::Series(series @ (SeriesRef::Fed | SeriesRef::Study(_))) if columns => {
                    source = Some(*series);
                    continue;
                }
                Arg::Series(SeriesRef::Declared(at)) if columns => {
                    return Err(fault(format!(
                        "{name} reads its source over the whole series, so it cannot read the \
                         series {}, which the script computes bar by bar; use a helper such as \
                         rma on it",
                        spec.series[*at].name
                    )));
                }
                _ => {}
            }
            let value = value(&input.kind, arg).filter(|value| input.accepts(value));
            let value = value.ok_or_else(|| {
                let takes = takes(&input.kind, columns);
                fault(format!("{name} input {id} takes {takes}{}", arg.got()))
            })?;
            study
                .set_input(id, value)
                .map_err(|error| fault(error.to_string()))?;
        }
        let refuse = |message: String| Err((None, message));
        if let Some(message) = unsupplied(name, &*study) {
            return refuse(message);
        }
        let names = study.line_names();
        let line = match &decl.output {
            None => 0,
            Some(title) => match line(&names, &defaults, title) {
                Some(line) => line,
                None => return refuse(no_line(name, &names, title)),
            },
        };
        Ok(Self {
            site,
            args,
            source,
            lines: study.line_count(),
            study,
            line,
            span,
        })
    }

    /// Bring the study up to date after the bars from `from` changed.
    fn update(&mut self, columns: &Columns, from: usize) {
        let bars = &columns.bars;
        let source = match self.source {
            Some(SeriesRef::Study(at)) => columns.studies.get(at),
            Some(_) => columns.fed.as_ref(),
            None => None,
        };
        match source {
            Some(source) => self.study.update_source(bars, from, source),
            None => self.study.update(bars, from),
        }
    }

    /// The bytes the study holds for `len` bars: its lines and the copy of the one read.
    pub fn bytes(&self, len: usize) -> usize {
        std::mem::size_of::<Self>() + (1 + self.lines) * len * 8
    }
}

impl Run {
    /// The line that study call `site`, at `span`, reads with the inputs `args`, computed over every
    /// bar of the run when the call first gives them, so that its values do not depend on the bar.
    pub fn study(&mut self, site: usize, args: Vec<Arg>, span: Span) -> Result<SeriesRef, Refusal> {
        let called = self.instances.iter().position(|instance| {
            instance.site == site && instance.args.iter().zip(&args).all(|(a, b)| a.same(b))
        });
        if let Some(at) = called {
            return Ok(SeriesRef::Study(at));
        }
        let at = self.instances.len();
        if at == MAX_STUDIES {
            return Err((
                None,
                format!(
                    "a script computes at most {MAX_STUDIES} studies, one per call and set of inputs"
                ),
            ));
        }
        // The calculation is not interrupted, so it is checked on either side.
        let stopped = (None, String::new());
        if self.check_time() {
            return Err(stopped);
        }
        let spec = self.spec.clone();
        self.instances.push(Instance::new(&spec, site, args, span)?);
        self.columns.studies.push(Vec::new());
        if self.bytes() > self.clock.limits.memory {
            self.stop = Some(Stop::Memory);
            return Err(stopped);
        }
        self.update_study(at, 0);
        if self.check_time() {
            return Err(stopped);
        }
        Ok(SeriesRef::Study(at))
    }

    /// Bring study `at` up to date after the bars from `from` changed, and its column, which holds
    /// the bars before them. Returns whether the study's values on those changed, which the script
    /// has read.
    pub fn update_study(&mut self, at: usize, from: usize) -> bool {
        let instance = &mut self.instances[at];
        instance.update(&self.columns, from);
        let len = self.columns.bars.len();
        let values = instance.study.values().line(instance.line);
        let values = &values[..values.len().min(len)];
        let column = &mut self.columns.studies[at];
        let changed = !unchanged(column, values);
        column.extend_from_slice(&values[column.len().min(values.len())..]);
        column.resize(len, f64::NAN);
        changed
    }
}

/// Whether `column`, the first values of a line before an update, are its first values after it,
/// `values`, where a missing value may be left out at the end.
fn unchanged(column: &[f64], values: &[f64]) -> bool {
    let (column, missing) = column.split_at(column.len().min(values.len()));
    // Without an exit inside a chunk the comparison runs on several values at once.
    let differ = |(a, b): (&[f64], &[f64])| {
        let pairs = a.iter().zip(b);
        pairs.fold(0, |bits, (a, b)| bits | (a.to_bits() ^ b.to_bits())) != 0
    };
    !column.chunks(1024).zip(values.chunks(1024)).any(differ)
        && missing.iter().all(|value| value.is_nan())
}
