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
        // Its tick is the price increment, checked below.
        Slippage::Increments(_) => 0.0,
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
    if settings
        .start_time
        .zip(settings.end_time)
        .is_some_and(|(start, end)| end <= start)
    {
        return Err("Use an end after the start");
    }
    Ok(())
}

/// Returns whether the user changed the value.
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64, min: f64) -> bool {
    let label = ui.label(l10n::text(ui.ctx(), label));
    let response = ui.add(
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
    );
    let changed = response.labelled_by(label.id).changed();
    ui.end_row();
    changed
}

/// The tick count and tick size fields. Until a size is typed, a tick is one price increment
/// and follows later changes of the increment.
fn tick_fields(ui: &mut egui::Ui, mut ticks: u32, typed: Option<f64>, increment: f64) -> Slippage {
    let label = ui.label(l10n::text(ui.ctx(), "Slippage ticks"));
    ui.add(egui::DragValue::new(&mut ticks).range(0..=u32::MAX))
        .labelled_by(label.id);
    ui.end_row();
    let mut tick_size = typed.unwrap_or(increment);
    let edited = number(ui, "Tick size", &mut tick_size, 0.0);
    match typed {
        None if !edited => Slippage::Increments(ticks),
        _ => Slippage::Ticks { ticks, tick_size },
    }
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
    end: &mut String,
    lower: &mut bool,
    lower_timeframe: &mut Timeframe,
) -> bool {
    let before = (
        settings.clone(),
        start.clone(),
        end.clone(),
        *lower,
        *lower_timeframe,
    );
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
        let (mut kind, shown) = match settings.order_size {
            OrderSize::Fixed(v) => (0, v),
            OrderSize::Cash(v) => (1, v),
            // Bound the percentage display without changing untouched saved fractions.
            OrderSize::EquityFraction(v) => (2, (v * 100.0).clamp(f64::MIN_POSITIVE, f64::MAX)),
        };
        let before = (kind, shown);
        choice(
            ui,
            "Order size model",
            &mut kind,
            &["Fixed units", "Cash", "% of equity"],
        );
        let mut amount = shown;
        number(ui, "Order size", &mut amount, f64::MIN_POSITIVE);
        // Avoid rounding an untouched fraction through a percent round trip.
        if (kind, amount) != before {
            settings.order_size = match kind {
                0 => OrderSize::Fixed(amount),
                1 => OrderSize::Cash(amount),
                _ => OrderSize::EquityFraction(amount / 100.0),
            };
        }
        let label = ui.label(l10n::text(ui.ctx(), "Pyramiding"));
        // A saved zero runs as one: show the limit in effect, and store only what the user edits.
        let mut limit = settings.pyramiding.max(1);
        if ui
            .add(egui::DragValue::new(&mut limit).range(1..=usize::MAX))
            .labelled_by(label.id)
            .changed()
        {
            settings.pyramiding = limit;
        }
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
        let mut kind = usize::from(!matches!(settings.slippage, Slippage::Fraction(_)));
        choice(
            ui,
            "Slippage model",
            &mut kind,
            &["Price fraction", "Ticks"],
        );
        match (kind, settings.slippage) {
            (0, Slippage::Fraction(mut amount)) => {
                number(ui, "Slippage fraction", &mut amount, 0.0);
                settings.slippage = Slippage::Fraction(amount);
            }
            (0, _) => settings.slippage = Slippage::Fraction(0.0),
            (_, Slippage::Ticks { ticks, tick_size }) => {
                settings.slippage =
                    tick_fields(ui, ticks, Some(tick_size), settings.price_increment);
            }
            (_, Slippage::Increments(ticks)) => {
                settings.slippage = tick_fields(ui, ticks, None, settings.price_increment);
            }
            _ => settings.slippage = Slippage::Increments(0),
        }
        for (side, label, rule) in [
            (OrderSide::Buy, "Long funding (%)", "Long margin calls"),
            (OrderSide::Sell, "Short funding (%)", "Short margin calls"),
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
            ui.label(l10n::text(ui.ctx(), rule));
            ui.label(l10n::text(ui.ctx(), settings.margin_calls(side)));
            ui.end_row();
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
        let label = ui.label(l10n::text(ui.ctx(), "End (UTC)"));
        ui.add(
            egui::TextEdit::singleline(end)
                .hint_text("YYYY-MM-DD or RFC 3339; empty runs to the newest bar"),
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
    ui.checkbox(
        &mut settings.recalculate_on_every_update,
        l10n::text(ui.ctx(), "Recalculate on every update"),
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
    before
        != (
            settings.clone(),
            start.clone(),
            end.clone(),
            *lower,
            *lower_timeframe,
        )
}
