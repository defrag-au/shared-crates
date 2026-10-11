//! `0011-direct-clean-path-layout`: map logical paths to safe content paths by
//! replacing or removing "dangerous" characters — after David A. Wheeler's
//! *Fixing Unix/Linux/POSIX Filenames*.
//!
//! This is a *path-mapping* extension, not a storage layout: [`DirectClean`]
//! implements the projection itself, so it composes with any writer — clean a
//! logical path, then store the content at the result while the logical state
//! keeps the original name.
//!
//! With `encodeUTF` on, every dangerous character becomes its `=uXXXX` code and
//! the mapping is injective; with it off, characters are replaced by
//! `replacementString` and the projection can collide (`~file`, `-file` and
//! `file` all map to `file`) — the spec's caveat, and the caller's decision.
//! When a segment or the whole path exceeds its length limit the extension
//! falls back to a digest-derived name under `fallbackFolder`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::digest::DigestAlgorithm;

/// What the mapping can refuse.
#[derive(Debug, Error)]
pub enum PathMapError {
    #[error("{0}")]
    Config(String),
    #[error("the path maps to nothing (every segment was removed)")]
    EmptyPath,
    #[error("the fallback path {path:?} is {} characters, over the {max}-character limit", path.len())]
    TooLong { path: String, max: usize },
}

/// The extension's parameters, with the spec's defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectCleanConfig {
    /// Replace dangerous characters with their `=uXXXX` codes rather than
    /// `replacementString`. Injective when on (the spec's caveat).
    #[serde(default)]
    pub encode_utf: bool,
    /// Maximum characters in one `/`-separated segment; more is an error.
    #[serde(default = "default_max_path_segment_len")]
    pub max_path_segment_len: usize,
    /// Maximum characters in the whole result; more is an error.
    #[serde(default = "default_max_pathname_len")]
    pub max_pathname_len: usize,
    /// Replaces dangerous (non-whitespace) characters. With `encode_utf` on,
    /// only used for characters that cannot be UTF-encoded.
    #[serde(default = "default_replacement")]
    pub replacement_string: String,
    /// Replaces whitespace when `encode_utf` is off. The empty string removes
    /// inner whitespace.
    #[serde(default = "default_whitespace_replacement")]
    pub whitespace_replacement_string: String,
    /// Digest for fallback names; restricted to the OCFL fixity vocabulary.
    #[serde(default = "default_md5")]
    pub fallback_digest_algorithm: DigestAlgorithm,
    /// The folder fallback names live under.
    #[serde(default = "default_fallback_folder")]
    pub fallback_folder: String,
    /// Sub-folders built from the fallback digest's prefix.
    #[serde(default)]
    pub number_of_fallback_tuples: usize,
    /// Characters per fallback sub-folder.
    #[serde(default = "default_one")]
    pub fallback_tuple_size: usize,
}

impl Default for DirectCleanConfig {
    fn default() -> Self {
        Self {
            encode_utf: false,
            max_path_segment_len: default_max_path_segment_len(),
            max_pathname_len: default_max_pathname_len(),
            replacement_string: default_replacement(),
            whitespace_replacement_string: default_whitespace_replacement(),
            fallback_digest_algorithm: default_md5(),
            fallback_folder: default_fallback_folder(),
            number_of_fallback_tuples: 0,
            fallback_tuple_size: 1,
        }
    }
}

fn default_max_path_segment_len() -> usize {
    127
}

fn default_max_pathname_len() -> usize {
    32000
}

fn default_replacement() -> String {
    "_".to_string()
}

fn default_whitespace_replacement() -> String {
    " ".to_string()
}

fn default_md5() -> DigestAlgorithm {
    DigestAlgorithm::Md5
}

fn default_fallback_folder() -> String {
    "fallback".to_string()
}

fn default_one() -> usize {
    1
}

/// The `0011` projection.
#[derive(Clone)]
pub struct DirectClean {
    encode_utf: bool,
    max_path_segment_len: usize,
    max_pathname_len: usize,
    replacement_string: String,
    whitespace_replacement_string: String,
    fallback_digest_algorithm: DigestAlgorithm,
    fallback_folder: String,
    number_of_fallback_tuples: usize,
    fallback_tuple_size: usize,
}

