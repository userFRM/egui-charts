//! Volume classified by the direction of host-supplied lower-timeframe candles.
//!
//! A rising candle contributes to up volume; a falling candle to down volume. When open
//! equals close, a close at or above the previous intrabar close counts as up, otherwise
//! down. Up volume is positive, down volume negative and delta their sum. Missing intrabar
//! history has no value; the parent candle's volume is never substituted for it.

use crate::model::{Bar, BarClock, Instrument, Timeframe};
use crate::studies::{Indicator, IndicatorValues, PlotPattern, PlotStyle, PlotType, StudyFormat};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Up and down volume from intrabars supplied by the host, in a separate pane.
#[derive(Clone)]
pub struct UpDownVolume {
    clock: BarClock,
    intrabars: Vec<Bar>,
    values: IndicatorValues,
    colors: [Color32; 3],
    symbol: Option<String>,
    visible: bool,
}

impl UpDownVolume {
    /// One-minute intrabars on a one-minute chart until the chart supplies its interval.
    /// Hosts read [`Indicator::lower_timeframe`] and supply that interval's history.
    pub fn new() -> Self {
        Self {
            clock: BarClock::new(Timeframe::Min1),
            intrabars: Vec::new(),
            values: IndicatorValues::new(),
            colors: [
                DESIGN_TOKENS.semantic.chart.bullish.gamma_multiply(0.35),
                DESIGN_TOKENS.semantic.chart.bearish.gamma_multiply(0.35),
                DESIGN_TOKENS.semantic.extended.gray,
            ],
            symbol: None,
            visible: true,
        }
    }
}

impl Default for UpDownVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for UpDownVolume {
    fn name(&self) -> &str {
        "Up/Down Volume"
    }
    fn desc(&self) -> &str {
        "Up and down volume from lower-timeframe candles supplied by the host"
    }
    fn lower_timeframe(&self) -> Option<Timeframe> {
        let tf = self.clock.timeframe()?;
        if matches!(tf, Timeframe::Ticks(_)) {
            return None;
        }
        Some(if tf.duration_ms() < Timeframe::Min1.duration_ms() {
            Timeframe::Sec1
        } else if tf.duration_ms() < Timeframe::Day1.duration_ms() {
            Timeframe::Min1
        } else if tf == Timeframe::Day1 {
            Timeframe::Min5
        } else {
            Timeframe::Hour1
        })
    }
    fn set_lower_timeframe_bars(
        &mut self,
        timeframe: Timeframe,
        bars: Vec<Bar>,
    ) -> Result<(), &'static str> {
        if self.lower_timeframe() != Some(timeframe) {
            return Err("unexpected lower timeframe");
        }
        if bars.windows(2).any(|pair| pair[0].time >= pair[1].time) {
            return Err("intrabar times must be strictly increasing");
        }
        if bars.iter().any(|bar| {
            !bar.open.is_finite()
                || !bar.close.is_finite()
                || !bar.volume.is_finite()
                || bar.volume < 0.0
        }) {
            return Err("invalid intrabar price or volume");
        }
        self.intrabars = bars;
        self.values.clear();
        Ok(())
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let symbol = instrument.map(|instrument| instrument.symbol.clone());
        let changed = self.symbol != symbol;
        if changed {
            self.intrabars.clear();
        }
        self.symbol = symbol;
        changed
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        if self.clock.timeframe() != clock.timeframe() {
            self.intrabars.clear();
        }
        self.clock = clock.clone();
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
        let Some(tf) = self
            .clock
            .timeframe()
            .filter(|tf| !matches!(tf, Timeframe::Ticks(_)))
        else {
            return;
        };
        let daily = tf.duration_ms() >= Timeframe::Day1.duration_ms();
        let sessions = self.clock.for_timeframe(Timeframe::Min1).sessions();
        let key = |bar: &Bar| {
            if daily {
                BarClock::new(tf).bar_open(
                    sessions
                        .trading_date(bar.time)
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc(),
                )
            } else {
                self.clock.bar_open(bar.time)
            }
        };
        let mut at = 0;
        let mut previous = None;
        for (i, bar) in data.iter().enumerate() {
            let parent = if daily {
                BarClock::new(tf).bar_open(bar.time)
            } else {
                self.clock.bar_open(bar.time)
            };
            let (mut up, mut down, mut count) = (0.0, 0.0, 0);
            while let Some(intrabar) = self.intrabars.get(at).filter(|b| key(b) <= parent) {
                let rising = intrabar.close > intrabar.open
                    || (intrabar.close == intrabar.open
                        && previous.is_none_or(|close| intrabar.close >= close));
                if key(intrabar) == parent {
                    if rising {
                        up += intrabar.volume;
                    } else {
                        down -= intrabar.volume;
                    }
                    count += 1;
                }
                previous = Some(intrabar.close);
                at += 1;
            }
            if count > 0 && bar.close.is_finite() {
                for (line, value) in [up, down, up + down].into_iter().enumerate() {
                    self.values.set_value(line, i, value);
                }
            }
        }
    }
    fn is_overlay(&self) -> bool {
        false
    }
    fn value_format(&self) -> StudyFormat {
        StudyFormat::Volume
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: PlotPattern::Solid,
            plot_type: if line == 2 {
                PlotType::Crosses
            } else {
                PlotType::Columns
            },
            visible: true,
        }
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        (line == 2)
            .then(|| {
                self.values.value(2, bar).map(|delta| {
                    let [r, g, b, _] =
                        self.colors[usize::from(delta < 0.0)].to_srgba_unmultiplied();
                    Color32::from_rgb(r, g, b)
                })
            })
            .flatten()
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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
        ["Up Volume", "Down Volume", "Delta"]
            .map(String::from)
            .to_vec()
    }
}
