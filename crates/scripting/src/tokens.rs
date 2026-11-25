//! The pieces of a script an editor colours, read by the scanner the compile check reads, so the
//! editor and the compiler agree on what is a string or a comment.

use crate::rhai::engine::HELPERS;
use crate::rhai::syntax::{self, DECLARATIONS, NA, PRICES, Token};
use std::ops::Range;

/// What a piece of a script is, for colouring it in an editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenClass {
    /// A keyword of the language, such as `let`, `if` or `fn`.
    Keyword,
    /// A declaration: `display`, `input`, `state`, `series`, `bars`, `plot`, `fill` or `study`.
    Declaration,
    /// A series such as `close`, `na`, or a helper such as `sma` where it is called.
    Builtin,
    /// A number.
    Number,
    /// A string or a character, with its quotes.
    Text,
    /// A comment.
    Comment,
}

/// The statement and value keywords of the language.
const KEYWORDS: [&str; 23] = [
    "let", "const", "if", "else", "switch", "do", "while", "until", "loop", "for", "in",
    "continue", "break", "return", "throw", "try", "catch", "fn", "private", "true", "false",
    "this", "global",
];

/// The classified pieces of `text`, in order, on character boundaries and without overlap.
/// Names, operators and anything else are left out, so an editor keeps its own colour there.
/// Never fails: an unterminated string or comment runs to the end of the text.
pub fn tokens(text: &str) -> Vec<(Range<usize>, TokenClass)> {
    let pieces = syntax::pieces(text);
    let number = |at: usize| {
        matches!(pieces.get(at), Some((Some(Token::Word(word)), ..))
            if word.starts_with(|c: char| c.is_ascii_digit()))
    };
    let mut classified = Vec::new();
    let mut at = 0;
    while let Some((token, _, range)) = pieces.get(at) {
        let mut range = range.clone();
        let class = match *token {
            None => Some(TokenClass::Comment),
            Some(Token::Text(_)) => Some(TokenClass::Text),
            Some(Token::Word(_)) if number(at) => {
                // `1.5` scans as `1`, `.` and `5`, each where the one before ends.
                if let (Some((Some(Token::Symbol('.')), _, dot)), Some((_, _, after))) =
                    (pieces.get(at + 1), pieces.get(at + 2))
                    && dot.start == range.end
                    && after.start == dot.end
                    && number(at + 2)
                {
                    range.end = after.end;
                    at += 2;
                }
                Some(TokenClass::Number)
            }
            Some(Token::Word(word)) => {
                let call = matches!(pieces.get(at + 1), Some((Some(Token::Symbol('(')), ..)));
                name_class(word, call)
            }
            Some(Token::Symbol(_)) => None,
        };
        if let Some(class) = class {
            classified.push((range, class));
        }
        at += 1;
    }
    classified
}

/// What a name is, `call` when a `(` follows it: a declaration, a built-in name or a keyword;
/// `None` for any other name. A helper's name is a variable's too where it is not called.
fn name_class(word: &str, call: bool) -> Option<TokenClass> {
    if DECLARATIONS
        .iter()
        .any(|declaration| declaration.name == word)
    {
        Some(TokenClass::Declaration)
    } else if PRICES.iter().any(|(name, ..)| *name == word)
        || NA.0 == word
        || (call && HELPERS.iter().any(|helper| helper.name == word))
    {
        Some(TokenClass::Builtin)
    } else if KEYWORDS.contains(&word) {
        Some(TokenClass::Keyword)
    } else {
        None
    }
}
