//! Function and method calling for Pine Script runtime
//!
//! Handles call_function, call_method, and call_array_method.

use super::Runtime;
use crate::scripting::ast::Expr;
use crate::scripting::types::{RuntimeError, Value};

impl Runtime {
    /// Call a function by name
    pub(crate) fn call_function(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
        // Evaluate arguments
        let mut arg_values = Vec::new();
        for arg in args {
            arg_values.push(self.eval_expr(arg)?);
        }

        // Look up built-in function
        if let Some(func) = self.builtins.get(name).cloned() {
            return func(&arg_values, &self.context);
        }

        // Handle special functions
        match name {
            "plot" => {
                // plot() function - handled separately
                Ok(Value::Null)
            }
            "indicator" | "strategy" => {
                // Metadata functions - do nothing during execution
                Ok(Value::Null)
            }
            "na" => {
                // Check if value is NaN
                if let Some(val) = arg_values.first() {
                    match val {
                        Value::Number(n) => Ok(Value::Boolean(n.is_nan())),
                        Value::Null => Ok(Value::Boolean(true)),
                        _ => Ok(Value::Boolean(false)),
                    }
                } else {
                    Ok(Value::Number(f64::NAN))
                }
            }
            "nz" => {
                // Replace NaN with default
                let val = arg_values.first().cloned().unwrap_or(Value::Null);
                let default = arg_values.get(1).cloned().unwrap_or(Value::Number(0.0));

                if let Value::Number(n) = &val {
                    if n.is_nan() { Ok(default) } else { Ok(val) }
                } else {
                    Ok(val)
                }
            }
            "fixnan" => {
                // Forward fill NaN values
                if let Some(Value::Number(n)) = arg_values.first() {
                    if n.is_nan() {
                        Ok(Value::Number(0.0)) // Should carry forward, simplified
                    } else {
                        Ok(Value::Number(*n))
                    }
                } else {
                    Ok(Value::Number(0.0))
                }
            }
            _ => Err(RuntimeError::FunctionNotFound(name.to_string())),
        }
    }

    /// Call a method on a namespace (e.g., ta.sma, array.push)
    pub(crate) fn call_method(
        &mut self,
        namespace: &str,
        method: &str,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
        // Evaluate arguments
        let mut arg_values = Vec::new();
        for arg in args {
            arg_values.push(self.eval_expr(arg)?);
        }

        // Handle array.* methods
        if namespace == "array" {
            return self.call_array_method(method, &arg_values);
        }

        // Handle strategy.* methods
        if namespace == "strategy" {
            return self.call_strategy_method(method, &arg_values);
        }

        // Handle namespaced functions (ta.sma, ta.ema, etc.)
        let full_name = format!("{namespace}.{method}");

        // Look up in builtins
        if let Some(func) = self.builtins.get(&full_name).cloned() {
            return func(&arg_values, &self.context);
        }

        Err(RuntimeError::FunctionNotFound(full_name))
    }

