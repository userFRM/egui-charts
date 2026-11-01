//! Named layouts, kept in the host's storage when it supplies one.

use crate::config::KeyboardAction;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::services::{ShareKind, Sharing, Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui::dialogs::keyboard_shortcuts::shortcut_key_labels;
use crate::ui::share_controls::ShareControls;
use crate::ui::top_toolbar::toolbar::DOWNLOAD_ICON;
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::list_item::{ContentContext, ListItem, ListItemContent};
use crate::widget::SavedLayout;
use egui::{Rect, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

/// Configuration for layout menu
#[derive(Debug, Clone)]
pub struct LayoutMenuConfig {
    /// Menu width, in egui logical points.
    pub menu_width: f32,
}

impl Default for LayoutMenuConfig {
    fn default() -> Self {
        Self { menu_width: 220.0 }
    }
}

/// Action from layout menu
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LayoutAction {
    /// No action requested.
    None,
    /// Select a tiled or tabbed chart arrangement.
    SelectGrid(crate::widget::ChartLayoutMode),
    /// Save the current layout under its name (Save layout, Ctrl+S).
    Save,
    /// Save the current layout under a new name, which becomes current (Save layout as).
    SaveAs(String),
    /// Export the displayed series through [`Chart::export_csv`](crate::Chart::export_csv).
    DownloadData,
    /// Open the host's saved-layout chooser.
    OpenLayout,
    /// Request a chart image download.
    DownloadImage,
    /// Request a new layout.
    CreateNew,
    /// Open the named layout.
    Open(String),
    /// Delete the named layout.
    Delete(String),
    /// Rename the current layout from the first name to the second.
    Rename(String, String),
}

enum Request {
    List,
    Open(String),
    Save(String, String),
    /// Saving under the second name, then deleting the first.
    Rename(String, String, String),
    Delete(String),
}

/// Named layout menu. The host handles the returned intents, capturing a [`SavedLayout`] for
/// each save. With [`set_storage`](Self::set_storage) the menu lists the layouts of
/// [`StoreKind::Layouts`] and [`save`](Self::save), [`open`](Self::open),
/// [`rename`](Self::rename) and [`delete`](Self::delete) keep them there; without it the host
/// supplies `layouts` and keeps them itself.
pub struct LayoutMenu {
    /// Name of the current layout.
    pub curr_layout: Option<String>,
    /// The layouts available to open, sorted by name when read from storage.
    pub layouts: Vec<String>,
    /// Menu sizing.
    pub config: LayoutMenuConfig,
    /// Whether the current layout is saved after each change; see
    /// [`autosave_due`](Self::autosave_due).
    pub autosave: bool,
    /// Names of the starred layouts. The menu stars and unstars them, moves a name on a rename
    /// and drops it on a delete; keep it between sessions in
    /// [`UserSettings::layout_favorites`](crate::services::UserSettings::layout_favorites).
    pub favorites: Vec<String>,
    favorites_only: bool,
    editing: Option<bool>,
    new_name: String,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    pending: Option<Request>,
    requests: VecDeque<Request>,
    failing: bool,
    /// The edit count given to `autosave_due` last, and the one saved or opened.
    edits: u64,
    saved_edits: Option<u64>,
    observed_layout: Option<String>,
    /// A changed edit count with the input time it was first seen.
    changed: Option<(u64, f64)>,
    error: Option<String>,
    share: ShareControls,
}

impl Default for LayoutMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutMenu {
    /// Creates an unnamed layout menu.
    pub fn new() -> Self {
        Self {
            curr_layout: Some("Unnamed".into()),
            layouts: Vec::new(),
            config: LayoutMenuConfig::default(),
            autosave: false,
            favorites: Vec::new(),
            favorites_only: false,
            editing: None,
            new_name: String::new(),
            storage: None,
            inbox: Arc::default(),
            pending: None,
            requests: VecDeque::new(),
            failing: false,
            edits: 0,
            saved_edits: None,
            observed_layout: None,
            changed: None,
            error: None,
            share: ShareControls::default(),
        }
    }

    /// How long the layout stays unchanged before [`autosave_due`](Self::autosave_due) asks
    /// for a save.
    pub const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

    /// Keep layouts in `storage` and list them from it.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.inbox = Arc::default();
        self.requests.clear();
        self.pending = None;
        self.layouts.clear();
        self.saved_edits = None;
        self.changed = None;
        self.storage = Some(storage);
        self.request(Request::List);
    }

    /// Offers copy-link and save-a-copy on the current stored layout, as `sharing` reports them.
    /// A saved copy is read back from the [`Storage`] given to [`set_storage`](Self::set_storage),
    /// where the host writes it.
    pub fn set_sharing(&mut self, sharing: Arc<dyn Sharing>) {
        self.share.set_service(sharing);
    }

    fn request(&mut self, request: Request) {
        self.requests.push_back(request);
        self.send_next();
    }

    fn send_next(&mut self) {
        let Some(storage) = &self.storage else { return };
        if self.pending.is_some() {
            return;
        }
        let Some(request) = self.requests.pop_front() else {
            return;
        };
        match &request {
            Request::List => storage.list(StoreKind::Layouts, self.inbox.reply(0)),
            Request::Open(name) => storage.load(StoreKind::Layouts, name, self.inbox.reply(0)),
            Request::Save(name, document) | Request::Rename(_, name, document) => {
                storage.save(
                    StoreKind::Layouts,
                    name,
                    document.clone(),
                    self.inbox.reply(0),
                );
            }
            Request::Delete(name) => storage.delete(StoreKind::Layouts, name, self.inbox.reply(0)),
        }
        self.pending = Some(request);
    }

    /// Saves `layout` as `name`, which becomes the current layout. With a layout of that name,
    /// replaces it.
    pub fn save(&mut self, name: &str, layout: &SavedLayout) {
        if name.trim().is_empty() {
            self.error = Some("Enter a layout name".into());
            return;
        }
        self.curr_layout = Some(name.to_owned());
        self.saved_edits = Some(self.edits);
        match layout.to_json() {
            Ok(document) => self.request(Request::Save(name.to_owned(), document)),
            Err(error) => self.error = Some(format!("Could not save {name}: {error}")),
        }
    }

    /// Reads the layout `name`; [`poll`](Self::poll) returns it to restore.
    pub fn open(&mut self, name: &str) {
        self.request(Request::Open(name.to_owned()));
    }

    /// Renames the current layout to `name` by saving `layout`, the current layout, under it
    /// and then deleting the old one.
    pub fn rename(&mut self, name: &str, layout: &SavedLayout) {
        if name.trim().is_empty() {
            self.error = Some("Enter a layout name".into());
            return;
        }
        let old = self.curr_layout.clone().unwrap_or_default();
        self.saved_edits = Some(self.edits);
        match layout.to_json() {
            Ok(document) => self.request(Request::Rename(old, name.to_owned(), document)),
            Err(error) => self.error = Some(format!("Could not rename to {name}: {error}")),
        }
    }

    /// Deletes the layout `name`.
    pub fn delete(&mut self, name: &str) {
        self.request(Request::Delete(name.to_owned()));
    }

    /// Receives storage's answers, updating the list, and wakes the UI on the next answer.
    /// Returns a layout [`open`](Self::open) read, now the current layout, for the host to
    /// restore before its next [`autosave_due`](Self::autosave_due).
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<(String, SavedLayout)> {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let mut opened = None;
        if self.share.poll(ctx).is_some() {
            self.request(Request::List);
        }
        loop {
            let answers = self.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (_, answer) in answers {
                let Some(request) = self.pending.take() else {
                    continue;
                };
                match request {
                    Request::List => {
                        match *answer
                            .downcast::<Result<Vec<String>, FeedError>>()
                            .expect("layout list")
                        {
                            Ok(mut names) => {
                                names.sort();
                                self.layouts = names;
                            }
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not list layouts: {error}",
                                    &[("error", error.to_string().as_str())],
                                ))
                            }
                        }
                    }
                    Request::Open(name) => {
                        let answer = *answer
                            .downcast::<Result<Option<String>, FeedError>>()
                            .expect("layout load");
                        match answer {
                            Ok(Some(document)) => match SavedLayout::from_json(&document) {
                                Ok(layout) => {
                                    self.curr_layout = Some(name.clone());
                                    self.saved_edits = None;
                                    opened = Some((name, layout));
                                }
                                Err(error) => {
                                    self.error = Some(l10n::format_text(
                                        ctx,
                                        "Could not open {name}: {error}",
                                        &[
                                            ("name", name.as_str()),
                                            ("error", error.to_string().as_str()),
                                        ],
                                    ));
                                }
                            },
                            Ok(None) => {
                                self.layouts.retain(|layout| *layout != name);
                                self.favorites.retain(|layout| *layout != name);
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "{name} no longer exists",
                                    &[("name", name.as_str())],
                                ));
                            }
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not open {name}: {error}",
                                    &[
                                        ("name", name.as_str()),
                                        ("error", error.to_string().as_str()),
                                    ],
                                ))
                            }
                        }
                    }
                    request => {
                        let answer = *answer
                            .downcast::<Result<(), FeedError>>()
                            .expect("layout write");
                        let report = answer.is_err() && !self.failing;
                        self.failing = answer.is_err();
                        match (request, answer) {
                            (Request::Save(name, _), Ok(())) => self.insert(name),
                            (Request::Rename(old, name, _), Ok(())) => {
                                if self.curr_layout.as_ref() == Some(&old) {
                                    self.curr_layout = Some(name.clone());
                                }
                                if old != name
                                    && let Some(starred) =
                                        self.favorites.iter_mut().find(|f| **f == old)
                                {
                                    *starred = name.clone();
                                }
                                if old != name {
                                    self.requests.push_front(Request::Delete(old));
                                }
                                self.insert(name);
                            }
                            (Request::Delete(name), Ok(())) => {
                                self.layouts.retain(|layout| *layout != name);
                                self.favorites.retain(|layout| *layout != name);
                            }
                            (Request::Save(name, _), Err(error)) => {
                                if report {
                                    self.error = Some(l10n::format_text(
                                        ctx,
                                        "Could not save {name}: {error}",
                                        &[
                                            ("name", name.as_str()),
                                            ("error", error.to_string().as_str()),
                                        ],
                                    ));
                                }
                            }
                            (Request::Rename(old, name, _), Err(error)) => {
                                if self.curr_layout.as_ref() == Some(&name) {
                                    self.curr_layout = Some(old);
                                }
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not rename to {name}: {error}",
                                    &[
                                        ("name", name.as_str()),
                                        ("error", error.to_string().as_str()),
                                    ],
                                ));
                            }
                            (Request::Delete(name), Err(error)) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not delete {name}: {error}",
                                    &[
                                        ("name", name.as_str()),
                                        ("error", error.to_string().as_str()),
                                    ],
                                ));
                            }
                            (Request::List | Request::Open(_), _) => unreachable!("not a write"),
                        }
                    }
                }
            }
            self.send_next();
        }
        opened
    }

    fn insert(&mut self, name: String) {
        if let Err(at) = self.layouts.binary_search(&name) {
            self.layouts.insert(at, name);
        }
    }

    /// Call every frame with a count that changes whenever the layout does, such as the sum of
    /// [`Chart::edit_count`](crate::widget::Chart::edit_count) over its charts. Returns `true`
    /// when the host should save the current layout: autosave is on, the layout is stored, and
    /// the count changed and then stayed the same for [`AUTOSAVE_DELAY`](Self::AUTOSAVE_DELAY).
    pub fn autosave_due(&mut self, ctx: &egui::Context, edits: u64) -> bool {
        self.edits = edits;
        if self.observed_layout != self.curr_layout {
            self.observed_layout = self.curr_layout.clone();
            self.saved_edits = None;
            self.changed = None;
        }
        let saved = *self.saved_edits.get_or_insert(edits);
        let stored = self
            .curr_layout
            .as_ref()
            .is_some_and(|name| self.layouts.contains(name));
        if !self.autosave || !stored || self.storage.is_none() || edits == saved {
            self.changed = None;
            return false;
        }
        let now = ctx.input(|i| i.time);
        let since = match self.changed {
            Some((changed, since)) if changed == edits => since,
            _ => {
                self.changed = Some((edits, now));
                now
            }
        };
        let wait = Self::AUTOSAVE_DELAY.as_secs_f64() - (now - since);
        if wait > 0.0 {
            ctx.request_repaint_after(Duration::from_secs_f64(wait));
            return false;
        }
        self.changed = None;
        true
    }

    /// The error of a failed storage request since the last call, to show to the user.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Width of the named trigger.
    pub fn width(&self, ui: &Ui) -> f32 {
        let label = l10n::text(ui.ctx(), self.curr_layout.as_deref().unwrap_or("Unnamed"));
        ui.fonts_mut(|f| {
            f.layout_no_wrap(
                label,
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            )
            .size()
            .x
        })
        .min(self.config.menu_width - 32.0)
            + 32.0
    }

    /// Shows the current name and an anchored menu. Actions do not mutate the host's catalog.
    pub fn show(&mut self, ui: &mut Ui) -> LayoutAction {
        let (rect, button) =
            ui.allocate_exact_size(egui::vec2(self.width(ui), 28.0), egui::Sense::click());
        if button.hovered() {
            ui.painter().rect_filled(
                rect,
                ui.active_theme().radius.sm,
                ui.visuals().widgets.hovered.bg_fill,
            );
        }
        ui.painter()
            .with_clip_rect(egui::Rect::from_min_max(
                rect.min,
                rect.max - egui::vec2(22.0, 0.0),
            ))
            .text(
                rect.left_center() + egui::vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                l10n::text(ui.ctx(), self.curr_layout.as_deref().unwrap_or("Unnamed")),
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
        crate::icons::icons::CHEVRON_DOWN
            .as_image_tinted(egui::Vec2::splat(12.0), ui.visuals().text_color())
            .paint_at(
                ui,
                egui::Rect::from_center_size(
                    rect.right_center() - egui::vec2(10.0, 0.0),
                    egui::Vec2::splat(12.0),
                ),
            );
        let mut action = LayoutAction::None;
        let translated_label = l10n::text(ui.ctx(), "Named layouts");
        button.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &translated_label)
        });
        crate::styles::focus::draw_focus_ring(ui, &button, ui.active_theme().radius.sm);
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                action = self.show_menu(ui);
            });
        action
    }

    /// Named layout menu contents, shared with the save dropdown.
    pub fn show_menu(&mut self, ui: &mut Ui) -> LayoutAction {
        let height =
            (ui.ctx().content_rect().bottom() - ui.next_widget_position().y - 16.0).max(0.0);
        egui::ScrollArea::vertical()
            .max_height(height)
            .show(ui, |ui| self.show_contents(ui))
            .inner
    }

    fn show_contents(&mut self, ui: &mut Ui) -> LayoutAction {
        let width = self
            .config
            .menu_width
            .min(ui.ctx().content_rect().width() - 24.0);
        ui.set_width(width.max(0.0));
        let mut action = LayoutAction::None;
        let ctx = ui.ctx().clone();
        let shortcut = ctx
            .data(|data| {
                data.get_temp::<crate::widget::ShortcutMap>(egui::Id::NULL.with("chart_shortcuts"))
            })
            .map_or_else(
                || KeyboardAction::SaveLayout.shortcut(),
                |map| map.shortcut(KeyboardAction::SaveLayout),
            );
        // Keys as the toolbar's hover labels name them: "Ctrl + S".
        let shortcut = shortcut.map(|shortcut| {
            let font = egui::FontId::proportional(LABEL_SIZE);
            let keys: Vec<_> = shortcut_key_labels(&ctx, &font, &shortcut)
                .iter()
                .map(|key| l10n::text(&ctx, key))
                .collect();
            keys.join(" + ")
        });
        if menu_row(ui, None, "Save layout", shortcut.as_deref(), None).clicked() {
            action = LayoutAction::Save;
            ui.close();
        }
        for (icon, label, value) in [
            (
                &DOWNLOAD_ICON,
                "Download chart data",
                LayoutAction::DownloadData,
            ),
            (&icons::PLUS, "Create new layout", LayoutAction::CreateNew),
            (&icons::FOLDER_OPEN, "Open layout", LayoutAction::OpenLayout),
        ] {
            if menu_row(ui, Some(icon), label, None, None).clicked() {
                action = value;
                ui.close();
            }
        }
        ui.separator();
        let curr = self.curr_layout.clone().unwrap_or_default();
        let rename = menu_row(ui, None, "Rename…", None, None).clicked();
        let save_as = menu_row(ui, None, "Save layout as…", None, None).clicked();
        if rename {
            self.editing = Some(true);
            self.new_name = curr.clone();
        }
        if save_as {
            self.editing = Some(false);
            self.new_name.clear();
        }
        ui.separator();
        let image = menu_row(ui, Some(&icons::CAMERA), "Download chart image", None, None);
        if image.clicked() {
            action = LayoutAction::DownloadImage;
            ui.close();
        }
        if menu_row(ui, None, "Autosave", None, Some(self.autosave))
            .on_hover_text(l10n::text(ui.ctx(), "Save the layout after each change"))
            .clicked()
        {
            self.autosave = !self.autosave;
        }
        if let Some(is_rename) = self.editing {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_name).desired_width(ui.available_width()),
            );
            if save_as || rename {
                response.request_focus();
            }
            let name = self.new_name.trim();
            let valid = !name.is_empty() && !self.layouts.iter().any(|l| l == name);
            if ui
                .add_enabled(valid, egui::Button::new(l10n::text(ui.ctx(), "Apply")))
                .clicked()
                || (valid && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                action = if is_rename {
                    LayoutAction::Rename(
                        self.curr_layout.clone().unwrap_or_default(),
                        name.to_owned(),
                    )
                } else {
                    LayoutAction::SaveAs(name.to_owned())
                };
                self.editing = None;
                ui.close();
            }
        }
        if let Some(name) = self.curr_layout.as_deref()
            && self.layouts.iter().any(|layout| layout == name)
        {
            self.share.show(ui, ShareKind::Layout, name);
        }
        if self.layouts.is_empty() {
            return action;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.colored_label(
                ui.active_theme().ui.text_secondary,
                l10n::text(ui.ctx(), "Layouts"),
            );
            ui.toggle_value(&mut self.favorites_only, l10n::text(ui.ctx(), "Favorites"))
                .on_hover_text(l10n::text(ui.ctx(), "Show only favorite layouts"));
        });
        let shown: Vec<&String> = self
            .layouts
            .iter()
            .filter(|layout| !self.favorites_only || self.favorites.contains(layout))
            .collect();
        if shown.is_empty() {
            ui.colored_label(
                ui.active_theme().ui.text_secondary,
                l10n::text(ui.ctx(), "No favorite layouts"),
            );
        }
        let mut starred = None;
        egui::ScrollArea::vertical()
            .max_height(ui.available_height().clamp(0.0, 240.0))
            .show(ui, |ui| {
                for layout in shown {
                    ui.horizontal(|ui| {
                        let favorite = self.favorites.contains(layout);
                        if IconButton::new(if favorite {
                            &icons::STAR_FILLED
                        } else {
                            &icons::STAR_EMPTY
                        })
                        .size(ButtonSize::SM)
                        .tooltip(if favorite {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        })
                        .show(ui)
                        .clicked()
                        {
                            starred = Some(layout.clone());
                        }
                        if ui
                            .add_sized(
                                egui::vec2(
                                    (ui.available_width() - 28.0).max(0.0),
                                    ui.spacing().interact_size.y,
                                ),
                                egui::Button::new(l10n::text(ui.ctx(), layout))
                                    .selected(self.curr_layout.as_deref() == Some(layout.as_str()))
                                    .frame(false)
                                    .truncate(),
                            )
                            .on_hover_text(l10n::text(ui.ctx(), layout))
                            .clicked()
                        {
                            action = LayoutAction::Open(layout.clone());
                            ui.close();
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text(l10n::text(ui.ctx(), "Delete layout"))
                            .clicked()
                        {
                            action = LayoutAction::Delete(layout.clone());
                            ui.close();
                        }
                    });
                }
            });
        if let Some(name) = starred {
            if let Some(at) = self.favorites.iter().position(|f| *f == name) {
                self.favorites.remove(at);
            } else {
                self.favorites.push(name);
            }
        }
        action
    }
}

