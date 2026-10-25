//! Versioned, length-framed binary recordings. Hosts supply files or any `Read`/`Write` stream.

use super::{MarketEvent, OrderEvent, ReplayEvent, RestingOrder};
use crate::model::{Level1Quote, OrderBook, OrderBookLevel, Tick, TradeConditions, TradeSide};
use chrono::{DateTime, Utc};
use std::io::{self, Cursor, Read, Write};

const MAGIC: &[u8; 8] = b"EGCFLOW1";
const MAX_RECORD: usize = 16 * 1024 * 1024;

/// Writes a compact binary recording: a version header, then length-framed events with fixed
/// little-endian numbers, optional-id flags and UTF-8 symbols. No runtime or filesystem policy
/// is imposed; a host can use a buffered file on native targets or bytes in a browser.
pub struct RecordingWriter<W: Write> {
    writer: W,
}

impl<W: Write> RecordingWriter<W> {
    /// Writes the version header to a new recording.
    pub fn new(mut writer: W) -> io::Result<Self> {
        writer.write_all(MAGIC)?;
        Ok(Self { writer })
    }

    /// Appends an event without flushing the underlying stream. Invalid data writes nothing.
    pub fn write(&mut self, event: &ReplayEvent) -> io::Result<()> {
        if !super::replay::valid(event) || event.symbol.len() > u16::MAX as usize {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(event.symbol.len() as u16).to_le_bytes());
        bytes.extend_from_slice(event.symbol.as_bytes());
        put_time(&mut bytes, event.time);
        put_u64(&mut bytes, event.sequence);
        match &event.event {
            MarketEvent::Trade(t) => {
                bytes.push(0);
                put_tick(&mut bytes, t);
            }
            MarketEvent::Quote(q) => {
                bytes.push(1);
                for value in [q.bid, q.ask, q.bid_size, q.ask_size] {
                    put_f64(&mut bytes, value);
                }
            }
            MarketEvent::DepthSnapshot { sequence, book }
            | MarketEvent::DepthDelta { sequence, book } => {
                bytes.push(
                    if matches!(event.event, MarketEvent::DepthSnapshot { .. }) {
                        2
                    } else {
                        3
                    },
                );
                put_u64(&mut bytes, *sequence);
                for side in [&book.bids, &book.asks] {
                    bytes.extend_from_slice(&(side.len() as u32).to_le_bytes());
                    for level in side {
                        put_f64(&mut bytes, level.price);
                        put_f64(&mut bytes, level.size);
                        bytes.push(u8::from(level.order_cnt.is_some()));
                        if let Some(n) = level.order_cnt {
                            bytes.extend_from_slice(&n.to_le_bytes());
                        }
                    }
                }
            }
            MarketEvent::OrderSnapshot { sequence, orders } => {
                bytes.push(4);
                put_u64(&mut bytes, *sequence);
                bytes.extend_from_slice(&(orders.len() as u32).to_le_bytes());
                for order in orders {
                    put_order(&mut bytes, order);
                }
            }
            MarketEvent::Order { sequence, event } => {
                bytes.push(5);
                put_u64(&mut bytes, *sequence);
                match event {
                    OrderEvent::Add(order) => {
                        bytes.push(0);
                        put_order(&mut bytes, order);
                    }
                    OrderEvent::Modify {
                        id,
                        price,
                        size,
                        keep_priority,
                    } => {
                        bytes.push(1);
                        put_u64(&mut bytes, *id);
                        for value in [price, size] {
                            bytes.push(u8::from(value.is_some()));
                            if let Some(value) = value {
                                put_f64(&mut bytes, *value);
                            }
                        }
                        bytes.push(u8::from(*keep_priority));
                    }
                    OrderEvent::Cancel { id } => {
                        bytes.push(2);
                        put_u64(&mut bytes, *id);
                    }
                    OrderEvent::Execute { id, size, trade } => {
                        bytes.push(3);
                        put_u64(&mut bytes, *id);
                        put_f64(&mut bytes, *size);
                        bytes.push(u8::from(trade.is_some()));
                        if let Some(trade) = trade {
                            put_tick(&mut bytes, trade);
                        }
                    }
                }
            }
        }
        if bytes.len() > MAX_RECORD {
            return Err(invalid());
        }
        self.writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        self.writer.write_all(&bytes)
    }

