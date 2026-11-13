use std::io::Write;

use nutype::nutype;
use winnow::{
    ModalResult, Parser,
    combinator::{opt, seq},
    error::StrContext,
    token::{rest, take_until},
};

use crate::{
    ende::Decode,
    ende::Encode,
    identity::Identity,
    object::{
        ObjectType,
        hash::{ObjectHash, parse_object_hash_str},
        parse_object_type,
    },
    parsing::{Stream, property},
    time::Timestamp,
};

/// A tag: an object that points to an [`crate::object::Object`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Tag {
    /// The object it's pointing to, along with its type.
    pub target: TagTarget,

    /// The name of the tag.
    pub name: TagName,

    /// The tagger and time of creation of the tag.
    pub tagger: Option<(Identity, Timestamp)>,

    /// The description of the tag.
    pub description: String,
}

impl Tag {
    #[must_use]
    pub fn new(
        target: TagTarget,
        name: TagName,
        tagger: Option<(Identity, Timestamp)>,
        description: String,
    ) -> Self {
        Self {
            target,
            name,
            tagger,
            description,
        }
    }
}

/// The hash and type of an object a tag is pointing to.
pub type TagTarget = (ObjectHash, ObjectType);

impl Encode for Tag {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), crate::TinygitError> {
        writeln!(writer, "object {}", self.target.0)?;
        writeln!(writer, "type {}", Into::<&'static str>::into(self.target.1))?;
        writeln!(writer, "tag {}", self.name)?;

        if let Some(tagger) = &self.tagger {
            write!(writer, "tagger ")?;
            tagger.encode(writer)?;
            writeln!(writer)?;
        }

        writeln!(writer)?;
        write!(writer, "{}", self.description)?;

        Ok(())
    }
}

impl Decode for Tag {
    fn parse(input: &mut Stream<'_>) -> ModalResult<Self> {
        seq! {Tag{
        target: seq!(
            property("object", parse_object_hash_str),

            property("type", parse_object_type),
        ),
        name: property("tag", take_until(1.., "\n")
            .map(str::from_utf8)
            .verify_map(Result::ok)
            .map(str::to_owned)
            .map(TagName::try_new)
            .verify_map(Result::ok)),
        tagger: opt(property("tagger", <(Identity, Timestamp)>::parse)),
        _: "\n",
        description: rest.map(str::from_utf8).verify_map(Result::ok).map(str::to_owned),
        }}
        .context(StrContext::Label("tag object"))
        .parse_next(input)
    }
}

#[nutype(
    sanitize(with = |x| x.trim_ascii().into()),
    validate(predicate = |x| !x.is_empty()),
    derive(Debug, PartialEq, Eq, Clone, Display, AsRef, Deref)
)]
pub struct TagName(String);