/// Text size of the save menu's labels and shortcut.
const LABEL_SIZE: f32 = 15.0;
/// Size of the glyphs before the save menu's labels.
const GLYPH_SIZE: f32 = 17.0;

/// A 32 px save menu entry: an optional glyph, the label after the glyph column, and at the end a
/// muted `shortcut` or, for an on and off entry, a `switch` showing its state.
fn menu_row(
    ui: &mut Ui,
    icon: Option<&Icon>,
    label: &str,
    shortcut: Option<&str>,
    switch: Option<bool>,
) -> egui::Response {
    ListItem::new().height(32.0).show(
        ui,
        MenuRow {
            icon,
            label,
            shortcut,
            switch,
        },
    )
}

struct MenuRow<'a> {
    icon: Option<&'a Icon>,
    label: &'a str,
    shortcut: Option<&'a str>,
    switch: Option<bool>,
}

impl ListItemContent for MenuRow<'_> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> egui::Response {
        let theme = ui.active_theme();
        let rect = context.rect;
        let response = context.response;
        let label = l10n::text(ui.ctx(), self.label);
        response.widget_info(|| match self.switch {
            Some(on) => WidgetInfo::selected(Role::Switch, response.enabled(), on, &label),
            None => WidgetInfo::labeled(Role::MenuItem, response.enabled(), &label),
        });
        if let Some(icon) = self.icon {
            let glyph = Rect::from_center_size(
                pos2(rect.left() + GLYPH_SIZE / 2.0, rect.center().y),
                vec2(GLYPH_SIZE, GLYPH_SIZE),
            );
            icon.as_image_tinted(glyph.size(), context.visuals.icon_tint(ui.visuals()))
                .paint_at(ui, glyph);
        }
        let font = egui::FontId::proportional(LABEL_SIZE);
        ui.painter().text(
            pos2(rect.left() + GLYPH_SIZE + theme.spacing.lg, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font.clone(),
            context.visuals.text_color(ui.visuals()),
        );
        if let Some(shortcut) = self.shortcut {
            ui.painter().text(
                rect.right_center(),
                egui::Align2::RIGHT_CENTER,
                shortcut,
                font,
                theme.ui.text_muted,
            );
        }
        if let Some(on) = self.switch {
            let switch = Rect::from_min_size(
                pos2(rect.right() - 38.0, rect.center().y - 10.0),
                vec2(38.0, 20.0),
            );
            let track = if on {
                theme.ui.text
            } else {
                theme.ui.border_control
            };
            ui.painter().rect_filled(switch, 10.0, track);
            let knob = if on {
                switch.right() - 10.0
            } else {
                switch.left() + 10.0
            };
            ui.painter()
                .circle_filled(pos2(knob, switch.center().y), 7.0, theme.ui.panel_bg);
        }
        response.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host(Mutex<BTreeMap<String, String>>, Mutex<Option<Reply<()>>>);

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self.0.lock().unwrap().keys().rev().cloned().collect()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().insert(name.into(), document);
            if name == "Delayed" {
                assert!(
                    self.1.lock().unwrap().replace(reply).is_none(),
                    "writes must be serialized"
                );
            } else {
                reply.send(Ok(()));
            }
        }
        fn delete(&self, _: StoreKind, name: &str, reply: Reply<()>) {
            self.0.lock().unwrap().remove(name);
            reply.send(Ok(()));
        }
    }

    #[test]
    fn layouts_save_rename_open_and_autosave_through_storage() {
        let ctx = egui::Context::default();
        let at = |time: f64, run: &mut dyn FnMut(&egui::Context)| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| run(ui.ctx()))
                .drop_without_applying_deltas();
        };
        let layout = SavedLayout {
            mode: crate::widget::ChartLayoutMode::TwoHorizontal,
            charts: Vec::new(),
        };
        let host = Arc::new(Host::default());
        host.0
            .lock()
            .unwrap()
            .insert("B".into(), layout.to_json().unwrap());
        let mut menu = LayoutMenu::new();
        menu.set_storage(host.clone());
        at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_none()));
        assert_eq!(menu.layouts, ["B"]);
        menu.save("C", &layout);
        at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_none()));
        assert_eq!(menu.layouts, ["B", "C"]);
        // A rename writes the new name before it deletes the old one, and the star follows it.
        menu.favorites = vec!["C".into(), "B".into()];
        menu.rename("A", &layout);
        at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_none()));
        assert!(host.0.lock().unwrap().contains_key("A"));
        at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_none()));
        assert_eq!(menu.layouts, ["A", "B"]);
        assert_eq!(
            host.0.lock().unwrap().keys().collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(menu.favorites, ["A", "B"]);
        menu.rename("A", &layout);
        at(0.0, &mut |ctx| {
            menu.poll(ctx);
        });
        assert!(
            host.0.lock().unwrap().contains_key("A"),
            "renaming to the same name must not delete it"
        );
        menu.open("B");
        at(0.0, &mut |ctx| {
            let (name, opened) = menu.poll(ctx).unwrap();
            assert_eq!((name.as_str(), opened.mode), ("B", layout.mode));
        });
        assert_eq!(menu.curr_layout.as_deref(), Some("B"));
        // Autosave waits for the layout to stay unchanged after an edit.
        menu.autosave = true;
        let mut due = Vec::new();
        for (time, edits) in [(0.0, 5), (1.0, 6), (2.5, 7), (4.0, 7), (4.6, 7), (5.0, 7)] {
            at(time, &mut |ctx| due.push(menu.autosave_due(ctx, edits)));
            if due.last() == Some(&true) {
                menu.save("B", &layout);
            }
        }
        assert_eq!(due, [false, false, false, false, true, false]);
        menu.poll(&ctx);
        menu.save("Delayed", &layout);
        let mut latest = layout.clone();
        latest.mode = crate::widget::ChartLayoutMode::Single;
        menu.save("Delayed", &latest);
        let first = host.0.lock().unwrap()["Delayed"].clone();
        assert_eq!(SavedLayout::from_json(&first).unwrap().mode, layout.mode);
        host.1.lock().unwrap().take().unwrap().send(Ok(()));
        menu.poll(&ctx);
        let last = host.0.lock().unwrap()["Delayed"].clone();
        assert_eq!(SavedLayout::from_json(&last).unwrap().mode, latest.mode);
        host.1.lock().unwrap().take().unwrap().send(Ok(()));
        menu.poll(&ctx);
        menu.delete("B");
        menu.poll(&ctx);
        assert_eq!(menu.favorites, ["A"]);
        // Changing tabs starts a new debounce, without autosaving an unchanged tab.
        menu.curr_layout = Some("A".into());
        at(10.0, &mut |ctx| assert!(!menu.autosave_due(ctx, 99)));
        at(20.0, &mut |ctx| assert!(!menu.autosave_due(ctx, 99)));
    }

    #[test]
    fn long_layout_names_keep_menu_actions_inside_both_theme_windows() {
        let entries = [
            "Save layout",
            "Download chart data",
            "Create new layout",
            "Open layout",
            "Rename…",
            "Save layout as…",
            "Download chart image",
            "Autosave",
        ];
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for size in [egui::vec2(320.0, 360.0), egui::vec2(1440.0, 900.0)] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                ctx.enable_accesskit();
                let mut menu = LayoutMenu::new();
                menu.layouts = (0..20)
                    .map(|i| format!("{i} {}", "Long layout ".repeat(30)))
                    .collect();
                menu.curr_layout = Some(menu.layouts[0].clone());
                menu.editing = Some(true);
                menu.new_name = menu.layouts[0].clone();
                for pass in 0..2 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| {
                            assert!(menu.width(ui) <= menu.config.menu_width);
                            egui::Area::new(egui::Id::NULL.with("layout menu"))
                                .fixed_pos(egui::pos2(8.0, 8.0))
                                .show(ui.ctx(), |ui| {
                                    menu.show_menu(ui);
                                    assert!(ui.min_rect().right() <= size.x);
                                    assert!(ui.min_rect().bottom() <= size.y);
                                });
                        },
                    );
                    if pass == 1 {
                        // Every entry is a 32 px row with its label at one indent.
                        let label_x: Vec<_> = entries
                            .iter()
                            .map(|entry| {
                                output.shapes.iter().find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.text() == *entry => {
                                        Some(text.pos.x)
                                    }
                                    _ => None,
                                })
                            })
                            .collect();
                        assert!(label_x.iter().all(|x| x.is_some() && *x == label_x[0]));
                        let rows: Vec<_> = output
                            .platform_output
                            .accesskit_update
                            .iter()
                            .flat_map(|update| &update.nodes)
                            .filter(|(_, node)| node.label().is_some_and(|l| entries.contains(&l)))
                            .filter_map(|(_, node)| node.bounds())
                            .collect();
                        assert_eq!(rows.len(), entries.len());
                        assert!(rows.iter().all(|row| row.height() == 32.0), "{rows:?}");
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }
}
