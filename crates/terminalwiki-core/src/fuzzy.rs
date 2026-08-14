//! Fuzzy matching and typo-tolerant suggestions (spec §13, §23, §73).
//!
//! Two distinct problems live here, because they are genuinely different:
//!
//! * **Subsequence matching** ([`score`]) drives the interactive picker. The
//!   user types a prefix of what they remember — `heap expl` — and expects the
//!   list to narrow on every keystroke. This must be cheap enough to run over
//!   every candidate per keystroke (spec §13: no full-text recomputation).
//! * **Edit distance** ([`similarity`]) drives "did you mean". A typo like
//!   `heep` is *not* a subsequence of `Heap`, so subsequence matching cannot
//!   find it and a different algorithm is required (spec §73).

/// Score of a successful fuzzy match. Higher is better.
pub type Score = i32;

const BONUS_BOUNDARY: Score = 16;
const BONUS_CAMEL: Score = 14;
const BONUS_CONSECUTIVE: Score = 12;
const BONUS_FIRST_CHAR: Score = 24;
const BONUS_EXACT_CASE: Score = 2;
const PENALTY_GAP_START: Score = -6;
const PENALTY_GAP_EXTEND: Score = -2;

/// A successful match: its score and the byte offsets that matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: Score,
    /// Char indices in the haystack that matched, ascending.
    pub positions: Vec<usize>,
}

/// Characters that mark a word boundary for scoring purposes.
fn is_separator(c: char) -> bool {
    matches!(
        c,
        '/' | '\\' | '_' | '-' | '.' | ' ' | ':' | ',' | '(' | ')' | '[' | ']' | '#'
    )
}

/// Scores `needle` against `haystack`, case-insensitively.
///
/// Returns `None` when `needle` is not a subsequence of `haystack`. An empty
/// needle matches everything with score 0, which is what an empty picker query
/// should do.
///
/// The algorithm is a single greedy forward pass with lookahead-free bonuses.
/// It is deliberately not an optimal alignment search: the picker runs this
/// across tens of thousands of candidates per keystroke, and a greedy pass with
/// good boundary bonuses ranks well enough while staying O(n).
pub fn score(needle: &str, haystack: &str) -> Option<Match> {
    if needle.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }

    let hay: Vec<char> = haystack.chars().collect();
    let mut positions = Vec::new();
    let mut total: Score = 0;
    let mut hay_idx = 0usize;
    let mut prev_match_idx: Option<usize> = None;
    let mut in_gap = false;

    for nc in needle.chars() {
        let nc_lower = nc.to_lowercase().next().unwrap_or(nc);
        let mut found = None;

        while hay_idx < hay.len() {
            let hc = hay[hay_idx];
            let hc_lower = hc.to_lowercase().next().unwrap_or(hc);
            if hc_lower == nc_lower {
                found = Some(hay_idx);
                break;
            }
            // Skipping a character opens or extends a gap.
            total += if in_gap {
                PENALTY_GAP_EXTEND
            } else {
                PENALTY_GAP_START
            };
            in_gap = true;
            hay_idx += 1;
        }

        let idx = found?;

        let mut gain: Score = 1;
        if idx == 0 {
            gain += BONUS_FIRST_CHAR;
        } else {
            let prev = hay[idx - 1];
            if is_separator(prev) {
                gain += BONUS_BOUNDARY;
            } else if prev.is_lowercase() && hay[idx].is_uppercase() {
                gain += BONUS_CAMEL;
            }
        }
        if prev_match_idx == Some(idx.wrapping_sub(1)) {
            gain += BONUS_CONSECUTIVE;
        }
        if hay[idx] == nc {
            gain += BONUS_EXACT_CASE;
        }

        total += gain;
        positions.push(idx);
        prev_match_idx = Some(idx);
        in_gap = false;
        hay_idx = idx + 1;
    }

    // Prefer shorter haystacks when scores are otherwise equal: a query that
    // matches both `Heap` and `Heap Exploitation Techniques` should rank the
    // tighter one first.
    total -= (hay.len() as Score) / 16;

    Some(Match {
        score: total,
        positions,
    })
}