impl DirectClean {
    /// Validate and adopt a configuration.
    pub fn new(config: DirectCleanConfig) -> Result<Self, PathMapError> {
        if config.max_path_segment_len == 0 {
            return Err(PathMapError::Config(
                "maxPathSegmentLen must be greater than 0".into(),
            ));
        }
        if config.max_pathname_len == 0 {
            return Err(PathMapError::Config(
                "maxPathnameLen must be greater than 0".into(),
            ));
        }
        if config.fallback_tuple_size == 0 {
            return Err(PathMapError::Config(
                "fallbackTupleSize must be greater than 0".into(),
            ));
        }
        if config.fallback_folder.is_empty() {
            return Err(PathMapError::Config("fallbackFolder is empty".into()));
        }
        let Some(hex_len) = config.fallback_digest_algorithm.hex_len() else {
            return Err(PathMapError::Config(format!(
                "fallbackDigestAlgorithm {} is not a fixed-length hash",
                config.fallback_digest_algorithm
            )));
        };
        if config.number_of_fallback_tuples * config.fallback_tuple_size > hex_len {
            return Err(PathMapError::Config(format!(
                "numberOfFallbackTuples × fallbackTupleSize exceeds the {hex_len}-character {} digest",
                config.fallback_digest_algorithm
            )));
        }
        Ok(Self {
            encode_utf: config.encode_utf,
            max_path_segment_len: config.max_path_segment_len,
            max_pathname_len: config.max_pathname_len,
            replacement_string: config.replacement_string,
            whitespace_replacement_string: config.whitespace_replacement_string,
            fallback_digest_algorithm: config.fallback_digest_algorithm,
            fallback_folder: config.fallback_folder,
            number_of_fallback_tuples: config.number_of_fallback_tuples,
            fallback_tuple_size: config.fallback_tuple_size,
        })
    }

    /// Parse the extension's `config.json` (an unknown `extensionName` key is
    /// ignored, as any other unknown key).
    pub fn from_json(bytes: &[u8]) -> Result<Self, PathMapError> {
        let config: DirectCleanConfig = serde_json::from_slice(bytes)
            .map_err(|error| PathMapError::Config(error.to_string()))?;
        Self::new(config)
    }

    /// Map a logical path or identifier to a safe content path. Lengths are
    /// counted in bytes, as the reference implementation does (the spec's
    /// stated rationale is filesystem byte limits).
    pub fn clean(&self, path: &str) -> Result<String, PathMapError> {
        let mut parts = Vec::new();
        for part in path.split('/') {
            if part.is_empty() {
                continue;
            }
            let cleaned = if self.encode_utf {
                self.clean_utf(part)
            } else {
                self.clean_plain(part)
            };
            if cleaned.len() > self.max_path_segment_len {
                return self.fallback(path);
            }
            if !cleaned.is_empty() {
                parts.push(cleaned);
            }
        }
        let joined = parts.join("/");
        if joined.len() > self.max_pathname_len {
            return self.fallback(path);
        }
        if joined.is_empty() {
            return Err(PathMapError::EmptyPath);
        }
        Ok(joined)
    }

    fn clean_utf(&self, part: &str) -> String {
        let escaped = escape_equals(part);
        let mut out = String::new();
        for character in escaped.chars() {
            if is_dangerous_utf(character) {
                out.push_str(&utf_code(character));
            } else {
                out.push(character);
            }
        }
        let chars: Vec<char> = out.chars().collect();
        if let Some(&first) = chars.first()
            && (first == '~' || chars.iter().all(|character| *character == '.'))
        {
            // A leading `~` (home-relative) and an all-periods segment (`.`,
            // `..`) must not survive unmapped.
            let head = utf_code(first);
            let rest: String = chars[1..].iter().collect();
            return format!("{head}{rest}");
        }
        out
    }

