//! Pine Script runtime - executes parsed AST
//!
//! The runtime is split into several modules:
//! - `series_context`: Bar data and series caching
//! - `eval`: Expression evaluation
//! - `calls`: Function and method calling
//! - `strategy_exec`: Strategy execution for backtesting

mod calls;
mod eval;
mod series_context;
mod strategy_exec;

pub use series_context::SeriesContext;

use crate::model::Bar;
use crate::scripting::ast::{Expr, Program, Stmt};
use crate::scripting::strategy::{PlotOutput, StrategyState};
use crate::scripting::types::{RuntimeError, Value};
use crate::studies::IndicatorValue;
use std::collections::HashMap;

pub type BuiltinFn = fn(&[Value], &SeriesContext) -> Result<Value, RuntimeError>;

#[derive(Clone)]
pub struct Runtime {
    /// Variable bindings (per-bar values)
    pub(crate) variables: HashMap<String, Value>,

    /// Persistent variables (var keyword) - maintained across bars
    pub(crate) persistent_vars: HashMap<String, Value>,

    /// Built-in functions
    pub(crate) builtins: HashMap<String, BuiltinFn>,

    /// Series context
    pub(crate) context: SeriesContext,

    /// Parsed program
    #[allow(dead_code)]
    program: Option<Program>,

    /// Plot outputs
    plots: Vec<PlotOutput>,

    /// Indicator values for output
    indicator_values: Vec<IndicatorValue>,

