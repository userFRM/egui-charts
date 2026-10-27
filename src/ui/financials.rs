//! A company's financials in place of the price chart: a chart of one row over the periods and
//! the table of every row, page by page, from figures the host supplies.

use crate::icons::icons;
use crate::localization as l10n;
use crate::scales::StudyFormat;
use crate::theme::ThemeExt;
use crate::ui_kit::EmptyState;
use crate::ui_kit::tab_bar::{TabBar, TabLabel};
use egui::{Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui, pos2, vec2};

/// Height of the chart above the table.
const CHART_HEIGHT: f32 = 200.0;
/// Width of the column of row names.
const NAME_WIDTH: f32 = 240.0;
/// Width of a period's column.
const PERIOD_WIDTH: f32 = 96.0;
/// Height of a table row.
const ROW_HEIGHT: f32 = 32.0;

/// A company's reported figures, as the host supplies them.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct CompanyFinancials {
    /// The currency of the amounts, shown above the tables.
    pub currency: Option<String>,
    /// The pages, such as the income statement, the balance sheet and the statistics, each
    /// chosen with its own pill.
    pub pages: Vec<FinancialPage>,
}

/// One page of [`CompanyFinancials`]: rows of figures by period.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FinancialPage {
    /// The page's name, such as "Income statement".
    pub name: String,
    /// The periods, oldest first, such as "FY 2024" or "Q3 '25": the table's columns.
    pub periods: Vec<String>,
    /// The rows of the table.
    pub rows: Vec<FinancialRow>,
}

/// One row of a [`FinancialPage`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FinancialRow {
    /// The row's name, such as "Total revenue".
    pub name: String,
    /// One value per period of the page; `None` where the company reported none. Non-finite
    /// values are shown as unavailable.
    pub values: Vec<Option<f64>>,
}

impl FinancialPage {
    /// A page named `name` with columns `periods` and no rows.
    pub fn new(name: impl Into<String>, periods: Vec<String>) -> Self {
        Self {
            name: name.into(),
            periods,
            rows: Vec::new(),
        }
    }

    /// Adds a row named `name` with one value per period.
    pub fn with_row(mut self, name: impl Into<String>, values: Vec<Option<f64>>) -> Self {
        self.rows.push(FinancialRow {
            name: name.into(),
            values,
        });
        self
    }
}

impl CompanyFinancials {
    /// Financials made of `pages`, with amounts in `currency`.
    pub fn new(currency: Option<String>, pages: Vec<FinancialPage>) -> Self {
        Self { currency, pages }
    }
}

/// A page's pill.
#[derive(Clone, Copy, PartialEq)]
struct Page<'a>(&'a str);

impl TabLabel for Page<'_> {
    fn tab_label(&self) -> &str {
        self.0
    }
}

/// The financials of a company, shown in place of the price chart.
///
/// Open it with the host's figures, such as when
/// [`SymbolDetailsPanel::take_financials_request`](super::widget_bar::SymbolDetailsPanel::take_financials_request)
/// reports the user's request: a pill per page, a column chart of the chosen row over the page's
/// periods, and the table of all its rows. Clicking a row's name charts it. Without figures it
/// says that none are available. Its Back to chart button ends it.
pub struct FinancialsPanel {
    title: String,
    financials: Option<CompanyFinancials>,
    page: usize,
    charted: usize,
}

impl FinancialsPanel {
    /// The financials of `title`, such as the company's name, or the unavailable state when the
    /// host has none.
    pub fn new(title: impl Into<String>, financials: Option<CompanyFinancials>) -> Self {
        Self {
            title: title.into(),
            financials: financials.filter(|financials| !financials.pages.is_empty()),
            page: 0,
            charted: 0,
        }
    }

