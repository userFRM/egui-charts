//! Typed input schemas for built-in and host studies.

pub use crate::model::{InputValue, StudySource};
use chrono::Timelike;

/// The accepted values of a study input.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InputKind {
    /// Inclusive integer bounds.
    Length(usize, usize),
    /// Inclusive signed integer bounds.
    Integer(i32, i32),
    /// Inclusive finite numeric bounds.
    Number(f64, f64),
    /// Labels in selection order.
    Choice(Vec<String>),
    /// An enabled or disabled input.
    Toggle,
    /// A price field or another study's output. The chart supplies an output only to the input
    /// `source` of a study whose [`Indicator::source`](super::Indicator::source) names it, and
    /// checks dependency availability and cycles.
    Source,
    /// An RGBA colour.
    Color,
    /// A calculation interval, or the chart interval.
    Timeframe,
    /// An interval, or none for the one the study picks from the chart's, such as the interval
    /// of the lower-interval candles a volume study reads. Holds the interval it picks on the
    /// current chart, `None` where it reads none. Values are [`InputValue::Timeframe`]. A
    /// [`ConfiguredStudy`](super::ConfiguredStudy) never calculates a study with such an input on
    /// another interval than the chart's.
    AutoTimeframe(Option<crate::model::Timeframe>),
    /// A host-resolved symbol, or the chart symbol.
    Symbol,
    /// Text on one line.
    Text,
    /// A time of day in hours and minutes, edited as two-digit hour and minute fields.
    TimeOfDay,
    /// A time zone named as [`TimezoneId::named`](crate::model::TimezoneId::named) reads it,
    /// picked from the chart's time zones. The study resolves the name when it is set.
    TimeZone {
        /// Whether the empty name is offered, as Exchange time, for the instrument's zone.
        exchange: bool,
    },
}

/// An editable input, with a stable study-local id and its current value.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct StudyInput {
    /// Id passed to [`Indicator::set_input`](super::Indicator::set_input).
    pub id: String,
    /// User-facing field label.
    pub label: String,
    /// Current value.
    pub value: InputValue,
    /// Initial value, retained when the current value is edited.
    pub default: InputValue,
    /// Accepted values, used to build the editor.
    pub kind: InputKind,
    /// Edited in the Style tab; saved and applied like any other input.
    pub style: bool,
    /// The choice or toggle input this one applies with and its accepted values (0/1 for a
    /// toggle). An editor hides it under other values and keeps its value. `None` always shows it.
    pub shown_when: Option<(String, Vec<usize>)>,
}

