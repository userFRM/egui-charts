//! Named layouts, kept in the host's storage when it supplies one.

use crate::config::KeyboardAction;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::services::revisions::{Revisions, Settled, Written};
use crate::services::{ConflictChoice, ShareKind, Sharing, Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui::dialogs::keyboard_shortcuts::shortcut_key_labels;
use crate::ui::share_controls::ShareControls;
use crate::ui::top_toolbar::toolbar::DOWNLOAD_ICON;
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::list_item::{ContentContext, ListItem, ListItemContent};
use crate::widget::SavedLayout;
use egui::{Rect, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::{HashMap, VecDeque};
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
    /// Open a new layout under the name the user entered in the menu's name field: trimmed, not
    /// empty, and not the name of a layout in [`LayoutMenu::layouts`]. The host opens it under
    /// that name, with a numeric suffix if one of its open, unsaved layouts already has it.
    CreateNew(String),
    /// Open the named layout.
    Open(String),
    /// Delete the named layout.
    Delete(String),
    /// Rename the current layout from the first name to the second.
    Rename(String, String),
}

/// What the menu's name field names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Naming {
    /// The current layout, under a new name.
    Rename,
    /// A copy of the current layout.
    SaveAs,
    /// A new layout.
    Create,
}

#[derive(Debug)]
enum Request {
    List,
    Open(String),
    /// The name, the document and the edit count it holds.
    Save(String, String, u64),
    /// Saving under the second name, then deleting the first; the document and its edit count.
    Rename(String, String, String, u64),
    Delete(String),
}

/// Named layout menu. The host handles the returned intents, capturing a [`SavedLayout`] for
/// each save. With [`set_storage`](Self::set_storage) the menu lists the layouts of
/// [`StoreKind::Layouts`] and [`save`](Self::save), [`open`](Self::open),
/// [`rename`](Self::rename) and [`delete`](Self::delete) keep them there; without it the host
/// supplies `layouts` and keeps them itself. With a host that keeps revisions each layout is
/// written over the version read; see [`storage_conflict`](Self::storage_conflict).
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
    /// What the name field is open for.
    editing: Option<Naming>,
    /// Whether the name field still waits for the keyboard focus it takes on opening.
    focus_name: bool,
    new_name: String,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    pending: Option<Request>,
    requests: VecDeque<Request>,
    /// The revisions the layouts were read or written at, and the writes the host refused.
    revisions: Revisions,
    /// The edit count of the unsaved layout of each refused write.
    refused_edits: HashMap<String, u64>,
    failing: bool,
    /// The edit count given to `autosave_due` last, and the one saved or opened.
    edits: u64,
    saved_edits: Option<u64>,
    /// The input time from which `autosave_due` asks for the current layout again, after its
    /// write failed.
    retry_at: Option<f64>,
    observed_layout: Option<String>,
    /// A changed edit count with the input time it was first seen.
    changed: Option<(u64, f64)>,
    error: Option<String>,
    /// The context of the last poll, to translate errors of calls between frames.
    ctx: Option<egui::Context>,
    share: ShareControls,
}