/// Scores a needle against several fields, keeping the best result.
///
/// Used to match a query against a page's title, aliases and path at once
/// (spec §13), with a weight per field so a title hit outranks a path hit.
pub fn score_best(needle: &str, fields: &[(&str, Score)]) -> Option<Match> {
    let mut best: Option<Match> = None;
    for (text, weight) in fields {
        if let Some(mut m) = score(needle, text) {
            m.score += weight;
            if best.as_ref().is_none_or(|b| m.score > b.score) {
                best = Some(m);
            }
        }
    }
    best
}

/// Levenshtein edit distance, bounded for early exit.
///
/// Returns `max + 1` as soon as the distance is known to exceed `max`, which
/// keeps suggestion generation linear over a large candidate set.
pub fn levenshtein(a: &str, b: &str, max: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len().min(max + 1);
    }
    if b.is_empty() {
        return a.len().min(max + 1);
    }
    if a.len().abs_diff(b.len()) > max {
        return max + 1;
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];

    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > max {
            return max + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Case-insensitive similarity in `0.0..=1.0`, 1.0 being identical.
pub fn similarity(a: &str, b: &str) -> f32 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    if a == b {
        return 1.0;
    }
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        return 1.0;
    }
    let distance = levenshtein(&a, &b, longest);
    1.0 - (distance as f32 / longest as f32)
}

