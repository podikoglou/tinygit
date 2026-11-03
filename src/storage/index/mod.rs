use std::{io::prelude::Write, path::PathBuf};

use bstr::BString;
use winnow::{
    Parser,
    binary::{be_u16, be_u32},
    combinator::terminated,
    error::{ContextError, ErrMode, FromExternalError},
    prelude::ModalResult,
    token::{rest, take, take_until},
};

use crate::{
    TinygitError,
    ende::{Decode, Encode},
    object::hash::{ObjectHash, parse_object_hash},
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

    pub hash: ObjectHash,

    pub flags: Flags,

    pub path_name: BString,
}

impl Decode for IndexEntry {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let ctime_seconds = be_u32.parse_next(input)?;
        let ctime_ns_fractions = be_u32.parse_next(input)?;
        let mtime_seconds = be_u32.parse_next(input)?;
        let mtime_ns_fractions = be_u32.parse_next(input)?;

        let dev = be_u32.parse_next(input)?;
        let ino = be_u32.parse_next(input)?;

        let mode = Mode::parse.parse_next(input)?;

        let uid = be_u32.parse_next(input)?;
        let gid = be_u32.parse_next(input)?;
        let file_size = be_u32.parse_next(input)?;

        let hash = parse_object_hash.parse_next(input)?;

        let flags = Flags::parse.parse_next(input)?;

        // NOTE: version 3 / later has a 16 bit field here, which we are skipping

        let path_name: BString = if flags.name_length() == 0xFFF {
            terminated(take_until(1.., 0x00), 0x00).parse_next(input)?
        } else {
            terminated(take(flags.name_length() as usize), 0x00).parse_next(input)?
        }
        .into();

        // this is padding to ensure the total size is divisible by 8
        //
        // it should be 0x00, if it isn't, it probably means
        // we're parsing something wrong (or wrote it wrong!)
        rest.verify(|x: &[u8]| x.iter().all(|x| *x == 0x00))
            .parse_next(input)?;

        Ok(Self {
            ctime_seconds,
            ctime_ns_fractions,
            mtime_seconds,
            mtime_ns_fractions,
            dev,
            ino,
            mode,
            uid,
            gid,
            file_size,
            hash,
            flags,
            path_name,
        })
    }
}

/// +------------+------------+------------+------------+
/// |   unused   |  obj type  |   unused   | permission |
/// | 31 .... 16 | 15  ... 12 | 11  ...  9 | 8  ...   0 |
/// +------------+------------+------------+------------+
#[derive(Debug, PartialEq, Eq)]
pub struct Mode {
    object_type: ObjectType,

    /// 0o0755 or 0o0644 for regular files, 0 for symbolic links and git links.
    permission: u16,
}

impl Mode {
    /// Creates a mode, after checking the constraints that the index format puts on it.
    pub fn try_new(object_type: ObjectType, permission: u16) -> Result<Self, TinygitError> {
        let permission_is_valid = match object_type {
            ObjectType::Regular => permission == 0o0755 || permission == 0o0644,
            ObjectType::SymbolicLink | ObjectType::GitLink => permission == 0,
        };

        if !permission_is_valid {
            return Err(TinygitError::InvalidModePermission(permission));
        }

        Ok(Self {
            object_type,
            permission,
        })
    }

    pub fn object_type(&self) -> ObjectType {
        self.object_type
    }

    pub fn permission(&self) -> u16 {
        self.permission
    }
}

impl Decode for Mode {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let value = be_u32.parse_next(input)?;

        // ensure the first unused part is 0
        if value >> 16 != 0 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        // ensure the second unused part is 0
        if (value >> 9) & 0b0111 != 0 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        let object_type = ObjectType::try_from(((value >> 12) & 0xF) as u8)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        let permission: u16 = (value & 0x1FF)
            .try_into()
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        Self::try_new(object_type, permission)
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))
    }
}

impl Encode for Mode {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let obj_type: u8 = self.object_type.into();

        let value: u32 = ((obj_type as u32) << 12) | self.permission as u32;

        writer.write_all(&value.to_be_bytes())?;

        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
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

/// +----+----+--------+--------------+
/// | x  | x  |   xx   | xxxxxxxxxxxx |
/// | 15 | 14 | 13..12 |     11..0    |
/// +----+----+--------+--------------+
#[derive(Debug)]
pub struct Flags {
    assume_valid: bool,
    extended: bool,
    stage: u8,
    name_length: u16,
}

impl Flags {
    pub fn try_new(
        assume_valid: bool,
        extended: bool,
        stage: u8,
        name_length: u16,
    ) -> Result<Self, TinygitError> {
        if VERSION == 2 && extended {
            return Err(TinygitError::InvalidFlag("extended"));
        }

        Ok(Self {
            assume_valid,
            extended,
            stage,
            name_length,
        })
    }

    pub fn assume_valid(&self) -> bool {
        self.assume_valid
    }

    pub fn extended(&self) -> bool {
        self.extended
    }

    pub fn stage(&self) -> u8 {
        self.stage
    }

    pub fn name_length(&self) -> u16 {
        self.name_length
    }
}

impl Decode for Flags {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let value = be_u16.parse_next(input)?;

        let assume_valid: bool = value >> 15 == 1;
        let extended: bool = (value >> 14) & 0b0001 == 1;
        let stage: u8 = ((value >> 12) & 0b0011) as u8;
        let name_length: u16 = value & 0x0FFF;

        Ok(Self {
            assume_valid,
            extended,
            stage,
            name_length,
        })
    }
}

impl Encode for Flags {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let value: u16 = (u16::from(self.assume_valid) << 15)
            | (u16::from(self.extended) << 14)
            | ((self.stage as u16) << 12)
            | self.name_length;

        writer.write_all(&value.to_be_bytes())?;

        Ok(())
    }
}
