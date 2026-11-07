//! This module deals with packed objects.
//!
//! The main types are [DeltifiedObject], [UndeltifiedObject], and [PackedObject] that holds one of
//! the two.
use std::io::{Read, Write};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use nutype::nutype;
use strum::FromRepr;
use winnow::{
    ModalResult, Parser,
    combinator::alt,
    error::{ContextError, ErrMode, FromExternalError, StrContext},
    stream::Location,
    token::{any, take},
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    object::{
        ObjectType,
        hash::{ObjectHash, parse_object_hash},
    },
    parsing::Stream,
    storage::object::{
        RawObject,
        packed::{delta::Delta, offset},
    },
};

pub const MAX_BUF_SIZE: u64 = 512 * 1024 * 1024; // 512MB, like Git

#[derive(Debug, PartialEq)]
pub struct PackedObjectHeader {
    /// The object type.
    pub r#type: PackedObjectType,

    /// The length of the *decompressed* data.
    pub length: u64,
}

impl Encode for PackedObjectHeader {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let type_id = self.r#type as u8;

        let mut value = self.length;

        let mut curr_byte = type_id << 4 | (value & 0x0F) as u8;
        value >>= 4;

        if value != 0 {
            curr_byte |= 1 << 7;
        }

        writer.write_all(&[curr_byte])?;

        while value != 0 {
            curr_byte = (value & 0x7F) as u8;
            value >>= 7;

            if value != 0 {
                curr_byte |= 1 << 7;
            }

            writer.write_all(&[curr_byte])?;
        }

        Ok(())
    }
}

#[derive(FromRepr, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum PackedObjectType {
    Commit = 1,
    Tree = 2,
    Blob = 3,
    Tag = 4,
    OfsDelta = 6,
    RefDelta = 7,
}

#[derive(Debug, PartialEq)]
pub enum BaseObject {
    Ref(ObjectHash),

    Ofs(u64),
}

impl Encode for BaseObject {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        match self {
            BaseObject::Ref(hash) => {
                let bytes: &[u8] = hash.into();

                writer.write_all(bytes)?;
            }
            BaseObject::Ofs(offset) => {
                let mut num = *offset;
                let mut buf = [0u8; 10];
                let mut pos = buf.len();

                pos -= 1;
                buf[pos] = (num & 0x7F) as u8;

                while num > 0x7F {
                    num = (num >> 7) - 1;
                    pos -= 1;
                    buf[pos] = 0x80 | ((num & 0x7F) as u8);
                }

                writer.write_all(&buf[pos..])?;
            }
        }

        Ok(())
    }
}

#[nutype(derive(Debug, PartialEq, AsRef))]
pub struct DeltifiedObject((BaseObject, Delta));

impl Encode for DeltifiedObject {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let (base_object, delta) = self.as_ref();

        let compressed_buf: Vec<u8> = Vec::default();
        let mut encoder = ZlibEncoder::new(compressed_buf, Compression::default());

        delta.encode(&mut encoder)?;

        let uncompressed_length = encoder.total_in();

        let compressed_buf = encoder.finish()?;

        let header = PackedObjectHeader {
            r#type: match base_object {
                BaseObject::Ref(_) => PackedObjectType::RefDelta,
                BaseObject::Ofs(_) => PackedObjectType::OfsDelta,
            },
            length: uncompressed_length,
        };

        header.encode(writer)?;
        base_object.encode(writer)?;

        writer.write_all(&compressed_buf)?;

        Ok(())
    }
}

#[nutype(derive(Debug, PartialEq, AsRef))]
pub struct UndeltifiedObject(RawObject);

impl Encode for UndeltifiedObject {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let raw_object = self.as_ref();

        let header = PackedObjectHeader {
            r#type: match raw_object.r#type {
                // TODO: this should be somewhere else, possibly already a function for it i can't
                // think of right now
                ObjectType::Blob => PackedObjectType::Blob,
                ObjectType::Tree => PackedObjectType::Tree,
                ObjectType::Commit => PackedObjectType::Commit,
                ObjectType::Tag => PackedObjectType::Tag,
            },
            length: raw_object.bytes.len() as u64,
        };

        header.encode(writer)?;

        let mut encoder = ZlibEncoder::new(writer, Compression::default());

        // NOTE: This bypasses the `RawObject` `Encode` implementation, because that is meant for
        // loose objects which have a different format. perhaps this is a bad API.
        // Possible future work: don't implement `Encode` for `RawObject`; have newtypes over it
        // depending on the way you want to encode it.
        encoder.write_all(&raw_object.bytes)?;

        encoder.finish()?;

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum PackedObject {
    Deltified(usize, DeltifiedObject),
    Undeltified(usize, UndeltifiedObject),
}

