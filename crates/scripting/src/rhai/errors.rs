//! Rhai's errors in a script's terms, at one-based lines and character columns.

use super::engine::{NO_HISTORY, span};
use crate::ScriptError;
use crate::runtime::BarError;
use ::rhai::{EvalAltResult, LexError, ParseError, ParseErrorType};

/// A compile error, in the terms of `source`.
pub(crate) fn compile(error: &ParseError, source: &str) -> ScriptError {
    let at = span(error.position());
    let message = match (error.err_type(), word_at(source, at)) {
        // As a method, `x.curry()`, a disabled keyword reads as a missing property.
        (_, Some(word @ ("eval" | "curry"))) => format!("reserved keyword '{word}' is disabled"),
        (ParseErrorType::VariableUndefined(name), _) => {
            format!("{name} is not declared; declare it with let, input, state or series")
        }
        (ParseErrorType::BadInput(LexError::UnexpectedInput(token)), _)
            if token == "|" || token == "||" =>
        {
            "anonymous functions are not available; declare the function with fn and pass \
             Fn(\"name\")"
                .into()
        }
        (other, _) => other.to_string(),
    };
    ScriptError::at(message, at, None)
}

/// The word that starts at `span` in `source`.
fn word_at(source: &str, span: crate::runtime::program::Span) -> Option<&str> {
    let line = source.lines().nth((span.line as usize).checked_sub(1)?)?;
    let start = line
        .char_indices()
        .nth((span.column as usize).checked_sub(1)?)?
        .0;
    let word = &line[start..];
    let end = word
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(word.len());
    Some(&word[..end])
}

/// A runtime error of the bar program, at the innermost position, in the terms of `source`.
pub(crate) fn runtime(error: &EvalAltResult, source: &str) -> BarError {
    let mut error = error;
    while let EvalAltResult::ErrorInFunctionCall(_, _, inner, _) = error {
        error = inner;
    }
    let position = error.position();
    let message = match error {
        EvalAltResult::ErrorRuntime(message, _) => {
            let message = message.to_string();
            if message == NO_HISTORY {
                named_history(source, span(position))
                    .map_or(message, |name| NO_HISTORY.replace("NAME", name))
            } else {
                message
            }
        }
        EvalAltResult::ErrorFunctionNotFound(signature, _) => not_found(signature),
        EvalAltResult::ErrorAssignmentToConstant(name, _) => {
            format!("cannot assign to the constant {name}")
        }
        EvalAltResult::ErrorStackOverflow(_) => format!(
            "functions call each other more than {} deep",
            super::engine::MAX_CALL_LEVELS
        ),
        other => {
            // Display ends with the position, which the error reports on its own.
            let text = other.to_string();
            let suffix = format!(" ({position})");
            match text.strip_suffix(&suffix) {
                Some(message) if !position.is_none() => message.to_owned(),
                _ => text,
            }
        }
    };
    BarError {
        message,
        span: span(position),
    }
}

/// `name (types)` as Rhai writes a call no function takes, in a script's terms.
fn not_found(signature: &str) -> String {
    let Some((name, types)) = signature.split_once(" (") else {
        return format!("no function {signature}");
    };
    let types: Vec<_> = types
        .trim_end_matches(')')
        .split(", ")
        .filter(|kind| !kind.is_empty())
        .map(|kind| match kind {
            "f64" => "number",
            "i64" => "int",
            kind if kind.contains("String") || kind.contains("str") => "string",
            kind if kind.contains("Handle") => "series",
            "()" => "nothing",
            kind => kind,
        })
        .collect();
    let operator = !name.starts_with(|c: char| c.is_alphabetic() || c == '_');
    format!(
        "no {} {name} takes ({})",
        if operator { "operator" } else { "function" },
        types.join(", ")
    )
}

/// The name before the `[` at `span`, which read an earlier bar of a value without history.
fn named_history(source: &str, span: crate::runtime::program::Span) -> Option<&str> {
    let line = source.lines().nth((span.line as usize).checked_sub(1)?)?;
    let bracket = line
        .char_indices()
        .nth((span.column as usize).checked_sub(1)?)?
        .0;
    let before = line[..bracket].trim_end();
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()?
        .0;
    Some(&before[start..])
}
