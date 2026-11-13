use std::{
    fs::{self, File},
    io::{ErrorKind, prelude::Write},
    path::PathBuf,
};

use bitfield::bitfield;
use bstr::BString;
use sha1::{
    Digest, Sha1,
    digest::{array::Array, consts::U20},
};
use winnow::{
    Parser,
    binary::{be_u16, be_u32},
    combinator::{repeat, terminated},
    error::{ContextError, ErrMode, FromExternalError},
    prelude::ModalResult,
    stream::Location,
    token::{take, take_until},
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
    pub header: IndexHeader,
    pub hash: ObjectHash,
    entries: Vec<IndexEntry>,
}

impl Index {
    /// Creates a new empty [Index] at `path`. Equivalent to Git reading a missing index file.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            header: IndexHeader {
                version: VERSION,
                entries: 0,
            },
            hash: ObjectHash::default(),
            entries: Vec::default(),
        }
    }

    pub fn open(path: PathBuf) -> Result<Self, TinygitError> {
        let (header, entries, hash) =
            <(IndexHeader, Vec<IndexEntry>, ObjectHash)>::decode_file(&path)?;

        Ok(Self {
            path,
            header,
            hash,
            entries,
        })
    }

    /// Tries to write to the index by acquiring the `index.lock`, fails if it's already being held.
    pub fn try_write(&self) -> Result<(), TinygitError> {
        // acquire the lock file
        let lock_file = File::create_new(self.lock_path()).map_err(|err| {
            if err.kind() == ErrorKind::AlreadyExists {
                TinygitError::LockHeld
            } else {
                err.into()
            }
        })?;

        let mut writer = HashingWriter::new(lock_file);

        self.header.encode(&mut writer)?;

        for entry in &self.entries {
            entry.encode(&mut writer)?;
        }

        let (hash, mut writer) = writer.finalize();

        writer.write_all(&hash)?;

        // written to the lock, we can now move that to the actual index file
        fs::rename(self.lock_path(), &self.path)?;

        Ok(())
    }

    pub fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    /// Iterates over the entries in this index, sorted by path name.
    pub fn entries(&self) -> impl Iterator<Item = &IndexEntry> {
        self.entries.iter()
    }

    /// Adds `entry` to the index, keeping the entries sorted by path name.
    ///
    /// If an entry with the same path name already exists, it is removed first.
    /// The header's entry count is kept in sync.
    pub fn add_entry(&mut self, entry: IndexEntry) {
        // remove any entry that already exists with the same path name
        self.entries
            .retain(|existing| existing.path_name != entry.path_name);

        // entries must remain sorted by path name for the format to be valid
        let insert_at = self
            .entries
            .partition_point(|existing| existing.path_name < entry.path_name);

        self.entries.insert(insert_at, entry);

        self.header.entries = u32::try_from(self.entries.len())
            .expect("the index cannot hold more entries than u32 allows");
    }
}

impl Decode for (IndexHeader, Vec<IndexEntry>, ObjectHash) {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let header = IndexHeader::parse.parse_next(input)?;

        let entries = repeat(header.entries as usize, IndexEntry::parse).parse_next(input)?;

        // TODO: support extensions
        // read (and completel ignore) extensions
        while input.len() > 20 {
            take(4usize).parse_next(input)?;

            let size = be_u32.parse_next(input)?;

            take(size as usize).parse_next(input)?;
        }

        let hash = parse_object_hash.parse_next(input)?;

        Ok((header, entries, hash))
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

#[derive(Debug, PartialEq, Eq)]
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
        let start_offset = input.current_token_start();

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
        let consumed_so_far = input.current_token_start() - start_offset;
        let remainder = consumed_so_far % 8;
        let extra_padding = if remainder == 0 { 0 } else { 8 - remainder };

        take(extra_padding)
            .verify(|x: &[u8]| x.iter().all(|b| *b == 0x00))
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

impl Encode for IndexEntry {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        let writer = &mut CountingWriter::new(writer);

        writer.write_all(&self.ctime_seconds.to_be_bytes())?;
        writer.write_all(&self.ctime_ns_fractions.to_be_bytes())?;

        writer.write_all(&self.mtime_seconds.to_be_bytes())?;
        writer.write_all(&self.mtime_ns_fractions.to_be_bytes())?;

        writer.write_all(&self.dev.to_be_bytes())?;
        writer.write_all(&self.ino.to_be_bytes())?;

        self.mode.encode(writer)?;

        writer.write_all(&self.uid.to_be_bytes())?;
        writer.write_all(&self.gid.to_be_bytes())?;
        writer.write_all(&self.file_size.to_be_bytes())?;

        writer.write_all(Into::<&[u8]>::into(&self.hash))?;

        self.flags.encode(writer)?;

        writer.write_all(&self.path_name)?;
        // write the null terminator that separates the path from the padding
        writer.write_all(&[0x00])?;

        // compute how much padding we need (if any) to make it so the entry is divisible by 8
        if !writer.written.is_multiple_of(8) {
            let padding = 8 - writer.written % 8;

            writer.write_all(&[0x00].repeat(padding))?;
        }

        Ok(())
    }
}

bitfield! {
    /// +------------+------------+------------+------------+
    /// |   unused   |  obj type  |   unused   | permission |
    /// | 31 .... 16 | 15  ... 12 | 11  ...  9 | 8  ...   0 |
    /// +------------+------------+------------+------------+
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct Mode(u32);
    impl Debug;

    /// 0o0755 or 0o0644 for regular files, 0 for symbolic links and git links.
    pub u16, permission, set_permission: 8, 0;
    pub u8, unused_2, set_unused_2: 11, 9;
    pub u8, from try_into ObjectType, try_object_type, set_object_type: 15, 12;
    pub u16, unused_1, set_unused_1: 31, 16;
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

        let mut mode = Self(0);
        mode.set_object_type(object_type);
        mode.set_permission(permission);
        Ok(mode)
    }

    pub fn object_type(&self) -> ObjectType {
        self.try_object_type()
            .expect("Mode contains a valid ObjectType")
    }

    /// Returns the raw, bitpacked mode.
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl Decode for Mode {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        let value = be_u32.parse_next(input)?;
        let mode = Self(value);

        // ensure the first unused part is 0
        if mode.unused_1() != 0 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        // ensure the second unused part is 0
        if mode.unused_2() != 0 {
            return Err(ErrMode::Cut(ContextError::new()));
        }

        let object_type = mode
            .try_object_type()
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))?;

        Self::try_new(object_type, mode.permission())
            .map_err(|err| ErrMode::Cut(ContextError::from_external_error(input, err)))
    }
}

impl Encode for Mode {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        writer.write_all(&self.0.to_be_bytes())?;

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
#[derive(Debug, PartialEq, Eq)]
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

/// A wrapper of a [Write] that tracks how many bytes it has written.
struct CountingWriter<W: Write> {
    writer: W,
    written: usize,
}

impl<W: Write> CountingWriter<W> {
    fn new(writer: W) -> Self {
        Self { writer, written: 0 }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let size = buf.len();

        self.writer.write_all(buf)?;
        self.written += size;

        Ok(size)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

/// A wrapper of a [Write] that computes a hash of the contents written to it.
struct HashingWriter<W: Write> {
    writer: W,
    hasher: Sha1,
}

impl<W: Write> HashingWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            hasher: Sha1::default(),
        }
    }

    fn finalize(self) -> (Array<u8, U20>, W) {
        (self.hasher.finalize(), self.writer)
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.writer.write(buf)?;

        self.hasher.update(buf);

        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
