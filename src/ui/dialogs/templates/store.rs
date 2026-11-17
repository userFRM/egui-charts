//! Sequential storage requests keep a late save or delete from racing another edit.

use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::drawings::{Drawing, DrawingTemplate};
use crate::services::{Storage, StoreKind};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

#[derive(serde::Serialize, serde::Deserialize)]
struct Document<T> {
    version: u32,
    template: T,
}

#[derive(Debug)]
enum Pending {
    List,
    Load(String),
    Save,
    Delete(String),
}

/// Named, host-readable templates. Without a storage service they live in memory. With a
/// service attached, poll every frame, including while the template UI is closed. A failed
/// read or an unknown document version disables writes until a successful reload, preventing
/// accidental replacement of documents that could not be understood.
pub struct TemplateStore<T> {
    entries: BTreeMap<String, T>,
    kind: StoreKind,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    pending: Option<Pending>,
    remaining: VecDeque<String>,
    loaded: bool,
    /// Last storage or document error. Failed saves retain the edited template in memory.
    pub error: Option<String>,
}

impl<T> std::fmt::Debug for TemplateStore<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateStore")
            .field("kind", &self.kind)
            .field("remaining", &self.remaining)
            .field("loaded", &self.loaded)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<T: Clone + Serialize + DeserializeOwned> TemplateStore<T> {
    /// An empty catalog in the supplied namespace, initially kept only in memory.
    ///
    /// ```
    /// use egui_charts::model::Timeframe;
    /// use egui_charts::services::StoreKind;
    /// use egui_charts::studies::StudyTemplateSet;
    /// use egui_charts::ui::dialogs::templates::TemplateStore;
    ///
    /// let mut store = TemplateStore::new(StoreKind::StudyTemplates);
    /// let mut set = StudyTemplateSet::default();
    /// set.interval = Some(Timeframe::Day1);
    /// store.save("Daily", set).unwrap();
    /// assert_eq!(store.entries()["Daily"].interval, Some(Timeframe::Day1));
    /// ```
    pub fn new(kind: StoreKind) -> Self {
        Self {
            entries: BTreeMap::new(),
            kind,
            storage: None,
            inbox: Arc::default(),
            pending: None,
            remaining: VecDeque::new(),
            loaded: true,
            error: None,
        }
    }

    /// Read the catalog, sorted by document name, for a host-provided menu or export. Drawing
    /// templates saved with [`save_for_tool`](TemplateStore::save_for_tool) have names prefixed
    /// by their tool; [`template_name`](TemplateStore::template_name) gives the name to show.
    pub fn entries(&self) -> &BTreeMap<String, T> {
        &self.entries
    }

    /// Whether saving, loading or deleting can begin. Reads must finish before writes start.
    pub fn ready(&self) -> bool {
        self.loaded && self.pending.is_none()
    }

    /// Attach the host storage service and read its catalog. No document is written on attach.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.storage = Some(storage);
        self.pending = None;
        self.reload();
    }

    /// Retry reading the catalog after a failed read. While a request is outstanding this does
    /// nothing, so reloading cannot race a write or discard an unsaved edit.
    pub fn reload(&mut self) {
        if self.pending.is_some() {
            return;
        }
        if let Some(storage) = &self.storage {
            self.entries.clear();
            self.remaining.clear();
            self.inbox = Arc::default();
            self.loaded = false;
            self.error = None;
            self.pending = Some(Pending::List);
            storage.list(self.kind, self.inbox.reply(0));
        }
    }

    /// Receive storage answers and wake the UI when the next asynchronous answer arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (_, answer) in self.inbox.drain().answers {
            let Some(pending) = self.pending.take() else {
                continue;
            };
            let result = match pending {
                Pending::List => {
                    let result = *answer
                        .downcast::<Result<Vec<String>, FeedError>>()
                        .expect("template list");
                    result
                        .map(|names| {
                            self.remaining = names.into_iter().collect();
                        })
                        .map_err(|error| error.to_string())
                }
                Pending::Load(name) => {
                    let result = *answer
                        .downcast::<Result<Option<String>, FeedError>>()
                        .expect("template load");
                    result
                        .map_err(|error| error.to_string())
                        .and_then(|document| {
                            if let Some(document) = document {
                                let document: Document<T> = serde_json::from_str(&document)
                                    .map_err(|error| error.to_string())?;
                                if document.version != 1 {
                                    return Err("Unsupported template version".into());
                                }
                                self.entries.insert(name, document.template);
                            }
                            Ok(())
                        })
                }
                Pending::Save | Pending::Delete(_) => {
                    let result = *answer
                        .downcast::<Result<(), FeedError>>()
                        .expect("template write");
                    result
                        .map(|()| {
                            if let Pending::Delete(name) = pending {
                                self.entries.remove(&name);
                            }
                        })
                        .map_err(|error| error.to_string())
                }
            };
            if let Err(error) = result {
                self.error = Some(error);
                self.remaining.clear();
            } else if !self.loaded {
                if let Some(name) = self.remaining.pop_front() {
                    self.storage.as_ref().expect("template storage").load(
                        self.kind,
                        &name,
                        self.inbox.reply(0),
                    );
                    self.pending = Some(Pending::Load(name));
                } else {
                    self.loaded = true;
                }
            }
        }
    }

    /// Save or explicitly replace a named template. Blank names and concurrent writes are rejected.
    /// Save drawing templates with [`save_for_tool`](TemplateStore::save_for_tool), which keeps
    /// each tool's templates apart.
    pub fn save(&mut self, name: &str, template: T) -> Result<(), String> {
        if !self.ready() {
            return Err("Templates are not ready".into());
        }
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a template name".into());
        }
        let document = serde_json::to_string(&Document {
            version: 1,
            template: template.clone(),
        })
        .map_err(|error| error.to_string())?;
        self.entries.insert(name.into(), template);
        self.error = None;
        if let Some(storage) = &self.storage {
            self.pending = Some(Pending::Save);
            storage.save(self.kind, name, document, self.inbox.reply(0));
        }
        Ok(())
    }

    /// Delete a named template. With storage attached the row remains until deletion succeeds.
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        if !self.ready() {
            return Err("Templates are not ready".into());
        }
        self.error = None;
        if let Some(storage) = &self.storage {
            self.pending = Some(Pending::Delete(name.into()));
            storage.delete(self.kind, name, self.inbox.reply(0));
        } else {
            self.entries.remove(name);
        }
        Ok(())
    }
}