    /// Shows the financials; returns whether the user asked to go back to the chart.
    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let theme = ui.active_theme();
        let mut back = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&self.title)
                    .size(theme.text.title + 2.0)
                    .strong()
                    .color(theme.ui.text),
            );
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Financials"))
                    .size(theme.text.title + 2.0)
                    .color(theme.ui.text_secondary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                back = ui
                    .add(egui::Button::image_and_text(
                        icons::UI_ARROW_BACK.as_image_tinted(vec2(16.0, 16.0), theme.ui.text),
                        l10n::text(ui.ctx(), "Back to chart"),
                    ))
                    .clicked();
            });
        });
        ui.add_space(theme.spacing.md);
        let Some(financials) = &self.financials else {
            back |= EmptyState::new("No financial data is available for this symbol")
                .icon(&icons::CHART_COLUMNS)
                .action("Back to chart")
                .fill_height()
                .show(ui);
            return back;
        };
        let names: Vec<_> = financials
            .pages
            .iter()
            .map(|page| Page(&page.name))
            .collect();
        if names.len() > 1 {
            let mut active = names[self.page.min(names.len() - 1)];
            if TabBar::new(&names, &mut active).pills().show(ui) {
                self.page = names.iter().position(|page| *page == active).unwrap_or(0);
                self.charted = 0;
            }
            ui.add_space(theme.spacing.md);
        }
        let page = &financials.pages[self.page.min(financials.pages.len() - 1)];
        if let Some(row) = page.rows.get(self.charted) {
            column_chart(ui, page, row);
        }
        ui.add_space(theme.spacing.md);
        let currency = financials.currency.as_deref();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(charted) = table(ui, page, currency, self.charted) {
                    self.charted = charted;
                }
            });
        back
    }
}

/// A value as the table and chart show it: thousands, millions and billions with a suffix.
fn figure(value: Option<f64>) -> String {
    value.filter(|value| value.is_finite()).map_or_else(
        || "-".to_owned(),
        |value| StudyFormat::Volume.format(value, Some(2)),
    )
}

/// Columns of `row` over the periods of `page`, from a zero line, with their values above them
/// and the periods below.
fn column_chart(ui: &mut Ui, page: &FinancialPage, row: &FinancialRow) {
    let theme = ui.active_theme();
    let width = (NAME_WIDTH + PERIOD_WIDTH * page.periods.len() as f32).min(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, CHART_HEIGHT), Sense::hover());
    let name = l10n::text(ui.ctx(), &row.name);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
    let painter = ui.painter_at(rect);
    let font = FontId::proportional(theme.text.small);
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(theme.text.body),
        theme.ui.text,
    );
    let values = || {
        row.values
            .iter()
            .take(page.periods.len())
            .flatten()
            .copied()
            .filter(|value| value.is_finite())
    };
    let mut high = values().fold(0.0_f64, f64::max);
    let low = values().fold(0.0_f64, f64::min);
    if high == low {
        high = 1.0;
    }
    // Room for the name and the values above, and the periods below.
    let plot = Rect::from_min_max(rect.min + vec2(0.0, 40.0), rect.max - vec2(0.0, 20.0));
    // Normalize first so opposite finite extremes cannot overflow the range.
    let scale = high.max(-low);
    let y = |value: f64| {
        plot.bottom()
            - ((value / scale - low / scale) / (high / scale - low / scale)) as f32 * plot.height()
    };
    let slot = plot.width() / page.periods.len() as f32;
    painter.hline(
        plot.x_range(),
        y(0.0),
        egui::Stroke::new(1.0, theme.ui.border),
    );
    for (i, period) in page.periods.iter().enumerate() {
        let center = plot.left() + slot * (i as f32 + 0.5);
        painter.text(
            pos2(center, plot.bottom() + 4.0),
            Align2::CENTER_TOP,
            l10n::text(ui.ctx(), period),
            font.clone(),
            theme.ui.text_secondary,
        );
        let Some(value) = row
            .values
            .get(i)
            .copied()
            .flatten()
            .filter(|v| v.is_finite())
        else {
            continue;
        };
        let column = Rect::from_two_pos(
            pos2(center - slot * 0.3, y(0.0)),
            pos2(center + slot * 0.3, y(value)),
        );
        let color = if value < 0.0 {
            theme.ui.error
        } else {
            theme.ui.accent
        };
        painter.rect_filled(column, 2.0, color);
        painter.text(
            pos2(center, column.top() - 2.0),
            Align2::CENTER_BOTTOM,
            figure(Some(value)),
            font.clone(),
            theme.ui.text,
        );
    }
}

