//! Study browser entries and saved favorites.
//!
//! Use [`IndicatorPreferences`] to retain favorite factory names and
//! [`IndicatorInfo`] to describe studies in a picker.

use crate::studies::IndicatorFactory;

/// Host-owned indicator browser settings. Save these alongside the user's other settings
/// after [`IndicatorDialogAction::ToggleFavorite`](super::IndicatorDialogAction::ToggleFavorite),
/// and restore them with [`IndicatorDialog::set_preferences`](super::IndicatorDialog::set_preferences).
/// Unknown study names are retained so temporarily unavailable host studies keep their stars.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndicatorPreferences {
    /// Favourite factory names, sorted for stable saved documents.
    #[serde(default)]
    pub favorites: std::collections::BTreeSet<String>,
}

/// Indicator information
#[derive(Debug, Clone)]
pub struct IndicatorInfo {
    /// The name the dialog's [`IndicatorFactory`] creates the study under
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description displayed to the user.
    pub desc: String,
    /// Category label used to group the entry in the browser.
    pub category: String,
    /// Whether the study draws on the price pane by default.
    pub is_overlay: bool,
    /// Display flag for a premium entry; access control belongs to the host.
    pub is_premium: bool,
}

impl IndicatorInfo {
    /// Create a new indicator info entry with the given ID, name, and description
    pub fn new(id: impl Into<String>, name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            desc: desc.into(),
            category: "Technical Analysis".to_string(),
            is_overlay: false,
            is_premium: false,
        }
    }

    /// Mark this indicator as an overlay (renders on the price chart)
    pub fn overlay(mut self) -> Self {
        self.is_overlay = true;
        self
    }

    /// Mark this entry as premium for display. Access control belongs to the host.
    pub fn premium(mut self) -> Self {
        self.is_premium = true;
        self
    }

    /// Every study `factory` creates, sorted by display name, with its description, category
    /// and overlay flag
    pub(super) fn from_factory(factory: &IndicatorFactory) -> Vec<IndicatorInfo> {
        let mut infos: Vec<IndicatorInfo> = factory
            .list()
            .into_iter()
            .filter_map(|id| {
                let study = factory.create(&id)?;
                // A description reads "Full name - what it measures", as `Indicator::desc`
                // shows; one without a full name keeps the registered name.
                let (name, desc) = study
                    .desc()
                    .split_once(" - ")
                    .unwrap_or((id.as_str(), study.desc()));
                let mut info = IndicatorInfo::new(id.clone(), name, desc);
                info.category = factory.category(&id).unwrap_or_default().to_string();
                info.is_overlay = study.is_overlay();
                Some(info)
            })
            .collect();
        infos.sort_by_key(|info| info.name.to_lowercase());
        infos
    }
}
