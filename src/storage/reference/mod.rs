//! This module deals with the storage and parsing of refs (`.git/refs`)

use std::{io::prelude::Write, path::Path};

use winnow::{
    ModalResult, Parser,
    ascii::{newline, till_line_ending},
    combinator::{alt, opt, preceded, terminated},
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    object::hash::{ObjectHash, parse_object_hash_str},
    parsing::Stream,
};

pub mod db;

pub type RefName = String;

#[derive(Debug, PartialEq, Eq)]
pub enum RefPointer {
    Hash(ObjectHash),
    Symbolic(RefName),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Ref {
    pub name: RefName,
    pub pointer: RefPointer,
    pub peeled: Option<ObjectHash>,
}

impl Ref {
    /// Opens a loose ref given its path, and given its name.
    ///
    /// No validation is done to ensure the name matches the path, so is trusted.
    pub fn open_loose(path: impl AsRef<Path>, name: &RefName) -> Result<Ref, TinygitError> {
        let pointer = RefPointer::decode_file(path)?;

        Ok(Ref {
            name: name.to_owned(),
            pointer,
            peeled: None,
        })
    }

    pub fn is_peeled(&self) -> bool {
        self.peeled.is_some()
    }

    /// Returns the name with the `refs/heads/` prefix stripped
    pub fn stripped_name(&self) -> String {
        self.name
            .strip_prefix("refs/heads/")
            .unwrap_or_else(|| &self.name)
            .to_string()
    }
}

impl Decode for RefPointer {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        terminated(
            alt((
                parse_object_hash_str.map(RefPointer::Hash),
                preceded(
                    "ref: ",
                    till_line_ending
                        .map(str::from_utf8)
                        .verify_map(Result::ok)
                        .map(str::to_owned),
                )
                .map(RefPointer::Symbolic),
            )),
            opt(newline),
        )
        .parse_next(input)
    }
}

impl Encode for RefPointer {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        match self {
            RefPointer::Hash(hash) => write!(writer, "{}", hash),
            RefPointer::Symbolic(r#ref) => write!(writer, "ref: {}", r#ref),
        }?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ref_pointer_hash_with_newline() {
        let input = b"61d5cf2c92d28f042275da58239417db20c5c3da\n";
        let pointer = RefPointer::decode_bytes(input);
        assert!(pointer.is_ok(), "expected Ok, got {:?}", pointer);
    }

    #[test]
    fn parses_ref_pointer_symbolic_without_newline() {
        let input = b"ref: refs/heads/main";
        let pointer = RefPointer::decode_bytes(input).unwrap();
        assert_eq!(pointer, RefPointer::Symbolic("refs/heads/main".into()));
    }

    #[test]
    fn parses_ref_pointer_symbolic_with_newline() {
        let input = b"ref: refs/heads/main\n";
        let pointer = RefPointer::decode_bytes(input).unwrap();
        assert_eq!(pointer, RefPointer::Symbolic("refs/heads/main".into()));
    }
}
