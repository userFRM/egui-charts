//! Account and execution settings for the tester.

use crate::{
    localization as l10n,
    model::Timeframe,
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
            .speed(0.01),
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
        number(ui, "Leverage", &mut settings.leverage, 1.0);
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
