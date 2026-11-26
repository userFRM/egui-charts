//! Fixed aggregations of executions, quotes and synchronized depth, never estimates from bars.

use super::anchor_period::AnchorPeriod;
use super::trade_delta::CvdMarkers;
use crate::data::{OrderFilter, OrderFlowInput, OrderFlowKind, SizeBasis};
use crate::model::{Bar, BarClock};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, PlotStyle, PlotType, StudyCandle, StudyInput,
};
use chrono::TimeDelta;
use egui::Color32;

const ANCHORS: [Option<AnchorPeriod>; 8] = [
    None,
    Some(AnchorPeriod::Session),
    Some(AnchorPeriod::Week),
    Some(AnchorPeriod::Month),
    Some(AnchorPeriod::Quarter),
    Some(AnchorPeriod::Year),
    Some(AnchorPeriod::Decade),
    Some(AnchorPeriod::Century),
];

/// An order-flow study in a separate pane. Use the factory titles or [`Self::new`].
///
/// Changing an aggregation input starts a fresh source: the host must replay historical events
/// if it needs values before that change. Reset period, representation and marker settings reuse
/// the held source. Book size and book balance use the last synchronized snapshot in the bar;
/// spread averages every accepted quote equally. Bars without the relevant stream remain gaps.
#[derive(Clone, Debug)]
pub struct OrderFlowStudy {
    kind: OrderFlowKind,
    filter: OrderFilter,
    basis: SizeBasis,
    input: OrderFlowInput,
    clock: BarClock,
    values: IndicatorValues,
    reset_revision: u64,
    colors: Vec<Color32>,
    visible: bool,
    /// Draw the rolling sums as a histogram rather than lines.
    histogram: bool,
    /// `Some(false)` is per-bar execution delta; `Some(true)` is cumulative execution delta.
    delta: Option<bool>,
    candle_style: bool,
    candles: Vec<StudyCandle>,
    anchor: Option<AnchorPeriod>,
    volume_precision: Option<usize>,
    markers: CvdMarkers,
    /// Earlier bars used to rank each execution range.
    lookback: usize,
    /// Low and high percentile boundaries for Volatility Regime.
    regime: [f64; 2],
}

impl OrderFlowStudy {
    #[cfg(feature = "widget")]
    pub(crate) fn replay_rate(&self) -> Option<(bool, usize, usize)> {
        match self.kind {
            OrderFlowKind::TradeRate {
                volume,
                seconds,
                instant,
            } if self.filter == OrderFilter::default() && self.basis == SizeBasis::Fill => {
                Some((volume, seconds, instant))
            }
            _ => None,
        }
    }

    /// Constructs one of the fixed stream aggregations with an empty source.
    pub fn new(kind: OrderFlowKind) -> Self {
        let kind = kind.normalized();
        Self {
            kind,
            filter: OrderFilter::default(),
            basis: SizeBasis::Fill,
            input: OrderFlowInput::new(kind),
            clock: BarClock::new(crate::model::Timeframe::Min1),
            values: IndicatorValues::new(),
            reset_revision: 0,
            colors: vec![
                Color32::from_rgb(38, 166, 154),
                Color32::from_rgb(239, 83, 80),
                Color32::from_rgb(120, 100, 220),
            ],
            visible: true,
            histogram: true,
            delta: None,
            candle_style: false,
            candles: Vec::new(),
            anchor: None,
            volume_precision: None,
            markers: CvdMarkers::default(),
            lookback: 100,
            regime: [20.0, 80.0],
        }
    }

    /// Exact execution delta in a separate pane, as candles by default. With `cumulative`,
    /// candles carry the total until the selected anchor period changes (each exchange trading
    /// day by default); otherwise every candle opens at zero. Inputs select an inclusive size
    /// range, whether it measures each fill or its aggressive order, and candles or a cumulative
    /// line / per-bar histogram. The histogram draws a bar whose delta is above zero in the Up
    /// colour and any other in the Down colour, as the candles do. No volume is inferred from
    /// price bars. The chart connects the same source as Size-Filtered CVD, including its
    /// historical coverage, retention and replay cutoff.
    ///
    /// Cumulative mode offers `divergence` and `exhaustion`, both off by default. `swing_bars`
    /// (3) requires strictly lower highs or higher lows on both sides of a price pivot.
    /// `exhaustion_bars` (3) requires consecutive new extremes with shrinking same-side bar
    /// delta. Labels sit at price extremes; a live bar never confirms a marker. When enabled,
    /// outputs 3..=6 are bullish/bearish divergence and bullish/bearish exhaustion: 1 on the
    /// closed confirmation bar, 0 on other closed bars. Use bar-close study alerts on these
    /// outputs. Resets and missing executions between the swings prevent comparison; a
    /// boundary after the second swing still permits its price confirmation. Invalid prices
    /// break price windows and comparisons. Exhaustion follows per-bar delta across resets.
    pub fn execution_delta(cumulative: bool) -> Self {
        Self {
            delta: Some(cumulative),
            candle_style: true,
            anchor: cumulative.then_some(AnchorPeriod::Session),
            colors: vec![
                crate::studies::palette::BLUE,
                crate::studies::palette::UP,
                crate::studies::palette::DOWN,
            ],
            ..Self::new(OrderFlowKind::FilteredCvd {
                min: 0.0,
                max: f64::MAX,
            })
        }
    }

    /// Raw per-bar rows of the input for `data[start..]`, `None` for a bar without any.
    fn rows(&self, data: &[Bar], start: usize) -> Vec<Option<[f64; 3]>> {
        let Some(first) = data.get(start) else {
            return Vec::new();
        };
        let source = self.input.values_from(self.clock.bar_open(first.time));
        data[start..]
            .iter()
            .map(|bar| source.get(&self.clock.bar_open(bar.time)).copied())
            .collect()
    }

    /// Recomputes the outputs of `data[from..]`; the bars before `from` are unchanged.
    /// A rolling sum reads the bars it spans from before `from` and a cumulative sum resumes
    /// from the last value before it. A bar without input leaves a gap, and a rolling sum
    /// over one is a gap, as is the warm-up.
    fn refresh(&mut self, data: &[Bar], from: usize) {
        let reset_revision = self.input.reset_revision();
        let from = if reset_revision != self.reset_revision {
            0
        } else {
            from.min(self.values.len()).min(data.len())
        };
        self.reset_revision = reset_revision;
        self.candles
            .truncate(self.candles.partition_point(|c| c.bars.start < from));
        self.values.truncate(from);
        self.values
            .resize(data.len(), crate::studies::IndicatorValue::None);
        match self.kind {
            OrderFlowKind::FlowSum { bars } => {
                let start = from.saturating_sub(bars - 1);
                let rows = self.rows(data, start);
                for at in from..data.len() {
                    let Some(first) = (at + 1).checked_sub(bars) else {
                        continue;
                    };
                    let window = &rows[first - start..=at - start];
                    if window.iter().all(Option::is_some) {
                        for line in 0..2 {
                            let sum = window.iter().flatten().map(|row| row[line]).sum();
                            self.values.set_value(line, at, sum);
                        }
                    }
                }
            }
            OrderFlowKind::FilteredCvd { .. } => {
                let sessions = self.clock.sessions();
                let period =
                    |bar: &Bar| self.anchor.map(|a| a.key(sessions.trading_date(bar.time)));
                let mut last = self
                    .values
                    .line(0)
                    .get(..from)
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, v)| v.is_finite())
                    .map(|(i, v)| (period(&data[i]), *v));
                for (at, row) in self.rows(data, from).into_iter().enumerate() {
                    if let Some(row) = row {
                        let at = from + at;
                        let key = period(&data[at]);
                        let open = last
                            .filter(|(p, _)| self.delta != Some(false) && *p == key)
                            .map_or(0.0, |(_, total)| total);
                        let (high, low, close) = (open + row[1], open + row[2], open + row[0]);
                        if ![open, high, low, close].iter().all(|v| v.is_finite()) {
                            continue;
                        }
                        self.values.set_value(0, at, close);
                        if self.delta.is_some() {
                            self.values.set_value(1, at, f64::NAN);
                            self.values.set_value(2, at, f64::NAN);
                        }
                        if self.delta.is_some() && self.candle_style {
                            // A zero bar delta sells; an unchanged cumulative total keeps buying.
                            let down = if self.delta == Some(true) {
                                row[0] < 0.0
                            } else {
                                row[0] <= 0.0
                            };
                            let plot = 1 + usize::from(down);
                            self.values.set_value(plot, at, close);
                            self.candles.push(StudyCandle {
                                bars: at..at + 1,
                                bar: Bar::new(data[at].time, open, high, low, close, 0.0),
                                plot,
                            });
                        }
                        last = Some((key, close));
                    }
                }
            }
            OrderFlowKind::PriceRange { .. } => {
                let start = from.saturating_sub(self.lookback);
                let rows = self.rows(data, start);
                let range = |at: usize| rows[at - start].filter(|row| row[0].is_finite());
                let [lower, upper] = self.regime;
                for at in from..data.len() {
                    let Some(row) = range(at) else { continue };
                    let (mut below, mut count) = (0_usize, 0_usize);
                    for earlier in (at.saturating_sub(self.lookback)..at).filter_map(range) {
                        count += 1;
                        // Decimal price subtraction can leave equal ranges a few ulps apart.
                        let scale = row[1]
                            .abs()
                            .max(row[2].abs())
                            .max(earlier[1].abs().max(earlier[2].abs()));
                        if row[0] - earlier[0] > 4.0 * f64::EPSILON * scale {
                            below += 1;
                        }
                    }
                    if count == 0 {
                        continue;
                    }
                    let rank = below as f64 / count as f64 * 100.0;
                    let regime = if rank < lower {
                        -1.0
                    } else if rank > upper {
                        1.0
                    } else {
                        0.0
                    };
                    self.values.set_value(0, at, rank);
                    self.values.set_value(1, at, regime);
                }
            }
            _ => {
                for (at, row) in self.rows(data, from).into_iter().enumerate() {
                    for (line, value) in row.iter().flatten().take(self.line_count()).enumerate() {
                        self.values.set_value(line, from + at, *value);
                    }
                }
            }
        }
        if self.delta == Some(true) && self.markers.enabled() {
            let start = self.markers.start(data.len(), from);
            let rows = self.rows(data, start);
            let sessions = self.clock.sessions();
            let period = |at: usize| {
                self.anchor
                    .map(|a| a.key(sessions.trading_date(data[at].time)))
            };
            self.markers
                .refresh(data, start, &rows, &mut self.values, |at| {
                    at > 0 && period(at) != period(at - 1)
                });
        }
    }

    /// Book size or book balance over the individual orders `filter` accepts: a minimum size,
    /// minimum and maximum resting ages and excluded participants, all but the last also set by the study's
    /// inputs. Needs a feed that sends order events, see [`OrderFlowInput::with_order_filter`].
    /// Starts a fresh source.
    pub fn with_order_filter(mut self, filter: OrderFilter) -> Self {
        self.filter = filter;
        self.reset();
        self
    }

    /// A fresh source for the current kind and filter, with no values.
    fn reset(&mut self) {
        self.input = OrderFlowInput::new(self.kind)
            .with_order_filter(self.filter.clone())
            .with_size_basis(self.basis);
        self.values = IndicatorValues::new();
        self.candles.clear();
        self.markers.labels.clear();
        self.reset_revision = 0;
    }
}

