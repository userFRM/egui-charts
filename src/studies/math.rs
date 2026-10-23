//! Least-squares maths shared by studies and drawings.

use crate::model::Bar;

/// Least-squares line through the closes of a span of bars, against their
/// position in the span.
#[derive(Clone, Copy)]
pub(crate) struct LinearFit {
    /// Price change per bar.
    pub(crate) slope: f64,
    /// Fitted price at the first bar.
    pub(crate) intercept: f64,
    /// Standard deviation of the closes around the line (n - 1 denominator).
    pub(crate) sigma: f64,
    /// Share of the closes' variance the line explains.
    pub(crate) r_squared: f64,
    /// Pearson correlation of the closes with their position; `None` when
    /// the closes are flat.
    pub(crate) correlation: Option<f64>,
}

/// Means of the closes of a span of bars and of their positions in it, and
/// the sums of the products of their deviations from those means.
#[derive(Clone, Copy, Default)]
pub(crate) struct Moments {
    n: f64,
    mean_x: f64,
    mean_y: f64,
    sxx: f64,
    sxy: f64,
    syy: f64,
}

impl Moments {
    /// Moments of the closes of `bars`: the mean, corrected for rounding, then the centred sums.
    pub(crate) fn of(bars: &[Bar]) -> Self {
        let n = bars.len() as f64;
        if bars.is_empty() {
            return Self::default();
        }
        let mean_x = (n - 1.0) / 2.0;
        let mean_y = bars.iter().map(|bar| bar.close).sum::<f64>() / n;
        // Correct the rounding of the sum, so equal closes give their close as the mean.
        let mean_y = mean_y + bars.iter().map(|bar| bar.close - mean_y).sum::<f64>() / n;
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for (i, bar) in bars.iter().enumerate() {
            let (dx, dy) = (i as f64 - mean_x, bar.close - mean_y);
            sxx += dx * dx;
            sxy += dx * dy;
            syy += dy * dy;
        }
        Self {
            n,
            mean_x,
            mean_y,
            sxx,
            sxy,
            syy,
        }
    }

    /// These moments with the close `y` of the next bar added (Welford's update).
    #[cfg(feature = "widget")]
    pub(crate) fn with(&self, y: f64) -> Self {
        let n = self.n + 1.0;
        let (dx, dy) = (self.n - self.mean_x, y - self.mean_y);
        let (mean_x, mean_y) = (self.mean_x + dx / n, self.mean_y + dy / n);
        Self {
            n,
            mean_x,
            mean_y,
            sxx: self.sxx + dx * (self.n - mean_x),
            sxy: self.sxy + dx * (y - mean_y),
            syy: self.syy + dy * (y - mean_y),
        }
    }

    /// The least-squares line; `None` for fewer than two bars.
    pub(crate) fn fit(&self) -> Option<LinearFit> {
        if self.n < 2.0 {
            return None;
        }
        let slope = self.sxy / self.sxx;
        let residual = (self.syy - slope * self.sxy).max(0.0);
        // Closes are flat when they vary by no more than the rounding of their mean.
        let flat = self.syy <= 16.0 * self.n * (self.mean_y * f64::EPSILON).powi(2);
        Some(LinearFit {
            slope,
            intercept: self.mean_y - slope * self.mean_x,
            sigma: (residual / (self.n - 1.0)).sqrt(),
            // Flat closes are fitted exactly.
            r_squared: if flat { 1.0 } else { 1.0 - residual / self.syy },
            correlation: (!flat).then(|| self.sxy / (self.sxx * self.syy).sqrt()),
        })
    }
}
