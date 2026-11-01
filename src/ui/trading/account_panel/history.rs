//! Paging through the broker's older history: what the account panel asks
//! [`Broker::history`](crate::services::Broker::history) for, and how a page joins the
//! histories the stream fills.

use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};

use super::{AccountPanel, HISTORY_LIMIT};
use crate::model::{Order, OrderId, OrderStatus};
use crate::services::{AccountEvent, HistoryKind, HistoryRequest};

/// Entries asked for in one page of older history.
const HISTORY_PAGE: u32 = 100;

/// How far the account panel has paged through one kind of the broker's older history since the
/// stream started.
#[derive(Debug, Default)]
pub(super) enum Paging {
    /// No page asked for yet: the first request ends just after the oldest entry listed.
    #[default]
    Idle,
    /// The request under this key waits for its answer.
    Loading(u64, HistoryRequest),
    /// The request for the next page of the range.
    Next(HistoryRequest),
    /// The broker keeps nothing older or keeps no history, or the request failed for another
    /// reason than the connection.
    Ended,
    /// The request failed for want of a connection; [`resume`](AccountPanel::resume) asks it
    /// again once the broker is live.
    Deferred(HistoryRequest),
}

impl AccountPanel {
    /// Records the request for the page of `kind` before the oldest entry listed under `key` and
    /// returns it; nothing while a page of `kind` is on its way or waits for the broker to be
    /// live, after its history ended, or once it lists [`HISTORY_LIMIT`] entries.
    pub(in crate::ui::trading) fn ask(
        &mut self,
        kind: HistoryKind,
        key: u64,
    ) -> Option<HistoryRequest> {
        if self.held(kind) >= HISTORY_LIMIT {
            return None;
        }
        let oldest = self.oldest(kind);
        let paging = self.paging.entry(kind).or_default();
        let request = match paging {
            Paging::Idle => {
                let mut request = HistoryRequest::new(kind, HISTORY_PAGE);
                // A second past the oldest entry, so that entries sharing its time are not
                // skipped at the precision a broker keeps; those listed come back as repeats.
                request.to = oldest.and_then(|time| time.checked_add_signed(TimeDelta::seconds(1)));
                request
            }
            Paging::Next(request) => request.clone(),
            Paging::Loading(..) | Paging::Deferred(_) | Paging::Ended => return None,
        };
        *paging = Paging::Loading(key, request.clone());
        Some(request)
    }

    /// The kind of history whose page is on its way under `key`.
    pub(in crate::ui::trading) fn loading(&self, key: u64) -> Option<HistoryKind> {
        self.paging.iter().find_map(|(kind, paging)| {
            matches!(paging, Paging::Loading(at, _) if *at == key).then_some(*kind)
        })
    }

    /// Lists the valid `events` of the page answered for `kind`, and keeps its `next` cursor
    /// for the page before it.
    pub(in crate::ui::trading) fn page(
        &mut self,
        kind: HistoryKind,
        events: Vec<AccountEvent>,
        next: Option<String>,
    ) {
        let Some(Paging::Loading(_, request)) = self.paging.get(&kind) else {
            return;
        };
        let mut request = request.clone();
        let paging = match next {
            Some(cursor) if request.cursor.as_ref() != Some(&cursor) => {
                request.cursor = Some(cursor);
                Paging::Next(request)
            }
            Some(_) => {
                log::warn!(
                    "the broker answered a {kind:?} history page with the cursor it was asked for; paging stopped"
                );
                Paging::Ended
            }
            None => Paging::Ended,
        };
        self.paging.insert(kind, paging);
        self.merge(kind, events);
    }

    /// Stops paging through `kind` until the stream starts again.
    pub(in crate::ui::trading) fn end(&mut self, kind: HistoryKind) {
        self.paging.insert(kind, Paging::Ended);
    }

    /// Keeps the request for the page of `kind` that failed for want of a connection, for
    /// [`resume`](Self::resume) to ask again.
    pub(in crate::ui::trading) fn defer(&mut self, kind: HistoryKind) {
        if let Some(Paging::Loading(_, request)) = self.paging.get(&kind) {
            let request = request.clone();
            self.paging.insert(kind, Paging::Deferred(request));
        }
    }

