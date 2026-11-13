//! This module contains items that deal with file names.

use crate::{TinygitError, ende::Decode, ende::Encode, parsing::Stream};
use std::{fmt::Display, path::PathBuf};
use winnow::{ModalResult, Parser, combinator::terminated, token::take_until};

/// A file name.
///
/// This is a newtype which is validated upon creation, thus a [`FileName`] is assumed to be a valid
/// file name that Git supports.
///
/// A [`FileName`] can be created using [`FromStr`]. For example:
/// ```
/// use tinygit::fs::FileName;
/// use tinygit::ende::Decode;
///
/// let name: FileName = FileName::decode_str("foo.rs\0").unwrap();
///
/// assert_eq!(name.as_str(), "foo.rs");
/// ```
#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone)]
pub struct FileName(String);

impl FileName {
    /// Creates a new [`FileName`].
    ///
    /// # Errors
    ///
    /// Returns [`TinygitError::InvalidFileName`] if `name` is empty.
    pub fn try_new(name: String) -> Result<Self, TinygitError> {
        if name.is_empty() {
            return Err(TinygitError::InvalidFileName);
        }

        Ok(Self(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Decode for FileName {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        terminated(take_until(1.., "\x00"), "\x00")
            .map(str::from_utf8)
            .verify_map(Result::ok)
            .map(str::to_owned)
            .map(FileName::try_new)
            .verify_map(Result::ok)
            .parse_next(input)
    }
}

impl Display for FileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<FileName> for PathBuf {
    fn from(val: FileName) -> Self {
        Self::from(val.0)
    }
}

impl Encode for FileName {
    fn encode<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), TinygitError> {
        write!(writer, "{}", self.0)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{TinygitError, ende::Decode, fs::FileName};

    #[test]
    fn file_name_parses_valid_inputs() {
        assert_eq!(FileName::decode_str("foo.bar\0").unwrap().0, "foo.bar");
        assert_eq!(
            FileName::decode_str("even this!!!\0").unwrap().0,
            "even this!!!"
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
            FileName::try_new(String::new()),
            Err(TinygitError::InvalidFileName)
        ));
    }

    #[test]
    fn file_name_try_new_accepts_non_empty_names() {
        assert_eq!(
            FileName::try_new("foo.bar".to_string()).unwrap().as_str(),
            "foo.bar"
        );
    }
}