impl Indicator for OrderFlowStudy {
    fn name(&self) -> &str {
        if let Some(cumulative) = self.delta {
            return if cumulative {
                "Cumulative Execution Delta"
            } else {
                "Execution Delta"
            };
        }
        match self.kind {
            OrderFlowKind::Spread => "Quoted Spread",
            OrderFlowKind::BookSize { .. } => "Book Size",
            OrderFlowKind::BookBalance { .. } => "Book Balance",
            OrderFlowKind::PriceRange { .. } => "Volatility Regime",
            OrderFlowKind::TradeRate { .. } => "Trade Rate",
            OrderFlowKind::AggressorRate { .. } => "Aggressor Rate",
            OrderFlowKind::LevelsExecuted { .. } => "Levels Executed",
            OrderFlowKind::SizeDelta { .. } => "Multi-Range Delta",
            OrderFlowKind::FlowSum { .. } => "Rolling Volume and Delta",
            OrderFlowKind::FilteredCvd { .. } => "Size-Filtered CVD",
            OrderFlowKind::LevelsTradedThrough { .. } => "Levels Traded Through",
        }
    }
    fn desc(&self) -> &str {
        "Exact stream aggregation; missing market data leaves gaps"
    }
    fn order_flow_inputs(&self) -> &[OrderFlowInput] {
        std::slice::from_ref(&self.input)
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
    }
    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        self.volume_precision = instrument.and_then(|i| i.volume_decimals.map(usize::from));
        false
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.refresh(data, 0);
    }
    fn is_incremental(&self) -> bool {
        true
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        self.refresh(data, from);
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn candles(&self) -> &[StudyCandle] {
        &self.candles
    }
    fn labels(&self) -> &[crate::studies::StudyLabel] {
        &self.markers.labels
    }
    fn labels_on_price(&self) -> bool {
        self.delta == Some(true)
    }
    fn value_format(&self) -> crate::scales::StudyFormat {
        use crate::scales::StudyFormat;
        match self.kind {
            OrderFlowKind::BookBalance { .. }
            | OrderFlowKind::Spread
            | OrderFlowKind::TradeRate { volume: false, .. }
            | OrderFlowKind::AggressorRate { .. } => StudyFormat::Fixed(2),
            OrderFlowKind::LevelsExecuted { .. }
            | OrderFlowKind::LevelsTradedThrough { .. }
            | OrderFlowKind::PriceRange { .. } => StudyFormat::Fixed(0),
            _ => StudyFormat::Volume,
        }
    }
    fn precision(&self) -> Option<usize> {
        if matches!(
            self.kind,
            OrderFlowKind::TradeRate { volume: false, .. } | OrderFlowKind::AggressorRate { .. }
        ) {
            return None;
        }
        if matches!(self.kind, OrderFlowKind::TradeRate { volume: true, .. }) {
            return self.volume_precision.map(|decimals| decimals.max(2));
        }
        match self.value_format() {
            crate::scales::StudyFormat::Volume => self.volume_precision,
            crate::scales::StudyFormat::Fixed(decimals) => Some(decimals),
            _ => None,
        }
    }
    fn format_value(&self, value: f64) -> String {
        use crate::scales::StudyFormat;
        let format = self.value_format();
        let precision = self
            .precision()
            .or((format == StudyFormat::Volume && self.delta.is_none()).then_some(2));
        match precision {
            Some(precision) if format == StudyFormat::Volume => volume_text(value, precision),
            _ => format.format(value, precision),
        }
    }
    fn format_plot_value(&self, line: usize, value: f64) -> String {
        use crate::scales::StudyFormat;
        if line == 1 && matches!(self.kind, OrderFlowKind::PriceRange { .. }) {
            return if value < 0.0 {
                "Low"
            } else if value > 0.0 {
                "High"
            } else {
                "Normal"
            }
            .to_owned();
        }
        if line == 0 {
            match self.kind {
                OrderFlowKind::TradeRate { volume: true, .. } => {
                    return volume_text(value, self.volume_precision.unwrap_or(2));
                }
                OrderFlowKind::TradeRate { volume: false, .. }
                | OrderFlowKind::AggressorRate { .. } => {
                    return StudyFormat::Fixed(0).format(value, None);
                }
                _ => {}
            }
        }
        self.format_value(value)
    }
    fn legend_visible(&self, line: usize) -> bool {
        line < self.line_count() && (self.plot_style(line).visible || line == 0)
    }
    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        // The per-bar histogram takes its candles' colours: a balanced bar sells.
        if line != 0 || self.delta != Some(false) || self.candle_style {
            return None;
        }
        let delta = self.values.value(0, bar)?;
        Some(self.colors[1 + usize::from(delta <= 0.0)])
    }
    fn is_overlay(&self) -> bool {
        false
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
    fn clone_for_export(&self) -> Box<dyn Indicator> {
        let mut frozen = self.clone();
        frozen.input = self.input.frozen();
        Box::new(frozen)
    }
    fn line_count(&self) -> usize {
        if self.delta.is_some() {
            return if self.markers.enabled() { 7 } else { 3 };
        }
        match self.kind {
            OrderFlowKind::FlowSum { .. }
            | OrderFlowKind::BookBalance { .. }
            | OrderFlowKind::PriceRange { .. } => 2,
            OrderFlowKind::FilteredCvd { .. } => 1,
            _ => 3,
        }
    }
    fn line_names(&self) -> Vec<String> {
        if self.delta.is_some() {
            let mut names = ["Delta", "Up", "Down"].map(String::from).to_vec();
            if self.markers.enabled() {
                names.extend(
                    [
                        "Bull divergence",
                        "Bear divergence",
                        "Bull exhaustion",
                        "Bear exhaustion",
                    ]
                    .map(String::from),
                );
            }
            return names;
        }
        let names: &[&str] = match self.kind {
            OrderFlowKind::TradeRate {
                volume,
                seconds,
                instant,
            } => {
                let unit = if volume { "lots" } else { "executions" };
                return vec![
                    format!("Bar {unit}"),
                    format!("Rolling {seconds}s ({unit}/s)"),
                    format!("Instant {instant}s ({unit}/s)"),
                ];
            }
            OrderFlowKind::AggressorRate { seconds, instant } => {
                return vec![
                    "Bar orders".into(),
                    format!("Rolling {seconds}s (orders/s)"),
                    format!("Instant {instant}s (orders/s)"),
                ];
            }
            OrderFlowKind::SizeDelta { ranges } => {
                return ranges
                    .iter()
                    .map(|[min, limit]| {
                        if *limit == f64::MAX {
                            format!("Delta [{min}, ∞) lots")
                        } else {
                            format!("Delta [{min}, {limit}) lots")
                        }
                    })
                    .collect();
            }
            OrderFlowKind::FlowSum { .. } => &["Volume sum (lots)", "Delta sum (lots)"],
            OrderFlowKind::FilteredCvd { .. } => &["CVD (lots)"],
            OrderFlowKind::Spread => &["Last (ticks)", "Mean (ticks)", "Maximum (ticks)"],
            OrderFlowKind::BookSize { .. } => {
                &["Bid size (lots)", "Ask size (lots)", "Difference (lots)"]
            }
            OrderFlowKind::BookBalance { .. } => &["Near balance (%)", "Far balance (%)"],
            OrderFlowKind::PriceRange { .. } => &["Range percentile", "Regime"],
            OrderFlowKind::LevelsExecuted { .. } => &["Ask levels", "Bid levels", "Total levels"],
            OrderFlowKind::LevelsTradedThrough { .. } => {
                &["Bid levels", "Ask levels", "Total levels"]
            }
        };
        names.iter().map(|name| (*name).to_owned()).collect()
    }
    fn readout_only(&self, line: usize) -> bool {
        line == 1 && matches!(self.kind, OrderFlowKind::PriceRange { .. })
    }
    fn plot_tagged(&self, line: usize) -> bool {
        line < 3 && !self.readout_only(line)
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors[line.min(2)],
            plot_type: if self.delta == Some(false)
                || (line == 2 && matches!(self.kind, OrderFlowKind::BookSize { .. }))
                || (self.histogram && matches!(self.kind, OrderFlowKind::FlowSum { .. }))
            {
                PlotType::Histogram
            } else {
                PlotType::Line
            },
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            visible: (self.delta != Some(true) || line < 3)
                && !(line == 0
                    && matches!(
                        self.kind,
                        OrderFlowKind::TradeRate { .. } | OrderFlowKind::AggressorRate { .. }
                    )),
        }
    }
    fn pane(&self) -> crate::studies::StudyPane {
        use crate::studies::{StudyPane, palette::MUTED};
        match self.kind {
            OrderFlowKind::BookBalance { .. } => StudyPane {
                height: None,
                range: Some((-100.0, 100.0)),
                levels: vec![(0.0, MUTED, "Balanced")],
                fill_between_levels: false,
            },
            OrderFlowKind::PriceRange { .. } => StudyPane {
                height: None,
                range: Some((0.0, 100.0)),
                levels: vec![
                    (self.regime[1], MUTED, "High"),
                    (self.regime[0], MUTED, "Low"),
                ],
                fill_between_levels: true,
            },
            _ => StudyPane::default(),
        }
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let length = |id, label, value, min, max| {
            StudyInput::new(
                id,
                label,
                InputValue::Length(value),
                InputKind::Length(min, max),
            )
        };
        let order_filter = || {
            [
                StudyInput::new(
                    "min_size",
                    "Minimum order size (needs order events)",
                    InputValue::Number(self.filter.min_size),
                    InputKind::Number(0.0, f64::MAX),
                ),
                StudyInput::new(
                    "min_age",
                    "Minimum resting seconds (0 for any, needs order events)",
                    InputValue::Number(self.filter.min_age.map_or(0.0, |age| age.as_seconds_f64())),
                    InputKind::Number(0.0, 86400.0),
                ),
                StudyInput::new(
                    "max_age",
                    "Maximum resting seconds (0 for any, needs order events)",
                    InputValue::Number(self.filter.max_age.map_or(0.0, |age| age.as_seconds_f64())),
                    InputKind::Number(0.0, 86400.0),
                ),
            ]
        };
        let combine = StudyInput::new(
            "combine_fills",
            "Combine fills of an aggressive order",
            InputValue::Toggle(self.basis == SizeBasis::AggressiveOrder),
            InputKind::Toggle,
        )
        .with_default(InputValue::Toggle(false));
        match self.kind {
            OrderFlowKind::Spread => vec![],
            OrderFlowKind::BookSize { levels, distance } => {
                let mut inputs = vec![
                    length("levels", "Nearest levels", levels, 1, 10000),
                    StudyInput::new(
                        "distance",
                        "Price distance (0 uses levels)",
                        InputValue::Number(distance),
                        InputKind::Number(0.0, f64::MAX),
                    ),
                ];
                inputs.extend(order_filter());
                inputs
            }
            OrderFlowKind::BookBalance {
                near_levels,
                far_levels,
                near_distance,
                far_distance,
            } => {
                let mut inputs = vec![
                    length("near_levels", "Near levels", near_levels, 1, 10000),
                    length(
                        "far_levels",
                        "Far levels beyond the near band",
                        far_levels,
                        1,
                        10000,
                    ),
                    StudyInput::new(
                        "near_distance",
                        "Near price distance (0 uses levels)",
                        InputValue::Number(near_distance),
                        InputKind::Number(0.0, f64::MAX),
                    ),
                    StudyInput::new(
                        "far_distance",
                        "Far price distance beyond the near band",
                        InputValue::Number(far_distance),
                        InputKind::Number(0.0, f64::MAX),
                    ),
                ];
                inputs.extend(order_filter());
                inputs
            }
            OrderFlowKind::PriceRange { seconds } => vec![
                length("seconds", "Window seconds", seconds, 1, 86400),
                length("lookback", "Lookback bars", self.lookback, 1, 1000)
                    .with_default(InputValue::Length(100)),
                StudyInput::new(
                    "lower",
                    "Low regime below percentile",
                    InputValue::Number(self.regime[0]),
                    InputKind::Number(0.0, 100.0),
                )
                .with_default(InputValue::Number(20.0)),
                StudyInput::new(
                    "upper",
                    "High regime above percentile",
                    InputValue::Number(self.regime[1]),
                    InputKind::Number(0.0, 100.0),
                )
                .with_default(InputValue::Number(80.0)),
            ],
            OrderFlowKind::TradeRate {
                volume,
                seconds,
                instant,
            } => vec![
                StudyInput::new(
                    "volume",
                    "Count volume",
                    InputValue::Toggle(volume),
                    InputKind::Toggle,
                ),
                length("seconds", "Rolling seconds", seconds, 1, 86400),
                length("instant", "Instant seconds", instant, 1, 86400),
            ],
            OrderFlowKind::SizeDelta { ranges } => {
                let size = |id: &str, label: String, value| {
                    StudyInput::new(
                        id,
                        label,
                        InputValue::Number(value),
                        InputKind::Number(0.0, f64::MAX),
                    )
                };
                let mut inputs: Vec<_> = ranges
                    .iter()
                    .enumerate()
                    .flat_map(|(i, [min, limit])| {
                        [
                            size(
                                &format!("min{i}"),
                                format!("Range {} smallest size (included)", i + 1),
                                *min,
                            ),
                            size(
                                &format!("limit{i}"),
                                format!("Range {} size limit (excluded)", i + 1),
                                *limit,
                            ),
                        ]
                    })
                    .collect();
                inputs.push(combine);
                inputs
            }
            OrderFlowKind::FlowSum { bars } => vec![
                length("bars", "Bars", bars, 1, 10_000),
                StudyInput::new(
                    "histogram",
                    "Histogram",
                    InputValue::Toggle(self.histogram),
                    InputKind::Toggle,
                ),
            ],
            OrderFlowKind::FilteredCvd { min, max } => {
                let mut inputs = [("min", "Smallest size", min), ("max", "Largest size", max)]
                    .map(|(id, label, value)| {
                        StudyInput::new(
                            id,
                            label,
                            InputValue::Number(value),
                            InputKind::Number(0.0, f64::MAX),
                        )
                    })
                    .to_vec();
                inputs.push(combine);
                if self.delta != Some(false) {
                    inputs.push(
                        StudyInput::new(
                            "anchor",
                            "Reset period",
                            InputValue::Choice(
                                ANCHORS.iter().position(|a| *a == self.anchor).unwrap_or(0),
                            ),
                            InputKind::Choice(
                                ANCHORS
                                    .iter()
                                    .map(|a| {
                                        a.map_or_else(
                                            || "Continuous".to_owned(),
                                            |a| format!("{a:?}"),
                                        )
                                    })
                                    .collect(),
                            ),
                        )
                        .with_default(InputValue::Choice(usize::from(self.delta == Some(true)))),
                    );
                }
                if self.delta.is_some() {
                    inputs.push(
                        StudyInput::new(
                            "representation",
                            "Representation",
                            InputValue::Choice(usize::from(!self.candle_style)),
                            InputKind::Choice(vec![
                                "Candles".to_owned(),
                                if self.delta == Some(false) {
                                    "Histogram"
                                } else {
                                    "Line"
                                }
                                .to_owned(),
                            ]),
                        )
                        .with_default(InputValue::Choice(0)),
                    );
                }
                if self.delta == Some(true) {
                    inputs.extend([
                        StudyInput::new(
                            "divergence",
                            "Divergence",
                            InputValue::Toggle(self.markers.divergence),
                            InputKind::Toggle,
                        )
                        .with_default(InputValue::Toggle(false)),
                        length(
                            "swing_bars",
                            "Swing bars on each side",
                            self.markers.swing_bars,
                            1,
                            10_000,
                        )
                        .with_default(InputValue::Length(3)),
                        StudyInput::new(
                            "exhaustion",
                            "Exhaustion",
                            InputValue::Toggle(self.markers.exhaustion),
                            InputKind::Toggle,
                        )
                        .with_default(InputValue::Toggle(false)),
                        length(
                            "exhaustion_bars",
                            "Exhaustion consecutive bars",
                            self.markers.exhaustion_bars,
                            2,
                            10_000,
                        )
                        .with_default(InputValue::Length(3)),
                    ]);
                }
                inputs
            }
            OrderFlowKind::AggressorRate { seconds, instant } => vec![
                length("seconds", "Rolling seconds", seconds, 1, 86400),
                length("instant", "Instant seconds", instant, 1, 86400),
            ],
            OrderFlowKind::LevelsExecuted { seconds, reversal } => vec![
                length("seconds", "Window seconds (0 uses bar)", seconds, 0, 86400),
                StudyInput::new(
                    "reversal",
                    "Restart on price reversal",
                    InputValue::Toggle(reversal),
                    InputKind::Toggle,
                ),
            ],
            OrderFlowKind::LevelsTradedThrough { seconds } => vec![length(
                "seconds",
                "Window seconds (0 uses bar)",
                seconds,
                0,
                86400,
            )],
        }
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        if !self
            .inputs()
            .iter()
            .any(|i| i.id == id && i.accepts(&value))
        {
            return Err(crate::studies::StudyError::rejected(&self.inputs(), id));
        }
        match (id, &value) {
            ("lookback", InputValue::Length(n)) => self.lookback = *n,
            ("lower", InputValue::Number(v)) => self.regime[0] = *v,
            ("upper", InputValue::Number(v)) => self.regime[1] = *v,
            ("divergence", InputValue::Toggle(on)) => self.markers.divergence = *on,
            ("exhaustion", InputValue::Toggle(on)) => self.markers.exhaustion = *on,
            ("swing_bars", InputValue::Length(n)) => self.markers.swing_bars = *n,
            ("exhaustion_bars", InputValue::Length(n)) => self.markers.exhaustion_bars = *n,
            _ => {}
        }
        if matches!(
            id,
            "divergence"
                | "exhaustion"
                | "swing_bars"
                | "exhaustion_bars"
                | "lookback"
                | "lower"
                | "upper"
        ) {
            self.values = IndicatorValues::new();
            self.candles.clear();
            self.markers.labels.clear();
            return Ok(());
        }
        let before = self.kind;
        if let InputValue::Choice(at) = &value {
            match id {
                "anchor" => self.anchor = ANCHORS[*at],
                "representation" => self.candle_style = *at == 0,
                _ => return Err(crate::studies::StudyError::invalid_input(id)),
            }
            self.values = IndicatorValues::new();
            self.candles.clear();
            self.markers.labels.clear();
            return Ok(());
        }
        if let ("histogram", InputValue::Toggle(on)) = (id, &value) {
            self.histogram = *on;
            return Ok(());
        }
        if let ("combine_fills", InputValue::Toggle(on)) = (id, &value) {
            self.basis = if *on {
                SizeBasis::AggressiveOrder
            } else {
                SizeBasis::Fill
            };
            self.reset();
            return Ok(());
        }
        match (&mut self.kind, id, value) {
            (OrderFlowKind::SizeDelta { ranges }, id, InputValue::Number(v)) => {
                let (bound, index) = match id.strip_prefix("min") {
                    Some(index) => (0, index),
                    None => (1, id.trim_start_matches("limit")),
                };
                let index = index
                    .parse::<usize>()
                    .map_err(|_| crate::studies::StudyError::invalid_input(id))?;
                ranges[index][bound] = v;
            }
            (OrderFlowKind::FlowSum { bars }, "bars", InputValue::Length(v)) => *bars = v,
            (OrderFlowKind::FilteredCvd { min, .. }, "min", InputValue::Number(v)) => *min = v,
            (OrderFlowKind::FilteredCvd { max, .. }, "max", InputValue::Number(v)) => *max = v,
            (OrderFlowKind::BookSize { levels, .. }, "levels", InputValue::Length(v)) => {
                *levels = v
            }
            (OrderFlowKind::BookSize { distance, .. }, "distance", InputValue::Number(v)) => {
                *distance = v
            }
            (
                OrderFlowKind::BookBalance { near_levels, .. },
                "near_levels",
                InputValue::Length(v),
            ) => *near_levels = v,
            (
                OrderFlowKind::BookBalance { far_levels, .. },
                "far_levels",
                InputValue::Length(v),
            ) => *far_levels = v,
            (
                OrderFlowKind::BookBalance { near_distance, .. },
                "near_distance",
                InputValue::Number(v),
            ) => *near_distance = v,
            (
                OrderFlowKind::BookBalance { far_distance, .. },
                "far_distance",
                InputValue::Number(v),
            ) => *far_distance = v,
            (
                OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. },
                "min_size",
                InputValue::Number(v),
            ) => {
                self.filter.min_size = v;
                self.reset();
            }
            (
                OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. },
                "min_age",
                InputValue::Number(v),
            ) => {
                self.filter.min_age = (v > 0.0).then(|| TimeDelta::milliseconds((v * 1e3) as i64));
                self.reset();
            }
            (
                OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. },
                "max_age",
                InputValue::Number(v),
            ) => {
                self.filter.max_age = (v > 0.0).then(|| TimeDelta::milliseconds((v * 1e3) as i64));
                self.reset();
            }
            (OrderFlowKind::TradeRate { volume, .. }, "volume", InputValue::Toggle(v)) => {
                *volume = v
            }
            (
                OrderFlowKind::TradeRate { seconds, .. }
                | OrderFlowKind::AggressorRate { seconds, .. }
                | OrderFlowKind::LevelsExecuted { seconds, .. }
                | OrderFlowKind::LevelsTradedThrough { seconds }
                | OrderFlowKind::PriceRange { seconds },
                "seconds",
                InputValue::Length(v),
            ) => *seconds = v,
            (
                OrderFlowKind::TradeRate { instant, .. }
                | OrderFlowKind::AggressorRate { instant, .. },
                "instant",
                InputValue::Length(v),
            ) => *instant = v,
            (OrderFlowKind::LevelsExecuted { reversal, .. }, "reversal", InputValue::Toggle(v)) => {
                *reversal = v
            }
            _ => return Err(crate::studies::StudyError::invalid_input(id)),
        }
        if self.kind != before {
            self.reset();
        }
        Ok(())
    }
}

