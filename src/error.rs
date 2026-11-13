use std::{
    error::Error,
    fmt::{self, Display},
    io,
    num::TryFromIntError,
    path::PathBuf,
    time::SystemTimeError,
};

use hex::FromHexError;
use thiserror::Error;

use crate::{
    fs::FileNameError,
    identity::{EmailError, NameError},
    time::TimestampError,
};

/// Context for parsing used in the error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserContext {
    /// We are parsing something from a file.
    File(PathBuf),

    /// There is no useful context (e.g parsing from memory)
    None,
}

impl Display for ParserContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(f, "file '{}'", path.display()),
            Self::None => write!(f, "<input>"),
        }
    }
}

#[derive(Debug, Error)]
pub struct ParseError {
    pub context: ParserContext,
    pub offset: Option<usize>,
    pub message: String,

    #[source]
    pub cause: Option<Box<dyn Error + Send + Sync>>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse {}", self.context)?;

        if let Some(offset) = self.offset {
            write!(f, " at offset {offset:#x}")?;
        }

        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        } else if let Some(ref cause) = self.cause {
            write!(f, ": {cause}")?;
        }

        Ok(())
    }
}

impl ParseError {
    pub fn new(context: ParserContext, message: impl Into<String>) -> Self {
        Self {
            context,
            offset: None,
            message: message.into(),
            cause: None,
        }
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_cause(mut self, cause: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

#[derive(Error, Debug)]
pub enum TinygitError {
    #[error("IO Error: {0}")]
    IO(#[from] io::Error),

    #[error("Couldn't convert int: {0}")]
    TryFromInt(#[from] TryFromIntError),

    #[error("System time error: {0}")]
    SystemTime(#[from] SystemTimeError),

    #[error("Invalid file name")]
    InvalidFileName,

    #[error("Invalid file name: {0}")]
    FileNameError(#[from] FileNameError),

    #[error("No git directory")]
    NoGitDirectory,

    #[error("Not in repo")]
    NotInRepo,

    #[error("Directory not empty")]
    DirNotEmpty,

    #[error("Bucket not found")]
    BucketNotFound,

    #[error("Object not found")]
    ObjectNotFound,

    #[error("Object already exists")]
    ObjectExists,

    #[error("Ref not found")]
    RefNotFound,

    #[error("Empty tree")]
    EmptyTree,

    #[error("Unexpected object type")]
    UnexpectedObjectType,

    #[error("Unexpected object type")]
    UnexpectedType,

    #[error("Too much data")]
    TooMuchData,

    #[error(transparent)]
    ParserError(#[from] ParseError),

    #[error("Invalid pack index")]
    InvalidPackIndex,

    /// Not to be confused with InvalidPackIndex
    #[error("Pack not indexed")]
    PackNotIndexed,

    #[error("Hex decoding error: {0}")]
    HexDecodingError(#[from] FromHexError),

    #[error("Invalid object type: {0}")]
    InvalidObjectType(u8),

    #[error("Invalid mode permission: {0:o}")]
    InvalidModePermission(u16),

    #[error("Invalid instruction")]
    InvalidInstruction,

    #[error("Invalid flag : {0}")]
    InvalidFlag(&'static str),

    #[error("Lock is already beind held")]
    LockHeld,

    #[error("Invalid identity: {0}")]
    IdentityNameError(#[from] NameError),

    #[error("Invalid identity email: {0}")]
    IdentityEmailError(#[from] EmailError),

    #[error("Invalid timestamp: {0}")]
    TimestampError(#[from] TimestampError),
}
