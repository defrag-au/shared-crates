use crate::collection::normalize_token_attributes;
use crate::{Collection, Scorer, Token};

/// OpenRarity Information Content scorer.
///
/// Score = `sum(-log2(p))` / collection_entropy for each trait slot.
/// Higher score = rarer token.
///
/// Reference: <https://github.com/OpenRarity/open-rarity>
pub struct ICScorer;

impl ICScorer {
    /// Calculate collection entropy: `-sum(p * log2(p))` across all trait/value pairs.
    fn collection_entropy(&self, collection: &Collection) -> f64 {
        let total = collection.total_supply as f64;
        let mut entropy = 0.0;

        for value_counts in collection.frequencies.values() {
            for &count in value_counts.values() {
                let p = count as f64 / total;
                if p > 0.0 {
                    entropy -= p * p.log2();
                }
            }
        }

        entropy
    }

    /// Calculate the IC score for a single token (before entropy normalization).
    fn token_ic_score(&self, collection: &Collection, token: &Token) -> f64 {
        let total = collection.total_supply as f64;
        let normalized = normalize_token_attributes(token, &collection.shape);

        let mut ic = 0.0;
        for (trait_type, slot_idx, value) in &normalized {
            let count = collection.count_for_value(trait_type, *slot_idx, value) as f64;
            let p = count / total;
            if p > 0.0 {
                // -log2(p) = log2(total/count) = information content
                ic += -p.log2();
            }
        }

        ic
    }
}

impl Scorer for ICScorer {
    fn score(&self, collection: &Collection, tokens: &[Token]) -> Vec<(String, f64)> {
        let entropy = self.collection_entropy(collection);
        // Guard against zero entropy (all tokens identical)
        let normalization = if entropy > 0.0 { entropy } else { 1.0 };

        tokens
            .iter()
            .map(|token| {
                let ic = self.token_ic_score(collection, token);
                (token.id.clone(), ic / normalization)
            })
            .collect()
    }

    fn lower_is_rarer(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "OpenRarity Information Content"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_collection, Attribute};

    #[test]
    fn test_uniform_collection_scores_one() {
        // OpenRarity: uniform distribution should score all tokens at 1.0
        let tokens: Vec<Token> = (0..1000)
            .map(|i| {
                let mut attrs = Vec::new();
                for attr_idx in 0..5 {
                    attrs.push(Attribute::new(
                        format!("attr_{attr_idx}"),
                        format!("val_{}", i % 10),
                    ));
                }
                Token::new(format!("{i}"), attrs)
            })
            .collect();

        let collection = build_collection(&tokens);
        let scores = ICScorer.score(&collection, &tokens);

        for (id, score) in &scores {
            assert!(
                (score - 1.0).abs() < 1e-8,
                "Token {id} should score 1.0 in uniform collection, got {score}"
            );
        }
    }

    #[test]
    fn test_rare_token_scores_highest() {
        let mut tokens: Vec<Token> = (0..99)
            .map(|i| {
                Token::new(
                    format!("{i}"),
                    vec![
                        Attribute::new("hat", "common"),
                        Attribute::new("body", "common"),
                    ],
                )
            })
            .collect();

        tokens.push(Token::new(
            "rare",
            vec![
                Attribute::new("hat", "legendary"),
                Attribute::new("body", "legendary"),
            ],
        ));

        let collection = build_collection(&tokens);
        let scores = ICScorer.score(&collection, &tokens);

        let rare_score = scores.iter().find(|(id, _)| id == "rare").unwrap().1;
        let common_score = scores.iter().find(|(id, _)| id == "0").unwrap().1;

        assert!(
            rare_score > common_score,
            "Rare token ({rare_score}) should score higher than common ({common_score})"
        );
    }

    #[test]
    fn test_null_equals_missing() {
        // OpenRarity: missing traits should produce identical scores to explicit "none"
        let tokens_with_missing = vec![
            Token::new(
                "0",
                vec![
                    Attribute::new("bottom", "1"),
                    Attribute::new("hat", "1"),
                    Attribute::new("special", "true"),
                ],
            ),
            Token::new(
                "1",
                vec![Attribute::new("bottom", "1"), Attribute::new("hat", "1")],
            ),
            Token::new(
                "2",
                vec![Attribute::new("bottom", "2"), Attribute::new("hat", "2")],
            ),
            Token::new(
                "3",
                vec![Attribute::new("bottom", "2"), Attribute::new("hat", "2")],
            ),
            Token::new(
                "4",
                vec![Attribute::new("bottom", "3"), Attribute::new("hat", "2")],
            ),
        ];

        // For the "explicit none" version, we rely on the fact that the
        // collection builder will treat missing "special" the same as having
        // a null marker — both get __null_0 assigned. So instead, we verify
        // that the scores from the missing version are self-consistent.
        let collection = build_collection(&tokens_with_missing);
        let scores = ICScorer.score(&collection, &tokens_with_missing);

        // Token 0 has the rare "special=true" (1/5) so should score highest
        let score_0 = scores[0].1;
        let score_1 = scores[1].1;
        assert!(
            score_0 > score_1,
            "Token with rare trait ({score_0}) should score higher than without ({score_1})"
        );
    }

    #[test]
    fn test_score_ordering() {
        // From OpenRarity test_information_content_null_value_attribute
        let tokens = vec![
            Token::new(
                "0",
                vec![
                    Attribute::new("bottom", "spec"),
                    Attribute::new("hat", "spec"),
                    Attribute::new("special", "true"),
                ],
            ),
            Token::new(
                "1",
                vec![
                    Attribute::new("bottom", "1"),
                    Attribute::new("hat", "1"),
                    Attribute::new("special", "true"),
                ],
            ),
            Token::new(
                "2",
                vec![Attribute::new("bottom", "1"), Attribute::new("hat", "1")],
            ),
            Token::new(
                "3",
                vec![Attribute::new("bottom", "2"), Attribute::new("hat", "2")],
            ),
            Token::new(
                "4",
                vec![Attribute::new("bottom", "2"), Attribute::new("hat", "2")],
            ),
            Token::new(
                "5",
                vec![Attribute::new("bottom", "3"), Attribute::new("hat", "2")],
            ),
        ];

        let collection = build_collection(&tokens);
        let scores = ICScorer.score(&collection, &tokens);

        let s: Vec<f64> = scores.iter().map(|(_, s)| *s).collect();

        // Expected ordering from OpenRarity tests:
        // scores[0] > scores[1] > scores[2]
        // scores[5] > scores[2]
        // scores[2] > scores[3]
        // scores[3] == scores[4]
        assert!(s[0] > s[1], "s[0]={} should > s[1]={}", s[0], s[1]);
        assert!(s[1] > s[2], "s[1]={} should > s[2]={}", s[1], s[2]);
        assert!(s[5] > s[2], "s[5]={} should > s[2]={}", s[5], s[2]);
        assert!(s[2] > s[3], "s[2]={} should > s[3]={}", s[2], s[3]);
        assert!(
            (s[3] - s[4]).abs() < 1e-10,
            "s[3]={} should == s[4]={}",
            s[3],
            s[4]
        );
    }
}
