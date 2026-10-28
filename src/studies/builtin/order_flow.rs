//! Fixed aggregations of executions, quotes and synchronized depth, never estimates from bars.

use crate::data::{OrderFlowInput, OrderFlowKind};
use crate::model::{Bar, BarClock};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, PlotStyle, PlotType, StudyInput,
};
use egui::Color32;

/// An order-flow study in a separate pane. Use the factory titles or [`Self::new`].
///
/// Setting an input starts a fresh source: the host must replay historical events if it needs
/// values before that change. Book size is the last synchronized snapshot in the bar; spread
/// averages every accepted quote equally. Bars without the relevant stream remain gaps.
#[derive(Clone)]
pub struct OrderFlowStudy {
    kind: OrderFlowKind,
    input: OrderFlowInput,
    clock: BarClock,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
    /// Draw the rolling sums as a histogram rather than lines.
    histogram: bool,
}

impl OrderFlowStudy {
    /// Constructs one of the fixed stream aggregations with an empty source.
    pub fn new(kind: OrderFlowKind) -> Self {
        let kind = kind.normalized();
        Self {
            kind,
            input: OrderFlowInput::new(kind),
            clock: BarClock::new(crate::model::Timeframe::Min1),
            values: IndicatorValues::new(),
            colors: vec![
                Color32::from_rgb(38, 166, 154),
                Color32::from_rgb(239, 83, 80),
                Color32::from_rgb(120, 100, 220),
            ],
            visible: true,
            histogram: true,
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
        let from = from.min(data.len());
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
                let mut sum = self.values.line(0)[..from]
                    .iter()
                    .rev()
                    .find(|v| v.is_finite())
                    .copied();
                for (at, row) in self.rows(data, from).into_iter().enumerate() {
                    if let Some(row) = row {
                        let total = sum.unwrap_or(0.0) + row[0];
                        sum = Some(total);
                        self.values.set_value(0, from + at, total);
                    }
                }
            }
            _ => {
                for (at, row) in self.rows(data, from).into_iter().enumerate() {
                    for (line, value) in row.iter().flatten().enumerate() {
                        self.values.set_value(line, from + at, *value);
                    }
                }
            }
        }
    }
}