impl StudyInput {
    /// Define an input for a host study.
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        value: InputValue,
        kind: InputKind,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            default: value.clone(),
            value,
            kind,
            style: false,
            shown_when: None,
        }
    }

    /// List this input in the Style tab instead of the Inputs tab.
    pub fn in_style_tab(mut self) -> Self {
        self.style = true;
        self
    }

    /// Set the declared default independently of the current value.
    pub fn with_default(mut self, value: InputValue) -> Self {
        self.default = value;
        self
    }

    /// Show this input while `id` holds one of `choices`, with toggles counted as 0 or 1.
    pub fn shown_when(mut self, id: impl Into<String>, choices: impl Into<Vec<usize>>) -> Self {
        self.shown_when = Some((id.into(), choices.into()));
        self
    }

    /// Whether an editor listing `inputs`, this one among them, shows this input.
    pub fn is_shown(&self, inputs: &[StudyInput]) -> bool {
        let mut input = self;
        // Follow dependent controls too; a cyclic host schema shows none of that cycle.
        for _ in 0..=inputs.len() {
            let Some((id, choices)) = &input.shown_when else {
                return true;
            };
            let Some(parent) = inputs.iter().find(|parent| parent.id == *id) else {
                return false;
            };
            let shown = match parent.value {
                InputValue::Choice(choice) => choices.contains(&choice),
                InputValue::Toggle(on) => choices.contains(&usize::from(on)),
                _ => false,
            };
            if !shown {
                return false;
            }
            input = parent;
        }
        false
    }

    /// Check a value against this input's schema. Setters must also validate values.
    pub fn accepts(&self, value: &InputValue) -> bool {
        match (&self.kind, value) {
            (InputKind::Length(min, max), InputValue::Length(v)) => (*min..=*max).contains(v),
            (InputKind::Integer(min, max), InputValue::Integer(v)) => (*min..=*max).contains(v),
            (InputKind::Number(min, max), InputValue::Number(v)) => {
                v.is_finite() && (*min..=*max).contains(v)
            }
            (InputKind::Choice(choices), InputValue::Choice(v)) => *v < choices.len(),
            (InputKind::Toggle, InputValue::Toggle(_))
            | (InputKind::Source, InputValue::Source(_))
            | (InputKind::Color, InputValue::Color(_))
            | (InputKind::Timeframe | InputKind::AutoTimeframe(_), InputValue::Timeframe(_)) => {
                true
            }
            (InputKind::Symbol, InputValue::Symbol(value))
            | (InputKind::Text, InputValue::Text(value)) => {
                value.trim() == value && !value.chars().any(char::is_control)
            }
            (InputKind::TimeOfDay, InputValue::TimeOfDay(time)) => {
                time.second() == 0 && time.nanosecond() == 0
            }
            (InputKind::TimeZone { exchange }, InputValue::TimeZone(name)) => {
                if name.is_empty() {
                    *exchange
                } else {
                    name.trim() == name && !name.chars().any(char::is_control)
                }
            }
            _ => false,
        }
    }
}

/// `value` in the form `kind` takes now: a minute count below 1440 as a time of day and text as
/// a time zone, as the built-in studies saved them before inputs had those kinds, and a whole
/// number as a decimal, as studies saved inputs that now take decimals. Any other value is
/// returned unchanged, for `accepts` to judge.
#[cfg(feature = "studies")]
pub(crate) fn upgrade_saved_value(kind: &InputKind, value: InputValue) -> InputValue {
    match (kind, value) {
        (InputKind::TimeOfDay, InputValue::Length(minutes)) if minutes < 1440 => {
            InputValue::TimeOfDay(clock(minutes))
        }
        (InputKind::TimeZone { .. }, InputValue::Text(name)) => InputValue::TimeZone(name),
        (InputKind::Number(..), InputValue::Length(count)) => InputValue::Number(count as f64),
        (_, value) => value,
    }
}

/// `input` with its range widened to the value it holds, so that a study restored with a value
/// saved before its range narrowed shows and applies that value until it is edited.
#[cfg(feature = "studies")]
pub(crate) fn holding(mut input: StudyInput) -> StudyInput {
    match (&mut input.kind, &input.value) {
        (InputKind::Length(min, max), InputValue::Length(value)) => {
            (*min, *max) = ((*min).min(*value), (*max).max(*value));
        }
        (InputKind::Number(min, max), InputValue::Number(value)) if value.is_finite() => {
            (*min, *max) = (min.min(*value), max.max(*value));
        }
        _ => {}
    }
    input
}

