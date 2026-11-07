//! This module deals with loose objects, i.e. objects in `.git/objects/`.

pub mod bucket;
pub mod db;

pub use db::LooseDb;
use nutype::nutype;
use winnow::Parser;

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    error::ParserContext,
    object::{Object, ObjectType, object, parse_object},
    parsing::Stream,
    storage::object::RawObject,
};
use flate2::read::ZlibDecoder;
use std::io::{BufRead, Read, prelude::Write};

#[nutype(derive(Debug, PartialEq, AsRef))]
pub struct LooseObject(RawObject);

impl Encode for LooseObject {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let raw = self.as_ref();

        match raw.r#type {
            ObjectType::Blob => write!(writer, "blob"),
            ObjectType::Tree => write!(writer, "tree"),
            ObjectType::Commit => write!(writer, "commit"),
            ObjectType::Tag => write!(writer, "tag"),
        }?;

        write!(writer, " {}\0", raw.bytes.len())?;

        writer.write_all(&raw.bytes)?;

        Ok(())
    }
}

impl Decode for LooseObject {
    fn parse(input: &mut Stream<'_>) -> winnow::prelude::ModalResult<Self> {
        object
            .verify_map(|object| {
                let raw: RawObject = object.try_into().ok()?;

                Some(LooseObject::new(raw))
            })
            .parse_next(input)
    }
}

/// Reads and parses an [Object] from a [`Read`].
///
/// Objects are typically small enough, so this is not a streaming operation.
pub fn read_object(
    mut reader: impl BufRead,
    context: ParserContext,
) -> Result<Object, TinygitError> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;

    parse_object(&buf, context)
}

/// Reads, decompresses and parses an [Object] from a [`Read`].
///
/// Objects are typically small enough, so this is not a streaming operation.
pub fn read_object_compressed(
    reader: impl BufRead,
    context: ParserContext,
) -> Result<Object, TinygitError> {
    let mut decoder = ZlibDecoder::new(reader);

    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;

    parse_object(&buf, context)
}