impl std::fmt::Debug for LayoutMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutMenu")
            .field("curr_layout", &self.curr_layout)
            .field("layouts", &self.layouts)
            .field("config", &self.config)
            .field("autosave", &self.autosave)
            .field("favorites", &self.favorites)
            .field("favorites_only", &self.favorites_only)
            .field("editing", &self.editing)
            .field("new_name", &self.new_name)
            .field("failing", &self.failing)
            .field("edits", &self.edits)
            .field("saved_edits", &self.saved_edits)
            .field("retry_at", &self.retry_at)
            .field("observed_layout", &self.observed_layout)
            .field("changed", &self.changed)
            .field("error", &self.error)
            .field("ctx", &self.ctx)
            .finish_non_exhaustive()
    }
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
            focus_name: false,
            new_name: String::new(),
            storage: None,
            inbox: Arc::default(),
            pending: None,
            requests: VecDeque::new(),
            revisions: Revisions::new(StoreKind::Layouts),
            refused_edits: HashMap::new(),
            failing: false,
            edits: 0,
            saved_edits: None,
            retry_at: None,
            observed_layout: None,
            changed: None,
            error: None,
            ctx: None,
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
        self.revisions = Revisions::new(StoreKind::Layouts);
        self.refused_edits.clear();
        self.layouts.clear();
        self.saved_edits = None;
        self.retry_at = None;
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
        let Some(storage) = self.storage.clone() else {
            return;
        };
        while self.pending.is_none() {
            let Some(request) = self.requests.pop_front() else {
                return;
            };
            match &request {
                Request::List => storage.list(StoreKind::Layouts, self.inbox.reply(0)),
                Request::Open(name) => self.revisions.load(&*storage, name, &self.inbox, 0),
                Request::Save(name, document, edits) => {
                    // A newer layout of a refused name waits with it for the user's choice.
                    if self.revisions.hold(name, document) {
                        self.refused_edits.insert(name.clone(), *edits);
                        continue;
                    }
                    self.revisions
                        .save(&*storage, name, document.clone(), &self.inbox, 0);
                }
                Request::Rename(_, name, document, _) => {
                    self.revisions
                        .save(&*storage, name, document.clone(), &self.inbox, 0);
                }
                Request::Delete(name) => {
                    storage.delete(StoreKind::Layouts, name, self.inbox.reply(0));
                }
            }
            self.pending = Some(request);
        }
    }

    /// Saves `layout` as `name`, which becomes the current layout. With a layout of that name,
    /// replaces it, over the version read when the host keeps revisions. The layout counts as
    /// saved for [`autosave_due`](Self::autosave_due) once the host has kept it.
    pub fn save(&mut self, name: &str, layout: &SavedLayout) {
        if name.trim().is_empty() {
            self.error = Some("Enter a layout name".into());
            return;
        }
        self.curr_layout = Some(name.to_owned());
        match layout.to_json() {
            Ok(document) => self.request(Request::Save(name.to_owned(), document, self.edits)),
            Err(error) => {
                self.error = Some(l10n::format_text_or_english(
                    self.ctx.as_ref(),
                    "Could not save {name}: {error}",
                    &[("name", name), ("error", error.to_string().as_str())],
                ))
            }
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
        match layout.to_json() {
            Ok(document) => {
                self.request(Request::Rename(old, name.to_owned(), document, self.edits));
            }
            Err(error) => {
                self.error = Some(l10n::format_text_or_english(
                    self.ctx.as_ref(),
                    "Could not rename to {name}: {error}",
                    &[("name", name), ("error", error.to_string().as_str())],
                ))
            }
        }
    }

    /// Deletes the layout `name`.
    pub fn delete(&mut self, name: &str) {
        self.request(Request::Delete(name.to_owned()));
    }

    /// Receives storage's answers, updating the list, and wakes the UI on the next answer.
    /// Returns a layout [`open`](Self::open) read, now the current layout, or the stored version
    /// of the current layout after [`resolve_storage_conflict`](Self::resolve_storage_conflict)
    /// reloaded it, for the host to restore before its next [`autosave_due`](Self::autosave_due).
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<(String, SavedLayout)> {
        self.ctx = Some(ctx.clone());
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let now = ctx.input(|input| input.time);
        if self.share.poll(ctx).is_some() {
            self.request(Request::List);
        }
        let storage = self.storage.clone()?;
        let mut opened = None;
        for (name, settled) in self.revisions.poll(&*storage, ctx) {
            if let Some(reloaded) = self.settle(ctx, name, settled) {
                opened = Some(reloaded);
            }
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
                        let Some(answer) =
                            self.revisions
                                .loaded(&*storage, &name, answer, &self.inbox, 0)
                        else {
                            self.pending = Some(Request::Open(name));
                            continue;
                        };
                        match answer {
                            Ok(Some(document)) => match SavedLayout::from_json(&document) {
                                Ok(layout) => {
                                    self.curr_layout = Some(name.clone());
                                    self.saved_edits = None;
                                    self.retry_at = None;
                                    opened = Some((name, layout));
                                }
                                Err(error) => {
                                    // A write of it is refused rather than replacing a layout
                                    // this release cannot read.
                                    self.revisions.forget(&name);
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
                    Request::Save(name, document, edits) => {
                        let Some(written) = self.revisions.saved(
                            &*storage,
                            &name,
                            &document,
                            answer,
                            &self.inbox,
                            0,
                        ) else {
                            self.pending = Some(Request::Save(name, document, edits));
                            continue;
                        };
                        match written {
                            Written::Saved => {
                                self.failing = false;
                                self.kept(&name, edits);
                                self.insert(name);
                            }
                            Written::Refused(stored) => {
                                // The host answered, so a later failure starts a new run.
                                self.failing = false;
                                if self.curr_layout.as_ref() == Some(&name) {
                                    self.retry_at = None;
                                }
                                self.refused_edits.insert(name.clone(), edits);
                                self.revisions.refuse(&name, document, stored);
                            }
                            Written::Failed(error) => {
                                self.failed(&name, now);
                                if !self.failing {
                                    self.error = Some(l10n::format_text(
                                        ctx,
                                        "Could not save {name}: {error}",
                                        &[
                                            ("name", name.as_str()),
                                            ("error", error.to_string().as_str()),
                                        ],
                                    ));
                                }
                                self.failing = true;
                            }
                        }
                    }
                    Request::Rename(old, name, document, edits) => {
                        let Some(written) = self.revisions.saved(
                            &*storage,
                            &name,
                            &document,
                            answer,
                            &self.inbox,
                            0,
                        ) else {
                            self.pending = Some(Request::Rename(old, name, document, edits));
                            continue;
                        };
                        match written {
                            Written::Saved => {
                                self.failing = false;
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
                                self.kept(&name, edits);
                                self.insert(name);
                            }
                            Written::Refused(_) => {
                                // Another client wrote a layout of the new name, which the
                                // rename would replace unread: the layout keeps its name.
                                self.failing = false;
                                if self.curr_layout.as_ref() == Some(&name) {
                                    self.curr_layout = Some(old);
                                }
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not rename to {name}: a layout of that name exists",
                                    &[("name", name.as_str())],
                                ));
                            }
                            Written::Failed(error) => {
                                self.failing = true;
                                if self.curr_layout.as_ref() == Some(&name) {
                                    self.curr_layout = Some(old.clone());
                                }
                                self.failed(&old, now);
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not rename to {name}: {error}",
                                    &[
                                        ("name", name.as_str()),
                                        ("error", error.to_string().as_str()),
                                    ],
                                ));
                            }
                        }
                    }
                    Request::Delete(name) => {
                        let answer = *answer
                            .downcast::<Result<(), FeedError>>()
                            .expect("layout delete");
                        self.failing = answer.is_err();
                        match answer {
                            Ok(()) => {
                                self.revisions.forget(&name);
                                self.layouts.retain(|layout| *layout != name);
                                self.favorites.retain(|layout| *layout != name);
                            }
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Could not delete {name}: {error}",
                                    &[
                                        ("name", name.as_str()),
                                        ("error", error.to_string().as_str()),
                                    ],
                                ));
                            }
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

    /// Finishes the user's choice for the refused write of `name`. Returns the stored layout to
    /// restore when `name`, the current layout, was reloaded.
    fn settle(
        &mut self,
        ctx: &egui::Context,
        name: String,
        settled: Settled,
    ) -> Option<(String, SavedLayout)> {
        match settled {
            Settled::Overwrite(document) => {
                let edits = self.refused_edits.remove(&name).unwrap_or_default();
                self.request(Request::Save(name, document, edits));
            }
            Settled::Copied(copy) => self.insert(copy),
            Settled::Reload(Some(document)) => {
                self.refused_edits.remove(&name);
                match SavedLayout::from_json(&document) {
                    Ok(layout) if self.curr_layout.as_ref() == Some(&name) => {
                        self.saved_edits = None;
                        self.retry_at = None;
                        return Some((name, layout));
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.revisions.forget(&name);
                        self.error = Some(l10n::format_text(
                            ctx,
                            "Could not open {name}: {error}",
                            &[
                                ("name", name.as_str()),
                                ("error", error.to_string().as_str()),
                            ],
                        ));
                    }
                }
            }
            Settled::Reload(None) => {
                self.refused_edits.remove(&name);
                self.layouts.retain(|layout| *layout != name);
                self.favorites.retain(|layout| *layout != name);
                self.error = Some(l10n::format_text(
                    ctx,
                    "{name} no longer exists",
                    &[("name", name.as_str())],
                ));
            }
            Settled::CopyFailed(reason) => {
                self.error = Some(l10n::format_text(
                    ctx,
                    "Could not keep a copy of {name}: {reason}",
                    &[("name", name.as_str()), ("reason", reason.as_str())],
                ));
            }
        }
        None
    }

    /// The host kept a write of `name` holding the layout at `edits`: if it is the current
    /// layout, it is saved up to them.
    fn kept(&mut self, name: &str, edits: u64) {
        if self.curr_layout.as_deref() == Some(name) {
            self.saved_edits = Some(edits);
            self.retry_at = None;
        }
    }

    /// A write of `name` failed: if it is the current layout, `autosave_due` asks for it again a
    /// second of input time later.
    fn failed(&mut self, name: &str, now: f64) {
        if self.curr_layout.as_deref() == Some(name) {
            // A layout saved under another name became current with this write: `autosave_due`
            // must not take it for a layout opened as stored.
            self.observed_layout = Some(name.to_owned());
            self.retry_at = Some(now + 1.0);
        }
    }

    /// Whether a write of the current layout is queued, in flight or refused. A rename away from
    /// it counts: a write of the old name behind it would bring the old name back after the
    /// rename deletes it.
    fn writing_current(&self) -> bool {
        let Some(current) = &self.curr_layout else {
            return false;
        };
        self.revisions.refused(current)
            || self
                .pending
                .iter()
                .chain(&self.requests)
                .any(|request| match request {
                    Request::Save(name, ..) => name == current,
                    Request::Rename(old, name, ..) => old == current || name == current,
                    Request::List | Request::Open(_) | Request::Delete(_) => false,
                })
    }

    /// Call every frame with a count that changes whenever the layout does, such as the sum of
    /// [`Chart::edit_count`](crate::widget::Chart::edit_count) over its charts. Returns `true`
    /// when the host should save the current layout: autosave is on, the layout is stored, no
    /// write of it is under way or waits for
    /// [`resolve_storage_conflict`](Self::resolve_storage_conflict), and the count changed and
    /// then stayed the same for
    /// [`AUTOSAVE_DELAY`](Self::AUTOSAVE_DELAY). The layout counts as saved only once the host
    /// has kept it: after a failed write this returns `true` again a second of input time later,
    /// without waiting for the delay, until a write succeeds or another layout becomes current.
    pub fn autosave_due(&mut self, ctx: &egui::Context, edits: u64) -> bool {
        self.edits = edits;
        if self.observed_layout != self.curr_layout {
            self.observed_layout = self.curr_layout.clone();
            self.saved_edits = None;
            self.changed = None;
            self.retry_at = None;
        }
        let saved = *self.saved_edits.get_or_insert(edits);
        let stored = self
            .curr_layout
            .as_ref()
            .is_some_and(|name| self.layouts.contains(name));
        if !self.autosave || !stored || self.storage.is_none() || self.writing_current() {
            self.changed = None;
            return false;
        }
        let now = ctx.input(|i| i.time);
        // A failed write is tried again even without a new edit, such as a save under another name.
        if let Some(at) = self.retry_at {
            if now < at {
                ctx.request_repaint_after(Duration::from_secs_f64(at - now));
                return false;
            }
            self.retry_at = None;
            self.changed = None;
            return true;
        }
        if edits == saved {
            self.changed = None;
            return false;
        }
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

    /// The layout whose write the host refused because another client changed or created it
    /// since it was read: the current layout first, then the others by name. `None` while no
    /// write waits, and while the choice to keep a local copy takes effect. The unsaved layout
    /// stays in memory, newer saves of it replace it, and autosave waits; show a
    /// [`StorageConflictDialog`](crate::ui::dialogs::StorageConflictDialog) and pass the choice
    /// to [`resolve_storage_conflict`](Self::resolve_storage_conflict).
    pub fn storage_conflict(&self) -> Option<&str> {
        let current = self.curr_layout.as_deref();
        self.revisions
            .conflicts()
            .find(|name| Some(*name) == current)
            .or_else(|| self.revisions.conflicts().next())
    }

    /// Settles the refused write of the layout `name`, taking effect at the next
    /// [`poll`](Self::poll). [`ConflictChoice::ReloadRemote`] drops the unsaved layout, and
    /// `poll` returns the stored one to restore if `name` is the current layout;
    /// [`ConflictChoice::KeepLocalCopy`] first saves the unsaved layout as `<name> (local copy)`,
    /// which joins `layouts`; [`ConflictChoice::Retry`] writes it over the stored one. Does
    /// nothing if no write of `name` waits.
    pub fn resolve_storage_conflict(&mut self, name: &str, choice: ConflictChoice) {
        if let Some(storage) = self.storage.clone() {
            self.revisions.resolve(&*storage, name, choice);
        }
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
        // The popup's area holds the last frame's size: without a minimum the scroll area keeps
        // to it, and the name field opened by a click would stay clipped below it.
        egui::ScrollArea::vertical()
            .max_height(height)
            .min_scrolled_height(height)
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
        let mut create = false;
        for (icon, label, value) in [
            (
                &DOWNLOAD_ICON,
                "Download chart data",
                Some(LayoutAction::DownloadData),
            ),
            (&icons::PLUS, "Create new layout", None),
            (
                &icons::FOLDER_OPEN,
                "Open layout",
                Some(LayoutAction::OpenLayout),
            ),
        ] {
            if menu_row(ui, Some(icon), label, None, None).clicked() {
                match value {
                    Some(value) => {
                        action = value;
                        ui.close();
                    }
                    // The name field below takes the name; the menu stays open.
                    None => create = true,
                }
            }
        }
        ui.separator();
        let curr = self.curr_layout.clone().unwrap_or_default();
        let rename = menu_row(ui, None, "Rename…", None, None).clicked();
        let save_as = menu_row(ui, None, "Save layout as…", None, None).clicked();
        if rename {
            self.editing = Some(Naming::Rename);
            self.new_name = curr.clone();
        }
        if save_as {
            self.editing = Some(Naming::SaveAs);
            self.new_name.clear();
        }
        if create {
            self.editing = Some(Naming::Create);
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
        if let Some(naming) = self.editing {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_name).desired_width(ui.available_width()),
            );
            // The click that opened the field can take the focus away again within its frame,
            // so the request repeats until the field has it.
            self.focus_name |= save_as || rename || create;
            if self.focus_name {
                if response.has_focus() {
                    self.focus_name = false;
                } else {
                    response.request_focus();
                }
            }
            let name = self.new_name.trim();
            let valid = !name.is_empty() && !self.layouts.iter().any(|l| l == name);
            if ui
                .add_enabled(valid, egui::Button::new(l10n::text(ui.ctx(), "Apply")))
                .clicked()
                || (valid && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                action = match naming {
                    Naming::Rename => LayoutAction::Rename(
                        self.curr_layout.clone().unwrap_or_default(),
                        name.to_owned(),
                    ),
                    Naming::SaveAs => LayoutAction::SaveAs(name.to_owned()),
                    Naming::Create => LayoutAction::CreateNew(name.to_owned()),
                };
                self.editing = None;
                self.focus_name = false;
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
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Holds the reply to a write of `Delayed`, and fails writes without keeping them while the
    /// flag is set.
    #[derive(Default)]
    struct Host(
        Mutex<BTreeMap<String, String>>,
        Mutex<Option<Reply<()>>>,
        AtomicBool,
    );

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self.0.lock().unwrap().keys().rev().cloned().collect()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            if self.2.load(Ordering::Relaxed) {
                return reply.send(Err(FeedError::Network("offline".into())));
            }
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
            sizing: None,
            link_groups: None,
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
    fn failed_layout_saves_stay_unsaved_and_autosave_retries_them_once_a_second() {
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
            sizing: None,
            link_groups: None,
        };
        let mut latest = layout.clone();
        latest.mode = crate::widget::ChartLayoutMode::Single;
        let opened = |host: &Arc<Host>| {
            host.0
                .lock()
                .unwrap()
                .insert("A".into(), layout.to_json().unwrap());
            let mut menu = LayoutMenu::new();
            menu.set_storage(host.clone());
            at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_none()));
            menu.open("A");
            at(0.0, &mut |ctx| assert!(menu.poll(ctx).is_some()));
            menu.autosave = true;
            menu
        };
        // The storage fails before 6 s and from 7.5 s, and the layout changes at 1 s and 7 s.
        // Whether the user also saves it at 1 s, and when autosave asks for it and an error is
        // reported.
        for (manual, due, errors) in [
            (false, [3.0, 4.25, 5.5, 6.75, 9.0], [3.25, 9.25]),
            (true, [2.25, 3.5, 4.75, 6.0, 9.0], [1.25, 9.25]),
        ] {
            let host = Arc::new(Host::default());
            let mut menu = opened(&host);
            let (mut saves, mut reports) = (Vec::new(), Vec::new());
            for time in (0..=37).map(|k| f64::from(k) * 0.25) {
                host.2.store(!(6.0..7.5).contains(&time), Ordering::Relaxed);
                let edits = if time < 1.0 {
                    5
                } else if time < 7.0 {
                    6
                } else {
                    7
                };
                at(time, &mut |ctx| {
                    menu.poll(ctx);
                    if menu.take_error().is_some() {
                        reports.push(time);
                    }
                    if menu.autosave_due(ctx, edits) {
                        saves.push(time);
                        menu.save("A", &latest);
                    }
                });
                if manual && time == 1.0 {
                    menu.save("A", &latest);
                }
            }
            assert_eq!(
                (saves.as_slice(), reports.as_slice()),
                (&due[..], &errors[..])
            );
            let stored = host.0.lock().unwrap()["A"].clone();
            assert_eq!(SavedLayout::from_json(&stored).unwrap().mode, latest.mode);
        }
        // An autosave behind a rename in flight would bring the old name back after the rename
        // deletes it.
        let host = Arc::new(Host::default());
        let mut menu = opened(&host);
        at(0.0, &mut |ctx| assert!(!menu.autosave_due(ctx, 5)));
        menu.rename("Delayed", &layout);
        for time in [1.0, 3.5] {
            at(time, &mut |ctx| {
                if menu.autosave_due(ctx, 6) {
                    menu.save("A", &layout);
                }
            });
        }
        host.1.lock().unwrap().take().unwrap().send(Ok(()));
        menu.poll(&ctx);
        menu.poll(&ctx);
        assert_eq!(
            host.0.lock().unwrap().keys().collect::<Vec<_>>(),
            ["Delayed"]
        );
        assert_eq!(menu.layouts, ["Delayed"]);
        // A save that fails under the name of another stored layout is tried again too, with no
        // edit since the layout was opened.
        let host = Arc::new(Host::default());
        host.0
            .lock()
            .unwrap()
            .insert("B".into(), layout.to_json().unwrap());
        let mut menu = opened(&host);
        at(0.0, &mut |ctx| assert!(!menu.autosave_due(ctx, 5)));
        host.2.store(true, Ordering::Relaxed);
        menu.save("B", &latest);
        at(0.25, &mut |ctx| {
            menu.poll(ctx);
            assert!(!menu.autosave_due(ctx, 5));
        });
        host.2.store(false, Ordering::Relaxed);
        at(1.25, &mut |ctx| assert!(menu.autosave_due(ctx, 5)));
    }

    #[test]
    fn a_layout_changed_elsewhere_is_not_overwritten_and_waits_for_the_users_choice() {
        use crate::services::document::tests::Revised;
        use crate::widget::ChartLayoutMode::{Four, Single, TwoHorizontal};
        let ctx = egui::Context::default();
        let at = |time: f64, run: &mut dyn FnMut(&egui::Context)| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| run(ui.ctx()))
                .drop_without_applying_deltas();
        };
        let layout = |mode| SavedLayout {
            mode,
            charts: Vec::new(),
            sizing: None,
            link_groups: None,
        };
        let document = |mode| layout(mode).to_json().unwrap();
        let stored = |host: &Revised, name: &str| {
            host.get(name)
                .map(|document| SavedLayout::from_json(&document).unwrap().mode)
        };
        // The choice, the layout `poll` returns to restore, and the layouts kept, with the
        // modes stored under `A`, `A (local copy)` and `A (local copy 2)`.
        for (choice, reloaded, layouts, modes) in [
            (
                ConflictChoice::ReloadRemote,
                Some(Four),
                &["A"][..],
                [Some(Four), None, None],
            ),
            (
                ConflictChoice::KeepLocalCopy,
                Some(Four),
                &["A", "A (local copy 2)", "A (local copy)"][..],
                [Some(Four), Some(TwoHorizontal), Some(Single)],
            ),
            (
                ConflictChoice::Retry,
                None,
                &["A"][..],
                [Some(Single), None, None],
            ),
        ] {
            let host = Arc::new(Revised::default());
            host.put("A", &document(TwoHorizontal));
            if choice == ConflictChoice::KeepLocalCopy {
                // A copy kept earlier is never replaced.
                host.put("A (local copy)", &document(TwoHorizontal));
            }
            let mut menu = LayoutMenu::new();
            menu.set_storage(host.clone());
            menu.poll(&ctx);
            menu.open("A");
            assert!(menu.poll(&ctx).is_some());
            // Another client changes the layout after it was read.
            host.put("A", &document(Four));
            menu.save("A", &layout(Single));
            menu.poll(&ctx);
            assert_eq!(menu.storage_conflict(), Some("A"));
            assert_eq!(stored(&host, "A"), Some(Four));
            // Autosave waits for the choice: it would save at 12.5 s otherwise.
            menu.autosave = true;
            at(10.0, &mut |ctx| assert!(!menu.autosave_due(ctx, 5)));
            for time in [10.5, 13.0] {
                at(time, &mut |ctx| assert!(!menu.autosave_due(ctx, 6)));
            }
            menu.resolve_storage_conflict("A", choice);
            let restored = menu.poll(&ctx).map(|(name, layout)| (name, layout.mode));
            assert_eq!(restored, reloaded.map(|mode| ("A".to_owned(), mode)));
            assert_eq!(menu.storage_conflict(), None);
            assert_eq!(menu.layouts, layouts);
            let names = ["A", "A (local copy)", "A (local copy 2)"];
            assert_eq!(names.map(|name| stored(&host, name)), modes);
            if choice == ConflictChoice::ReloadRemote {
                // The next save is made over the version reloaded.
                menu.save("A", &layout(Single));
                menu.poll(&ctx);
                assert_eq!(stored(&host, "A"), Some(Single));
            }
        }
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
                menu.editing = Some(Naming::Rename);
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
