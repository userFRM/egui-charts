//! Replay trading dialogs: the question when the replay cursor moves before the replay account's
//! last change, and the results of the session.

use crate::localization as l10n;
use crate::model::{Bar, StrategyReport};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::strategy_report::{StrategyReportAction, StrategyReportPanel};
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;

/// The user's answer to a [`ReplayTradingPrompt`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayTradingChoice {
    /// No answer yet, or the prompt is closed.
    #[default]
    None,
    /// Keep the cursor where it was: Escape, the close button or a click beside the dialog.
    Stay,
    /// End the session and show its results; the cursor moves without the account.
    EndSession,
    /// Reset the account, or build a new one, and keep trading from the new bar.
    NewAccount,
}

/// Asks what to do when the replay cursor moves before the replay account's last change, or the
/// user picks a new start while the account has history: keep trading from the new bar with a
/// new account, or end the session and see its results. Open it by setting `is_open` when
/// [`PaperAccount::set_replay_time`](crate::services::PaperAccount::set_replay_time) refuses the
/// new bar's time, and keep the cursor where it was until the answer.
///
/// ```
/// use egui_charts::model::Timeframe;
/// use egui_charts::services::ReplayAccountSettings;
/// use egui_charts::ui::replay::{ReplayResults, ReplayTradingChoice, ReplayTradingPrompt};
///
/// // The account the user chose in the replay start menu, apart from any live account.
/// let settings = ReplayAccountSettings::new("USD", 10_000.0, 0.001);
/// let mut account = settings.account().expect("valid settings");
/// let mut prompt = ReplayTradingPrompt::default();
/// let mut results = ReplayResults::default();
/// // Before feeding the price of the bar the user moved the cursor to:
/// let bar = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
/// if !account.set_replay_time(Some(bar)) {
///     prompt.is_open = true; // Keep the cursor where it was until the answer.
/// }
/// # egui::__run_test_ui(|ui| {
/// match prompt.show(ui.ctx()) {
///     ReplayTradingChoice::NewAccount => {
///         account = settings.account().expect("valid settings");
///         // Move the cursor to the new bar.
///     }
///     ReplayTradingChoice::EndSession => {
///         results.open(account.report("Replay trading", "AAPL", Timeframe::Min1), &[]);
///         // Move the cursor without the account.
///     }
///     _ => {}
/// }
/// results.show(ui.ctx());
/// # });
/// ```
#[derive(Debug, Default)]
pub struct ReplayTradingPrompt {
    /// Whether the dialog is open.
    pub is_open: bool,
}

impl ReplayTradingPrompt {
    /// Shows the dialog while it is open and returns the answer once given, closing it.
    pub fn show(&mut self, ctx: &egui::Context) -> ReplayTradingChoice {
        let Some(response) = Modal::new("replay_trading_prompt", "Replay trading")
            .width(DESIGN_TOKENS.sizing.dialog.confirm_width)
            .show(ctx, &mut self.is_open, |ui| {
                ui.label(l10n::text(
                    ui.ctx(),
                    "The replay account has orders or trades after this bar. Keep trading from \
                     here with a new account, or end the session and see its results?",
                ));
                DialogFooter::new("Trade from here")
                    .secondary("End session")
                    .show(ui)
            })
        else {
            return ReplayTradingChoice::None;
        };
        // Unlike a confirm dialog's cancel, Escape keeps the session going.
        let choice = match response.inner {
            (true, _) => ReplayTradingChoice::NewAccount,
            (_, true) => ReplayTradingChoice::EndSession,
            _ if response.close_requested => ReplayTradingChoice::Stay,
            _ => ReplayTradingChoice::None,
        };
        if choice != ReplayTradingChoice::None {
            self.is_open = false;
        }
        choice
    }
}

/// The results of a replay trading session in a dialog: the report of
/// [`PaperAccount::report`](crate::services::PaperAccount::report) in the strategy report view,
/// with its summary, equity and drawdown, list of trades and CSV exports.
#[derive(Debug, Default)]
pub struct ReplayResults {
    panel: StrategyReportPanel,
    /// Whether the dialog is open.
    pub is_open: bool,
}

impl ReplayResults {
    /// Opens the dialog on `report`. `bars` are the replayed source bars, oldest first, for
    /// holding times, excursions and buy-and-hold; pass an empty slice if unavailable.
    pub fn open(&mut self, report: StrategyReport, bars: &[Bar]) {
        self.panel.set_report(Some(report), bars);
        self.is_open = true;
    }

    /// Shows the dialog while it is open and returns the report view's action, such as
    /// [`StrategyReportAction::ExportCsv`] for the host to save.
    pub fn show(&mut self, ctx: &egui::Context) -> StrategyReportAction {
        // Wide enough for the report's two columns of figures, which start at 760 points.
        Modal::new("replay_results", "Replay trading results")
            .width(880.0)
            .height(640.0)
            .show(ctx, &mut self.is_open, |ui| self.panel.show(ui))
            .map_or(StrategyReportAction::None, |response| response.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escape keeps the cursor where it was instead of ending the session, as a confirm
    /// dialog's cancel button would.
    #[test]
    fn replay_trading_prompt_maps_its_buttons_and_escape() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (button, expected) in [
            (Some("Trade from here"), ReplayTradingChoice::NewAccount),
            (Some("End session"), ReplayTradingChoice::EndSession),
            (None, ReplayTradingChoice::Stay),
        ] {
            let mut harness = Harness::builder().allow_missing_glyphs().build_ui_state(
                |ui, (prompt, choice): &mut (ReplayTradingPrompt, ReplayTradingChoice)| {
                    let answer = prompt.show(ui.ctx());
                    if answer != ReplayTradingChoice::None {
                        *choice = answer;
                    }
                },
                (
                    ReplayTradingPrompt { is_open: true },
                    ReplayTradingChoice::None,
                ),
            );
            harness.run();
            match button {
                Some(label) => harness.get_by_label(label).click(),
                None => harness.key_press(egui::Key::Escape),
            }
            harness.run();
            assert_eq!(harness.state().1, expected);
            assert!(!harness.state().0.is_open);
        }
    }
}
