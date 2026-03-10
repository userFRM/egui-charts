/// Advanced TA functions (ta namespace)
///
/// Pivot detection, momentum indicators (Williams %R, CMO, MFI, OBV),
/// trend indicators (DMI, ADX, Parabolic SAR), and channel indicators
/// (Keltner Channel, Donchian Channel).
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};
use super::ta::ta_ema;

// ============================================================================
// Pivot Detection Functions
// ============================================================================

/// Pivot high detection
pub(crate) fn ta_pivothigh(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.pivothigh requires 3 arguments: source, leftbars, rightbars".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let left = args[1].as_num()? as usize;
    let right = args[2].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    // Need enough bars on both sides
    if curr_bar < left + right || right == 0 {
        return Ok(Value::Number(f64::NAN));
    }

    // The pivot bar is 'right' bars ago
    let pivot_bar = curr_bar - right;
    if pivot_bar < left {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(high_series) = ctx.get_series("high") {
        let pivot_value = high_series[pivot_bar];

        // Check left bars
        for i in (pivot_bar - left)..pivot_bar {
            if high_series[i] >= pivot_value {
                return Ok(Value::Number(f64::NAN));
            }
        }

        // Check right bars
        for i in (pivot_bar + 1)..=curr_bar {
            if high_series[i] >= pivot_value {
                return Ok(Value::Number(f64::NAN));
            }
        }

        Ok(Value::Number(pivot_value))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Pivot low detection
pub(crate) fn ta_pivotlow(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.pivotlow requires 3 arguments: source, leftbars, rightbars".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let left = args[1].as_num()? as usize;
    let right = args[2].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if curr_bar < left + right || right == 0 {
        return Ok(Value::Number(f64::NAN));
    }

    let pivot_bar = curr_bar - right;
    if pivot_bar < left {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(low_series) = ctx.get_series("low") {
        let pivot_value = low_series[pivot_bar];

        // Check left bars
        for i in (pivot_bar - left)..pivot_bar {
            if low_series[i] <= pivot_value {
                return Ok(Value::Number(f64::NAN));
            }
        }

        // Check right bars
        for i in (pivot_bar + 1)..=curr_bar {
            if low_series[i] <= pivot_value {
                return Ok(Value::Number(f64::NAN));
            }
        }

        Ok(Value::Number(pivot_value))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

// ============================================================================
// Momentum Indicators
// ============================================================================

/// Williams %R
pub(crate) fn ta_wpr(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.wpr requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
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
            Ok(Value::Number(-50.0))
        } else {
            // %R = (Highest High - Close) / (Highest High - Lowest Low) * -100
            let wpr = (highest - close[curr_bar]) / range * -100.0;
            Ok(Value::Number(wpr))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Chande Momentum Oscillator
pub(crate) fn ta_cmo(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.cmo requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let mut sum_up = 0.0;
        let mut sum_down = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            if i > 0 {
                let change = close_series[i] - close_series[i - 1];
                if change > 0.0 {
                    sum_up += change;
                } else {
                    sum_down -= change; // Make positive
                }
            }
        }

        let total = sum_up + sum_down;
        if total == 0.0 {
            Ok(Value::Number(0.0))
        } else {
            // CMO = (Sum Up - Sum Down) / (Sum Up + Sum Down) * 100
            let cmo = (sum_up - sum_down) / total * 100.0;
            Ok(Value::Number(cmo))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Money Flow Index
pub(crate) fn ta_mfi(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.mfi requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");
    let volume_series = ctx.get_series("volume");

    if let (Some(high), Some(low), Some(close), Some(volume)) =
        (high_series, low_series, close_series, volume_series)
    {
        let mut positive_mf = 0.0;
        let mut negative_mf = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            // Typical Price = (H + L + C) / 3
            let tp = (high[i] + low[i] + close[i]) / 3.0;
            let mf = tp * volume[i];

            if i > 0 {
                let prev_tp = (high[i - 1] + low[i - 1] + close[i - 1]) / 3.0;
                if tp > prev_tp {
                    positive_mf += mf;
                } else if tp < prev_tp {
                    negative_mf += mf;
                }
            }
        }

        if negative_mf == 0.0 {
            Ok(Value::Number(100.0))
        } else {
            // MFI = 100 - (100 / (1 + MF Ratio))
            let mf_ratio = positive_mf / negative_mf;
            let mfi = 100.0 - (100.0 / (1.0 + mf_ratio));
            Ok(Value::Number(mfi))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// On Balance Volume
pub(crate) fn ta_obv(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    let _ = args;
    let curr_bar = ctx.curr_bar;

    let close_series = ctx.get_series("close");
    let volume_series = ctx.get_series("volume");

    if let (Some(close), Some(volume)) = (close_series, volume_series) {
        let mut obv = 0.0;

        for i in 0..=curr_bar {
            if i > 0 {
                if close[i] > close[i - 1] {
                    obv += volume[i];
                } else if close[i] < close[i - 1] {
                    obv -= volume[i];
                }
                // If equal, OBV unchanged
            } else {
                obv = volume[i];
            }
        }

        Ok(Value::Number(obv))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

// ============================================================================
// Trend Indicators
// ============================================================================

/// Directional Movement Index (+DI)
pub(crate) fn ta_dmi(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.dmi requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar < length {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");

    if let (Some(high), Some(low), Some(close)) = (high_series, low_series, close_series) {
        let mut plus_dm_sum = 0.0;
        let mut tr_sum = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            if i > 0 {
                let up_move = high[i] - high[i - 1];
                let down_move = low[i - 1] - low[i];

                // +DM
                if up_move > down_move && up_move > 0.0 {
                    plus_dm_sum += up_move;
                }

                // True Range
                let tr = (high[i] - low[i])
                    .max((high[i] - close[i - 1]).abs())
                    .max((low[i] - close[i - 1]).abs());
                tr_sum += tr;
            }
        }

        if tr_sum == 0.0 {
            Ok(Value::Number(0.0))
        } else {
            // +DI = 100 * Smoothed +DM / ATR
            let plus_di = 100.0 * plus_dm_sum / tr_sum;
            Ok(Value::Number(plus_di))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Average Directional Index
pub(crate) fn ta_adx(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.adx requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar < length * 2 {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");
    let close_series = ctx.get_series("close");

    if let (Some(high), Some(low), Some(close)) = (high_series, low_series, close_series) {
        let mut plus_dm_sum = 0.0;
        let mut minus_dm_sum = 0.0;
        let mut tr_sum = 0.0;

        for i in (curr_bar + 1 - length)..=curr_bar {
            if i > 0 {
                let up_move = high[i] - high[i - 1];
                let down_move = low[i - 1] - low[i];

                if up_move > down_move && up_move > 0.0 {
                    plus_dm_sum += up_move;
                }
                if down_move > up_move && down_move > 0.0 {
                    minus_dm_sum += down_move;
                }

                let tr = (high[i] - low[i])
                    .max((high[i] - close[i - 1]).abs())
                    .max((low[i] - close[i - 1]).abs());
                tr_sum += tr;
            }
        }

        if tr_sum == 0.0 {
            return Ok(Value::Number(0.0));
        }

        let plus_di = 100.0 * plus_dm_sum / tr_sum;
        let minus_di = 100.0 * minus_dm_sum / tr_sum;
        let di_sum = plus_di + minus_di;

        if di_sum == 0.0 {
            Ok(Value::Number(0.0))
        } else {
            // DX = |+DI - -DI| / (+DI + -DI) * 100
            let dx = (plus_di - minus_di).abs() / di_sum * 100.0;
            // ADX is smoothed DX - simplified here as single DX value
            Ok(Value::Number(dx))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Parabolic SAR
pub(crate) fn ta_sar(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.sar requires 3 arguments: start, increment, max".to_string(),
        ));
    }

    let start_af = args[0].as_num()?;
    let inc_af = args[1].as_num()?;
    let max_af = args[2].as_num()?;
    let curr_bar = ctx.curr_bar;

    if curr_bar < 2 {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");

    if let (Some(high), Some(low)) = (high_series, low_series) {
        // Initialize SAR calculation
        let mut sar = low[0];
        let mut ep = high[0]; // Extreme point
        let mut af = start_af;
        let mut is_uptrend = true;

        for i in 1..=curr_bar {
            let prev_sar = sar;

            if is_uptrend {
                sar = prev_sar + af * (ep - prev_sar);
                sar = sar.min(low[i - 1]);
                if i > 1 {
                    sar = sar.min(low[i - 2].min(low[i - 1]));
                }

                if high[i] > ep {
                    ep = high[i];
                    af = (af + inc_af).min(max_af);
                }

                if low[i] < sar {
                    is_uptrend = false;
                    sar = ep;
                    ep = low[i];
                    af = start_af;
                }
            } else {
                sar = prev_sar - af * (prev_sar - ep);
                sar = sar.max(high[i - 1]);
                if i > 1 {
                    sar = sar.max(high[i - 2].max(high[i - 1]));
                }

                if low[i] < ep {
                    ep = low[i];
                    af = (af + inc_af).min(max_af);
                }

                if high[i] > sar {
                    is_uptrend = true;
                    sar = ep;
                    ep = high[i];
                    af = start_af;
                }
            }
        }

        Ok(Value::Number(sar))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

// ============================================================================
// Channel Indicators
// ============================================================================

/// Keltner Channel (returns middle band = EMA)
pub(crate) fn ta_kc(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.kc requires 2 arguments: length, mult".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let _mult = args[1].as_num()?;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    // Middle band = EMA(close, length)
    if let Some(close_series) = ctx.get_series("close") {
        let source = close_series[curr_bar];
        ta_ema(&[Value::Number(source), Value::Number(length as f64)], ctx)
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Donchian Channel (returns middle = (highest + lowest) / 2)
pub(crate) fn ta_dc(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "ta.dc requires 1 argument: length".to_string(),
        ));
    }

    let length = args[0].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    let high_series = ctx.get_series("high");
    let low_series = ctx.get_series("low");

    if let (Some(high), Some(low)) = (high_series, low_series) {
        let start = curr_bar + 1 - length;

        let highest: f64 = high[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest: f64 = low[start..=curr_bar]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);

        // Middle = (Upper + Lower) / 2
        Ok(Value::Number((highest + lowest) / 2.0))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}
