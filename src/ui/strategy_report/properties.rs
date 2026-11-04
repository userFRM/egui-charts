//! Account and execution settings for the tester.

use crate::{
    localization as l10n,
    model::{ChartType, OrderSide, Timeframe},
    strategies::{BacktestSettings, Commission, OrderSize, Slippage},
};

pub(super) fn validate(settings: &BacktestSettings) -> Result<(), &'static str> {
    let size = match settings.order_size {
        OrderSize::Fixed(v) | OrderSize::Cash(v) | OrderSize::EquityFraction(v) => v,
    };
    let commission = match settings.commission {
        Commission::Fraction(v) | Commission::PerOrder(v) | Commission::PerUnit(v) => v,
    };
    let slippage = match settings.slippage {
        Slippage::Fraction(v) => v,
        Slippage::Ticks { tick_size, .. } => tick_size,
    };
    if [settings.long_funding, settings.short_funding]
        .into_iter()
        .flatten()
        .any(|value| !value.is_finite() || value < 0.0)
        || !settings.price_increment.is_finite()
        || settings.price_increment <= 0.0
        || !settings.liquidation_increment.is_finite()
        || settings.liquidation_increment <= 0.0
        || (settings.chart_type.transforms_data() && settings.recalculate_on_history)
    {
        return Err(
            "Use nonnegative funding, positive price and unit steps, and regular candles for historical ticks",
        );
    }
    if !settings.initial_capital.is_finite()
        || !settings.risk_free_rate.is_finite()
        || settings.risk_free_rate <= -1.0
        || !settings.leverage.is_finite()
        || settings.leverage < 1.0
        || !size.is_finite()
        || size <= 0.0
        || !commission.is_finite()
        || commission < 0.0
        || !slippage.is_finite()
        || slippage < 0.0
    {
        return Err(
            "Use finite account values, a positive size, leverage at least one and nonnegative costs",
        );
    }
    Ok(())
}

fn number(ui: &mut egui::Ui, label: &str, value: &mut f64, min: f64) {
    let label = ui.label(l10n::text(ui.ctx(), label));
    ui.add(
        egui::DragValue::new(value)
            .range(min..=f64::MAX)
            .speed(0.01)
            .custom_formatter(|value, decimals| {
                let text = egui::emath::format_with_decimals_in_range(value, decimals);
                let tolerance = crate::model::rounding_tolerance(value, 4.0 * f64::EPSILON);
                if text
                    .parse::<f64>()
                    .is_ok_and(|shown| (shown - value).abs() > tolerance)
                {
                    value.to_string()
                } else {
                    text
                }
            }),
    )
    .labelled_by(label.id);
    ui.end_row();
}

fn choice(ui: &mut egui::Ui, label: &str, index: &mut usize, choices: &[&str]) {
    let label_response = ui.label(l10n::text(ui.ctx(), label));
    egui::ComboBox::from_id_salt(label)
        .selected_text(l10n::text(ui.ctx(), choices[*index]))
        .show_ui(ui, |ui| {
            for (i, label) in choices.iter().enumerate() {
                ui.selectable_value(index, i, l10n::text(ui.ctx(), *label));
            }
        })
        .response
        .labelled_by(label_response.id);
    ui.end_row();
}

