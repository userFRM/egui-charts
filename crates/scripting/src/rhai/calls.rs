//! The names a script may call, checked when it compiles. Rhai resolves a call when it runs, so
//! without this check a misspelt function in a branch no bar takes stays hidden.

use super::engine::HELPERS;
use super::syntax::{DECLARATIONS, Token, tokens};
use crate::ScriptError;
use ::rhai::AST;
use std::collections::HashSet;

/// Rhai's keywords that a script may write before a `(` without calling a function, as it may the
/// declarations.
const KEYWORDS: &str = "if while until for in switch return throw break catch";

/// The functions the engine itself resolves.
const BUILT_IN: &str = "\
    Fn call type_of is_shared is_def_fn is_def_var";

/// The functions of Rhai's standard package, without `sleep`, which a script does not have.
const STANDARD: &str = "\
    E PI abs acos acosh all append as_string asin asinh atan atanh bits blob bytes ceiling \
    chars chop clear contains cos cosh crop debug dedup drain end ends_with exit exp extract \
    fill_with filter find find_map floor for_each fraction get get_bit get_bits \
    get_fn_metadata_list hypot index_of insert int is_anonymous is_empty is_even is_exclusive \
    is_finite is_inclusive is_infinite is_nan is_odd is_zero keys len ln log make_lower \
    make_upper map max min mixin name order order_by order_desc pad parse_be_float \
    parse_be_int parse_float parse_int parse_json parse_le_float parse_le_int pop print push \
    range reduce reduce_rev remove replace retain reverse round set set_bit set_bits set_tag \
    shift sign sin sinh some sort sort_by sort_desc splice split split_rev sqrt start \
    starts_with sub_string tag take tan tanh to_array to_binary to_blob to_chars to_debug \
    to_degrees to_float to_hex to_int to_json to_lower to_octal to_radians to_string to_upper \
    trim truncate values write_ascii write_be write_le write_utf8 zip";

/// The first call in `source`, a script that compiled into `ast`, of a name that neither the
/// script, the helpers nor Rhai defines. A method may also be a function a map holds, so a name
/// the script writes outside a call too passes there.
pub(crate) fn unknown(source: &str, ast: &AST) -> Option<ScriptError> {
    let tokens = tokens(source);
    let token = |at: usize| tokens.get(at).map(|(token, _)| *token);
    let symbol = |at: usize, symbol: char| token(at) == Some(Token::Symbol(symbol));
    let called = |at: usize| symbol(at + 1, '(') || symbol(at + 1, '!') && symbol(at + 2, '(');
    let known: HashSet<&str> = [KEYWORDS, BUILT_IN, STANDARD]
        .into_iter()
        .flat_map(str::split_whitespace)
        .chain(DECLARATIONS.iter().map(|declaration| declaration.name))
        .chain(HELPERS.iter().map(|helper| helper.name))
        .chain(ast.iter_functions().map(|function| function.name))
        .collect();
    let keys: HashSet<&str> = (0..tokens.len())
        .filter_map(|at| match tokens[at].0 {
            Token::Word(word) if !called(at) => Some(word),
            Token::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    (0..tokens.len()).find_map(|at| {
        let (Token::Word(name), span) = tokens[at] else {
            return None;
        };
        // A number takes a keyword after it into its word, as in `1if (x)`.
        let number = name.starts_with(|c: char| c.is_ascii_digit());
        // A range `a..f(x)` ends in a `.` too.
        let method = at > 1 && symbol(at - 1, '.') && !symbol(at - 2, '.');
        // `input NAME = KIND(...)` and `bars NAME = history(...)`.
        let kind = at > 2
            && symbol(at - 1, '=')
            && matches!(token(at - 3), Some(Token::Word("input" | "bars")));
        let passes =
            !called(at) || number || known.contains(name) || kind || method && keys.contains(name);
        (!passes).then(|| {
            let message = format!("no function {name}; declare it with fn or call a helper");
            ScriptError::at(message, span, None)
        })
    })
}
