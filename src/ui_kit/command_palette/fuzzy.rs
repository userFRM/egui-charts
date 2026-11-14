//! Fuzzy search implementation for command palette

/// Result of a fuzzy match
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    /// Match score (higher is better)
    pub score: i32,
    /// Indices in the target string that matched
    pub matched_indices: Vec<usize>,
}

/// How much a tier of match outweighs the bonuses within it.
const TIER: i32 = 1_000_000;

fn is_separator(c: char) -> bool {
    c == ' ' || c == '_' || c == '-'
}

/// Score `indices` (ascending positions in `target`) with the per-letter bonuses.
fn score_indices(target: &[char], indices: &[usize]) -> i32 {
    let mut score = 0i32;
    for (k, &i) in indices.iter().enumerate() {
        score += 10;
        if k > 0 && indices[k - 1] + 1 == i {
            score += 15;
        }
        if i == 0 || is_separator(target[i - 1]) {
            score += 20;
        }
        if i == 0 {
            score += 25;
        }
    }
    // Penalty for unmatched characters
    score -= (target.len() - indices.len()) as i32;
    // Bonus for exact match
    if indices.len() == target.len() {
        score += 50;
    }
    score
}

/// Match `query` against `target`, both lowercase. Returns the tier of the match (4 the query is
/// the whole target or one of its words, 3 it starts a word, 2 it occurs inside a word, 1 its
/// letters occur in order) with the match itself, preferring the best occurrence of the tier.
fn locate(query: &[char], target: &[char]) -> Option<(i32, FuzzyMatch)> {
    let n = query.len();
    let mut best: Option<(i32, usize)> = None;
    for start in 0..=target.len().saturating_sub(n) {
        if target.len() < n || target[start..start + n] != *query {
            continue;
        }
        let word_start = start == 0 || is_separator(target[start - 1]);
        let word_end = start + n == target.len() || is_separator(target[start + n]);
        let tier = match (word_start, word_end) {
            (true, true) => 4,
            (true, false) => 3,
            _ => 2,
        };
        if best.is_none_or(|(t, _)| tier > t) {
            best = Some((tier, start));
        }
    }
    let (tier, indices) = match best {
        Some((tier, start)) => (tier, (start..start + n).collect()),
        None => {
            // Letters in order, each at its first occurrence after the previous one.
            let mut indices = Vec::new();
            let mut wanted = query.iter().peekable();
            for (i, c) in target.iter().enumerate() {
                if wanted.next_if(|&w| w == c).is_some() {
                    indices.push(i);
                }
            }
            if wanted.peek().is_some() {
                return None;
            }
            (1, indices)
        }
    };
    Some((
        tier,
        FuzzyMatch {
            score: score_indices(target, &indices),
            matched_indices: indices,
        },
    ))
}

fn lowercase(text: &str) -> Vec<char> {
    text.to_lowercase().chars().collect()
}

/// Perform fuzzy matching of a query against a target string
///
/// Returns `Some(FuzzyMatch)` if the query matches, `None` otherwise.
/// The match is case-insensitive and supports non-contiguous matching. A query that is the target
/// or one of its words scores above one that starts a word, above one inside a word, above one
/// whose letters are only found in order; when the query occurs contiguously, that occurrence is
/// the one matched.
pub fn fuzzy_match(query: &str, target: &str) -> Option<FuzzyMatch> {
    if query.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            matched_indices: Vec::new(),
        });
    }
    let (tier, mut m) = locate(&lowercase(query), &lowercase(target))?;
    m.score += tier * TIER;
    Some(m)
}

/// Rank a command for `query`: its `label` counts above `other` text (description, category,
/// keywords). From the top: the label is or has a word equal to the query, a label word starts
/// with it, the label contains it, `other` contains it contiguously, then letters in order over
/// the label and `other` as one text. The indices point into `label`, and are empty when `other`
/// made the match.
pub(super) fn rank(query: &str, label: &str, other: &str) -> Option<FuzzyMatch> {
    let query = lowercase(query);
    let (label, other) = (lowercase(label), lowercase(other));
    // Levels 5, 4, 3 are a label tier of 4, 3, 2; level 2 is `other` at tier 2 or above.
    let (level, mut m) = match (locate(&query, &label), locate(&query, &other)) {
        (Some((tier, m)), _) if tier >= 2 => (tier + 1, m),
        (_, Some((tier, mut m))) if tier >= 2 => {
            m.matched_indices.clear();
            (2, m)
        }
        _ => {
            // A query may run from the label into the other text, as the row reads.
            let (_, mut m) = locate(&query, &[&label[..], &[' '], &other[..]].concat())?;
            m.matched_indices.retain(|&i| i < label.len());
            (1, m)
        }
    };
    m.score += level * TIER;
    Some(m)
}

