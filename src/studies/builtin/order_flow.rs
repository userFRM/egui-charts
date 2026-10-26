//! Fixed aggregations of executions, quotes and synchronized depth, never estimates from bars.

use crate::model::{Bar, BarClock};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, OrderFlowInput, OrderFlowKind, PlotStyle,
    PlotType, StudyInput,
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
        }
    }
}

impl Indicator for OrderFlowStudy {
    fn name(&self) -> &str {
        match self.kind {
            OrderFlowKind::Spread => "Quoted Spread",
            OrderFlowKind::BookSize { .. } => "Book Size",
            OrderFlowKind::TradeRate { .. } => "Trade Rate",
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
        let source = self.input.values();
        self.values = IndicatorValues::missing(data.len());
        for (i, bar) in data.iter().enumerate() {
            if let Some(row) = source.get(&self.clock.bar_open(bar.time)) {
                for (line, value) in row.iter().enumerate() {
                    self.values.set_value(line, i, *value);
                }
            }
        }
    }
    fn is_incremental(&self) -> bool {
        true
    }
    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.calculate(data);
            return;
        }
        let from = from.min(data.len());
        self.values.truncate(from);
        self.values
            .resize(data.len(), crate::studies::IndicatorValue::None);
        if let Some(first) = data.get(from) {
            let source = self.input.values_from(self.clock.bar_open(first.time));
            for (i, bar) in data.iter().enumerate().skip(from) {
                if let Some(row) = source.get(&self.clock.bar_open(bar.time)) {
                    for (line, value) in row.iter().enumerate() {
                        self.values.set_value(line, i, *value);
                    }
                }
            }
        }
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
    fn line_cnt(&self) -> usize {
        3
    }
    fn line_names(&self) -> Vec<String> {
        match self.kind {
            OrderFlowKind::Spread => ["Last (ticks)", "Mean (ticks)", "Maximum (ticks)"],
            OrderFlowKind::BookSize { .. } => ["Bid size", "Ask size", "Difference"],
            OrderFlowKind::TradeRate { .. } => {
                ["Bar total", "Rolling / second", "Instant / second"]
            }
            OrderFlowKind::LevelsTradedThrough { .. } => {
                ["Bid levels", "Ask levels", "Total levels"]
            }
        }
        .map(str::to_owned)
        .to_vec()
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors[line.min(2)],
            plot_type: if line == 2 && matches!(self.kind, OrderFlowKind::BookSize { .. }) {
                PlotType::Histogram
            } else {
                PlotType::Line
            },
            width: 1.0,
            pattern: crate::studies::PlotPattern::Solid,
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
        match (&mut self.kind, id, value) {
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
                | OrderFlowKind::LevelsTradedThrough { seconds },
                "seconds",
                InputValue::Length(v),
            ) => *seconds = v,
            (OrderFlowKind::TradeRate { instant, .. }, "instant", InputValue::Length(v)) => {
                *instant = v
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