    /// Asks again, at the next [`ask`](Self::ask), for the pages that failed for want of a
    /// connection or were on their way when the broker stopped being live; a late answer to one
    /// of those is dropped.
    pub(in crate::ui::trading) fn resume(&mut self) {
        for paging in self.paging.values_mut() {
            *paging = match std::mem::take(paging) {
                Paging::Loading(_, request) | Paging::Deferred(request) => Paging::Next(request),
                other => other,
            };
        }
    }

    fn held(&self, kind: HistoryKind) -> usize {
        match kind {
            HistoryKind::Orders => self.history.len(),
            HistoryKind::Executions => self.fills.executions.len(),
            HistoryKind::Balance => self.balance.len(),
            HistoryKind::Journal => self.journal.len(),
        }
    }

    /// The time of the oldest entry listed of `kind`; for orders, when it closed.
    fn oldest(&self, kind: HistoryKind) -> Option<DateTime<Utc>> {
        match kind {
            HistoryKind::Orders => self.history.first().and_then(|order| order.closed),
            HistoryKind::Executions => self.fills.executions.first().map(|fill| fill.time),
            HistoryKind::Balance => self.balance.first().map(|change| change.time),
            HistoryKind::Journal => self.journal.first().map(|entry| entry.time),
        }
    }

    /// Lists the entries of a page of `kind` before the oldest entry: a working order, an order
    /// listed already and other kinds of event are dropped, and other entries repeating one the
    /// stream listed. Only the newest entries that fit under [`HISTORY_LIMIT`] are kept.
    fn merge(&mut self, kind: HistoryKind, events: Vec<AccountEvent>) {
        match kind {
            HistoryKind::Orders => {
                let mut listed: HashSet<OrderId> = self
                    .history
                    .iter()
                    .map(|order| order.id.clone())
                    .chain(self.fills.working.iter().cloned())
                    .collect();
                let room = HISTORY_LIMIT.saturating_sub(self.history.len());
                let mut older: Vec<Order> = events
                    .into_iter()
                    .rev()
                    .filter_map(|event| match event {
                        AccountEvent::Order(order)
                            if order.status != OrderStatus::Working
                                && listed.insert(order.id.clone()) =>
                        {
                            Some(order)
                        }
                        _ => None,
                    })
                    .take(room)
                    .collect();
                older.reverse();
                for order in &older {
                    self.fills.order(order);
                }
                older.append(&mut self.history);
                self.history = older;
            }
            HistoryKind::Executions => {
                let page = events
                    .into_iter()
                    .filter_map(|event| match event {
                        AccountEvent::Execution(fill) => Some(fill),
                        _ => None,
                    })
                    .collect();
                let fills = &mut self.fills;
                let room = HISTORY_LIMIT.saturating_sub(fills.executions.len());
                let mut older = unlisted(page, &fills.executions[fills.paged..], room);
                for fill in &older {
                    *fills.filled.entry(fill.order.clone()).or_default() += fill.quantity;
                    if let Some(symbol) = fills
                        .orders
                        .get(&fill.order)
                        .map(|order| order.symbol.clone())
                    {
                        fills.remember(&symbol, fill);
                    }
                }
                fills.paged += older.len();
                older.append(&mut fills.executions);
                fills.executions = older;
            }
            HistoryKind::Balance => {
                let page = events
                    .into_iter()
                    .filter_map(|event| match event {
                        AccountEvent::Balance(change) => Some(change),
                        _ => None,
                    })
                    .collect();
                let room = HISTORY_LIMIT.saturating_sub(self.balance.len());
                let mut older = unlisted(page, &self.balance[self.balance_paged..], room);
                self.balance_paged += older.len();
                older.append(&mut self.balance);
                self.balance = older;
            }
            HistoryKind::Journal => {
                let page = events
                    .into_iter()
                    .filter_map(|event| match event {
                        AccountEvent::Journal(entry) => Some(entry),
                        _ => None,
                    })
                    .collect();
                let room = HISTORY_LIMIT.saturating_sub(self.journal.len());
                let mut older = unlisted(page, &self.journal[self.journal_paged..], room);
                self.journal_paged += older.len();
                older.append(&mut self.journal);
                self.journal = older;
            }
        }
    }
}

