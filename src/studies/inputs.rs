//! Typed input schemas for built-in and host studies.

use crate::model::{PriceSource, Timeframe};
use egui::Color32;

/// A price series or an output of another study on the chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StudySource {
    /// A field or average of fields of the input bars.
    Price(PriceSource),
    /// A stable chart study id and its zero-based output column, without plot displacement.
    Study {
        /// Source series identifier assigned by the chart.
        id: usize,
        /// Zero-based plot index within the source study.
        plot: usize,
    },
}

/// A study input's current value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum InputValue {
    /// A bar count or nonnegative integer parameter.
    Length(usize),
    /// A signed integer parameter.
    Integer(i32),
    /// A finite numeric parameter.
    Number(f64),
    /// Index in the input's choices.
    Choice(usize),
    /// An enabled or disabled input.
    Toggle(bool),
    /// A price field or another study's output.
    Source(StudySource),
    /// An RGBA colour.
    Color(Color32),
    /// A calculation interval; `None` follows the chart.
    Timeframe(Option<Timeframe>),
    /// A host-resolved symbol; an empty string follows the chart.
    Symbol(String),
}

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
    /// A price field or another study's output. The chart checks dependency availability.
    Source,
    /// An RGBA colour.
    Color,
    /// A calculation interval, or the chart interval.
    Timeframe,
    /// A host-resolved symbol, or the chart symbol.
    Symbol,
}

/// An editable input, with a stable study-local id and its current value.
#[derive(Clone, Debug)]
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
        }
    }

    /// Set the declared default independently of the current value.
    pub fn with_default(mut self, value: InputValue) -> Self {
        self.default = value;
        self
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
            | (InputKind::Timeframe, InputValue::Timeframe(_)) => true,
            (InputKind::Symbol, InputValue::Symbol(value)) => {
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
            crate::studies::InputKind::Choice(choices.iter().map(|v| format!("{v:?}")).collect()),
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
}

#[cfg(feature = "studies")]
macro_rules! study_inputs {
    ($this:ident; $($field:ident $([$index:literal])? : $kind:ident($($args:expr),*)),+ $(,)? ; $after:block) => {
        fn inputs(&$this) -> Vec<crate::studies::StudyInput> {
            let defaults = Self::default();
            vec![$(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), $this.$field $([$index])?, $kind($($args),*))
                .with_default(crate::studies::inputs::input_spec!(stringify!($field $([$index])?), defaults.$field $([$index])?, $kind($($args),*)).value)),+]
        }

        fn set_input(&mut $this, id: &str, value: crate::studies::InputValue) -> Result<(), &'static str> {
            let schema = $this.inputs().into_iter().find(|input| input.id == id).ok_or("unknown study input")?;
            if !schema.accepts(&value) { return Err("study input is outside its accepted values"); }
            match id {
                $(stringify!($field $([$index])?) => { $this.$field $([$index])? = crate::studies::inputs::input_assign!(value, $kind($($args),*)); }),+
                _ => return Err("unknown study input"),
            }
            $after
            $this.calculate(&[]);
            Ok(())
        }
    };
}

#[cfg(feature = "studies")]
pub(crate) use {input_assign, input_spec, study_inputs};
