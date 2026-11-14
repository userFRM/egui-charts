use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Moving Average Channel: the simple average of the highs as the upper line and of the lows as
/// the lower line, each over its own length and drawn its own number of bars later (earlier for
/// a negative offset), with the band between them filled. A window with a non-finite high (low)
/// has no upper (lower) value.
#[derive(Clone)]
pub struct MAChannel {
    upper_length: usize,
    lower_length: usize,
    upper_offset: i32,
    lower_offset: i32,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl MAChannel {
    /// A channel over `upper_length` highs and `lower_length` lows, each at least 1, drawn on
    /// the bars they end at.
    pub fn new(upper_length: usize, lower_length: usize) -> Self {
        Self {
            upper_length: upper_length.max(1),
            lower_length: lower_length.max(1),
            upper_offset: 0,
            lower_offset: 0,
            values: IndicatorValues::new(),
            colors: vec![
                palette::LIGHT_BLUE,                       // Upper
                palette::ORANGE,                           // Lower
                palette::LIGHT_BLUE.gamma_multiply_u8(13), // Fill
            ],
            visible: true,
        }
    }

    /// Draw the upper line `upper` and the lower line `lower` bars later; negative values draw
    /// earlier.
    pub fn with_offsets(mut self, upper: i32, lower: i32) -> Self {
        self.upper_offset = upper;
        self.lower_offset = lower;
        self
    }
}

impl Default for MAChannel {
    fn default() -> Self {
        Self::new(20, 20)
    }
}

impl MAChannel {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        // Each window is summed afresh, so an update equals a full calculation bit for bit.
        let mean = |i: usize, n: usize, price: fn(&Bar) -> f64| {
            if i + 1 < n {
                f64::NAN
            } else {
                data[i + 1 - n..=i].iter().map(price).sum::<f64>() / n as f64
            }
        };
        let mut values = IndicatorValues::new();
        for i in 0..data.len() {
            values.push_lines(&[
                mean(i, self.upper_length, |b| b.high),
                mean(i, self.lower_length, |b| b.low),
            ]);
        }
        values
    }
}

impl Indicator for MAChannel {
    crate::studies::inputs::study_inputs!(self;
        upper_length: length("Upper length", 1, usize::MAX),
        upper_offset: integer("Upper offset", i32::MIN, i32::MAX),
        lower_length: length("Lower length", 1, usize::MAX),
        lower_offset: integer("Lower offset", i32::MIN, i32::MAX)
    ; {});

    fn plot_offset(&self, line: usize) -> isize {
        match line {
            0 => self.upper_offset as isize,
            1 => self.lower_offset as isize,
            _ => 0,
        }
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(1),
            color: self.colors[2],
            inverted_color: None,
        }]
    }

    fn name(&self) -> &str {
        "MA Channel"
    }

    fn desc(&self) -> &str {
        "Moving Average Channel - Simple averages of the highs and of the lows"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        let window = self.upper_length.max(self.lower_length);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(window - 1),
            |data| self.compute(data),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
            let n = colors.len().min(self.colors.len());
            self.colors[..n].copy_from_slice(&colors[..n]);
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
        2
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("Upper({})", self.upper_length),
            format!("Lower({})", self.lower_length),
        ]
    }
}
