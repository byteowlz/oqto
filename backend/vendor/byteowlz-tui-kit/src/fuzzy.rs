//! A tiny, deterministic subsequence fuzzy matcher.
//!
//! No external dependency; the matcher is a deterministic transform (same input → same
//! output) suitable for ranking command-palette results. It returns the char indices of
//! the matched subsequence so callers can highlight them.

/// Fuzzy-match `query` against `haystack` (case-insensitive, subsequence).
///
/// Returns the char indices of matched characters when `query` is a subsequence of
/// `haystack`, otherwise `None`. An empty query matches everything with no positions.
///
/// Deterministic and allocation-light; the indices are character (not byte) positions.
#[must_use]
pub fn fuzzy_indices(haystack: &str, query: &str) -> Option<Vec<usize>> {
    let query: Vec<char> = query.chars().collect();
    if query.is_empty() {
        return Some(Vec::new());
    }
    let haystack: Vec<char> = haystack.chars().collect();
    let mut positions = Vec::new();
    let mut matched = 0usize;
    for (index, hchar) in haystack.iter().enumerate() {
        if matched >= query.len() {
            break;
        }
        if hchar.eq_ignore_ascii_case(&query[matched]) {
            positions.push(index);
            matched += 1;
        }
    }
    if matched == query.len() {
        Some(positions)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_all() {
        assert_eq!(fuzzy_indices("anything", "").as_deref(), Some(&[][..]));
    }

    #[test]
    fn subsequence_matches_case_insensitively() {
        // "Sort by date": S(0) o(1) r(2) t(3) ' '(4) b(5) y(6) ' '(7) d(8)
        let idx = fuzzy_indices("Sort by date", "sd");
        assert_eq!(idx.as_deref(), Some(&[0usize, 8][..]));
    }

    #[test]
    fn non_subsequence_is_none() {
        assert!(fuzzy_indices("date", "sd").is_none());
    }
}
