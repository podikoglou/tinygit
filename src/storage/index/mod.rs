use std::{io::prelude::Write, path::PathBuf};

use winnow::{
    Parser,
    binary::be_u32,
    combinator::{alt, seq},
    error::{ContextError, ErrMode, FromExternalError},
    prelude::ModalResult,
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    parsing::Stream,
};

/// This is the version this parser/encoder is written for.
const VERSION: u32 = 2;

#[derive(Debug)]
pub struct Index {
    pub path: PathBuf,
}

impl Index {
    pub fn open(path: PathBuf) -> Result<Self, TinygitError> {
        Ok(Self { path })
    }
}

#[derive(Debug)]
pub struct IndexHeader {
    /// Version of the index file
    version: u32,

    /// Amount of entries found in the index
    entries: u32,
}

impl Decode for IndexHeader {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        "DIRC".parse_next(input)?;
        let version = be_u32.verify(|x| *x == VERSION).parse_next(input)?;
        let entries = be_u32.parse_next(input)?;

        Ok(Self { entries, version })
    }
}

impl Encode for IndexHeader {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        writer.write_all(b"DIRC")?;
        writer.write_all(&self.version.to_be_bytes())?;
        writer.write_all(&self.entries.to_be_bytes())?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct IndexEntry {
    /// ctime seconds, the last time a file's metadata changed
    pub ctime_seconds: u32,

    /// ctime nanosecond fractions
    pub ctime_ns_fractions: u32,

    /// mtime seconds, the last time a file's data changed
    pub mtime_seconds: u32,

    /// mtime nanosecond fractions
    pub mtime_ns_fractions: u32,

    /// dev(ice?)
    pub dev: u32,

    /// inode
    pub ino: u32,

    pub mode: Mode,

    pub uid: u32,

    pub gid: u32,

    pub file_size: u32,
}

impl Decode for IndexEntry {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq! {Self {
            ctime_seconds: be_u32,
            ctime_ns_fractions: be_u32,

            mtime_seconds: be_u32,
            mtime_ns_fractions: be_u32,

            dev: be_u32,
            ino: be_u32,

            mode: Mode::parse,

            uid: be_u32,
            gid: be_u32,
            file_size: be_u32,

        }}
        .parse_next(input)
    }
}

/// +------------+------------+------------+------------+
/// |   unused   |  obj type  |   unused   | permission |
/// | 31 .... 16 | 15  ... 12 | 11  ...  9 | 8  ...   0 |
/// +------------+------------+------------+------------+
#[derive(Debug)]
pub struct Mode {
    pub object_type: ObjectType,

    /// 0755 or 0644
    pub permission: u16,
}

impl Decode for Mode {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let value = be_u32.parse_next(input)?;

        let object_type = ObjectType::try_from(((value >> 12) & 0xFF) as u8)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        let permission: u16 = (value & 0x1FF)
            .try_into()
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        // this could be considered too strict
        if permission != 0 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        Ok(Self {
            object_type,
            permission,
        })
    }
}

impl Encode for Mode {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let obj_type: u8 = self.object_type.into();

        let value: u32 = ((obj_type as u32) << 12) & self.permission as u32;

        writer.write_all(&value.to_be_bytes())?;

        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ObjectType {
    Regular,
    SymbolicLink,
    GitLink,
}

impl From<ObjectType> for u8 {
    fn from(val: ObjectType) -> Self {
        match val {
            ObjectType::Regular => 0b1000,
            ObjectType::SymbolicLink => 0b1010,
            ObjectType::GitLink => 0b1110,
        }
    }
}

impl TryFrom<u8> for ObjectType {
    type Error = TinygitError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b1000 => Ok(ObjectType::Regular),
            0b1010 => Ok(ObjectType::SymbolicLink),
            0b1110 => Ok(ObjectType::GitLink),
            other => Err(TinygitError::InvalidObjectType(other)),
        }
    }
}
