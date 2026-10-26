//! The products launcher: the host's other tools and pages.

use crate::localization as l10n;
use egui::{RichText, Sense, Ui};

use crate::icons::{Icon, icons};
use crate::theme::ThemeExt;
use crate::ui_kit::{EmptyState, PanelHeader};

/// One product the launcher offers.
#[derive(Clone, Debug)]
pub struct Product {
    /// Stable host id, returned when the product is opened.
    pub id: String,
    /// Glyph displayed by the component.
    pub icon: Icon,
    /// Display title.
    pub title: String,
    /// One line under the title. May be empty.
    pub description: String,
}

/// An action the host performs for the products launcher.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum ProductsAction {
    /// No host action.
    #[default]
    None,
    /// Open the product with this id.
    Open(String),
}

/// The products the host offers, opened by the host.
#[derive(Clone, Debug, Default)]
pub struct ProductsPanel {
    /// The products in display order.
    pub products: Vec<Product>,
}

impl ProductsPanel {
    /// A launcher of `products`.
    pub fn new(products: Vec<Product>) -> Self {
        Self { products }
    }

    /// Render the products, returning the one the user opens.
    pub fn show(&self, ui: &mut Ui) -> ProductsAction {
        let mut action = ProductsAction::None;
        let theme = ui.active_theme();
        PanelHeader::new("Products").show_plain(ui);
        if self.products.is_empty() {
            EmptyState::new("No products")
                .icon(&icons::WIDGET_BAR_PRODUCTS)
                .fill_height()
                .show(ui);
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for product in &self.products {
                    let response = ui
                        .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
                            let hovered = ui.response().hovered();
                            egui::Frame::new()
                                .fill(if hovered {
                                    theme.ui.btn_bg_hover
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .corner_radius(theme.spacing.sm)
                                .inner_margin(theme.spacing.sm)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.add(product.icon.as_image_tinted(
                                            egui::Vec2::splat(24.0),
                                            theme.ui.text,
                                        ));
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(l10n::text(ui.ctx(), &product.title))
                                                    .strong(),
                                            );
                                            if !product.description.is_empty() {
                                                ui.label(
                                                    RichText::new(l10n::text(
                                                        ui.ctx(),
                                                        &product.description,
                                                    ))
                                                    .size(theme.text.small)
                                                    .color(theme.ui.text_secondary),
                                                );
                                            }
                                        });
                                    });
                                });
                        })
                        .response;
                    let translated_label = l10n::text(ui.ctx(), &product.title);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                    });
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        action = ProductsAction::Open(product.id.clone());
                    }
                }
            });
        action
    }
}