/// The entries of `page`, oldest first, that repeat none of `live`, the entries the stream
/// listed: each entry of `live` stands for one entry of the page, so that two equal fills of a
/// page against one listed keep one. At most `room`, the newest, so that none is missing between
/// them and `live`.
fn unlisted<T: PartialEq>(page: Vec<T>, live: &[T], room: usize) -> Vec<T> {
    let mut matched = vec![false; live.len()];
    let mut older = Vec::new();
    for entry in page.into_iter().rev() {
        if older.len() == room {
            break;
        }
        match (0..live.len()).find(|&at| !matched[at] && live[at] == entry) {
            Some(at) => matched[at] = true,
            None => older.push(entry),
        }
    }
    older.reverse();
    older
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::data::FeedError;
    use crate::model::{
        BalanceChange, Execution, JournalEntry, OrderRequest, OrderSide, OrderType, Position,
    };
    use crate::services::HistoryPage;
    use crate::ui::trading::Trading;
    use crate::ui::trading::account_panel::Tab;
    use crate::ui::trading::tests::Recorder;
    use crate::ui_kit::notifications::Toasts;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_704_067_200 + seconds, 0).unwrap()
    }

    /// Orders are matched by id and other entries one to one by value, so each entry is listed
    /// once whether the page or the stream brings it first, and two equal fills of a page against
    /// one listed stay two. The first request ends a second after the oldest entry listed and the
    /// next follows the cursor. Pages leave working orders, positions, notifications and the
    /// fill sound alone.
    #[test]
    fn older_history_lists_each_entry_once_whichever_arrives_first() {
        let ctx = egui::Context::default();
        let working = Order::new(
            OrderId::new("w"),
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0)),
            OrderStatus::Working,
        );
        let position = Position::new("SYM", 1.0, 100.0);
        let filled = |order: &Order, closed| {
            let mut order = order.clone();
            order.status = OrderStatus::Filled;
            order.filled = 1.0;
            order.closed = Some(at(closed));
            order
        };
        let market = |id| {
            let request = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market);
            Order::new(OrderId::new(id), request, OrderStatus::Working)
        };
        let o1 = filled(&market("1"), 10);
        let o2 = filled(&market("2"), 20);
        let mut o2_page = o2.clone();
        o2_page.fill_price = Some(100.0);
        let working_done = filled(&working, 15);
        let fill = |order, seconds, price| {
            Execution::new(OrderId::new(order), at(seconds), OrderSide::Buy, price, 1.0)
        };
        let (f0, f1, f2) = (
            fill("0", 5, 98.0),
            fill("1", 10, 99.0),
            fill("2", 20, 100.0),
        );
        let b1 = BalanceChange::new(at(10), 1_000.0, -1.0, "Commission for order 1");
        let b2 = BalanceChange::new(at(20), 999.0, -1.0, "Commission for order 2");
        let j1 = JournalEntry::new(at(10), "Order 1 filled");
        let j2 = JournalEntry::new(at(20), "Order 2 filled");
        let stream = [
            AccountEvent::Order(o2.clone()),
            AccountEvent::Execution(f1.clone()),
            AccountEvent::Execution(f2.clone()),
            AccountEvent::Balance(b2.clone()),
            AccountEvent::Journal(j2.clone()),
        ];
        let kinds = [
            HistoryKind::Orders,
            HistoryKind::Executions,
            HistoryKind::Balance,
            HistoryKind::Journal,
        ];
        for page_first in [false, true] {
            let broker = Arc::new(Recorder {
                state: vec![
                    AccountEvent::Order(working.clone()),
                    AccountEvent::Position(position.clone()),
                ],
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.display.play_execution_sound = true;
            trading.poll(&ctx, &mut Toasts::new());
            let replay = |trading: &mut Trading| {
                for event in stream.clone() {
                    trading.apply(&egui::Context::default(), event, &mut Toasts::new());
                }
                trading.take_execution_sound();
            };
            if !page_first {
                replay(&mut trading);
            }
            for kind in kinds {
                trading.load_older(kind);
            }
            let asked = std::mem::take(&mut *broker.histories.lock().unwrap());
            let to = |seconds| (!page_first).then(|| at(seconds));
            let pages = [
                vec![
                    AccountEvent::Order(o1.clone()),
                    AccountEvent::Order(working_done.clone()),
                    AccountEvent::Order(o2_page.clone()),
                ],
                [&f0, &f1, &f1]
                    .into_iter()
                    .chain(page_first.then_some(&f2))
                    .map(|fill| AccountEvent::Execution(fill.clone()))
                    .collect(),
                vec![
                    AccountEvent::Balance(b1.clone()),
                    AccountEvent::Balance(b2.clone()),
                ],
                vec![
                    AccountEvent::Journal(j1.clone()),
                    AccountEvent::Journal(j2.clone()),
                ],
            ];
            let mut first = Vec::new();
            for (((request, reply), page), (kind, seconds)) in asked
                .into_iter()
                .zip(pages)
                .zip(kinds.into_iter().zip([21, 11, 21, 21]))
            {
                let mut expected = HistoryRequest::new(kind, 100);
                expected.to = to(seconds);
                assert_eq!(request, expected, "page first: {page_first}");
                reply.send(Ok(HistoryPage::new(page, Some(format!("{kind:?}")))));
                first.push(request);
            }
            assert_eq!(first.len(), 4);
            let mut toasts = Toasts::new();
            trading.poll(&ctx, &mut toasts);
            assert!(toasts.toasts().is_empty());
            assert_eq!(trading.take_execution_sound(), None);
            assert_eq!(trading.orders, std::slice::from_ref(&working));
            assert_eq!(trading.positions, std::slice::from_ref(&position));
            if page_first {
                replay(&mut trading);
            }

            let panel = &trading.account_panel;
            assert_eq!(
                panel.history,
                [o1.clone(), o2.clone()],
                "page first: {page_first}"
            );
            assert_eq!(
                panel.fills.executions,
                [f0.clone(), f1.clone(), f1.clone(), f2.clone()],
                "page first: {page_first}"
            );
            assert_eq!(panel.fills.filled_quantity(&OrderId::new("1")), 2.0);
            assert_eq!(panel.fills.price("SYM"), Some(100.0));
            assert_eq!(panel.balance, [b1.clone(), b2.clone()]);
            assert_eq!(panel.journal, [j1.clone(), j2.clone()]);

            for kind in kinds {
                trading.load_older(kind);
            }
            let asked = std::mem::take(&mut *broker.histories.lock().unwrap());
            for ((request, reply), mut expected) in asked.into_iter().zip(first) {
                expected.cursor = Some(format!("{:?}", expected.kind));
                assert_eq!(request, expected);
                reply.send(Ok(HistoryPage::default()));
            }
            trading.poll(&ctx, &mut Toasts::new());
            for kind in kinds {
                trading.load_older(kind);
            }
            assert!(broker.histories.lock().unwrap().is_empty());
        }
    }

    /// An answer for the previous account or stream is never listed, and the new stream asks
    /// again from its own oldest entry.
    #[test]
    fn older_history_answers_of_a_previous_stream_are_dropped() {
        let ctx = egui::Context::default();
        for account_change in [true, false] {
            let broker = Arc::new(Recorder::default());
            let next = Arc::new(Recorder::default());
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            trading.load_older(HistoryKind::Journal);
            let (_, reply) = broker.histories.lock().unwrap().pop().unwrap();
            let asked = if account_change {
                trading.set_service(next.clone());
                &next
            } else {
                trading.connect();
                &broker
            };
            let old = JournalEntry::new(at(0), "Old account");
            reply.send(Ok(HistoryPage::new(vec![AccountEvent::Journal(old)], None)));
            trading.poll(&ctx, &mut Toasts::new());
            trading.load_older(HistoryKind::Journal);
            assert!(trading.account_panel.journal.is_empty());
            let requests: Vec<_> = (asked.histories.lock().unwrap().iter())
                .map(|(request, _)| request.clone())
                .collect();
            assert_eq!(
                requests,
                [HistoryRequest::new(HistoryKind::Journal, 100)],
                "account change: {account_change}"
            );
        }
    }

    /// Paging stops on an error, a repeated cursor or a full history, so the broker is not asked
    /// on every frame; only a real failure is shown; and a page that does not fit keeps its
    /// newest entries next to the listed ones.
    #[test]
    fn older_history_paging_ends_at_its_bound_and_on_errors() {
        let ctx = egui::Context::default();
        let line = |seconds| JournalEntry::new(at(seconds), seconds.to_string());
        let page = |lines: Vec<JournalEntry>, next: Option<&str>| {
            let events = lines.into_iter().map(AccountEvent::Journal).collect();
            Ok(HistoryPage::new(events, next.map(String::from)))
        };
        let request = |to, cursor: Option<&str>| {
            let mut request = HistoryRequest::new(HistoryKind::Journal, 100);
            request.to = to;
            request.cursor = cursor.map(String::from);
            request
        };
        for (name, listed, answers, expected, messages) in [
            (
                "unsupported",
                0..0,
                vec![Err(FeedError::NotSupported)],
                vec![request(None, None)],
                &[][..],
            ),
            (
                "failed",
                0..0,
                vec![Err(FeedError::Network("offline".into()))],
                vec![request(None, None)],
                &["Account history unavailable: network error: offline"][..],
            ),
            (
                "same cursor",
                0..0,
                vec![page(vec![], Some("a")), page(vec![], Some("a"))],
                vec![request(None, None), request(None, Some("a"))],
                &[][..],
            ),
            (
                "full",
                100..1_099,
                vec![page(vec![line(1), line(2), line(3)], Some("more"))],
                vec![request(Some(at(101)), None)],
                &[][..],
            ),
        ] {
            let broker = Arc::new(Recorder::default());
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            let mut toasts = Toasts::new();
            for seconds in listed {
                trading.apply(
                    &egui::Context::default(),
                    AccountEvent::Journal(line(seconds)),
                    &mut toasts,
                );
            }
            let mut asked = Vec::new();
            for answer in answers {
                trading.load_older(HistoryKind::Journal);
                let (request, reply) = broker.histories.lock().unwrap().pop().unwrap();
                reply.send(answer);
                trading.poll(&ctx, &mut toasts);
                asked.push(request);
            }
            assert_eq!(asked, expected, "{name}");
            let shown: Vec<_> = (toasts.toasts().iter())
                .map(|toast| toast.message.as_str())
                .collect();
            assert_eq!(shown, messages, "{name}");
            trading.load_older(HistoryKind::Journal);
            assert!(broker.histories.lock().unwrap().is_empty(), "{name}");
            if name == "full" {
                let journal = &trading.account_panel.journal;
                assert_eq!(journal.len(), 1_000);
                assert_eq!((&journal[0], &journal[1]), (&line(3), &line(100)));
            }
        }
    }

    /// Only the four history tables ask, each for its own kind, once shown to their end.
    #[test]
    fn the_end_of_a_history_tab_asks_for_its_kind() {
        for (tab, show_executions, expected) in [
            (Tab::Positions, false, None),
            (Tab::Orders, false, None),
            (Tab::History, false, Some(HistoryKind::Orders)),
            (Tab::History, true, Some(HistoryKind::Executions)),
            (Tab::Balance, false, Some(HistoryKind::Balance)),
            (Tab::Journal, false, Some(HistoryKind::Journal)),
        ] {
            let ctx = egui::Context::default();
            let broker = Arc::new(Recorder::default());
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            trading.account_panel.tab = tab;
            trading.account_panel.show_executions = show_executions;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                trading.show_account_panel(ui, "SYM");
            })
            .drop_without_applying_deltas();
            let kinds: Vec<_> = (broker.histories.lock().unwrap().iter())
                .map(|(request, _)| request.kind)
                .collect();
            assert_eq!(kinds, Vec::from_iter(expected), "{tab:?} {show_executions}");
        }
    }
}