    /// Flushes and returns the underlying writer. Propagates disk and stream errors.
    pub fn finish(mut self) -> io::Result<W> {
        self.writer.flush()?;
        Ok(self.writer)
    }
}

/// Reads one binary event at a time. Limits allocation to 16 MiB per record, rejects unknown
/// versions, malformed values and truncated records, and distinguishes clean EOF from truncation.
pub struct RecordingReader<R: Read> {
    reader: R,
}
impl<R: Read> RecordingReader<R> {
    /// Reads and validates the recording header.
    pub fn new(mut reader: R) -> io::Result<Self> {
        if &read::<8>(&mut reader)? != MAGIC {
            return Err(invalid());
        }
        Ok(Self { reader })
    }

    /// Reads the next event, or `None` only at a complete record boundary.
    pub fn read(&mut self) -> io::Result<Option<ReplayEvent>> {
        let mut length = [0; 4];
        loop {
            match self.reader.read(&mut length[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        self.reader.read_exact(&mut length[1..])?;
        let length = u32::from_le_bytes(length) as usize;
        if length > MAX_RECORD {
            return Err(invalid());
        }
        let mut bytes = vec![0; length];
        self.reader.read_exact(&mut bytes)?;
        let mut reader = Cursor::new(bytes.as_slice());
        let symbol_len = u16::from_le_bytes(read(&mut reader)?) as usize;
        let mut symbol = vec![0; symbol_len];
        reader.read_exact(&mut symbol)?;
        let symbol = String::from_utf8(symbol).map_err(|_| invalid())?;
        let time = get_time(&mut reader)?;
        let sequence = get_u64(&mut reader)?;
        let event = match get_byte(&mut reader)? {
            0 => MarketEvent::Trade(get_tick(&mut reader)?),
            1 => MarketEvent::Quote(Level1Quote {
                ts: time,
                symbol: symbol.clone(),
                bid: get_f64(&mut reader)?,
                ask: get_f64(&mut reader)?,
                bid_size: get_f64(&mut reader)?,
                ask_size: get_f64(&mut reader)?,
            }),
            kind @ (2 | 3) => {
                let sequence = get_u64(&mut reader)?;
                let book = OrderBook {
                    ts: time,
                    symbol: symbol.clone(),
                    bids: get_levels(&mut reader)?,
                    asks: get_levels(&mut reader)?,
                };
                if kind == 2 {
                    MarketEvent::DepthSnapshot { sequence, book }
                } else {
                    MarketEvent::DepthDelta { sequence, book }
                }
            }
            4 => {
                let sequence = get_u64(&mut reader)?;
                let count = u32::from_le_bytes(read(&mut reader)?) as usize;
                if count > super::depth::MAX_LEVELS * 2 {
                    return Err(invalid());
                }
                let orders = (0..count)
                    .map(|_| get_order(&mut reader))
                    .collect::<io::Result<Vec<_>>>()?;
                MarketEvent::OrderSnapshot { sequence, orders }
            }
            5 => {
                let sequence = get_u64(&mut reader)?;
                let event = match get_byte(&mut reader)? {
                    0 => OrderEvent::Add(get_order(&mut reader)?),
                    1 => OrderEvent::Modify {
                        id: get_u64(&mut reader)?,
                        price: get_optional(&mut reader, get_f64)?,
                        size: get_optional(&mut reader, get_f64)?,
                        keep_priority: get_bool(&mut reader)?,
                    },
                    2 => OrderEvent::Cancel {
                        id: get_u64(&mut reader)?,
                    },
                    3 => OrderEvent::Execute {
                        id: get_u64(&mut reader)?,
                        size: get_f64(&mut reader)?,
                        trade: get_optional(&mut reader, get_tick)?,
                    },
                    _ => return Err(invalid()),
                };
                MarketEvent::Order { sequence, event }
            }
            _ => return Err(invalid()),
        };
        let event = ReplayEvent::new(symbol, time, sequence, event);
        if reader.position() as usize != bytes.len() || !super::replay::valid(&event) {
            return Err(invalid());
        }
        Ok(Some(event))
    }
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid market recording")
}
fn read<const N: usize>(reader: &mut impl Read) -> io::Result<[u8; N]> {
    let mut b = [0; N];
    reader.read_exact(&mut b)?;
    Ok(b)
}
fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn put_f64(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn get_u64(reader: &mut impl Read) -> io::Result<u64> {
    Ok(u64::from_le_bytes(read(reader)?))
}
fn get_f64(reader: &mut impl Read) -> io::Result<f64> {
    Ok(f64::from_le_bytes(read(reader)?))
}
fn get_byte(reader: &mut impl Read) -> io::Result<u8> {
    Ok(read::<1>(reader)?[0])
}
fn get_bool(reader: &mut impl Read) -> io::Result<bool> {
    match get_byte(reader)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid()),
    }
}
fn get_optional<R: Read, T>(
    reader: &mut R,
    get: impl FnOnce(&mut R) -> io::Result<T>,
) -> io::Result<Option<T>> {
    if get_bool(reader)? {
        get(reader).map(Some)
    } else {
        Ok(None)
    }
}
fn put_time(bytes: &mut Vec<u8>, time: DateTime<Utc>) {
    bytes.extend_from_slice(&time.timestamp().to_le_bytes());
    bytes.extend_from_slice(&time.timestamp_subsec_nanos().to_le_bytes());
}
fn get_time(reader: &mut impl Read) -> io::Result<DateTime<Utc>> {
    DateTime::from_timestamp(
        i64::from_le_bytes(read(reader)?),
        u32::from_le_bytes(read(reader)?),
    )
    .ok_or_else(invalid)
}
fn put_tick(bytes: &mut Vec<u8>, trade: &Tick) {
    put_time(bytes, trade.ts);
    put_f64(bytes, trade.price);
    put_f64(bytes, trade.size);
    bytes.push(match trade.side {
        None => 0,
        Some(TradeSide::Buy) => 1,
        Some(TradeSide::Sell) => 2,
    });
    for id in [trade.id, trade.aggressor_order_id, trade.resting_order_id] {
        bytes.push(u8::from(id.is_some()));
        if let Some(id) = id {
            put_u64(bytes, id);
        }
    }
    bytes.extend_from_slice(&trade.conditions.bits().to_le_bytes());
}
fn get_tick(reader: &mut impl Read) -> io::Result<Tick> {
    let time = get_time(reader)?;
    let price = get_f64(reader)?;
    let size = get_f64(reader)?;
    let side = match get_byte(reader)? {
        0 => None,
        1 => Some(TradeSide::Buy),
        2 => Some(TradeSide::Sell),
        _ => return Err(invalid()),
    };
    let mut trade = Tick::new(time, price, size, side);
    trade.id = get_optional(reader, get_u64)?;
    trade.aggressor_order_id = get_optional(reader, get_u64)?;
    trade.resting_order_id = get_optional(reader, get_u64)?;
    trade.conditions = TradeConditions::from_bits(u16::from_le_bytes(read(reader)?));
    Ok(trade)
}
fn put_order(bytes: &mut Vec<u8>, order: &RestingOrder) {
    put_u64(bytes, order.id);
    bytes.push(u8::from(order.side == TradeSide::Sell));
    put_f64(bytes, order.price);
    put_f64(bytes, order.size);
}
fn get_order(reader: &mut impl Read) -> io::Result<RestingOrder> {
    Ok(RestingOrder::new(
        get_u64(reader)?,
        if get_bool(reader)? {
            TradeSide::Sell
        } else {
            TradeSide::Buy
        },
        get_f64(reader)?,
        get_f64(reader)?,
    ))
}
fn get_levels(reader: &mut impl Read) -> io::Result<Vec<OrderBookLevel>> {
    let count = u32::from_le_bytes(read(reader)?) as usize;
    if count > super::depth::MAX_LEVELS {
        return Err(invalid());
    }
    (0..count)
        .map(|_| {
            Ok(OrderBookLevel {
                price: get_f64(reader)?,
                size: get_f64(reader)?,
                order_cnt: if get_bool(reader)? {
                    Some(u32::from_le_bytes(read(reader)?))
                } else {
                    None
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_recording_round_trips_market_data_and_rejects_truncation() {
        let time = DateTime::from_timestamp(1_700_000_000, 123).unwrap();
        let trade = Tick::new(time, 101.25, 3.0, Some(TradeSide::Buy))
            .with_id(7)
            .with_aggressor_order_id(8)
            .with_resting_order_id(9)
            .with_conditions(TradeConditions::AUCTION);
        let book = OrderBook {
            ts: time,
            symbol: "A".into(),
            bids: vec![OrderBookLevel {
                price: 100.0,
                size: 2.0,
                order_cnt: Some(1),
            }],
            asks: vec![],
        };
        let order = RestingOrder::new(9, TradeSide::Sell, 101.25, 5.0);
        let events = vec![
            MarketEvent::Trade(trade.clone()),
            MarketEvent::Quote(Level1Quote {
                ts: time,
                symbol: "A".into(),
                bid: 100.0,
                ask: 101.25,
                bid_size: 2.0,
                ask_size: 5.0,
            }),
            MarketEvent::DepthSnapshot {
                sequence: 1,
                book: book.clone(),
            },
            MarketEvent::DepthDelta { sequence: 2, book },
            MarketEvent::OrderSnapshot {
                sequence: 3,
                orders: vec![order.clone()],
            },
            MarketEvent::Order {
                sequence: 4,
                event: OrderEvent::Add(order),
            },
            MarketEvent::Order {
                sequence: 5,
                event: OrderEvent::Modify {
                    id: 9,
                    price: None,
                    size: Some(6.0),
                    keep_priority: false,
                },
            },
            MarketEvent::Order {
                sequence: 6,
                event: OrderEvent::Cancel { id: 9 },
            },
            MarketEvent::Order {
                sequence: 7,
                event: OrderEvent::Execute {
                    id: 9,
                    size: 3.0,
                    trade: Some(trade),
                },
            },
        ];
        for payload in events {
            let event = ReplayEvent::new("A", time, 10, payload);
            let mut writer = RecordingWriter::new(Vec::new()).unwrap();
            writer.write(&event).unwrap();
            let bytes = writer.finish().unwrap();
            let mut reader = RecordingReader::new(bytes.as_slice()).unwrap();
            let decoded = reader.read().unwrap().unwrap();
            assert_eq!(decoded, event);
            let mut again = RecordingWriter::new(Vec::new()).unwrap();
            again.write(&decoded).unwrap();
            assert_eq!(again.finish().unwrap(), bytes);
            assert!(reader.read().unwrap().is_none());
            for end in 9..bytes.len() {
                assert!(
                    RecordingReader::new(&bytes[..end]).unwrap().read().is_err(),
                    "truncated at {end}"
                );
            }
        }
        assert!(RecordingReader::new(b"EGCFLOW2".as_slice()).is_err());
        let mut oversized = MAGIC.to_vec();
        oversized.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            RecordingReader::new(oversized.as_slice())
                .unwrap()
                .read()
                .is_err()
        );
    }
}