impl PackedObject {
    /// Gets the offset of the object in the packfile.
    pub fn offset(&self) -> usize {
        match self {
            PackedObject::Deltified(offset, _) => *offset,
            PackedObject::Undeltified(offset, _) => *offset,
        }
    }
}

impl Encode for PackedObject {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        match self {
            PackedObject::Deltified(_offset, object) => object.encode(writer),
            PackedObject::Undeltified(_offset, object) => object.encode(writer),
        }
    }
}

impl Decode for PackedObjectHeader {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let mut current_byte = any.parse_next(input)?;

        let type_id = (current_byte & 0b01110000) >> 4;

        // TODO: better error handling
        let r#type = PackedObjectType::from_repr(type_id)
            .ok_or_else(|| ErrMode::Cut(ContextError::new()))?;

        let mut size = (current_byte & 0b00001111) as u64;
        let mut pos = 4;

        // keep reading, the continuation bit is 1
        // (thanks for the trick Aditya! - https://codewords.recurse.com/issues/three/unpacking-git-packfiles)
        while current_byte >= 128 && pos < 67 {
            current_byte = any.parse_next(input)?;

            // these are the bits that constitute the size, which is the byte as-is, but with the first
            // bit masked off as it's the continuation bit
            let chunk = (current_byte & 0b0111_1111) as u64;

            size |= chunk << pos;
            pos += 7;
        }

        Ok(PackedObjectHeader {
            r#type,
            length: size,
        })
    }
}

pub fn base_object(
    r#type: PackedObjectType,
) -> impl FnMut(&mut Stream<'_>) -> ModalResult<BaseObject> {
    match r#type {
        PackedObjectType::OfsDelta => |input: &mut Stream<'_>| {
            offset
                .map(BaseObject::Ofs)
                .context(StrContext::Label("base object offset"))
                .parse_next(input)
        },
        PackedObjectType::RefDelta => |input: &mut Stream<'_>| {
            parse_object_hash
                .map(BaseObject::Ref)
                .context(StrContext::Label("base object name"))
                .parse_next(input)
        },

        _ => unreachable!(),
    }
}

impl Decode for PackedObject {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let offset = input.current_token_start();

        alt((
            DeltifiedObject::parse
                .map(|object| PackedObject::Deltified(offset, object))
                .context(StrContext::Label("deltified object")),
            UndeltifiedObject::parse
                .map(|object| PackedObject::Undeltified(offset, object))
                .context(StrContext::Label("undeltified object")),
        ))
        .context(StrContext::Label("object"))
        .parse_next(input)
    }
}

impl Decode for DeltifiedObject {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let offset = input.current_token_start();

        let header = PackedObjectHeader::parse
            .verify(|header| {
                matches!(
                    header.r#type,
                    PackedObjectType::OfsDelta | PackedObjectType::RefDelta
                )
            })
            .context(StrContext::Label("object header"))
            .parse_next(input)?;

