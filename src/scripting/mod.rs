//! Pine Script interpreter module
//!
//! Provides a Pine Script v5-compatible interpreter for creating custom indicators
//! and strategies.
//!
//! # Example Pine Script
//! ```pine
//! //@version=5
//! indicator("My SMA", overlay=true)
//!
//! length = input.int(20, "Length")
//! source = close
//!
//! sma_val = ta.sma(source, length)
//! plot(sma_val, color=color.blue, linewidth=2)
//! ```

mod ast;
mod builtins;
mod lexer;
mod parser;
mod runtime;
mod strategy;
mod types;

pub use ast::{Expr, Program, Stmt};
pub use builtins::register_builtins;
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{ParseError, Parser};
pub use runtime::Runtime;
pub use strategy::{PlotOutput, Pos, StrategyMetrics, StrategyState, Trade};
pub use types::{RuntimeError, TradeDirection, Value};

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};

/// Pine Script indicator wrapper
///
/// Wraps a Pine Script program and provides the Indicator trait implementation
#[derive(Clone)]
pub struct PineScriptIndicator {
    name: String,
    script: String,
    runtime: Runtime,
    values: Vec<IndicatorValue>,
    overlay: bool,
    colors: Vec<egui::Color32>,
    visible: bool,
}

impl PineScriptIndicator {
    /// Create a new Pine Script indicator
    pub fn new(script: String) -> Result<Self, ParseError> {
        // Parse the script
        let lexer = Lexer::new(&script);
        let mut parser = Parser::new(lexer);
        let program = parser.parse()?;

        // Create runtime and register built-in functions
        let mut runtime = Runtime::new();
        register_builtins(&mut runtime);

        // Extract metadata from script (indicator name, overlay, etc.)
        let name = Self::extract_name(&program).unwrap_or_else(|| "Pine Script".to_string());
        let overlay = Self::extract_overlay(&program);

        Ok(Self {
            name,
            script,
            runtime,
            values: Vec::new(),
            overlay,
            colors: vec![egui::Color32::BLUE],
            visible: true,
        })
    }

    /// Extract indicator name from Pine Script
    fn extract_name(program: &Program) -> Option<String> {
        // Look for indicator() or strategy() calls
        for stmt in &program.statements {
            if let Stmt::Expression(expr) = stmt
                && let Expr::FunctionCall { name, args } = expr
                && (name == "indicator" || name == "strategy")
                && let Some(Expr::StringLiteral(title)) = args.first()
            {
                return Some(title.clone());
            }
        }
        None
    }

    /// Extract overlay setting from Pine Script
    fn extract_overlay(program: &Program) -> bool {
        // Look for overlay=true in indicator() call
        for stmt in &program.statements {
            if let Stmt::Expression(expr) = stmt
                && let Expr::FunctionCall { name, args: _ } = expr
                && name == "indicator"
            {
                // TODO(P1): Parse named keyword arguments in indicator() calls
                // (e.g., `indicator("RSI", overlay=true)`). Requires extending the
                // Pine Script parser to handle `name=expr` pairs in FunctionCall args,
                // then checking for the `overlay` key here to return its bool value.
                return true;
            }
        }
        false
    }

    /// Execute the Pine Script with bar data
    pub fn execute(&mut self, bars: &[Bar]) -> Result<(), RuntimeError> {
        self.values.clear();

        // Set up the runtime with bar data
        self.runtime.set_bars(bars);

        // Execute the script
        self.runtime.execute(&self.script)?;

        // Extract plot values from runtime
        self.values = self.runtime.get_plot_values();

        Ok(())
    }
}

impl Indicator for PineScriptIndicator {
    fn name(&self) -> &str {
        &self.name
    }

    fn calculate(&mut self, bars: &[Bar]) {
        if let Err(e) = self.execute(bars) {
            eprintln!("Pine Script error: {e}");
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn is_overlay(&self) -> bool {
        self.overlay
    }

    fn colors(&self) -> Vec<egui::Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<egui::Color32>) {
        self.colors = colors;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn line_cnt(&self) -> usize {
        1
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_sma_script() {
        let script = r#"
//@version=5
indicator("My SMA", overlay=true)

length = 20
sma_val = ta.sma(close, length)
plot(sma_val)
"#;

        let result = PineScriptIndicator::new(script.to_string());
        assert!(result.is_ok());

        let indicator = result.unwrap();
        assert_eq!(indicator.name(), "My SMA");
        assert!(indicator.is_overlay());
    }
}
