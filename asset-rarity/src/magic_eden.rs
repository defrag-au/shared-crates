use crate::collection::normalize_token_attributes;
use crate::{Collection, Scorer, Token};

/// Magic Eden statistical rarity scorer.
///
/// Score = product of `(count / total_supply)` for each trait slot.
/// Lower score = rarer token.
///
/// Reference: <https://help.magiceden.io/en/articles/9062884-how-statistical-rarity-works-for-nfts-on-magic-eden>
pub struct MagicEdenScorer;

impl Scorer for MagicEdenScorer {
    fn score(&self, collection: &Collection, tokens: &[Token]) -> Vec<(String, f64)> {
        let total = collection.total_supply as f64;

        tokens
            .iter()
            .map(|token| {
                let normalized = normalize_token_attributes(token, &collection.shape);
                let score = normalized
                    .iter()
                    .map(|(trait_type, slot_idx, value)| {
                        let count = collection.count_for_value(trait_type, *slot_idx, value) as f64;
                        count / total
                    })
                    .product::<f64>();

                (token.id.clone(), score)
            })
            .collect()
    }

    fn lower_is_rarer(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "Magic Eden Statistical Rarity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_collection, Attribute};

    #[test]
    fn test_uniform_collection_equal_scores() {
        // All tokens have the same trait distribution — should all score equal
        let tokens: Vec<Token> = (0..100)
            .map(|i| {
                let hat = format!("hat_{}", i % 10);
                let body = format!("body_{}", i % 10);
                Token::new(
                    format!("{i}"),
                    vec![Attribute::new("hat", hat), Attribute::new("body", body)],
                )
            })
            .collect();

        let collection = build_collection(&tokens);
        let scores = MagicEdenScorer.score(&collection, &tokens);

        // All scores should be identical
        let first_score = scores[0].1;
        for (_, score) in &scores {
            assert!(
                (score - first_score).abs() < 1e-12,
                "Expected uniform scores, got {score} vs {first_score}"
            );
        }
    }

    #[test]
    fn test_rare_token_scores_lowest() {
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

        // One rare token with unique values
        tokens.push(Token::new(
            "rare",
            vec![
                Attribute::new("hat", "legendary"),
                Attribute::new("body", "legendary"),
            ],
        ));

        let collection = build_collection(&tokens);
        let scores = MagicEdenScorer.score(&collection, &tokens);

        let rare_score = scores.iter().find(|(id, _)| id == "rare").unwrap().1;
        let common_score = scores.iter().find(|(id, _)| id == "0").unwrap().1;

        assert!(
            rare_score < common_score,
            "Rare token ({rare_score}) should score lower than common ({common_score})"
        );
    }

    #[test]
    fn test_duplicate_trait_types() {
        let tokens = vec![
            Token::new(
                "1",
                vec![
                    Attribute::new("outfit", "jeans"),
                    Attribute::new("outfit", "tee"),
                    Attribute::new("hat", "cap"),
                ],
            ),
            Token::new(
                "2",
                vec![
                    Attribute::new("outfit", "jeans"),
                    Attribute::new("outfit", "tee"),
                    Attribute::new("hat", "cap"),
                ],
            ),
            Token::new(
                "3",
                vec![
                    Attribute::new("outfit", "spacesuit"),
                    Attribute::new("hat", "helmet"),
                ],
            ),
        ];

        let collection = build_collection(&tokens);
        let scores = MagicEdenScorer.score(&collection, &tokens);

        // Token 3 has a unique outfit and missing outfit slot — should be rarest
        let score_1 = scores.iter().find(|(id, _)| id == "1").unwrap().1;
        let score_3 = scores.iter().find(|(id, _)| id == "3").unwrap().1;

        assert!(
            score_3 < score_1,
            "Token 3 ({score_3}) should be rarer than token 1 ({score_1})"
        );
    }

    #[test]
    fn test_missing_trait_affects_score() {
        let tokens = vec![
            Token::new(
                "1",
                vec![
                    Attribute::new("hat", "red"),
                    Attribute::new("special", "true"),
                ],
            ),
            Token::new("2", vec![Attribute::new("hat", "red")]),
            Token::new("3", vec![Attribute::new("hat", "red")]),
            Token::new("4", vec![Attribute::new("hat", "red")]),
        ];

        let collection = build_collection(&tokens);
        let scores = MagicEdenScorer.score(&collection, &tokens);

        let score_1 = scores.iter().find(|(id, _)| id == "1").unwrap().1;
        let score_2 = scores.iter().find(|(id, _)| id == "2").unwrap().1;

        // Token 1 has "special=true" (1/4 = 0.25 probability)
        // Token 2 has "special=__null_0" (3/4 = 0.75 probability)
        // So token 1 should be rarer
        assert!(
            score_1 < score_2,
            "Token with rare trait ({score_1}) should score lower than without ({score_2})"
        );
    }
}
