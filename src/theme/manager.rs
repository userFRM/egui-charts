//! Theme Manager - Runtime theme switching and persistence
//!
//! Handles current theme state, runtime switching, localStorage persistence,
//! and theme change notifications.

use super::presets::ThemePreset;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global theme version counter for change detection
static THEME_VERSION: AtomicU64 = AtomicU64::new(0);

/// Storage key for persisted theme (wasm only)
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "egui_charts_theme";

// ============================================================================
// THEME MANAGER
// ============================================================================

/// Manages theme state, switching, and persistence
#[derive(Debug)]
pub struct ThemeManager {
    /// Current active theme preset
    current: ThemePreset,
    /// Version number (incremented on every change)
    version: u64,
    /// Whether to auto-persist on change
    auto_persist: bool,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    /// Create a new theme manager with default (Classic) theme
    pub fn new() -> Self {
        let initial = Self::load_persisted().unwrap_or(ThemePreset::Classic);
        Self {
            current: initial,
            version: THEME_VERSION.fetch_add(1, Ordering::SeqCst),
            auto_persist: true,
        }
    }

    /// Create with a specific initial theme
    pub fn with_theme(preset: ThemePreset) -> Self {
        Self {
            current: preset,
            version: THEME_VERSION.fetch_add(1, Ordering::SeqCst),
            auto_persist: true,
        }
    }

    /// Get current theme preset
    #[inline]
    pub fn current(&self) -> ThemePreset {
        self.current
    }

    /// Get current theme (fully resolved)
    #[inline]
    pub fn theme(&self) -> super::Theme {
        self.current.to_theme()
    }

    /// Get current version (for change detection)
    #[inline]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check if theme has changed since a given version
    #[inline]
    pub fn has_changed_since(&self, version: u64) -> bool {
        self.version != version
    }

    /// Switch to a new theme
    pub fn set_theme(&mut self, preset: ThemePreset) {
        if self.current != preset {
            self.current = preset;
            self.version = THEME_VERSION.fetch_add(1, Ordering::SeqCst);

            if self.auto_persist {
                self.persist();
            }
        }
    }

    /// Cycle to next theme
    pub fn next_theme(&mut self) {
        let presets = ThemePreset::all();
        let curr_idx = presets.iter().position(|p| *p == self.current).unwrap_or(0);
        let next_idx = (curr_idx + 1) % presets.len();
        self.set_theme(presets[next_idx]);
    }

    /// Enable/disable auto-persistence
    pub fn set_auto_persist(&mut self, enabled: bool) {
        self.auto_persist = enabled;
    }

    /// Manually persist current theme
    pub fn persist(&self) {
        Self::save_persisted(self.current);
    }

    /// Load persisted theme from localStorage
    fn load_persisted() -> Option<ThemePreset> {
        #[cfg(target_arch = "wasm32")]
        {
            Self::load_from_local_storage()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Save theme to localStorage
    fn save_persisted(preset: ThemePreset) {
        #[cfg(target_arch = "wasm32")]
        {
            Self::save_to_local_storage(preset);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = preset;
        }
    }

    // =========================================================================
    // WASM Storage (localStorage)
    // =========================================================================

    #[cfg(target_arch = "wasm32")]
    fn load_from_local_storage() -> Option<ThemePreset> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        let value = storage.get_item(STORAGE_KEY).ok()??;

        ThemePreset::from_name(&value)
    }

    #[cfg(target_arch = "wasm32")]
    fn save_to_local_storage(preset: ThemePreset) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(STORAGE_KEY, preset.name());
            }
        }
    }
}
