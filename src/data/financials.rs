//! Reported company figures supplied by the host.

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
