//! What a strategy's decision reads and the orders its bar places, without a type of a front end.

use egui_charts::model::OrderSide;

/// The readings of one decision, copied from the strategy context.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Readings {
    pub position: f64,
    pub equity: f64,
    pub price: f64,
    pub after_fill: bool,
}

/// One order call of a bar, replayed on the strategy context in call order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OrderCall {
    Entry {
        id: String,
        side: OrderSide,
        qty: Option<f64>,
        limit: Option<f64>,
        stop: Option<f64>,
    },
    Exit {
        id: String,
        qty: Option<f64>,
        profit: Option<f64>,
        loss: Option<f64>,
    },
    Relative {
        id: String,
        side: OrderSide,
        profit: f64,
        loss: f64,
    },
    Cancel {
        id: String,
    },
}

/// A decision being run: its readings and the calls of the bar so far.
#[derive(Clone, Debug)]
pub(crate) struct Decision {
    pub readings: Readings,
    pub calls: Vec<OrderCall>,
}
