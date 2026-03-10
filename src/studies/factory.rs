//! Indicator factory for dynamic indicator creation by name.
//!
//! The [`IndicatorFactory`] provides a registry of constructor functions
//! keyed by indicator name strings. All built-in indicators are
//! pre-registered, and users can add their own at runtime.
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::IndicatorFactory;
//!
//! let factory = IndicatorFactory::new(); // includes all built-ins
//! let rsi = factory.create("RSI(14)").unwrap();
//! assert_eq!(rsi.name(), "RSI");
//!
//! // Register a custom entry
//! let mut factory = factory;
//! factory.register("MySMA", || Box::new(egui_charts::studies::SMA::new(42)));
//! ```

use crate::studies::*;
use std::collections::HashMap;

/// Type alias for indicator constructor closures stored in the factory.
type IndicatorConstructor = Box<dyn Fn() -> Box<dyn Indicator> + Send + Sync>;

/// A factory that creates [`Indicator`] instances by name.
///
/// On construction, all built-in indicators are automatically registered
/// with commonly-used parameter presets (e.g. `"SMA(20)"`, `"RSI(14)"`,
/// `"MACD"`). Additional indicators can be registered at runtime with
/// [`register`](Self::register).
pub struct IndicatorFactory {
    constructors: HashMap<String, IndicatorConstructor>,
}

impl IndicatorFactory {
    /// Create a new factory pre-loaded with all built-in indicator presets.
    pub fn new() -> Self {
        let mut factory = Self {
            constructors: HashMap::new(),
        };

        // Register built-in indicators
        factory.register_builtin();
        factory
    }

    /// Register all built-in indicators with common default parameters.
    fn register_builtin(&mut self) {
        // === Core Moving Averages ===
        self.register("SMA(20)", || Box::new(SMA::new(20)));
        self.register("SMA(50)", || Box::new(SMA::new(50)));
        self.register("SMA(200)", || Box::new(SMA::new(200)));
        self.register("EMA(12)", || Box::new(EMA::new(12)));
        self.register("EMA(26)", || Box::new(EMA::new(26)));
        self.register("WMA(20)", || Box::new(WMA::new(20)));
        self.register("HMA(20)", || Box::new(HMA::new(20)));

        // === Core Oscillators ===
        self.register("RSI(14)", || Box::new(RSI::new(14)));
        self.register("RSI(21)", || Box::new(RSI::new(21)));
        self.register("MACD", || Box::new(MACD::new(12, 26, 9)));
        self.register("Williams %R", || Box::new(WilliamsR::new(14)));
        self.register("CCI(20)", || Box::new(CCI::new(20)));
        self.register("ROC(14)", || Box::new(RateOfChange::new(14)));

        // === Trend Indicators ===
        self.register("ADX(14)", || Box::new(ADX::new(14)));
        self.register("ATR(14)", || Box::new(ATR::new(14)));

        // === Volatility Indicators ===
        self.register("BB(20,2)", || Box::new(BollingerBands::new(20, 2.0)));
        self.register("StdDev(20)", || Box::new(StandardDeviation::new(20)));

        // === Volume Indicators ===
        self.register("OBV", || Box::new(OnBalanceVolume::new()));
        self.register("VWAP", || Box::new(VolumeWeightedAvgPrice::new()));
        self.register("MFI(14)", || Box::new(MoneyFlowIndex::new(14)));
    }

    /// Register a named indicator constructor.
    ///
    /// If a constructor with the same name already exists it is replaced.
    ///
    /// # Arguments
    /// * `name` -- The lookup key (e.g. `"MySMA(42)"`).
    /// * `constructor` -- A closure that returns a new boxed indicator.
    pub fn register<F>(&mut self, name: impl Into<String>, constructor: F)
    where
        F: Fn() -> Box<dyn Indicator> + Send + Sync + 'static,
    {
        self.constructors.insert(name.into(), Box::new(constructor));
    }

    /// Create an indicator by its registered name, or `None` if not found.
    pub fn create(&self, name: &str) -> Option<Box<dyn Indicator>> {
        self.constructors.get(name).map(|f| f())
    }

    /// Return a list of all registered indicator names (unordered).
    pub fn list(&self) -> Vec<String> {
        self.constructors.keys().cloned().collect()
    }

    /// Check whether an indicator with `name` is registered.
    pub fn has(&self, name: &str) -> bool {
        self.constructors.contains_key(name)
    }

    /// Remove the indicator registered under `name`.
    ///
    /// Returns `true` if it was present and removed, `false` otherwise.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.constructors.remove(name).is_some()
    }

    /// Remove all registered indicators, including built-ins.
    pub fn clear(&mut self) {
        self.constructors.clear();
    }

    /// Return the number of currently registered indicators.
    pub fn count(&self) -> usize {
        self.constructors.len()
    }
}

impl Default for IndicatorFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory() {
        let factory = IndicatorFactory::new();

        // Test built-in indicators
        assert!(factory.has("RSI(14)"));
        assert!(factory.has("MACD"));

        // Test creation
        let rsi = factory.create("RSI(14)");
        assert!(rsi.is_some());
        assert_eq!(rsi.unwrap().name(), "RSI");

        // Test list
        let list = factory.list();
        assert!(!list.is_empty());
    }

    #[test]
    fn test_custom_indicator() {
        let mut factory = IndicatorFactory::new();

        // Register custom indicator
        factory.register("Custom", || Box::new(SMA::new(10)));

        assert!(factory.has("Custom"));
        assert!(factory.create("Custom").is_some());
    }
}
