//! Bounded Cartesian grids of typed strategy inputs.

use crate::studies::{InputKind, InputValue, StudyInput};

const MAX_TRIALS: usize = 10_000;

pub(super) struct Range {
    pub enabled: bool,
    pub input: StudyInput,
    pub from: InputValue,
    pub to: InputValue,
    pub step: InputValue,
}

impl Range {
    pub fn new(input: &StudyInput) -> Option<Self> {
        let step = match input.value {
            InputValue::Length(_) => InputValue::Length(1),
            InputValue::Integer(_) => InputValue::Integer(1),
            InputValue::Number(_) => InputValue::Number(1.0),
            _ => return None,
        };
        Some(Self {
            enabled: false,
            input: input.clone(),
            from: input.value.clone(),
            to: input.value.clone(),
            step,
        })
    }

    pub fn values(&self) -> Result<Vec<InputValue>, &'static str> {
        if !self.input.accepts(&self.from) || !self.input.accepts(&self.to) {
            return Err("Range endpoints must be within the input bounds");
        }
        let count = match (&self.from, &self.to, &self.step) {
            (InputValue::Length(a), InputValue::Length(b), InputValue::Length(step))
                if b >= a && *step > 0 =>
            {
                ((b - a) / step).checked_add(1)
            }
            (InputValue::Integer(a), InputValue::Integer(b), InputValue::Integer(step))
                if b >= a && *step > 0 =>
            {
                usize::try_from((i64::from(*b) - i64::from(*a)) / i64::from(*step) + 1).ok()
            }
            (InputValue::Number(a), InputValue::Number(b), InputValue::Number(step))
                if b >= a && step.is_finite() && *step > 0.0 =>
            {
                let span = b - a;
                let steps = if span.is_finite() {
                    span / step
                } else {
                    b / step - a / step
                };
                // Include a decimal endpoint such as 0.3 reached by three steps of 0.1.
                let tolerance = 8.0 * f64::EPSILON * steps.abs().max(1.0);
                let count = (steps + tolerance).floor() + 1.0;
                (count.is_finite() && count <= MAX_TRIALS as f64).then_some(count as usize)
            }
            _ => return Err("Use ascending ranges and a positive finite step"),
        }
        .filter(|count| *count > 0 && *count <= MAX_TRIALS)
        .ok_or("An optimisation is limited to 10,000 trials")?;
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            let value = match (&self.from, &self.to, &self.step) {
                (InputValue::Length(a), _, InputValue::Length(step)) => {
                    InputValue::Length(a + index * step)
                }
                (InputValue::Integer(a), _, InputValue::Integer(step)) => {
                    InputValue::Integer((i64::from(*a) + index as i64 * i64::from(*step)) as i32)
                }
                (InputValue::Number(a), InputValue::Number(b), InputValue::Number(step)) => {
                    let value = (index as f64).mul_add(*step, *a);
                    if index > 0
                        && value
                            <= match values.last() {
                                Some(InputValue::Number(v)) => *v,
                                _ => *a,
                            }
                    {
                        return Err("Step is too small to change this input");
                    }
                    InputValue::Number(value.min(*b))
                }
                _ => unreachable!("validated numeric range"),
            };
            if !self.input.accepts(&value) {
                return Err("Range contains a value outside the input bounds");
            }
            values.push(value);
        }
        Ok(values)
    }
}

type Axis = (String, Vec<InputValue>);

pub(super) fn grid(ranges: &[Range]) -> Result<(usize, Vec<Axis>), &'static str> {
    let mut count = 1usize;
    let mut axes = Vec::new();
    for range in ranges.iter().filter(|range| range.enabled) {
        let values = range.values()?;
        count = count
            .checked_mul(values.len())
            .filter(|count| *count <= MAX_TRIALS)
            .ok_or("An optimisation is limited to 10,000 trials")?;
        axes.push((range.input.id.clone(), values));
    }
    if axes.is_empty() {
        return Err("Select at least one numeric input to optimise");
    }
    Ok((count, axes))
}

pub(super) fn show(ui: &mut egui::Ui, ranges: &mut [Range]) -> bool {
    let mut changed = false;
    for range in ranges {
        ui.push_id(&range.input.id, |ui| {
            changed |= ui
                .checkbox(&mut range.enabled, &range.input.label)
                .changed();
            if range.enabled {
                crate::ui_kit::FormGrid::new("range").show(ui, |ui| {
                    for (label, value) in [
                        ("From", &mut range.from),
                        ("To", &mut range.to),
                        ("Step", &mut range.step),
                    ] {
                        let mut input = range.input.clone();
                        input.id = label.into();
                        input.label = label.into();
                        input.value = value.clone();
                        if label == "Step" {
                            input.kind = match input.kind {
                                InputKind::Length(..) => InputKind::Length(1, usize::MAX),
                                InputKind::Integer(..) => InputKind::Integer(1, i32::MAX),
                                _ => InputKind::Number(0.0f64.next_up(), f64::MAX),
                            };
                        }
                        changed |= super::inputs::show_input(ui, &mut input, &[]);
                        *value = input.value;
                    }
                });
            }
        });
    }
    changed
}
