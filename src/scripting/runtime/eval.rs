//! Expression evaluation for Pine Script runtime
//!
//! Contains methods for evaluating expressions, binary operations, and unary operations.

use super::Runtime;
use crate::scripting::ast::{BinaryOp, BuiltinVar, Expr, UnaryOp};
use crate::scripting::types::{RuntimeError, Value};

impl Runtime {
    /// Evaluate an expression and return its value
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::StringLiteral(s) => Ok(Value::String(s.clone())),
            Expr::Boolean(b) => Ok(Value::Boolean(*b)),

            Expr::Variable(name) => {
                // Check persistent vars first, then regular vars
                if let Some(val) = self.persistent_vars.get(name) {
                    return Ok(val.clone());
                }
                if let Some(val) = self.variables.get(name) {
                    return Ok(val.clone());
                }
                // Check if it's a series reference
                if let Some(series) = self.context.get_series(name)
                    && self.context.curr_bar < series.len()
                {
                    return Ok(Value::Number(series[self.context.curr_bar]));
                }
                Err(RuntimeError::UndefinedVariable(name.clone()))
            }

            Expr::BuiltinVariable(var) => {
                if self.context.curr_bar >= self.context.bars.len() {
                    return Ok(Value::Number(f64::NAN));
                }

                let bar = &self.context.bars[self.context.curr_bar];
                let value = match var {
                    BuiltinVar::Open => bar.open,
                    BuiltinVar::High => bar.high,
                    BuiltinVar::Low => bar.low,
                    BuiltinVar::Close => bar.close,
                    BuiltinVar::Volume => bar.volume,
                    BuiltinVar::Time => bar.time.timestamp() as f64,
                };
                Ok(Value::Number(value))
            }

            Expr::Binary { left, op, right } => {
                let left_val = self.eval_expr(left)?;
                let right_val = self.eval_expr(right)?;
                self.eval_binary_op(&left_val, *op, &right_val)
            }

            Expr::Unary { op, expr } => {
                let val = self.eval_expr(expr)?;
                self.eval_unary_op(*op, &val)
            }

            Expr::FunctionCall { name, args } => self.call_function(name, args),

