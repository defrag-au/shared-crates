use std::collections::BTreeMap;

use crate::Token;

/// Precomputed collection-level statistics for rarity scoring.
#[derive(Debug)]
pub struct Collection {
    pub total_supply: usize,
    /// trait_type -> max times it appears on any single token.
    pub shape: BTreeMap<String, usize>,
    /// `(trait_type, slot_index)` -> value -> count across collection.
    /// `slot_index` is 0 for single-occurrence traits.
    /// For duplicate trait_types (e.g. multiple "Outfit" entries), each
    /// occurrence gets its own slot index (sorted alphabetically).
    pub frequencies: BTreeMap<(String, usize), BTreeMap<String, usize>>,
}

impl Collection {
    /// Total number of distinct values for a `(trait_type, slot_index)` pair,
    /// including null if present.
    pub fn total_values_for_slot(&self, trait_type: &str, slot_index: usize) -> usize {
        self.frequencies
            .get(&(trait_type.to_string(), slot_index))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Count of tokens with a specific `(trait_type, slot_index, value)`.
    pub fn count_for_value(&self, trait_type: &str, slot_index: usize, value: &str) -> usize {
        self.frequencies
            .get(&(trait_type.to_string(), slot_index))
            .and_then(|m| m.get(value))
            .copied()
            .unwrap_or(0)
    }
}

/// Build collection statistics from a list of tokens.
///
/// Handles duplicate `trait_type` entries by detecting the collection "shape"
/// (max occurrences per trait_type) and assigning slot indices. Tokens with
/// fewer occurrences than the max get null markers for missing slots.
pub fn build_collection(tokens: &[Token]) -> Collection {
    let total_supply = tokens.len();

    // Step 1: Determine collection shape — max count per trait_type
    let mut shape: BTreeMap<String, usize> = BTreeMap::new();
    for token in tokens {
        let mut token_trait_counts: BTreeMap<String, usize> = BTreeMap::new();
        for attr in &token.attributes {
            *token_trait_counts
                .entry(attr.trait_type.clone())
                .or_insert(0) += 1;
        }
        for (trait_type, count) in &token_trait_counts {
            let max = shape.entry(trait_type.clone()).or_insert(0);
            if *count > *max {
                *max = *count;
            }
        }
    }

    // Step 2: Build frequency maps with slot indices.
    // For each token, sort its values per trait_type alphabetically so that
    // slot assignment is deterministic. Then pad with null markers.
    let mut frequencies: BTreeMap<(String, usize), BTreeMap<String, usize>> = BTreeMap::new();

    // Initialize all slots
    for (trait_type, max_count) in &shape {
        for slot_idx in 0..*max_count {
            frequencies.insert((trait_type.clone(), slot_idx), BTreeMap::new());
        }
    }

    for token in tokens {
        // Group this token's values by trait_type, sorted
        let mut token_values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for attr in &token.attributes {
            token_values
                .entry(attr.trait_type.clone())
                .or_default()
                .push(attr.value.clone());
        }

        // For each trait_type in the shape, assign values to slots
        for (trait_type, max_count) in &shape {
            let values = token_values.get(trait_type);
            let mut sorted_values: Vec<String> = values
                .map(|v| {
                    let mut s = v.clone();
                    s.sort();
                    s
                })
                .unwrap_or_default();

            // Pad with unique null markers for missing slots
            let present_count = sorted_values.len();
            for i in 0..(*max_count - present_count) {
                // Each missing slot gets a unique null marker so they don't
                // share probability — per Magic Eden spec
                sorted_values.push(format!("__null_{i}"));
            }

            for (slot_idx, value) in sorted_values.iter().enumerate() {
                *frequencies
                    .get_mut(&(trait_type.clone(), slot_idx))
                    .unwrap()
                    .entry(value.clone())
                    .or_insert(0) += 1;
            }
        }
    }

    Collection {
        total_supply,
        shape,
        frequencies,
    }
}

/// Get the normalized attribute list for a token against a collection shape.
/// Returns `(trait_type, slot_index, value)` tuples, padded with null markers.
pub fn normalize_token_attributes(
    token: &Token,
    shape: &BTreeMap<String, usize>,
) -> Vec<(String, usize, String)> {
    let mut token_values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for attr in &token.attributes {
        token_values
            .entry(attr.trait_type.clone())
            .or_default()
            .push(attr.value.clone());
    }

    let mut result = Vec::new();

    for (trait_type, max_count) in shape {
        let mut sorted_values: Vec<String> = token_values
            .get(trait_type)
            .map(|v| {
                let mut s = v.clone();
                s.sort();
                s
            })
            .unwrap_or_default();

        let present_count = sorted_values.len();
        for i in 0..(*max_count - present_count) {
            sorted_values.push(format!("__null_{i}"));
        }

        for (slot_idx, value) in sorted_values.iter().enumerate() {
            result.push((trait_type.clone(), slot_idx, value.clone()));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Attribute;

    #[test]
    fn test_shape_single_traits() {
        let tokens = vec![
            Token::new(
                "1",
                vec![Attribute::new("hat", "red"), Attribute::new("body", "blue")],
            ),
            Token::new(
                "2",
                vec![
                    Attribute::new("hat", "green"),
                    Attribute::new("body", "blue"),
                ],
            ),
        ];
        let col = build_collection(&tokens);
        assert_eq!(col.shape["hat"], 1);
        assert_eq!(col.shape["body"], 1);
    }

    #[test]
    fn test_shape_duplicate_trait_types() {
        let tokens = vec![
            Token::new(
                "1",
                vec![
                    Attribute::new("outfit", "jeans"),
                    Attribute::new("outfit", "tee"),
                    Attribute::new("outfit", "jacket"),
                ],
            ),
            Token::new("2", vec![Attribute::new("outfit", "shorts")]),
        ];
        let col = build_collection(&tokens);
        assert_eq!(col.shape["outfit"], 3);
        // Token 2 should have 2 null markers for the missing outfit slots
        assert_eq!(col.count_for_value("outfit", 1, "__null_0"), 1);
        assert_eq!(col.count_for_value("outfit", 2, "__null_1"), 1);
    }

    #[test]
    fn test_missing_trait_gets_null() {
        let tokens = vec![
            Token::new(
                "1",
                vec![
                    Attribute::new("hat", "red"),
                    Attribute::new("special", "true"),
                ],
            ),
            Token::new("2", vec![Attribute::new("hat", "blue")]),
        ];
        let col = build_collection(&tokens);
        assert_eq!(col.shape["special"], 1);
        // Token 2 is missing "special" — gets null
        assert_eq!(col.count_for_value("special", 0, "__null_0"), 1);
        assert_eq!(col.count_for_value("special", 0, "true"), 1);
    }
}
