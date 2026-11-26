//! The alert calls of a script, read when it compiles, so a study lists its alert conditions
//! before it runs.

use super::syntax::{Token, pieces};
use crate::ScriptError;
use crate::runtime::program::Span;
use ::rhai::AST;

/// Most alert messages a script may declare.
pub(crate) const MAX_ALERTS: usize = 32;

/// Why a message is refused that is not read as written.
const WRITTEN: &str = "the message is a string in double quotes, without escapes";

/// The messages of `source`'s calls of `alert`, which compiled into `ast`, in order of first
/// appearance, or the first malformed call. A script that defines a function named `alert` calls
/// its own and declares none; `x.alert(...)` is a method call and declares none.
pub(crate) fn declared(source: &str, ast: &AST) -> Result<Vec<String>, ScriptError> {
    let mut messages: Vec<String> = Vec::new();
    if ast
        .iter_functions()
        .any(|function| function.name == "alert")
    {
        return Ok(messages);
    }
    let tokens: Vec<_> = pieces(source)
        .into_iter()
        .filter_map(|(token, span, range)| Some((token?, span, range)))
        .collect();
    for (at, (token, ..)) in tokens.iter().enumerate() {
        let symbol = |at: usize, c: char| tokens.get(at).is_some_and(|t| t.0 == Token::Symbol(c));
        // A range `a..alert(...)` ends in a `.` too.
        let method = at > 1 && symbol(at - 1, '.') && !symbol(at - 2, '.');
        if *token != Token::Word("alert") || !symbol(at + 1, '(') || method {
            continue;
        }
        let refused = |at: usize, message: &str| {
            let span = tokens.get(at).map_or(Span::default(), |token| token.1);
            Err(ScriptError::at(message.to_owned(), span, None))
        };
        // Rhai unescapes a string only after a `\`, so the text written is the value passed.
        let message = match tokens.get(at + 2) {
            Some((Token::Text(text), _, range))
                if source[range.clone()].starts_with('"') && !text.contains('\\') =>
            {
                *text
            }
            _ => return refused(at + 2, WRITTEN),
        };
        if message.trim().is_empty() {
            return refused(at + 2, "the message is empty");
        }
        if symbol(at + 3, ')') {
            return refused(at + 3, "alert takes a message and a condition");
        }
        // A message such as `"a" + b` is no string written in the call.
        if !symbol(at + 3, ',') {
            return refused(at + 2, WRITTEN);
        }
        if messages.iter().all(|seen| seen != message) {
            if messages.len() == MAX_ALERTS {
                return refused(
                    at + 2,
                    &format!("a script declares at most {MAX_ALERTS} alerts"),
                );
            }
            messages.push(message.to_owned());
        }
    }
    Ok(messages)
}
