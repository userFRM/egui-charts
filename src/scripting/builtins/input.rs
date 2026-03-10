/// Input built-in functions (input namespace)
///
/// int, float, bool, string, source — return default values for script inputs.
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};

pub(crate) fn input_int(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "input.int requires at least 1 argument (default value)".to_string(),
        ));
    }
    // Return the default value (first argument)
    args[0].as_num().map(|n: f64| Value::Number(n.round()))
}

pub(crate) fn input_float(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "input.float requires at least 1 argument (default value)".to_string(),
        ));
    }
    // Return the default value
    Ok(args[0].clone())
}

pub(crate) fn input_bool(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "input.bool requires at least 1 argument (default value)".to_string(),
        ));
    }
    // Return the default value
    Ok(args[0].clone())
}

pub(crate) fn input_string(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "input.string requires at least 1 argument (default value)".to_string(),
        ));
    }
    // Return the default value
    Ok(args[0].clone())
}

pub(crate) fn input_src(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    // Returns the close series by default
    if args.is_empty() {
        Ok(Value::String("close".to_string()))
    } else {
        Ok(args[0].clone())
    }
}