impl TemplateStore<DrawingTemplate> {
    /// Save or replace this tool's template `name`. Templates are kept per tool, built-in or
    /// host-registered, so several tools can each have a template of one name. The document is
    /// named after the tool and the template, as in `TrendLine/Red`, or `Polyline:host-path/Red`
    /// for a host tool; [`template_name`](Self::template_name) gives the name to show. A template
    /// this tool saved under its name alone, before templates were kept per tool, is replaced in
    /// place. Blank names, concurrent writes and a document name held by another tool's template
    /// are rejected.
    ///
    /// ```
    /// use egui_charts::drawings::{Drawing, DrawingTemplate, DrawingToolType};
    /// use egui_charts::services::StoreKind;
    /// use egui_charts::ui::dialogs::templates::TemplateStore;
    ///
    /// let mut store = TemplateStore::<DrawingTemplate>::new(StoreKind::DrawingTemplates);
    /// for tool in [DrawingToolType::HorizontalLine, DrawingToolType::VerticalLine] {
    ///     let red = DrawingTemplate::from_drawing(&Drawing::new(1, tool));
    ///     store.save_for_tool("Red", red).unwrap();
    /// }
    /// let documents: Vec<_> = store.entries().keys().map(String::as_str).collect();
    /// assert_eq!(documents, ["HorizontalLine/Red", "VerticalLine/Red"]);
    /// assert_eq!(store.template_name("VerticalLine/Red"), "Red");
    /// ```
    pub fn save_for_tool(&mut self, name: &str, template: DrawingTemplate) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a template name".into());
        }
        let tool = &template.settings;
        let document = self
            .entries
            .iter()
            .find(|(document, saved)| {
                same_tool(&saved.settings, tool) && self.template_name(document) == name
            })
            .map_or_else(
                || format!("{}{name}", tool_prefix(tool)),
                |(document, _)| document.clone(),
            );
        if self
            .entries
            .get(&document)
            .is_some_and(|saved| !same_tool(&saved.settings, tool))
        {
            return Err("A template of another tool is saved under this name".into());
        }
        self.save(&document, template)
    }

    /// The name to show for the template saved as `document`: the document name without its
    /// tool's prefix, or all of it for a template saved under its name alone.
    pub fn template_name<'a>(&self, document: &'a str) -> &'a str {
        self.entries
            .get(document)
            .and_then(|saved| document.strip_prefix(tool_prefix(&saved.settings).as_str()))
            .unwrap_or(document)
    }
}

