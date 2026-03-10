/// Abstract Syntax Tree for Pine Script
use std::fmt;

/// Pine Script program - collection of statements
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

impl Program {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

/// Statement types in Pine Script
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Variable declaration: var x = expr
    VarDeclaration {
        name: String,
        value: Box<Expr>,
        is_varip: bool, // varip for intrabar persistence
    },

    /// Assignment: x = expr OR x := expr (reassignment)
    Assignment { name: String, value: Box<Expr> },

    /// Expression statement
    Expression(Expr),

    /// If statement: if condition ... else ...
    If {
        condition: Box<Expr>,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },

    /// For loop: for i = start to end ...
    For {
        variable: String,
        start: Box<Expr>,
        end: Box<Expr>,
        body: Vec<Stmt>,
    },

    /// While loop: while condition ...
    While {
        condition: Box<Expr>,
        body: Vec<Stmt>,
    },
}

/// Expression types in Pine Script
#[derive(Debug, Clone)]
pub enum Expr {
    /// Numeric literal
    Number(f64),

    /// String literal
    StringLiteral(String),

    /// Boolean literal
    Boolean(bool),

    /// Variable reference
    Variable(String),

    /// Built-in variable (open, high, low, close, volume, time)
    BuiltinVariable(BuiltinVar),

    /// Binary operation
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },

    /// Unary operation
    Unary { op: UnaryOp, expr: Box<Expr> },

    /// Function call
    FunctionCall { name: String, args: Vec<Expr> },

    /// Method call (e.g., ta.sma)
    MethodCall {
        namespace: String,
        method: String,
        args: Vec<Expr>,
    },

    /// Ternary operator: condition ? true_expr : false_expr
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },

    /// Array access: `array[index]`
    ArrayAccess { array: Box<Expr>, index: Box<Expr> },

    /// Series offset: `close[1]` (previous bar)
    SeriesOffset { series: Box<Expr>, offset: i32 },

    /// Array literal: [1, 2, 3] or array.new_float(size, initial_val)
    ArrayLiteral(Vec<Expr>),
}

/// Built-in variables in Pine Script
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinVar {
    Open,
    High,
    Low,
    Close,
    Volume,
    Time,
}

impl fmt::Display for BuiltinVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuiltinVar::Open => write!(f, "open"),
            BuiltinVar::High => write!(f, "high"),
            BuiltinVar::Low => write!(f, "low"),
            BuiltinVar::Close => write!(f, "close"),
            BuiltinVar::Volume => write!(f, "volume"),
            BuiltinVar::Time => write!(f, "time"),
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
        };
        write!(f, "{op}")
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self {
            UnaryOp::Negate => "-",
            UnaryOp::Not => "not",
        };
        write!(f, "{op}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_var_display() {
        assert_eq!(format!("{}", BuiltinVar::Close), "close");
        assert_eq!(format!("{}", BuiltinVar::Volume), "volume");
    }

    #[test]
    fn test_binary_op_display() {
        assert_eq!(format!("{}", BinaryOp::Add), "+");
        assert_eq!(format!("{}", BinaryOp::Equal), "==");
    }
}
