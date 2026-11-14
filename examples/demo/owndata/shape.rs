//! Where the rows of a history response are, and which layout a response most likely has.

use serde_json::Value;

use super::profile::{BarFields, FieldPath, Layout, Rows, Segment, TimeSpec, at, text};
use super::time;

/// The rows of `response`, one JSON value per bar, in the order the response lists them.
/// Whatever the layout, a row is a value the paths of [`BarFields`] step into:
///
/// - `Objects`, and `Arrays` without a header: the elements of the list at `rows.path`, as
///   they are.
/// - `Arrays` with a header: each element paired with the header names into an object. A short
///   row leaves its last fields absent; values beyond the header are dropped.
/// - `Columns`: row `i` is an object that holds element `i` of every list in the object at
///   `rows.path`. Members that are not lists are skipped. The longest list gives the number of
///   rows, and a shorter one leaves its field absent in the rows it does not reach.
///
/// A path that leads nowhere or to `null` is a page without rows, because services omit the
/// list when a window holds no data. A value of another type than the layout needs is an error
/// that says what was found, such as `expected a list at data.candles, found an object`.
pub(crate) fn rows(response: &Value, rows: &Rows) -> Result<Vec<Value>, String> {
    let Some(found) = at(response, &rows.path).filter(|found| !found.is_null()) else {
        return Ok(Vec::new());
    };
    let list = |value: &Value, path: &[Segment]| match value {
        Value::Array(list) => Ok(list.clone()),
        other => Err(expected("a list", path, Some(other))),
    };
    match &rows.layout {
        Layout::Objects | Layout::Arrays { header: None } => list(found, &rows.path),
        Layout::Arrays {
            header: Some(header),
        } => {
            let names = match at(response, header) {
                Some(names) => list(names, header)?,
                None => return Err(expected("a list", header, None)),
            };
            let named = |row: Value| {
                let values = match row {
                    Value::Array(values) => values,
                    _ => Vec::new(),
                };
                let fields = names.iter().zip(values);
                Value::Object(
                    fields
                        .filter_map(|(name, value)| Some((text(name)?, value)))
                        .collect(),
                )
            };
            Ok(list(found, &rows.path)?.into_iter().map(named).collect())
        }
        Layout::Columns => {
            let Value::Object(members) = found else {
                return Err(expected("an object", &rows.path, Some(found)));
            };
            let columns = || {
                members
                    .iter()
                    .filter_map(|(name, column)| Some((name, column.as_array()?)))
            };
            let count = columns().map(|(_, column)| column.len()).max().unwrap_or(0);
            let row = |index: usize| {
                let fields = columns()
                    .filter_map(|(name, column)| Some((name.clone(), column.get(index)?.clone())));
                Value::Object(fields.collect())
            };
            Ok((0..count).map(row).collect())
        }
    }
}

/// The error for a value of the wrong type at `path`: what the layout needs and what is there.
fn expected(needed: &str, path: &[Segment], found: Option<&Value>) -> String {
    let steps: Vec<String> = path
        .iter()
        .map(|step| match step {
            Segment::Key(key) => key.clone(),
            Segment::Index(index) => index.to_string(),
        })
        .collect();
    let place = if steps.is_empty() {
        "the root".to_owned()
    } else {
        steps.join(".")
    };
    let found = match found {
        None | Some(Value::Null) => "nothing",
        Some(Value::Bool(_)) => "a boolean",
        Some(Value::Number(_)) => "a number",
        Some(Value::String(_)) => "text",
        Some(Value::Array(_)) => "a list",
        Some(Value::Object(_)) => "an object",
    };
    format!("expected {needed} at {place}, found {found}")
}

/// One way to read a response, which the dialog offers for confirmation.
pub(crate) struct Proposal {
    /// Where the rows are and how they are laid out.
    pub(crate) rows: Rows,
    /// The fields recognised by name. A field that was not recognised has an empty path, for
    /// the user to click.
    pub(crate) bar: BarFields,
    /// How many rows the response holds when it is read this way.
    pub(crate) count: usize,
}

/// The ways `response` can be read, the most likely first. Empty when no list of rows is found
/// within four levels of the root.
pub(crate) fn propose(response: &Value) -> Vec<Proposal> {
    let mut proposals = Vec::new();
    walk(response, response, &mut Vec::new(), &mut proposals);
    let recognised = |proposal: &Proposal| {
        let bar = &proposal.bar;
        let time = match &bar.time {
            TimeSpec::Instant { field, .. } => field,
            TimeSpec::Wall { date, .. } => date,
        };
        let fields = [
            time,
            &bar.open,
            &bar.high,
            &bar.low,
            &bar.close,
            &bar.volume,
        ];
        fields.into_iter().filter(|field| !field.is_empty()).count()
    };
    // The most fields recognised first, then the shorter path, then the most rows.
    proposals.sort_by_key(|proposal| {
        let found = std::cmp::Reverse(recognised(proposal));
        (
            found,
            proposal.rows.path.len(),
            std::cmp::Reverse(proposal.count),
        )
    });
    proposals
}