fn same_tool(a: &Drawing, b: &Drawing) -> bool {
    a.tool_type == b.tool_type && a.custom_tool == b.custom_tool
}

/// `TrendLine/`, or `Polyline:host-path/` for a host tool. Tool variant names hold neither `:`
/// nor `/`, and the escaped host id holds no `/`, so no two tools share a prefix.
fn tool_prefix(drawing: &Drawing) -> String {
    let tool = serde_json::to_value(drawing.tool_type)
        .ok()
        .and_then(|tool| tool.as_str().map(str::to_owned))
        .unwrap_or_default();
    match &drawing.custom_tool {
        Some(id) => format!("{tool}:{}/", id.replace('%', "%25").replace('/', "%2F")),
        None => format!("{tool}/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host {
        documents: Mutex<BTreeMap<String, String>>,
        fail_write: Mutex<bool>,
    }

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self.documents.lock().unwrap().keys().cloned().collect()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.documents.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            if *self.fail_write.lock().unwrap() {
                reply.send(Err(FeedError::NotSupported));
            } else {
                self.documents.lock().unwrap().insert(name.into(), document);
                reply.send(Ok(()));
            }
        }
        fn delete(&self, _: StoreKind, name: &str, reply: Reply<()>) {
            if *self.fail_write.lock().unwrap() {
                reply.send(Err(FeedError::NotSupported));
            } else {
                self.documents.lock().unwrap().remove(name);
                reply.send(Ok(()));
            }
        }
    }

    #[test]
    fn named_templates_survive_restart_and_failed_or_unknown_documents_are_not_lost() {
        let ctx = egui::Context::default();
        let host = Arc::new(Host::default());
        let mut store = TemplateStore::<Vec<u32>>::new(StoreKind::ChartTemplates);
        store.set_storage(host.clone());
        assert!(
            store.save("Example", vec![7]).is_err(),
            "wait for the initial read"
        );
        store.poll(&ctx);
        assert!(store.save("  ", vec![7]).is_err());
        store.save("  Example  ", vec![7]).unwrap();
        assert!(!store.ready(), "one write per document at a time");
        store.reload();
        assert_eq!(
            store.entries()["Example"],
            vec![7],
            "reloading cannot discard an outstanding save"
        );
        store.poll(&ctx);
        let mut restored = TemplateStore::<Vec<u32>>::new(StoreKind::ChartTemplates);
        restored.set_storage(host.clone());
        restored.poll(&ctx);
        restored.poll(&ctx);
        assert_eq!(restored.entries()["Example"], vec![7]);
        *host.fail_write.lock().unwrap() = true;
        restored.save("Example", vec![11]).unwrap();
        restored.poll(&ctx);
        assert!(restored.error.is_some());
        assert_eq!(
            restored.entries()["Example"],
            vec![11],
            "retain edits when save fails"
        );
        restored.delete("Example").unwrap();
        restored.poll(&ctx);
        assert!(
            restored.entries().contains_key("Example"),
            "failed delete retains the row"
        );
        *host.fail_write.lock().unwrap() = false;
        restored.save("Example", vec![11]).unwrap();
        restored.poll(&ctx);
        restored.delete("Example").unwrap();
        restored.poll(&ctx);
        assert!(restored.entries().is_empty());
        assert!(host.documents.lock().unwrap().is_empty());
        for document in [r#"{"version":2,"template":[7]}"#, "invalid json"] {
            host.documents
                .lock()
                .unwrap()
                .insert("Future".into(), document.into());
            restored.reload();
            restored.poll(&ctx);
            restored.poll(&ctx);
            assert!(!restored.ready());
            assert!(restored.error.is_some());
            assert!(restored.save("Future", vec![9]).is_err());
            assert!(restored.delete("Future").is_err());
            assert_eq!(host.documents.lock().unwrap()["Future"], document);
        }
    }
}
