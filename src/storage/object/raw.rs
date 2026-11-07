use std::{fmt::Debug, io::Write};

use flate2::{Compression, write::ZlibEncoder};
use humansize::{DECIMAL, format_size};
use sha1::{Digest, Sha1};
use strum::IntoDiscriminant;
use winnow::Parser;

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    error::ParserContext,
    object::{
        Object, ObjectType, blob::Blob, commit::Commit, hash::ObjectHash, tag::Tag, tree::Tree,
    },
    parsing::{ParseNextExt, Stream},
    storage::object::packed::delta::{CopyInstruction, Delta, InsertInstruction, Instruction},
};

/// An object that has not been parsed yet.
///
/// This struct contains its raw, uncompressed bytes (minus its header), and it is not guaranteed
/// that it is a valid object.
///
/// At any given time it can be turned into a real [Object] using [`Self::into_object`].
#[derive(PartialEq, Eq, Clone)]
pub struct RawObject {
    pub r#type: ObjectType,
    pub bytes: Vec<u8>,
}

impl RawObject {
    /// Creates a new [RawObject] given its type and raw uncompressed bytes.
    ///
    /// Does **not** perform any validation.
    pub fn new(r#type: ObjectType, bytes: Vec<u8>) -> Self {
        Self { r#type, bytes }
    }

    /// Parses the object bytes.
    pub fn into_object(&self, context: ParserContext) -> Result<Object, TinygitError> {
        let mut stream = Stream::new(&self.bytes);

        match self.r#type {
            ObjectType::Blob => Blob::parse.map(Object::from).parse_next(&mut stream),
            ObjectType::Tree => Tree::parse.map(Object::from).parse_next(&mut stream),
            ObjectType::Commit => Commit::parse.map(Object::from).parse_next(&mut stream),
            ObjectType::Tag => Tag::parse.map(Object::from).parse_next(&mut stream),
        }
        .context_err(context, &stream)
    }

    /// Computes the hash of the object.
    pub fn hash(&self) -> Result<ObjectHash, TinygitError> {
        let mut buf = Vec::new(); // TODO: capacity

        self.encode(&mut buf)?;

        Ok(Sha1::digest(buf).into())
    }

    /// Returns a new [RawObject] based on this, with a [Delta] applied.
    pub fn apply_delta(&self, delta: Delta) -> RawObject {
        let mut new_bytes: Vec<u8> = Vec::with_capacity(delta.deltified_object_size as usize);

        for instruction in delta.instructions {
            let mut new_data = match instruction {
                Instruction::Insert(InsertInstruction(data)) => data,
                Instruction::Copy(CopyInstruction { offset, size }) => {
                    self.bytes[offset as usize..offset as usize + size as usize].into()
                }
            };

            new_bytes.append(&mut new_data);
        }

        RawObject::new(self.r#type, new_bytes)
    }

    pub fn encode_compressed<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let mut zlib_writer = ZlibEncoder::new(writer, Compression::default());

        self.encode(&mut zlib_writer)
    }
}

impl Encode for RawObject {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        writer.write_all(&self.bytes)?;

        Ok(())
    }
}

impl Debug for RawObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RawObject({:?}, {})",
            self.r#type,
            format_size(self.bytes.len(), DECIMAL)
        )
    }
}

impl TryFrom<Object> for RawObject {
    type Error = TinygitError;

    fn try_from(value: Object) -> Result<Self, Self::Error> {
        let mut buf = Vec::default();

        value.encode(&mut buf)?;

        Ok(Self::new(value.discriminant(), buf))
    }
}