/// `value` in volume units at `precision` decimals, with K, M and B values at no fewer than two,
/// as studies write volumes with instrument volume decimals.
fn volume_text(value: f64, precision: usize) -> String {
    use crate::scales::{StudyFormat, VolumeFormatter};
    if value.is_finite() {
        // Capped as `StudyFormat` caps study decimals.
        VolumeFormatter {
            precision: precision.min(12),
        }
        .format_with_min_unit_decimals(value, 2)
    } else {
        StudyFormat::Volume.format(value, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Footprint;
    use crate::model::{OrderSide, Tick, Timeframe};
    use chrono::{DateTime, TimeDelta, Utc};

    const NAN: f64 = f64::NAN;

    fn start() -> DateTime<Utc> {
        "2026-01-05T10:00:00Z".parse().unwrap()
    }

    fn bars() -> Vec<Bar> {
        (0..6)
            .map(|i| Bar::new(start() + TimeDelta::minutes(i), 1.0, 1.0, 1.0, 1.0, 0.0))
            .collect()
    }

    fn push(study: &OrderFlowStudy, minute: i64, trades: &[(f64, Option<OrderSide>)]) {
        let input = &study.order_flow_inputs()[0];
        for (second, &(size, side)) in trades.iter().enumerate() {
            let at = start() + TimeDelta::minutes(minute) + TimeDelta::seconds(second as i64);
            input.push_trade(&study.clock, &Tick::new(at, 100.0, size, side));
        }
    }

    /// The same trades per bar (minute 2 has none): 7 and +3, 20 and +20, then 153 and -147,
    /// 104 and +100 with 4 of unknown side, and 12 and -12.
    fn fill(study: &OrderFlowStudy) {
        use OrderSide::{Buy, Sell};
        push(study, 0, &[(5.0, Some(Buy)), (2.0, Some(Sell))]);
        push(study, 1, &[(20.0, Some(Buy))]);
        push(study, 3, &[(150.0, Some(Sell)), (3.0, Some(Buy))]);
        push(study, 4, &[(100.0, Some(Buy)), (4.0, None)]);
        push(study, 5, &[(12.0, Some(Sell))]);
    }

    fn lines(study: &OrderFlowStudy) -> Vec<Vec<f64>> {
        study.values().columns().to_vec()
    }

    /// Guards what the trade-size filters measure (#3197): each fill alone, or the combined fills of
    /// its aggressive order, the fills of one aggressor order id at one timestamp, counted at their
    /// own prices. Expected values by hand from the tape below: per order, orders 1 and 2 and
    /// the single sell reach 50; an order growing past 49 is taken back with the delta path its
    /// fills made; the two 30-lot fills without an id stay apart; id 1 at 50 s is another order.
    #[test]
    fn size_filters_measure_single_fills_or_aggressive_orders() {
        let (buy, sell) = (Some(OrderSide::Buy), Some(OrderSide::Sell));
        let ten = start();
        let fill = |second: i64, price: f64, size: f64, side, id: Option<u64>| {
            let tick = Tick::new(ten + TimeDelta::seconds(second), price, size, side);
            match id {
                Some(id) => tick.with_aggressor_order_id(id),
                None => tick,
            }
        };
        let mut tape: Vec<Tick> = [100.0, 101.0, 102.0, 103.0, 104.0]
            .map(|price| fill(10, price, 10.0, buy, Some(1)))
            .to_vec();
        tape.extend([
            fill(20, 100.0, 50.0, sell, None),
            fill(30, 100.0, 20.0, buy, Some(2)),
            fill(30, 101.0, 30.0, sell, Some(3)),
            fill(30, 102.0, 30.0, buy, Some(2)),
            fill(30, 103.0, 10.0, sell, Some(3)),
            fill(40, 100.0, 30.0, buy, None),
            fill(40, 100.0, 30.0, buy, None),
            fill(50, 104.0, 40.0, sell, Some(1)),
        ]);
        let clock = BarClock::new(Timeframe::Min1);
        // Rows: (level, ask, ask trades, bid, bid trades); footprint (min_delta, max_delta); CVD
        // [delta, high, low]; Multi-Range Delta over [0, 50), [50, ∞) and [20, 21).
        #[rustfmt::skip]
        let cases = [
            ("per fill only the single 50-lot sell reaches 50", false, [50.0, f64::MAX],
                vec![(100, 0.0, 0, 50.0, 1)], (-50.0, 0.0), [-50.0, 0.0, -50.0], [80.0, -50.0, 20.0]),
            ("per order orders 1 and 2 and the single sell reach 50, at their fills' prices", true, [50.0, f64::MAX],
                vec![(100, 30.0, 2, 50.0, 1), (101, 10.0, 1, 0.0, 0), (102, 40.0, 2, 0.0, 0),
                    (103, 10.0, 1, 0.0, 0), (104, 10.0, 1, 0.0, 0)],
                (0.0, 50.0), [50.0, 50.0, 0.0], [-20.0, 50.0, 0.0]),
            ("an order growing past the maximum is taken back with its part of the delta path", true, [0.0, 49.0],
                vec![(100, 60.0, 2, 0.0, 0), (101, 0.0, 0, 30.0, 1), (103, 0.0, 0, 10.0, 1), (104, 0.0, 0, 40.0, 1)],
                (-40.0, 20.0), [-20.0, 20.0, -40.0], [-20.0, 50.0, 0.0]),
        ];
        for (name, combine, [min, max], rows, extremes, cvd, ranges) in cases {
            let mut flow = Footprint::new(clock.clone(), 1.0, 1).unwrap();
            flow.set_size_filter(min, max).unwrap();
            if combine {
                flow.set_size_basis(SizeBasis::AggressiveOrder).unwrap();
            }
            let mut studies = [
                OrderFlowStudy::new(OrderFlowKind::FilteredCvd { min, max }),
                OrderFlowStudy::new(OrderFlowKind::SizeDelta {
                    ranges: [[0.0, 50.0], [50.0, f64::MAX], [20.0, 21.0]],
                }),
            ];
            for study in &mut studies {
                study
                    .set_input("combine_fills", InputValue::Toggle(combine))
                    .unwrap();
            }
            for (i, trade) in tape.iter().enumerate() {
                flow.push(trade).unwrap();
                for study in &studies {
                    study.order_flow_inputs()[0].push_trade(&clock, trade);
                }
                // At 20 and 30 lots only the third range changes membership: its whole
                // order counts at 20, and none of it at 30, before range one changes.
                if combine && matches!(i, 1 | 2) {
                    let expected = if i == 1 {
                        [20.0, 0.0, 20.0]
                    } else {
                        [30.0, 0.0, 0.0]
                    };
                    assert_eq!(
                        studies[1].order_flow_inputs()[0].values()[&ten],
                        expected,
                        "{name}, fill {i}"
                    );
                }
            }
            let bar = &flow.bars()[&ten];
            let got: Vec<_> = (bar.levels.values())
                .map(|r| (r.level, r.ask, r.ask_trades, r.bid, r.bid_trades))
                .collect();
            assert_eq!(got, rows, "{name}");
            assert_eq!((bar.min_delta, bar.max_delta), extremes, "{name}");
            let values = studies.map(|study| study.order_flow_inputs()[0].values()[&ten]);
            assert_eq!(values, [cvd, ranges], "{name}");
        }
        // A basis change applies only to future fills. Old order parts must not undo a
        // fill counted in between under another basis, even at the same timestamp.
        let input = OrderFlowInput::new(OrderFlowKind::FilteredCvd {
            min: 50.0,
            max: f64::MAX,
        })
        .with_size_basis(SizeBasis::AggressiveOrder);
        input.push_trade(&clock, &fill(10, 100.0, 20.0, buy, Some(7)));
        let input = input.with_size_basis(SizeBasis::Fill);
        input.push_trade(&clock, &fill(10, 101.0, 50.0, sell, None));
        let input = input.with_size_basis(SizeBasis::AggressiveOrder);
        input.push_trade(&clock, &fill(10, 102.0, 30.0, buy, Some(7)));
        assert_eq!(input.values()[&ten], [-50.0, 0.0, -50.0]);
    }

    #[test]
    fn trade_rate_export_keeps_its_input_while_an_ordinary_clone_follows_trades() {
        let study = OrderFlowStudy::new(OrderFlowKind::TradeRate {
            volume: false,
            seconds: 2,
            instant: 1,
        });
        let fill = Tick::new(start(), 100.0, 7.0, None);
        study
            .input
            .push_trade(&study.clock, &fill.clone().with_id(1));
        let mut export = study.clone_for_export();
        let mut live = study.clone_box();
        study.input.push_trade(&study.clock, &fill.with_id(2));

        let bars = [Bar::new(start(), 100.0, 100.0, 100.0, 100.0, 7.0)];
        for (copy, executions) in [(&mut export, 1.0), (&mut live, 2.0)] {
            copy.calculate(&bars);
            assert_eq!(copy.values().value(0, 0), Some(executions));
            assert_eq!(copy.values().value(1, 0), Some(executions / 2.0));
            assert_eq!(copy.values().value(2, 0), Some(executions));
        }
    }

    /// Counts are integral; quantities use the instrument's lot precision, while rates and
    /// mean tick spreads retain fractions. The configured chart presentation preserves this.
    #[test]
    fn count_and_quantity_formats_follow_their_units() {
        use crate::model::Instrument;
        use crate::studies::ConfiguredStudy;
        let unconfigured = OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
            min: 0.0,
            max: f64::MAX,
        });
        assert_eq!(unconfigured.format_value(18.125), "18.12");
        for (kind, value, whole, fractional) in [
            (
                OrderFlowKind::LevelsExecuted {
                    seconds: 0,
                    reversal: false,
                },
                18.0,
                "18",
                "18",
            ),
            (
                OrderFlowKind::LevelsTradedThrough { seconds: 60 },
                18.0,
                "18",
                "18",
            ),
            (
                OrderFlowKind::FilteredCvd {
                    min: 0.0,
                    max: f64::MAX,
                },
                18.125,
                "18",
                "18.125",
            ),
            (
                OrderFlowKind::SizeDelta {
                    ranges: [[0.0, 10.0]; 3],
                },
                18.125,
                "18",
                "18.125",
            ),
            (
                OrderFlowKind::BookSize {
                    levels: 10,
                    distance: 0.0,
                },
                18.125,
                "18",
                "18.125",
            ),
            (OrderFlowKind::FlowSum { bars: 2 }, 18.125, "18", "18.125"),
            (
                OrderFlowKind::BookBalance {
                    near_levels: 5,
                    far_levels: 10,
                    near_distance: 0.0,
                    far_distance: 0.0,
                },
                1.25,
                "1.25",
                "1.25",
            ),
            (OrderFlowKind::PriceRange { seconds: 60 }, 1.25, "1", "1"),
            (OrderFlowKind::Spread, 1.25, "1.25", "1.25"),
            (
                OrderFlowKind::TradeRate {
                    volume: false,
                    seconds: 60,
                    instant: 5,
                },
                1.25,
                "1.25",
                "1.25",
            ),
            (
                OrderFlowKind::AggressorRate {
                    seconds: 60,
                    instant: 5,
                },
                1.25,
                "1.25",
                "1.25",
            ),
            (
                OrderFlowKind::TradeRate {
                    volume: true,
                    seconds: 60,
                    instant: 5,
                },
                1.125,
                "1.12",
                "1.125",
            ),
        ] {
            let mut study = ConfiguredStudy::new(Box::new(OrderFlowStudy::new(kind)));
            for (decimals, want) in [(0, whole), (3, fractional)] {
                study.set_instrument(Some(
                    &Instrument::new("SAMPLE").with_volume_decimals(decimals),
                ));
                assert_eq!(study.format_value(value), want, "{kind:?}, {decimals}");
            }
            if matches!(kind, OrderFlowKind::TradeRate { volume: false, .. }) {
                study.set_input("volume", InputValue::Toggle(true)).unwrap();
                assert_eq!(study.format_value(1.125), "1.125");
            }
        }
    }

    fn assert_lines<const N: usize>(study: &OrderFlowStudy, want: &[[f64; N]]) {
        let got = lines(study);
        assert_eq!(got.len(), want.len());
        for (got, want) in got.iter().zip(want) {
            assert_eq!(got.len(), want.len());
            for (got, want) in got.iter().zip(want) {
                assert!(
                    got == want || (got.is_nan() && want.is_nan()),
                    "{got:?} against {want:?}"
                );
            }
        }
    }

    /// The near 30/10 and far 10/30 lots give +50% and -50%. Decimal edges belong
    /// only to the near band; absent, lost and empty books have no value.
    #[test]
    fn book_balance_reads_near_and_far_bands_without_double_counting() {
        use crate::model::{OrderBook, OrderBookLevel};
        let book = OrderBook {
            ts: start(),
            symbol: "SAMPLE".into(),
            bids: [
                (100.0, 20.0),
                (99.9, 10.0),
                (99.8, 6.0),
                (99.7, 4.0),
                (99.6, 50.0),
            ]
            .map(|(price, size)| OrderBookLevel {
                price,
                size,
                order_cnt: None,
            })
            .to_vec(),
            asks: [
                (100.1, 4.0),
                (100.2, 6.0),
                (100.3, 25.0),
                (100.4, 5.0),
                (100.5, 50.0),
            ]
            .map(|(price, size)| OrderBookLevel {
                price,
                size,
                order_cnt: None,
            })
            .to_vec(),
        };
        for (name, near_levels, far_levels, near_distance, far_distance) in [
            ("levels", 2, 2, 0.0, 0.0),
            ("decimal distance edges", 1, 1, 0.1, 0.25),
        ] {
            let mut study = OrderFlowStudy::new(OrderFlowKind::BookBalance {
                near_levels,
                far_levels,
                near_distance,
                far_distance,
            });
            assert_eq!(study.line_count(), 2, "{name}");
            assert_eq!(study.pane().range, Some((-100.0, 100.0)), "{name}");
            for minute in [0, 2] {
                study.input.push_book(
                    &study.clock,
                    start() + TimeDelta::minutes(minute) + TimeDelta::seconds(5),
                    Some(&book),
                );
            }
            study.input.push_book(
                &study.clock,
                start() + TimeDelta::minutes(2) + TimeDelta::seconds(10),
                None,
            );
            study.input.push_book(
                &study.clock,
                start() + TimeDelta::minutes(3) + TimeDelta::seconds(5),
                Some(&OrderBook {
                    ts: start(),
                    symbol: "SAMPLE".into(),
                    bids: vec![],
                    asks: vec![],
                }),
            );
            study.calculate(&bars());
            assert_lines(
                &study,
                &[
                    [50.0, NAN, NAN, NAN, NAN, NAN],
                    [-50.0, NAN, NAN, NAN, NAN, NAN],
                ],
            );
        }
    }

    /// Four quiet windows tie despite decimal rounding; the wider window ranks 100.
    /// Ranks exactly on either boundary are normal, and incremental refresh sees earlier bars.
    #[test]
    fn volatility_regime_ranks_window_ranges_against_earlier_bars() {
        let mut study = OrderFlowStudy::new(OrderFlowKind::PriceRange { seconds: 30 });
        for (id, value) in [
            ("lookback", InputValue::Length(4)),
            ("lower", InputValue::Number(25.0)),
            ("upper", InputValue::Number(75.0)),
        ] {
            study.set_input(id, value).unwrap();
        }
        let data: Vec<_> = (0..9)
            .map(|m| Bar::new(start() + TimeDelta::minutes(m), 1.0, 1.0, 1.0, 1.0, 0.0))
            .collect();
        for (m, (first, second)) in [
            (100.1, 100.2),
            (100.2, 100.3),
            (100.3, 100.4),
            (100.4, 100.5),
            (100.5, 100.6),
            (100.6, 101.1),
            (101.1, 100.8),
            (100.8, 101.0),
            (101.0, 100.85),
        ]
        .into_iter()
        .enumerate()
        {
            for (seconds, price) in [(10, first), (20, second)] {
                study.input.push_trade(
                    &study.clock,
                    &Tick::new(
                        start() + TimeDelta::minutes(m as i64) + TimeDelta::seconds(seconds),
                        price,
                        1.0,
                        None,
                    ),
                );
            }
        }
        let expected = [
            [NAN, 0.0, 0.0, 0.0, 0.0, 100.0, 75.0, 50.0, 25.0],
            [NAN, -1.0, -1.0, -1.0, -1.0, 1.0, 0.0, 0.0, 0.0],
        ];
        study.calculate(&data);
        assert_lines(&study, &expected);
        let mut part = study.clone();
        part.calculate(&data[..6]);
        part.update(&data, 6);
        assert_lines(&part, &expected);
        for (value, word) in [(-1.0, "Low"), (0.0, "Normal"), (1.0, "High")] {
            assert_eq!(study.format_plot_value(1, value), word);
        }
        assert!(study.readout_only(1));
        assert!(!study.plot_tagged(1));
        assert_eq!(study.pane().range, Some((0.0, 100.0)));
        assert_eq!(
            study
                .pane()
                .levels
                .iter()
                .map(|level| level.0)
                .collect::<Vec<_>>(),
            [75.0, 25.0]
        );
        // Ranking inputs reuse the tape; a recalculation still has the source's ranges.
        study.set_input("lookback", InputValue::Length(1)).unwrap();
        study.calculate(&data);
        assert_eq!(study.values().value(0, 7), Some(0.0));
    }

    /// Non-default values must survive the same template path used by saved chart layouts.
    #[test]
    fn book_balance_and_volatility_regime_inputs_survive_templates() {
        use crate::studies::{IndicatorFactory, StudyTemplate};
        let factory = IndicatorFactory::new();
        for (name, edits) in [
            (
                "Book Balance",
                vec![
                    ("near_levels", InputValue::Length(3)),
                    ("far_levels", InputValue::Length(7)),
                    ("near_distance", InputValue::Number(0.5)),
                    ("far_distance", InputValue::Number(1.25)),
                    ("min_size", InputValue::Number(2.0)),
                    ("min_age", InputValue::Number(1.5)),
                    ("max_age", InputValue::Number(30.0)),
                ],
            ),
            (
                "Volatility Regime",
                vec![
                    ("seconds", InputValue::Length(30)),
                    ("lookback", InputValue::Length(50)),
                    ("lower", InputValue::Number(10.0)),
                    ("upper", InputValue::Number(90.0)),
                ],
            ),
        ] {
            let mut study = factory.create(name).unwrap();
            let original = StudyTemplate::from_study(study.as_ref()).inputs;
            for (id, value) in &edits {
                study.set_input(id, value.clone()).unwrap();
            }
            let edited = StudyTemplate::from_study(study.as_ref());
            assert_eq!(
                edited.inputs,
                edits
                    .iter()
                    .map(|(id, value)| (id.to_string(), value.clone()))
                    .collect::<Vec<_>>(),
                "{name}"
            );
            assert!(
                edited.inputs.iter().zip(original).all(|(a, b)| *a != b),
                "{name}"
            );
            let saved = serde_json::to_string(&edited).unwrap();
            let template: StudyTemplate = serde_json::from_str(&saved).unwrap();
            let restored = template.create_with(&factory).unwrap();
            assert_eq!(
                StudyTemplate::from_study(restored.as_ref()).inputs,
                edited.inputs,
                "{name}"
            );
        }
    }

    /// The factory's default ranges count executions of 9.5, 10, 99.5 and 100 lots in one range
    /// each (#4008). Then hand sums over the bars filled above: a range includes its smallest
    /// size and excludes its limit (the 100 buy is in the last), a window over the empty bar is
    /// a gap as is the warm-up, and a cumulative sum carries over the gap.
    #[test]
    fn size_ranges_rolling_sums_and_filtered_cvd_follow_the_same_trades() {
        use OrderSide::{Buy, Sell};
        let mut data = bars();
        // The default ranges meet at 10 and 100 lots, so 10 counts only in the second and 100
        // only in the third, and minute 5 sums to its delta, 9.5 + 99.5 - 5 = 104.
        let mut defaults = crate::studies::IndicatorFactory::new()
            .create("Multi-Range Delta")
            .unwrap();
        let input = defaults.order_flow_inputs()[0].clone();
        let clock = BarClock::new(Timeframe::Min1);
        for (minute, trades) in [
            (0, &[(9.5, Buy)][..]),
            (1, &[(10.0, Buy)]),
            (3, &[(99.5, Buy)]),
            (4, &[(100.0, Buy)]),
            (5, &[(9.5, Buy), (99.5, Buy), (5.0, Sell)]),
        ] {
            for (second, &(size, side)) in (0..).zip(trades) {
                let at = start() + TimeDelta::minutes(minute) + TimeDelta::seconds(second);
                input.push_trade(&clock, &Tick::new(at, 100.0, size, Some(side)));
            }
        }
        defaults.calculate(&data);
        assert_eq!(
            format!("{:?}", defaults.values().columns()),
            format!(
                "{:?}",
                [
                    [9.5, 0.0, NAN, 0.0, 0.0, 4.5],
                    [0.0, 10.0, NAN, 99.5, 0.0, 99.5],
                    [0.0, 0.0, NAN, 0.0, 100.0, 0.0],
                ]
            )
        );

        let mut study = OrderFlowStudy::new(OrderFlowKind::SizeDelta {
            ranges: [[0.0, 10.0], [10.0, 100.0], [100.0, f64::MAX]],
        });
        assert_eq!(
            study.line_names(),
            [
                "Delta [0, 10) lots",
                "Delta [10, 100) lots",
                "Delta [100, ∞) lots"
            ]
        );
        fill(&study);
        study.calculate(&data);
        assert_lines(
            &study,
            &[
                [3.0, 0.0, NAN, 3.0, 0.0, 0.0],
                [0.0, 20.0, NAN, 0.0, 0.0, -12.0],
                [0.0, 0.0, NAN, -150.0, 100.0, 0.0],
            ],
        );

        let mut sums = OrderFlowStudy::new(OrderFlowKind::FlowSum { bars: 2 });
        fill(&sums);
        sums.calculate(&data);
        assert_lines(
            &sums,
            &[
                [NAN, 27.0, NAN, NAN, 257.0, 116.0],
                [NAN, 23.0, NAN, NAN, -47.0, 88.0],
            ],
        );

        let mut cvd = OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
            min: 100.0,
            max: f64::MAX,
        });
        fill(&cvd);
        cvd.calculate(&data);
        assert_lines(&cvd, &[[0.0, 0.0, NAN, -150.0, -50.0, -50.0]]);

        // A trade in the last bar changes only its sums, whether recalculated from the start
        // or from that bar.
        for study in [&mut sums, &mut cvd] {
            push(study, 5, &[(5.0, Some(OrderSide::Sell))]);
            push(study, 6, &[(7.0, Some(OrderSide::Buy))]);
            let before = lines(study);
            data.push(Bar::new(
                start() + TimeDelta::minutes(6),
                1.0,
                1.0,
                1.0,
                1.0,
                0.0,
            ));
            let mut fresh = study.clone();
            fresh.calculate(&data);
            study.update(&data, 5);
            assert_eq!(
                format!("{:?}", lines(study)),
                format!("{:?}", lines(&fresh))
            );
            for (old, new) in before.iter().zip(lines(study)) {
                assert_eq!(format!("{:?}", &old[..5]), format!("{:?}", &new[..5]));
            }
            data.pop();
        }
    }

    /// Guards the resting age inputs of both book studies: seconds set the filter's age bound that the
    /// study's source applies and reads back, and 0 removes the bound.
    #[test]
    fn resting_age_inputs_set_the_book_filters() {
        for kind in [
            OrderFlowKind::BookSize {
                levels: 10,
                distance: 0.0,
            },
            OrderFlowKind::BookBalance {
                near_levels: 5,
                far_levels: 10,
                near_distance: 0.0,
                far_distance: 0.0,
            },
        ] {
            let ten = Some(TimeDelta::seconds(10));
            for (id, value, ages) in [
                ("min_age", 10.0, (ten, None)),
                ("max_age", 10.0, (None, ten)),
                ("max_age", 0.0, (None, None)),
            ] {
                let mut study = OrderFlowStudy::new(kind);
                study.set_input(id, InputValue::Number(value)).unwrap();
                let read = study
                    .inputs()
                    .into_iter()
                    .find(|i| i.id == id)
                    .map(|i| i.value);
                assert_eq!(read, Some(InputValue::Number(value)), "{kind:?} {id}");
                let filter = study
                    .order_flow_inputs()
                    .first()
                    .and_then(|input| input.order_filter())
                    .unwrap_or_default();
                assert_eq!(
                    (filter.min_age, filter.max_age),
                    ages,
                    "{kind:?} {id} {value}"
                );
            }
        }
    }

    /// An overnight exchange session opens at 17:00 UTC. Midnight stays in that session;
    /// calendar anchors use its closing date, including 31 December's session for 1 January.
    #[test]
    fn filtered_cvd_resets_on_exchange_trading_dates_for_every_anchor() {
        use crate::model::{Session, SessionWindow, Timeframe, TimezoneId};
        use chrono::{NaiveTime, Weekday};

        let times = [
            "2025-12-31T16:00:00Z",
            "2025-12-31T17:00:00Z",
            "2026-01-01T00:00:00Z",
            "2026-01-01T17:00:00Z",
            "2026-01-04T17:00:00Z",
            "2026-02-01T17:00:00Z",
            "2026-04-01T17:00:00Z",
        ];
        let data: Vec<_> = times
            .map(|t| Bar::new(t.parse().unwrap(), 100.0, 100.0, 100.0, 100.0, 0.0))
            .to_vec();
        let clock = BarClock::new(Timeframe::Min1).with_session(
            TimezoneId::utc(),
            Session::new([SessionWindow::new(
                [
                    Weekday::Mon,
                    Weekday::Tue,
                    Weekday::Wed,
                    Weekday::Thu,
                    Weekday::Fri,
                    Weekday::Sat,
                    Weekday::Sun,
                ],
                NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
            )]),
            false,
        );
        let mut study = OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
            min: 2.0,
            max: 13.0,
        });
        study.set_bar_clock(&clock);
        let input = study.order_flow_inputs()[0].clone();
        // The first bar of the new session has no input. Unknown sides and size-one buys
        // do not add to delta; bounds 2 and 13 are inclusive.
        for (i, size) in [(0, 4.0), (2, 3.0), (3, 5.0), (4, 7.0), (5, 11.0), (6, 13.0)] {
            for (offset, size, side) in [
                (0, size, Some(OrderSide::Buy)),
                (1, 1.0, Some(OrderSide::Buy)),
                (2, 2.0, None),
            ] {
                input.push_trade(
                    &clock,
                    &Tick::new(data[i].time + TimeDelta::seconds(offset), 100.0, size, side),
                );
            }
        }
        for (anchor, want) in [
            (0, [4.0, NAN, 7.0, 12.0, 19.0, 30.0, 43.0]),
            (1, [4.0, NAN, 3.0, 5.0, 7.0, 11.0, 13.0]),
            (2, [4.0, NAN, 7.0, 12.0, 7.0, 11.0, 13.0]),
            (3, [4.0, NAN, 3.0, 8.0, 15.0, 11.0, 13.0]),
            (4, [4.0, NAN, 3.0, 8.0, 15.0, 26.0, 13.0]),
            (5, [4.0, NAN, 3.0, 8.0, 15.0, 26.0, 39.0]),
            (6, [4.0, NAN, 7.0, 12.0, 19.0, 30.0, 43.0]),
            (7, [4.0, NAN, 7.0, 12.0, 19.0, 30.0, 43.0]),
        ] {
            study
                .set_input("anchor", InputValue::Choice(anchor))
                .unwrap();
            study.calculate(&data[..2]);
            study.update(&data, 2);
            for (got, want) in study.values().line(0).iter().zip(want) {
                assert!(
                    got == &want || (got.is_nan() && want.is_nan()),
                    "anchor {anchor}: {got} against {want}"
                );
            }
            let mut full = study.clone();
            full.calculate(&data);
            assert_eq!(
                format!("{:?}", lines(&study)),
                format!("{:?}", lines(&full))
            );
        }
    }

    /// Two symmetric three-bar pivots: highs 101/103 with CVD 20/12, or lows
    /// 99/97 with CVD -20/-12. Only the close of the third right bar confirms.
    /// Boundaries either separate the swings or follow the second before it confirms.
    #[test]
    fn cvd_divergence_confirms_closed_swings_without_crossing_resets_or_gaps() {
        for bullish in [false, true] {
            for boundary in [
                "none",
                "reset",
                "gap",
                "combined",
                "confirm_reset",
                "confirm_gap",
            ] {
                let mut study = OrderFlowStudy::execution_delta(true);
                assert!(study.labels().is_empty());
                study
                    .set_input("divergence", InputValue::Toggle(true))
                    .unwrap();
                let configured = crate::studies::ConfiguredStudy::new(Box::new(study.clone()));
                for line in 0..configured.line_count() {
                    assert_eq!(configured.plot_style(line).visible, line < 3);
                    assert_eq!(configured.legend_visible(line), line < 3);
                    assert_eq!(configured.plot_tagged(line), line < 3);
                }
                let highs = [
                    97., 98., 99., 101., 99., 98., 97., 96., 95., 96., 100., 101., 103., 101.,
                    100., 99., 98., 97.,
                ];
                let mut deltas: [f64; 18] = [
                    1., 1., 1., 17., -1., -1., -1., 1., -1., 1., -1., -1., -4., 1., 1., 1., 1., 1.,
                ];
                if boundary == "combined" {
                    study
                        .set_input("exhaustion", InputValue::Toggle(true))
                        .unwrap();
                    // -11 + 12 + 8 + 3 = 12 at the second high, with exhaustion there too.
                    deltas[9..13].copy_from_slice(&[-28., 12., 8., 3.]);
                }
                let data: Vec<_> = highs
                    .iter()
                    .enumerate()
                    .map(|(i, &high)| {
                        let time = start()
                            + TimeDelta::minutes(i as i64)
                            + if (boundary == "reset" && i >= 7)
                                || (boundary == "confirm_reset" && i >= 14)
                            {
                                TimeDelta::days(1)
                            } else {
                                TimeDelta::zero()
                            };
                        let (high, low) = if bullish {
                            (202. - high, 200. - high)
                        } else {
                            (high, high - 2.)
                        };
                        Bar::new(time, low, high, low, high, 1.)
                    })
                    .collect();
                let input = study.order_flow_inputs()[0].clone();
                for (i, (bar, &delta)) in data.iter().zip(&deltas).enumerate() {
                    if (boundary == "gap" && i == 7) || (boundary == "confirm_gap" && i == 14) {
                        continue;
                    }
                    let delta = if bullish { -delta } else { delta };
                    input.push_trade(
                        &study.clock,
                        &Tick::new(
                            bar.time,
                            100.,
                            delta.abs(),
                            Some(if delta > 0. {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            }),
                        ),
                    );
                }
                study.calculate(&data[..16]);
                let diverges = matches!(
                    boundary,
                    "none" | "combined" | "confirm_reset" | "confirm_gap"
                );
                if diverges {
                    let sign = if bullish { -1.0 } else { 1.0 };
                    assert_eq!(
                        (study.values().value(0, 3), study.values().value(0, 12)),
                        (Some(sign * 20.0), Some(sign * 12.0))
                    );
                }
                assert!(
                    study
                        .labels()
                        .iter()
                        .all(|m| !m.text.contains("divergence")),
                    "the third right bar is still live"
                );
                study.update(&data[..17], 16);
                let expected: Vec<_> = if diverges {
                    vec![(12, if bullish { 97. } else { 103. })]
                } else {
                    vec![]
                };
                assert_eq!(
                    study
                        .labels()
                        .iter()
                        .map(|m| (m.bar, m.price))
                        .collect::<Vec<_>>(),
                    expected,
                    "{bullish} {boundary}"
                );
                let line = if bullish { 3 } else { 4 };
                assert_eq!(
                    study.values().value(line, 15),
                    Some(if diverges { 1. } else { 0. })
                );
                #[cfg(feature = "alert-engine")]
                {
                    use crate::model::{
                        Alert, AlertCondition, AlertFrequency, AlertSource, Timeframe,
                    };
                    use std::sync::{
                        Arc,
                        atomic::{AtomicUsize, Ordering},
                    };
                    let fired = Arc::new(AtomicUsize::new(0));
                    let count = fired.clone();
                    let mut alert = Alert::new("SYM", AlertCondition::CrossingUp(0.5));
                    alert.source = AlertSource::Study {
                        id: "cvd".into(),
                        output: line,
                    };
                    alert.frequency = AlertFrequency::OncePerBarClose;
                    alert.timeframe = Some(Timeframe::Min1);
                    let engine = crate::services::AlertEngine::new()
                        .with_alerts(vec![alert])
                        .on_trigger(move |_, _| {
                            count.fetch_add(1, Ordering::Relaxed);
                        });
                    for len in [16, 17, 17] {
                        engine.update(
                            "SYM",
                            None,
                            Timeframe::Min1,
                            &data[..len],
                            data[len - 1].time,
                            |_, at| study.values().value(line, at),
                        );
                    }
                    assert_eq!(fired.load(Ordering::Relaxed), usize::from(diverges));
                }
                #[cfg(feature = "widget")]
                if matches!(boundary, "none" | "combined") {
                    use crate::model::{BarData, LogicalRange, Timeframe};
                    use crate::theme::Theme;
                    use crate::widget::Chart;
                    for theme in [Theme::light(), Theme::dark()] {
                        for size in [egui::vec2(1440., 900.), egui::vec2(360., 480.)] {
                            let mut shown = data.to_vec();
                            let live = shown.last_mut().unwrap();
                            (live.low, live.high) = if bullish { (96., 98.) } else { (102., 104.) };
                            live.open = live.low + 0.5;
                            live.close = live.high - 0.5;
                            let mut chart = Chart::new(BarData::from_bars(shown));
                            chart.set_timeframe(Timeframe::Min1);
                            chart.set_theme_override(Some(theme.clone()));
                            chart.add_indicator(Box::new(study.clone()));
                            let ctx = egui::Context::default();
                            crate::theme::apply_to_egui(&ctx, &theme);
                            let frame = |chart: &mut Chart| {
                                ctx.run_ui(
                                    egui::RawInput {
                                        screen_rect: Some(egui::Rect::from_min_size(
                                            egui::Pos2::ZERO,
                                            size,
                                        )),
                                        ..Default::default()
                                    },
                                    |ui| {
                                        chart.show(ui);
                                    },
                                )
                            };
                            frame(&mut chart).drop_without_applying_deltas();
                            chart.set_visible_logical_range(LogicalRange::new(-2., 20.));
                            let output = frame(&mut chart);
                            let text_rect = |text: &egui::epaint::TextShape| {
                                egui::Rect::from_min_size(text.pos, text.galley.size())
                            };
                            let marks: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text)
                                        if text.galley.text().contains("divergence") =>
                                    {
                                        Some(text_rect(text))
                                    }
                                    _ => None,
                                })
                                .collect();
                            assert_eq!(
                                marks.len(),
                                1,
                                "the configured study paints its marker once"
                            );
                            assert!(
                                chart.get_rendered_price_rect().contains_rect(marks[0]),
                                "the whole caption belongs to the price pane"
                            );
                            let title = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.text() == "1min" => {
                                        Some(text_rect(text))
                                    }
                                    _ => None,
                                })
                                .unwrap();
                            let legend_row = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Rect(rect)
                                        if rect.fill == theme.chart.bg
                                            && rect.rect.contains_rect(title) =>
                                    {
                                        Some(rect.rect)
                                    }
                                    _ => None,
                                })
                                .min_by(|a, b| a.area().total_cmp(&b.area()))
                                .unwrap();
                            assert!(
                                !marks[0].intersects(legend_row),
                                "caption under the price legend: {size:?} {boundary}"
                            );
                            output.drop_without_applying_deltas();
                        }
                    }
                }
                if boundary == "combined" {
                    assert_eq!(
                        study.labels()[0].text,
                        if bullish {
                            "Bull exhaustion, Bull divergence"
                        } else {
                            "Bear exhaustion, Bear divergence"
                        }
                    );
                }
                let confirmed = study.labels().to_vec();
                study.update(&data, 17);
                assert_eq!(study.labels(), confirmed);
                let mut batch = study.clone();
                batch.calculate(&data);
                assert_eq!(study.labels(), batch.labels());
                assert_eq!(
                    format!("{:?}", lines(&study)),
                    format!("{:?}", lines(&batch))
                );
                // A correction or replay rewind discards confirmations beyond the new end.
                study.update(&data[..16], 15);
                assert!(
                    study
                        .labels()
                        .iter()
                        .all(|m| !m.text.contains("divergence"))
                );
            }
        }
    }

    /// A bar before the run establishes the extreme: 12 > 8 > 3 fades through three
    /// new extremes; 12 > 8 < 9 does not. Negating deltas mirrors the rule at lows.
    #[test]
    fn cvd_exhaustion_requires_consecutive_closed_extremes_and_fading_signed_delta() {
        for bullish in [false, true] {
            for (last, boundary, expected) in [
                (3., "none", true),
                (9., "none", false),
                (3., "gap", false),
                (3., "before", true),
                (3., "reset", true),
                (0., "none", false),
                (-3., "none", false),
            ] {
                let mut study = OrderFlowStudy::execution_delta(true);
                study
                    .set_input("exhaustion", InputValue::Toggle(true))
                    .unwrap();
                let data: Vec<_> = (0..6)
                    .map(|i| {
                        let p = if bullish {
                            100. - i as f64
                        } else {
                            100. + i as f64
                        };
                        let time = start()
                            + TimeDelta::minutes(i)
                            + if boundary == "reset" && i >= 2 {
                                TimeDelta::days(1)
                            } else {
                                TimeDelta::zero()
                            };
                        Bar::new(time, p, p, p, p, 1.)
                    })
                    .collect();
                let input = study.order_flow_inputs()[0].clone();
                for (i, delta) in [20., 12., 8., last, 2., 1.].into_iter().enumerate() {
                    if (boundary == "gap" && i == 2) || (boundary == "before" && i == 0) {
                        continue;
                    }
                    let delta: f64 = if bullish { -delta } else { delta };
                    input.push_trade(
                        &study.clock,
                        &Tick::new(
                            data[i].time,
                            data[i].close,
                            delta.abs(),
                            Some(if delta > 0. {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            }),
                        ),
                    );
                }
                study.calculate(&data[..1]);
                assert_eq!(study.values().columns().len(), study.line_count());
                for len in 2..=4 {
                    study.update(&data[..len], len - 1);
                    assert!(study.labels().is_empty());
                }
                study.update(&data[..5], 4);
                assert_eq!(
                    study.labels().len(),
                    usize::from(expected),
                    "{bullish} {last} {boundary}"
                );
                if expected {
                    assert_eq!(
                        (study.labels()[0].bar, study.labels()[0].price),
                        (3, if bullish { 97. } else { 103. })
                    );
                }
                assert_eq!(
                    study.values().value(if bullish { 5 } else { 6 }, 3),
                    Some(if expected { 1.0 } else { 0.0 })
                );
                let mut batch = study.clone();
                batch.calculate(&data[..5]);
                assert_eq!(study.labels(), batch.labels());
            }
        }
    }

    /// Signed sizes give -1 then +24 on three-tick/range bars. A sell of eight
    /// splits 4/4 on volume bars; a buy of sixteen splits 12/4 on delta bars.
    #[test]
    fn trade_intervals_allocate_every_fill_to_its_chart_bar() {
        use OrderSide::{Buy, Sell};
        let six = [
            (1.0, Buy),
            (2.0, Buy),
            (4.0, Sell),
            (8.0, Buy),
            (16.0, Sell),
            (32.0, Buy),
        ];
        let mut results = Vec::new();
        let mut expected = Vec::new();
        for (interval, fills, same_time, want) in [
            ("3t", six.to_vec(), false, vec![-1.0, 23.0]),
            ("2r", six.to_vec(), false, vec![-1.0, 23.0]),
            (
                "10v",
                vec![(6.0, Buy), (8.0, Sell), (6.0, Buy)],
                false,
                vec![2.0, 4.0],
            ),
            (
                "10delta",
                vec![(6.0, Buy), (8.0, Sell), (16.0, Buy)],
                false,
                vec![10.0, 14.0],
            ),
            (
                "1t",
                six.to_vec(),
                true,
                vec![1.0, 3.0, -1.0, 7.0, -9.0, 23.0],
            ),
        ] {
            let clock = BarClock::new(interval.parse().unwrap());
            let ticks: Vec<_> = fills
                .into_iter()
                .enumerate()
                .map(|(i, (size, side))| {
                    let second = if same_time { 0 } else { i as i64 };
                    Tick::new(
                        start() + TimeDelta::seconds(second),
                        100.0 + i as f64,
                        size,
                        Some(side),
                    )
                })
                .collect();
            let data =
                crate::data::aggregate_trades_with_tick_size(&ticks, clock.clone(), Some(1.0))
                    .unwrap();
            let (size, filtered) = match interval {
                "10v" => (8.0, vec![-4.0, -8.0]),
                "10delta" => (16.0, vec![12.0, 16.0]),
                "1t" => (8.0, vec![0.0, 0.0, 0.0, 8.0, 8.0, 8.0]),
                _ => (8.0, vec![0.0, 8.0]),
            };
            for (min, max, want) in [(0.0, f64::MAX, want), (size, size, filtered)] {
                let mut study = OrderFlowStudy::new(OrderFlowKind::FilteredCvd { min, max });
                study.set_bar_clock(&clock);
                let input = study.order_flow_inputs()[0].clone();
                input.set_tick_size(Some(1.0));
                for tick in &ticks {
                    input.push_trade(&clock, tick);
                }
                study.calculate(&data);
                results.push((
                    interval,
                    study.values().line(0).to_vec(),
                    input.values().keys().copied().collect::<Vec<_>>(),
                    input
                        .values()
                        .last_key_value()
                        .map(|(_, row)| [row[1], row[2]]),
                ));
                let last_range = match (interval, min == 0.0) {
                    ("3t" | "2r", true) => [24.0, -8.0],
                    ("3t" | "2r", false) => [8.0, 0.0],
                    ("10v", true) => [2.0, -4.0],
                    ("10v", false) => [0.0, -4.0],
                    ("10delta", _) => [4.0, 0.0],
                    ("1t", true) => [32.0, 0.0],
                    ("1t", false) => [0.0, 0.0],
                    _ => unreachable!(),
                };
                expected.push((
                    interval,
                    want,
                    data.iter().map(|bar| bar.time).collect::<Vec<_>>(),
                    Some(last_range),
                ));
            }
        }
        assert_eq!(results, expected);
    }

    /// At 11,001 bars retention keeps bars 1,001..=11,000: 10,000 buys of one.
    /// A ten-bar sum first has a complete window at bar 1,010.
    #[test]
    fn incremental_values_restart_after_input_retention() {
        let data: Vec<_> = (0..11_001)
            .map(|i| {
                Bar::new(
                    start() + TimeDelta::minutes(i),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    1.0,
                )
            })
            .collect();
        let mut results = Vec::new();
        for kind in [
            OrderFlowKind::FilteredCvd {
                min: 0.0,
                max: f64::MAX,
            },
            OrderFlowKind::FlowSum { bars: 10 },
        ] {
            let mut study = OrderFlowStudy::new(kind);
            for i in 0..11_000 {
                push(&study, i, &[(1.0, Some(OrderSide::Buy))]);
            }
            study.calculate(&data[..11_000]);
            push(&study, 11_000, &[(1.0, Some(OrderSide::Buy))]);
            study.update(&data, 11_000);
            let mut fresh = study.clone();
            fresh.calculate(&data);
            let line = study.values().line(0);
            results.push((
                line[11_000],
                line[1_001..1_010].iter().all(|v| v.is_nan()),
                // All outputs, including dropped history and warm-up gaps.
                format!("{:?}", lines(&study)) == format!("{:?}", lines(&fresh)),
            ));
        }
        assert_eq!(results, [(10_000.0, false, true), (10.0, true, true)]);
    }

    /// A bar with a buy of 5 and a sell of 5 is a down candle in Execution Delta, as Volume Delta
    /// paints it, and the next bar's +5 an up candle; the cumulative form keeps an unchanged
    /// total up (#4009). Hand values: minute 0 runs +5 then 0, so (0, 5, 0, 0); minute 1 opens
    /// at the total of 0 in both forms, so (0, 5, 0, 5).
    #[test]
    fn execution_delta_paints_a_balanced_bar_down_unless_cumulative() {
        use OrderSide::{Buy, Sell};
        for (cumulative, balanced) in [(true, "Up"), (false, "Down")] {
            let mut study = OrderFlowStudy::execution_delta(cumulative);
            push(&study, 0, &[(5.0, Some(Buy)), (5.0, Some(Sell))]);
            push(&study, 1, &[(5.0, Some(Buy))]);
            study.calculate(&bars());
            let names = study.line_names();
            let candles: Vec<_> = (study.candles().iter())
                .map(|c| {
                    let b = &c.bar;
                    ([b.open, b.high, b.low, b.close], names[c.plot].as_str())
                })
                .collect();
            assert_eq!(
                candles,
                [
                    ([0.0, 5.0, 0.0, 0.0], balanced),
                    ([0.0, 5.0, 0.0, 5.0], "Up")
                ],
                "cumulative {cumulative}"
            );
        }
    }

    /// Executions +5, -8, +5 reach 5, -3, 2. A cumulative candle adds the previous
    /// total of four until the exchange trading day changes, when its open returns to zero.
    #[cfg(feature = "widget")]
    #[test]
    fn registered_execution_candles_follow_chart_updates_and_reset_with_the_line() {
        use crate::model::{BarData, Timeframe};
        use crate::studies::IndicatorFactory;
        use crate::widget::{Chart, SeriesId};
        use egui::{Context, RawInput, Rect, Shape, pos2, vec2};

        let start: DateTime<Utc> = "2026-01-04T23:58:00Z".parse().unwrap();
        let data: Vec<_> = (0..5)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(i),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    999.0,
                )
            })
            .collect();
        let factory = IndicatorFactory::new();
        for (name, expected) in [
            (
                "Execution Delta",
                vec![
                    [0.0, 4.0, 0.0, 4.0],
                    [0.0, 5.0, -3.0, 2.0],
                    [0.0, 5.0, -3.0, 2.0],
                ],
            ),
            (
                "Cumulative Execution Delta",
                vec![
                    [0.0, 4.0, 0.0, 4.0],
                    [4.0, 9.0, 1.0, 6.0],
                    [0.0, 5.0, -3.0, 2.0],
                ],
            ),
        ] {
            assert_eq!(factory.category(name), Some("Order flow"), "{name}");
            let mut chart = Chart::new(BarData::default());
            chart.set_timeframe(Timeframe::Min1);
            chart.set_data(SeriesId::MAIN, data[..3].to_vec());
            let id = chart.add_indicator(factory.create(name).unwrap());
            let input = chart.indicators().next().unwrap().1.order_flow_inputs()[0].clone();
            let mut trades = crate::data::trades::Trades::new(100);
            for (second, size, side) in [
                (0, 4.0, OrderSide::Buy),
                (60, 5.0, OrderSide::Buy),
                (61, 8.0, OrderSide::Sell),
                (62, 5.0, OrderSide::Buy),
                (120, 5.0, OrderSide::Buy),
                (121, 8.0, OrderSide::Sell),
                (122, 5.0, OrderSide::Buy),
            ] {
                let trade = Tick::new(start + TimeDelta::seconds(second), 100.0, size, Some(side));
                input.push_trade(&chart.bar_clock(), &trade);
                trades.push(trade);
            }
            let buy = Color32::from_rgb(10, 200, 40);
            chart
                .indicator_mut(id)
                .unwrap()
                .set_colors(vec![Color32::BLUE, buy, Color32::RED]);
            let ctx = Context::default();
            let frame = |chart: &mut Chart| {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0))),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                    },
                )
            };
            frame(&mut chart).drop_without_applying_deltas();
            let output = frame(&mut chart);
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, Shape::LineSegment { points, stroke } if stroke.color == buy && points[0].x == points[1].x && points[0].y != points[1].y)), "{name} must reach the runtime candle painter");
            output.drop_without_applying_deltas();
            let drawn = |chart: &Chart| {
                chart
                    .indicators()
                    .next()
                    .unwrap()
                    .1
                    .candles()
                    .iter()
                    .map(|c| [c.bar.open, c.bar.high, c.bar.low, c.bar.close])
                    .collect::<Vec<_>>()
            };
            assert_eq!(drawn(&chart), expected, "{name}");
            // A live buy of three follows an empty bar, retaining the cumulative total.
            input.push_trade(
                &chart.bar_clock(),
                &Tick::new(data[4].time, 100.0, 3.0, Some(OrderSide::Buy)),
            );
            chart.update(SeriesId::MAIN, data[3].clone());
            chart.update(SeriesId::MAIN, data[4].clone());
            frame(&mut chart).drop_without_applying_deltas();
            assert_eq!(
                drawn(&chart).last().unwrap()[3],
                if name == "Execution Delta" { 3.0 } else { 5.0 }
            );
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("representation", InputValue::Choice(1))
                .unwrap();
            frame(&mut chart).drop_without_applying_deltas();
            let study = chart.indicators().next().unwrap().1;
            assert!(study.candles().is_empty());
            assert!(study.plot_style(0).visible);
            assert_eq!(
                study
                    .inputs()
                    .iter()
                    .find(|input| input.id == "representation")
                    .unwrap()
                    .default,
                InputValue::Choice(0)
            );
            assert!(
                matches!(&study.values().to_rows()[0], crate::studies::IndicatorValue::Multiple(values) if values.len() == 3)
            );
            assert_eq!(
                study.plot_style(0).plot_type,
                if name == "Execution Delta" {
                    PlotType::Histogram
                } else {
                    PlotType::Line
                }
            );
            if name == "Cumulative Execution Delta" {
                assert_eq!(
                    (
                        study.values().value(0, 1),
                        study.values().value(0, 2),
                        study.values().value(0, 4)
                    ),
                    (Some(6.0), Some(2.0), Some(5.0))
                );
                // Continuous keeps the prior six at midnight, on the same source.
                chart
                    .indicator_mut(id)
                    .unwrap()
                    .set_input("anchor", InputValue::Choice(0))
                    .unwrap();
                frame(&mut chart).drop_without_applying_deltas();
                assert_eq!(
                    chart.indicators().next().unwrap().1.values().value(0, 4),
                    Some(11.0)
                );
            }
            // Replaying inside bar 1 includes its first buy and excludes its later sell and buy.
            input.set_replay(
                &chart.bar_clock(),
                Some(start + TimeDelta::seconds(61)),
                Some(&trades),
                &[],
            );
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("representation", InputValue::Choice(0))
                .unwrap();
            frame(&mut chart).drop_without_applying_deltas();
            assert_eq!(
                drawn(&chart),
                if name == "Execution Delta" {
                    vec![[0.0, 4.0, 0.0, 4.0], [0.0, 5.0, 0.0, 5.0]]
                } else {
                    vec![[0.0, 4.0, 0.0, 4.0], [4.0, 9.0, 4.0, 9.0]]
                }
            );
        }
    }

    /// Execution Delta drawn as a histogram colours each bar as its candles do (#4314): the +5
    /// bar takes Up, and the -5 bar and the balanced bar of a buy and a sell of 5 take Down. A
    /// bar without executions has no value and no colour. Edited Up and Down colours recolour
    /// the bars, and a saved study restores them.
    #[test]
    fn execution_delta_histogram_colours_bars_by_sign_and_keeps_edited_colours() {
        use crate::studies::{ConfiguredStudy, IndicatorFactory, StudyTemplate, palette};
        use OrderSide::{Buy, Sell};
        let factory = IndicatorFactory::new();
        // Each source gets the tape once; a second push would double every bar.
        let fill = |study: &dyn Indicator| {
            let input = &study.order_flow_inputs()[0];
            let clock = BarClock::new(Timeframe::Min1);
            let balanced: &[(f64, OrderSide)] = &[(5.0, Buy), (5.0, Sell)];
            for (minute, trades) in [(0, &[(5.0, Buy)][..]), (1, &[(5.0, Sell)]), (2, balanced)] {
                for (second, &(size, side)) in trades.iter().enumerate() {
                    let at =
                        start() + TimeDelta::minutes(minute) + TimeDelta::seconds(second as i64);
                    input.push_trade(&clock, &Tick::new(at, 100.0, size, Some(side)));
                }
            }
        };
        let read = |study: &mut dyn Indicator| {
            study.calculate(&bars());
            (0..4)
                .map(|bar| (study.values().value(0, bar), study.plot_color(0, bar)))
                .collect::<Vec<_>>()
        };
        let expect = |up, down| {
            [
                (Some(5.0), Some(up)),
                (Some(-5.0), Some(down)),
                (Some(0.0), Some(down)),
                (None, None),
            ]
        };
        let mut study = ConfiguredStudy::new(factory.create("Execution Delta").unwrap());
        study
            .set_input("representation", InputValue::Choice(1))
            .unwrap();
        fill(&study);
        assert_eq!(read(&mut study), expect(palette::UP, palette::DOWN));
        let (up, down) = (
            Color32::from_rgb(10, 200, 40),
            Color32::from_rgb(200, 20, 120),
        );
        let mut settings = study.settings();
        settings.plots[1].color = up;
        settings.plots[2].color = down;
        study.apply_settings(&[], settings).unwrap();
        assert_eq!(read(&mut study), expect(up, down));
        let saved = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
        let template: StudyTemplate = serde_json::from_str(&saved).unwrap();
        let mut restored = template.create_with(&factory).unwrap();
        fill(restored.as_ref());
        assert_eq!(read(restored.as_mut()), expect(up, down));
    }
}
