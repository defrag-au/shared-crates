use crate::RankedToken;

/// Rank scored tokens with tie handling.
///
/// Ties share the same rank, and the next rank skips accordingly:
/// e.g. scores `[0.1, 0.2, 0.2, 0.5]` with `lower_is_rarer=true`
/// produce ranks `[1, 2, 2, 4]`.
pub fn rank(scores: Vec<(String, f64)>, lower_is_rarer: bool) -> Vec<RankedToken> {
    if scores.is_empty() {
        return Vec::new();
    }

    // Sort by score (rarest first)
    let mut indexed: Vec<(String, f64)> = scores;
    if lower_is_rarer {
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }

    let mut result = Vec::with_capacity(indexed.len());
    let mut current_rank = 1usize;

    for (i, (id, score)) in indexed.iter().enumerate() {
        if i > 0 {
            let prev_score = indexed[i - 1].1;
            // Exact comparison — tokens with identical trait combinations produce
            // identical f64 scores (same multiplication sequence). No tolerance
            // needed; approximate ties would incorrectly collapse distinct scores.
            if *score != prev_score {
                current_rank = i + 1;
            }
        }

        result.push(RankedToken {
            id: id.clone(),
            score: *score,
            rank: current_rank,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rank_no_ties() {
        let scores = vec![("a".into(), 0.5), ("b".into(), 0.1), ("c".into(), 0.3)];
        let ranked = rank(scores, true);
        assert_eq!(ranked[0].id, "b");
        assert_eq!(ranked[0].rank, 1);
        assert_eq!(ranked[1].id, "c");
        assert_eq!(ranked[1].rank, 2);
        assert_eq!(ranked[2].id, "a");
        assert_eq!(ranked[2].rank, 3);
    }

    #[test]
    fn test_rank_with_ties_skip() {
        // Tied scores should share rank, next rank skips
        let scores = vec![
            ("a".into(), 0.2),
            ("b".into(), 0.2),
            ("c".into(), 0.1),
            ("d".into(), 0.5),
        ];
        let ranked = rank(scores, true);
        assert_eq!(ranked[0].id, "c");
        assert_eq!(ranked[0].rank, 1);
        // a and b tied at 0.2
        assert_eq!(ranked[1].rank, 2);
        assert_eq!(ranked[2].rank, 2);
        // d at 0.5 — rank 4 (not 3)
        assert_eq!(ranked[3].id, "d");
        assert_eq!(ranked[3].rank, 4);
    }

    #[test]
    fn test_rank_higher_is_rarer() {
        let scores = vec![("a".into(), 1.5), ("b".into(), 3.0), ("c".into(), 2.0)];
        let ranked = rank(scores, false); // higher = rarer
        assert_eq!(ranked[0].id, "b");
        assert_eq!(ranked[0].rank, 1);
        assert_eq!(ranked[1].id, "c");
        assert_eq!(ranked[1].rank, 2);
        assert_eq!(ranked[2].id, "a");
        assert_eq!(ranked[2].rank, 3);
    }

    #[test]
    fn test_rank_empty() {
        let ranked = rank(Vec::new(), true);
        assert!(ranked.is_empty());
    }

    #[test]
    fn test_rank_single() {
        let ranked = rank(vec![("only".into(), 0.5)], true);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].rank, 1);
    }
}
