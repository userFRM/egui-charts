//! Icon struct for compile-time embedded SVG icons

use egui::Vec2;

/// A compile-time embedded SVG icon.
///
/// Icons are embedded at compile time using `include_bytes!`, which means
/// they are part of the binary and don't require filesystem access at runtime.
///
/// # Example
///
/// ```ignore
/// use egui_charts::ui_kit::icons::{icons, Icon};
///
/// // Use pre-defined icon
/// ui.add(icons::SETTINGS.as_image(Vec2::splat(20.0)));
///
/// // Create custom icon (typically via icon_from_path! macro)
/// const MY_ICON: Icon = Icon::new("custom.svg", include_bytes!("path/to/custom.svg"));
/// ```
#[derive(Clone, Copy)]
pub struct Icon {
    /// Name/path of the icon (for debugging and cache keys)
    pub name: &'static str,
    /// Raw SVG bytes embedded at compile time
    pub svg_bytes: &'static [u8],
}

impl Icon {
    /// Create a new icon from embedded bytes.
    ///
    /// This is typically called via the `icon_from_path!` macro.
    #[inline]
    pub const fn new(name: &'static str, svg_bytes: &'static [u8]) -> Self {
        Self { name, svg_bytes }
    }

    /// Convert to an egui Image widget with the specified size.
    ///
    /// # Arguments
    ///
    /// * `size` - The size to render the icon at (width and height)
    ///
    /// # Example
    ///
    /// ```ignore
    /// ui.add(icons::SETTINGS.as_image(Vec2::splat(20.0)));
    /// ```
    #[inline]
    pub fn as_image(&self, size: Vec2) -> egui::Image<'static> {
        let uri = format!("bytes://{}", self.name);
        egui::Image::from_bytes(uri, self.svg_bytes).fit_to_exact_size(size)
    }

    /// Convert to an egui Image widget with the specified size and tint color.
    ///
    /// # Arguments
    ///
    /// * `size` - The size to render the icon at
    /// * `tint` - The color to tint the icon with
    #[inline]
    pub fn as_image_tinted(&self, size: Vec2, tint: egui::Color32) -> egui::Image<'static> {
        self.as_image(size).tint(tint)
    }

    /// Convert to an egui Image widget with square dimensions.
    ///
    /// # Arguments
    ///
    /// * `size` - The size (both width and height) to render the icon at
    #[inline]
    pub fn as_image_square(&self, size: f32) -> egui::Image<'static> {
        self.as_image(Vec2::splat(size))
    }

    /// Get the name/path of this icon (useful for debugging).
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Get the raw SVG bytes.
    #[inline]
    pub const fn svg_bytes(&self) -> &'static [u8] {
        self.svg_bytes
    }

    /// Get the size of the SVG data in bytes.
    #[inline]
    pub const fn byte_size(&self) -> usize {
        self.svg_bytes.len()
    }

    /// Convert to an egui Button widget with the icon.
    ///
    /// The button will automatically tint the icon to match text color,
    /// providing proper visual feedback for hover/active states.
    ///
    /// # Arguments
    ///
    /// * `size` - The size to render the icon at
    ///
    /// # Example
    ///
    /// ```ignore
    /// if ui.add(icons::CLOSE.as_button(Vec2::splat(16.0))).clicked() {
    ///     self.close();
    /// }
    /// ```
    #[inline]
    pub fn as_button(&self, size: Vec2) -> egui::Button<'static> {
        egui::Button::image(self.as_image(size)).image_tint_follows_text_color(true)
    }

    /// Convert to an egui Button widget with the icon and a text label.
    ///
    /// # Arguments
    ///
    /// * `size` - The size to render the icon at
    /// * `label` - The text label to display next to the icon
    ///
    /// # Example
    ///
    /// ```ignore
    /// if ui.add(icons::ADD.as_button_with_label(Vec2::splat(16.0), "Add Item")).clicked() {
    ///     self.add_item();
    /// }
    /// ```
    #[inline]
    pub fn as_button_with_label(
        &self,
        size: Vec2,
        label: impl Into<egui::WidgetText>,
    ) -> egui::Button<'static> {
        egui::Button::image_and_text(self.as_image(size), label).image_tint_follows_text_color(true)
    }

    /// Convert to an egui Button widget with square icon dimensions.
    ///
    /// # Arguments
    ///
    /// * `size` - The size (both width and height) to render the icon at
    #[inline]
    pub fn as_button_square(&self, size: f32) -> egui::Button<'static> {
        self.as_button(Vec2::splat(size))
    }
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("name", &self.name)
            .field("byte_size", &self.svg_bytes.len())
            .finish()
    }
}

impl PartialEq for Icon {
    fn eq(&self, other: &Self) -> bool {
        // Compare by name since bytes comparison would be expensive
        self.name == other.name
    }
}

impl Eq for Icon {}

impl std::hash::Hash for Icon {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}
