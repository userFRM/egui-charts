//! Price formatting for Y-axis labels.
//!
//! The [`PriceFormatter`] trait lets users control how numeric price values are
//! rendered as text on the price axis.  The library ships with several built-in
//! formatters ([`DefaultPriceFormatter`], [`CurrencyFormatter`],
//! [`VolumeFormatter`], [`PercentageFormatter`], [`ScientificFormatter`]) and
//! supports closures via [`CustomPriceFormatter`].
//!
//! Use [`PriceFormatterBuilder`] for a fluent construction API.

/// Trait for custom price formatting on the Y axis.
///
/// Implement this trait to control how numeric prices are displayed as labels.
/// All formatters must be `Send + Sync` so they can be shared across the
/// render pipeline.
pub trait PriceFormatter: Send + Sync {
    /// Format a price value into a display string
    ///
    /// # Arguments
    /// * `price` - The price value to format
    ///
    /// # Returns
    /// Formatted string to display on the price axis
    fn format(&self, price: f64) -> String;

    /// Clone this formatter into a Box
    fn clone_box(&self) -> Box<dyn PriceFormatter>;
}

/// Default price formatter with configurable precision
#[derive(Debug, Clone)]
pub struct DefaultPriceFormatter {
    /// Number of decimal places
    pub precision: usize,

    /// Min number of decimal places (0 = auto)
    pub min_precision: usize,
}

impl Default for DefaultPriceFormatter {
    fn default() -> Self {
        Self {
            precision: 2,
            min_precision: 0,
        }
    }
}

impl DefaultPriceFormatter {
    /// Create formatter with fixed precision
    pub fn with_precision(precision: usize) -> Self {
        Self {
            precision,
            min_precision: precision,
        }
    }

    /// Create formatter with auto precision (shows significant digits)
    pub fn auto() -> Self {
        Self {
            precision: 8,
            min_precision: 0,
        }
    }
}

