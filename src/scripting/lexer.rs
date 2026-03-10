/// Pine Script lexer - tokenizes Pine Script source code
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Number(f64),
    String(String),
    Boolean(bool),

    // Identifiers and keywords
    Identifier(String),

    // Keywords
    If,
    Else,
    For,
    While,
    True,
    False,
    Var,
    Varip,

    // Built-in variables
    Open,
    High,
    Low,
    Close,
    Volume,
    Time,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equal,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Not,
    Question,
    Colon,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Dot,
    Arrow, // =>

    // Special
    Newline,
    Eof,
    Comment(String),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self { kind, line, column }
    }
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token();
            if token.kind == TokenKind::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }

        tokens
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let line = self.line;
        let column = self.column;

        if self.is_at_end() {
            return Token::new(TokenKind::Eof, line, column);
        }

        let ch = self.curr_char();

        // Comments
        if ch == '/' && self.peek_char() == Some('/') {
            return self.read_comment();
        }

        // Newline
        if ch == '\n' {
            self.advance();
            self.line += 1;
            self.column = 1;
            return Token::new(TokenKind::Newline, line, column);
        }

        // String literals
        if ch == '"' || ch == '\'' {
            return self.read_string(ch);
        }

        // Numbers
        if ch.is_ascii_digit()
            || (ch == '.' && self.peek_char().is_some_and(|c| c.is_ascii_digit()))
        {
            return self.read_num();
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' {
            return self.read_identifier();
        }

        // Operators and delimiters
        let token = match ch {
            '+' => Token::new(TokenKind::Plus, line, column),
            '-' => Token::new(TokenKind::Minus, line, column),
            '*' => Token::new(TokenKind::Star, line, column),
            '/' => Token::new(TokenKind::Slash, line, column),
            '%' => Token::new(TokenKind::Percent, line, column),
            '(' => Token::new(TokenKind::LeftParen, line, column),
            ')' => Token::new(TokenKind::RightParen, line, column),
            '[' => Token::new(TokenKind::LeftBracket, line, column),
            ']' => Token::new(TokenKind::RightBracket, line, column),
            ',' => Token::new(TokenKind::Comma, line, column),
            '.' => Token::new(TokenKind::Dot, line, column),
            '?' => Token::new(TokenKind::Question, line, column),
            ':' => Token::new(TokenKind::Colon, line, column),
            '=' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::new(TokenKind::EqualEqual, line, column)
                } else if self.peek_char() == Some('>') {
                    self.advance();
                    Token::new(TokenKind::Arrow, line, column)
                } else {
                    Token::new(TokenKind::Equal, line, column)
                }
            }
            '!' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::new(TokenKind::NotEqual, line, column)
                } else {
                    Token::new(TokenKind::Not, line, column)
                }
            }
            '<' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::new(TokenKind::LessEqual, line, column)
                } else {
                    Token::new(TokenKind::Less, line, column)
                }
            }
            '>' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::new(TokenKind::GreaterEqual, line, column)
                } else {
                    Token::new(TokenKind::Greater, line, column)
                }
            }
            _ => {
                // Unknown character, skip it
                self.advance();
                return self.next_token();
            }
        };

        self.advance();
        token
    }

    fn read_comment(&mut self) -> Token {
        let line = self.line;
        let column = self.column;

        self.advance(); // Skip first /
        self.advance(); // Skip second /

        let mut comment = String::new();
        while !self.is_at_end() && self.curr_char() != '\n' {
            comment.push(self.curr_char());
            self.advance();
        }

        Token::new(TokenKind::Comment(comment.trim().to_string()), line, column)
    }

    fn read_string(&mut self, quote: char) -> Token {
        let line = self.line;
        let column = self.column;

        self.advance(); // Skip opening quote

        let mut string = String::new();
        while !self.is_at_end() && self.curr_char() != quote {
            if self.curr_char() == '\\' {
                self.advance();
                if !self.is_at_end() {
                    // Handle escape sequences
                    let escaped = match self.curr_char() {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '"' => '"',
                        '\'' => '\'',
                        c => c,
                    };
                    string.push(escaped);
                    self.advance();
                }
            } else {
                string.push(self.curr_char());
                self.advance();
            }
        }

        if !self.is_at_end() {
            self.advance(); // Skip closing quote
        }

        Token::new(TokenKind::String(string), line, column)
    }

    fn read_num(&mut self) -> Token {
        let line = self.line;
        let column = self.column;

        let mut number = String::new();

        while !self.is_at_end() && (self.curr_char().is_ascii_digit() || self.curr_char() == '.') {
            number.push(self.curr_char());
            self.advance();
        }

        // Handle scientific notation (1e5, 1.5e-3)
        if !self.is_at_end() && (self.curr_char() == 'e' || self.curr_char() == 'E') {
            number.push(self.curr_char());
            self.advance();

            if !self.is_at_end() && (self.curr_char() == '+' || self.curr_char() == '-') {
                number.push(self.curr_char());
                self.advance();
            }

            while !self.is_at_end() && self.curr_char().is_ascii_digit() {
                number.push(self.curr_char());
                self.advance();
            }
        }

        let value = number.parse::<f64>().unwrap_or(0.0);
        Token::new(TokenKind::Number(value), line, column)
    }

    fn read_identifier(&mut self) -> Token {
        let line = self.line;
        let column = self.column;

        let mut ident = String::new();

        while !self.is_at_end()
            && (self.curr_char().is_alphanumeric()
                || self.curr_char() == '_'
                || self.curr_char() == '.')
        {
            ident.push(self.curr_char());
            self.advance();
        }

        // Check for keywords and built-in variables
        let kind = match ident.as_str() {
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "var" => TokenKind::Var,
            "varip" => TokenKind::Varip,
            "open" => TokenKind::Open,
            "high" => TokenKind::High,
            "low" => TokenKind::Low,
            "close" => TokenKind::Close,
            "volume" => TokenKind::Volume,
            "time" => TokenKind::Time,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            _ => TokenKind::Identifier(ident),
        };

        Token::new(kind, line, column)
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            let ch = self.curr_char();
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn curr_char(&self) -> char {
        self.input[self.position]
    }

    fn peek_char(&self) -> Option<char> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.position += 1;
            self.column += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Number(n) => write!(f, "Number({n})"),
            TokenKind::String(s) => write!(f, "String(\"{s}\")"),
            TokenKind::Boolean(b) => write!(f, "Boolean({b})"),
            TokenKind::Identifier(id) => write!(f, "Identifier({id})"),
            TokenKind::Comment(c) => write!(f, "Comment({c})"),
            _ => write!(f, "{self:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_numbers() {
        let mut lexer = Lexer::new("42 3.25 1e5 2.5e-3");
        let tokens = lexer.tokenize();

        assert!(matches!(tokens[0].kind, TokenKind::Number(n) if n == 42.0));
        assert!(matches!(tokens[1].kind, TokenKind::Number(n) if n == 3.25));
        assert!(matches!(tokens[2].kind, TokenKind::Number(n) if n == 1e5));
        assert!(matches!(tokens[3].kind, TokenKind::Number(n) if (n - 2.5e-3).abs() < 1e-10));
    }

    #[test]
    fn test_tokenize_strings() {
        let mut lexer = Lexer::new(r#""Hello" 'World'"#);
        let tokens = lexer.tokenize();

        assert!(matches!(&tokens[0].kind, TokenKind::String(s) if s == "Hello"));
        assert!(matches!(&tokens[1].kind, TokenKind::String(s) if s == "World"));
    }

    #[test]
    fn test_tokenize_identifiers() {
        let mut lexer = Lexer::new("length ta.sma close");
        let tokens = lexer.tokenize();

        assert!(matches!(&tokens[0].kind, TokenKind::Identifier(s) if s == "length"));
        assert!(matches!(&tokens[1].kind, TokenKind::Identifier(s) if s == "ta.sma"));
        assert!(matches!(tokens[2].kind, TokenKind::Close));
    }

    #[test]
    fn test_tokenize_operators() {
        let mut lexer = Lexer::new("+ - * / == != <= >=");
        let tokens = lexer.tokenize();

        assert!(matches!(tokens[0].kind, TokenKind::Plus));
        assert!(matches!(tokens[1].kind, TokenKind::Minus));
        assert!(matches!(tokens[2].kind, TokenKind::Star));
        assert!(matches!(tokens[3].kind, TokenKind::Slash));
        assert!(matches!(tokens[4].kind, TokenKind::EqualEqual));
        assert!(matches!(tokens[5].kind, TokenKind::NotEqual));
        assert!(matches!(tokens[6].kind, TokenKind::LessEqual));
        assert!(matches!(tokens[7].kind, TokenKind::GreaterEqual));
    }

    #[test]
    fn test_tokenize_comment() {
        let mut lexer = Lexer::new("// This is a comment\nclose");
        let tokens = lexer.tokenize();

        assert!(matches!(&tokens[0].kind, TokenKind::Comment(c) if c == "This is a comment"));
        assert!(matches!(tokens[1].kind, TokenKind::Newline));
        assert!(matches!(tokens[2].kind, TokenKind::Close));
    }
}
