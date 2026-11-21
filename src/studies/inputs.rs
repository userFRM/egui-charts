//! Typed input schemas for built-in and host studies.

pub use crate::model::{InputValue, StudySource};

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
            _ => false,
        }
    }
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
            vec![$(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), $this.$field $([$index])?, $kind($($args),*))
                .with_default(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), defaults.$field $([$index])?, $kind($($args),*)).value)
                $(.shown_when(stringify!($choice $([$choice_index])?), $shown))?),+]
        }

        fn set_input(&mut $this, id: &str, value: crate::studies::InputValue) -> Result<(), crate::studies::StudyError> {
            let schema = $this.inputs().into_iter().find(|input| input.id == id).ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
            if !schema.accepts(&value) { return Err(crate::studies::StudyError::invalid_input(id)); }
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