            Expr::MethodCall {
                namespace,
                method,
                args,
            } => self.call_method(namespace, method, args),

            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond = self.eval_expr(condition)?;
                if cond.as_boolean()? {
                    self.eval_expr(then_expr)
                } else {
                    self.eval_expr(else_expr)
                }
            }

            Expr::ArrayLiteral(elements) => {
                let mut values = Vec::new();
                for elem in elements {
                    values.push(self.eval_expr(elem)?);
                }
                Ok(Value::Array(values))
            }

            Expr::ArrayAccess { array, index } => {
                let arr = self.eval_expr(array)?;
                let idx = self.eval_expr(index)?.as_num()? as usize;

                match arr {
                    Value::Array(vec) => {
                        vec.get(idx).cloned().ok_or(RuntimeError::IndexOutOfBounds)
                    }
                    Value::String(s) => s
                        .chars()
                        .nth(idx)
                        .map(|c| Value::String(c.to_string()))
                        .ok_or(RuntimeError::IndexOutOfBounds),
                    _ => Err(RuntimeError::TypeError(
                        "Cannot index non-array".to_string(),
                    )),
                }
            }

            Expr::SeriesOffset { series, offset } => {
                // Evaluate the series expression to get the series name
                let series_name = match series.as_ref() {
                    Expr::Variable(name) => name.clone(),
                    Expr::BuiltinVariable(var) => match var {
                        BuiltinVar::Open => "open".to_string(),
                        BuiltinVar::High => "high".to_string(),
                        BuiltinVar::Low => "low".to_string(),
                        BuiltinVar::Close => "close".to_string(),
                        BuiltinVar::Volume => "volume".to_string(),
                        BuiltinVar::Time => "time".to_string(),
                    },
                    _ => {
                        return Err(RuntimeError::TypeError(
                            "Series offset requires a series name".to_string(),
                        ));
                    }
                };
                let offset_val = *offset as usize;
                if let Some(val) = self.context.get_series_val(&series_name, offset_val) {
                    Ok(Value::Number(val))
                } else {
                    Ok(Value::Number(f64::NAN))
                }
            }
        }
    }

    /// Evaluate a binary operation
    pub(crate) fn eval_binary_op(
        &self,
        left: &Value,
        op: BinaryOp,
        right: &Value,
    ) -> Result<Value, RuntimeError> {
        match (left, right) {
            (Value::Number(l), Value::Number(r)) => {
                let result = match op {
                    BinaryOp::Add => l + r,
                    BinaryOp::Subtract => l - r,
                    BinaryOp::Multiply => l * r,
                    BinaryOp::Divide => {
                        if *r == 0.0 {
                            return Err(RuntimeError::DivisionByZero);
                        }
                        l / r
                    }
                    BinaryOp::Modulo => l % r,
                    BinaryOp::Equal => return Ok(Value::Boolean((l - r).abs() < f64::EPSILON)),
                    BinaryOp::NotEqual => return Ok(Value::Boolean((l - r).abs() >= f64::EPSILON)),
                    BinaryOp::Less => return Ok(Value::Boolean(l < r)),
                    BinaryOp::LessEqual => return Ok(Value::Boolean(l <= r)),
                    BinaryOp::Greater => return Ok(Value::Boolean(l > r)),
                    BinaryOp::GreaterEqual => return Ok(Value::Boolean(l >= r)),
                    BinaryOp::And => return Ok(Value::Boolean(*l != 0.0 && *r != 0.0)),
                    BinaryOp::Or => return Ok(Value::Boolean(*l != 0.0 || *r != 0.0)),
                };
                Ok(Value::Number(result))
            }
            (Value::Boolean(l), Value::Boolean(r)) => {
                let result = match op {
                    BinaryOp::And => *l && *r,
                    BinaryOp::Or => *l || *r,
                    BinaryOp::Equal => *l == *r,
                    BinaryOp::NotEqual => *l != *r,
                    _ => {
                        return Err(RuntimeError::TypeError(
                            "Invalid operation for booleans".to_string(),
                        ));
                    }
                };
                Ok(Value::Boolean(result))
            }
            (Value::String(l), Value::String(r)) => match op {
                BinaryOp::Add => Ok(Value::String(format!("{l}{r}"))),
                BinaryOp::Equal => Ok(Value::Boolean(l == r)),
                BinaryOp::NotEqual => Ok(Value::Boolean(l != r)),
                BinaryOp::Less => Ok(Value::Boolean(l < r)),
                BinaryOp::LessEqual => Ok(Value::Boolean(l <= r)),
                BinaryOp::Greater => Ok(Value::Boolean(l > r)),
                BinaryOp::GreaterEqual => Ok(Value::Boolean(l >= r)),
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot apply {op} to strings"
                ))),
            },
            (Value::String(l), Value::Number(r)) => match op {
                BinaryOp::Add => Ok(Value::String(format!("{l}{r}"))),
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot apply {op} to string and number"
                ))),
            },
            (Value::Number(l), Value::String(r)) => match op {
                BinaryOp::Add => Ok(Value::String(format!("{l}{r}"))),
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot apply {op} to number and string"
                ))),
            },
            _ => Err(RuntimeError::TypeError(format!(
                "Cannot apply {op} to {left:?} and {right:?}"
            ))),
        }
    }

    /// Evaluate a unary operation
    pub(crate) fn eval_unary_op(&self, op: UnaryOp, val: &Value) -> Result<Value, RuntimeError> {
        match (op, val) {
            (UnaryOp::Negate, Value::Number(n)) => Ok(Value::Number(-n)),
            (UnaryOp::Not, Value::Boolean(b)) => Ok(Value::Boolean(!b)),
            (UnaryOp::Not, Value::Number(n)) => Ok(Value::Boolean(*n == 0.0)),
            _ => Err(RuntimeError::TypeError(format!(
                "Cannot apply {op} to {val:?}"
            ))),
        }
    }
}