impl PriceFormatter for DefaultPriceFormatter {
    fn format(&self, price: f64) -> String {
        if self.min_precision > 0 {
            // Fixed precision
            format!("{:.prec$}", price, prec = self.precision)
        } else {
            // Auto precision - remove trailing zeros
            let formatted = format!("{:.prec$}", price, prec = self.precision);
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Percentage price formatter
#[derive(Debug, Clone)]
pub struct PercentageFormatter {
    /// Number of decimal places for percentage
    pub precision: usize,

    /// Base value for percentage calculation (100.0 = 100%)
    pub base_val: f64,
}

impl Default for PercentageFormatter {
    fn default() -> Self {
        Self {
            precision: 2,
            base_val: 100.0,
        }
    }
}

impl PriceFormatter for PercentageFormatter {
    fn format(&self, price: f64) -> String {
        let percentage = ((price / self.base_val) * 100.0) - 100.0;
        format!("{:+.prec$}%", percentage, prec = self.precision)
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Currency formatter with symbol
#[derive(Debug, Clone)]
pub struct CurrencyFormatter {
    /// Currency symbol (e.g., "$", "€", "£", "¥")
    pub symbol: String,

    /// Number of decimal places
    pub precision: usize,

    /// Whether to place symbol before or after price
    pub symbol_before: bool,

    /// Whether to add space between symbol and value
    pub add_space: bool,

    /// Thousands separator (e.g., "," for US, "." for EU)
    pub thousands_sep: Option<char>,
}

impl CurrencyFormatter {
    /// Create USD formatter
    pub fn usd() -> Self {
        Self {
            symbol: "$".to_string(),
            precision: 2,
            symbol_before: true,
            add_space: false,
            thousands_sep: Some(','),
        }
    }

    /// Create EUR formatter
    pub fn eur() -> Self {
        Self {
            symbol: "€".to_string(),
            precision: 2,
            symbol_before: false,
            add_space: true,
            thousands_sep: Some('.'),
        }
    }

    /// Create BTC formatter
    pub fn btc() -> Self {
        Self {
            symbol: "₿".to_string(),
            precision: 8,
            symbol_before: false,
            add_space: true,
            thousands_sep: Some(','),
        }
    }

    /// Create custom currency formatter
    pub fn new(symbol: impl Into<String>, precision: usize) -> Self {
        Self {
            symbol: symbol.into(),
            precision,
            symbol_before: true,
            add_space: false,
            thousands_sep: Some(','),
        }
    }
}

impl PriceFormatter for CurrencyFormatter {
    fn format(&self, price: f64) -> String {
        let val_str = if let Some(sep) = self.thousands_sep {
            self.format_with_separator(price, sep)
        } else {
            format!("{:.prec$}", price, prec = self.precision)
        };

        let space = if self.add_space { " " } else { "" };

        if self.symbol_before {
            format!("{}{}{}", self.symbol, space, val_str)
        } else {
            format!("{}{}{}", val_str, space, self.symbol)
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

impl CurrencyFormatter {
    fn format_with_separator(&self, price: f64, separator: char) -> String {
        let formatted = format!("{:.prec$}", price, prec = self.precision);
        let parts: Vec<&str> = formatted.split('.').collect();

        let integer = parts[0];
        let decimal = if parts.len() > 1 { parts[1] } else { "" };

        // Add thousands separators
        let integer_with_sep: String = integer
            .chars()
            .rev()
            .enumerate()
            .fold(String::new(), |mut acc, (i, c)| {
                if i > 0 && i % 3 == 0 {
                    acc.push(separator);
                }
                acc.push(c);
                acc
            })
            .chars()
            .rev()
            .collect();

        if decimal.is_empty() {
            integer_with_sep
        } else {
            format!("{integer_with_sep}.{decimal}")
        }
    }
}

/// Volume formatter (K, M, B suffixes)
#[derive(Debug, Clone)]
pub struct VolumeFormatter {
    /// Number of decimal places
    pub precision: usize,
}

impl Default for VolumeFormatter {
    fn default() -> Self {
        Self { precision: 2 }
    }
}

impl PriceFormatter for VolumeFormatter {
    fn format(&self, value: f64) -> String {
        let abs_val = value.abs();

        if abs_val >= 1_000_000_000.0 {
            format!("{:.prec$}B", value / 1_000_000_000.0, prec = self.precision)
        } else if abs_val >= 1_000_000.0 {
            format!("{:.prec$}M", value / 1_000_000.0, prec = self.precision)
        } else if abs_val >= 1_000.0 {
            format!("{:.prec$}K", value / 1_000.0, prec = self.precision)
        } else {
            format!("{:.prec$}", value, prec = self.precision)
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Scientific notation formatter
#[derive(Debug, Clone)]
pub struct ScientificFormatter {
    /// Number of significant digits
    pub precision: usize,
}

impl Default for ScientificFormatter {
    fn default() -> Self {
        Self { precision: 3 }
    }
}

impl PriceFormatter for ScientificFormatter {
    fn format(&self, price: f64) -> String {
        format!("{:.prec$e}", price, prec = self.precision)
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Custom price formatter using a closure
pub struct CustomPriceFormatter {
    formatter: Box<dyn Fn(f64) -> String + Send + Sync>,
}

impl CustomPriceFormatter {
    /// Create a new custom formatter from a closure
    pub fn new<F>(formatter: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        Self {
            formatter: Box::new(formatter),
        }
    }
}

impl PriceFormatter for CustomPriceFormatter {
    fn format(&self, price: f64) -> String {
        (self.formatter)(price)
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        // Cannot clone closures, return default
        Box::new(DefaultPriceFormatter::default())
    }
}

/// Builder for creating price formatters
pub struct PriceFormatterBuilder {
    precision: usize,
    currency: Option<String>,
    percentage: bool,
    volume: bool,
}

impl PriceFormatterBuilder {
    /// Create a new formatter builder
    pub fn new() -> Self {
        Self {
            precision: 2,
            currency: None,
            percentage: false,
            volume: false,
        }
    }

    /// Set precision
    pub fn with_precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Set currency symbol
    pub fn with_currency(mut self, symbol: impl Into<String>) -> Self {
        self.currency = Some(symbol.into());
        self
    }

    /// Enable percentage formatting
    pub fn as_percentage(mut self) -> Self {
        self.percentage = true;
        self
    }

    /// Enable volume formatting
    pub fn as_volume(mut self) -> Self {
        self.volume = true;
        self
    }

    /// Build the formatter
    pub fn build(self) -> Box<dyn PriceFormatter> {
        if self.percentage {
            Box::new(PercentageFormatter {
                precision: self.precision,
                base_val: 100.0,
            })
        } else if self.volume {
            Box::new(VolumeFormatter {
                precision: self.precision,
            })
        } else if let Some(symbol) = self.currency {
            Box::new(CurrencyFormatter::new(symbol, self.precision))
        } else {
            Box::new(DefaultPriceFormatter::with_precision(self.precision))
        }
    }
}

impl Default for PriceFormatterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_formatter() {
        let formatter = DefaultPriceFormatter::default();
        assert_eq!(formatter.format(123.456), "123.46");
        assert_eq!(formatter.format(123.0), "123");
    }

    #[test]
    fn test_fixed_precision() {
        let formatter = DefaultPriceFormatter::with_precision(4);
        assert_eq!(formatter.format(123.456), "123.4560");
    }

    #[test]
    fn test_percentage_formatter() {
        let formatter = PercentageFormatter::default();
        assert_eq!(formatter.format(105.0), "+5.00%");
        assert_eq!(formatter.format(95.0), "-5.00%");
    }

    #[test]
    fn test_currency_formatter() {
        let usd = CurrencyFormatter::usd();
        assert_eq!(usd.format(1234.56), "$1,234.56");

        let eur = CurrencyFormatter::eur();
        assert!(eur.format(1234.56).contains("€"));

        let btc = CurrencyFormatter::btc();
        assert!(btc.format(0.12345678).contains("₿"));
    }

    #[test]
    fn test_volume_formatter() {
        let formatter = VolumeFormatter::default();
        assert_eq!(formatter.format(1_500_000_000.0), "1.50B");
        assert_eq!(formatter.format(2_500_000.0), "2.50M");
        assert_eq!(formatter.format(3_500.0), "3.50K");
        assert_eq!(formatter.format(500.0), "500.00");
    }

    #[test]
    fn test_scientific_formatter() {
        let formatter = ScientificFormatter::default();
        let result = formatter.format(0.000123);
        assert!(result.contains("e"));
    }

    #[test]
    fn test_custom_formatter() {
        let formatter = CustomPriceFormatter::new(|price| format!("Price: {price:.2}"));
        assert_eq!(formatter.format(123.456), "Price: 123.46");
    }

    #[test]
    fn test_formatter_builder() {
        let formatter = PriceFormatterBuilder::new().with_precision(4).build();
        assert_eq!(formatter.format(123.456789), "123.4568");

        let currency_formatter = PriceFormatterBuilder::new()
            .with_currency("$")
            .with_precision(2)
            .build();
        assert!(currency_formatter.format(100.0).contains("$"));
    }
}
