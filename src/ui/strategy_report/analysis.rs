use super::{ReportData, money, percent};
use crate::{localization as l10n, model::StrategyTradeGroup, theme::ThemeExt, ui_kit::EmptyState};
use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, Ui, vec2};

pub(super) fn periods(ui: &mut Ui, data: &ReportData) {
    egui::ScrollArea::both().id_salt("periods").show(ui, |ui| {
        if data.analysis.months.is_empty() {
            EmptyState::new("No equity or closed trades").show(ui);
            return;
        }
        let theme = ui.active_theme();
        for (heading, periods, format) in [
            ("Years", &data.analysis.years, "%Y"),
            ("Months", &data.analysis.months, "%Y-%m"),
        ] {
            ui.strong(l10n::text(ui.ctx(), heading));
            egui::Grid::new(heading)
                .striped(true)
                .spacing([theme.spacing.xxl, theme.spacing.sm])
                .show(ui, |ui| {
                    for label in ["Period", "Return", "Buy-and-hold", "Net profit", "Trades"] {
                        ui.strong(l10n::text(ui.ctx(), label));
                    }
                    ui.end_row();
                    for period in periods {
                        ui.label(period.start.format(format).to_string());
                        ui.label(percent(period.equity_return));
                        ui.label(percent(period.buy_and_hold_return));
                        ui.label(money(Some(period.trades.net_profit), &data.report));
                        ui.label(period.trades.closed_trades.to_string());
                        ui.end_row();
                    }
                });
        }
    });
}

pub(super) fn distribution(ui: &mut Ui, data: &ReportData) {
    egui::ScrollArea::both()
        .id_salt("distribution")
        .show(ui, |ui| {
            if data.shown.closed_trades == 0 {
                EmptyState::new("No closed trades").show(ui);
                return;
            }
            let theme = ui.active_theme();
            if !data.analysis.result_bins.is_empty() {
                ui.label(l10n::format_text(
                    ui.ctx(),
                    "Net profit as a percentage of entry value, in {width} bins",
                    &[("width", percent(Some(data.analysis.result_bin_width)))],
                ));
                histogram(ui, data);
            }
            ui.strong(l10n::text(ui.ctx(), "Winning and losing runs"));
            egui::Grid::new("runs")
                .striped(true)
                .spacing([theme.spacing.xxl, theme.spacing.sm])
                .show(ui, |ui| {
                    for label in ["Run length", "Winning runs", "Losing runs"] {
                        ui.strong(l10n::text(ui.ctx(), label));
                    }
                    ui.end_row();
                    let a = &data.analysis;
                    for i in 0..a.win_runs.len().max(a.loss_runs.len()) {
                        ui.label((i + 1).to_string());
                        ui.label(a.win_runs.get(i).unwrap_or(&0).to_string());
                        ui.label(a.loss_runs.get(i).unwrap_or(&0).to_string());
                        ui.end_row();
                    }
                });
        });
}