thread_local! {
    /// The saved values that built-in studies take outside their inputs' ranges while they are
    /// restored, from [`restoring`].
    static RESTORED: std::cell::RefCell<Vec<(String, InputValue)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Run `restore` with the built-in studies it sets up taking each of `saved` by its input id,
/// even outside the input's range.
pub(crate) fn restoring<R>(saved: Vec<(String, InputValue)>, restore: impl FnOnce() -> R) -> R {
    let outer = RESTORED.replace(saved);
    let result = restore();
    RESTORED.set(outer);
    result
}

/// Whether a study being restored takes `value` for input `id` as saved, from [`restoring`].
#[cfg(feature = "studies")]
pub(crate) fn restores(id: &str, value: &InputValue) -> bool {
    RESTORED.with_borrow(|saved| {
        saved
            .iter()
            .any(|(saved, held)| saved == id && held == value)
    })
}

/// The time `minutes` after midnight, for `minutes` below 1440.
#[cfg(feature = "studies")]
pub(crate) fn clock(minutes: usize) -> chrono::NaiveTime {
    chrono::NaiveTime::from_hms_opt((minutes / 60) as u32, (minutes % 60) as u32, 0)
        .expect("a minute of the day")
}

/// Minutes after midnight of `time`, without its seconds.
#[cfg(feature = "studies")]
pub(crate) fn minutes(time: chrono::NaiveTime) -> usize {
    (time.hour() * 60 + time.minute()) as usize
}

#[cfg(feature = "studies")]
macro_rules! input_spec {
    ($id:expr, $value:expr, integer($label:expr, $min:expr, $max:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Integer($value),
            crate::studies::InputKind::Integer($min, $max),
        )
    };
    ($id:expr, $value:expr, length($label:expr, $min:expr, $max:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Length($value),
            crate::studies::InputKind::Length($min, $max),
        )
    };
    ($id:expr, $value:expr, number($label:expr, $min:expr, $max:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Number($value as f64),
            crate::studies::InputKind::Number($min, $max),
        )
    };
    ($id:expr, $value:expr, choice($label:expr, $choices:expr)) => {{
        let choices = $choices;
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Choice(
                choices.iter().position(|v| *v == $value).unwrap_or(0),
            ),
            crate::studies::InputKind::Choice(
                choices.iter().map(|v| v.label().to_owned()).collect(),
            ),
        )
    }};
    ($id:expr, $value:expr, toggle($label:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Toggle($value),
            crate::studies::InputKind::Toggle,
        )
    };
    ($id:expr, $value:expr, timeframe($label:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Timeframe($value),
            crate::studies::InputKind::Timeframe,
        )
    };
    ($id:expr, $value:expr, text($label:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Text($value.clone()),
            crate::studies::InputKind::Text,
        )
    };
    ($id:expr, $value:expr, color($label:expr)) => {
        crate::studies::StudyInput::new(
            $id,
            $label,
            crate::studies::InputValue::Color($value),
            crate::studies::InputKind::Color,
        )
    };
}

#[cfg(feature = "studies")]
macro_rules! input_assign {
    ($value:expr, integer($($args:expr),*)) => {
        match $value {
            crate::studies::InputValue::Integer(v) => v,
            _ => unreachable!(),
        }
    };
    ($value:expr, length($($args:expr),*)) => {
        match $value {
            crate::studies::InputValue::Length(v) => v,
            _ => unreachable!(),
        }
    };
    ($value:expr, number($($args:expr),*)) => {
        match $value {
            crate::studies::InputValue::Number(v) => v as _,
            _ => unreachable!(),
        }
    };
    ($value:expr, choice($label:expr, $choices:expr)) => {
        match $value {
            crate::studies::InputValue::Choice(v) => $choices
                .into_iter()
                .nth(v)
                .expect("validated study input choice"),
            _ => unreachable!(),
        }
    };
    ($value:expr, toggle($label:expr)) => {
        match $value {
            crate::studies::InputValue::Toggle(v) => v,
            _ => unreachable!(),
        }
    };
    ($value:expr, timeframe($label:expr)) => {
        match $value {
            crate::studies::InputValue::Timeframe(v) => v,
            _ => unreachable!(),
        }
    };
    ($value:expr, text($label:expr)) => {
        match $value {
            crate::studies::InputValue::Text(v) => v,
            _ => unreachable!(),
        }
    };
    ($value:expr, color($label:expr)) => {
        match $value {
            crate::studies::InputValue::Color(v) => v,
            _ => unreachable!(),
        }
    };
}

