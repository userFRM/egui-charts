//! Arithmetic over symbols, for spread charts.

use crate::model::Bar;
use std::fmt;

/// An arithmetic expression over symbols, such as `AAPL-MSFT` or `(BTCUSD+ETHUSD)/2`, charted as
/// one synthetic series.
///
/// Symbols, decimal numbers, `+`, `-`, `*`, `/`, a leading minus and parentheses are accepted,
/// with the usual precedence; spaces are ignored. A symbol is a run of letters, digits and
/// `.`, `:`, `_` or `!`, such as `NASDAQ:AAPL`, `BRK.B` or `ES1!`, that is not a number. The
/// text it formats to is the symbol of its [`Instrument`](crate::model::Instrument).
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use egui_charts::model::Bar;
/// use egui_charts::services::SymbolExpression;
///
/// let spread = SymbolExpression::parse("aapl - msft").unwrap();
/// assert_eq!(spread.symbols(), ["aapl", "msft"]);
/// assert_eq!(spread.to_string(), "aapl-msft");
///
/// let day = |d| Utc.with_ymd_and_hms(2024, 3, d, 0, 0, 0).unwrap();
/// let aapl = [Bar::new(day(1), 170.0, 172.0, 169.0, 171.0, 0.0)];
/// let msft = [Bar::new(day(1), 410.0, 415.0, 405.0, 412.0, 0.0)];
/// let bars = spread
///     .bars(|symbol| match symbol {
///         "aapl" => Some(&aapl[..]),
///         "msft" => Some(&msft[..]),
///         _ => None,
///     })
///     .unwrap();
/// assert_eq!((bars[0].open, bars[0].close), (-240.0, -241.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolExpression {
    root: Node,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Number(f64),
    Symbol(String),
    Negate(Box<Node>),
    Binary(Operator, Box<Node>, Box<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operator {
    fn precedence(self) -> u8 {
        match self {
            Self::Add | Self::Subtract => 1,
            Self::Multiply | Self::Divide => 2,
        }
    }

    fn sign(self) -> char {
        match self {
            Self::Add => '+',
            Self::Subtract => '-',
            Self::Multiply => '*',
            Self::Divide => '/',
        }
    }
}

/// Why text is not a [`SymbolExpression`], or why one cannot be charted.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ExpressionError {
    /// A character that belongs to no symbol, number, operator or parenthesis.
    UnexpectedCharacter(char),
    /// An operator, a parenthesis or the end of the text where a symbol or number belongs.
    MissingOperand,
    /// A symbol, number or parenthesis that follows another without an operator.
    MissingOperator(String),
    /// A `(` without its `)`, or a `)` without its `(`.
    UnbalancedParenthesis,
    /// Numbers alone, with no symbol to chart.
    NoSymbol,
    /// A symbol the instrument catalog or the supplied history does not know.
    UnknownSymbol(String),
}

impl fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedCharacter(found) => write!(f, "Unexpected character {found:?}"),
            Self::MissingOperand => write!(f, "A symbol or number is missing"),
            Self::MissingOperator(found) => write!(f, "An operator is missing before {found}"),
            Self::UnbalancedParenthesis => write!(f, "Unbalanced parentheses"),
            Self::NoSymbol => write!(f, "The expression has no symbol"),
            Self::UnknownSymbol(symbol) => write!(f, "Unknown symbol {symbol}"),
        }
    }
}

impl std::error::Error for ExpressionError {}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Symbol(String),
    Operator(Operator),
    Open,
    Close,
}

fn tokens(text: &str) -> Result<Vec<Token>, ExpressionError> {
    let symbol_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | ':' | '_' | '!');
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => continue,
            '+' => Token::Operator(Operator::Add),
            '-' => Token::Operator(Operator::Subtract),
            '*' => Token::Operator(Operator::Multiply),
            '/' => Token::Operator(Operator::Divide),
            '(' => Token::Open,
            ')' => Token::Close,
            _ if symbol_char(c) => {
                let mut end = start + c.len_utf8();
                while let Some(&(at, next)) = chars.peek()
                    && symbol_char(next)
                {
                    end = at + next.len_utf8();
                    chars.next();
                }
                let word = &text[start..end];
                let numeric = word.chars().all(|c| c.is_ascii_digit() || c == '.');
                match word.parse::<f64>() {
                    Ok(number) if numeric && number.is_finite() => Token::Number(number),
                    _ => Token::Symbol(word.to_owned()),
                }
            }
            _ => return Err(ExpressionError::UnexpectedCharacter(c)),
        };
        tokens.push(token);
    }
    Ok(tokens)
}