fn histogram(ui: &mut Ui, data: &ReportData) {
    let bins = &data.analysis.result_bins;
    let width = data.analysis.result_bin_width;
    let theme = ui.active_theme();
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().max(1.0), 175.0), Sense::hover());
    if rect.width() < 140.0 {
        return;
    }
    let plot = Rect::from_min_max(rect.min + vec2(70.0, 8.0), rect.max - vec2(8.0, 24.0));
    let maximum = bins
        .iter()
        .map(|b| b.winners + b.losers + b.breakeven)
        .max()
        .unwrap_or(1)
        .max(1);
    let slot = plot.width() / bins.len() as f32;
    let painter = ui.painter_at(rect);
    for (i, bin) in bins.iter().enumerate() {
        let mut bottom = plot.bottom();
        for (count, color) in [
            (bin.winners, theme.ui.success),
            (bin.breakeven, theme.ui.text_muted),
            (bin.losers, theme.ui.error),
        ] {
            let height = count as f32 / maximum as f32 * plot.height();
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(plot.left() + i as f32 * slot + 1.0, bottom - height),
                    Pos2::new(plot.left() + (i + 1) as f32 * slot - 1.0, bottom),
                ),
                0.0,
                color,
            );
            bottom -= height;
        }
    }
    let font = FontId::proportional(theme.text.small);
    for (y, text) in [
        (plot.top(), maximum.to_string()),
        (plot.bottom(), "0".into()),
    ] {
        painter.text(
            Pos2::new(plot.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            text,
            font.clone(),
            theme.ui.text_muted,
        );
    }
    let lower = bins[0].lower;
    let upper = bins.last().unwrap().lower + width;
    let mut labels = vec![
        (plot.left(), lower, Align2::LEFT_TOP),
        (plot.right(), upper, Align2::RIGHT_TOP),
    ];
    if lower < 0.0 && upper > 0.0 {
        // Work in bin units so opposite extreme results cannot overflow their span.
        let zero = -(lower / width) / bins.len() as f64;
        labels.push((
            plot.left() + zero as f32 * plot.width(),
            0.0,
            Align2::CENTER_TOP,
        ));
    }
    for (x, value, align) in labels {
        painter.text(
            Pos2::new(x, plot.bottom() + 5.0),
            align,
            percent(Some(value)),
            font.clone(),
            theme.ui.text_muted,
        );
    }
    if let Some(pointer) = response.hover_pos().filter(|p| plot.contains(*p)) {
        let i = (((pointer.x - plot.left()) / slot) as usize).min(bins.len() - 1);
        let bin = &bins[i];
        response.on_hover_text(l10n::format_text(
            ui.ctx(),
            "{lower} to {upper}: {winners} winners, {losers} losers, {breakeven} breakeven",
            &[
                ("lower", percent(Some(bin.lower))),
                ("upper", percent(Some(bin.lower + width))),
                ("winners", bin.winners.to_string()),
                ("losers", bin.losers.to_string()),
                ("breakeven", bin.breakeven.to_string()),
            ],
        ));
    }
}

pub(super) fn times(ui: &mut Ui, data: &ReportData) {
    egui::ScrollArea::both()
        .id_salt("trade_times")
        .show(ui, |ui| {
            if data.shown.closed_trades == 0 {
                EmptyState::new("No closed trades").show(ui);
                return;
            }
            let weekdays = [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ];
            let months = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            let hours: Vec<_> = (0..24).map(|h| format!("{h:02}:00")).collect();
            for (heading, names, groups) in [
                (
                    "Entry hour (UTC)",
                    hours.iter().map(String::as_str).collect::<Vec<_>>(),
                    data.analysis.by_hour.as_slice(),
                ),
                (
                    "Entry weekday (UTC)",
                    weekdays.to_vec(),
                    data.analysis.by_weekday.as_slice(),
                ),
                (
                    "Entry month (UTC)",
                    months.to_vec(),
                    data.analysis.by_month.as_slice(),
                ),
            ] {
                time_groups(ui, data, heading, &names, groups);
            }
        });
}

fn time_groups(
    ui: &mut Ui,
    data: &ReportData,
    heading: &str,
    names: &[&str],
    groups: &[StrategyTradeGroup],
) {
    let theme = ui.active_theme();
    ui.label(RichText::new(l10n::text(ui.ctx(), heading)).strong());
    egui::Grid::new(heading)
        .striped(true)
        .spacing([theme.spacing.xxl, theme.spacing.sm])
        .show(ui, |ui| {
            for label in [heading, "Trades", "Net profit", "Win rate"] {
                ui.strong(l10n::text(ui.ctx(), label));
            }
            ui.end_row();
            for (name, group) in names.iter().zip(groups) {
                ui.label(l10n::text(ui.ctx(), name));
                ui.label(group.closed_trades.to_string());
                ui.label(money(Some(group.net_profit), &data.report));
                ui.label(percent(group.win_rate));
                ui.end_row();
            }
        });
}