    /// Handle array.* methods for Pine Script array support
    pub(crate) fn call_array_method(
        &mut self,
        method: &str,
        args: &[Value],
    ) -> Result<Value, RuntimeError> {
        match method {
            // array.new_float(size, initial_val)
            "new_float" | "new" => {
                let size = args.first().and_then(|v| v.as_num().ok()).unwrap_or(0.0) as usize;
                let initial = args
                    .get(1)
                    .and_then(|v| v.as_num().ok())
                    .unwrap_or(f64::NAN);

                let arr: Vec<Value> = vec![Value::Number(initial); size];
                Ok(Value::Array(arr))
            }

            // array.new_bool(size, initial_val)
            "new_bool" => {
                let size = args.first().and_then(|v| v.as_num().ok()).unwrap_or(0.0) as usize;
                let initial = args
                    .get(1)
                    .and_then(|v| v.as_boolean().ok())
                    .unwrap_or(false);

                let arr: Vec<Value> = vec![Value::Boolean(initial); size];
                Ok(Value::Array(arr))
            }

            // array.new_string(size, initial_val)
            "new_string" => {
                let size = args.first().and_then(|v| v.as_num().ok()).unwrap_or(0.0) as usize;
                let initial = if let Some(Value::String(s)) = args.get(1) {
                    s.clone()
                } else {
                    String::new()
                };

                let arr: Vec<Value> = vec![Value::String(initial); size];
                Ok(Value::Array(arr))
            }

            // array.size(arr)
            "size" => {
                if let Some(arr) = args.first() {
                    match arr {
                        Value::Array(a) => Ok(Value::Number(a.len() as f64)),
                        _ => Err(RuntimeError::TypeError(
                            "array.size requires an array".to_string(),
                        )),
                    }
                } else {
                    Ok(Value::Number(0.0))
                }
            }

            // array.get(arr, index)
            "get" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.get requires array and index".to_string())
                })?;
                let idx = args.get(1).and_then(|v| v.as_num().ok()).ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.get requires index".to_string())
                })? as usize;

                match arr {
                    Value::Array(a) => {
                        if idx < a.len() {
                            Ok(a[idx].clone())
                        } else {
                            Err(RuntimeError::IndexOutOfBounds)
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.get requires an array".to_string(),
                    )),
                }
            }

            // array.push(arr, value)
            "push" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments(
                        "array.push requires array and value".to_string(),
                    )
                })?;
                let value = args.get(1).cloned().unwrap_or(Value::Null);

                match arr {
                    Value::Array(a) => {
                        let mut new_arr = a.clone();
                        new_arr.push(value);
                        Ok(Value::Array(new_arr))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.push requires an array".to_string(),
                    )),
                }
            }

            // array.pop(arr)
            "pop" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.pop requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        if let Some(val) = a.last() {
                            Ok(val.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.pop requires an array".to_string(),
                    )),
                }
            }

            // array.sum(arr)
            "sum" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.sum requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let sum: f64 = a.iter().filter_map(|v| v.as_num().ok()).sum();
                        Ok(Value::Number(sum))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.sum requires an array".to_string(),
                    )),
                }
            }

            // array.avg(arr)
            "avg" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.avg requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let nums: Vec<f64> = a.iter().filter_map(|v| v.as_num().ok()).collect();
                        if nums.is_empty() {
                            Ok(Value::Number(f64::NAN))
                        } else {
                            let sum: f64 = nums.iter().sum();
                            Ok(Value::Number(sum / nums.len() as f64))
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.avg requires an array".to_string(),
                    )),
                }
            }

            // array.max(arr)
            "max" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.max requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let max = a
                            .iter()
                            .filter_map(|v| v.as_num().ok())
                            .fold(f64::NEG_INFINITY, f64::max);
                        Ok(Value::Number(max))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.max requires an array".to_string(),
                    )),
                }
            }

            // array.min(arr)
            "min" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.min requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let min = a
                            .iter()
                            .filter_map(|v| v.as_num().ok())
                            .fold(f64::INFINITY, f64::min);
                        Ok(Value::Number(min))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.min requires an array".to_string(),
                    )),
                }
            }

            // array.stdev(arr)
            "stdev" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.stdev requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let nums: Vec<f64> = a.iter().filter_map(|v| v.as_num().ok()).collect();
                        if nums.len() < 2 {
                            Ok(Value::Number(0.0))
                        } else {
                            let mean: f64 = nums.iter().sum::<f64>() / nums.len() as f64;
                            let variance: f64 =
                                nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                                    / nums.len() as f64;
                            Ok(Value::Number(variance.sqrt()))
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.stdev requires an array".to_string(),
                    )),
                }
            }

            // array.first(arr)
            "first" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.first requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        if let Some(val) = a.first() {
                            Ok(val.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.first requires an array".to_string(),
                    )),
                }
            }

            // array.last(arr)
            "last" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.last requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        if let Some(val) = a.last() {
                            Ok(val.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.last requires an array".to_string(),
                    )),
                }
            }

            // array.from(...values)
            "from" => Ok(Value::Array(args.to_vec())),

            // array.copy(arr)
            "copy" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.copy requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => Ok(Value::Array(a.clone())),
                    _ => Err(RuntimeError::TypeError(
                        "array.copy requires an array".to_string(),
                    )),
                }
            }

            // array.clear(arr)
            "clear" => Ok(Value::Array(Vec::new())),

            // array.includes(arr, value) / array.indexof(arr, value)
            "includes" | "indexof" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments(
                        "array.includes requires array and value".to_string(),
                    )
                })?;
                let search_val = args.get(1);

                match (arr, search_val) {
                    (Value::Array(a), Some(val)) => {
                        for (idx, item) in a.iter().enumerate() {
                            // Simple equality check for numbers
                            if let (Value::Number(n1), Value::Number(n2)) = (item, val)
                                && (n1 - n2).abs() < f64::EPSILON
                            {
                                return if method == "includes" {
                                    Ok(Value::Boolean(true))
                                } else {
                                    Ok(Value::Number(idx as f64))
                                };
                            }
                        }
                        if method == "includes" {
                            Ok(Value::Boolean(false))
                        } else {
                            Ok(Value::Number(-1.0))
                        }
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.includes requires array and value".to_string(),
                    )),
                }
            }

            // array.fill(arr, value)
            "fill" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments(
                        "array.fill requires array and value".to_string(),
                    )
                })?;
                let fill_val = args.get(1).cloned().unwrap_or(Value::Number(0.0));

                match arr {
                    Value::Array(a) => {
                        let new_arr: Vec<Value> = vec![fill_val; a.len()];
                        Ok(Value::Array(new_arr))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.fill requires an array".to_string(),
                    )),
                }
            }

            // array.slice(arr, start, end)
            "slice" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.slice requires array".to_string())
                })?;
                let start = args.get(1).and_then(|v| v.as_num().ok()).unwrap_or(0.0) as usize;
                let end = args.get(2).and_then(|v| v.as_num().ok());

                match arr {
                    Value::Array(a) => {
                        let end_idx = end.map(|e| e as usize).unwrap_or(a.len());
                        let end_idx = end_idx.min(a.len());
                        let start_idx = start.min(end_idx);
                        Ok(Value::Array(a[start_idx..end_idx].to_vec()))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.slice requires an array".to_string(),
                    )),
                }
            }

            // array.reverse(arr)
            "reverse" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.reverse requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let mut reversed = a.clone();
                        reversed.reverse();
                        Ok(Value::Array(reversed))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.reverse requires an array".to_string(),
                    )),
                }
            }

            // array.sort(arr)
            "sort" => {
                let arr = args.first().ok_or_else(|| {
                    RuntimeError::InvalidArguments("array.sort requires array".to_string())
                })?;

                match arr {
                    Value::Array(a) => {
                        let mut nums: Vec<f64> = a.iter().filter_map(|v| v.as_num().ok()).collect();
                        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        Ok(Value::Array(nums.into_iter().map(Value::Number).collect()))
                    }
                    _ => Err(RuntimeError::TypeError(
                        "array.sort requires an array".to_string(),
                    )),
                }
            }

            _ => Err(RuntimeError::FunctionNotFound(format!("array.{method}"))),
        }
    }
}
