use std::{io::prelude::Write, path::PathBuf};

use winnow::{
    Parser,
    binary::be_u32,
    combinator::{alt, seq},
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

    pub object_type: ObjectType,

    /// 0755 or 0644
    pub permission: u8,

    pub uid: u32,

    pub gid: u32,

    pub file_size: u32,
}

#[derive(Debug, Clone)]
pub enum ObjectType {
    Regular,
    SymbolicLink,
    GitLink,
}

impl Decode for ObjectType {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        alt((
            0b1000.value(Self::Regular),
            0b1010.value(Self::SymbolicLink),
            0b1110.value(Self::GitLink),
        ))
        .parse_next(input)
    }
}

impl Encode for ObjectType {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        match self {
            ObjectType::Regular => writer.write_all(&[0b1000]),
            ObjectType::SymbolicLink => writer.write_all(&[0b1010]),
            ObjectType::GitLink => writer.write_all(&[0b1110]),
        }?;

        Ok(())
    }
}
