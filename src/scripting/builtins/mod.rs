/// Pine Script built-in functions (ta.*, math.*, str.*, input.*)
///
/// Each category lives in its own submodule:
/// - `ta` — core technical analysis (moving averages, RSI, MACD, BB, ATR, etc.)
/// - `ta_statistical` — statistical TA (variance, median, correlation, linreg, etc.)
/// - `ta_advanced` — pivot detection, momentum, trend, and channel indicators
/// - `math` — math namespace functions (abs, pow, sqrt, log, etc.)
/// - `string` — string namespace functions (contains, split, upper, etc.)
/// - `input` — input namespace functions (int, float, bool, string, source)
mod input;
mod math;
mod string;
mod ta;
mod ta_advanced;
mod ta_statistical;

use super::runtime::SeriesContext;
use super::types::{RuntimeError, Value};

// Re-export all builtin functions for backwards compatibility and test access.
pub(crate) use input::*;
pub(crate) use math::*;
pub(crate) use string::*;
pub(crate) use ta::*;
pub(crate) use ta_advanced::*;
pub(crate) use ta_statistical::*;

/// Register all built-in Pine Script functions
pub fn register_builtins(runtime: &mut super::runtime::Runtime) {
    // Technical Analysis functions (ta namespace)
    runtime.register_builtin("ta.sma", ta_sma);
    runtime.register_builtin("ta.ema", ta_ema);
    runtime.register_builtin("ta.rsi", ta_rsi);
    runtime.register_builtin("ta.macd", ta_macd);
    runtime.register_builtin("ta.bb", ta_bb);
    runtime.register_builtin("ta.atr", ta_atr);
    runtime.register_builtin("ta.stoch", ta_stoch);
    runtime.register_builtin("ta.wma", ta_wma);
    runtime.register_builtin("ta.hma", ta_hma);
    runtime.register_builtin("ta.vwma", ta_vwma);

    // Additional ta functions
    runtime.register_builtin("ta.highest", ta_highest);
    runtime.register_builtin("ta.lowest", ta_lowest);
    runtime.register_builtin("ta.tr", ta_tr);
    runtime.register_builtin("ta.change", ta_change);
    runtime.register_builtin("ta.mom", ta_mom);
    runtime.register_builtin("ta.roc", ta_roc);
    runtime.register_builtin("ta.crossover", ta_crossover);
    runtime.register_builtin("ta.crossunder", ta_crossunder);
    runtime.register_builtin("ta.cum", ta_cum);
    runtime.register_builtin("ta.stdev", ta_stdev);
    runtime.register_builtin("ta.vwap", ta_vwap);

    // Statistical functions (Phase 1.2)
    runtime.register_builtin("ta.variance", ta_variance);
    runtime.register_builtin("ta.median", ta_median);
    runtime.register_builtin("ta.mode", ta_mode);
    runtime.register_builtin("ta.percentrank", ta_percentrank);
    runtime.register_builtin("ta.correlation", ta_correlation);
    runtime.register_builtin("ta.linreg", ta_linreg);

    // Pivot detection
    runtime.register_builtin("ta.pivothigh", ta_pivothigh);
    runtime.register_builtin("ta.pivotlow", ta_pivotlow);

    // Momentum indicators
    runtime.register_builtin("ta.wpr", ta_wpr);
    runtime.register_builtin("ta.cmo", ta_cmo);
    runtime.register_builtin("ta.mfi", ta_mfi);
    runtime.register_builtin("ta.obv", ta_obv);

    // Trend indicators
    runtime.register_builtin("ta.dmi", ta_dmi);
    runtime.register_builtin("ta.adx", ta_adx);
    runtime.register_builtin("ta.sar", ta_sar);

    // Channel indicators
    runtime.register_builtin("ta.kc", ta_kc);
    runtime.register_builtin("ta.dc", ta_dc);

    // Math functions
    runtime.register_builtin("math.abs", math_abs);
    runtime.register_builtin("math.max", math_max);
    runtime.register_builtin("math.min", math_min);
    runtime.register_builtin("math.round", math_round);
    runtime.register_builtin("math.floor", math_floor);
    runtime.register_builtin("math.ceil", math_ceil);
    runtime.register_builtin("math.sqrt", math_sqrt);
    runtime.register_builtin("math.pow", math_pow);
    runtime.register_builtin("math.log", math_log);
    runtime.register_builtin("math.log10", math_log10);
    runtime.register_builtin("math.exp", math_exp);
    runtime.register_builtin("math.sign", math_sign);
    runtime.register_builtin("math.avg", math_avg);
    runtime.register_builtin("math.sum", math_sum);

    // String functions
    runtime.register_builtin("str.contains", str_contains);
    runtime.register_builtin("str.length", str_length);
    runtime.register_builtin("str.replace", str_replace);
    runtime.register_builtin("str.split", str_split);
    runtime.register_builtin("str.tostring", str_tostring);
    runtime.register_builtin("str.tonumber", str_tonumber);
    runtime.register_builtin("str.upper", str_upper);
    runtime.register_builtin("str.lower", str_lower);
    runtime.register_builtin("str.startswith", str_startswith);
    runtime.register_builtin("str.endswith", str_endswith);
    runtime.register_builtin("str.substring", str_substring);
    runtime.register_builtin("str.trim", str_trim);

    // Input functions
    runtime.register_builtin("input.int", input_int);
    runtime.register_builtin("input.float", input_float);
    runtime.register_builtin("input.bool", input_bool);
    runtime.register_builtin("input.string", input_string);
    runtime.register_builtin("input.source", input_src);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;
    use chrono::Utc;

    fn create_test_ctx() -> SeriesContext {
        let bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                time: Utc::now(),
                open: 100.0 + i as f64,
                high: 105.0 + i as f64,
                low: 95.0 + i as f64,
                close: 102.0 + i as f64,
                volume: 1000.0,
            })
            .collect();
        let mut ctx = SeriesContext::new(&bars);
        ctx.curr_bar = 25;
        ctx
    }

    #[test]
    fn test_ta_sma() {
        let ctx = create_test_ctx();
        let result = ta_sma(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();

        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 100.0 && n < 130.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_ema() {
        let ctx = create_test_ctx();
        let result = ta_ema(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();

        if let Value::Number(n) = result {
            assert!(!n.is_nan());
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_rsi() {
        let ctx = create_test_ctx();
        let result = ta_rsi(&[Value::Number(102.0), Value::Number(14.0)], &ctx).unwrap();

        if let Value::Number(n) = result {
            assert!(n >= 0.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_math_abs() {
        let ctx = create_test_ctx();
        let result = math_abs(&[Value::Number(-5.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 5.0));
    }

    #[test]
    fn test_math_max() {
        let ctx = create_test_ctx();
        let result = math_max(&[Value::Number(3.0), Value::Number(7.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 7.0));
    }

    #[test]
    fn test_math_sqrt() {
        let ctx = create_test_ctx();
        let result = math_sqrt(&[Value::Number(16.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn test_math_pow() {
        let ctx = create_test_ctx();
        let result = math_pow(&[Value::Number(2.0), Value::Number(10.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if (n - 1024.0).abs() < f64::EPSILON));
    }

    #[test]
    fn test_math_floor_ceil() {
        let ctx = create_test_ctx();
        let floor = math_floor(&[Value::Number(3.7)], &ctx).unwrap();
        assert!(matches!(floor, Value::Number(n) if n == 3.0));
        let ceil = math_ceil(&[Value::Number(3.2)], &ctx).unwrap();
        assert!(matches!(ceil, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn test_math_log_exp() {
        let ctx = create_test_ctx();
        let log_val = math_log(&[Value::Number(std::f64::consts::E)], &ctx).unwrap();
        assert!(matches!(log_val, Value::Number(n) if (n - 1.0).abs() < 1e-10));
        let exp_val = math_exp(&[Value::Number(1.0)], &ctx).unwrap();
        assert!(matches!(exp_val, Value::Number(n) if (n - std::f64::consts::E).abs() < 1e-10));
    }

    #[test]
    fn test_math_min() {
        let ctx = create_test_ctx();
        let result = math_min(&[Value::Number(3.0), Value::Number(7.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 3.0));
    }

    #[test]
    fn test_math_round() {
        let ctx = create_test_ctx();
        let result = math_round(&[Value::Number(3.6)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 4.0));
    }

    #[test]
    fn test_math_sign() {
        let ctx = create_test_ctx();
        assert!(
            matches!(math_sign(&[Value::Number(42.0)], &ctx).unwrap(), Value::Number(n) if n == 1.0)
        );
        assert!(
            matches!(math_sign(&[Value::Number(-5.0)], &ctx).unwrap(), Value::Number(n) if n == -1.0)
        );
        assert!(
            matches!(math_sign(&[Value::Number(0.0)], &ctx).unwrap(), Value::Number(n) if n == 0.0)
        );
    }

    #[test]
    fn test_ta_vwap() {
        let ctx = create_test_ctx();
        let result = ta_vwap(&[], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 90.0 && n < 140.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_macd() {
        let ctx = create_test_ctx();
        let result = ta_macd(
            &[
                Value::Number(102.0),
                Value::Number(12.0),
                Value::Number(26.0),
                Value::Number(9.0),
            ],
            &ctx,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_ta_bb() {
        let ctx = create_test_ctx();
        let result = ta_bb(
            &[
                Value::Number(102.0),
                Value::Number(20.0),
                Value::Number(2.0),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result, Value::Number(n) if !n.is_nan()));
    }

    #[test]
    fn test_ta_atr() {
        let ctx = create_test_ctx();
        let result = ta_atr(&[Value::Number(14.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 0.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_stoch() {
        let ctx = create_test_ctx();
        let result = ta_stoch(
            &[
                Value::Number(102.0),
                Value::Number(105.0),
                Value::Number(95.0),
                Value::Number(14.0),
            ],
            &ctx,
        )
        .unwrap();
        if let Value::Number(n) = result {
            assert!(n >= 0.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_stdev() {
        let ctx = create_test_ctx();
        let result = ta_stdev(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if !n.is_nan() && n >= 0.0));
    }

    #[test]
    fn test_str_contains() {
        let ctx = create_test_ctx();
        let result = str_contains(
            &[
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result, Value::Boolean(true)));
        let result2 = str_contains(
            &[
                Value::String("hello world".to_string()),
                Value::String("xyz".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result2, Value::Boolean(false)));
    }

    #[test]
    fn test_str_length() {
        let ctx = create_test_ctx();
        let result = str_length(&[Value::String("hello".to_string())], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if n == 5.0));
    }

    #[test]
    fn test_str_replace() {
        let ctx = create_test_ctx();
        let result = str_replace(
            &[
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
                Value::String("rust".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result, Value::String(ref s) if s == "hello rust"));
    }

    #[test]
    fn test_str_split() {
        let ctx = create_test_ctx();
        let result = str_split(
            &[
                Value::String("a,b,c".to_string()),
                Value::String(",".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert!(matches!(&arr[0], Value::String(s) if s == "a"));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_str_tostring() {
        let ctx = create_test_ctx();
        let result = str_tostring(&[Value::Number(42.0)], &ctx).unwrap();
        assert!(matches!(result, Value::String(ref s) if s == "42"));
        let result2 = str_tostring(&[Value::Boolean(true)], &ctx).unwrap();
        assert!(matches!(result2, Value::String(ref s) if s == "true"));
    }

    #[test]
    fn test_str_tonumber() {
        let ctx = create_test_ctx();
        let result = str_tonumber(&[Value::String("3.14".to_string())], &ctx).unwrap();
        assert!(matches!(result, Value::Number(n) if (n - 3.14).abs() < f64::EPSILON));
        let result2 = str_tonumber(&[Value::String("not_a_number".to_string())], &ctx).unwrap();
        assert!(matches!(result2, Value::Number(n) if n.is_nan()));
    }

    #[test]
    fn test_str_upper_lower() {
        let ctx = create_test_ctx();
        let upper = str_upper(&[Value::String("hello".to_string())], &ctx).unwrap();
        assert!(matches!(upper, Value::String(ref s) if s == "HELLO"));
        let lower = str_lower(&[Value::String("WORLD".to_string())], &ctx).unwrap();
        assert!(matches!(lower, Value::String(ref s) if s == "world"));
    }

    #[test]
    fn test_str_startswith_endswith() {
        let ctx = create_test_ctx();
        let starts = str_startswith(
            &[
                Value::String("hello world".to_string()),
                Value::String("hello".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(starts, Value::Boolean(true)));
        let ends = str_endswith(
            &[
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(ends, Value::Boolean(true)));
    }

    #[test]
    fn test_str_substring() {
        let ctx = create_test_ctx();
        let result = str_substring(
            &[
                Value::String("hello world".to_string()),
                Value::Number(0.0),
                Value::Number(5.0),
            ],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_str_trim() {
        let ctx = create_test_ctx();
        let result = str_trim(&[Value::String("  hello  ".to_string())], &ctx).unwrap();
        assert!(matches!(result, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_arg_validation_errors() {
        let ctx = create_test_ctx();
        assert!(ta_sma(&[Value::Number(1.0)], &ctx).is_err());
        assert!(math_max(&[Value::Number(1.0)], &ctx).is_err());
        assert!(math_pow(&[Value::Number(1.0)], &ctx).is_err());
        assert!(str_contains(&[Value::String("a".to_string())], &ctx).is_err());
        assert!(str_replace(&[Value::String("a".to_string())], &ctx).is_err());
        assert!(math_abs(&[], &ctx).is_err());
        assert!(math_sqrt(&[], &ctx).is_err());
        assert!(str_length(&[], &ctx).is_err());
        assert!(str_tostring(&[], &ctx).is_err());
        assert!(str_length(&[Value::Number(42.0)], &ctx).is_err());
        assert!(str_upper(&[Value::Number(42.0)], &ctx).is_err());
    }

    // ========================================================================
    // Phase 1.2: New ta.* function tests
    // ========================================================================

    #[test]
    fn test_ta_variance() {
        let ctx = create_test_ctx();
        let result = ta_variance(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= 0.0); // Variance is always non-negative
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_median() {
        let ctx = create_test_ctx();
        let result = ta_median(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 90.0 && n < 140.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_mode() {
        let ctx = create_test_ctx();
        let result = ta_mode(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_percentrank() {
        let ctx = create_test_ctx();
        let result = ta_percentrank(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= 0.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_correlation() {
        let ctx = create_test_ctx();
        let result = ta_correlation(
            &[
                Value::Number(102.0),
                Value::Number(1000.0),
                Value::Number(10.0),
            ],
            &ctx,
        )
        .unwrap();
        if let Value::Number(n) = result {
            // Correlation is between -1 and 1 (or NaN for degenerate cases)
            assert!(n.is_nan() || (n >= -1.0 && n <= 1.0));
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_linreg() {
        let ctx = create_test_ctx();
        let result = ta_linreg(&[Value::Number(102.0), Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_pivothigh() {
        let ctx = create_test_ctx();
        let result = ta_pivothigh(
            &[Value::Number(105.0), Value::Number(3.0), Value::Number(3.0)],
            &ctx,
        )
        .unwrap();
        // Pivot may or may not be detected depending on data
        assert!(matches!(result, Value::Number(_)));
    }

    #[test]
    fn test_ta_pivotlow() {
        let ctx = create_test_ctx();
        let result = ta_pivotlow(
            &[Value::Number(95.0), Value::Number(3.0), Value::Number(3.0)],
            &ctx,
        )
        .unwrap();
        assert!(matches!(result, Value::Number(_)));
    }

    #[test]
    fn test_ta_wpr() {
        let ctx = create_test_ctx();
        let result = ta_wpr(&[Value::Number(14.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= -100.0 && n <= 0.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_cmo() {
        let ctx = create_test_ctx();
        let result = ta_cmo(&[Value::Number(102.0), Value::Number(14.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= -100.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_mfi() {
        let ctx = create_test_ctx();
        let result = ta_mfi(&[Value::Number(14.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= 0.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_obv() {
        let ctx = create_test_ctx();
        let result = ta_obv(&[], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_dmi() {
        let ctx = create_test_ctx();
        let result = ta_dmi(&[Value::Number(14.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= 0.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_adx() {
        let ctx = create_test_ctx();
        // ADX requires length * 2 bars; use shorter period for test context (30 bars, curr_bar=25)
        let result = ta_adx(&[Value::Number(10.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n >= 0.0 && n <= 100.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_sar() {
        let ctx = create_test_ctx();
        let result = ta_sar(
            &[Value::Number(0.02), Value::Number(0.02), Value::Number(0.2)],
            &ctx,
        )
        .unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 0.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_kc() {
        let ctx = create_test_ctx();
        let result = ta_kc(&[Value::Number(20.0), Value::Number(2.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_ta_dc() {
        let ctx = create_test_ctx();
        let result = ta_dc(&[Value::Number(20.0)], &ctx).unwrap();
        if let Value::Number(n) = result {
            assert!(!n.is_nan());
            assert!(n > 90.0 && n < 150.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_new_ta_arg_validation() {
        let ctx = create_test_ctx();
        // Test argument validation for new functions
        assert!(ta_variance(&[Value::Number(1.0)], &ctx).is_err());
        assert!(ta_median(&[Value::Number(1.0)], &ctx).is_err());
        assert!(ta_percentrank(&[Value::Number(1.0)], &ctx).is_err());
        assert!(ta_correlation(&[Value::Number(1.0), Value::Number(2.0)], &ctx).is_err());
        assert!(ta_linreg(&[Value::Number(1.0)], &ctx).is_err());
        assert!(ta_pivothigh(&[Value::Number(1.0), Value::Number(2.0)], &ctx).is_err());
        assert!(ta_pivotlow(&[Value::Number(1.0), Value::Number(2.0)], &ctx).is_err());
        assert!(ta_wpr(&[], &ctx).is_err());
        assert!(ta_cmo(&[Value::Number(1.0)], &ctx).is_err());
        assert!(ta_mfi(&[], &ctx).is_err());
        assert!(ta_dmi(&[], &ctx).is_err());
        assert!(ta_adx(&[], &ctx).is_err());
        assert!(ta_sar(&[Value::Number(0.02), Value::Number(0.02)], &ctx).is_err());
        assert!(ta_kc(&[Value::Number(20.0)], &ctx).is_err());
        assert!(ta_dc(&[], &ctx).is_err());
    }
}