    /// Strategy state for backtesting
    pub(crate) strategy: StrategyState,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            persistent_vars: HashMap::new(),
            builtins: HashMap::new(),
            context: SeriesContext::new(&[]),
            program: None,
            plots: Vec::new(),
            indicator_values: Vec::new(),
            strategy: StrategyState::default(),
        }
    }

    pub fn set_bars(&mut self, bars: &[Bar]) {
        self.context = SeriesContext::new(bars);
        self.variables.clear();
        // Don't clear persistent_vars - they should survive
        self.plots.clear();
        self.indicator_values.clear();
    }

    pub fn register_builtin(&mut self, name: &str, func: BuiltinFn) {
        self.builtins.insert(name.to_string(), func);
    }

    pub fn get_plot_values(&self) -> Vec<IndicatorValue> {
        self.indicator_values.clone()
    }

    pub fn get_plots(&self) -> &[PlotOutput] {
        &self.plots
    }

    #[allow(dead_code)]
    pub fn get_strategy_state(&self) -> &StrategyState {
        &self.strategy
    }

    #[allow(dead_code)]
    pub fn set_initial_capital(&mut self, capital: f64) {
        self.strategy.initial_capital = capital;
        self.strategy.equity = capital;
    }

    pub fn execute(&mut self, script: &str) -> Result<(), RuntimeError> {
        // Parse the script if not already parsed
        let lexer = crate::scripting::Lexer::new(script);
        let mut parser = crate::scripting::Parser::new(lexer);
        let program = parser
            .parse()
            .map_err(|e| RuntimeError::TypeError(format!("Parse error: {e}")))?;

        let bar_cnt = self.context.bars.len();
        let mut plot_values: Vec<Vec<f64>> = Vec::new();

        // Execute for each bar
        for bar_idx in 0..bar_cnt {
            self.context.curr_bar = bar_idx;

            // Execute each statement
            for stmt in &program.statements {
                self.exec_statement(stmt, &mut plot_values)?;
            }
        }

        // Convert plot values to indicator values
        if !plot_values.is_empty() {
            for i in 0..bar_cnt {
                if let Some(val) = plot_values.first().and_then(|p| p.get(i)) {
                    self.indicator_values.push(IndicatorValue::Single(*val));
                } else {
                    self.indicator_values.push(IndicatorValue::None);
                }
            }
        }

        Ok(())
    }

    fn exec_statement(
        &mut self,
        stmt: &Stmt,
        plot_values: &mut Vec<Vec<f64>>,
    ) -> Result<(), RuntimeError> {
        match stmt {
            Stmt::VarDeclaration {
                name,
                value,
                is_varip: _,
            } => {
                // var/varip declarations only initialize on first bar
                if self.context.curr_bar == 0 || !self.persistent_vars.contains_key(name) {
                    let val = self.eval_expr(value)?;
                    self.persistent_vars.insert(name.clone(), val);
                }
            }

            Stmt::Assignment { name, value } => {
                let val = self.eval_expr(value)?;
                // Check if it's a persistent variable
                if self.persistent_vars.contains_key(name) {
                    self.persistent_vars.insert(name.clone(), val);
                } else {
                    self.variables.insert(name.clone(), val);
                }
            }

            Stmt::Expression(expr) => {
                // Handle special functions
                if let Expr::FunctionCall { name, args } = expr {
                    if name == "plot" {
                        self.handle_plot(args, plot_values)?;
                    } else {
                        self.eval_expr(expr)?;
                    }
                } else {
                    self.eval_expr(expr)?;
                }
            }

            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.eval_expr(condition)?;
                if cond.as_boolean()? {
                    for s in then_branch {
                        self.exec_statement(s, plot_values)?;
                    }
                } else if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        self.exec_statement(s, plot_values)?;
                    }
                }
            }

            Stmt::For {
                variable,
                start,
                end,
                body,
            } => {
                let start_val = self.eval_expr(start)?.as_num()? as i64;
                let end_val = self.eval_expr(end)?.as_num()? as i64;

                for i in start_val..=end_val {
                    self.variables
                        .insert(variable.clone(), Value::Number(i as f64));
                    for s in body {
                        self.exec_statement(s, plot_values)?;
                    }
                }
            }

            Stmt::While { condition, body } => {
                let mut iterations = 0;
                const MAX_ITERATIONS: usize = 10000;

                while self.eval_expr(condition)?.as_boolean()? {
                    iterations += 1;
                    if iterations > MAX_ITERATIONS {
                        return Err(RuntimeError::TypeError(
                            "While loop exceeded max iterations".to_string(),
                        ));
                    }

                    for s in body {
                        self.exec_statement(s, plot_values)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_plot(
        &mut self,
        args: &[Expr],
        plot_values: &mut Vec<Vec<f64>>,
    ) -> Result<(), RuntimeError> {
        if args.is_empty() {
            return Ok(());
        }

        let value = self.eval_expr(&args[0])?;
        let val = value.as_num().unwrap_or(f64::NAN);

        // Ensure we have a plot vector
        if plot_values.is_empty() {
            plot_values.push(Vec::new());
        }

        plot_values[0].push(val);
        Ok(())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripting::ast::BuiltinVar;

    #[test]
    fn test_eval_num() {
        let mut rt = Runtime::new();
        let expr = Expr::Number(42.0);
        let result = rt.eval_expr(&expr).unwrap();
        match result {
            Value::Number(n) => assert!((n - 42.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number"),
        }
    }

    #[test]
    fn test_eval_binary_add() {
        use crate::scripting::ast::BinaryOp;

        let mut rt = Runtime::new();
        let expr = Expr::Binary {
            left: Box::new(Expr::Number(10.0)),
            op: BinaryOp::Add,
            right: Box::new(Expr::Number(5.0)),
        };
        let result = rt.eval_expr(&expr).unwrap();
        match result {
            Value::Number(n) => assert!((n - 15.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number"),
        }
    }

    #[test]
    fn test_series_ctx() {
        use crate::model::Bar;
        use chrono::Utc;

        let bars = vec![
            Bar {
                time: Utc::now(),
                open: 100.0,
                high: 110.0,
                low: 95.0,
                close: 105.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc::now(),
                open: 105.0,
                high: 115.0,
                low: 100.0,
                close: 110.0,
                volume: 1500.0,
            },
        ];

        let ctx = SeriesContext::new(&bars);
        assert_eq!(ctx.get_series_val("close", 0), Some(105.0)); // curr_bar = 0
    }

    #[test]
    fn test_series_offset_expr() {
        use crate::model::Bar;
        use chrono::Utc;

        let bars = vec![
            Bar {
                time: Utc::now(),
                open: 100.0,
                high: 110.0,
                low: 95.0,
                close: 105.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc::now(),
                open: 105.0,
                high: 115.0,
                low: 100.0,
                close: 110.0,
                volume: 1500.0,
            },
        ];

        let mut rt = Runtime::new();
        rt.set_bars(&bars);

        // At bar 1, close[1] should give previous close (105.0)
        rt.context.curr_bar = 1;
        let expr = Expr::SeriesOffset {
            series: Box::new(Expr::BuiltinVariable(BuiltinVar::Close)),
            offset: 1,
        };
        let result = rt.eval_expr(&expr).unwrap();
        match result {
            Value::Number(n) => assert!((n - 105.0).abs() < f64::EPSILON),
            _ => panic!("Expected Number"),
        }
    }

    #[test]
    fn test_eval_string_concat() {
        use crate::scripting::ast::BinaryOp;
        let mut rt = Runtime::new();
        let expr = Expr::Binary {
            left: Box::new(Expr::StringLiteral("hello ".to_string())),
            op: BinaryOp::Add,
            right: Box::new(Expr::StringLiteral("world".to_string())),
        };
        let result = rt.eval_expr(&expr).unwrap();
        assert!(matches!(result, Value::String(ref s) if s == "hello world"));
    }

    #[test]
    fn test_eval_string_comparison() {
        use crate::scripting::ast::BinaryOp;
        let mut rt = Runtime::new();
        let expr = Expr::Binary {
            left: Box::new(Expr::StringLiteral("abc".to_string())),
            op: BinaryOp::Equal,
            right: Box::new(Expr::StringLiteral("abc".to_string())),
        };
        assert!(matches!(rt.eval_expr(&expr).unwrap(), Value::Boolean(true)));
    }

    #[test]
    fn test_eval_ternary() {
        let mut rt = Runtime::new();
        let expr = Expr::Ternary {
            condition: Box::new(Expr::Boolean(true)),
            then_expr: Box::new(Expr::Number(10.0)),
            else_expr: Box::new(Expr::Number(20.0)),
        };
        assert!(matches!(rt.eval_expr(&expr).unwrap(), Value::Number(n) if n == 10.0));
        let expr2 = Expr::Ternary {
            condition: Box::new(Expr::Boolean(false)),
            then_expr: Box::new(Expr::Number(10.0)),
            else_expr: Box::new(Expr::Number(20.0)),
        };
        assert!(matches!(rt.eval_expr(&expr2).unwrap(), Value::Number(n) if n == 20.0));
    }

    #[test]
    fn test_eval_array_literal_and_access() {
        let mut rt = Runtime::new();
        let arr_expr = Expr::ArrayLiteral(vec![
            Expr::Number(10.0),
            Expr::Number(20.0),
            Expr::Number(30.0),
        ]);
        let result = rt.eval_expr(&arr_expr).unwrap();
        assert!(matches!(&result, Value::Array(a) if a.len() == 3));
        let access_expr = Expr::ArrayAccess {
            array: Box::new(Expr::ArrayLiteral(vec![
                Expr::Number(10.0),
                Expr::Number(20.0),
                Expr::Number(30.0),
            ])),
            index: Box::new(Expr::Number(1.0)),
        };
        assert!(matches!(rt.eval_expr(&access_expr).unwrap(), Value::Number(n) if n == 20.0));
    }

    #[test]
    fn test_eval_unary_ops() {
        use crate::scripting::ast::UnaryOp;
        let mut rt = Runtime::new();
        let neg = Expr::Unary {
            op: UnaryOp::Negate,
            expr: Box::new(Expr::Number(5.0)),
        };
        assert!(matches!(rt.eval_expr(&neg).unwrap(), Value::Number(n) if n == -5.0));
        let not = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(Expr::Boolean(true)),
        };
        assert!(matches!(rt.eval_expr(&not).unwrap(), Value::Boolean(false)));
    }

    #[test]
    fn test_eval_all_binary_ops() {
        use crate::scripting::ast::BinaryOp;
        let mut rt = Runtime::new();
        let sub = Expr::Binary {
            left: Box::new(Expr::Number(10.0)),
            op: BinaryOp::Subtract,
            right: Box::new(Expr::Number(3.0)),
        };
        assert!(
            matches!(rt.eval_expr(&sub).unwrap(), Value::Number(n) if (n - 7.0).abs() < f64::EPSILON)
        );
        let mul = Expr::Binary {
            left: Box::new(Expr::Number(4.0)),
            op: BinaryOp::Multiply,
            right: Box::new(Expr::Number(5.0)),
        };
        assert!(
            matches!(rt.eval_expr(&mul).unwrap(), Value::Number(n) if (n - 20.0).abs() < f64::EPSILON)
        );
        let div = Expr::Binary {
            left: Box::new(Expr::Number(15.0)),
            op: BinaryOp::Divide,
            right: Box::new(Expr::Number(3.0)),
        };
        assert!(
            matches!(rt.eval_expr(&div).unwrap(), Value::Number(n) if (n - 5.0).abs() < f64::EPSILON)
        );
        let div_zero = Expr::Binary {
            left: Box::new(Expr::Number(15.0)),
            op: BinaryOp::Divide,
            right: Box::new(Expr::Number(0.0)),
        };
        assert!(rt.eval_expr(&div_zero).is_err());
        let modulo = Expr::Binary {
            left: Box::new(Expr::Number(10.0)),
            op: BinaryOp::Modulo,
            right: Box::new(Expr::Number(3.0)),
        };
        assert!(
            matches!(rt.eval_expr(&modulo).unwrap(), Value::Number(n) if (n - 1.0).abs() < f64::EPSILON)
        );
        let less = Expr::Binary {
            left: Box::new(Expr::Number(3.0)),
            op: BinaryOp::Less,
            right: Box::new(Expr::Number(5.0)),
        };
        assert!(matches!(rt.eval_expr(&less).unwrap(), Value::Boolean(true)));
        let and = Expr::Binary {
            left: Box::new(Expr::Boolean(true)),
            op: BinaryOp::And,
            right: Box::new(Expr::Boolean(false)),
        };
        assert!(matches!(rt.eval_expr(&and).unwrap(), Value::Boolean(false)));
        let or = Expr::Binary {
            left: Box::new(Expr::Boolean(true)),
            op: BinaryOp::Or,
            right: Box::new(Expr::Boolean(false)),
        };
        assert!(matches!(rt.eval_expr(&or).unwrap(), Value::Boolean(true)));
    }
}
