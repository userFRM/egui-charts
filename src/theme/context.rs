//! Reading the theme last applied to an egui context.

use egui::{Context, Ui};

use super::Theme;

/// Read the theme last passed to [`apply_to_egui`](super::apply_to_egui).
///
/// Widgets call it once per frame and read the colours egui does not model from it;
/// the rest come from `ui.visuals()`, which `apply_to_egui` sets from the same theme.
///
/// ```
/// use egui_charts::theme::{Theme, ThemeExt, apply_to_egui};
///
/// # egui::__run_test_ui(|ui| {
/// apply_to_egui(ui.ctx(), &Theme::light());
/// let theme = ui.active_theme();
/// assert_eq!(theme, Theme::light());
/// let _on_accent = theme.ui.accent_text;
/// # });
/// ```
pub trait ThemeExt {
    /// The theme last applied, or [`Theme::dark`] if none has been.
    fn active_theme(&self) -> Theme;
}

impl ThemeExt for Context {
    fn active_theme(&self) -> Theme {
        applied_theme(self).unwrap_or_default()
    }
}

impl ThemeExt for Ui {
    fn active_theme(&self) -> Theme {
        self.ctx().active_theme()
    }
}

fn active_theme_id() -> egui::Id {
    egui::Id::NULL.with("active_theme")
}

/// The theme last applied to `ctx`, or `None` if none has been.
pub(crate) fn applied_theme(ctx: &Context) -> Option<Theme> {
    ctx.data(|d| d.get_temp::<Theme>(active_theme_id()))
}

/// Store the theme [`ThemeExt::active_theme`] returns.
pub(crate) fn set_active_theme(ctx: &Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(active_theme_id(), theme));
}

/// A component config derived from a theme, whose fields can follow a new theme one by one.
#[cfg(feature = "ui")]
pub(crate) trait ThemeDerived {
    /// Move every field still equal to its value in `old` to its value in `new`,
    /// keeping the fields the caller has changed.
    fn follow(&mut self, old: &Self, new: &Self);
}

/// Implement [`ThemeDerived`] for a config struct, listing all of its fields.
#[cfg(feature = "ui")]
macro_rules! theme_derived {
    ($config:ty { $($field:ident),+ $(,)? }) => {
        impl $crate::theme::ThemeDerived for $config {
            fn follow(&mut self, old: &Self, new: &Self) {
                // Exhaustive, so a field added to the struct must be listed here.
                let Self { $($field),+ } = self;
                $(if *$field == old.$field {
                    *$field = new.$field.clone();
                })+
            }
        }
    };
}
#[cfg(feature = "ui")]
pub(crate) use theme_derived;

/// Rederive a theme-derived component config when the active theme changes.
///
/// `derived` holds the theme and config that `config` was last derived from, or
/// `None` for a config the caller supplied, which is never replaced. Fields the
/// caller has changed since are kept; the others follow the new theme.
#[cfg(feature = "ui")]
pub(crate) fn follow_active_theme<C: ThemeDerived>(
    ctx: &Context,
    derived: &mut Option<(Theme, C)>,
    config: &mut C,
    from_theme: impl FnOnce(&Theme) -> C,
) {
    if let Some(theme) = applied_theme(ctx)
        && let Some((derived_from, old)) = derived
        && *derived_from != theme
    {
        let new = from_theme(&theme);
        config.follow(old, &new);
        *derived_from = theme;
        *old = new;
    }
}

#[cfg(all(test, feature = "ui"))]
mod tests {
    use super::*;
    use crate::theme::apply_to_egui;
    use crate::ui::timeframe_toolbar::{TimeframeToolbar, TimeframeToolbarConfig};
    use egui::Color32;

    #[test]
    fn theme_derived_config_follows_the_active_theme_and_keeps_caller_edits() {
        let ctx = Context::default();
        let mut themed = TimeframeToolbar::new();
        themed.config.panel_height = 40.0;
        themed.config.text_color = Color32::RED;
        let mut custom =
            TimeframeToolbar::with_config(TimeframeToolbarConfig::from_theme(&Theme::dark()));

        // Light differs from the Dark config the toolbar is built with; Dark is a later change,
        // and an edited Dark theme must propagate too.
        let mut edited_dark = Theme::dark();
        edited_dark.ui.border = Color32::GREEN;
        for (name, theme) in [
            ("light", Theme::light()),
            ("dark", Theme::dark()),
            ("edited dark", edited_dark),
        ] {
            let _ = ctx.run_ui(Default::default(), |ui| {
                apply_to_egui(ui.ctx(), &theme);
                egui::CentralPanel::default().show(ui, |ui| {
                    themed.show(ui);
                    custom.show(ui);
                });
            });
            assert_eq!(
                themed.config.panel_height, 40.0,
                "{name}: edited height replaced"
            );
            assert_eq!(
                themed.config.text_color,
                Color32::RED,
                "{name}: edited colour replaced"
            );
            assert_eq!(
                themed.config.border_color, theme.ui.border,
                "{name}: untouched colour stale"
            );
            assert_eq!(
                custom.config.border_color,
                Theme::dark().ui.border,
                "{name}: with_config replaced"
            );
        }
    }
}