/// Recursive descent over the tokens: sums of products of signed factors.
struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn sum(&mut self) -> Result<Node, ExpressionError> {
        let mut node = self.product()?;
        while let Some(&Token::Operator(op @ (Operator::Add | Operator::Subtract))) = self.peek() {
            self.at += 1;
            node = Node::Binary(op, Box::new(node), Box::new(self.product()?));
        }
        Ok(node)
    }

    fn product(&mut self) -> Result<Node, ExpressionError> {
        let mut node = self.factor()?;
        while let Some(&Token::Operator(op @ (Operator::Multiply | Operator::Divide))) = self.peek()
        {
            self.at += 1;
            node = Node::Binary(op, Box::new(node), Box::new(self.factor()?));
        }
        Ok(node)
    }

    fn factor(&mut self) -> Result<Node, ExpressionError> {
        let token = self.peek().cloned();
        self.at += 1;
        match token {
            Some(Token::Number(number)) => Ok(Node::Number(number)),
            Some(Token::Symbol(symbol)) => Ok(Node::Symbol(symbol)),
            Some(Token::Operator(Operator::Subtract)) => Ok(Node::Negate(Box::new(self.factor()?))),
            Some(Token::Open) => {
                let node = self.sum()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(match self.peek() {
                        None => ExpressionError::UnbalancedParenthesis,
                        Some(token) => ExpressionError::MissingOperator(describe(token)),
                    });
                }
                self.at += 1;
                Ok(node)
            }
            Some(Token::Close) if self.at == 1 => Err(ExpressionError::UnbalancedParenthesis),
            _ => Err(ExpressionError::MissingOperand),
        }
    }
}

fn describe(token: &Token) -> String {
    match token {
        Token::Number(number) => number.to_string(),
        Token::Symbol(symbol) => symbol.clone(),
        Token::Operator(op) => op.sign().to_string(),
        Token::Open => "(".into(),
        Token::Close => ")".into(),
    }
}

impl SymbolExpression {
    /// Parses `text`, which may also be a lone symbol.
    pub fn parse(text: &str) -> Result<Self, ExpressionError> {
        let mut parser = Parser {
            tokens: tokens(text)?,
            at: 0,
        };
        let root = parser.sum()?;
        if let Some(token) = parser.peek() {
            return Err(if *token == Token::Close {
                ExpressionError::UnbalancedParenthesis
            } else {
                ExpressionError::MissingOperator(describe(token))
            });
        }
        let expression = Self { root };
        if expression.symbols().is_empty() {
            return Err(ExpressionError::NoSymbol);
        }
        Ok(expression)
    }

    /// Whether the expression is more than one symbol, which is what a spread chart needs.
    pub fn is_spread(&self) -> bool {
        !matches!(self.root, Node::Symbol(_))
    }

