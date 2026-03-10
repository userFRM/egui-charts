//! Pine Script type definitions
//!
//! Contains core value types and error definitions for the runtime.

use std::fmt;

/// Runtime error types
#[derive(Debug, Clone)]
pub enum RuntimeError {
    UndefinedVariable(String),
    TypeError(String),
    DivisionByZero,
    IndexOutOfBounds,
    FunctionNotFound(String),
    InvalidArguments(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::UndefinedVariable(name) => write!(f, "Undefined variable: {name}"),
            RuntimeError::TypeError(msg) => write!(f, "Type error: {msg}"),
            RuntimeError::DivisionByZero => write!(f, "Division by zero"),
            RuntimeError::IndexOutOfBounds => write!(f, "Index out of bounds"),
            RuntimeError::FunctionNotFound(name) => write!(f, "Function not found: {name}"),
            RuntimeError::InvalidArguments(msg) => write!(f, "Invalid arguments: {msg}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Pine Script value types
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Series(Vec<f64>),
    Array(Vec<Value>),
    Null,
}

impl Value {
    pub fn as_num(&self) -> Result<f64, RuntimeError> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err(RuntimeError::TypeError(format!(
                "Expected number, got {self:?}"
            ))),
        }
    }

    pub fn as_boolean(&self) -> Result<bool, RuntimeError> {
        match self {
            Value::Boolean(b) => Ok(*b),
            Value::Number(n) => Ok(*n != 0.0),
            _ => Err(RuntimeError::TypeError(format!(
                "Expected boolean, got {self:?}"
            ))),
        }
    }

    pub fn as_series(&self) -> Result<&Vec<f64>, RuntimeError> {
        match self {
            Value::Series(s) => Ok(s),
            _ => Err(RuntimeError::TypeError(format!(
                "Expected series, got {self:?}"
            ))),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<Value>, RuntimeError> {
        match self {
            Value::Array(a) => Ok(a),
            _ => Err(RuntimeError::TypeError(format!(
                "Expected array, got {self:?}"
            ))),
        }
    }

    pub fn as_array_mut(&mut self) -> Result<&mut Vec<Value>, RuntimeError> {
        match self {
            Value::Array(a) => Ok(a),
            _ => Err(RuntimeError::TypeError(format!(
                "Expected array, got {self:?}"
            ))),
        }
    }
}

/// Trade direction for strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradeDirection {
    Long,
    Short,
}
