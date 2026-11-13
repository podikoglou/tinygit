//! This module deals with the modeling and valiation of user identities.
//!
//! It offers newtypes such as [`Identity`], which is consisted of a [`Name`] and [`Email`].

use std::{fmt::Display, io::Write};

use bstr::BString;
use nutype::nutype;
use winnow::{
    ModalResult, Parser,
    combinator::seq,
    error::{StrContext, StrContextValue},
    token::take_until,
};

use crate::{ende::Decode, ende::Encode, parsing::Stream};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Identity {
    pub name: Name,
    pub email: Email,
}

impl Identity {
    #[must_use]
    pub fn new(name: Name, email: Email) -> Self {
        Self { name, email }
    }
}

impl Decode for Identity {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq!(take_until(0.., " <").map(|x: &[u8]| x.into()).map(Name::try_new).verify_map(Result::ok).context(StrContext::Label("name")),
            _: " <",
            take_until(0.., ">").map(|x: &[u8]| x.into()).map(Email::try_new).verify_map(Result::ok).context(StrContext::Label("email")),
            _: ">"
        )
        .map(|(name, email)| Identity::new(name, email))
        .context(StrContext::Label("identity"))
        .context(StrContext::Expected(StrContextValue::Description(
            "<name> <<email>>",
        )))
        .parse_next(input)
    }
}

impl Encode for Identity {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        writer.write_all(self.name.as_ref())?;
        writer.write_all(b" <")?;
        writer.write_all(self.email.as_ref())?;
        writer.write_all(b">")?;

        Ok(())
    }
}

impl Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Identity { name, email } = self;

        write!(f, "{name} <{email}>")
    }
}

#[nutype(
    sanitize(with = |x| x.trim_ascii().into()),
    validate(predicate = |x| !x.is_empty() && !x.contains(&0x00)),
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref)
)]
pub struct Name(BString);

#[nutype(
    sanitize(with = |x| x.trim_ascii().into()),
    validate(predicate = |x| !x.is_empty() && !x.contains(&0x00)),
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref)
)]
pub struct Email(BString);

impl PartialEq<&str> for Email {
    fn eq(&self, other: &&str) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<str> for Email {
    fn eq(&self, other: &str) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<Email> for &str {
    fn eq(&self, other: &Email) -> bool {
        *self == other.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ende::Decode,
        identity::{Email, Identity, Name},
    };

    #[test]
    fn identity_parses_valid_identities() {
        assert_eq!(
            Identity::decode_bytes(b"John Doe <john@doe.com>").unwrap(),
            Identity::new(
                Name::try_new("John Doe".into()).unwrap(),
                Email::try_new("john@doe.com".into()).unwrap()
            )
        );
    }

    #[test]
    fn identity_rejects_invalid_input() {
        assert!(Identity::decode_bytes(b"<john@doe.com>").is_err());
        assert!(Identity::decode_bytes(b"j<john@doe.com>").is_err());
        assert!(Identity::decode_bytes(b"<john@doe.com").is_err());
        assert!(Identity::decode_bytes(b"john@doe.com>").is_err());
        assert!(Identity::decode_bytes(b"john@doe.com").is_err());
        // TODO: should this validate emails?
        assert!(Identity::decode_bytes(b"johndoe.com").is_err());
    }
}
