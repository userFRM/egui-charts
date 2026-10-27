//! Request state shared by the instrument choosers.

use crate::data::{FeedError, inbox::Inbox};
use crate::localization as l10n;
use crate::model::{Instrument, SymbolType};
use crate::services::{ExpressionError, StoreKind, SymbolExpression, SymbolQuery, SymbolSearch};
use crate::ui::dialogs::templates::TemplateStore;
use std::sync::Arc;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Recent {
    name: String,
    description: String,
    exchange: String,
    kind: Option<SymbolType>,
    /// Marks and country, absent from lists saved before they were kept.
    logo: Option<String>,
    exchange_logo: Option<String>,
    country: Option<String>,
    root: Option<String>,
}

impl Recent {
    fn symbol(&self) -> Instrument {
        let mut symbol = Instrument::new(&self.name).with_description(&self.description);
        symbol.exchange = Some(self.exchange.clone());
        symbol.symbol_type = self.kind;
        symbol.logo = self.logo.clone();
        symbol.exchange_logo = self.exchange_logo.clone();
        symbol.country = self.country.clone();
        symbol.root = self.root.clone();
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
    components: Components,
}

/// The lookups of an expression's symbols: one search each, answered under its position.
#[derive(Default)]
struct Components {
    text: String,
    inbox: Arc<Inbox>,
    /// Per symbol: `None` while unanswered, then the catalog's spelling or its lookup error.
    found: Vec<Option<Result<String, String>>>,
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
            components: Components::default(),
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
        self.components = Components::default();
    }

    pub(super) fn record(&mut self, symbol: &Instrument) {
        let exchange = symbol.exchange.clone().unwrap_or_default();
        self.opened
            .retain(|row| row.name != symbol.symbol || row.exchange != exchange);
        self.opened.push(Recent {
            name: symbol.symbol.clone(),
            description: symbol
                .description
                .clone()
                .unwrap_or_else(|| symbol.symbol.clone()),
            exchange,
            kind: symbol.symbol_type,
            logo: symbol.logo.clone(),
            exchange_logo: symbol.exchange_logo.clone(),
            country: symbol.country.clone(),
            root: symbol.root.clone(),
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
                recent.retain(|old| old.name != row.name || old.exchange != row.exchange);
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
                        if self.rows.iter().any(|row| {
                            row.symbol == instrument.symbol && row.exchange == instrument.exchange
                        }) {
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

    pub(super) fn recent_results(&self) -> Vec<Instrument> {
        self.recents
            .entries()
            .get("recent")
            .into_iter()
            .flatten()
            .map(Recent::symbol)
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
            self.rows = self.recent_results();
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

    /// Looks up each symbol of `expression`, parsed from `text`, through the host service, or in
    /// `fallback` without one, and spells it as the catalog does, ignoring case. `None` while
    /// lookups are outstanding.
    pub(super) fn resolve(
        &mut self,
        ctx: &egui::Context,
        text: &str,
        mut expression: SymbolExpression,
        fallback: &[Instrument],
    ) -> Option<Result<SymbolExpression, String>> {
        let symbols: Vec<String> = expression.symbols().into_iter().map(Into::into).collect();
        if self.components.text != text {
            self.components = Components {
                text: text.into(),
                inbox: Arc::default(),
                found: vec![None; symbols.len()],
            };
            let source: Arc<dyn SymbolSearch> = match &self.source {
                Some(source) => source.clone(),
                None => Arc::new(fallback.to_vec()),
            };
            for (at, symbol) in symbols.iter().enumerate() {
                let query = SymbolQuery {
                    text: symbol.clone(),
                    symbol_type: None,
                    exchange: None,
                    offset: 0,
                    limit: 20,
                };
                source.search(query, self.components.inbox.reply(at as u64));
            }
        }
        let repaint = ctx.clone();
        let viewport = ctx.viewport_id();
        self.components
            .inbox
            .set_waker(move || repaint.request_repaint_of(viewport));
        for (at, answer) in self.components.inbox.drain().answers {
            let at = at as usize;
            let rows = *answer
                .downcast::<Result<Vec<Instrument>, FeedError>>()
                .expect("symbol search");
            self.components.found[at] =
                Some(rows.map_err(|error| error.to_string()).and_then(|rows| {
                    rows.into_iter()
                        .map(|row| row.symbol)
                        .find(|name| name.to_lowercase() == symbols[at].to_lowercase())
                        .ok_or_else(|| {
                            ExpressionError::UnknownSymbol(symbols[at].clone()).to_string()
                        })
                }));
        }
        let found = match self
            .components
            .found
            .iter()
            .cloned()
            .collect::<Option<Result<Vec<_>, _>>>()?
        {
            Ok(found) => found,
            Err(error) => return Some(Err(error)),
        };
        expression.rename_symbols(|symbol| {
            let at = symbols.iter().position(|name| name == symbol);
            found[at.expect("symbol of the expression")].clone()
        });
        Some(Ok(expression))
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
    #[test]
    fn spread_lookup_reports_service_errors_without_claiming_the_symbol_is_unknown() {
        let ctx = egui::Context::default();
        let source = Arc::new(Source::default());
        let mut search = Search::default();
        search.set_service(Some(source.clone()));
        let expression = SymbolExpression::parse("A-B").unwrap();
        assert!(
            search
                .resolve(&ctx, "A-B", expression.clone(), &[])
                .is_none()
        );
        let requests = std::mem::take(&mut *source.0.lock().unwrap());
        let failure = FeedError::Network("offline".into());
        let expected = failure.to_string();
        for (query, reply) in requests {
            if query.text == "A" {
                reply.send(Err(failure.clone()));
            } else {
                reply.send(Ok(vec![Instrument::new("B")]));
            }
        }
        let result = search.resolve(&ctx, "A-B", expression, &[]).unwrap();
        assert_eq!(result.unwrap_err().to_string(), expected);
    }

    #[test]
    fn listings_keep_their_exchange_in_results_and_recents() {
        let ctx = egui::Context::default();
        let listings = vec![
            Instrument::new("ABC").with_exchange("X").with_root("ABC"),
            Instrument::new("ABC").with_exchange("Y").with_root("ABC"),
        ];
        let mut search = Search::default();
        search.set_service(Some(Arc::new(listings.clone())));
        search.query(&ctx, "ABC", None, 20);
        assert_eq!(search.results(), listings);
        for listing in [&listings[0], &listings[1], &listings[0]] {
            search.record(listing);
        }
        search.poll(&ctx);
        search.query(&ctx, "", None, 20);
        for (actual, expected) in search.results().iter().zip(&listings) {
            assert_eq!(actual.exchange, expected.exchange);
            assert_eq!(actual.root, expected.root);
        }
        assert_eq!(search.results().len(), 2);
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
            let symbol = Instrument::new(symbol).with_description("Host name");
            search.record(&symbol.with_logo("bytes://logo").with_country("US"));
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
        let row = &restored.rows[0];
        assert_eq!(
            (
                row.description.as_deref(),
                row.logo.as_deref(),
                row.country.as_deref()
            ),
            (Some("Host name"), Some("bytes://logo"), Some("US"))
        );
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
