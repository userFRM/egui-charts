//! Resumable seeded averages used by composite studies.

#[derive(Clone, Default)]
pub(super) struct Smoothing {
    count: usize,
    value: f64,
}

impl Smoothing {
    pub(super) fn next(&mut self, input: f64, seed: usize, alpha: f64) -> f64 {
        if !input.is_finite() {
            return f64::NAN;
        }
        if self.count < seed {
            self.count += 1;
            self.value += input;
            if self.count < seed {
                return f64::NAN;
            }
            self.value /= seed as f64;
        } else {
            self.value += (input - self.value) * alpha;
        }
        self.value
    }
}