    fn clean_plain(&self, part: &str) -> String {
        let mut out = String::new();
        for character in part.chars() {
            if is_whitespace(character) {
                out.push_str(&self.whitespace_replacement_string);
            } else if is_dangerous_plain(character) {
                out.push_str(&self.replacement_string);
            } else {
                out.push(character);
            }
        }
        // Leading `-`, `~` and whitespace go; trailing whitespace goes.
        // (The Go reference's trailing class is widened to U+20A0 by what
        // looks like a typo; the spec prose — "remove trailing spaces" — is
        // followed here.)
        let trimmed = out
            .trim_start_matches(|character: char| {
                is_whitespace(character) || character == '-' || character == '~'
            })
            .trim_end_matches(is_whitespace);
        let chars: Vec<char> = trimmed.chars().collect();
        if !chars.is_empty() && chars.iter().all(|character| *character == '.') {
            let rest: String = chars[1..].iter().collect();
            return format!("{}{rest}", self.replacement_string);
        }
        trimmed.to_string()
    }

    /// The fallback name: digest of the initial path, chunked to fit
    /// `maxPathSegmentLen`, under `fallbackFolder` and any tuple folders.
    fn fallback(&self, path: &str) -> Result<String, PathMapError> {
        let digest = self
            .fallback_digest_algorithm
            .digest(path.as_bytes())
            .ok_or_else(|| {
                PathMapError::Config(format!(
                    "fallbackDigestAlgorithm {} is unavailable",
                    self.fallback_digest_algorithm
                ))
            })?;
        let mut result = String::new();
        for chunk in digest.as_bytes().chunks(self.max_path_segment_len) {
            if !result.is_empty() {
                result.push('/');
            }
            result.push_str(std::str::from_utf8(chunk).expect("hex digests are ASCII"));
        }
        // The spec: the first `numberOfFallbackTuples × fallbackTupleSize`
        // digest characters, in order, as folders of `fallbackTupleSize`.
        let mut mapped = self.fallback_folder.clone();
        let prefix: String = digest
            .chars()
            .take(self.number_of_fallback_tuples * self.fallback_tuple_size)
            .collect();
        for chunk in prefix.as_bytes().chunks(self.fallback_tuple_size) {
            mapped.push('/');
            mapped.push_str(std::str::from_utf8(chunk).expect("hex digests are ASCII"));
        }
        mapped.push('/');
        mapped.push_str(&result);
        if mapped.len() > self.max_pathname_len {
            return Err(PathMapError::TooLong {
                path: mapped,
                max: self.max_pathname_len,
            });
        }
        Ok(mapped)
    }
}

/// `=uXXXX` (uppercase hex), the extension's UTF encoding.
fn utf_code(character: char) -> String {
    format!("=u{:04X}", character as u32)
}

/// `=` becomes `=u003D` when it introduces something that looks like an
/// existing UTF code (`u` plus four alphanumerics), keeping the encoding
/// reversible.
fn escape_equals(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '='
            && at + 5 < chars.len()
            && chars[at + 1] == 'u'
            && chars[at + 2..at + 6]
                .iter()
                .all(|character| character.is_ascii_alphanumeric())
        {
            out.push_str("=u003D");
        } else {
            out.push(chars[at]);
        }
        at += 1;
    }
    out
}

fn is_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'
            | '\u{000A}'..='\u{000D}'
            | '\u{0020}'
            | '\u{0085}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

fn is_dangerous_utf(character: char) -> bool {
    character <= '\u{001F}'
        || character == '\u{007F}'
        || is_whitespace(character)
        || is_dangerous_punctuation(character)
}

fn is_dangerous_plain(character: char) -> bool {
    // Control characters, DEL, and the punctuation set — but not whitespace
    // beyond the controls, which the whitespace pass owns.
    character <= '\u{001F}' || character == '\u{007F}' || is_dangerous_punctuation(character)
}

