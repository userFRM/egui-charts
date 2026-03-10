/// Pine Script parser - converts tokens into AST
use super::ast::*;
use super::lexer::{Lexer, Token, TokenKind};
use std::fmt;

#[derive(Debug, Clone)]
pub enum ParseError {
    UnexpectedToken {
        expected: String,
        found: TokenKind,
        line: usize,
        column: usize,
    },
    UnexpectedEof,
    InvalidSyntax(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken {
                expected,
                found,
                line,
                column,
            } => write!(
                f,
                "Parse error at {line}:{column}: expected {expected}, found {found}"
            ),
            ParseError::UnexpectedEof => write!(f, "Unexpected end of file"),
            ParseError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let tokens = lexer.tokenize();
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut program = Program::new();

        while !self.is_at_end() {
            // Skip comments and newlines at top level
            if matches!(
                self.curr_token().kind,
                TokenKind::Comment(_) | TokenKind::Newline
            ) {
                self.advance();
                continue;
            }

            let stmt = self.parse_statement()?;
            program.statements.push(stmt);

            // Consume optional newline after statement
            if matches!(self.curr_token().kind, TokenKind::Newline) {
                self.advance();
            }
        }

        Ok(program)
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        // Skip newlines and comments
        while matches!(
            self.curr_token().kind,
            TokenKind::Newline | TokenKind::Comment(_)
        ) {
            self.advance();
        }

        match &self.curr_token().kind {
            TokenKind::Var | TokenKind::Varip => self.parse_var_declaration(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Identifier(_) => {
                // Could be assignment or expression
                let checkpoint = self.position;

                // Try to parse as assignment
                if let TokenKind::Identifier(name) = &self.curr_token().kind {
                    let name = name.clone();
                    self.advance();

                    if matches!(self.curr_token().kind, TokenKind::Equal) {
                        self.advance();
                        let value = Box::new(self.parse_expression()?);
                        return Ok(Stmt::Assignment { name, value });
                    }
                }

                // Not an assignment, restore position and parse as expression
                self.position = checkpoint;
                let expr = self.parse_expression()?;
                Ok(Stmt::Expression(expr))
            }
            _ => {
                let expr = self.parse_expression()?;
                Ok(Stmt::Expression(expr))
            }
        }
    }

    fn parse_var_declaration(&mut self) -> Result<Stmt, ParseError> {
        let is_varip = matches!(self.curr_token().kind, TokenKind::Varip);
        self.advance(); // Skip var or varip

        let name = self.expect_identifier()?;
        self.expect(TokenKind::Equal)?;

        let value = Box::new(self.parse_expression()?);

        Ok(Stmt::VarDeclaration {
            name,
            value,
            is_varip,
        })
    }

    fn parse_if_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // Skip 'if'

        let condition = Box::new(self.parse_expression()?);

        // Parse then branch (could be single statement or block)
        let mut then_branch = Vec::new();
        if matches!(self.curr_token().kind, TokenKind::Newline) {
            self.advance();
        }
        then_branch.push(self.parse_statement()?);

        // Parse optional else branch
        let else_branch = if matches!(self.curr_token().kind, TokenKind::Else) {
            self.advance();
            if matches!(self.curr_token().kind, TokenKind::Newline) {
                self.advance();
            }
            let else_stmts = vec![self.parse_statement()?];
            Some(else_stmts)
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // Skip 'for'

        let variable = self.expect_identifier()?;
        self.expect(TokenKind::Equal)?;

        let start = Box::new(self.parse_expression()?);

        // Expect 'to' keyword (represented as identifier)
        if let TokenKind::Identifier(id) = &self.curr_token().kind {
            if id != "to" {
                return Err(ParseError::InvalidSyntax(
                    "Expected 'to' in for loop".to_string(),
                ));
            }
            self.advance();
        } else {
            return Err(ParseError::InvalidSyntax(
                "Expected 'to' in for loop".to_string(),
            ));
        }

        let end = Box::new(self.parse_expression()?);

        if matches!(self.curr_token().kind, TokenKind::Newline) {
            self.advance();
        }

        let body = vec![self.parse_statement()?];

        Ok(Stmt::For {
            variable,
            start,
            end,
            body,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // Skip 'while'

        let condition = Box::new(self.parse_expression()?);

        if matches!(self.curr_token().kind, TokenKind::Newline) {
            self.advance();
        }

        let body = vec![self.parse_statement()?];

        Ok(Stmt::While { condition, body })
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_logical_or()?;

        if matches!(self.curr_token().kind, TokenKind::Question) {
            self.advance();
            let then_expr = Box::new(self.parse_expression()?);
            self.expect(TokenKind::Colon)?;
            let else_expr = Box::new(self.parse_expression()?);

            expr = Expr::Ternary {
                condition: Box::new(expr),
                then_expr,
                else_expr,
            };
        }

        Ok(expr)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logical_and()?;

        while matches!(self.curr_token().kind, TokenKind::Or) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;

        while matches!(self.curr_token().kind, TokenKind::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;

        while matches!(
            self.curr_token().kind,
            TokenKind::EqualEqual | TokenKind::NotEqual
        ) {
            let op = match self.curr_token().kind {
                TokenKind::EqualEqual => BinaryOp::Equal,
                TokenKind::NotEqual => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            self.advance();

            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;

        while matches!(
            self.curr_token().kind,
            TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual
        ) {
            let op = match self.curr_token().kind {
                TokenKind::Less => BinaryOp::Less,
                TokenKind::LessEqual => BinaryOp::LessEqual,
                TokenKind::Greater => BinaryOp::Greater,
                TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
                _ => unreachable!(),
            };
            self.advance();

            let right = self.parse_addition()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;

        while matches!(self.curr_token().kind, TokenKind::Plus | TokenKind::Minus) {
            let op = match self.curr_token().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                _ => unreachable!(),
            };
            self.advance();

            let right = self.parse_multiplication()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;

        while matches!(
            self.curr_token().kind,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent
        ) {
            let op = match self.curr_token().kind {
                TokenKind::Star => BinaryOp::Multiply,
                TokenKind::Slash => BinaryOp::Divide,
                TokenKind::Percent => BinaryOp::Modulo,
                _ => unreachable!(),
            };
            self.advance();

            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match &self.curr_token().kind {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match &self.curr_token().kind {
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expression()?;
                    self.expect(TokenKind::RightBracket)?;

                    // Check if this is a series offset (simple integer)
                    if let Expr::Number(n) = index {
                        expr = Expr::SeriesOffset {
                            series: Box::new(expr),
                            offset: n as i32,
                        };
                    } else {
                        expr = Expr::ArrayAccess {
                            array: Box::new(expr),
                            index: Box::new(index),
                        };
                    }
                }
                TokenKind::Dot => {
                    self.advance();
                    let method = self.expect_identifier()?;

                    if matches!(self.curr_token().kind, TokenKind::LeftParen) {
                        // Method call
                        self.advance();
                        let args = self.parse_arguments()?;
                        self.expect(TokenKind::RightParen)?;

                        if let Expr::Variable(namespace) = expr {
                            expr = Expr::MethodCall {
                                namespace,
                                method,
                                args,
                            };
                        } else {
                            return Err(ParseError::InvalidSyntax(
                                "Method calls require a namespace".to_string(),
                            ));
                        }
                    } else {
                        // Property access (not supported yet, treat as variable)
                        return Err(ParseError::InvalidSyntax(
                            "Property access not supported yet".to_string(),
                        ));
                    }
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match &self.curr_token().kind.clone() {
            TokenKind::Number(n) => {
                let num = *n;
                self.advance();
                Ok(Expr::Number(num))
            }
            TokenKind::String(s) => {
                let string = s.clone();
                self.advance();
                Ok(Expr::StringLiteral(string))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Boolean(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Boolean(false))
            }
            TokenKind::Open => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::Open))
            }
            TokenKind::High => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::High))
            }
            TokenKind::Low => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::Low))
            }
            TokenKind::Close => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::Close))
            }
            TokenKind::Volume => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::Volume))
            }
            TokenKind::Time => {
                self.advance();
                Ok(Expr::BuiltinVariable(BuiltinVar::Time))
            }
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();

                // Check if this is a function call
                if matches!(self.curr_token().kind, TokenKind::LeftParen) {
                    self.advance();
                    let args = self.parse_arguments()?;
                    self.expect(TokenKind::RightParen)?;

                    // Check if this is a method call (namespace.method)
                    if let Some(dot_pos) = name.rfind('.') {
                        let namespace = name[..dot_pos].to_string();
                        let method = name[dot_pos + 1..].to_string();
                        Ok(Expr::MethodCall {
                            namespace,
                            method,
                            args,
                        })
                    } else {
                        Ok(Expr::FunctionCall { name, args })
                    }
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RightParen)?;
                Ok(expr)
            }
            _ => {
                let token = self.curr_token();
                Err(ParseError::UnexpectedToken {
                    expected: "expression".to_string(),
                    found: token.kind.clone(),
                    line: token.line,
                    column: token.column,
                })
            }
        }
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();

        if matches!(self.curr_token().kind, TokenKind::RightParen) {
            return Ok(args);
        }

        loop {
            // Check if this is a named argument (identifier = expr)
            if matches!(self.curr_token().kind, TokenKind::Identifier(_)) {
                // Peek ahead to see if there's an = sign
                let saved_pos = self.position;
                self.advance();

                if matches!(self.curr_token().kind, TokenKind::Equal) {
                    // This is a named argument, skip it
                    self.advance(); // Skip '='
                    self.parse_expression()?; // Parse and discard the value

                    if matches!(self.curr_token().kind, TokenKind::Comma) {
                        self.advance();
                        continue;
                    } else {
                        break;
                    }
                } else {
                    // Not a named argument, restore position and parse normally
                    self.position = saved_pos;
                }
            }

            args.push(self.parse_expression()?);

            if !matches!(self.curr_token().kind, TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(args)
    }

    fn expect(&mut self, expected: TokenKind) -> Result<(), ParseError> {
        let current = self.curr_token();
        if std::mem::discriminant(&current.kind) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: format!("{expected:?}"),
                found: current.kind.clone(),
                line: current.line,
                column: current.column,
            })
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        let current = self.curr_token();
        if let TokenKind::Identifier(name) = &current.kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: current.kind.clone(),
                line: current.line,
                column: current.column,
            })
        }
    }

    fn curr_token(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.position += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.curr_token().kind, TokenKind::Eof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_assignment() {
        let code = "length = 20";
        let lexer = Lexer::new(code);
        let mut parser = Parser::new(lexer);
        let program = parser.parse().unwrap();

        assert_eq!(program.statements.len(), 1);
        assert!(matches!(program.statements[0], Stmt::Assignment { .. }));
    }

    #[test]
    fn test_parse_function_call() {
        let code = "plot(close)";
        let lexer = Lexer::new(code);
        let mut parser = Parser::new(lexer);
        let program = parser.parse().unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Stmt::Expression(Expr::FunctionCall { name, args }) = &program.statements[0] {
            assert_eq!(name, "plot");
            assert_eq!(args.len(), 1);
        } else {
            panic!("Expected function call");
        }
    }

    #[test]
    fn test_parse_method_call() {
        let code = "ta.sma(close, 20)";
        let lexer = Lexer::new(code);
        let mut parser = Parser::new(lexer);
        let program = parser.parse().unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Stmt::Expression(Expr::MethodCall {
            namespace,
            method,
            args,
        }) = &program.statements[0]
        {
            assert_eq!(namespace, "ta");
            assert_eq!(method, "sma");
            assert_eq!(args.len(), 2);
        } else {
            panic!("Expected method call");
        }
    }

    #[test]
    fn test_parse_binary_expression() {
        let code = "close > open";
        let lexer = Lexer::new(code);
        let mut parser = Parser::new(lexer);
        let program = parser.parse().unwrap();

        assert_eq!(program.statements.len(), 1);
        if let Stmt::Expression(Expr::Binary { op, .. }) = &program.statements[0] {
            assert_eq!(*op, BinaryOp::Greater);
        } else {
            panic!("Expected binary expression");
        }
    }
}
