//! Fuzzy search implementation for command palette

/// Result of a fuzzy match
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    /// Match score (higher is better)
    pub score: i32,
    /// Indices in the target string that matched
    pub matched_indices: Vec<usize>,
}

/// Perform fuzzy matching of a query against a target string
///
/// Returns `Some(FuzzyMatch)` if the query matches, `None` otherwise.
/// The match is case-insensitive and supports non-contiguous matching.
pub fn fuzzy_match(query: &str, target: &str) -> Option<FuzzyMatch> {
    if query.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            matched_indices: Vec::new(),
        });
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    let mut score = 0i32;
    let mut matched_indices = Vec::new();
    let mut query_idx = 0;
    let mut prev_matched = false;
    let mut prev_was_separator = true; // Start of string counts as separator

    for (target_idx, &target_char) in target_lower.iter().enumerate() {
        let is_separator = target_char == ' ' || target_char == '_' || target_char == '-';

        if query_idx < query_lower.len() && target_char == query_lower[query_idx] {
            matched_indices.push(target_idx);
            query_idx += 1;

            // Scoring bonuses
            if prev_matched {
                // Consecutive match bonus
                score += 15;
            }
            if prev_was_separator {
                // Word boundary bonus (match at start of word)
                score += 20;
            }
            if target_idx == 0 {
                // First character bonus
                score += 25;
            }

            // Base score for a match
            score += 10;
            prev_matched = true;
        } else {
            prev_matched = false;
        }

        prev_was_separator = is_separator;
    }

    // Did we match all query characters?
    if query_idx == query_lower.len() {
        // Penalty for unmatched characters
        let unmatched_count = target_lower.len() - matched_indices.len();
        score -= unmatched_count as i32;

        // Bonus for exact match
        if matched_indices.len() == target_lower.len() {
            score += 50;
        }

        Some(FuzzyMatch {
            score,
            matched_indices,
        })
    } else {
        None
    }
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
    matches.sort_by(|a, b| b.1.score.cmp(&a.1.score));

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query() {
        let result = fuzzy_match("", "hello");
        assert!(result.is_some());
        assert_eq!(result.unwrap().score, 0);
    }

    #[test]
    fn test_exact_match() {
        let result = fuzzy_match("hello", "hello");
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(m.score > 0);
        assert_eq!(m.matched_indices.len(), 5);
    }

    #[test]
    fn test_fuzzy_match() {
        let result = fuzzy_match("nch", "New Chart");
        assert!(result.is_some());
    }

    #[test]
    fn test_no_match() {
        let result = fuzzy_match("xyz", "hello");
        assert!(result.is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let result = fuzzy_match("HELLO", "hello");
        assert!(result.is_some());
    }

    #[test]
    fn test_word_boundary_bonus() {
        // "nc" should score higher on "New Chart" than "branch"
        // because N matches at word boundary
        let score_nc = fuzzy_match("nc", "New Chart").unwrap().score;
        let score_branch = fuzzy_match("nc", "branch").unwrap().score;
        assert!(score_nc > score_branch);
    }
}
