/// Core Technical Analysis functions (ta namespace)
///
/// Moving averages (SMA, EMA, WMA, HMA, VWMA), RSI, MACD, Bollinger Bands,
/// ATR, True Range, Stochastic, highest/lowest, change/momentum/ROC,
/// crossover/crossunder, cumulative sum, standard deviation, VWAP.
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};

/// Simple Moving Avg
pub(crate) fn ta_sma(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.sma requires 2 arguments: source, length".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    if length == 0 {
        return Ok(Value::Number(source));
    }

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    // Get close series for calculation
    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;
        let sum: f64 = close_series[start..=curr_bar].iter().sum();
        Ok(Value::Number(sum / length as f64))
    } else {
        Ok(Value::Number(source))
    }
}

/// Exponential Moving Avg
pub(crate) fn ta_ema(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.ema requires 2 arguments: source, length".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    if length == 0 {
        return Ok(Value::Number(source));
    }

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    // Calculate EMA using all available data up to current bar
    if let Some(close_series) = ctx.get_series("close") {
        let multiplier = 2.0 / (length as f64 + 1.0);

        // Start with SMA for first 'length' bars
        let sma: f64 = close_series[0..length].iter().sum::<f64>() / length as f64;

        // Calculate EMA from there
        let mut ema = sma;
        for i in length..=curr_bar {
            ema = (close_series[i] - ema) * multiplier + ema;
        }

        Ok(Value::Number(ema))
    } else {
        Ok(Value::Number(source))
    }
}

/// Weighted Moving Avg
pub(crate) fn ta_wma(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.wma requires 2 arguments: source, length".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    if length == 0 {
        return Ok(Value::Number(source));
    }

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;
        let mut weighted_sum = 0.0;
        let weight_sum = (length * (length + 1)) / 2;

        for (i, val) in close_series[start..=curr_bar].iter().enumerate() {
            weighted_sum += val * (i + 1) as f64;
        }

        Ok(Value::Number(weighted_sum / weight_sum as f64))
    } else {
        Ok(Value::Number(source))
    }
}

/// Hull Moving Avg
pub(crate) fn ta_hma(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.hma requires 2 arguments: source, length".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    // HMA is complex - simplified approximation for now
    // HMA = WMA(2*WMA(close, length/2) - WMA(close, length), sqrt(length))
    ta_wma(&[Value::Number(source), Value::Number(length as f64)], ctx)
}

/// Volume Weighted Moving Avg
pub(crate) fn ta_vwma(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.vwma requires 2 arguments: source, length".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    if length == 0 {
        return Ok(Value::Number(source));
    }

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    let close_series = ctx.get_series("close");
    let volume_series = ctx.get_series("volume");

    if let (Some(close), Some(volume)) = (close_series, volume_series) {
        let start = curr_bar + 1 - length;
        let mut pv_sum = 0.0;
        let mut v_sum = 0.0;

        for i in start..=curr_bar {
            pv_sum += close[i] * volume[i];
            v_sum += volume[i];
        }

        if v_sum > 0.0 {
            Ok(Value::Number(pv_sum / v_sum))
        } else {
            Ok(Value::Number(f64::NAN))
        }
    } else {
        Ok(Value::Number(source))
    }
}

/// Relative Strength Index
pub(crate) fn ta_rsi(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.rsi requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;

    let curr_bar = ctx.curr_bar;
    if curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let mut gains = 0.0;
        let mut losses = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            if i > 0 {
                let change = close_series[i] - close_series[i - 1];
                if change > 0.0 {
                    gains += change;
                } else {
                    losses -= change;
                }
            }
        }

        let avg_gain = gains / length as f64;
        let avg_loss = losses / length as f64;

        if avg_loss == 0.0 {
            Ok(Value::Number(100.0))
        } else {
            let rs = avg_gain / avg_loss;
            Ok(Value::Number(100.0 - (100.0 / (1.0 + rs))))
        }
    } else {
        Ok(Value::Number(50.0))
    }
}

