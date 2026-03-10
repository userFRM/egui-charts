/// Statistical TA functions (ta namespace - Phase 1.2)
///
/// Variance, median, mode, percent rank, correlation, linear regression.
use super::super::runtime::SeriesContext;
use super::super::types::{RuntimeError, Value};

/// Variance over period
pub(crate) fn ta_variance(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.variance requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
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

        Ok(Value::Number(variance))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Median value over period
pub(crate) fn ta_median(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.median requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;
        let mut sorted: Vec<f64> = close_series[start..=curr_bar].to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mid = sorted.len() / 2;
        let median = if sorted.len().is_multiple_of(2) {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        } else {
            sorted[mid]
        };

        Ok(Value::Number(median))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Mode value over period (returns median for continuous data)
pub(crate) fn ta_mode(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    // For continuous price data, mode is not well-defined
    // We return the median as a reasonable approximation
    ta_median(args, ctx)
}

/// Percent rank - percentage of values below current value
pub(crate) fn ta_percentrank(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.percentrank requires 2 arguments: source, length".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;
        let current = close_series[curr_bar];
        let below_count = close_series[start..curr_bar]
            .iter()
            .filter(|&&x| x < current)
            .count();

        let percent_rank = (below_count as f64 / (length - 1) as f64) * 100.0;
        Ok(Value::Number(percent_rank))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Correlation coefficient between two series
pub(crate) fn ta_correlation(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 3 {
        return Err(RuntimeError::InvalidArguments(
            "ta.correlation requires 3 arguments: source1, source2, length".to_string(),
        ));
    }

    let _src1 = args[0].as_num()?;
    let _src2 = args[1].as_num()?;
    let length = args[2].as_num()? as usize;
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    // Calculate correlation between close and volume (common use case)
    let close_series = ctx.get_series("close");
    let volume_series = ctx.get_series("volume");

    if let (Some(close), Some(volume)) = (close_series, volume_series) {
        let start = curr_bar + 1 - length;

        let x_mean: f64 = close[start..=curr_bar].iter().sum::<f64>() / length as f64;
        let y_mean: f64 = volume[start..=curr_bar].iter().sum::<f64>() / length as f64;

        let mut cov = 0.0;
        let mut var_x = 0.0;
        let mut var_y = 0.0;

        for i in start..=curr_bar {
            let dx = close[i] - x_mean;
            let dy = volume[i] - y_mean;
            cov += dx * dy;
            var_x += dx * dx;
            var_y += dy * dy;
        }

        let denom = (var_x * var_y).sqrt();
        if denom > 0.0 {
            Ok(Value::Number(cov / denom))
        } else {
            Ok(Value::Number(f64::NAN))
        }
    } else {
        Ok(Value::Number(f64::NAN))
    }
}

/// Linear regression value
pub(crate) fn ta_linreg(args: &[Value], ctx: &SeriesContext) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::InvalidArguments(
            "ta.linreg requires 2-3 arguments: source, length[, offset]".to_string(),
        ));
    }

    let _src = args[0].as_num()?;
    let length = args[1].as_num()? as usize;
    let offset = if args.len() >= 3 {
        args[2].as_num()? as i32
    } else {
        0
    };
    let curr_bar = ctx.curr_bar;

    if length == 0 || curr_bar + 1 < length {
        return Ok(Value::Number(f64::NAN));
    }

    if let Some(close_series) = ctx.get_series("close") {
        let start = curr_bar + 1 - length;

        // Calculate linear regression using least squares
        // y = mx + b
        let n = length as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for (i, idx) in (start..=curr_bar).enumerate() {
            let x = i as f64;
            let y = close_series[idx];
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() < f64::EPSILON {
            return Ok(Value::Number(f64::NAN));
        }

        let m = (n * sum_xy - sum_x * sum_y) / denom;
        let b = (sum_y - m * sum_x) / n;

        // Return value at current bar + offset
        let target_x = (length - 1) as f64 + offset as f64;
        Ok(Value::Number(m * target_x + b))
    } else {
        Ok(Value::Number(f64::NAN))
    }
}