#[cfg(feature = "studies")]
macro_rules! study_inputs {
    ($this:ident; $($field:ident $([$index:literal])? : $kind:ident($($args:expr),*) $(if $choice:ident $([$choice_index:literal])? in $shown:expr)?),+ $(,)? ; $($changed:ident $(, $previous:ident)? =>)? $after:block) => {
        fn inputs(&$this) -> Vec<crate::studies::StudyInput> {
            let defaults = Self::default();
            vec![$(crate::studies::inputs::holding(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), $this.$field $([$index])?, $kind($($args),*)))
                .with_default(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), defaults.$field $([$index])?, $kind($($args),*)).value)
                $(.shown_when(stringify!($choice $([$choice_index])?), $shown))?),+]
        }

        fn set_input(&mut $this, id: &str, value: crate::studies::InputValue) -> Result<(), crate::studies::StudyError> {
            let schema = $this.inputs().into_iter().find(|input| input.id == id).ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
            if !schema.accepts(&value) && !crate::studies::inputs::restores(id, &value) { return Err(crate::studies::StudyError::invalid_input(id)); }
            match id {
                $(stringify!($field $([$index])?) => { $this.$field $([$index])? = crate::studies::inputs::input_assign!(value, $kind($($args),*)); }),+
                _ => return Err(crate::studies::StudyError::UnknownInput(id.into())),
            }
            $(let $changed = id; $(let $previous = schema.value;)?)?
            $after
            $this.calculate(&[]);
            Ok(())
        }
    };
}

#[cfg(feature = "studies")]
pub(crate) use {input_assign, input_spec, study_inputs};

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    /// A time of day takes whole minutes and a zone a name on one line, empty only where the
    /// input offers Exchange time. Minute counts below 1440 and text, as the built-in studies saved
    /// them, convert to those kinds; `accepts` itself refuses them, so host studies only ever
    /// receive the kinds they declare.
    #[test]
    fn time_and_zone_inputs_accept_their_values() {
        use InputKind::{Length, TimeOfDay, TimeZone};
        let time = |h, m| InputValue::TimeOfDay(NaiveTime::from_hms_opt(h, m, 0).unwrap());
        let zone = |name: &str| InputValue::TimeZone(name.into());
        let text = |name: &str| InputValue::Text(name.into());
        for (kind, value, upgraded, accepted) in [
            (TimeOfDay, InputValue::Length(0), time(0, 0), true),
            (TimeOfDay, InputValue::Length(570), time(9, 30), true),
            (TimeOfDay, InputValue::Length(1439), time(23, 59), true),
            (
                TimeOfDay,
                InputValue::Length(1440),
                InputValue::Length(1440),
                false,
            ),
            (
                TimeOfDay,
                InputValue::TimeOfDay(NaiveTime::from_hms_opt(9, 30, 15).unwrap()),
                InputValue::TimeOfDay(NaiveTime::from_hms_opt(9, 30, 15).unwrap()),
                false,
            ),
            (
                TimeOfDay,
                InputValue::TimeOfDay(NaiveTime::from_hms_nano_opt(9, 30, 0, 1).unwrap()),
                InputValue::TimeOfDay(NaiveTime::from_hms_nano_opt(9, 30, 0, 1).unwrap()),
                false,
            ),
            (TimeOfDay, text("09:30"), text("09:30"), false),
            (TimeZone { exchange: false }, text("UTC"), zone("UTC"), true),
            (TimeZone { exchange: false }, zone(""), zone(""), false),
            (
                TimeZone { exchange: false },
                zone(" UTC"),
                zone(" UTC"),
                false,
            ),
            // A line break inside the name, which trimming leaves in place.
            (
                TimeZone { exchange: false },
                zone("Europe/\nLondon"),
                zone("Europe/\nLondon"),
                false,
            ),
            (TimeZone { exchange: true }, text(""), zone(""), true),
            (
                Length(0, 1439),
                InputValue::Length(570),
                InputValue::Length(570),
                true,
            ),
        ] {
            let input = StudyInput::new("x", "X", value.clone(), kind);
            if value != upgraded {
                assert!(!input.accepts(&value), "{input:?}");
            }
            assert_eq!(upgrade_saved_value(&input.kind, value), upgraded);
            assert_eq!(input.accepts(&upgraded), accepted, "{input:?}");
        }
    }
}