/// Adds the proposals for `value`, which lies at `path` below `root`, and for the members below
/// it, to a depth of four.
fn walk(root: &Value, value: &Value, path: &mut FieldPath, proposals: &mut Vec<Proposal>) {
    let mut propose = |layout: Layout, bar: BarFields| {
        let rows = Rows {
            path: path.clone(),
            layout,
        };
        let count = self::rows(root, &rows).map_or(0, |rows| rows.len());
        proposals.push(Proposal { rows, bar, count });
    };
    let scalar = |value: &Value| !value.is_array() && !value.is_object();
    match value {
        Value::Array(list) => match list.first() {
            Some(Value::Object(first)) => {
                let named = first.iter().map(|(name, value)| (name.as_str(), value));
                propose(Layout::Objects, fields(named, key));
            }
            Some(Value::Array(first)) if !first.is_empty() && first.iter().all(scalar) => {
                let header = header(root, path, first.len());
                let bar = match &header {
                    Some((_, names)) => fields(names.iter().map(String::as_str).zip(first), key),
                    // Without names, the usual order of five or six columns.
                    None if first.len() >= 5 => {
                        let usual = ["time", "open", "high", "low", "close", "volume"];
                        let column = |_: &str, at: usize| vec![Segment::Index(at)];
                        fields(usual.into_iter().zip(first), column)
                    }
                    None => BarFields::default(),
                };
                let header = header.map(|(path, _)| path);
                propose(Layout::Arrays { header }, bar);
            }
            _ => {}
        },
        Value::Object(members) => {
            let lengths: Vec<usize> = members
                .values()
                .filter_map(|member| Some(member.as_array()?.len()))
                .filter(|length| *length > 0)
                .collect();
            let equal = |length: &usize| lengths.iter().filter(|other| *other == length).count();
            if lengths.iter().any(|length| equal(length) >= 2) {
                let firsts = members.iter().filter_map(|(name, member)| {
                    Some((name.as_str(), member.as_array()?.first()?))
                });
                propose(Layout::Columns, fields(firsts, key));
            }
            if path.len() < 4 {
                for (name, member) in members {
                    path.push(Segment::Key(name.clone()));
                    walk(root, member, path, proposals);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

/// The path of the field called `name`, which is column `at` of its row.
fn key(name: &str, _at: usize) -> FieldPath {
    vec![Segment::Key(name.to_owned())]
}

/// The names of the columns of rows at `path` that are `width` wide: the first list of that
/// many strings among the members of the root and of the object that holds the rows, with its
/// path from the root.
fn header(root: &Value, path: &[Segment], width: usize) -> Option<(FieldPath, Vec<String>)> {
    let top: &[Segment] = &[];
    let holder = &path[..path.len().saturating_sub(1)];
    [top, holder].into_iter().find_map(|below| {
        let members = at(root, below)?.as_object()?;
        members.iter().find_map(|(name, member)| {
            let names = member.as_array()?.iter();
            let names: Vec<String> = names
                .map(|name| Some(name.as_str()?.to_owned()))
                .collect::<Option<_>>()?;
            let step = [Segment::Key(name.clone())];
            (names.len() == width).then(|| ([below, step.as_slice()].concat(), names))
        })
    })
}

/// The fields of a bar among the named values of the first row, recognised by name whatever
/// the letter case. `path` gives the path of a field from its name and its place in the row.
/// The form of the time comes from its value.
fn fields<'a>(
    row: impl Iterator<Item = (&'a str, &'a Value)>,
    path: impl Fn(&str, usize) -> FieldPath,
) -> BarFields {
    let row: Vec<(&str, &Value)> = row.collect();
    // The first name of each list that the row has. The names are generic vocabulary.
    let find = |names: &[&str]| {
        let named = |name: &&str| {
            row.iter()
                .position(|(held, _)| held.eq_ignore_ascii_case(name))
        };
        let at = names.iter().find_map(named)?;
        Some((path(row[at].0, at), row[at].1))
    };
    let field = |names: &[&str]| find(names).map_or_else(FieldPath::new, |(path, _)| path);
    let when = find(&["t", "ts", "time", "timestamp", "datetime", "date", "start"]);
    BarFields {
        time: when.map_or_else(TimeSpec::default, |(path, value)| time::guess(path, value)),
        open: field(&["o", "open"]),
        high: field(&["h", "high"]),
        low: field(&["l", "low"]),
        close: field(&["c", "close"]),
        volume: field(&["v", "vol", "volume"]),
    }
}