/// Builds a "did you mean" list for a query the user got slightly wrong (spec §73).
///
/// Candidates qualify by edit-distance similarity *or* by being a
/// case-insensitive substring match, so both `heep` → `Heap` and `heap` →
/// `Heap Exploitation` are offered.
pub fn suggestions<'a, I>(query: &str, candidates: I, limit: usize) -> Vec<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    const MIN_SIMILARITY: f32 = 0.55;
    let lower = query.to_lowercase();

    let mut scored: Vec<(f32, &str)> = candidates
        .into_iter()
        .filter_map(|c| {
            let sim = similarity(query, c);
            let contains = c.to_lowercase().contains(&lower) && !lower.is_empty();
            if sim >= MIN_SIMILARITY {
                Some((sim, c))
            } else if contains {
                // A substring hit is worth offering even at low similarity,
                // but must rank below genuine near-misses.
                Some((MIN_SIMILARITY - 0.01, c))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(b.1))
    });
    scored.dedup_by(|a, b| a.1.eq_ignore_ascii_case(b.1));
    scored.into_iter().take(limit).map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_subsequence_and_rejects_a_non_subsequence() {
        assert!(score("hep", "Heap Exploitation").is_some());
        assert!(score("xyz", "Heap").is_none());
        // Order matters: `pah` is not a subsequence of `Heap`.
        assert!(score("pah", "Heap").is_none());
    }

    #[test]
    fn empty_needle_matches_everything() {
        let m = score("", "anything").unwrap();
        assert_eq!(m.score, 0);
        assert!(m.positions.is_empty());
    }

    #[test]
    fn matching_is_case_insensitive_but_prefers_exact_case() {
        let exact = score("Heap", "Heap").unwrap();
        let inexact = score("heap", "Heap").unwrap();
        assert!(exact.score > inexact.score);
    }

    #[test]
    fn word_boundaries_outrank_mid_word_matches() {
        // `he` at the start of a word beats `he` buried inside one.
        let boundary = score("he", "the heap").unwrap();
        let interior = score("he", "another").unwrap();
        assert!(
            boundary.score > interior.score,
            "{boundary:?} vs {interior:?}"
        );
    }

    #[test]
    fn consecutive_matches_outrank_scattered_ones() {
        let consecutive = score("heap", "heap").unwrap();
        let scattered = score("heap", "h e a p").unwrap();
        assert!(consecutive.score > scattered.score);
    }

    #[test]
    fn shorter_haystacks_win_ties() {
        let short = score("heap", "Heap.md").unwrap();
        let long = score("heap", "Heap Exploitation In Depth Extended.md").unwrap();
        assert!(short.score > long.score);
    }

    #[test]
    fn positions_point_at_the_matched_characters() {
        let m = score("hp", "heap").unwrap();
        assert_eq!(m.positions, vec![0, 3]);
        let chars: Vec<char> = "heap".chars().collect();
        assert_eq!(chars[m.positions[0]], 'h');
        assert_eq!(chars[m.positions[1]], 'p');
    }

    #[test]
    fn path_separators_count_as_boundaries() {
        let m = score("sh", "security/heap.md").unwrap();
        // `s` at 0 and `h` right after the slash: both boundary matches.
        assert_eq!(m.positions, vec![0, 9]);
    }

    #[test]
    fn camel_case_boundaries_are_recognised() {
        // `P` at index 2 follows a lowercase letter, so it earns the camel bonus.
        let camel = score("mp", "MyPage").unwrap();
        assert_eq!(camel.positions, vec![0, 2]);
        let interior = score("mg", "MyPage").unwrap(); // `g` is mid-word
        assert!(camel.score > interior.score, "{camel:?} vs {interior:?}");
    }

    #[test]
    fn matching_is_greedy_leftmost_by_design() {
        // Documents the tradeoff in `score`: the first viable character is
        // taken rather than searching for the best global alignment, so `p`
        // binds to the lowercase `p` in "Heap" and not the later camel `P`.
        // This keeps the picker O(n) per candidate per keystroke (spec §13).
        let m = score("hp", "HeapPointer").unwrap();
        assert_eq!(m.positions, vec![0, 3]);
    }

    #[test]
    fn unicode_needles_do_not_panic_or_misindex() {
        let m = score("größe", "Größe messen").unwrap();
        assert_eq!(m.positions.len(), 5);
        assert!(score("漢", "漢字").is_some());
        assert!(score("字漢", "漢字").is_none());
    }

    #[test]
    fn score_best_prefers_the_higher_weighted_field() {
        // Same text in two fields; the weighted one must win.
        let m = score_best("heap", &[("heap", 0), ("heap", 100)]).unwrap();
        let plain = score("heap", "heap").unwrap();
        assert_eq!(m.score, plain.score + 100);
    }

    #[test]
    fn levenshtein_is_correct_and_bounded() {
        assert_eq!(levenshtein("heep", "heap", 10), 1);
        assert_eq!(levenshtein("kitten", "sitting", 10), 3);
        assert_eq!(levenshtein("", "abc", 10), 3);
        assert_eq!(levenshtein("abc", "abc", 10), 0);
        // Bounded early exit returns max+1, not the true distance.
        assert_eq!(levenshtein("aaaaaaaa", "bbbbbbbb", 2), 3);
    }

    #[test]
    fn did_you_mean_finds_the_spec_example() {
        // `tw heep` must suggest Heap and friends (spec §73).
        let candidates = [
            "Heap",
            "Heap Exploitation",
            "Heap Allocator",
            "Compiler",
            "Analysis",
        ];
        let got = suggestions("heep", candidates, 3);
        assert!(got.contains(&"Heap"), "{got:?}");
        assert_eq!(got[0], "Heap", "closest match must rank first: {got:?}");
    }

    #[test]
    fn did_you_mean_offers_substring_matches_too() {
        let candidates = ["Heap Exploitation", "Compiler"];
        let got = suggestions("heap", candidates, 3);
        assert_eq!(got, vec!["Heap Exploitation"]);
    }

    #[test]
    fn did_you_mean_returns_nothing_for_unrelated_queries() {
        let candidates = ["Heap", "Compiler"];
        assert!(suggestions("zzzzzzzzzz", candidates, 3).is_empty());
    }

    #[test]
    fn suggestions_respect_the_limit() {
        let candidates = ["Heap", "Heaps", "Heapy", "Heape", "Heapa"];
        assert_eq!(suggestions("heap", candidates, 2).len(), 2);
    }

    #[test]
    fn scoring_is_linear_enough_for_per_keystroke_use() {
        // Guards against an accidental quadratic rewrite; not a perf promise.
        let hay = "security/heap/exploitation/techniques/tcache-poisoning.md";
        let start = std::time::Instant::now();
        for _ in 0..20_000 {
            let _ = score("shexp", hay);
        }
        assert!(
            start.elapsed().as_millis() < 2000,
            "fuzzy scoring far too slow"
        );
    }
}
