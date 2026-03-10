/// String built-in functions (str namespace)
///
/// contains, length, replace, split, tostring, tonumber, upper, lower,
/// startswith, endswith, substring, trim.
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};

pub(crate) fn str_contains(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "str.contains requires 2 arguments: source, substr".to_string(),
        ));
    }
    let source = match &args[0] {
        Value::String(s) => s.clone(),
        other => format!("{other:?}"),
    };
    let substr = match &args[1] {
        Value::String(s) => s.clone(),
        other => format!("{other:?}"),
    };
    Ok(Value::Boolean(source.contains(&substr)))
}

pub(crate) fn str_length(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.length requires 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::Number(s.len() as f64)),
        _ => Err(RuntimeError::TypeError(
            "str.length requires a string".to_string(),
        )),
    }
}

pub(crate) fn str_replace(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "str.replace requires 3 arguments: source, target, replacement".to_string(),
        ));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::String(src), Value::String(target), Value::String(repl)) => {
            Ok(Value::String(src.replace(target.as_str(), repl.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "str.replace requires string arguments".to_string(),
        )),
    }
}

pub(crate) fn str_split(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "str.split requires 2 arguments: source, delimiter".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::String(src), Value::String(delim)) => {
            let parts: Vec<Value> = src
                .split(delim.as_str())
                .map(|s| Value::String(s.to_string()))
                .collect();
            Ok(Value::Array(parts))
        }
        _ => Err(RuntimeError::TypeError(
            "str.split requires string arguments".to_string(),
        )),
    }
}

pub(crate) fn str_tostring(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.tostring requires 1 argument".to_string(),
        ));
    }
    let result = match &args[0] {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::String(s) => s.clone(),
        Value::Boolean(b) => format!("{b}"),
        Value::Null => "NaN".to_string(),
        Value::Series(_) => "series".to_string(),
        Value::Array(a) => format!("[{} elements]", a.len()),
    };
    Ok(Value::String(result))
}

pub(crate) fn str_tonumber(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.tonumber requires 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::Number(s.parse::<f64>().unwrap_or(f64::NAN))),
        Value::Number(n) => Ok(Value::Number(*n)),
        _ => Ok(Value::Number(f64::NAN)),
    }
}

pub(crate) fn str_upper(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.upper requires 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::String(s.to_uppercase())),
        _ => Err(RuntimeError::TypeError(
            "str.upper requires a string".to_string(),
        )),
    }
}

pub(crate) fn str_lower(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.lower requires 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::String(s.to_lowercase())),
        _ => Err(RuntimeError::TypeError(
            "str.lower requires a string".to_string(),
        )),
    }
}

pub(crate) fn str_startswith(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "str.startswith requires 2 arguments: source, prefix".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::String(s), Value::String(prefix)) => {
            Ok(Value::Boolean(s.starts_with(prefix.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "str.startswith requires string arguments".to_string(),
        )),
    }
}

pub(crate) fn str_endswith(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "str.endswith requires 2 arguments: source, suffix".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::String(s), Value::String(suffix)) => {
            Ok(Value::Boolean(s.ends_with(suffix.as_str())))
        }
        _ => Err(RuntimeError::TypeError(
            "str.endswith requires string arguments".to_string(),
        )),
    }
}

pub(crate) fn str_substring(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "str.substring requires 2-3 arguments: source, start[, end]".to_string(),
        ));
    }
    let source = match &args[0] {
        Value::String(s) => s.clone(),
        _ => {
            return Err(RuntimeError::TypeError(
                "str.substring requires a string".to_string(),
            ));
        }
    };
    let start = args[1].as_num()? as usize;
    let end = args
        .get(2)
        .and_then(|v| v.as_num().ok())
        .map(|n| n as usize)
        .unwrap_or(source.len());
    let start = start.min(source.len()).min(end);
    let end = end.min(source.len());
    let result: String = source.chars().skip(start).take(end - start).collect();
    Ok(Value::String(result))
}

pub(crate) fn str_trim(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "str.trim requires 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::String(s.trim().to_string())),
        _ => Err(RuntimeError::TypeError(
            "str.trim requires a string".to_string(),
        )),
    }
}