/// The table of `page`: the periods over the columns, the currency in the corner, and a row per
/// [`FinancialRow`], the charted one highlighted. Returns the row whose name the user clicked.
fn table(
    ui: &mut Ui,
    page: &FinancialPage,
    currency: Option<&str>,
    charted: usize,
) -> Option<usize> {
    let theme = ui.active_theme();
    let width = NAME_WIDTH + PERIOD_WIDTH * page.periods.len() as f32;
    let mut clicked = None;
    let (header, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let font = FontId::proportional(theme.text.body);
    if let Some(currency) = currency {
        let label = format!("{}: {currency}", l10n::text(ui.ctx(), "Currency"));
        ui.painter().text(
            pos2(header.left(), header.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            theme.ui.text_secondary,
        );
    }
    for (i, period) in page.periods.iter().enumerate() {
        ui.painter().text(
            pos2(
                header.left() + NAME_WIDTH + PERIOD_WIDTH * (i as f32 + 1.0) - 8.0,
                header.center().y,
            ),
            Align2::RIGHT_CENTER,
            l10n::text(ui.ctx(), period),
            font.clone(),
            theme.ui.text_secondary,
        );
    }
    ui.painter().hline(
        header.x_range(),
        header.bottom(),
        egui::Stroke::new(1.0, theme.ui.border),
    );
    for (index, row) in page.rows.iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
        let name = l10n::text(ui.ctx(), &row.name);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Row, true, index == charted, &name)
        });
        if index == charted || response.hovered() {
            let fill = if index == charted {
                theme.ui.selection_bg
            } else {
                theme.ui.btn_bg_hover
            };
            ui.painter().rect_filled(rect, 0.0, fill);
        }
        ui.painter().text(
            pos2(rect.left() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font.clone(),
            theme.ui.text,
        );
        for i in 0..page.periods.len() {
            ui.painter().text(
                pos2(
                    rect.left() + NAME_WIDTH + PERIOD_WIDTH * (i as f32 + 1.0) - 8.0,
                    rect.center().y,
                ),
                Align2::RIGHT_CENTER,
                figure(row.values.get(i).copied().flatten()),
                font.clone(),
                theme.ui.text,
            );
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero and missing rows keep their period labels; only finite figures become columns,
    /// measured from zero even when a row contains both gains and losses.
    #[test]
    fn financial_columns_keep_periods_and_scale_finite_signed_values() {
        for (values, heights) in [
            (vec![Some(0.0), Some(0.0)], vec![0.0, 0.0]),
            (vec![None, None], vec![]),
            (vec![Some(-2.0), Some(6.0)], vec![35.0, 105.0]),
            (vec![Some(f64::NAN), Some(f64::INFINITY)], vec![]),
            (vec![Some(-f64::MAX), Some(f64::MAX)], vec![70.0, 70.0]),
        ] {
            let page = FinancialPage::new("Income", vec!["FY 2024".into(), "FY 2025".into()])
                .with_row("Profit", values.clone());
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                column_chart(ui, &page, &page.rows[0]);
            });
            output.textures_delta.clear();
            for period in &page.periods {
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == period)
                }), "missing {period} for {values:?}");
            }
            let actual: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Rect(rect) = &shape.shape {
                        assert!(rect.rect.is_finite(), "{values:?}");
                        Some(rect.rect.height())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(actual, heights, "{values:?}");
        }
        assert_eq!(figure(Some(f64::NAN)), "-");
        assert_eq!(figure(Some(f64::INFINITY)), "-");
    }
}