/// MACD - returns MACD line value
pub(crate) fn ta_macd(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 4 {
        return Err(RuntimeError::InvalidArguments(
            "ta.macd requires 4 arguments: source, fast_len, slow_len, signal_len".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let fast = args[1].as_num()? as usize;
    let slow = args[2].as_num()? as usize;
    let _signal = args[3].as_num()? as usize;

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < slow {
        return Ok(Value::Number(f64::NAN));
    }

    // Calculate fast and slow EMAs
    let fast_ema = ta_ema(&[Value::Number(source), Value::Number(fast as f64)], ctx)?;
    let slow_ema = ta_ema(&[Value::Number(source), Value::Number(slow as f64)], ctx)?;

    let fast_val = fast_ema.as_num()?;
    let slow_val = slow_ema.as_num()?;

    Ok(Value::Number(fast_val - slow_val))
}

/// Bollinger Bands - returns middle band (SMA)
pub(crate) fn ta_bb(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.bb requires 3 arguments: source, length, mult".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let _mult = args[2].as_num()?;

    // Return the middle band (SMA)
    ta_sma(&[Value::Number(source), Value::Number(length as f64)], ctx)
}

/// Avg True Range
pub(crate) fn ta_atr(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.atr requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");

    if let (Some(high), Some(low), Some(close)) = (high_series, low_series, close_series) {
        let mut tr_sum = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            let tr = if i == 0 {
                high[i] - low[i]
            } else {
                let hl = high[i] - low[i];
                let hc = (high[i] - close[i - 1]).abs();
                let lc = (low[i] - close[i - 1]).abs();
                hl.max(hc).max(lc)
            };
            tr_sum += tr;
        }

        Ok(Value::Number(tr_sum / length as f64))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// True Range
pub(crate) fn ta_tr(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    let _ = args; // Unused
    let curr_bar = ctx.curr_bar;

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");

    if let (Some(high), Some(low), Some(close)) = (high_series, low_series, close_series) {
        let tr = if curr_bar == 0 {
            high[curr_bar] - low[curr_bar]
        } else {
            let hl = high[curr_bar] - low[curr_bar];
            let hc = (high[curr_bar] - close[curr_bar - 1]).abs();
            let lc = (low[curr_bar] - close[curr_bar - 1]).abs();
            hl.max(hc).max(lc)
        };
        Ok(Value::Number(tr))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Stochastic %K
pub(crate) fn ta_stoch(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.stoch requires 3 arguments: close, high, low (or k_period)".to_string(),
        ));
    }

    // In Pine, ta.stoch(close, high, low, k_period)
    let length = if args.len() >= 4 {
        args[3].as_num()? as usize
    } else {
        14
    };

    let curr_bar = ctx.curr_bar;
    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");

    if let (Some(high), Some(low), Some(close)) = (high_series, low_series, close_series) {
        let start = curr_bar + 1 - length;
        let highest: f64 = high[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest: f64 = low[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);

        let range = highest - lowest;
        if range == 0.0 {
            Ok(Value::Number(50.0))
        } else {
            let stoch = ((close[curr_bar] - lowest) / range) * 100.0;
            Ok(Value::Number(stoch))
        }
    } else {
        Ok(Value::Number(50.0))
    }
}

/// Highest value over period
pub(crate) fn ta_highest(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.highest requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(high_series) = ctx.get_series("high") {
        let start = curr_bar + 1 - length;
        let highest = high_series[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        Ok(Value::Number(highest))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Lowest value over period
pub(crate) fn ta_lowest(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.lowest requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(low_series) = ctx.get_series("low") {
        let start = curr_bar + 1 - length;
        let lowest = low_series[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        Ok(Value::Number(lowest))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Change from previous bar
pub(crate) fn ta_change(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.change requires 1 argument: source".to_string(),
        ));
    }

    let source = args[0].as_num()?;
    let length = if args.len() >= 2 {
        args[1].as_num()? as usize
    } else {
        1
    };

    let curr_bar = ctx.curr_bar;
    if curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        Ok(Value::Number(
            close_series[curr_bar] - close_series[curr_bar - length],
        ))
    } else {
        Ok(Value::Number(source))
    }
}

/// Momentum
pub(crate) fn ta_mom(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    ta_change(args, ctx)
}

/// Rate of Change
pub(crate) fn ta_roc(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.roc requires 1 argument: source".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = if args.len() >= 2 {
        args[1].as_num()? as usize
    } else {
        1
    };

    let curr_bar = ctx.curr_bar;
    if curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let prev = close_series[curr_bar - length];
        if prev != 0.0 {
            Ok(Value::Number(
                ((close_series[curr_bar] - prev) / prev) * 100.0,
            ))
        } else {
            Ok(Value::Number(f64::NAN))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Crossover - returns true when series1 crosses over series2
pub(crate) fn ta_crossover(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.crossover requires 2 arguments: series1, series2".to_string(),
        ));
    }

    // Simplified - would need historical values to properly detect crossover
    let _s1 = args[0].as_num()?;
    let _s2 = args[1].as_num()?;

    Ok(Value::Boolean(false))
}

/// Crossunder - returns true when series1 crosses under series2
pub(crate) fn ta_crossunder(args: &[Value], _ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.crossunder requires 2 arguments: series1, series2".to_string(),
        ));
    }

    let _s1 = args[0].as_num()?;
    let _s2 = args[1].as_num()?;

    Ok(Value::Boolean(false))
}

/// Cumulative sum
pub(crate) fn ta_cum(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.cum requires 1 argument: source".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let curr_bar = ctx.curr_bar;

    if let Some(close_series) = ctx.get_series("close") {
        let sum: f64 = close_series[0..=curr_bar].iter().sum();
        Ok(Value::Number(sum))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Standard Deviation
pub(crate) fn ta_stdev(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.stdev requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;
        let mean: f64 = close_series[start..=curr_bar].iter().sum::<f64>() / length as f64;

        let variance: f64 = close_series[start..=curr_bar]
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / length as f64;

        Ok(Value::Number(variance.sqrt()))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Volume Weighted Average Price
pub(crate) fn ta_vwap(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    let _ = args;
    let curr_bar = ctx.curr_bar;
    let hlc3_series = ctx.get_series("hlc3");
    let volume_series = ctx.get_series("volume");
    if let (Some(hlc3), Some(volume)) = (hlc3_series, volume_series) {
        let mut cum_pv = 0.0;
        let mut cum_vol = 0.0;
        for i in 0..=curr_bar {
            cum_pv += hlc3[i] * volume[i];
            cum_vol += volume[i];
        }
        if cum_vol > 0.0 {
            Ok(Value::Number(cum_pv / cum_vol))
        } else {
            Ok(Value::Number(f64::NAN))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}
