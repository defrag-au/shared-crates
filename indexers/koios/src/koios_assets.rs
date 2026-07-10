use crate::koios_serde::as_f64;

use nom::{
    bytes::complete::{tag, take_until},
    character::complete::{char, digit1},
    combinator::map_res,
    multi::separated_list0,
    sequence::{delimited as del, separated_pair},
    IResult, Parser,
};
use serde::{
    de::{self, value::SeqAccessDeserializer, SeqAccess, Visitor},
    Serialize,
};
use serde::{Deserialize, Deserializer};
use std::{fmt, vec::IntoIter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KoiosAsset {
    pub policy_id: String,
    pub asset_name: String,
    pub fingerprint: String,
    #[serde(deserialize_with = "as_f64")]
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct KoiosAssetList(pub Vec<KoiosAsset>);

impl std::iter::IntoIterator for KoiosAssetList {
    type Item = KoiosAsset;
    type IntoIter = IntoIter<KoiosAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl std::iter::IntoIterator for &KoiosAssetList {
    type Item = KoiosAsset;
    type IntoIter = IntoIter<KoiosAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.clone().into_iter()
    }
}

impl<'de> Deserialize<'de> for KoiosAssetList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KoiosAssetListVisitor;

        impl<'de> Visitor<'de> for KoiosAssetListVisitor {
            type Value = KoiosAssetList;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a Koios asset list as a string or a structured array")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                parse_asset_list_nom(s)
                    .map(KoiosAssetList)
                    .map_err(de::Error::custom)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let inner = <Vec<KoiosAsset>>::deserialize(SeqAccessDeserializer::new(seq))?;
                Ok(KoiosAssetList(inner))
            }
        }

        deserializer.deserialize_any(KoiosAssetListVisitor)
    }
}

pub fn parse_asset_list_nom(input: &str) -> Result<Vec<KoiosAsset>, String> {
    match asset_list(input) {
        Ok((_, assets)) => Ok(assets),
        Err(e) => Err(format!("parse error: {e:?}")),
    }
}

fn quoted_string(input: &str) -> IResult<&str, &str> {
    del(char('"'), take_until("\""), char('"'))(input)
}

fn parse_f64(input: &str) -> IResult<&str, f64> {
    map_res(digit1, str::parse::<f64>)(input)
}

fn asset_pair(input: &str) -> IResult<&str, (String, f64)> {
    del(
        char('('),
        separated_pair(
            quoted_string.map(std::string::ToString::to_string),
            char(','),
            parse_f64,
        ),
        char(')'),
    )(input)
}

fn asset_pairs(input: &str) -> IResult<&str, Vec<(String, f64)>> {
    del(char('['), separated_list0(tag(","), asset_pair), char(']'))(input)
}

type PolicyBlockOutput = (String, Vec<(String, f64)>);
fn policy_id_block(input: &str) -> IResult<&str, PolicyBlockOutput> {
    del(
        tag("(PolicyID {policyID = ScriptHash "),
        separated_pair(
            quoted_string.map(std::string::ToString::to_string),
            tag("},"),
            asset_pairs,
        ),
        char(')'),
    )(input)
}

fn asset_list(input: &str) -> IResult<&str, Vec<KoiosAsset>> {
    del(
        char('['),
        separated_list0(tag(","), policy_id_block),
        char(']'),
    )(input)
    .map(|(rest, blocks)| {
        let mut all = Vec::new();
        for (policy, assets) in blocks {
            all.extend(assets.into_iter().map(|(name, qty)| KoiosAsset {
                policy_id: policy.clone(),
                asset_name: name.clone(),
                quantity: qty,
                // TODO: properly compute the fingerprint, for now we'll fake it
                fingerprint: format!("{}{}", &policy, &name),
            }));
        }
        (rest, all)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nom_parser() {
        let input = r#"[(PolicyID {policyID = ScriptHash \"861dabc123\"},[(\"deadbeef\",100),(\"cafebabe\",250)])]"#;
        let input = input.replace("\\\"", "\""); // simulate actual Koios format
        let assets = parse_asset_list_nom(&input).expect("should parse");

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].policy_id, "861dabc123");
        assert_eq!(assets[0].asset_name, "deadbeef");
        assert_eq!(assets[0].quantity, 100.0);

        assert_eq!(assets[1].asset_name, "cafebabe");
        assert_eq!(assets[1].quantity, 250.0);
    }
}
