//! This module contains items that deal with file names.

use crate::{TinygitError, ende::Decode, ende::Encode, parsing::Stream};
use bstr::BString;
use nutype::nutype;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
use winnow::{ModalResult, Parser, combinator::terminated, token::take_until};

/// A file name.
///
/// This is a newtype which is validated upon creation, thus a [`FileName`] is assumed to be a valid
/// file name that Git supports.
///
/// Git stores a file name as the bytes the filesystem gave it, so a [`FileName`] holds bytes
/// and is not required to be valid UTF-8.
///
/// A [`FileName`] can be created using [`FileName::try_new`]. For example:
/// ```
/// use tinygit::fs::FileName;
///
/// let name: FileName = FileName::try_new("foo.rs".into()).unwrap();
///
/// assert_eq!(*name, "foo.rs");
/// ```
#[nutype(
    validate(predicate = |x| !x.is_empty() && !x.contains(&0x00)),
    derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone, Display, AsRef, Deref)
)]
pub struct FileName(BString);

impl Decode for FileName {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        terminated(take_until(1.., "\x00"), "\x00")
            .map(|x: &[u8]| x.into())
            .map(FileName::try_new)
            .verify_map(Result::ok)
            .parse_next(input)
    }
}

impl From<FileName> for PathBuf {
    fn from(val: FileName) -> Self {
        Self::from(OsString::from_vec(val.into_inner().into()))
    }
}

impl Encode for FileName {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        writer.write_all(self.as_ref())?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        ende::Decode,
        fs::{FileName, FileNameError},
    };

    #[test]
    fn file_name_parses_valid_inputs() {
        assert_eq!(*FileName::decode_str("foo.bar\0").unwrap(), "foo.bar");
        assert_eq!(
            *FileName::decode_str("even this!!!\0").unwrap(),
            "even this!!!"
        );
    }

    #[test]
    fn file_name_parses_names_that_are_not_utf8() {
        assert_eq!(
            *FileName::decode_bytes(b"caf\xe9\0").unwrap(),
            b"caf\xe9".as_slice()
        );
    }

    #[test]
    fn file_name_rejects_invalid_inputs() {
        assert!(FileName::decode_str("foo.bar").is_err());
        assert!(FileName::decode_str("foo.bar\n").is_err());
    }

    #[test]
    fn file_name_try_new_rejects_empty_names() {
        assert!(matches!(
            FileName::try_new("".into()),
            Err(FileNameError::PredicateViolated)
        ));
    }

    #[test]
    fn file_name_try_new_rejects_names_with_nul_bytes() {
        assert!(matches!(
            FileName::try_new("foo\0bar".into()),
            Err(FileNameError::PredicateViolated)
        ));
    }

    #[test]
    fn file_name_try_new_accepts_non_empty_names() {
        assert_eq!(*FileName::try_new("foo.bar".into()).unwrap(), "foo.bar");
    }
}
