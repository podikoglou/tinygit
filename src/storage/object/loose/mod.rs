//! This module deals with loose objects, i.e. objects in `.git/objects/`.

pub mod bucket;
pub mod db;

pub use db::LooseDb;
use nutype::nutype;
use winnow::{
    Parser,
    error::{ContextError, ErrMode, FromExternalError as _},
    token::rest,
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    object::{ObjectType, object},
    parsing::Stream,
    storage::object::RawObject,
};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, prelude::Write};

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

impl From<CompressedLooseObject> for LooseObject {
    fn from(val: CompressedLooseObject) -> Self {
        val.into_inner()
    }
}

#[nutype(derive(Debug, PartialEq, AsRef))]
pub struct CompressedLooseObject(LooseObject);

impl Encode for CompressedLooseObject {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let mut encoder = ZlibEncoder::new(writer, Compression::default());

        self.as_ref().encode(&mut encoder)
    }
}

impl Decode for CompressedLooseObject {
    fn parse(input: &mut Stream<'_>) -> winnow::prelude::ModalResult<Self> {
        let bytes = rest.parse_next(input)?;

        let mut decoder = ZlibDecoder::new(bytes);

        let mut buf = Vec::new();

        decoder
            .read_to_end(&mut buf)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        LooseObject::parse(&mut Stream::new(&buf)).map(Self::new)
    }
}

impl From<LooseObject> for CompressedLooseObject {
    fn from(val: LooseObject) -> Self {
        Self::new(val)
    }
}