/// Search through a list of items and return matches sorted by score
pub fn fuzzy_search<'a, T>(
    query: &str,
    items: &'a [T],
    get_text: impl Fn(&T) -> &str,
) -> Vec<(&'a T, FuzzyMatch)> {
    let mut matches: Vec<_> = items
        .iter()
        .filter_map(|item| fuzzy_match(query, get_text(item)).map(|m| (item, m)))
        .collect();

    // Sort by score (descending)
    matches.sort_by_key(|m| std::cmp::Reverse(m.1.score));

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequences_case_insensitively() {
        for (query, target, indices) in [
            ("", "hello", Some(&[][..])),
            ("hello", "hello", Some(&[0, 1, 2, 3, 4][..])),
            ("nch", "new chart", Some(&[0, 4, 5][..])),
            ("xyz", "hello", None),
            ("HELLO", "hello", Some(&[0, 1, 2, 3, 4][..])),
        ] {
            let result = fuzzy_match(query, target);
            assert_eq!(
                result.as_ref().map(|m| m.matched_indices.as_slice()),
                indices,
                "{query:?} in {target:?}"
            );
            if query.is_empty() {
                assert_eq!(result.unwrap().score, 0);
            }
        }
    }

    #[test]
    fn a_command_the_query_names_ranks_above_scattered_letters() {
        // (query, (label, other text) listed first, (label, other text) listed after it). Each
        // pair is worked by hand from the tiers: label word, label word start, label substring,
        // other text contiguous, letters in order.
        for (query, (hi_label, hi_other), (lo_label, lo_other)) in [
            // "sma" is a word of "Add SMA"; "Add AO" has no label match and the s, m, a of
            // "Studies Momentum Awesome oscillator" are scattered over three words.
            (
                "sma",
                ("Add SMA", "Studies"),
                ("Add AO", "Studies Momentum Awesome oscillator"),
            ),
            // "log" starts the word "logarithmic"; "long position tool" holds l, o, g apart.
            (
                "log",
                ("Toggle logarithmic scale", "Chart"),
                ("Long Position tool", "Drawings"),
            ),
            // "atr" is a word of "Add ATR"; "add tr" holds a, t, r apart.
            ("atr", ("Add ATR", "Studies"), ("Add TR", "Studies")),
            // "cci" is a word of "Add CCI"; "cyclic lines" holds c, c, i apart.
            (
                "cci",
                ("Add CCI", "Studies"),
                ("Curve tool", "Drawings cyclic lines"),
            ),
            // "grid" is a word of "Toggle grid"; "gann retrace indicator" holds g, r, i, d apart.
            (
                "grid",
                ("Toggle grid", "Chart"),
                ("Gann Box tool", "Gann retrace indicator"),
            ),
            // "settings" is a word of "Chart settings"; the other text holds s, e, t, t, i, n,
            // g, s over several words.
            (
                "settings",
                ("Chart settings", "Chart"),
                ("Toggle grid", "Chart set tight inner grid spacing"),
            ),
            // Letters that begin three words are still only letters in order.
            ("sma", ("Add SMA", "Studies"), ("Show Many Assets", "Chart")),
            // The label tiers, then other text, then letters in order, for one query.
            ("log", ("Log scale", ""), ("Logarithmic scale", "")),
            ("log", ("Logarithmic scale", ""), ("Backlog view", "")),
            (
                "log",
                ("Backlog view", ""),
                ("Chart", "Show logarithmic prices"),
            ),
            (
                "log",
                ("Chart", "Show logarithmic prices"),
                ("Long Position tool", ""),
            ),
        ] {
            let rank = |label, other| {
                rank(query, label, other)
                    .unwrap_or_else(|| panic!("{query:?} in {label:?}"))
                    .score
            };
            assert!(
                rank(hi_label, hi_other) > rank(lo_label, lo_other),
                "{query:?}: {hi_label:?} before {lo_label:?}"
            );
        }
    }

    #[test]
    fn highlight_indices_follow_the_label_only() {
        // "log" inside "Toggle logarithmic scale" sits at 7..10, not at the scattered first
        // l, o, g of the text.
        let m = rank("log", "Toggle logarithmic scale", "Chart").unwrap();
        assert_eq!(m.matched_indices, [7, 8, 9]);
        let m = rank("log", "Chart", "Show logarithmic prices").unwrap();
        assert!(m.matched_indices.is_empty());
    }

    #[test]
    fn letters_in_order_may_run_from_the_label_into_the_other_text() {
        // The space after "grid" is the one before the category; the indices stop at the label.
        let m = rank("grid ", "Toggle grid", "Chart").unwrap();
        assert_eq!(m.matched_indices, [7, 8, 9, 10]);
    }
}