        let base_object = base_object(header.r#type)
            .context(StrContext::Label("base object"))
            .verify_map(|base| {
                match base {
                    BaseObject::Ofs(relative_offset) => {
                        // we resolve the relative offset ofs_delta gives us offset to a file-level offset
                        Some(BaseObject::Ofs(
                            (offset as u64).checked_sub(relative_offset)?,
                        ))
                    }

                    other => Some(other),
                }
            })
            .parse_next(input)?;

        if header.length > MAX_BUF_SIZE {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        let mut buf = Vec::default();
        buf.try_reserve_exact(header.length as usize)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        let mut decoder = ZlibDecoder::new(&input[..]);

        decoder
            .read_to_end(&mut buf)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        if buf.len() != header.length as usize {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        // consume the amount of bytes that the decoder read, advancing the `input` slice
        take(decoder.total_in() as usize).parse_next(input)?;

        let delta = Delta::parse.parse_next(&mut Stream::new(&buf))?;

        Ok(DeltifiedObject::new((base_object, delta)))
    }
}

impl Decode for UndeltifiedObject {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let header = PackedObjectHeader::parse
            .verify(|header| {
                matches!(
                    header.r#type,
                    PackedObjectType::Commit
                        | PackedObjectType::Tag
                        | PackedObjectType::Blob
                        | PackedObjectType::Tree
                )
            })
            .context(StrContext::Label("object header"))
            .parse_next(input)?;

        if header.length > MAX_BUF_SIZE {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        // we create a buffer with the size we read from the header
        //
        // keep in mind the fact that this size is the size of the decompressed data
        // rather than the compressed data
        let mut buf = Vec::default();
        buf.try_reserve_exact(header.length as usize)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        if header.length == 0 {
            // fixed-sized zlib header + trailer
            take(8usize).parse_next(input)?;
        } else {
            let mut decoder = ZlibDecoder::new(&input[..]);

            decoder
                .read_to_end(&mut buf)
                .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

            if buf.len() != header.length as usize {
                return Err(ErrMode::Cut(ContextError::new()));
            }

            // consume the amount of bytes that the decoder read, advancing the `input` slice
            take(decoder.total_in() as usize).parse_next(input)?;
        }

        let r#type = match header.r#type {
            PackedObjectType::Commit => ObjectType::Commit,
            PackedObjectType::Tree => ObjectType::Tree,
            PackedObjectType::Blob => ObjectType::Blob,
            PackedObjectType::Tag => ObjectType::Tag,

            _ => unreachable!(),
        };

        Ok(UndeltifiedObject::new(RawObject::new(r#type, buf)))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ende::Decode,
        parsing::Stream,
        storage::object::packed::{PackedObjectHeader, UndeltifiedObject},
    };

    use super::{BaseObject, PackedObjectType, base_object};
    use insta::assert_debug_snapshot;

    use winnow::Parser;

    #[test]
    fn object_header_parses_basic_headers() {
        let (rest, header) = PackedObjectHeader::parse
            .parse_peek(Stream::new(&[0x99, 0x0a, 0x78, 0x9c][..]))
            .expect("commit header should parse");
        assert_eq!(header.r#type, PackedObjectType::Commit);
        assert_eq!(header.length, 169);
        assert_eq!(*rest, &[0x78, 0x9c]);

        let (rest, header) = PackedObjectHeader::parse
            .parse_peek(Stream::new(&[0xb7, 0x02, 0x78, 0x9c][..]))
            .expect("blob header should parse");
        assert_eq!(header.r#type, PackedObjectType::Blob);
        assert_eq!(header.length, 39);
        assert_eq!(*rest, &[0x78, 0x9c]);

        let (rest, header) = PackedObjectHeader::parse
            .parse_peek(Stream::new(&[0xa4, 0x02, 0x78, 0x9c][..]))
            .expect("tree header should parse");
        assert_eq!(header.r#type, PackedObjectType::Tree);
        assert_eq!(header.length, 36);
        assert_eq!(*rest, &[0x78, 0x9c]);
    }

    #[test]
    fn object_header_parses_single_byte_header() {
        let (rest, header) = PackedObjectHeader::parse
            .parse_peek(Stream::new(&[0x19, 0x0a][..]))
            .expect("commit header should parse");
        assert_eq!(header.r#type, PackedObjectType::Commit);
        assert_eq!(header.length, 9);
        assert_eq!(*rest, &[0x0a]);
    }

    #[test]
    fn undeltified_object_parses_basic_object() {
        let (_, obj) = UndeltifiedObject::parse
            .parse_peek(Stream::new(&[
                0x99, 0x0a, 0x78, 0x9c, 0x9d, 0xcb, 0x4d, 0x0a, 0xc2, 0x30, 0x10, 0x40, 0xe1, 0x7d,
                0x4e, 0x31, 0x7b, 0xa1, 0x64, 0x12, 0xf3, 0x53, 0x10, 0xf1, 0x2a, 0x99, 0x66, 0xd2,
                0x0e, 0x26, 0x46, 0x4a, 0x0a, 0x1e, 0x5f, 0xbd, 0x42, 0x37, 0x6f, 0xf1, 0xc1, 0x1b,
                0x3b, 0x33, 0xc4, 0x84, 0x3e, 0x73, 0x28, 0x14, 0x09, 0x83, 0x8f, 0x64, 0x9c, 0x0b,
                0x4b, 0xd1, 0x96, 0x8c, 0x29, 0xd1, 0x73, 0x66, 0x6b, 0x08, 0x73, 0xc9, 0x2a, 0x1d,
                0x63, 0xeb, 0x3b, 0xa4, 0xca, 0x1f, 0xb8, 0xfd, 0x3b, 0xbd, 0x7b, 0x96, 0x67, 0x5f,
                0x6b, 0x3f, 0x1e, 0x6b, 0x4b, 0x52, 0xa7, 0xa5, 0xb7, 0x3b, 0x60, 0x88, 0xb3, 0xf7,
                0xd7, 0x19, 0x1d, 0x5c, 0xb4, 0xd5, 0x5a, 0xfd, 0xb4, 0xc9, 0x18, 0x7c, 0xe6, 0x55,
                0xf2, 0x92, 0xa1, 0xbe, 0xc5, 0xe5, 0x34, 0x91, 0xb7, 0x02, 0x78, 0x9c,
            ]))
            .expect("commit object should parse");

        assert_debug_snapshot!(obj);
    }

    #[test]
    fn parse_base_object_parses_offset_delta() {
        let (rest, base_object) = base_object(PackedObjectType::OfsDelta)
            .parse_peek(Stream::new(&[0x81, 0x08, 0x78, 0x9c][..]))
            .expect("ofs delta base object should parse");

        assert_eq!(base_object, BaseObject::Ofs(264));
        assert_eq!(*rest, &[0x78, 0x9c]);
    }
}