impl Indicator for OrderFlowStudy {
    fn name(&self) -> &str {
        match self.kind {
            OrderFlowKind::Spread => "Quoted Spread",
            OrderFlowKind::BookSize { .. } => "Book Size",
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
    fn order_flow_input(&self) -> Option<OrderFlowInput> {
        Some(self.input.clone())
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
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
    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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
    fn line_count(&self) -> usize {
        match self.kind {
            OrderFlowKind::FlowSum { .. } => 2,
            OrderFlowKind::FilteredCvd { .. } => 1,
            _ => 3,
        }
    }
    fn line_names(&self) -> Vec<String> {
        let names: &[&str] = match self.kind {
            OrderFlowKind::FlowSum { .. } => &["Volume sum", "Delta sum"],
            OrderFlowKind::FilteredCvd { .. } => &["CVD"],
            OrderFlowKind::SizeDelta { .. } => &["Delta 1", "Delta 2", "Delta 3"],
            OrderFlowKind::Spread => &["Last (ticks)", "Mean (ticks)", "Maximum (ticks)"],
            OrderFlowKind::BookSize { .. } => &["Bid size", "Ask size", "Difference"],
            OrderFlowKind::TradeRate { .. } => {
                &["Bar total", "Rolling / second", "Instant / second"]
            }
            OrderFlowKind::AggressorRate { .. } => {
                &["Bar orders", "Rolling / second", "Instant / second"]
            }
            OrderFlowKind::LevelsExecuted { .. } => &["Ask levels", "Bid levels", "Total levels"],
            OrderFlowKind::LevelsTradedThrough { .. } => {
                &["Bid levels", "Ask levels", "Total levels"]
            }
        };
        names.iter().map(|name| (*name).to_owned()).collect()
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors[line.min(2)],
            plot_type: if (line == 2 && matches!(self.kind, OrderFlowKind::BookSize { .. }))
                || (self.histogram && matches!(self.kind, OrderFlowKind::FlowSum { .. }))
            {
                PlotType::Histogram
            } else {
                PlotType::Line
            },
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            visible: true,
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
        match self.kind {
            OrderFlowKind::Spread => vec![],
            OrderFlowKind::BookSize { levels, distance } => vec![
                length("levels", "Nearest levels", levels, 1, 10000),
                StudyInput::new(
                    "distance",
                    "Price distance (0 uses levels)",
                    InputValue::Number(distance),
                    InputKind::Number(0.0, f64::MAX),
                ),
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
                ranges
                    .iter()
                    .enumerate()
                    .flat_map(|(i, [min, max])| {
                        [
                            size(
                                &format!("min{i}"),
                                format!("Range {} smallest size", i + 1),
                                *min,
                            ),
                            size(
                                &format!("max{i}"),
                                format!("Range {} largest size", i + 1),
                                *max,
                            ),
                        ]
                    })
                    .collect()
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
            OrderFlowKind::FilteredCvd { min, max } => ["min", "max"]
                .into_iter()
                .zip([min, max])
                .map(|(id, value)| {
                    StudyInput::new(
                        id,
                        format!("{} size", if id == "min" { "Smallest" } else { "Largest" }),
                        InputValue::Number(value),
                        InputKind::Number(0.0, f64::MAX),
                    )
                })
                .collect(),
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
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if !self
            .inputs()
            .iter()
            .any(|i| i.id == id && i.accepts(&value))
        {
            return Err("invalid order flow input");
        }
        let before = self.kind;
        if let ("histogram", InputValue::Toggle(on)) = (id, &value) {
            self.histogram = *on;
            return Ok(());
        }
        match (&mut self.kind, id, value) {
            (OrderFlowKind::SizeDelta { ranges }, id, InputValue::Number(v)) => {
                let index = id[3..]
                    .parse::<usize>()
                    .map_err(|_| "invalid order flow input")?;
                ranges[index][usize::from(id.starts_with("max"))] = v;
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
            (OrderFlowKind::TradeRate { volume, .. }, "volume", InputValue::Toggle(v)) => {
                *volume = v
            }
            (
                OrderFlowKind::TradeRate { seconds, .. }
                | OrderFlowKind::AggressorRate { seconds, .. }
                | OrderFlowKind::LevelsExecuted { seconds, .. }
                | OrderFlowKind::LevelsTradedThrough { seconds },
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
            _ => return Err("invalid order flow input"),
        }
        if self.kind != before {
            self.input = OrderFlowInput::new(self.kind);
            self.values = IndicatorValues::new();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrderSide, Tick};
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
        let input = study.order_flow_input().unwrap();
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

    fn assert_lines(study: &OrderFlowStudy, want: &[[f64; 6]]) {
        let got = lines(study);
        assert_eq!(got.len(), want.len());
        for (got, want) in got.iter().zip(want) {
            for (got, want) in got.iter().zip(want) {
                assert!(
                    got == want || (got.is_nan() && want.is_nan()),
                    "{got:?} against {want:?}"
                );
            }
        }
    }

    /// Hand sums over the bars filled above. Size ranges include both bounds (the 100 buy
    /// is in the last), a window over the empty bar is a gap as is the warm-up, and a
    /// cumulative sum carries over the gap.
    #[test]
    fn size_ranges_rolling_sums_and_filtered_cvd_follow_the_same_trades() {
        let mut data = bars();
        let mut study = OrderFlowStudy::new(OrderFlowKind::SizeDelta {
            ranges: [[0.0, 9.0], [10.0, 99.0], [100.0, f64::MAX]],
        });
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
}
