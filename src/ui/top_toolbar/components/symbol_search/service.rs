//! Request state shared by the instrument choosers.

use crate::data::{FeedError, inbox::Inbox};
use crate::localization as l10n;
use crate::model::{Instrument, SymbolType};
use crate::services::{StoreKind, SymbolQuery, SymbolSearch};
use crate::ui::dialogs::templates::TemplateStore;
use std::sync::Arc;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Recent {
    name: String,
    description: String,
    exchange: String,
    kind: Option<SymbolType>,
}

impl Recent {
    fn symbol(&self) -> Instrument {
        let mut symbol = Instrument::new(&self.name).with_description(&self.description);
        symbol.exchange = Some(self.exchange.clone());
        symbol.symbol_type = self.kind;
        symbol
    }
}

pub(super) struct Search {
    source: Option<Arc<dyn SymbolSearch>>,
    inbox: Arc<Inbox>,
    query: Option<SymbolQuery>,
    rows: Vec<Instrument>,
    loading: bool,
    more: bool,
    offset: usize,
    error: Option<String>,
    pub(super) exchange: Option<String>,
    pub(super) recents: TemplateStore<Vec<Recent>>,
    opened: Vec<Recent>,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            source: None,
            inbox: Arc::default(),
            query: None,
            rows: Vec::new(),
            loading: false,
            more: false,
            offset: 0,
            error: None,
            exchange: None,
            recents: TemplateStore::new(StoreKind::RecentSymbols),
            opened: Vec::new(),
        }
    }
}

impl Search {
    pub(super) fn set_service(&mut self, source: Option<Arc<dyn SymbolSearch>>) {
        self.source = source;
        self.reset();
    }

    pub(super) fn connected(&self) -> bool {
        self.source.is_some()
    }

    pub(super) fn reset(&mut self) {
        self.query = None;
        self.inbox = Arc::default();
        self.rows.clear();
        self.loading = false;
        self.more = false;
        self.error = None;
        self.offset = 0;
    }

    pub(super) fn record(&mut self, symbol: &Instrument) {
        self.opened.retain(|row| row.name != symbol.symbol);
        self.opened.push(Recent {
            name: symbol.symbol.clone(),
            description: symbol
                .description
                .clone()
                .unwrap_or_else(|| symbol.symbol.clone()),
            exchange: symbol.exchange.clone().unwrap_or_default(),
            kind: symbol.symbol_type,
        });
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.recents.poll(ctx);
        if self.recents.ready() && !self.opened.is_empty() {
            let mut recent = self
                .recents
                .entries()
                .get("recent")
                .cloned()
                .unwrap_or_default();
            for row in self.opened.drain(..) {
                recent.retain(|old| old.name != row.name);
                recent.insert(0, row);
            }
            recent.truncate(20);
            if let Err(error) = self.recents.save("recent", recent) {
                self.error = Some(error);
            }
        }
        let repaint = ctx.clone();
        let viewport = ctx.viewport_id();
        self.inbox
            .set_waker(move || repaint.request_repaint_of(viewport));
        for (_, answer) in self.inbox.drain().answers {
            self.loading = false;
            match *answer
                .downcast::<Result<Vec<Instrument>, FeedError>>()
                .expect("symbol search")
            {
                Ok(rows) => {
                    let limit = self.query.as_ref().expect("search request").limit;
                    self.more = rows.len() >= limit && limit > 0;
                    self.offset += rows.len();
                    for instrument in rows.into_iter().take(limit) {
                        if self.rows.iter().any(|row| row.symbol == instrument.symbol) {
                            continue;
                        }
                        self.rows.push(instrument);
                    }
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    self.more = false;
                }
            }
        }
    }

    pub(super) fn has_recents(&self) -> bool {
        self.recents
            .entries()
            .get("recent")
            .is_some_and(|rows| !rows.is_empty())
    }

    pub(super) fn recent_names(&self) -> Vec<String> {
        self.recents
            .entries()
            .get("recent")
            .into_iter()
            .flatten()
            .map(|row| row.name.clone())
            .collect()
    }

    pub(super) fn query(
        &mut self,
        ctx: &egui::Context,
        text: &str,
        kind: Option<SymbolType>,
        limit: usize,
    ) {
        if self.source.is_none() {
            return;
        }
        let query = SymbolQuery {
            text: text.into(),
            symbol_type: kind,
            exchange: self.exchange.clone(),
            offset: 0,
            limit,
        };
        let recent =
            text.is_empty() && kind.is_none() && self.exchange.is_none() && self.has_recents();
        if self.query.as_ref() != Some(&query) {
            self.reset();
            self.query = Some(query.clone());
            self.more = true;
            if !recent {
                self.next_page();
            }
        }
        if recent {
            self.rows = self.recents.entries()["recent"]
                .iter()
                .map(Recent::symbol)
                .collect();
            self.more = false;
            self.loading = false;
            self.inbox = Arc::default();
        }
        self.poll(ctx);
    }

