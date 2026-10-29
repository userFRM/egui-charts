//! Profile details and follow controls for a host's [`ProfileDirectory`].

use crate::localization as l10n;
use crate::services::{FollowStatus, ProfileDirectory, ProfileId};
use crate::theme::ThemeExt;
use crate::ui_kit::EmptyState;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use egui::{RichText, Ui};

/// A profile card that embeds in any layout and sends follow requests to the directory's
/// [`Profiles`](crate::services::Profiles) service. Details and follow state stay host-owned.
/// Nothing is cached in the card, so replacing the directory's service clears what it shows.
///
/// Handle `NotificationsAction::OpenProfile(id)` by selecting `id` in the host's layout, then
/// call [`show`](Self::show) for it. Keep polling the directory every frame, including while
/// the card is hidden. Install an egui image loader to display host-provided avatar URIs.
///
/// ```
/// use egui_charts::services::{ProfileDirectory, ProfileId};
/// use egui_charts::ui::profile::ProfileCard;
///
/// let mut directory = ProfileDirectory::new(); // Attach the host with set_service.
/// let selected = ProfileId("ada".into());
/// # egui::__run_test_ui(|ui| {
/// directory.poll(ui.ctx());
/// ProfileCard::new(&selected).show(ui, &mut directory);
/// # });
/// ```
pub struct ProfileCard<'a> {
    id: &'a ProfileId,
}

impl<'a> ProfileCard<'a> {
    /// Shows the profile with this host identifier, or an unavailable state until it is pushed.
    pub fn new(id: &'a ProfileId) -> Self {
        Self { id }
    }

    /// Draws the latest details, follow control and any pending request or host failure.
    /// A failed request keeps the reported follow state and may be retried or dismissed.
    pub fn show(self, ui: &mut Ui, directory: &mut ProfileDirectory) {
        let Some(profile) = directory.profile(self.id) else {
            EmptyState::new(&l10n::text(ui.ctx(), "Profile unavailable")).show(ui);
            return;
        };
        let theme = ui.active_theme();
        let operation = directory.operation(self.id);
        let pending = operation.is_some_and(|op| op.status == FollowStatus::Pending);
        let mut follow = None;
        let mut dismiss = false;
        ui.push_id(("profile", self.id), |ui| {
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some(avatar) = &profile.avatar {
                        ui.add(
                            egui::Image::new(avatar)
                                .fit_to_exact_size(egui::vec2(48.0, 48.0))
                                .alt_text(&profile.name)
                                .corner_radius(24),
                        );
                    }
                    ui.label(
                        RichText::new(&profile.name)
                            .size(theme.text.heading)
                            .strong(),
                    );
                });
                if !profile.bio.is_empty() {
                    ui.label(&profile.bio);
                }
                if let Some(followers) = profile.followers {
                    ui.label(
                        RichText::new(l10n::format_text(
                            ui.ctx(),
                            "Followers: {count}",
                            &[("count", followers.to_string())],
                        ))
                        .color(theme.ui.text_secondary),
                    );
                }
                ui.add_space(theme.spacing.sm);
                ui.horizontal_wrapped(|ui| {
                    let label = if pending {
                        if operation.is_some_and(|op| op.follow) {
                            "Following…"
                        } else {
                            "Unfollowing…"
                        }
                    } else if profile.followed {
                        "Unfollow"
                    } else {
                        "Follow"
                    };
                    if Button::new(l10n::text(ui.ctx(), label))
                        .primary()
                        .enabled(!pending)
                        .show(ui)
                        .clicked()
                    {
                        follow = Some(!profile.followed);
                    }
                    if pending {
                        ui.spinner();
                    }
                });
                if let Some(operation) = operation
                    && let FollowStatus::Failed(message) = &operation.status
                {
                    status_message(ui, MessageKind::Error, message);
                    dismiss = Button::new(l10n::text(ui.ctx(), "Dismiss"))
                        .small()
                        .show(ui)
                        .clicked();
                }
            });
        });
        if let Some(follow) = follow {
            directory.follow(self.id, follow);
        } else if dismiss {
            directory.dismiss(self.id);
        }
    }
}

#[cfg(test)]
mod tests;