fn is_dangerous_punctuation(character: char) -> bool {
    matches!(
        character,
        '*' | '?'
            | ':'
            | '['
            | ']'
            | '"'
            | '<'
            | '>'
            | '|'
            | '('
            | ')'
            | '{'
            | '}'
            | '&'
            | '\''
            | '!'
            | ';'
            | '#'
            | '@'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> DirectClean {
        DirectClean::new(DirectCleanConfig {
            number_of_fallback_tuples: 2,
            ..DirectCleanConfig::default()
        })
        .unwrap()
    }

    fn utf() -> DirectClean {
        DirectClean::new(DirectCleanConfig {
            encode_utf: true,
            fallback_digest_algorithm: DigestAlgorithm::Sha512,
            number_of_fallback_tuples: 2,
            ..DirectCleanConfig::default()
        })
        .unwrap()
    }

    const LONG: &str = "abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij abcdefghijabcdefghij";

    #[test]
    fn the_plain_examples_match_the_spec() {
        let mapper = plain();
        assert_eq!(
            mapper.clean("..hor_rib:lé-$id").unwrap(),
            "..hor_rib_lé-$id"
        );
        assert_eq!(
            mapper.clean("info:fedora/object-01").unwrap(),
            "info_fedora/object-01"
        );
        assert_eq!(
            mapper.clean("~ info:fedora/-obj#ec@t-\"01 ").unwrap(),
            "info_fedora/obj_ec_t-_01"
        );
        assert_eq!(mapper.clean("/test/ ~/.../blah").unwrap(), "test/_../blah");
        assert_eq!(
            mapper
                .clean("https://hdl.handle.net/XXXXX/test/bl ah")
                .unwrap(),
            "https_/hdl.handle.net/XXXXX/test/bl ah"
        );
        assert_eq!(
            mapper.clean(LONG).unwrap(),
            "fallback/0/e/0eafabb38fa7f1583d1461afe980ebdc"
        );
    }

    #[test]
    fn the_utf_examples_match_the_spec() {
        let mapper = utf();
        assert_eq!(
            mapper.clean("..hor_rib:lé-$id").unwrap(),
            "..hor_rib=u003Alé-$id"
        );
        assert_eq!(
            mapper.clean("object=u123a-01").unwrap(),
            "object=u003Du123a-01"
        );
        assert_eq!(mapper.clean("object=u13a-01").unwrap(), "object=u13a-01");
        assert_eq!(
            mapper.clean("info:fedora/object-01").unwrap(),
            "info=u003Afedora/object-01"
        );
        assert_eq!(
            mapper.clean("~ info:fedora/-obj#ec@t-\"01 ").unwrap(),
            "=u007E=u0020info=u003Afedora/-obj=u0023ec=u0040t-=u002201=u0020"
        );
        assert_eq!(
            mapper.clean("/test/ ~/.../blah").unwrap(),
            "test/=u0020~/=u002E../blah"
        );
        assert_eq!(
            mapper
                .clean("https://hdl.handle.net/XXXXX/test/bl ah")
                .unwrap(),
            "https=u003A/hdl.handle.net/XXXXX/test/bl=u0020ah"
        );
        assert_eq!(
            mapper.clean(LONG).unwrap(),
            "fallback/b/8/b8acda4abac53237afa03d6bbb078e1bf46b40438bb256df79b8d9ff0e57b32a688156ad21755363ea19953c160c4dd6d4db175b71e9aa87d68937181a9f69d/9"
        );
    }

    #[test]
    fn a_path_that_maps_to_nothing_is_an_error() {
        let mapper = plain();
        assert!(matches!(mapper.clean(" ~- "), Err(PathMapError::EmptyPath)));
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(
            DirectClean::new(DirectCleanConfig {
                max_path_segment_len: 0,
                ..DirectCleanConfig::default()
            })
            .is_err()
        );
        assert!(
            DirectClean::new(DirectCleanConfig {
                fallback_tuple_size: 0,
                ..DirectCleanConfig::default()
            })
            .is_err()
        );
        assert!(
            DirectClean::new(DirectCleanConfig {
                fallback_digest_algorithm: DigestAlgorithm::Size,
                ..DirectCleanConfig::default()
            })
            .is_err()
        );
        assert!(
            DirectClean::new(DirectCleanConfig {
                number_of_fallback_tuples: 33,
                fallback_tuple_size: 1,
                ..DirectCleanConfig::default()
            })
            .is_err()
        );
    }
}
