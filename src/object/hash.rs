//! Types and functions dealing with hashing of objects.
use std::{
    fmt::{Debug, Display},
    path::PathBuf,
    str::FromStr,
};

use sha1::digest::{array::Array, consts::U20};
use winnow::{
    ModalResult, Parser,
    error::{StrContext, StrContextValue},
    token::take,
};

use crate::{
    TinygitError,
    error::{ParseError, ParserContext},
    parsing::Stream,
};

/// A hash that identifies an [`super::Object`]. It is a SHA1 hash of the header and
/// contents of the object.
#[derive(PartialEq, Eq, Clone, Default, PartialOrd, Ord, Hash)]
pub struct ObjectHash(Array<u8, U20>);

impl ObjectHash {
    /// Gets the [`HashPrefix`] (first byte) of the hash.
    #[must_use]
    pub fn prefix(&self) -> HashPrefix {
        self.0[0].into()
    }
}

impl From<Array<u8, U20>> for ObjectHash {
    fn from(value: Array<u8, U20>) -> Self {
        Self(value)
    }
}

impl From<[u8; 20]> for ObjectHash {
    fn from(value: [u8; 20]) -> Self {
        Self(value.into())
    }
}

impl<'a> From<&'a ObjectHash> for &'a [u8] {
    fn from(val: &'a ObjectHash) -> Self {
        &val.0
    }
}

impl FromStr for ObjectHash {
    type Err = TinygitError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decoded = hex::decode(value)?;
        let bytes: [u8; 20] = decoded.try_into().map_err(|_| {
            ParseError::new(
                ParserContext::None,
                "Couldn't parse 20 bytes from hexadecimal hash",
            )
        })?;

        Ok(Self(bytes.into()))
    }
}

impl TryFrom<&PathBuf> for ObjectHash {
    type Error = TinygitError;

    /// Converts a [`PathBuf`] into an [`ObjectHash`]. This expects a path to a loose object that is
    /// under a bucket directory.
    fn try_from(value: &PathBuf) -> Result<Self, Self::Error> {
        let prefix = value
            .parent()
            .ok_or(TinygitError::BucketNotFound)?
            .file_name()
            .ok_or(TinygitError::InvalidFileName)?
            .to_string_lossy();

        let name = value
            .file_name()
            .ok_or(TinygitError::InvalidFileName)?
            .to_string_lossy();

        let hash_str = format!("{prefix}{name}");

        hash_str.parse()
    }
}

impl Display for ObjectHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl Debug for ObjectHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<ObjectHash> for Array<u8, U20> {
    fn from(val: ObjectHash) -> Self {
        val.0
    }
}

/// Parses a binary hash from some bytes.
///
/// To parse a UTF-8 hash, see [`object_hash_str`].
pub fn parse_object_hash(input: &mut Stream<'_>) -> ModalResult<ObjectHash> {
    take(20usize)
        .map(Array::try_from)
        .verify_map(Result::ok)
        .map(ObjectHash::from)
        .context(StrContext::Label("object hash"))
        .context(StrContext::Expected(StrContextValue::Description(
            "hash bytes",
        )))
        .parse_next(input)
}

/// Parses a UTF-8 encoded hash from some bytes.
///
/// To parse a binary-encoded hash, see [`object_hash`].
pub fn parse_object_hash_str(input: &mut Stream<'_>) -> ModalResult<ObjectHash> {
    take(40usize)
        .map(str::from_utf8)
        .verify_map(Result::ok)
        .map(str::parse::<ObjectHash>)
        .verify_map(Result::ok)
        .context(StrContext::Label("object hash"))
        .context(StrContext::Expected(StrContextValue::Description(
            "hash string",
        )))
        .parse_next(input)
}

/// Prefix of an [`ObjectHash`]. This is the first byte of the hash.
///
/// This is used in the object store for indexing objects by the first byte of their hash.
#[derive(PartialEq, Eq, Copy, Clone)]
pub struct HashPrefix(u8);

impl Display for HashPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02x}", self.0)
    }
}

impl From<u8> for HashPrefix {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<HashPrefix> for u8 {
    fn from(val: HashPrefix) -> Self {
        val.0
    }
}

impl FromStr for HashPrefix {
    type Err = TinygitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let decoded = hex::decode(s)?;

        // this should be safe because if `hex::decode` returned `Ok()` it probably means it parsed
        // at least one byte
        let first_byte = decoded[0];

        Ok(Self(first_byte))
    }
}

impl Debug for HashPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use winnow::Parser;

    use crate::{
        object::hash::{HashPrefix, ObjectHash, parse_object_hash_str},
        parsing::Stream,
    };

    #[test]
    fn object_hash_str_parses_valid_hashes() {
        assert_eq!(
            parse_object_hash_str
                .parse_peek(Stream::new(b"29f323b31ad129964ffb4f97f203be9c2f35107d"))
                .map(|(r, v)| (*r, v)),
            Ok((
                &b""[..],
                [
                    0x29, 0xf3, 0x23, 0xb3, 0x1a, 0xd1, 0x29, 0x96, 0x4f, 0xfb, 0x4f, 0x97, 0xf2,
                    0x03, 0xbe, 0x9c, 0x2f, 0x35, 0x10, 0x7d
                ]
                .into()
            ))
        )
    }

    #[test]
    fn parse_objecthash_try_from_path() {
        let path = "/home/alex/tinygit/.git/objects/9d/5476d96d3262b69f03f2af27750a495cca43b6"
            .parse::<PathBuf>()
            .unwrap();

        assert_eq!(
            ObjectHash::try_from(&path).expect("can't parse PathBuf into ObjectHash"),
            ObjectHash(
                [
                    0x9d, 0x54, 0x76, 0xd9, 0x6d, 0x32, 0x62, 0xb6, 0x9f, 0x03, 0xf2, 0xaf, 0x27,
                    0x75, 0x0a, 0x49, 0x5c, 0xca, 0x43, 0xb6
                ]
                .into()
            )
        );
    }

    #[test]
    fn hash_prefix_display() {
        assert_eq!(format!("{}", HashPrefix(0x00)), "00".to_string());
        assert_eq!(format!("{}", HashPrefix(0x2f)), "2f".to_string());
        assert_eq!(format!("{}", HashPrefix(0x10)), "10".to_string());
    }

    #[test]
    fn hash_display() {
        assert_eq!(
            format!(
                "{}",
                ObjectHash(
                    [
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
                    ]
                    .into()
                )
            ),
            "0000000000000000000000000000000000000000".to_string()
        );

        assert_eq!(
            format!(
                "{}",
                ObjectHash(
                    [
                        0xdb, 0xc7, 0x4b, 0x22, 0x44, 0xf5, 0x7e, 0xd7, 0xdd, 0xf6, 0xe5, 0xb5,
                        0x3c, 0x17, 0x82, 0x6b, 0xfb, 0xf9, 0xbe, 0x51
                    ]
                    .into()
                )
            ),
            "dbc74b2244f57ed7ddf6e5b53c17826bfbf9be51".to_string()
        );
    }
}