    pub(super) fn next_page(&mut self) {
        if self.loading || !self.more {
            return;
        }
        if let (Some(source), Some(query)) = (&self.source, &self.query) {
            let mut query = query.clone();
            query.offset = self.offset;
            self.loading = true;
            source.search(query, self.inbox.reply(0));
        }
    }

    pub(super) fn results(&self) -> Vec<Instrument> {
        self.rows.clone()
    }

    pub(super) fn status(&self, ui: &mut egui::Ui) {
        if self.loading {
            ui.weak(l10n::text(ui.ctx(), "Searching…"));
        }
        if let Some(error) = self.error.as_ref().or(self.recents.error.as_ref()) {
            ui.colored_label(ui.visuals().error_fg_color, l10n::text(ui.ctx(), error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Source(Mutex<Vec<(SymbolQuery, Reply<Vec<Instrument>>)>>);

    impl SymbolSearch for Source {
        fn search(&self, query: SymbolQuery, reply: Reply<Vec<Instrument>>) {
            self.0.lock().unwrap().push((query, reply));
        }
    }

    #[test]
    fn queries_discard_stale_answers_and_append_the_next_page() {
        let ctx = egui::Context::default();
        let source = Arc::new(Source::default());
        let mut search = Search::default();
        search.set_service(Some(source.clone()));
        search.exchange = Some("Venue".into());
        search.query(&ctx, "old", None, 1);
        let (_, stale) = source.0.lock().unwrap().pop().unwrap();
        search.query(&ctx, "new", Some(SymbolType::Fund), 1);
        let (query, first) = source.0.lock().unwrap().pop().unwrap();
        assert_eq!(
            (
                query.text.as_str(),
                query.symbol_type,
                query.exchange.as_deref(),
                query.offset
            ),
            ("new", Some(SymbolType::Fund), Some("Venue"), 0)
        );
        stale.send(Ok(vec![Instrument::new("stale")]));
        search.poll(&ctx);
        assert!(search.rows.is_empty());
        first.send(Ok(vec![Instrument::new("first")]));
        search.poll(&ctx);
        search.query(&ctx, "new", Some(SymbolType::Fund), 1);
        assert!(
            source.0.lock().unwrap().is_empty(),
            "same query does not refetch"
        );
        search.next_page();
        let (query, second) = source.0.lock().unwrap().pop().unwrap();
        assert_eq!(query.offset, 1);
        second.send(Ok(vec![Instrument::new("second")]));
        search.poll(&ctx);
        assert_eq!(
            search
                .rows
                .iter()
                .map(|s| s.symbol.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        search.next_page();
        source.0.lock().unwrap().pop().unwrap().1.send(Ok(vec![]));
        search.poll(&ctx);
        search.next_page();
        assert!(
            source.0.lock().unwrap().is_empty(),
            "empty page ends pagination"
        );
    }
    #[derive(Default)]
    struct Vault(Mutex<Option<String>>);
    impl crate::services::Storage for Vault {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self
                .0
                .lock()
                .unwrap()
                .as_ref()
                .map(|_| vec!["recent".into()])
                .unwrap_or_default()));
        }
        fn load(&self, _: StoreKind, _: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().clone()));
        }
        fn save(&self, _: StoreKind, _: &str, document: String, reply: Reply<()>) {
            *self.0.lock().unwrap() = Some(document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            *self.0.lock().unwrap() = None;
            reply.send(Ok(()));
        }
    }

    #[test]
    fn recent_instruments_survive_a_restart_and_keep_newest_first() {
        let ctx = egui::Context::default();
        let vault = Arc::new(Vault::default());
        let mut search = Search::default();
        search.recents.set_storage(vault.clone());
        for symbol in ["A", "B", "A"] {
            search.record(&Instrument::new(symbol).with_description("Host name"));
        }
        for _ in 0..4 {
            search.poll(&ctx);
        }
        let mut restored = Search::default();
        restored.recents.set_storage(vault);
        restored.set_service(Some(Arc::new(Vec::<Instrument>::new())));
        // Query before the stored catalog is loaded; a later poll must replace it with recents.
        restored.query(&ctx, "", None, 20);
        for _ in 0..4 {
            restored.poll(&ctx);
        }
        restored.query(&ctx, "", None, 20);
        assert_eq!(
            restored
                .rows
                .iter()
                .map(|row| row.symbol.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(restored.rows[0].description.as_deref(), Some("Host name"));
        let source = Arc::new(Source::default());
        restored.set_service(Some(source.clone()));
        for (kind, exchange) in [(Some(SymbolType::Stock), None), (None, Some("Venue"))] {
            restored.exchange = exchange.map(str::to_owned);
            restored.query(&ctx, "", kind, 20);
            let (query, _) = source
                .0
                .lock()
                .unwrap()
                .pop()
                .expect("filters query the host catalog even when recents exist");
            assert_eq!(
                (query.symbol_type, query.exchange.as_deref()),
                (kind, exchange)
            );
        }
    }
}