    /// The symbols of the expression, each once, in order of appearance.
    pub fn symbols(&self) -> Vec<&str> {
        fn walk<'a>(node: &'a Node, found: &mut Vec<&'a str>) {
            match node {
                Node::Number(_) => {}
                Node::Symbol(symbol) => {
                    if !found.contains(&symbol.as_str()) {
                        found.push(symbol);
                    }
                }
                Node::Negate(inner) => walk(inner, found),
                Node::Binary(_, left, right) => {
                    walk(left, found);
                    walk(right, found);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.root, &mut found);
        found
    }

    /// Replaces each symbol by `rename(symbol)`, such as the spelling of the instrument catalog.
    pub fn rename_symbols(&mut self, mut rename: impl FnMut(&str) -> String) {
        fn walk(node: &mut Node, rename: &mut impl FnMut(&str) -> String) {
            match node {
                Node::Number(_) => {}
                Node::Symbol(symbol) => *symbol = rename(symbol),
                Node::Negate(inner) => walk(inner, rename),
                Node::Binary(_, left, right) => {
                    walk(left, rename);
                    walk(right, rename);
                }
            }
        }
        walk(&mut self.root, &mut rename);
    }

    /// The value of the expression with each symbol replaced by `value(symbol)`, or `None` when
    /// a symbol has no value or the result is not finite, as after a division by zero.
    pub fn evaluate(&self, value: impl Fn(&str) -> Option<f64>) -> Option<f64> {
        fn walk(node: &Node, value: &impl Fn(&str) -> Option<f64>) -> Option<f64> {
            Some(match node {
                Node::Number(number) => *number,
                Node::Symbol(symbol) => value(symbol)?,
                Node::Negate(inner) => -walk(inner, value)?,
                Node::Binary(op, left, right) => {
                    let (left, right) = (walk(left, value)?, walk(right, value)?);
                    match op {
                        Operator::Add => left + right,
                        Operator::Subtract => left - right,
                        Operator::Multiply => left * right,
                        Operator::Divide => left / right,
                    }
                }
            })
        }
        walk(&self.root, &value).filter(|result| result.is_finite())
    }

    /// The synthetic series from the time-ordered bars `history` returns for each symbol.
    ///
    /// A bar is made for each time at which every symbol has a bar. Its open and close apply the
    /// expression to the components' opens and closes, its high and low to their highs and lows,
    /// and the high and low then widen to cover all four results, since a difference or a
    /// quotient of highs need not be the highest value. Times whose result is not finite are left
    /// out, and volume is unknown. A symbol without history is an
    /// [`ExpressionError::UnknownSymbol`].
    pub fn bars<'a>(
        &self,
        history: impl Fn(&str) -> Option<&'a [Bar]>,
    ) -> Result<Vec<Bar>, ExpressionError> {
        let components = self
            .symbols()
            .into_iter()
            .map(|symbol| {
                history(symbol)
                    .map(|bars| (symbol, bars))
                    .ok_or_else(|| ExpressionError::UnknownSymbol(symbol.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Some((_, timeline)) = components.first() else {
            return Ok(Vec::new());
        };
        let mut bars = Vec::new();
        for bar in timeline.iter() {
            let Some(aligned) = components
                .iter()
                .map(|(symbol, bars)| {
                    let at = bars.binary_search_by_key(&bar.time, |bar| bar.time).ok()?;
                    Some((*symbol, &bars[at]))
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let field = |pick: fn(&Bar) -> f64| {
                self.evaluate(|symbol| {
                    aligned
                        .iter()
                        .find(|(name, _)| *name == symbol)
                        .map(|(_, bar)| pick(bar))
                })
            };
            let (Some(open), Some(high), Some(low), Some(close)) = (
                field(|bar| bar.open),
                field(|bar| bar.high),
                field(|bar| bar.low),
                field(|bar| bar.close),
            ) else {
                continue;
            };
            let values = [open, high, low, close];
            bars.push(Bar::new(
                bar.time,
                open,
                values.into_iter().fold(f64::MIN, f64::max),
                values.into_iter().fold(f64::MAX, f64::min),
                close,
                f64::NAN,
            ));
        }
        Ok(bars)
    }
}

impl fmt::Display for SymbolExpression {
    /// The expression without spaces and with only the parentheses its order needs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write(node: &Node, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match node {
                Node::Number(number) => write!(f, "{number}"),
                Node::Symbol(symbol) => f.write_str(symbol),
                Node::Negate(inner) => {
                    f.write_str("-")?;
                    operand(inner, 3, f)
                }
                Node::Binary(op, left, right) => {
                    operand(left, op.precedence(), f)?;
                    write!(f, "{}", op.sign())?;
                    // A right operand of equal precedence keeps its parentheses: a-(b-c).
                    operand(right, op.precedence() + 1, f)
                }
            }
        }
        fn operand(node: &Node, min: u8, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let precedence = match node {
                Node::Binary(op, ..) => op.precedence(),
                _ => 3,
            };
            if precedence < min {
                f.write_str("(")?;
                write(node, f)?;
                f.write_str(")")
            } else {
                write(node, f)
            }
        }
        write(&self.root, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Precedence, signs and parentheses read and print as written, and each malformed text
    /// names its fault.
    #[test]
    fn expressions_parse_print_and_report_faults() {
        for (text, expected) in [
            ("AAPL - MSFT", Ok("AAPL-MSFT")),
            ("(BTCUSD+ETHUSD)/2", Ok("(BTCUSD+ETHUSD)/2")),
            ("a-(b-c)", Ok("a-(b-c)")),
            ("a-b-c", Ok("a-b-c")),
            ("a*b+c", Ok("a*b+c")),
            ("a*(b+c)", Ok("a*(b+c)")),
            ("-a+2.5*NASDAQ:B", Ok("-a+2.5*NASDAQ:B")),
            ("ES1!/BRK.B", Ok("ES1!/BRK.B")),
            ("", Err(ExpressionError::MissingOperand)),
            ("AAPL -", Err(ExpressionError::MissingOperand)),
            (
                "AAPL MSFT",
                Err(ExpressionError::MissingOperator("MSFT".into())),
            ),
            ("(AAPL-MSFT", Err(ExpressionError::UnbalancedParenthesis)),
            ("AAPL-MSFT)", Err(ExpressionError::UnbalancedParenthesis)),
            ("AAPL%2", Err(ExpressionError::UnexpectedCharacter('%'))),
            ("2*3", Err(ExpressionError::NoSymbol)),
        ] {
            let parsed = SymbolExpression::parse(text).map(|e| e.to_string());
            assert_eq!(parsed, expected.map(str::to_owned), "{text:?}");
        }
    }

    /// The synthetic bars against hand-computed values: only times every symbol trades, the
    /// expression applied to each price, the high and low widened over all four results, and a
    /// zero divisor dropping its time.
    #[test]
    fn spread_bars_align_components_by_time() {
        let day = |d| Utc.with_ymd_and_hms(2024, 3, d, 0, 0, 0).unwrap();
        let a = [
            Bar::new(day(1), 10.0, 14.0, 9.0, 12.0, 100.0),
            Bar::new(day(2), 12.0, 13.0, 11.0, 11.5, 100.0),
            Bar::new(day(3), 11.0, 15.0, 10.0, 14.0, 100.0),
        ];
        let b = [
            Bar::new(day(1), 4.0, 8.0, 3.5, 5.0, 50.0),
            Bar::new(day(3), 2.0, 0.0, 1.0, 7.0, 50.0),
        ];
        let history = |symbol: &str| match symbol {
            "A" => Some(&a[..]),
            "B" => Some(&b[..]),
            _ => None,
        };
        let ohlc = |text: &str| {
            SymbolExpression::parse(text)
                .unwrap()
                .bars(history)
                .map(|bars| {
                    bars.iter()
                        .map(|bar| (bar.time, bar.open, bar.high, bar.low, bar.close))
                        .collect::<Vec<_>>()
                })
        };
        // Day 1: opens 10-4, highs 14-8, lows 9-3.5, closes 12-5. Day 3: 11-2, 15-0, 10-1, 14-7.
        assert_eq!(
            ohlc("A-B"),
            Ok(vec![
                (day(1), 6.0, 7.0, 5.5, 7.0),
                (day(3), 9.0, 15.0, 7.0, 7.0)
            ])
        );
        // Day 1: 10/4, 14/8, 9/3.5, 12/5; day 3 divides a high by zero.
        assert_eq!(
            ohlc("A/B"),
            Ok(vec![(day(1), 2.5, 2.5714285714285716, 1.75, 2.4)])
        );
        assert_eq!(ohlc("A-C"), Err(ExpressionError::UnknownSymbol("C".into())));
    }
}
