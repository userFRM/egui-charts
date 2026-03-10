/// Math built-in functions (math namespace)
///
/// abs, max, min, round, floor, ceil, sqrt, pow, log, log10, exp, sign, avg, sum.
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};

pub(crate) fn math_abs(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.abs requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.abs()))
}

pub(crate) fn math_max(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "math.max requires at least 2 arguments".to_string(),
        ));
    }
    let a = args[0].as_num()?;
    let b = args[1].as_num()?;
    Ok(Value::Number(a.max(b)))
}

pub(crate) fn math_min(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "math.min requires at least 2 arguments".to_string(),
        ));
    }
    let a = args[0].as_num()?;
    let b = args[1].as_num()?;
    Ok(Value::Number(a.min(b)))
}

pub(crate) fn math_round(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.round requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.round()))
}

pub(crate) fn math_floor(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.floor requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.floor()))
}

pub(crate) fn math_ceil(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.ceil requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.ceil()))
}

pub(crate) fn math_sqrt(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.sqrt requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.sqrt()))
}

pub(crate) fn math_pow(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "math.pow requires 2 arguments".to_string(),
        ));
    }
    let base = args[0].as_num()?;
    let exp = args[1].as_num()?;
    Ok(Value::Number(base.powf(exp)))
}

pub(crate) fn math_log(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.log requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.ln()))
}

pub(crate) fn math_log10(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.log10 requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.log10()))
}

pub(crate) fn math_exp(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.exp requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    Ok(Value::Number(n.exp()))
}

pub(crate) fn math_sign(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.sign requires 1 argument".to_string(),
        ));
    }
    let n = args[0].as_num()?;
    // Pine Script semantics: sign(0) = 0 (unlike Rust's signum which returns 1.0 for +0.0)
    let sign = if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        0.0
    };
    Ok(Value::Number(sign))
}

pub(crate) fn math_avg(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.avg requires at least 1 argument".to_string(),
        ));
    }
    let sum: f64 = args.iter().filter_map(|v: &Value| v.as_num().ok()).sum();
    let count = args.len() as f64;
    Ok(Value::Number(sum / count))
}

pub(crate) fn math_sum(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "math.sum requires at least 1 argument".to_string(),
        ));
    }
    let sum: f64 = args.iter().filter_map(|v: &Value| v.as_num().ok()).sum();
    Ok(Value::Number(sum))
}