pub(super) fn show(
    ui: &mut egui::Ui,
    settings: &mut BacktestSettings,
    start: &mut String,
    lower: &mut bool,
    lower_timeframe: &mut Timeframe,
) -> bool {
    let before = (settings.clone(), start.clone(), *lower, *lower_timeframe);
    crate::ui_kit::FormGrid::new("backtest_settings").show(ui, |ui| {
        number(
            ui,
            "Initial capital",
            &mut settings.initial_capital,
            -f64::MAX,
        );
        let label = ui.label(l10n::text(ui.ctx(), "Currency"));
        let mut currency = settings.currency.clone().unwrap_or_default();
        if ui
            .text_edit_singleline(&mut currency)
            .labelled_by(label.id)
            .changed()
        {
            settings.currency = (!currency.trim().is_empty()).then(|| currency.trim().to_owned());
        }
        ui.end_row();
        number(
            ui,
            "Annual risk-free fraction",
            &mut settings.risk_free_rate,
            (-1.0f64).next_up(),
        );
        let (mut kind, mut amount) = match settings.order_size {
            OrderSize::Fixed(v) => (0, v),
            OrderSize::Cash(v) => (1, v),
            OrderSize::EquityFraction(v) => (2, v),
        };
        choice(
            ui,
            "Order size model",
            &mut kind,
            &["Fixed units", "Cash", "Equity fraction"],
        );
        number(ui, "Order size", &mut amount, f64::MIN_POSITIVE);
        settings.order_size = match kind {
            0 => OrderSize::Fixed(amount),
            1 => OrderSize::Cash(amount),
            _ => OrderSize::EquityFraction(amount),
        };
        let label = ui.label(l10n::text(ui.ctx(), "Pyramiding"));
        ui.add(egui::DragValue::new(&mut settings.pyramiding).range(0..=usize::MAX))
            .labelled_by(label.id);
        ui.end_row();
        let (mut kind, mut amount) = match settings.commission {
            Commission::Fraction(v) => (0, v),
            Commission::PerUnit(v) => (1, v),
            Commission::PerOrder(v) => (2, v),
        };
        choice(
            ui,
            "Commission model",
            &mut kind,
            &["Fraction", "Per unit", "Per order"],
        );
        number(ui, "Commission", &mut amount, 0.0);
        settings.commission = match kind {
            0 => Commission::Fraction(amount),
            1 => Commission::PerUnit(amount),
            _ => Commission::PerOrder(amount),
        };
        let mut kind = usize::from(matches!(settings.slippage, Slippage::Ticks { .. }));
        choice(
            ui,
            "Slippage model",
            &mut kind,
            &["Price fraction", "Ticks"],
        );
        match (kind, &mut settings.slippage) {
            (0, Slippage::Fraction(amount)) => number(ui, "Slippage fraction", amount, 0.0),
            (1, Slippage::Ticks { ticks, tick_size }) => {
                let label = ui.label(l10n::text(ui.ctx(), "Slippage ticks"));
                ui.add(egui::DragValue::new(ticks).range(0..=u32::MAX))
                    .labelled_by(label.id);
                ui.end_row();
                number(ui, "Tick size", tick_size, 0.0);
            }
            (0, _) => settings.slippage = Slippage::Fraction(0.0),
            _ => {
                settings.slippage = Slippage::Ticks {
                    ticks: 0,
                    tick_size: 0.01,
                }
            }
        }
        for (side, label) in [
            (OrderSide::Buy, "Long funding (%)"),
            (OrderSide::Sell, "Short funding (%)"),
        ] {
            let before = settings.funding(side) * 100.0;
            let mut percent = before;
            number(ui, label, &mut percent, 0.0);
            if percent != before {
                let funding = match side {
                    OrderSide::Buy => &mut settings.long_funding,
                    OrderSide::Sell => &mut settings.short_funding,
                };
                *funding = Some(percent / 100.0);
            }
        }
        number(
            ui,
            "Price increment",
            &mut settings.price_increment,
            0.0f64.next_up(),
        );
        number(
            ui,
            "Liquidation unit step",
            &mut settings.liquidation_increment,
            0.0f64.next_up(),
        );
        let mut crossing = usize::from(settings.require_limit_cross);
        choice(
            ui,
            "Limit verification",
            &mut crossing,
            &["At the limit", "Cross by one increment"],
        );
        settings.require_limit_cross = crossing == 1;
        let label = ui.label(l10n::text(ui.ctx(), "Order execution delay (price ticks)"));
        ui.add(egui::DragValue::new(&mut settings.order_delay))
            .labelled_by(label.id);
        ui.end_row();
        let label = ui.label(l10n::text(ui.ctx(), "Decision candles"));
        egui::ComboBox::from_id_salt("decision_candles")
            .selected_text(l10n::text(ui.ctx(), settings.chart_type.to_string()))
            .show_ui(ui, |ui| {
                for kind in [ChartType::Candles, ChartType::Heikin] {
                    ui.selectable_value(
                        &mut settings.chart_type,
                        kind,
                        l10n::text(ui.ctx(), kind.to_string()),
                    );
                }
            })
            .response
            .labelled_by(label.id);
        ui.end_row();
        if settings.chart_type.transforms_data() {
            settings.recalculate_on_history = false;
        }
        let label = ui.label(l10n::text(ui.ctx(), "Start (UTC)"));
        ui.add(
            egui::TextEdit::singleline(start)
                .hint_text("YYYY-MM-DD or RFC 3339; empty uses chart history"),
        )
        .labelled_by(label.id);
        ui.end_row();
    });
    ui.checkbox(
        &mut settings.fill_on_close,
        l10n::text(ui.ctx(), "Fill on close"),
    );
    ui.checkbox(
        &mut settings.recalculate_after_fills,
        l10n::text(ui.ctx(), "Recalculate after fills"),
    );
    ui.add_enabled_ui(!settings.chart_type.transforms_data(), |ui| {
        ui.checkbox(
            &mut settings.recalculate_on_history,
            l10n::text(ui.ctx(), "Decide on historical price ticks"),
        );
    });
    ui.add_enabled_ui(settings.chart_type == ChartType::Heikin, |ui| {
        ui.checkbox(
            &mut settings.original_price_fills,
            l10n::text(ui.ctx(), "Execute at original candle prices"),
        );
    });
    ui.checkbox(lower, l10n::text(ui.ctx(), "Lower-interval fills"));
    if *lower {
        egui::ComboBox::from_id_salt("fill_interval")
            .selected_text(lower_timeframe.to_string())
            .show_ui(ui, |ui| {
                for timeframe in Timeframe::all()
                    .into_iter()
                    .filter(|tf| !tf.is_trade_based())
                {
                    ui.selectable_value(lower_timeframe, timeframe, timeframe.to_string());
                }
            })
            .response
            .accessible_name(l10n::text(ui.ctx(), "Fill interval"));
    }
    before != (settings.clone(), start.clone(), *lower, *lower_timeframe)
}
